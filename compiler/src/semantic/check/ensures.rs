use std::collections::HashMap;

use crate::FixedTerminal;
use crate::syntax::NodeId;
use crate::{
    BuiltinPreludeId, DeclarationClass, DeclarationId, LexicalUseRole,
    PostconditionCandidateRecord, PostconditionResolutionRecord, PostconditionSelectorClass,
    Production, ResolvedTarget, SemanticCompilerFailure, SemanticIssue, SemanticIssueKind,
    SemanticLocation, SemanticRule, SourceOrigin,
};

use super::super::goal::{GoalOperation, GoalProjection};
use super::super::model::{
    BindingId, CheckedArrayRoot, CheckedExpression, CheckedIntegerOperation, CheckedMode,
    CheckedNominalKind, CheckedParameter, CheckedPlaceStep, CheckedStatement, CheckedType,
    CheckedValue, FunctionId, IntegerType,
};
use super::super::postcondition::{
    CheckedPostcondition, CheckedPostconditionSelector, NormalizedRelation,
    PostconditionConstantOrigin, PostconditionFieldIdentity, PostconditionPlace,
    PostconditionPlaceRoot, PostconditionReturnDatum, PostconditionReturnPlace,
    PostconditionReturnPlaceRoot, RelationDatum, RelationTemplate, RelationTerm,
    SelectedPostconditionReturn,
};
use super::generics::GenericArgument;
use super::publication;
use super::requires::{ClauseKind, ExpandedClauseDatum, ExpandedClauseExpression};
use super::{
    CheckStop, Checker, ControlCounters, ControlScope, FunctionSignature, LocalBinding,
    ParameterSignature,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectorAdmissionType {
    Fragment,
    ResultFragment,
    Symbolic,
    /// [CALL-4] one declared result of measured type. Its value is no [ENT-2]
    /// term, so only a measure over it is an admitted clause operand.
    Measured,
    Invalid,
}

#[derive(Clone, Copy)]
struct PostconditionBindingInfo {
    ty: CheckedType,
    implicit_deref: bool,
}

/// [MSR-3] whether one declared parameter has an exit state a clause may name.
///
/// v0.60 has one reference kind, so the mode no longer decides it: the
/// denotation is keyed on the parameter's mode *and* on what the callable's
/// declared row writes [MSR-3]. A reference the row writes has two states at
/// the boundary — the entry state `entry(p)` names and the exit state a bare
/// `p` names in `ensures` — while a by-value parameter and a reference the
/// row only reads have one.
fn parameter_has_exit_state(function: &FunctionSignature, parameter: &ParameterSignature) -> bool {
    parameter.mode.is_reference()
        && function
            .declared_effects
            .writes
            .iter()
            .any(|path| path.root == parameter.declaration)
}

impl<'unit> Checker<'unit> {
    pub(super) fn with_postcondition_context<T>(
        &self,
        record: &PostconditionResolutionRecord,
        result_type: CheckedType,
        datums: Vec<(String, u32, CheckedType)>,
        check: impl FnOnce() -> Result<T, CheckStop>,
    ) -> Result<T, CheckStop> {
        let index = self
            .resolved
            .postconditions()
            .iter()
            .position(|candidate| candidate.block == record.block)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let previous = self
            .active_postcondition
            .replace(Some(super::PostconditionCheckContext {
                record: index,
                result_type,
            }));
        let previous_datums =
            std::mem::replace(&mut *self.active_result_datums.borrow_mut(), datums);
        let result = check();
        self.active_postcondition.set(previous);
        *self.active_result_datums.borrow_mut() = previous_datums;
        result
    }

    /// The result datums one [FN-9] clause admits, by written spelling
    /// [CALL-4].
    ///
    /// An unrouted clause admits every declared result ordinal's binder at
    /// that ordinal's own type. A routed clause admits its fresh payload
    /// datum for the ordinal the route names, and every other ordinal's
    /// binder unchanged; the routed ordinal's own whole-result binder stays
    /// unavailable [FN-9].
    pub(super) fn postcondition_result_datums(
        &self,
        record: &PostconditionResolutionRecord,
        signature: &FunctionSignature,
        selector: &CheckedPostconditionSelector,
    ) -> Vec<(String, u32, CheckedType)> {
        let routed = selector.variant.is_some();
        let mut datums = Vec::with_capacity(record.result_binders.len() + 1);
        for (ordinal, binder) in record.result_binders.iter().enumerate() {
            let Ok(ordinal) = u32::try_from(ordinal) else {
                continue;
            };
            if routed && ordinal == selector.ordinal {
                continue;
            }
            let Some(declared) = signature.results.get(ordinal as usize) else {
                continue;
            };
            datums.push((binder.spelling.clone(), ordinal, declared.ty));
        }
        if routed && let Some(field) = record.fields.first() {
            datums.push((
                field.candidate.spelling.clone(),
                selector.ordinal,
                selector.result_type,
            ));
        }
        datums
    }

    /// The result ordinal and datum type one written selector spelling names
    /// in the clause being checked, when it names one.
    fn active_result_datum(&self, spelling: &str) -> Option<(u32, CheckedType)> {
        self.active_result_datums
            .borrow()
            .iter()
            .find(|(candidate, _, _)| candidate == spelling)
            .map(|(_, ordinal, ty)| (*ordinal, *ty))
    }

    /// Supplies a value-only placeholder to the ordinary expression typer.
    /// The placeholder is discarded immediately after typing; relation
    /// identity is rebuilt from the resolver-owned selector-use record and
    /// never becomes a declaration, binding, or storage location.
    pub(super) fn postcondition_result_placeholder(
        &self,
        atom: NodeId,
    ) -> Result<Option<CheckedValue>, CheckStop> {
        let Some(context) = self.active_postcondition.get() else {
            return Ok(None);
        };
        let record = self
            .resolved
            .postconditions()
            .get(context.record)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let atom_path = self.tree.path(atom)?.components();
        let Some(usage) = record.selector_uses.iter().find(|usage| {
            let path = usage.origin.node().components();
            path.len() > atom_path.len() && path.starts_with(atom_path)
        }) else {
            return Ok(None);
        };
        // [CALL-4] the spelling names a result ordinal, and its datum type is
        // that ordinal's. An ordinal whose datum is not a fragment integer is
        // outside [FN-9]'s admitted operand set in this version.
        let ty = self
            .active_result_datum(&usage.spelling)
            .map_or(context.result_type, |(_, ty)| ty);
        let _ = context;
        Ok(Some(match ty {
            CheckedType::Integer(ty) => CheckedValue::Integer { ty, bits: 0 },
            CheckedType::GenericInt(_) => CheckedValue::NumericIdentity { ty, one: false },
            // [OP-15, MSR-1] a measure is a read-only `own u64` member of the
            // measured place, so a selector atom that reads one stands for a
            // u64 whatever the measured result's own type is. [CALL-4]
            // already admits a result of measured type as an operand; the
            // placeholder has to agree with the measure's type and not with
            // the place's.
            _ if self.selector_atom_reads_a_measure(atom)? => CheckedValue::Integer {
                ty: super::super::model::IntegerType::U64,
                bits: 0,
            },
            _ => {
                return self.issue_origin(
                    SemanticRule::Fn9,
                    &usage.origin,
                    SemanticIssueKind::InvalidPostconditionSelector,
                );
            }
        }))
    }

    /// Whether this selector atom is a `place` whose trailing `psuffix` names
    /// one of [MSR-1]'s four measures.
    fn selector_atom_reads_a_measure(&self, atom: NodeId) -> Result<bool, CheckStop> {
        let place = if self.tree.production(atom)? == Production::Place {
            atom
        } else {
            let Some(place) = self.tree.first_child_with(atom, Production::Place)? else {
                return Ok(false);
            };
            place
        };
        let suffixes = self.tree.children_with(place, Production::Psuffix)?;
        Ok(self.trailing_measure_member(&suffixes)?.is_some())
    }

    /// Performs the one semantic subjudgment that DIAG-1 interleaves into
    /// resolution. This checker is throwaway: it reuses the ordinary nominal,
    /// constant, generic-cycle, signature, and FN-2 implementations, but none
    /// of the scratch identities or tables are published to the real checker.
    pub(super) fn preflight_postcondition_selectors(
        &mut self,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        let records = self.resolved.postconditions().to_vec();
        if records.is_empty() {
            return Ok(());
        }

        let prepared = self.prepare_postcondition_selector_preflight(items);
        match prepared {
            Ok(()) => {}
            // A non-FN-9 source premise that has not succeeded establishes no
            // selector instance. The ordinary checker will publish that
            // verdict unless a resolver verdict was deliberately delayed by
            // FN-9, in which case the original resolver issue wins unchanged.
            Err(
                CheckStop::Issue(_)
                | CheckStop::Unsupported(_)
                | CheckStop::PostconditionPrerequisiteUnavailable,
            ) => return Ok(()),
            Err(stop) => return Err(stop),
        }

        let eligible = self.eligible_postcondition_functions(&[])?;
        let by_function = self.eligible_signatures_by_function(&eligible);
        let mut admitted_records = Vec::new();
        for record in &records {
            let concrete = self.signatures_of(&by_function, &record.function);
            if concrete.is_empty() {
                let symbolic = match self.symbolic_postcondition_signature(record) {
                    Ok(symbolic) => symbolic,
                    Err(
                        CheckStop::Issue(_)
                        | CheckStop::Unsupported(_)
                        | CheckStop::PostconditionPrerequisiteUnavailable,
                    ) => {
                        continue;
                    }
                    Err(stop) => return Err(stop),
                };
                if let Some(signature) = symbolic {
                    let _ = self.admit_postcondition_selector(record, &signature, true)?;
                    admitted_records.push(record.clone());
                }
            } else {
                for signature in concrete {
                    let _ = self.admit_postcondition_selector(record, &signature, false)?;
                }
                admitted_records.push(record.clone());
            }
        }
        self.forward_delayed_postcondition_issue(&admitted_records)
    }

    /// The table positions of the signatures `eligible` names, by the path
    /// of the function each one instantiates, in table order.
    ///
    /// Admitting a selector may append signatures, whose fresh ids `eligible`
    /// never names, and changes no signature it already holds, so one index
    /// taken before the admissions serves every record of one pass.
    fn eligible_signatures_by_function(
        &self,
        eligible: &[FunctionId],
    ) -> HashMap<crate::NodePath, Vec<usize>> {
        let eligible: std::collections::HashSet<FunctionId> = eligible.iter().copied().collect();
        let mut by_function: HashMap<crate::NodePath, Vec<usize>> = HashMap::new();
        for (index, signature) in self.signatures.iter().enumerate() {
            if !eligible.contains(&signature.id) {
                continue;
            }
            if let Ok(path) = self.tree.path(signature.node) {
                by_function.entry(path.clone()).or_default().push(index);
            }
        }
        by_function
    }

    /// The indexed signatures of the function at `function`, cloned.
    fn signatures_of(
        &self,
        by_function: &HashMap<crate::NodePath, Vec<usize>>,
        function: &crate::NodePath,
    ) -> Vec<FunctionSignature> {
        by_function
            .get(function)
            .into_iter()
            .flatten()
            .filter_map(|index| self.signatures.get(*index).cloned())
            .collect()
    }

    fn prepare_postcondition_selector_preflight(
        &mut self,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        self.collect_behavior_groups(items)?;
        self.reject_instantiation_cycles(items)?;
        self.declare_nominals_for_postconditions(items)?;
        self.collect_constants_for_postconditions(items)?;
        self.collect_function_templates_for_postconditions(items)?;
        self.collect_concrete_function_signatures_for_postconditions()
    }

    fn forward_delayed_postcondition_issue(
        &self,
        records: &[PostconditionResolutionRecord],
    ) -> Result<(), CheckStop> {
        // Inventory remains one global stage even though FN-9 delays the
        // entry-local slice. It therefore precedes every delayed entry lookup.
        if let Some(issue) = records
            .iter()
            .find_map(|record| record.entry_inventory_issue.clone())
        {
            return Err(CheckStop::Resolution(Box::new(issue)));
        }
        if let Some(issue) = records
            .iter()
            .find_map(|record| record.entry_resolution_issue.clone())
        {
            return Err(CheckStop::Resolution(Box::new(issue)));
        }
        Ok(())
    }

    fn symbolic_postcondition_signature(
        &mut self,
        record: &PostconditionResolutionRecord,
    ) -> Result<Option<FunctionSignature>, CheckStop> {
        // The internal `FnSig` production carries both [FN-3]'s function-kind
        // parameter and a [PRE-1] declaration head. Only the first carries a
        // `FunctionParameter` declaration, so the node is asked rather than
        // assumed; a prelude record falls through to the ordinary template
        // path below, exactly as a source `fn_decl` does.
        if let Some(node) = self.tree.node_with_path(&record.function)
            && self.tree.production(node)? == Production::FnSig
            && let Some(declaration) = self
                .declarations_at(node, crate::DeclarationRole::FunctionParameter)?
                .first()
                .map(|declaration| declaration.id())
        {
            return self.symbolic_behavior_signature(declaration).map(Some);
        }
        let Some(template) = self
            .function_templates
            .iter()
            .find(|template| {
                self.tree
                    .path(template.node)
                    .is_ok_and(|path| path == &record.function)
            })
            .cloned()
        else {
            return Ok(None);
        };
        if self.postcondition_declaration_unavailable(template.declaration) {
            return Ok(None);
        }
        if template.generic_parameters.is_empty() {
            // A failed nongeneric header establishes no selector premise.
            return Ok(None);
        }
        if !self.postcondition_function_header_dependencies_available(template.node)? {
            return Ok(None);
        }
        let substitution = self.symbolic_generic_substitution(&template.generic_parameters)?;
        self.ensure_nominals_in_function_signature(template.node, &substitution)?;
        self.build_function_signature(&template, substitution, FunctionId(u32::MAX))
            .map(Some)
    }

    /// Computes the exact locally meaningful instance universe. H0 may
    /// temporarily materialize a generic signature from the type/const prefix
    /// of a call whose trailing region arguments are malformed. Such a call
    /// has not completed FN-2 and therefore contributes no selector instance.
    /// The scratch and real checkers each run this helper over their own dense
    /// identities; no FunctionId, NominalId, or CheckedType crosses between
    /// them.
    ///
    /// `seeds` are instances the caller knows are checked although no
    /// nongeneric signature reaches them. Symbolic schema validation checks
    /// every generic template's own body, so the callees those bodies name —
    /// a [PRE-1] record's `ensures` among them — are part of that pass's
    /// universe and are walked from these seeds exactly as a nongeneric
    /// caller's callees are.
    fn eligible_postcondition_functions(
        &self,
        seeds: &[FunctionId],
    ) -> Result<Vec<FunctionId>, CheckStop> {
        let group_functions = self
            .behavior
            .declaration_arguments
            .iter()
            .map(|argument| self.function_argument_instance(*argument))
            .collect::<Result<Vec<_>, _>>()?;
        let mut eligible = self
            .signatures
            .iter()
            .filter(|signature| {
                signature.formal_parameter.is_some()
                    || group_functions.contains(&signature.id)
                    || seeds.contains(&signature.id)
                    || self
                        .templates_by_declaration
                        .get(&signature.declaration)
                        .and_then(|index| self.function_templates.get(*index))
                        .is_some_and(|template| template.generic_parameters.is_empty())
            })
            .map(|signature| signature.id)
            .collect::<Vec<_>>();

        let mut cursor = 0_usize;
        while cursor < eligible.len() {
            let caller = self
                .signatures
                .get(eligible[cursor].0 as usize)
                .cloned()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            for (_, argument) in caller.substitution.entries() {
                if let GenericArgument::Function(argument) = argument {
                    let target = self.function_argument_instance(*argument)?;
                    if !eligible.contains(&target) {
                        eligible.push(target);
                    }
                }
            }
            for call in self.tree.descendants_with(caller.node, Production::Call)? {
                if self.call_is_inside_postcondition(call)? {
                    continue;
                }
                let Some((_, template)) = self.called_function_template(call)? else {
                    continue;
                };
                // [OP-10, OP-11, OP-14] an operand-directed row names no
                // instance in its written syntax; the body check selects one
                // and the deferred instantiation admits its selectors.
                if self.operand_directed_row_index(&template)?.is_some() {
                    continue;
                }
                if !self.postcondition_call_arguments_have_links(call)? {
                    continue;
                }
                let target = if template.generic_parameters.is_empty() {
                    self.functions_by_declaration
                        .get(&template.declaration)
                        .into_iter()
                        .flatten()
                        .copied()
                        .next()
                } else {
                    // A caller whose own substitution is symbolic is a schema
                    // instance, and every callee substitution it forms is
                    // symbolic with it: requiring a concrete one here would
                    // reach no callee of any generic body. A concrete caller
                    // keeps the concrete requirement, which is what excludes
                    // an H0 instance materialized from an incomplete [FN-2]
                    // call.
                    let symbolic_caller = !caller.substitution.is_concrete(&self.elements.borrow());
                    let substitution =
                        match self.call_generic_substitution(call, &template, &caller.substitution)
                        {
                            Ok(substitution)
                                if symbolic_caller
                                    || substitution.is_concrete(&self.elements.borrow()) =>
                            {
                                substitution
                            }
                            Ok(_)
                            | Err(
                                CheckStop::Issue(_)
                                | CheckStop::Unsupported(_)
                                | CheckStop::PostconditionPrerequisiteUnavailable,
                            ) => {
                                continue;
                            }
                            Err(stop) => return Err(stop),
                        };
                    self.functions_by_declaration
                        .get(&template.declaration)
                        .into_iter()
                        .flatten()
                        .copied()
                        .find(|id| {
                            self.signatures
                                .get(id.0 as usize)
                                .is_some_and(|signature| signature.substitution == substitution)
                        })
                };
                let Some(target) = target else {
                    continue;
                };
                let target_signature = self
                    .signatures
                    .get(target.0 as usize)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                // A call's written argument list carries type, const and
                // function arguments alone [GRAM-3], all of which the
                // instance selection above has already read.
                let _ = target_signature;
                if !eligible.contains(&target) {
                    eligible.push(target);
                }
            }
            cursor = cursor
                .checked_add(1)
                .ok_or(SemanticCompilerFailure::CounterOverflow)?;
        }
        Ok(eligible)
    }

    /// Builds final selector metadata after the ordinary H0 signature path.
    /// The verdict-bearing form of the same validation already ran in the
    /// throwaway preflight; any divergence here is a compiler invariant
    /// failure.
    pub(super) fn admit_postcondition_selectors(&mut self) -> Result<(), CheckStop> {
        self.admit_postcondition_selectors_including(&[])
    }

    /// Admits the selectors of one instance built after the ordinary pass.
    ///
    /// An operand-directed [PRE-1] row [OP-10, OP-11, OP-14] selects its
    /// instance from an operand, so that instance is built while a body is
    /// being checked and the whole-inventory admission above has already run.
    /// The record set and the per-signature admission are the same; only the
    /// signature set this call walks is narrower.
    pub(super) fn admit_postcondition_selectors_for(
        &mut self,
        function: FunctionId,
    ) -> Result<(), CheckStop> {
        let records = self.resolved.postconditions().to_vec();
        if records.is_empty() {
            return Ok(());
        }
        let signature = self
            .signatures
            .get(function.0 as usize)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let node = self.tree.path(signature.node)?.clone();
        for record in &records {
            if record.function != node {
                continue;
            }
            let admitted = match self.admit_postcondition_selector(record, &signature, false) {
                Ok(admitted) => admitted,
                Err(CheckStop::Issue(_)) => {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                Err(stop) => return Err(stop),
            };
            self.postcondition_selectors.push(admitted);
        }
        Ok(())
    }

    /// Builds selectors for the ordinary locally reachable set plus concrete
    /// instances replayed from an uninstantiated generic source body. Those
    /// replayed calls were already checked in the schema pass, but their final
    /// FunctionIds are not reachable from a nongeneric concrete caller.
    pub(super) fn admit_postcondition_selectors_including(
        &mut self,
        additional: &[FunctionId],
    ) -> Result<(), CheckStop> {
        let records = self.resolved.postconditions().to_vec();
        if records.is_empty() {
            return Ok(());
        }
        let mut eligible = self.eligible_postcondition_functions(additional)?;
        for function in additional {
            if !eligible.contains(function) {
                eligible.push(*function);
            }
        }
        let by_function = self.eligible_signatures_by_function(&eligible);
        for record in &records {
            let concrete = self.signatures_of(&by_function, &record.function);
            for signature in concrete {
                // A schema instance is judged as a schema instance here too:
                // its type arguments are symbolic and the clause typing that
                // needs a concrete [FN-2] substitution waits for one, exactly
                // as the from-source path above decides.
                let symbolic = !signature.substitution.is_concrete(&self.elements.borrow());
                let admitted = match self.admit_postcondition_selector(record, &signature, symbolic)
                {
                    Ok(admitted) => admitted,
                    Err(CheckStop::Issue(_)) => {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    }
                    Err(stop) => return Err(stop),
                };
                self.postcondition_selectors.push(admitted);
            }
        }
        Ok(())
    }

    pub(super) fn postcondition_selectors_for_signature(
        &self,
        signature: &FunctionSignature,
    ) -> Result<Vec<CheckedPostconditionSelector>, CheckStop> {
        if signature.formal_parameter.is_none()
            && signature.substitution.is_concrete(&self.elements.borrow())
        {
            return Ok(self
                .postcondition_selectors
                .iter()
                .filter(|selector| selector.function == signature.id)
                .cloned()
                .collect());
        }
        self.postcondition_selectors_from_source(signature)
    }

    pub(super) fn postcondition_selectors_from_source(
        &self,
        signature: &FunctionSignature,
    ) -> Result<Vec<CheckedPostconditionSelector>, CheckStop> {
        let function = self.tree.path(signature.node)?;
        let mut selectors = Vec::new();
        for record in self
            .resolved
            .postconditions()
            .iter()
            .filter(|record| &record.function == function)
        {
            let selector = self.admit_postcondition_selector(
                record,
                signature,
                !signature.substitution.is_concrete(&self.elements.borrow()),
            )?;
            // An unbounded symbolic type has no concrete FN-2 fragment
            // judgment yet, so clause typing and selected-return
            // classification wait for a concrete instance. A declared Int
            // bound already supplies the exact symbolic integer row used by
            // ordinary generic validation. The selector is still kept: a
            // clause naming only exclusive exit state supplies no result
            // datum at all [FN-9], so a row like `take_back`, whose result
            // ordinal is the operand's element type, still publishes the
            // length it carries across the call; the schema builder below is
            // what holds every other clause to the fragment.
            selectors.push(selector);
        }
        Ok(selectors)
    }

    pub(super) fn check_postcondition_clause(
        &self,
        function: &FunctionSignature,
        selector: &CheckedPostconditionSelector,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
    ) -> Result<RelationTemplate, CheckStop> {
        let expanded = self.expand_postcondition_clause(function, selector, bindings, counters)?;
        let clause = self
            .tree
            .node_with_path(&selector.block)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let expression = self
            .tree
            .first_child_with(clause, Production::ClauseExpr)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        self.postcondition_relation(expression, expanded)
    }

    pub(super) fn expand_postcondition_clause(
        &self,
        function: &FunctionSignature,
        selector: &CheckedPostconditionSelector,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        let record = self
            .resolved
            .postconditions()
            .iter()
            .find(|record| record.block == selector.block)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let clause = self
            .tree
            .node_with_path(&record.block)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let contract = self
            .tree
            .first_child_with(function.node, Production::ContractBlock)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let datums = self.postcondition_result_datums(record, function, selector);
        self.with_postcondition_context(record, selector.result_type, datums, || {
            let mut expanded_bindings = HashMap::<BindingId, ExpandedClauseExpression>::new();
            for (ordinal, parameter) in function.parameters.iter().enumerate() {
                let local = bindings
                    .get(&parameter.declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                expanded_bindings.insert(
                    local.binding,
                    ExpandedClauseExpression::Datum(ExpandedClauseDatum::Parameter {
                        ordinal: u32::try_from(ordinal)
                            .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                        projections: Vec::new(),
                        ty: parameter.ty,
                        exit_state: parameter_has_exit_state(function, parameter),
                    }),
                );
            }
            for definition in self
                .tree
                .children_with(contract, Production::ContractDefine)?
            {
                let expression = self
                    .tree
                    .first_child_with(definition, Production::Expr)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                if !self.validate_clause_computation(
                    ClauseKind::Postcondition(record),
                    definition,
                    expression,
                )? {
                    self.validate_clause_definition_datum(
                        ClauseKind::Postcondition(record),
                        definition,
                        expression,
                    )?;
                }
                let checked = self.check_statement(
                    function,
                    definition,
                    bindings,
                    counters,
                    ControlScope {
                        loops: &[],
                        give_context: None,
                    },
                )?;
                if !checked.can_continue {
                    return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
                }
                let CheckedStatement::Let { binding, value, .. } = &checked.statement else {
                    return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
                };
                self.validate_clause_checked_forms(
                    ClauseKind::Postcondition(record),
                    definition,
                    value,
                )?;
                self.validate_clause_copy_local(
                    ClauseKind::Postcondition(record),
                    definition,
                    *binding,
                    bindings,
                )?;
                let expanded =
                    self.build_clause_expression(expression, value, bindings, &expanded_bindings)?;
                if expanded.contains_invalid_selector_use() {
                    return self.invalid_postcondition_relation(expression);
                }
                expanded_bindings.insert(*binding, expanded);
            }
            let expression = self
                .tree
                .first_child_with(clause, Production::ClauseExpr)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.validate_clause_condition(ClauseKind::Postcondition(record), clause, expression)?;
            let condition = self.check_expression(function, expression, bindings, 0)?;
            self.validate_clause_checked_forms(
                ClauseKind::Postcondition(record),
                clause,
                &condition.expression,
            )?;
            if condition.mode != CheckedMode::Own || condition.expression.ty() != CheckedType::Bool
            {
                return self.issue_node(
                    SemanticRule::Op5,
                    expression,
                    SemanticIssueKind::InvalidPredicateCondition,
                );
            }
            let expanded = self.build_clause_expression(
                expression,
                &condition.expression,
                bindings,
                &expanded_bindings,
            )?;
            Ok(expanded)
        })
    }

    /// `entry` selects proof state, never an executable place expression.
    /// Its argument must be the declaration's own exclusive parameter.
    pub(super) fn check_entry_formers(
        &self,
        function: &FunctionSignature,
    ) -> Result<(), CheckStop> {
        for base in self
            .tree
            .descendants_with(function.node, Production::Pbase)?
        {
            if !self.has_fixed(base, FixedTerminal::Entry)? {
                continue;
            }
            // A function-kind formal's own contract names that formal's
            // parameters [FN-3]; its signature is judged as its own
            // declaration, so an enclosing declaration skips it here.
            let mut owner = self.tree.parent(base)?;
            let mut nested = false;
            while let Some(node) = owner {
                if node == function.node {
                    break;
                }
                if self.tree.production(node)? == Production::FnSig {
                    nested = true;
                    break;
                }
                owner = self.tree.parent(node)?;
            }
            if nested {
                continue;
            }
            // Clause uses remain provisional until selector admission. This
            // whole-function position check runs outside an active clause.
            let path = self.tree.path(base)?;
            let usage = self
                .resolved
                .lexical_uses_at(base)
                .chain(
                    self.resolved
                        .postconditions()
                        .iter()
                        .flat_map(|record| &record.provisional_uses)
                        .filter(|usage| usage.origin().node() == path),
                )
                .find(|usage| usage.role() == LexicalUseRole::PlaceBase)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let parameter = function.parameters.iter().find(|parameter| {
                matches!(usage.target(), ResolvedTarget::Source { declaration, .. }
                    if declaration == parameter.declaration)
            });
            let mut ancestor = self.tree.parent(base)?;
            let mut in_ensures = false;
            while let Some(node) = ancestor {
                if self.tree.production(node)? == Production::EnsuresClause {
                    in_ensures = true;
                    break;
                }
                if node == function.node {
                    break;
                }
                ancestor = self.tree.parent(node)?;
            }
            if !in_ensures || !parameter.is_some_and(|p| parameter_has_exit_state(function, p)) {
                return self.issue_node(
                    SemanticRule::Msr3,
                    base,
                    SemanticIssueKind::InvalidEntryFormer {
                        mechanical_fix: "use entry only on an exclusive parameter in ensures",
                    },
                );
            }
        }
        Ok(())
    }

    /// [CALL-6] a declaration whose published relations instantiate to a
    /// contradiction is refused at the declaration.
    ///
    /// The set is partitioned by route first, because a routed clause is
    /// established only on its own arm [CALL-6] and two clauses on two arms
    /// are never in one caller state together. Every unrouted clause is in
    /// every route's set, since an unrouted clause selects every explicit
    /// return.
    pub(super) fn check_published_relation_consistency(
        &self,
        function: &FunctionSignature,
        selectors: &[CheckedPostconditionSelector],
        relations: &[RelationTemplate],
    ) -> Result<(), CheckStop> {
        let mut routes: Vec<Option<BuiltinPreludeId>> = vec![None];
        for selector in selectors {
            if let Some(variant) = selector.variant
                && !routes.contains(&Some(variant))
            {
                routes.push(Some(variant));
            }
        }
        for route in routes {
            let published = selectors
                .iter()
                .zip(relations)
                .filter(|(selector, _)| selector.variant.is_none() || selector.variant == route)
                .map(|(_, relation)| relation)
                .collect::<Vec<_>>();
            // One clause is already a set: [CALL-6] asks whether the closure
            // of the published set "derives a negative self-bound", and a
            // single clause equating one term with a displacement of itself
            // derives exactly that. Demanding two clauses would let the
            // shortest inconsistent contract there is publish every fact at
            // every caller.
            if published.is_empty() || !publication::relations_are_contradictory(&published) {
                continue;
            }
            let rendered = selectors
                .iter()
                .zip(relations)
                .filter(|(selector, _)| selector.variant.is_none() || selector.variant == route)
                .map(|(selector, _)| self.postcondition_clause_text(selector))
                .collect::<Result<Vec<_>, _>>()?;
            return self.issue_node(
                SemanticRule::Call6,
                function.node,
                SemanticIssueKind::ContradictoryPublishedRelations {
                    relations: rendered,
                    mechanical_fix: "state one consistent relation set: a contract whose clauses cannot hold together publishes every fact at every caller",
                },
            );
        }
        Ok(())
    }

    /// The written text of one `ensures_clause`, for the [CALL-6]
    /// diagnostic. The clause is quoted as the writer wrote it, because the
    /// judgment is over the written set and the fix is to rewrite it.
    fn postcondition_clause_text(
        &self,
        selector: &CheckedPostconditionSelector,
    ) -> Result<String, CheckStop> {
        let clause = self
            .tree
            .node_with_path(&selector.block)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        Ok(self.tree.source_spelling(clause)?.trim().to_owned())
    }

    fn postcondition_relation(
        &self,
        final_expression: NodeId,
        expanded: ExpandedClauseExpression,
    ) -> Result<RelationTemplate, CheckStop> {
        let ExpandedClauseExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation,
                    operand_type,
                },
            arguments,
            ..
        } = expanded
        else {
            return self.invalid_postcondition_relation(final_expression);
        };
        let [left, right] = arguments.as_slice() else {
            return self.invalid_postcondition_relation(final_expression);
        };
        let Some(left) = self.postcondition_relation_term(left, operand_type) else {
            return self.invalid_postcondition_relation(final_expression);
        };
        let Some(right) = self.postcondition_relation_term(right, operand_type) else {
            return self.invalid_postcondition_relation(final_expression);
        };
        if left.ty() != operand_type || right.ty() != operand_type {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let is_output = |term: &RelationTerm| {
            term.contains_result()
                || matches!(
                    term.datum,
                    RelationDatum::Measure(
                        _,
                        PostconditionPlace {
                            root: PostconditionPlaceRoot::ExitParameter { .. },
                            ..
                        }
                    )
                )
        };
        if !is_output(&left) && !is_output(&right) {
            return self.invalid_postcondition_relation(final_expression);
        }
        let normalized = match operation {
            super::super::model::CheckedIntegerOperation::Equal => NormalizedRelation::Equal,
            super::super::model::CheckedIntegerOperation::NotEqual => NormalizedRelation::NotEqual,
            super::super::model::CheckedIntegerOperation::Less => NormalizedRelation::UpperBound {
                left: 0,
                right: 1,
                strict: true,
            },
            super::super::model::CheckedIntegerOperation::LessEqual => {
                NormalizedRelation::UpperBound {
                    left: 0,
                    right: 1,
                    strict: false,
                }
            }
            super::super::model::CheckedIntegerOperation::Greater => {
                NormalizedRelation::UpperBound {
                    left: 1,
                    right: 0,
                    strict: true,
                }
            }
            super::super::model::CheckedIntegerOperation::GreaterEqual => {
                NormalizedRelation::UpperBound {
                    left: 1,
                    right: 0,
                    strict: false,
                }
            }
            _ => return self.invalid_postcondition_relation(final_expression),
        };
        Ok(RelationTemplate {
            operation,
            operands: [left, right],
            normalized,
        })
    }

    /// One relation term [FN-9]: the datum one clause side names, displaced
    /// by the constant the rest of that side reduces to.
    ///
    /// A side carrying two datums, or a datum with any coefficient other than
    /// one, is outside the difference-bound fragment [ENT-4] and yields
    /// `None`, which is the ordinary FN-9 rejection at the clause.
    fn postcondition_relation_term(
        &self,
        expanded: &ExpandedClauseExpression,
        operand_type: CheckedType,
    ) -> Option<RelationTerm> {
        let (datum, displacement) = self.postcondition_relation_operand(expanded)?;
        match datum {
            Some(datum) => Some(RelationTerm {
                datum,
                displacement,
            }),
            // A side with no datum reduced to one constant; it is a literal
            // operand exactly as a written one is, and the fragment holds it
            // in the operand's own type.
            None => Some(RelationTerm::undisplaced(RelationDatum::Literal {
                value: CheckedValue::Integer {
                    ty: match operand_type {
                        CheckedType::Integer(ty) => ty,
                        _ => return None,
                    },
                    bits: representable_bits(operand_type, displacement)?,
                },
                origin: PostconditionConstantOrigin::Literal,
            })),
        }
    }

    /// One clause side's affine expression, as at most one datum with
    /// coefficient one plus a constant [MSR-5].
    fn postcondition_relation_operand(
        &self,
        expanded: &ExpandedClauseExpression,
    ) -> Option<(Option<RelationDatum>, i128)> {
        if let ExpandedClauseExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation:
                        operation @ (CheckedIntegerOperation::AddExact
                        | CheckedIntegerOperation::SubtractExact
                        | CheckedIntegerOperation::MultiplyExact),
                    ..
                },
            arguments,
            ..
        } = expanded
        {
            let [left, right] = arguments.as_slice() else {
                return None;
            };
            let (left_datum, left_value) = self.postcondition_relation_operand(left)?;
            let (right_datum, right_value) = self.postcondition_relation_operand(right)?;
            return match operation {
                CheckedIntegerOperation::AddExact => {
                    if left_datum.is_some() && right_datum.is_some() {
                        return None;
                    }
                    Some((
                        left_datum.or(right_datum),
                        left_value.checked_add(right_value)?,
                    ))
                }
                // Subtracting a datum gives it coefficient minus one, which
                // no difference-bound term carries.
                CheckedIntegerOperation::SubtractExact => {
                    if right_datum.is_some() {
                        return None;
                    }
                    Some((left_datum, left_value.checked_sub(right_value)?))
                }
                // A multiplication of two constants is one constant; any
                // other coefficient leaves the fragment.
                _ => {
                    if left_datum.is_some() || right_datum.is_some() {
                        return None;
                    }
                    Some((None, left_value.checked_mul(right_value)?))
                }
            };
        }
        let datum = self.postcondition_relation_datum(expanded)?;
        // A written integer literal is a constant of the side rather than
        // its datum; a const generic and a generic numeric identity keep
        // their own datum identity, symbolic or not.
        if let RelationDatum::Literal {
            value: CheckedValue::Integer { ty, bits },
            origin: PostconditionConstantOrigin::Literal,
        } = &datum
        {
            return Some((None, integer_value(*ty, *bits)));
        }
        Some((Some(datum), 0))
    }

    fn postcondition_relation_datum(
        &self,
        expanded: &ExpandedClauseExpression,
    ) -> Option<RelationDatum> {
        match expanded {
            // A result datum is a relation term as itself. A member path
            // below one names storage inside the result rather than the
            // result [CALL-4], and only [MSR-1]'s measure members are
            // admitted terms over such a place; the measure arm below is
            // where those arrive.
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::Result {
                ordinal,
                projections,
                ty,
            }) if projections.is_empty() => Some(RelationDatum::Result {
                ordinal: *ordinal,
                ty: *ty,
            }),
            // [FN-9] a parameter or named-const datum carries field and
            // `deref` projections only: a subscripted readonly field is an
            // [ENT-2] clause (b) term a requirement may name, but no relation
            // datum in this version, and only a measure member of a formal
            // place reaches a relation through a subscript (the arm below).
            ExpandedClauseExpression::Datum(
                ExpandedClauseDatum::Parameter { projections, .. }
                | ExpandedClauseDatum::NamedConst { projections, .. },
            ) if projections.iter().any(|projection| {
                matches!(
                    projection,
                    GoalProjection::Subscript(_) | GoalProjection::FormalSubscript { .. }
                )
            }) =>
            {
                None
            }
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::Parameter {
                ordinal,
                projections,
                ty,
                ..
            }) => Some(RelationDatum::Parameter {
                ordinal: *ordinal,
                projections: projections.clone(),
                ty: *ty,
            }),
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::NamedConst {
                declaration,
                projections,
                ty,
            }) => Some(RelationDatum::NamedConst {
                declaration: *declaration,
                projections: projections.clone(),
                ty: *ty,
            }),
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::Literal { value, origin }) => {
                Some(RelationDatum::Literal {
                    value: value.clone(),
                    origin: origin.clone(),
                })
            }
            // [CALL-4] the clause operands of [FN-9] are terms, so a measure
            // over an admitted formal place is an operand with no per-family
            // admission, and so is one over an admitted result place.
            ExpandedClauseExpression::Operation {
                row:
                    GoalOperation::ArrayMeasure { measure, .. }
                    | GoalOperation::BufferMeasure { measure, .. }
                    | GoalOperation::ContainerMeasure { measure, .. },
                arguments,
                ..
            } => {
                let [ExpandedClauseExpression::Datum(datum)] = arguments.as_slice() else {
                    return None;
                };
                let (root, projections, ty) = match datum {
                    ExpandedClauseDatum::Parameter {
                        ordinal,
                        projections,
                        ty,
                        exit_state,
                    } => (
                        if *exit_state {
                            PostconditionPlaceRoot::ExitParameter { ordinal: *ordinal }
                        } else {
                            PostconditionPlaceRoot::Parameter { ordinal: *ordinal }
                        },
                        projections.clone(),
                        *ty,
                    ),
                    ExpandedClauseDatum::Result {
                        ordinal,
                        projections,
                        ty,
                    } => (
                        PostconditionPlaceRoot::Result { ordinal: *ordinal },
                        projections.clone(),
                        *ty,
                    ),
                    ExpandedClauseDatum::NamedConst { .. }
                    | ExpandedClauseDatum::Literal { .. } => return None,
                };
                Some(RelationDatum::Measure(
                    *measure,
                    PostconditionPlace {
                        root,
                        projections,
                        ty,
                    },
                ))
            }
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::Result { .. })
            | ExpandedClauseExpression::Operation { .. }
            | ExpandedClauseExpression::InvalidSelectorUse { .. } => None,
        }
    }

    fn invalid_postcondition_relation<T>(&self, expression: NodeId) -> Result<T, CheckStop> {
        self.issue_node(
            SemanticRule::Fn9,
            expression,
            SemanticIssueKind::InvalidPostconditionRelation,
        )
    }

    pub(super) fn build_checked_postcondition(
        &self,
        function: &FunctionSignature,
        parameters: &[CheckedParameter],
        selector: CheckedPostconditionSelector,
        relation: RelationTemplate,
        body: &[CheckedStatement],
    ) -> Result<CheckedPostcondition, CheckStop> {
        if !function.substitution.is_concrete(&self.elements.borrow()) {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        self.build_checked_postcondition_inner(
            function, parameters, selector, relation, body, false,
        )
    }

    /// Builds the source-schema FN-9 handoff only when the written relation
    /// and selected returns are already in the ordinary concrete integer
    /// fragment. GenericInt remains an exact symbolic goal datum, never an L0
    /// term, so a postcondition over `T` is intentionally concrete-instance
    /// only.
    pub(super) fn build_checked_schema_postcondition(
        &self,
        function: &FunctionSignature,
        parameters: &[CheckedParameter],
        selector: CheckedPostconditionSelector,
        relation: RelationTemplate,
        body: &[CheckedStatement],
    ) -> Result<Option<CheckedPostcondition>, CheckStop> {
        if function.substitution.is_concrete(&self.elements.borrow()) {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        // FN-9 already admits a clause naming only exclusive exit state
        // regardless of result type (v0.56, FN-9). Such a clause supplies no
        // result datum at all. Keep the integer-result gate for every other
        // clause, and retain the integer-operand and selected-return proof
        // checks below; this exception proves no relation by itself.
        let exclusive_state_only = selector.variant.is_none()
            && relation
                .operands
                .iter()
                .all(|operand| !operand.contains_result())
            && relation.operands.iter().any(|operand| {
                matches!(
                    &operand.datum,
                    RelationDatum::Measure(_, PostconditionPlace {
                        root: PostconditionPlaceRoot::ExitParameter { ordinal }, ..
                    }) if function.parameters.get(*ordinal as usize)
                        .is_some_and(|parameter| parameter_has_exit_state(function, parameter))
                )
            });
        // [FN-9, CALL-4] the second admitted unrouted shape: a result ordinal
        // of a measured type [MSR-1] named as a measure member and nowhere
        // else. Every measure is u64 whatever the element type is, so such a
        // clause is already inside the concrete integer fragment and the
        // symbolic type argument reaches none of it. Without this a [PRE-1]
        // construction row's `ensures result.len == 0_u64` was published to
        // no generic body, and every window proof inside one started with no
        // length at all.
        let measured_result = selector.result_type.measured().is_some()
            || match selector.result_type {
                CheckedType::Nominal(nominal) => self.measured_descendant(nominal)?,
                _ => false,
            };
        let measured_result_only = measured_result
            && relation
                .operands
                .iter()
                .all(|operand| !matches!(operand.datum, RelationDatum::Result { .. }));
        if (!matches!(selector.result_type, CheckedType::Integer(_))
            && !exclusive_state_only
            && !measured_result_only)
            || relation
                .operands
                .iter()
                .any(|operand| !matches!(operand.ty(), CheckedType::Integer(_)))
        {
            return Ok(None);
        }
        // [CALL-4, FN-9] schema admission classifies only the result ordinals
        // the relation names. An unnamed ordinal imposes no postcondition, so
        // its symbolic or affine value cannot disqualify an otherwise closed
        // integer relation over another ordinal.
        let direct_result_ordinals = relation
            .operands
            .iter()
            .filter_map(|operand| match &operand.datum {
                RelationDatum::Result { ordinal, .. } => Some(*ordinal),
                _ => None,
            })
            .collect::<Vec<_>>();
        let measured_result_ordinals = relation
            .operands
            .iter()
            .filter_map(|operand| match &operand.datum {
                RelationDatum::Measure(
                    _,
                    PostconditionPlace {
                        root: PostconditionPlaceRoot::Result { ordinal },
                        ..
                    },
                ) => Some(*ordinal),
                _ => None,
            })
            .collect::<Vec<_>>();
        let checked = self.build_checked_postcondition_inner(
            function, parameters, selector, relation, body, true,
        )?;
        // A state-only clause has no selected-result operand to classify.
        // Its return edges still enter the ordinary proof below; the value
        // returned on those edges (for example unit) is not evidence for it.
        let fragment_returns = exclusive_state_only
            || checked.selected_returns.iter().all(|selected| {
                direct_result_ordinals.iter().all(|ordinal| {
                    selected
                        .values
                        .get(*ordinal as usize)
                        .and_then(Option::as_ref)
                        .is_some_and(|value| match value {
                            PostconditionReturnDatum::ResultPayload { ty } => {
                                matches!(ty, CheckedType::Integer(_))
                            }
                            PostconditionReturnDatum::Place(place) => {
                                matches!(place.ty, CheckedType::Integer(_))
                            }
                            PostconditionReturnDatum::Literal { value, .. } => {
                                matches!(value.ty(), CheckedType::Integer(_))
                            }
                            PostconditionReturnDatum::Measure(..) => true,
                        })
                }) && measured_result_ordinals.iter().all(|ordinal| {
                    matches!(
                        selected
                            .values
                            .get(*ordinal as usize)
                            .and_then(Option::as_ref),
                        Some(PostconditionReturnDatum::Place(_))
                    )
                })
            });
        Ok(fragment_returns.then_some(checked))
    }

    fn build_checked_postcondition_inner(
        &self,
        function: &FunctionSignature,
        parameters: &[CheckedParameter],
        selector: CheckedPostconditionSelector,
        relation: RelationTemplate,
        body: &[CheckedStatement],
        symbolic_schema: bool,
    ) -> Result<CheckedPostcondition, CheckStop> {
        let mut type_substitutions = Vec::new();
        let mut const_substitutions = Vec::new();
        for (declaration, argument) in function.substitution.entries() {
            if matches!(argument, GenericArgument::Function(_)) {
                continue;
            }
            let super::generics::GenericParameterKey::Source(declaration) = declaration else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            match argument {
                GenericArgument::Type(ty) if ty.is_concrete(&self.elements.borrow()) => {
                    type_substitutions.push((*declaration, *ty));
                }
                GenericArgument::Const(super::super::model::CheckedConst::Value(value)) => {
                    const_substitutions.push((*declaration, *value));
                }
                _ if symbolic_schema => {}
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            }
        }

        if parameters.len() != function.parameters.len() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let mut binding_info = HashMap::new();
        for parameter in parameters {
            binding_info.insert(
                parameter.binding,
                PostconditionBindingInfo {
                    ty: parameter.ty,
                    implicit_deref: parameter.mode != CheckedMode::Own,
                },
            );
        }
        self.collect_postcondition_binding_info(body, &mut binding_info);

        // [FN-9, CALL-4] every result ordinal the relation names must
        // evaluate at a selected return to one admitted datum, including a
        // place named only under a measure. An unnamed ordinal imposes nothing.
        let named = relation
            .operands
            .iter()
            .filter_map(|operand| match &operand.datum {
                RelationDatum::Result { ordinal, .. } => Some(*ordinal),
                RelationDatum::Measure(
                    _,
                    PostconditionPlace {
                        root: PostconditionPlaceRoot::Result { ordinal },
                        ..
                    },
                ) => Some(*ordinal),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut selected_returns = Vec::new();
        self.collect_postcondition_returns(
            function,
            &selector,
            &named,
            body,
            &binding_info,
            &mut selected_returns,
        )?;
        selected_returns.sort_by(|left, right| {
            left.statement
                .components()
                .cmp(right.statement.components())
        });
        Ok(CheckedPostcondition {
            selector,
            type_substitutions,
            const_substitutions,
            relation,
            selected_returns,
        })
    }

    fn collect_postcondition_binding_info(
        &self,
        statements: &[CheckedStatement],
        bindings: &mut HashMap<BindingId, PostconditionBindingInfo>,
    ) {
        for statement in statements {
            match statement {
                CheckedStatement::Let { binding, value, .. } => {
                    let implicit_deref = matches!(
                        value,
                        CheckedExpression::BorrowAddressed { .. }
                            | CheckedExpression::BorrowRangeIndex { .. }
                    ) || match value {
                        CheckedExpression::Binding { binding, .. } => bindings
                            .get(binding)
                            .is_some_and(|source| source.implicit_deref),
                        _ => false,
                    };
                    bindings.insert(
                        *binding,
                        PostconditionBindingInfo {
                            ty: value.ty(),
                            implicit_deref,
                        },
                    );
                }
                // [CALL-4] a destructuring `let` binds one fresh own value per
                // declared result ordinal, and each is a place a return can
                // name exactly as an ordinary `let` binding is.
                CheckedStatement::DestructuringLet {
                    bindings: binders, ..
                } => {
                    for (binding, ty, _) in binders {
                        bindings.insert(
                            *binding,
                            PostconditionBindingInfo {
                                ty: *ty,
                                implicit_deref: false,
                            },
                        );
                    }
                }
                CheckedStatement::PropagateLet {
                    binding, ok_type, ..
                } => {
                    bindings.insert(
                        *binding,
                        PostconditionBindingInfo {
                            ty: *ok_type,
                            implicit_deref: false,
                        },
                    );
                }
                CheckedStatement::Match { arms, .. } => {
                    for arm in arms {
                        for binder in &arm.binders {
                            bindings.insert(
                                binder.binding,
                                PostconditionBindingInfo {
                                    ty: binder.ty,
                                    implicit_deref: binder.mode != CheckedMode::Own,
                                },
                            );
                        }
                        self.collect_postcondition_binding_info(&arm.body, bindings);
                    }
                }
                CheckedStatement::ValueMatchLet {
                    binding,
                    result_type,
                    arms,
                    ..
                } => {
                    bindings.insert(
                        *binding,
                        PostconditionBindingInfo {
                            ty: *result_type,
                            implicit_deref: false,
                        },
                    );
                    for arm in arms {
                        for binder in &arm.binders {
                            bindings.insert(
                                binder.binding,
                                PostconditionBindingInfo {
                                    ty: binder.ty,
                                    implicit_deref: binder.mode != CheckedMode::Own,
                                },
                            );
                        }
                        self.collect_postcondition_binding_info(&arm.body, bindings);
                    }
                }
                CheckedStatement::Loop { body, .. } => {
                    self.collect_postcondition_binding_info(body, bindings);
                }
                CheckedStatement::CountedRange {
                    binder,
                    lower,
                    body,
                    ..
                } => {
                    bindings.insert(
                        *binder,
                        PostconditionBindingInfo {
                            ty: lower.ty(),
                            implicit_deref: false,
                        },
                    );
                    self.collect_postcondition_binding_info(body, bindings);
                }
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_postcondition_returns(
        &self,
        function: &FunctionSignature,
        selector: &CheckedPostconditionSelector,
        named: &[u32],
        statements: &[CheckedStatement],
        binding_info: &HashMap<BindingId, PostconditionBindingInfo>,
        selected: &mut Vec<SelectedPostconditionReturn>,
    ) -> Result<(), CheckStop> {
        for statement in statements {
            match statement {
                CheckedStatement::Return {
                    node_path, value, ..
                } => {
                    // [GRAM-4, CALL-4] a `return e1, ..., en;` is checked as
                    // one result-list value, so the ordinals are its fields;
                    // a single-result return is the whole value.
                    let ordinals: Vec<&CheckedExpression> = match (function.result_list, value) {
                        (
                            Some(list),
                            CheckedExpression::ConstructStruct {
                                nominal, fields, ..
                            },
                        ) if *nominal == list => fields.iter().collect(),
                        (Some(_), _) => {
                            return Err(SemanticCompilerFailure::InvalidResolution.into());
                        }
                        (None, value) => vec![value],
                    };
                    // [FN-9] route selection precedes datum admission for
                    // every ordinal. An Err exit is unselected even when a
                    // different result precedes it in the written list.
                    let routed_payload = if selector.variant.is_some() {
                        let ordinal = usize::try_from(selector.ordinal)
                            .map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                        let produced = ordinals
                            .get(ordinal)
                            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                        match self.postcondition_route_payload(
                            function,
                            selector.ordinal,
                            produced,
                            node_path,
                        )? {
                            Some(payload) => Some(payload),
                            None => continue,
                        }
                    } else {
                        None
                    };
                    let mut values = Vec::with_capacity(ordinals.len());
                    for (ordinal, produced) in ordinals.into_iter().enumerate() {
                        let Ok(ordinal) = u32::try_from(ordinal) else {
                            return Err(SemanticCompilerFailure::CounterOverflow.into());
                        };
                        let produced = if ordinal == selector.ordinal {
                            routed_payload.unwrap_or(produced)
                        } else {
                            produced
                        };
                        let datum = if selector.variant.is_some()
                            && ordinal == selector.ordinal
                            && produced.ty() != selector.result_type
                        {
                            Some(PostconditionReturnDatum::ResultPayload {
                                ty: selector.result_type,
                            })
                        } else {
                            self.postcondition_return_datum(produced, node_path, binding_info)?
                        };
                        if datum.is_none() && named.contains(&ordinal) {
                            return self.invalid_postcondition_return(node_path);
                        }
                        values.push(datum);
                    }
                    selected.push(SelectedPostconditionReturn {
                        statement: node_path.clone(),
                        values,
                    });
                }
                CheckedStatement::Match { arms, .. }
                | CheckedStatement::ValueMatchLet { arms, .. } => {
                    for arm in arms {
                        self.collect_postcondition_returns(
                            function,
                            selector,
                            named,
                            &arm.body,
                            binding_info,
                            selected,
                        )?;
                    }
                }
                CheckedStatement::Loop { body, .. }
                | CheckedStatement::CountedRange { body, .. } => self
                    .collect_postcondition_returns(
                        function,
                        selector,
                        named,
                        body,
                        binding_info,
                        selected,
                    )?,
                _ => {}
            }
        }
        Ok(())
    }

    /// The `Ok` payload one routed ordinal produces at a return, `None` for a
    /// direct `Err`, and the whole value for an ordinary forwarded Result.
    fn postcondition_route_payload<'value>(
        &self,
        function: &FunctionSignature,
        ordinal: u32,
        value: &'value CheckedExpression,
        _node_path: &crate::NodePath,
    ) -> Result<Option<&'value CheckedExpression>, CheckStop> {
        let declared = function
            .results
            .get(ordinal as usize)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let CheckedType::Nominal(result_nominal) = declared.ty else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        if self
            .prelude_types
            .get(result_nominal.0 as usize)
            .and_then(|entry| *entry)
            .is_none_or(|ty| !matches!(ty, super::PreludeType::Result(_, _)))
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        match value {
            CheckedExpression::ConstructEnum {
                nominal,
                variant,
                fields,
                ..
            } if *nominal == result_nominal && *variant == 0 && fields.len() == 1 => {
                Ok(fields.first())
            }
            CheckedExpression::ConstructEnum {
                nominal,
                variant,
                fields,
                ..
            } if *nominal == result_nominal && *variant == 1 && fields.len() == 1 => Ok(None),
            _ => Ok(Some(value)),
        }
    }

    fn postcondition_return_datum(
        &self,
        value: &CheckedExpression,
        statement: &crate::NodePath,
        binding_info: &HashMap<BindingId, PostconditionBindingInfo>,
    ) -> Result<Option<PostconditionReturnDatum>, CheckStop> {
        if let Some(place) = self.postcondition_return_place(value, statement, binding_info)? {
            return Ok(Some(PostconditionReturnDatum::Place(place)));
        }
        match value {
            CheckedExpression::Constant(value) => {
                let origin = self.postcondition_return_constant_origin(statement)?;
                Ok(Some(PostconditionReturnDatum::Literal {
                    value: value.clone(),
                    origin,
                }))
            }
            CheckedExpression::ArrayMeasure {
                measure,
                root,
                length,
            } => {
                let place = match root {
                    CheckedArrayRoot::Binding { binding, fields } => {
                        self.postcondition_binding_place(*binding, fields, statement, binding_info)?
                    }
                    CheckedArrayRoot::Constant(constant) => {
                        let constant = self.constant(*constant)?;
                        Some(PostconditionReturnPlace {
                            root: PostconditionReturnPlaceRoot::NamedConst(constant.declaration),
                            projections: Vec::new(),
                            ty: constant.ty,
                            range_referent: false,
                            source: statement.clone(),
                        })
                    }
                };
                let Some(
                    place @ PostconditionReturnPlace {
                        ty:
                            CheckedType::Array {
                                length: actual_length,
                                ..
                            },
                        ..
                    },
                ) = place
                else {
                    return Ok(None);
                };
                if actual_length != *length {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                Ok(Some(PostconditionReturnDatum::Measure(*measure, place)))
            }
            // [FN-9, REF-4] a direct `return deref(part).len` is one exact
            // ENT-2 range-measure term. TYPE-8 carries the range kind in the
            // binding mode rather than `CheckedType`, so retain that kind for
            // the selected-return proof instead of trying to recover it from
            // the element type.
            CheckedExpression::RangeMeasure { measure, root } => {
                let Some(info) = binding_info.get(&root.binding) else {
                    return Ok(None);
                };
                let element = root.element_type;
                if info.ty != element {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                Ok(Some(PostconditionReturnDatum::Measure(
                    *measure,
                    PostconditionReturnPlace {
                        root: PostconditionReturnPlaceRoot::Binding(root.binding),
                        projections: Vec::new(),
                        ty: element,
                        range_referent: true,
                        source: statement.clone(),
                    },
                )))
            }
            CheckedExpression::RangeElementMeasure { measure, place, .. } => {
                let Some(info) = binding_info.get(&place.root.binding) else {
                    return Ok(None);
                };
                if info.ty != place.root.element_type {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                Ok(Some(PostconditionReturnDatum::Measure(
                    *measure,
                    PostconditionReturnPlace {
                        root: PostconditionReturnPlaceRoot::Binding(place.root.binding),
                        projections: place.goal_projections(),
                        ty: place.ty,
                        range_referent: false,
                        source: statement.clone(),
                    },
                )))
            }
            CheckedExpression::BufferMeasure { measure, root } => {
                let expected = CheckedType::Buffer {
                    element: root.element,
                };
                let Some(
                    place @ PostconditionReturnPlace {
                        ty: CheckedType::Buffer { .. },
                        ..
                    },
                ) = self.postcondition_storage_place(
                    root.binding,
                    &root.path,
                    expected,
                    statement,
                    binding_info,
                )?
                else {
                    return Ok(None);
                };
                Ok(Some(PostconditionReturnDatum::Measure(*measure, place)))
            }
            // [MSR-1, CALL-4] a measure of a storage shape, in the same
            // return position the three flat measures already occupy. Its
            // checked path already names the one ENT-2 place, including an
            // ordinary field, Box content, or admitted measured subscript.
            CheckedExpression::ContainerMeasure { measure, root } => {
                let Some(binding) = root.binding() else {
                    return Ok(None);
                };
                let Some(place) = self.postcondition_storage_place(
                    binding,
                    &root.path,
                    root.ty,
                    statement,
                    binding_info,
                )?
                else {
                    return Ok(None);
                };
                Ok(Some(PostconditionReturnDatum::Measure(*measure, place)))
            }
            _ => Ok(None),
        }
    }

    fn postcondition_return_place(
        &self,
        value: &CheckedExpression,
        statement: &crate::NodePath,
        binding_info: &HashMap<BindingId, PostconditionBindingInfo>,
    ) -> Result<Option<PostconditionReturnPlace>, CheckStop> {
        let place = match value {
            CheckedExpression::Binding {
                carrier,
                binding,
                ty,
                ..
            } => {
                let Some(mut place) =
                    self.postcondition_binding_place(*binding, &[], statement, binding_info)?
                else {
                    return Ok(None);
                };
                if place.ty != *ty {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                place.source = carrier.clone();
                place
            }
            CheckedExpression::NamedConstant {
                declaration, value, ..
            } => PostconditionReturnPlace {
                root: PostconditionReturnPlaceRoot::NamedConst(*declaration),
                projections: Vec::new(),
                ty: value.ty(),
                range_referent: false,
                source: statement.clone(),
            },
            CheckedExpression::Project {
                carrier,
                binding,
                fields,
                ty,
                consume_root: false,
                ..
            } => {
                let Some(mut place) =
                    self.postcondition_binding_place(*binding, fields, statement, binding_info)?
                else {
                    return Ok(None);
                };
                if place.ty != *ty {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                place.source = carrier.clone();
                place
            }
            CheckedExpression::DerefAddressed {
                carrier,
                binding,
                ty,
            } => {
                let Some(info) = binding_info.get(binding) else {
                    return Ok(None);
                };
                if info.ty != *ty {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                PostconditionReturnPlace {
                    root: PostconditionReturnPlaceRoot::Binding(*binding),
                    // A reference binding is already the body-place root of
                    // its referent. Clause templates retain their written
                    // wrapper deref, but a concrete selected return does not.
                    projections: Vec::new(),
                    ty: *ty,
                    range_referent: false,
                    source: carrier.clone(),
                }
            }
            CheckedExpression::BoxDeref {
                carrier,
                value,
                referent,
                ..
            } => {
                let Some(mut place) =
                    self.postcondition_return_place(value, statement, binding_info)?
                else {
                    return Ok(None);
                };
                place.projections.push(GoalProjection::Deref);
                place.ty = *referent;
                place.source = carrier.clone();
                place
            }
            CheckedExpression::ProjectValue {
                carrier,
                value,
                field,
                ty,
                ..
            } => {
                let Some(mut place) =
                    self.postcondition_return_place(value, statement, binding_info)?
                else {
                    return Ok(None);
                };
                place.projections.push(GoalProjection::Field(*field));
                place.ty = *ty;
                place.source = carrier.clone();
                place
            }
            _ => return Ok(None),
        };
        Ok(Some(place))
    }

    fn postcondition_binding_place(
        &self,
        binding: BindingId,
        fields: &[u32],
        source: &crate::NodePath,
        binding_info: &HashMap<BindingId, PostconditionBindingInfo>,
    ) -> Result<Option<PostconditionReturnPlace>, CheckStop> {
        let Some(info) = binding_info.get(&binding).copied() else {
            return Ok(None);
        };
        let Some(ty) = self.postcondition_projected_type(info.ty, fields)? else {
            return Ok(None);
        };
        // This is a concrete body place, not a declaration template. A
        // reference binding roots the referent directly; only projections
        // written below it belong in the selected-return identity. Recursive
        // `BoxDeref` classification appends its real content step separately.
        let projections = fields.iter().copied().map(GoalProjection::Field).collect();
        Ok(Some(PostconditionReturnPlace {
            root: PostconditionReturnPlaceRoot::Binding(binding),
            projections,
            ty,
            range_referent: false,
            source: source.clone(),
        }))
    }

    /// Rebuilds one already-checked storage place for [FN-9]'s selected-return
    /// substitution. Unlike the source field-only helper above, this path is
    /// typed checked metadata and retains every [ENT-2] projection, including
    /// the content step of a `Box` [TYPE-9].
    fn postcondition_storage_place(
        &self,
        binding: BindingId,
        path: &[CheckedPlaceStep],
        expected: CheckedType,
        source: &crate::NodePath,
        binding_info: &HashMap<BindingId, PostconditionBindingInfo>,
    ) -> Result<Option<PostconditionReturnPlace>, CheckStop> {
        let Some(info) = binding_info.get(&binding).copied() else {
            return Ok(None);
        };
        let mut ty = info.ty;
        for step in path {
            ty = match step {
                CheckedPlaceStep::Field(field) => {
                    let CheckedType::Nominal(nominal) = ty else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let CheckedNominalKind::Struct { fields } = &self.nominal(nominal)?.kind else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    fields
                        .get(*field as usize)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?
                        .ty
                }
                CheckedPlaceStep::BoxReferent(nominal) => {
                    if ty != CheckedType::Nominal(*nominal) {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    }
                    let CheckedNominalKind::Box { referent, .. } = self.nominal(*nominal)?.kind
                    else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    referent
                }
                CheckedPlaceStep::Subscript(index) => {
                    if ty != index.base_type {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    }
                    index.element_type
                }
            };
        }
        if ty != expected {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(Some(PostconditionReturnPlace {
            root: PostconditionReturnPlaceRoot::Binding(binding),
            projections: path.iter().map(CheckedPlaceStep::goal_projection).collect(),
            ty,
            range_referent: false,
            source: source.clone(),
        }))
    }

    fn postcondition_projected_type(
        &self,
        mut ty: CheckedType,
        fields: &[u32],
    ) -> Result<Option<CheckedType>, CheckStop> {
        for field in fields {
            let CheckedType::Nominal(nominal) = ty else {
                return Ok(None);
            };
            let CheckedNominalKind::Struct { fields } = &self.nominal(nominal)?.kind else {
                return Ok(None);
            };
            let Some(selected) = fields.get(*field as usize) else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            ty = selected.ty;
        }
        Ok(Some(ty))
    }

    fn postcondition_return_constant_origin(
        &self,
        statement: &crate::NodePath,
    ) -> Result<PostconditionConstantOrigin, CheckStop> {
        let statement = self
            .tree
            .node_with_path(statement)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let expression = self
            .tree
            .first_child_with(statement, Production::Expr)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let atoms = self.tree.descendants_with(expression, Production::Atom)?;
        if let [atom] = atoms.as_slice()
            && let Some(literal) = self
                .tree
                .direct_token_with(*atom, crate::TerminalPredicate::Literal)?
        {
            let bytes = self.tree.token_bytes(literal)?;
            if matches!(bytes, b"0_T" | b"1_T") {
                let usage = self.use_at(*atom, LexicalUseRole::GenericNumericSuffix)?;
                let ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::GenericType,
                } = usage.target()
                else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                return Ok(PostconditionConstantOrigin::GenericNumericIdentity {
                    type_parameter: declaration,
                    one: bytes == b"1_T",
                });
            }
        }
        Ok(PostconditionConstantOrigin::Literal)
    }

    fn invalid_postcondition_return<T>(&self, path: &crate::NodePath) -> Result<T, CheckStop> {
        Err(self.invalid_postcondition_return_stop(path))
    }

    fn invalid_postcondition_return_stop(&self, path: &crate::NodePath) -> CheckStop {
        let node = self.tree.node_with_path(path);
        match node {
            Some(node) => self.issue_value(
                SemanticRule::Fn9,
                node,
                SemanticIssueKind::InvalidPostconditionReturn,
            ),
            None => SemanticCompilerFailure::InvalidResolution.into(),
        }
    }

    pub(super) fn postcondition_selector_use_inside(
        &self,
        node: NodeId,
    ) -> Result<bool, CheckStop> {
        let Some(context) = self.active_postcondition.get() else {
            return Ok(false);
        };
        let record = self
            .resolved
            .postconditions()
            .get(context.record)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let owner = self.tree.path(node)?.components();
        Ok(record.selector_uses.iter().any(|usage| {
            let path = usage.origin.node().components();
            path.len() > owner.len() && path.starts_with(owner)
        }))
    }

    /// The result ordinal and datum type one clause `place` is rooted at,
    /// whatever `psuffix` path is written below it [FN-9, CALL-4].
    ///
    /// [OP-15] reads a measure as a member of the measured place, so
    /// `result.len` is one place whose base is the clause's own result datum
    /// rather than a call over it. The base still has to be exactly that
    /// datum: a `deref` around it, or any other `pbase` shape, names no
    /// result.
    pub(super) fn postcondition_selector_place_base(
        &self,
        place: NodeId,
    ) -> Result<Option<(u32, CheckedType)>, CheckStop> {
        let Some(context) = self.active_postcondition.get() else {
            return Ok(None);
        };
        let record = self
            .resolved
            .postconditions()
            .get(context.record)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let pbase = self
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if !self.tree.children(pbase)?.is_empty() {
            return Ok(None);
        }
        let pbase_path = self.tree.path(pbase)?;
        Ok(record
            .selector_uses
            .iter()
            .find(|usage| usage.origin.node() == pbase_path)
            .and_then(|usage| self.active_result_datum(&usage.spelling)))
    }

    /// The declared result ordinal one clause's result datum is anchored to
    /// [CALL-4].
    ///
    /// An unrouted clause is anchored to the first ordinal admitted as a
    /// result datum; every ordinal remains a datum of the clause, and the
    /// anchor only fixes which one the selector's admission judgment reads.
    /// A routed clause is anchored to the ordinal its written binder names,
    /// or, when the binder is omitted, to the one ordinal whose enum type can
    /// carry the route. Two such ordinals leave the route ambiguous, and the
    /// declaration is a hard error citing CALL-4 at the clause.
    fn postcondition_route_ordinal(
        &self,
        record: &PostconditionResolutionRecord,
        signature: &FunctionSignature,
        symbolic: bool,
    ) -> Result<u32, CheckStop> {
        let routed = record.class == PostconditionSelectorClass::Variant;
        if !routed {
            let anchor = record
                .result_binders
                .iter()
                .zip(&signature.results)
                .position(|(_, declared)| self.postcondition_fragment_type(declared.ty, symbolic));
            return u32::try_from(anchor.unwrap_or(0))
                .map_err(|_| SemanticCompilerFailure::CounterOverflow.into());
        }
        if let Some(spelling) = &record.route_ordinal {
            let Some(named) = record
                .result_binders
                .iter()
                .position(|binder| &binder.spelling == spelling)
            else {
                return self
                    .issue_selector(record, SemanticIssueKind::InvalidPostconditionSelector);
            };
            return u32::try_from(named)
                .map_err(|_| SemanticCompilerFailure::CounterOverflow.into());
        }
        let carriers = signature
            .results
            .iter()
            .enumerate()
            .filter(|(_, declared)| self.postcondition_route_carrier(declared.ty, symbolic))
            .map(|(ordinal, _)| ordinal)
            .collect::<Vec<_>>();
        match carriers.as_slice() {
            [only] => {
                u32::try_from(*only).map_err(|_| SemanticCompilerFailure::CounterOverflow.into())
            }
            // Zero carriers is the ordinary route-admission refusal below,
            // which reports the offending result type; anchor it at ordinal
            // zero and let `validate_postcondition_selector` speak.
            [] => Ok(0),
            _ => {
                // [CALL-4] the repair names the results that could carry the
                // route, so the writer picks the one it means.
                let binders = carriers
                    .iter()
                    .filter_map(|ordinal| record.result_binders.get(*ordinal))
                    .map(|binder| format!("`{}`", binder.spelling))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.issue_selector(
                    record,
                    SemanticIssueKind::AmbiguousResultRoute {
                        mechanical_fix: format!(
                            "more than one result can carry this route: name the one it applies to, writing `when r is` before its variant with `r` one of {binders}"
                        ),
                    },
                )
            }
        }
    }

    /// Whether this nominal is a `Box` whose content the measure table gives
    /// a row [TYPE-9, MSR-1].
    /// [CALL-4] whether a result of this nominal type reaches a measured
    /// place through an owned descendant projection made of struct-field
    /// selections and `Box` `inner` steps [MSR-3]: `made.storage.len`, or
    /// the `result.inner.len` every boxed construction record publishes. An
    /// enum payload step reaches none, because no route selects a variant of
    /// an unrouted result.
    fn measured_descendant(
        &self,
        nominal: super::super::model::NominalId,
    ) -> Result<bool, CheckStop> {
        let mut pending = vec![nominal];
        let mut seen = std::collections::HashSet::new();
        while let Some(nominal) = pending.pop() {
            if !seen.insert(nominal) {
                continue;
            }
            let children = match &self.nominal(nominal)?.kind {
                CheckedNominalKind::Box { referent, .. } => vec![*referent],
                CheckedNominalKind::Struct { fields } => {
                    fields.iter().map(|field| field.ty).collect()
                }
                _ => Vec::new(),
            };
            for child in children {
                if child.measured().is_some() {
                    return Ok(true);
                }
                if let CheckedType::Nominal(inner) = child {
                    pending.push(inner);
                }
            }
        }
        Ok(false)
    }

    /// Whether one declared result type can carry a route in this version:
    /// exactly `own Result<T, E>` with T a fragment integer [FN-9, CALL-4].
    fn postcondition_route_carrier(&self, ty: CheckedType, symbolic: bool) -> bool {
        let CheckedType::Nominal(nominal) = ty else {
            return false;
        };
        matches!(
            self.prelude_types.get(nominal.0 as usize).and_then(|entry| *entry),
            Some(super::PreludeType::Result(value, _))
                if self.postcondition_fragment_type(value, symbolic)
        )
    }

    /// [FN-9, CALL-4] admits one clause's selector for one signature; a
    /// rejection raised for a requested concrete instance names its requester
    /// [FN-2, MOD-8].
    fn admit_postcondition_selector(
        &self,
        record: &PostconditionResolutionRecord,
        signature: &FunctionSignature,
        symbolic: bool,
    ) -> Result<CheckedPostconditionSelector, CheckStop> {
        self.admit_postcondition_selector_unattributed(record, signature, symbolic)
            .map_err(|stop| self.attribute_to_request(signature.id, stop))
    }

    fn admit_postcondition_selector_unattributed(
        &self,
        record: &PostconditionResolutionRecord,
        signature: &FunctionSignature,
        symbolic: bool,
    ) -> Result<CheckedPostconditionSelector, CheckStop> {
        let state_only = record.class == PostconditionSelectorClass::Plain
            && record.selector_uses.is_empty()
            && signature
                .parameters
                .iter()
                .any(|parameter| parameter_has_exit_state(signature, parameter));
        if signature
            .results
            .iter()
            .any(|entry| entry.mode != CheckedMode::Own)
            && !state_only
        {
            return self.issue_selector(record, SemanticIssueKind::InvalidPostconditionSelector);
        }

        // [CALL-4] a route applies to exactly one declared result ordinal:
        // the one its written binder names, or — when the binder is omitted —
        // the one ordinal whose type carries the route's variant. Two ordinals
        // that could carry it leave the route ambiguous and the declaration is
        // refused here.
        let ordinal = self.postcondition_route_ordinal(record, signature, symbolic)?;
        let declared = signature
            .results
            .get(ordinal as usize)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;

        let (admission, result_type) = match declared.ty {
            ty if self.postcondition_fragment_type(ty, symbolic) => {
                (SelectorAdmissionType::Fragment, ty)
            }
            CheckedType::Nominal(nominal) => match self
                .prelude_types
                .get(nominal.0 as usize)
                .and_then(|entry| *entry)
            {
                Some(super::PreludeType::Result(value, _))
                    if self.postcondition_fragment_type(value, symbolic) =>
                {
                    (SelectorAdmissionType::ResultFragment, value)
                }
                // [MSR-1] admits a measure place formed with field-selection
                // steps, and [TYPE-9] reaches a `Box`'s content by exactly
                // one such step, `b.inner`. A result whose owned descendant
                // reached by such steps is measured therefore supplies the
                // same measured datum a directly measured result does: every
                // boxed construction record's `ensures result.inner.len`,
                // and a constructor's `ensures made.storage.len` [CALL-4].
                _ if self.measured_descendant(nominal)? => {
                    (SelectorAdmissionType::Measured, declared.ty)
                }
                _ => (SelectorAdmissionType::Invalid, declared.ty),
            },
            CheckedType::Generic(_) if symbolic => (SelectorAdmissionType::Symbolic, declared.ty),
            // [CALL-4] a result of measured type is admitted, and a measure
            // over that result place is the operand it supplies.
            ty if super::expressions::flat_storage::measured_kind_of(ty).is_some() => {
                (SelectorAdmissionType::Measured, ty)
            }
            _ => (SelectorAdmissionType::Invalid, declared.ty),
        };
        self.validate_postcondition_selector(
            record,
            if state_only {
                SelectorAdmissionType::Symbolic
            } else {
                admission
            },
            ordinal,
        )?;

        let (candidate, variant, field) = match record.class {
            PostconditionSelectorClass::Plain => (
                record
                    .result_binders
                    .get(ordinal as usize)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                None,
                None,
            ),
            PostconditionSelectorClass::Variant => {
                let ResolvedTarget::Prelude(variant) = record
                    .variant_target
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let field = record
                    .fields
                    .first()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                (
                    &field.candidate,
                    Some(variant),
                    Some(PostconditionFieldIdentity {
                        declaration: BuiltinPreludeId::OK_VALUE,
                        origin: field.origin.clone(),
                    }),
                )
            }
        };

        Ok(CheckedPostconditionSelector {
            function: signature.id,
            block: record.block.clone(),
            selector: record.selector.clone(),
            candidate: candidate.origin.clone(),
            ordinal,
            variant,
            field,
            result_type,
        })
    }

    fn validate_postcondition_selector(
        &self,
        record: &PostconditionResolutionRecord,
        admission: SelectorAdmissionType,
        ordinal: u32,
    ) -> Result<(), CheckStop> {
        let candidate = match record.class {
            PostconditionSelectorClass::Plain => {
                if !matches!(
                    admission,
                    SelectorAdmissionType::Fragment
                        | SelectorAdmissionType::Symbolic
                        | SelectorAdmissionType::Measured
                ) {
                    return self
                        .issue_selector(record, SemanticIssueKind::InvalidPostconditionSelector);
                }
                record
                    .result_binders
                    .get(ordinal as usize)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
            }
            PostconditionSelectorClass::Variant => {
                if !matches!(
                    admission,
                    SelectorAdmissionType::ResultFragment | SelectorAdmissionType::Symbolic
                ) {
                    return self
                        .issue_selector(record, SemanticIssueKind::InvalidPostconditionSelector);
                }
                if record.variant_target != Some(ResolvedTarget::Prelude(BuiltinPreludeId::OK)) {
                    return self
                        .issue_selector(record, SemanticIssueKind::InvalidPostconditionSelector);
                }
                let Some(field) = record.fields.first() else {
                    return self.issue_selector(
                        record,
                        SemanticIssueKind::InvalidPostconditionFields {
                            required_fields: vec!["value".to_owned()],
                        },
                    );
                };
                if field.spelling != "value" {
                    return self.issue_origin_node(
                        &field.origin,
                        SemanticIssueKind::InvalidPostconditionFields {
                            required_fields: vec!["value".to_owned()],
                        },
                    );
                }
                if let Some(extra) = record.fields.get(1) {
                    return self.issue_origin_node(
                        &extra.origin,
                        SemanticIssueKind::InvalidPostconditionFields {
                            required_fields: vec!["value".to_owned()],
                        },
                    );
                }
                &field.candidate
            }
        };
        self.check_postcondition_candidate(candidate)
    }

    fn postcondition_fragment_type(&self, ty: CheckedType, symbolic: bool) -> bool {
        matches!(ty, CheckedType::Integer(_))
            || symbolic && matches!(ty, CheckedType::Generic(_) | CheckedType::GenericInt(_))
    }

    fn check_postcondition_candidate(
        &self,
        candidate: &PostconditionCandidateRecord,
    ) -> Result<(), CheckStop> {
        if candidate
            .paired_field
            .as_ref()
            .is_some_and(|field| field == &candidate.spelling)
            || !candidate.live_conflicts.is_empty()
        {
            return self.issue_origin(
                SemanticRule::Fn9,
                &candidate.origin,
                SemanticIssueKind::PostconditionCandidateNotFresh {
                    spelling: candidate.spelling.clone(),
                    conflicts: candidate.live_conflicts.clone(),
                },
            );
        }
        if let Some(local) = &candidate.later_local_collision {
            return self.issue_origin(
                SemanticRule::Fn9,
                local,
                SemanticIssueKind::PostconditionLocalShadowsResult {
                    spelling: candidate.spelling.clone(),
                    selector: candidate.origin.clone(),
                },
            );
        }
        Ok(())
    }

    fn issue_selector<T>(
        &self,
        record: &PostconditionResolutionRecord,
        kind: SemanticIssueKind,
    ) -> Result<T, CheckStop> {
        // [CALL-4] owns the result ordinal and the route's ambiguity; every
        // other selector rejection is [FN-9]'s admission.
        let rule = if matches!(kind, SemanticIssueKind::AmbiguousResultRoute { .. }) {
            SemanticRule::Call4
        } else {
            SemanticRule::Fn9
        };
        let node = self
            .tree
            .node_with_path(&record.selector)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.issue_node(rule, node, kind)
    }

    fn issue_origin<T>(
        &self,
        rule: SemanticRule,
        origin: &SourceOrigin,
        kind: SemanticIssueKind,
    ) -> Result<T, CheckStop> {
        Err(CheckStop::source_issue(SemanticIssue {
            rule,
            location: SemanticLocation::SourceNode(origin.node().clone(), origin.coordinate()),
            kind,
            request: None,
        }))
    }

    fn issue_origin_node<T>(
        &self,
        origin: &SourceOrigin,
        kind: SemanticIssueKind,
    ) -> Result<T, CheckStop> {
        let node = self
            .tree
            .node_with_path(origin.node())
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.issue_node(SemanticRule::Fn9, node, kind)
    }
}

/// The mathematical value of one checked integer constant, whose `bits` hold
/// the type-width two's-complement pattern.
const fn integer_value(ty: IntegerType, bits: u64) -> i128 {
    let value = bits as i128;
    if ty.signed() {
        let width = ty.width() as u32;
        let sign_bit = 1_u64 << (width - 1);
        if bits & sign_bit != 0 {
            return value - (1_i128 << width);
        }
    }
    value
}

/// The type-width bit pattern of one mathematical value, or `None` when the
/// value does not fit that type. A clause side reduces over the mathematical
/// integers [MSR-5], so a constant side outside its own operand type is not
/// a relation datum and the clause is refused rather than wrapped.
const fn representable_bits(ty: CheckedType, value: i128) -> Option<u64> {
    let CheckedType::Integer(ty) = ty else {
        return None;
    };
    let width = ty.width() as u32;
    let (low, high) = if ty.signed() {
        (-(1_i128 << (width - 1)), (1_i128 << (width - 1)) - 1)
    } else {
        (0, (1_i128 << width) - 1)
    };
    if value < low || value > high {
        return None;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    Some((value & ((1_i128 << width) - 1)) as u64)
}
