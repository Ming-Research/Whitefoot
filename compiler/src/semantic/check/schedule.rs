//! The schedule of one inventory's entailment analyses: receipts, renamed
//! symbolic summaries, and concurrent analysis of independent postcondition
//! components, level by level [FN-9, MOD-8].

use std::collections::HashMap;

use super::generics::{RenamingClass, summary_entailment, symbolic_renaming_class};
use super::{CheckStop, CheckedFunctionInventory, Checker};
use crate::semantic::SemanticCompilerFailure;
use crate::semantic::entailment::{
    EntailmentCallee, EntailmentContext, PostconditionSchedule, VerifiedPostconditionSummary,
    analyze_function, analyze_function_candidate, postcondition_schedule,
};

impl Checker<'_, '_> {
    /// Analyzes every function of the inventory, or only those `analyzed`
    /// marks. A caller that restricts the set must close it under callees.
    ///
    /// Symbolic validation passes `judged`, the canonical instances whose own
    /// analysis is judged. Every other symbolic instance is analyzed only for
    /// the summaries its callers read, so one that renames an instance of the
    /// same declaration already analyzed in an earlier component takes that
    /// analysis's summary instead of analyzing the same body again [FN-2].
    pub(super) fn analyze_function_inventory(
        &mut self,
        functions: &mut [CheckedFunctionInventory],
        callees: &[EntailmentCallee],
        optimistic_batch: bool,
        analyzed: Option<&[bool]>,
        allow_receipts: bool,
        judged: Option<&[bool]>,
    ) -> Result<PostconditionSchedule, CheckStop> {
        let condition_scope = functions.iter().enumerate().map(|(index, checked)| {
            judged.map_or_else(|| {
                !self.types.function_templates.iter().any(|template| {
                    template.declaration == checked.function.declaration && !template.generic_parameters.is_empty()
                })
            }, |judged| judged[index])
        }).collect::<Vec<_>>();
        let selected = |index: usize| analyzed.is_none_or(|analyzed| analyzed[index]);
        let const_parameter_types: HashMap<_, _> = self.types.const_generic_types().collect();
        let renaming_classes = (0..functions.len())
            .map(|index| {
                judged
                    .filter(|judged| !judged[index])
                    .and_then(|_| self.types.signatures.get(index))
                    .and_then(|signature| {
                        symbolic_renaming_class(signature, &const_parameter_types)
                    })
            })
            .collect::<Vec<_>>();
        let mut renamed_analyses: HashMap<RenamingClass, (usize, usize)> = HashMap::new();
        self.analysis.renamed_summaries = vec![false; functions.len()];
        let contract_queries = self.analysis.contract_queries.clone();
        // [MOD-8] the concrete inventory's analyses may stand on receipts;
        // the symbolic validation of generic templates always runs afresh.
        let receipts = self
            .receipts
            .filter(|_| allow_receipts && self.reject_entailment);
        let items = receipts
            .map(|_| self.types.declarations.receipt_items())
            .transpose()?;
        if receipts.is_some() {
            self.analysis.reused_analyses = vec![false; functions.len()];
        }
        // ENT is the single acceptance-bearing proof path for ordinary
        // obligations, call requirements, invariants and postconditions.
        let mut schedule =
            postcondition_schedule(functions.iter().map(|checked| &checked.function))
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if schedule.components.is_empty() {
            // Without postconditions no analysis reads another's, so every
            // function is analyzed concurrently.
            let mut fresh = Vec::new();
            for index in 0..functions.len() {
                if !selected(index) {
                    continue;
                }
                if let (Some(store), Some(items)) = (receipts, &items)
                    && let Some(entailment) = self.recorded_analysis(
                        store,
                        items,
                        functions,
                        index,
                        callees,
                        &[],
                        &const_parameter_types,
                    )
                {
                    functions[index].function.entailment = entailment;
                    continue;
                }
                fresh.push(index);
            }
            let analyses = {
                let types = &self.types;
                let functions = &*functions;
                crate::in_parallel(&fresh, |index| {
                    let checked = &functions[*index];
                    let context = EntailmentContext {
                        judge_conditions: condition_scope[*index],
                        declarations: types.declarations.resolved.declarations(),
                        callees,
                        constants: &types.checked_constants,
                        constant_ids: &types.constants,
                        const_parameter_types: &const_parameter_types,
                        nominals: &types.nominals,
                        elements: &types.elements,
                        contract_queries: &contract_queries,
                        verified_postconditions: &[],
                        verified_postcondition_proofs: &[],
                        binding_names: &checked.binding_names,
                    };
                    if optimistic_batch {
                        analyze_function_candidate(&checked.function, &context)
                    } else {
                        analyze_function(&checked.function, &context)
                    }
                })
            };
            for (index, entailment) in fresh.into_iter().zip(analyses) {
                functions[index].function.entailment = entailment;
            }
        } else {
            // A component reads only its callees' summaries, and its callees
            // are in earlier components, so the components of one level, all
            // of whose callees are in lower levels, are analyzed concurrently;
            // each level then publishes in component order. Functions of one
            // component read none of each other's summaries either.
            let mut levels = vec![0_usize; schedule.components.len()];
            for position in 0..schedule.components.len() {
                levels[position] = schedule.components[position]
                    .outgoing
                    .iter()
                    .map(|callee| levels[*callee as usize] + 1)
                    .max()
                    .unwrap_or(0);
            }
            let level_count = levels.iter().max().map_or(0, |deepest| deepest + 1);
            let mut by_level = vec![Vec::new(); level_count];
            for (position, level) in levels.iter().enumerate() {
                // Callee closure keeps components whole: skipping one skips
                // both its analysis and the summaries no analyzed body reads.
                if schedule.components[position]
                    .functions
                    .iter()
                    .any(|function| selected(function.0 as usize))
                {
                    by_level[*level].push(position);
                }
            }
            for (level, positions) in by_level.iter().enumerate() {
                let verified_postconditions = functions
                    .iter()
                    .map(|checked| {
                        checked
                            .function
                            .entailment
                            .postconditions
                            .iter()
                            .filter(|proof| {
                                selected(checked.function.id.0 as usize) && proof.summary.is_some()
                            })
                            .filter_map(|proof| {
                                checked
                                    .function
                                    .postconditions
                                    .get(proof.relation_ordinal as usize)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let verified_postcondition_proofs = functions
                    .iter()
                    .map(|checked| {
                        checked
                            .function
                            .entailment
                            .postconditions
                            .iter()
                            .filter(|proof| {
                                selected(checked.function.id.0 as usize) && proof.summary.is_some()
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let mut settled = Vec::new();
                let mut fresh = Vec::new();
                // Instances of a renaming class first met at this level: the
                // first is analyzed, and the others take its summary after it.
                let mut copies = Vec::new();
                let mut level_representatives = HashMap::new();
                for position in positions {
                    for function in &schedule.components[*position].functions {
                        let function_index = function.0 as usize;
                        functions
                            .get(function_index)
                            .filter(|checked| checked.function.id == *function)
                            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                        let recorded = match (receipts, &items) {
                            (Some(store), Some(items)) => self.recorded_analysis(
                                store,
                                items,
                                functions,
                                function_index,
                                callees,
                                &verified_postconditions,
                                &const_parameter_types,
                            ),
                            _ => None,
                        };
                        if let Some(entailment) = recorded {
                            settled.push((function_index, entailment, false));
                            continue;
                        }
                        let renamed = renaming_classes[function_index]
                            .as_ref()
                            .and_then(|class| renamed_analyses.get(class))
                            .filter(|(represented, _)| *represented < level)
                            .map(|(_, representative)| {
                                summary_entailment(&functions[*representative].function.entailment)
                            });
                        match (renamed, renaming_classes[function_index].as_ref()) {
                            (Some(entailment), _) => {
                                settled.push((function_index, entailment, true));
                            }
                            (None, Some(class)) => match level_representatives.get(class) {
                                Some(representative) => {
                                    copies.push((function_index, *representative));
                                }
                                None => {
                                    level_representatives.insert(class.clone(), function_index);
                                    fresh.push(function_index);
                                }
                            },
                            (None, None) => fresh.push(function_index),
                        }
                    }
                }
                let analyses = {
                    let types = &self.types;
                    let functions = &*functions;
                    crate::in_parallel(&fresh, |index| {
                        let checked = &functions[*index];
                        let context = EntailmentContext {
                            judge_conditions: condition_scope[*index],
                            declarations: types.declarations.resolved.declarations(),
                            callees,
                            constants: &types.checked_constants,
                            constant_ids: &types.constants,
                            const_parameter_types: &const_parameter_types,
                            nominals: &types.nominals,
                            elements: &types.elements,
                            contract_queries: &contract_queries,
                            verified_postconditions: &verified_postconditions,
                            verified_postcondition_proofs: &verified_postcondition_proofs,
                            binding_names: &checked.binding_names,
                        };
                        analyze_function_candidate(&checked.function, &context)
                    })
                };
                drop(verified_postconditions);
                drop(verified_postcondition_proofs);
                for (function_index, entailment, renamed) in settled {
                    functions[function_index].function.entailment = entailment;
                    self.analysis.renamed_summaries[function_index] = renamed;
                }
                for (function_index, entailment) in fresh.into_iter().zip(analyses) {
                    functions[function_index].function.entailment = entailment;
                    if let Some(class) = renaming_classes[function_index].clone() {
                        renamed_analyses
                            .entry(class)
                            .or_insert((level, function_index));
                    }
                }
                for (function_index, representative) in copies {
                    functions[function_index].function.entailment =
                        summary_entailment(&functions[representative].function.entailment);
                    self.analysis.renamed_summaries[function_index] = true;
                }
                for position in positions {
                    let component = &mut schedule.components[*position];
                    let publish = component.functions.iter().all(|function| {
                        let checked = &functions[function.0 as usize].function;
                        checked
                            .entailment
                            .loop_invariants
                            .iter()
                            .all(|invariant| invariant.proof.discharged())
                            && (matches!(
                                checked.entailment.body_disposition,
                                crate::semantic::model::CheckedBodyDisposition::Uninhabited { .. }
                            ) || checked.postconditions.is_empty()
                                || (checked.entailment.postconditions.len()
                                    == checked.postconditions.len()
                                    && checked
                                        .entailment
                                        .postconditions
                                        .iter()
                                        .all(|proof| proof.aggregate.discharged)))
                    });
                    if publish {
                        for function in &component.functions {
                            let checked = &mut functions[function.0 as usize].function;
                            if matches!(
                                checked.entailment.body_disposition,
                                crate::semantic::model::CheckedBodyDisposition::Uninhabited { .. }
                            ) {
                                continue;
                            }
                            for proof in &mut checked.entailment.postconditions {
                                let summary = VerifiedPostconditionSummary {
                                    function: *function,
                                    block: proof.block.clone(),
                                    relation_ordinal: proof.relation_ordinal,
                                    component: component.ordinal,
                                };
                                proof.summary = Some(summary.clone());
                                component.summaries.push(summary);
                            }
                        }
                    }
                }
            }
        }
        Ok(schedule)
    }
}
