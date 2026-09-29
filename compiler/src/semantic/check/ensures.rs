use crate::semantic::check::CheckContext;
use crate::semantic::check::FunctionContext;
use crate::semantic::check::{DeclarationInventory, TypeContext};
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
    CheckedPostcondition, CheckedPostconditionSelector, NormalizedRelation, ParameterDenotation,
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
    /// [FN-9] a `Result` or `Option` ordinal whose success payload supplies
    /// data under [CALL-4]; only a routed clause may name it.
    SuccessPayload,
    Symbolic,
    /// [CALL-4] one declared result of measured or aggregate type. Its value
    /// is no [ENT-2] term, so only the places its owned descendant
    /// projections reach are admitted clause operands: a fragment-integer
    /// place as its value and a measured place through its measures.
    Aggregate,
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

impl<'unit> Checker<'_, 'unit> {
    /// The result datums one [FN-9] clause admits, by written spelling
    /// [CALL-4].
    ///
    /// An unrouted clause admits every declared result ordinal's binder at
    /// that ordinal's own type. A routed clause admits its fresh payload
    /// datum for the ordinal the route names, and every other ordinal's
    /// binder unchanged; the routed ordinal's own whole-result binder stays
    /// unavailable [FN-9].
    pub(super) fn postcondition_result_datums(
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
    fn active_result_datum(
        check_context: &CheckContext<'_>,
        spelling: &str,
    ) -> Option<(u32, CheckedType)> {
        check_context
            .active_result_datums
            .iter()
            .find(|(candidate, _, _)| candidate == spelling)
            .map(|(_, ordinal, ty)| (*ordinal, *ty))
    }

    /// Performs the one semantic subjudgment that DIAG-1 interleaves into
    /// resolution. Its availability and judgment state are local to the
    /// preflight view; formed type and callable identities stay interned for
    /// ordinary checking, which still runs all required source judgments.
    pub(super) fn preflight_postcondition_selectors(
        &mut self,
        check_context: &CheckContext<'_>,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        let records = self.types.declarations.resolved.postconditions().to_vec();
        if records.is_empty() {
            return Ok(());
        }

        let prepared = self.prepare_postcondition_selector_preflight(check_context, items);
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

        let eligible = self.eligible_postcondition_functions(check_context, &[])?;
        let by_function = self.types.eligible_signatures_by_function(&eligible);
        let mut admitted_records = Vec::new();
        for record in &records {
            let concrete = self.types.signatures_of(&by_function, &record.function);
            if concrete.is_empty() {
                let symbolic = match self.symbolic_postcondition_signature(check_context, record) {
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
                    let _ = self
                        .types
                        .admit_postcondition_selector(record, &signature, true)?;
                    admitted_records.push(record.clone());
                }
            } else {
                for signature in concrete {
                    let _ = self
                        .types
                        .admit_postcondition_selector(record, &signature, false)?;
                }
                admitted_records.push(record.clone());
            }
        }
        Checker::forward_delayed_postcondition_issue(&admitted_records)
    }

    fn prepare_postcondition_selector_preflight(
        &mut self,
        check_context: &CheckContext<'_>,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        self.types.collect_behavior_groups(check_context, items)?;
        self.types
            .reject_instantiation_cycles(check_context, items)?;
        self.declare_nominals_for_postconditions(check_context, items)?;
        self.collect_constants_for_postconditions(check_context, items)?;
        self.collect_function_templates_for_postconditions(check_context, items)?;
        self.collect_concrete_function_signatures_for_postconditions(check_context)
    }

    fn forward_delayed_postcondition_issue(
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
        check_context: &CheckContext<'_>,
        record: &PostconditionResolutionRecord,
    ) -> Result<Option<FunctionSignature>, CheckStop> {
        // The internal `FnSig` production carries both [FN-3]'s function-kind
        // parameter and a [PRE-1] declaration head. Only the first carries a
        // `FunctionParameter` declaration, so the node is asked rather than
        // assumed; a prelude record falls through to the ordinary template
        // path below, exactly as a source `fn_decl` does.
        if let Some(node) = self
            .types
            .declarations
            .tree
            .node_with_path(&record.function)
            && self.types.declarations.tree.production(node)? == Production::FnSig
            && let Some(declaration) = self
                .types
                .declarations
                .declarations_at(node, crate::DeclarationRole::FunctionParameter)?
                .first()
                .map(|declaration| declaration.id())
        {
            return self
                .symbolic_behavior_signature(check_context, declaration)
                .map(Some);
        }
        let Some(template) = self
            .types
            .function_templates
            .iter()
            .find(|template| {
                self.types
                    .declarations
                    .tree
                    .path(template.node)
                    .is_ok_and(|path| path == &record.function)
            })
            .cloned()
        else {
            return Ok(None);
        };
        if self
            .analysis
            .postcondition_declaration_unavailable(template.declaration)
        {
            return Ok(None);
        }
        if template.generic_parameters.is_empty() {
            // A failed nongeneric header establishes no selector premise.
            return Ok(None);
        }
        if !self.postcondition_function_header_dependencies_available(template.node)? {
            return Ok(None);
        }
        let substitution = Checker::symbolic_generic_substitution(&template.generic_parameters)?;
        self.ensure_nominals_in_function_signature(check_context, template.node, &substitution)?;
        self.build_function_signature(check_context, &template, substitution, FunctionId(u32::MAX))
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
        &mut self,
        check_context: &CheckContext<'_>,
        seeds: &[FunctionId],
    ) -> Result<Vec<FunctionId>, CheckStop> {
        let group_functions = self
            .types
            .behavior
            .declaration_arguments
            .iter()
            .map(|argument| self.types.function_argument_instance(*argument))
            .collect::<Result<Vec<_>, _>>()?;
        let mut eligible = self
            .types
            .view
            .functions
            .iter()
            .map(|id| &self.types.signatures[id.0 as usize])
            .filter(|signature| {
                signature.formal_parameter.is_some()
                    || group_functions.contains(&signature.id)
                    || seeds.contains(&signature.id)
                    || self
                        .types
                        .templates_by_declaration
                        .get(&signature.declaration)
                        .and_then(|index| self.types.function_templates.get(*index))
                        .is_some_and(|template| template.generic_parameters.is_empty())
            })
            .map(|signature| signature.id)
            .collect::<Vec<_>>();

        let mut cursor = 0_usize;
        while cursor < eligible.len() {
            let caller = self
                .types
                .signatures
                .get(eligible[cursor].0 as usize)
                .cloned()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            for (_, argument) in caller.substitution.entries() {
                if let GenericArgument::Function(argument) = argument {
                    let target = self.types.function_argument_instance(*argument)?;
                    if !eligible.contains(&target) {
                        eligible.push(target);
                    }
                }
            }
            for call in self
                .types
                .declarations
                .tree
                .descendants_with(caller.node, Production::Call)?
            {
                if self.types.declarations.call_is_inside_postcondition(call)? {
                    continue;
                }
                let Some((_, template)) = self.called_function_template(call)? else {
                    continue;
                };
                // [OP-10, OP-11, OP-14] an operand-directed row names no
                // instance in its written syntax; the body check selects one
                // and the deferred instantiation admits its selectors.
                if self
                    .types
                    .declarations
                    .operand_directed_row_index(&template)?
                    .is_some()
                {
                    continue;
                }
                if !self.postcondition_call_arguments_have_links(call)? {
                    continue;
                }
                let target = if template.generic_parameters.is_empty() {
                    self.types
                        .functions_by_declaration
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
                    let symbolic_caller = !caller.substitution.is_concrete(&self.types.elements);
                    let substitution = match self.call_generic_substitution(
                        check_context,
                        call,
                        &template,
                        &caller.substitution,
                    ) {
                        Ok(substitution)
                            if symbolic_caller
                                || substitution.is_concrete(&self.types.elements) =>
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
                    self.types
                        .functions_by_declaration
                        .get(&template.declaration)
                        .into_iter()
                        .flatten()
                        .copied()
                        .find(|id| {
                            self.types
                                .signatures
                                .get(id.0 as usize)
                                .is_some_and(|signature| signature.substitution == substitution)
                        })
                };
                let Some(target) = target else {
                    continue;
                };
                let target_signature = self
                    .types
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
    /// preflight view; any divergence here is a compiler invariant
    /// failure.
    pub(super) fn admit_postcondition_selectors(
        &mut self,
        check_context: &CheckContext<'_>,
    ) -> Result<(), CheckStop> {
        self.admit_postcondition_selectors_including(check_context, &[])
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
        let records = self.types.declarations.resolved.postconditions().to_vec();
        if records.is_empty() {
            return Ok(());
        }
        let signature = self
            .types
            .signatures
            .get(function.0 as usize)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let node = self.types.declarations.tree.path(signature.node)?.clone();
        for record in &records {
            if record.function != node {
                continue;
            }
            let admitted = match self
                .types
                .admit_postcondition_selector(record, &signature, false)
            {
                Ok(admitted) => admitted,
                Err(CheckStop::Issue(_)) => {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                Err(stop) => return Err(stop),
            };
            self.analysis.postcondition_selectors.push(admitted);
        }
        Ok(())
    }

    /// Builds selectors for the ordinary locally reachable set plus concrete
    /// instances retained from an uninstantiated generic source body. Those
    /// calls were discovered in the symbolic view, but their FunctionIds
    /// are not reachable from a nongeneric concrete caller.
    pub(super) fn admit_postcondition_selectors_including(
        &mut self,
        check_context: &CheckContext<'_>,
        additional: &[FunctionId],
    ) -> Result<(), CheckStop> {
        let records = self.types.declarations.resolved.postconditions().to_vec();
        if records.is_empty() {
            return Ok(());
        }
        let mut eligible = self.eligible_postcondition_functions(check_context, additional)?;
        for function in additional {
            if !eligible.contains(function) {
                eligible.push(*function);
            }
        }
        let by_function = self.types.eligible_signatures_by_function(&eligible);
        for record in &records {
            let concrete = self.types.signatures_of(&by_function, &record.function);
            for signature in concrete {
                // A schema instance is judged as a schema instance here too:
                // its type arguments are symbolic and the clause typing that
                // needs a concrete [FN-2] substitution waits for one, exactly
                // as the from-source path above decides.
                let symbolic = !signature.substitution.is_concrete(&self.types.elements);
                let admitted = match self
                    .types
                    .admit_postcondition_selector(record, &signature, symbolic)
                {
                    Ok(admitted) => admitted,
                    Err(CheckStop::Issue(_)) => {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    }
                    Err(stop) => return Err(stop),
                };
                self.analysis.postcondition_selectors.push(admitted);
            }
        }
        Ok(())
    }

    pub(super) fn postcondition_selectors_for_signature(
        &self,
        signature: &FunctionSignature,
    ) -> Result<Vec<CheckedPostconditionSelector>, CheckStop> {
        if signature.formal_parameter.is_none()
            && signature.substitution.is_concrete(&self.types.elements)
        {
            return Ok(self
                .analysis
                .postcondition_selectors
                .iter()
                .filter(|selector| selector.function == signature.id)
                .cloned()
                .collect());
        }
        self.types.postcondition_selectors_from_source(signature)
    }

    pub(super) fn check_postcondition_clause(
        &mut self,
        context: FunctionContext<'_, '_>,
        selector: &CheckedPostconditionSelector,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
    ) -> Result<RelationTemplate, CheckStop> {
        let expanded = self.expand_postcondition_clause(context, selector, bindings, counters)?;
        let clause = self
            .types
            .declarations
            .tree
            .node_with_path(&selector.block)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let expression = self
            .types
            .declarations
            .tree
            .first_child_with(clause, Production::ClauseExpr)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        self.types
            .declarations
            .postcondition_relation(expression, expanded)
    }

    pub(super) fn expand_postcondition_clause(
        &mut self,
        context: FunctionContext<'_, '_>,
        selector: &CheckedPostconditionSelector,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
    ) -> Result<ExpandedClauseExpression, CheckStop> {
        let FunctionContext {
            check_context,
            function,
        } = context;
        let record = self
            .types
            .declarations
            .resolved
            .postconditions()
            .iter()
            .find(|record| record.block == selector.block)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let clause = self
            .types
            .declarations
            .tree
            .node_with_path(&record.block)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let contract = self
            .types
            .declarations
            .tree
            .first_child_with(function.node, Production::ContractBlock)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let datums = Checker::postcondition_result_datums(record, function, selector);
        let record_index = self
            .types
            .declarations
            .resolved
            .postconditions()
            .iter()
            .position(|candidate| candidate.block == record.block)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let check_context = &CheckContext {
            active_postcondition: Some(super::PostconditionCheckContext {
                record: record_index,
                result_type: selector.result_type,
            }),
            active_result_datums: &datums,
            ..*check_context
        };
        {
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
                        denotation: if parameter_has_exit_state(function, parameter) {
                            ParameterDenotation::ExitState
                        } else {
                            ParameterDenotation::EntryImage
                        },
                    }),
                );
            }
            for definition in self
                .types
                .declarations
                .tree
                .children_with(contract, Production::ContractDefine)?
            {
                let expression = self
                    .types
                    .declarations
                    .tree
                    .first_child_with(definition, Production::Expr)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                if !self.types.declarations.validate_clause_computation(
                    ClauseKind::Postcondition(record),
                    definition,
                    expression,
                )? {
                    self.types.declarations.validate_clause_definition_datum(
                        ClauseKind::Postcondition(record),
                        definition,
                        expression,
                    )?;
                }
                let checked = self.check_statement(
                    FunctionContext {
                        check_context,
                        function,
                    },
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
                self.types.validate_clause_checked_forms(
                    ClauseKind::Postcondition(record),
                    definition,
                    value,
                )?;
                self.types.validate_clause_copy_local(
                    check_context,
                    ClauseKind::Postcondition(record),
                    definition,
                    *binding,
                    bindings,
                )?;
                let expanded = self.types.build_clause_expression(
                    check_context,
                    expression,
                    value,
                    bindings,
                    &expanded_bindings,
                )?;
                if expanded.contains_invalid_selector_use() {
                    return self
                        .types
                        .declarations
                        .invalid_postcondition_relation(expression);
                }
                expanded_bindings.insert(*binding, expanded);
            }
            let expression = self
                .types
                .declarations
                .tree
                .first_child_with(clause, Production::ClauseExpr)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.types.declarations.validate_clause_condition(
                ClauseKind::Postcondition(record),
                clause,
                expression,
            )?;
            let condition = self.check_expression(
                FunctionContext {
                    check_context,
                    function,
                },
                expression,
                bindings,
                0,
            )?;
            self.types.validate_clause_checked_forms(
                ClauseKind::Postcondition(record),
                clause,
                &condition.expression,
            )?;
            if condition.mode != CheckedMode::Own || condition.expression.ty() != CheckedType::Bool
            {
                return self.types.declarations.issue_node(
                    SemanticRule::Op5,
                    expression,
                    SemanticIssueKind::InvalidPredicateCondition,
                );
            }
            let expanded = self.types.build_clause_expression(
                check_context,
                expression,
                &condition.expression,
                bindings,
                &expanded_bindings,
            )?;
            Ok(expanded)
        }
    }

    /// One relation term [FN-9]: the datum one clause side names, displaced
    /// by the constant the rest of that side reduces to.
    ///
    /// A side carrying two datums, or a datum with any coefficient other than
    /// one, is outside the difference-bound fragment [ENT-4] and yields
    /// `None`, which is the ordinary FN-9 rejection at the clause.
    fn postcondition_relation_term(
        expanded: &ExpandedClauseExpression,
        operand_type: CheckedType,
    ) -> Option<RelationTerm> {
        let (datum, displacement) = Checker::postcondition_relation_operand(expanded)?;
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
            let (left_datum, left_value) = Checker::postcondition_relation_operand(left)?;
            let (right_datum, right_value) = Checker::postcondition_relation_operand(right)?;
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
        let datum = Checker::postcondition_relation_datum(expanded)?;
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

    fn postcondition_relation_datum(expanded: &ExpandedClauseExpression) -> Option<RelationDatum> {
        match expanded {
            // [CALL-4] a result datum is a relation term as itself, or as the
            // fragment-integer place its owned descendant projection reaches
            // below the result: `result.width`, `made.header.width`. The
            // clause walk admitted only struct-field and `Box` content steps;
            // a measure member over a result place is the measure arm below.
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::Result {
                ordinal,
                projections,
                ty,
            }) if projections.is_empty()
                || projections
                    .iter()
                    .all(|projection| {
                        matches!(projection, GoalProjection::Field(_) | GoalProjection::Deref)
                    }) =>
            {
                Some(RelationDatum::Result {
                    ordinal: *ordinal,
                    projections: projections.clone(),
                    ty: *ty,
                })
            }
            // [FN-9] a parameter or named-const datum carries field and
            // Box-content projections only: a subscripted readonly field is an
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
                denotation,
            }) => Some(RelationDatum::Parameter {
                ordinal: *ordinal,
                projections: projections.clone(),
                ty: *ty,
                denotation: *denotation,
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
            // [ENT-2] a widening conversion denotes its operand's
            // mathematical value, so a relation term over one is that
            // operand's datum: `cvt::<u32, u64>(index) < spans.len` relates
            // the u32 datum to the u64 measure directly.
            ExpandedClauseExpression::Operation { row, arguments, .. }
                if super::super::goal::widening_integer_conversion(row) =>
            {
                let [argument] = arguments.as_slice() else {
                    return None;
                };
                Checker::postcondition_relation_datum(argument)
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
                        denotation,
                    } => (
                        if *denotation == ParameterDenotation::ExitState {
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

    fn collect_postcondition_binding_info(
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
                        Checker::collect_postcondition_binding_info(&arm.body, bindings);
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
                        Checker::collect_postcondition_binding_info(&arm.body, bindings);
                    }
                }
                CheckedStatement::Loop { body, .. } => {
                    Checker::collect_postcondition_binding_info(body, bindings);
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
                    Checker::collect_postcondition_binding_info(body, bindings);
                }
                _ => {}
            }
        }
    }

    fn postcondition_fragment_type(ty: CheckedType, symbolic: bool) -> bool {
        matches!(ty, CheckedType::Integer(_))
            || symbolic && matches!(ty, CheckedType::Generic(_) | CheckedType::GenericInt(_))
    }

    fn check_postcondition_candidate(
        candidate: &PostconditionCandidateRecord,
    ) -> Result<(), CheckStop> {
        if candidate
            .paired_field
            .as_ref()
            .is_some_and(|field| field == &candidate.spelling)
            || !candidate.live_conflicts.is_empty()
        {
            return Checker::issue_origin(
                SemanticRule::Fn9,
                &candidate.origin,
                SemanticIssueKind::PostconditionCandidateNotFresh {
                    spelling: candidate.spelling.clone(),
                    conflicts: candidate.live_conflicts.clone(),
                },
            );
        }
        if let Some(local) = &candidate.later_local_collision {
            return Checker::issue_origin(
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

    fn issue_origin<T>(
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

/// [FN-9, CALL-4] whether one selected return's value, projected by a
/// result datum's owned descendant projection, is inside the concrete integer
/// fragment: an integer place, literal or success payload for a value datum,
/// and a place for a measured one. A construction selects the operand of its
/// field and continues below it.
fn returned_projection_is_fragment(
    value: &PostconditionReturnDatum,
    projections: &[GoalProjection],
    measured: bool,
) -> bool {
    match value {
        PostconditionReturnDatum::Construct { fields } => {
            let Some((GoalProjection::Field(field), rest)) = projections.split_first() else {
                return false;
            };
            fields
                .get(*field as usize)
                .and_then(Option::as_ref)
                .is_some_and(|operand| returned_projection_is_fragment(operand, rest, measured))
        }
        PostconditionReturnDatum::Place(_) if measured => true,
        PostconditionReturnDatum::Place(place) => {
            projections.is_empty() && matches!(place.ty, CheckedType::Integer(_))
                || !projections.is_empty()
        }
        PostconditionReturnDatum::ResultPayload { ty } => {
            measured || !projections.is_empty() || matches!(ty, CheckedType::Integer(_))
        }
        PostconditionReturnDatum::Literal { value, .. } => {
            !measured && projections.is_empty() && matches!(value.ty(), CheckedType::Integer(_))
        }
        PostconditionReturnDatum::Measure(..) => !measured && projections.is_empty(),
    }
}

impl<'unit> DeclarationInventory<'unit> {
    /// The written selector use one clause atom contains and the type of the
    /// result datum its spelling names [CALL-4], when the atom contains one.
    fn postcondition_selector_datum(
        &self,
        check_context: &CheckContext<'_>,
        atom: NodeId,
    ) -> Result<Option<(SourceOrigin, CheckedType)>, CheckStop> {
        let Some(context) = check_context.active_postcondition else {
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
        // that ordinal's.
        let ty = Checker::active_result_datum(check_context, &usage.spelling)
            .map_or(context.result_type, |(_, ty)| ty);
        Ok(Some((usage.origin.clone(), ty)))
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
            if !self.tree.has_fixed(base, FixedTerminal::Entry)? {
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
        let Some(left) = Checker::postcondition_relation_term(left, operand_type) else {
            return self.invalid_postcondition_relation(final_expression);
        };
        let Some(right) = Checker::postcondition_relation_term(right, operand_type) else {
            return self.invalid_postcondition_relation(final_expression);
        };
        // [ENT-2] a widening conversion's operand keeps its own type: the
        // relation is over mathematical values, and the conversion's pair is
        // whole-type total, so no other operand type reaches here.
        let operand_matches = |term: &RelationTerm| {
            term.ty() == operand_type
                || matches!(
                    (
                        super::super::model::CheckedNumericType::from_type(term.ty()),
                        super::super::model::CheckedNumericType::from_type(operand_type),
                    ),
                    (Some(source), Some(destination)) if source.converts_totally_to(destination)
                )
        };
        if !operand_matches(&left) || !operand_matches(&right) {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let is_output =
            |term: &RelationTerm| term.contains_result() || term.datum.is_exit_state();
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
    fn invalid_postcondition_relation<T>(&self, expression: NodeId) -> Result<T, CheckStop> {
        self.issue_node(
            SemanticRule::Fn9,
            expression,
            SemanticIssueKind::InvalidPostconditionRelation,
        )
    }
    fn postcondition_return_constant_origin(
        &self,
        check_context: &CheckContext<'_>,
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
                let usage =
                    self.use_at(check_context, *atom, LexicalUseRole::GenericNumericSuffix)?;
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
        check_context: &CheckContext<'_>,
        node: NodeId,
    ) -> Result<bool, CheckStop> {
        let Some(context) = check_context.active_postcondition else {
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
    /// datum. The ordinary type walk checks each suffix on that result.
    pub(super) fn postcondition_selector_place_base(
        &self,
        check_context: &CheckContext<'_>,
        place: NodeId,
    ) -> Result<Option<(u32, CheckedType)>, CheckStop> {
        let Some(context) = check_context.active_postcondition else {
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
            .and_then(|usage| Checker::active_result_datum(check_context, &usage.spelling)))
    }
    fn validate_postcondition_selector(
        &self,
        record: &PostconditionResolutionRecord,
        admission: SelectorAdmissionType,
        ordinal: u32,
        success_variants: &[BuiltinPreludeId],
    ) -> Result<(), CheckStop> {
        let candidate = match record.class {
            PostconditionSelectorClass::Plain => {
                if !matches!(
                    admission,
                    SelectorAdmissionType::Fragment
                        | SelectorAdmissionType::Symbolic
                        | SelectorAdmissionType::Aggregate
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
                    SelectorAdmissionType::SuccessPayload | SelectorAdmissionType::Symbolic
                ) {
                    return self
                        .issue_selector(record, SemanticIssueKind::InvalidPostconditionSelector);
                }
                // [FN-9] the route names the success variant of the routed
                // ordinal's own type: `Ok` of a Result, `Some` of an Option.
                if !success_variants
                    .iter()
                    .any(|variant| record.variant_target == Some(ResolvedTarget::Prelude(*variant)))
                {
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
        Checker::check_postcondition_candidate(candidate)
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

impl<'unit> TypeContext<'unit> {
    /// Supplies a value-only placeholder to the ordinary expression typer.
    /// The placeholder is discarded immediately after typing; relation
    /// identity is rebuilt from the resolver-owned selector-use record and
    /// never becomes a declaration, binding, or storage location.
    pub(super) fn postcondition_result_placeholder(
        &self,
        check_context: &CheckContext<'_>,
        atom: NodeId,
    ) -> Result<Option<CheckedValue>, CheckStop> {
        let Some((origin, ty)) = self
            .declarations
            .postcondition_selector_datum(check_context, atom)?
        else {
            return Ok(None);
        };
        Ok(Some(match ty {
            CheckedType::Integer(ty) => CheckedValue::Integer { ty, bits: 0 },
            CheckedType::GenericInt(_) => CheckedValue::NumericIdentity { ty, one: false },
            // [OP-15, MSR-1] a measure is a read-only `own u64` member of the
            // measured place, so a selector atom that reads one stands for a
            // u64 whatever the measured result's own type is. [CALL-4]
            // already admits a result of measured type as an operand; the
            // placeholder has to agree with the measure's type and not with
            // the place's.
            _ if self.declarations.selector_atom_reads_a_measure(atom)? => {
                CheckedValue::Integer {
                    ty: super::super::model::IntegerType::U64,
                    bits: 0,
                }
            }
            // [CALL-4] a fragment-integer place reached from an aggregate
            // result through struct-field and `Box` content steps stands for
            // a value of that field's type.
            _ => match self.postcondition_selector_field_type(check_context, atom, ty)? {
                Some(CheckedType::Integer(ty)) => CheckedValue::Integer { ty, bits: 0 },
                Some(field @ CheckedType::GenericInt(_)) => CheckedValue::NumericIdentity {
                    ty: field,
                    one: false,
                },
                _ => {
                    return Checker::issue_origin(
                        SemanticRule::Fn9,
                        &origin,
                        SemanticIssueKind::InvalidPostconditionSelector,
                    );
                }
            },
        }))
    }
    /// [CALL-4] the type a selector atom's written member path reaches below
    /// an aggregate result datum, when every step is a struct-field
    /// selection or a `Box` `inner` step. An enum-payload step, a subscript,
    /// a dereference and a step below a value of type-parameter type reach no
    /// datum, and a bare aggregate binder is none either.
    fn postcondition_selector_field_type(
        &self,
        check_context: &CheckContext<'_>,
        atom: NodeId,
        mut ty: CheckedType,
    ) -> Result<Option<CheckedType>, CheckStop> {
        let Some(place) = self.declarations.tree.first_child_with(atom, Production::Place)? else {
            return Ok(None);
        };
        let suffixes = self
            .declarations
            .tree
            .children_with(place, Production::Psuffix)?;
        if suffixes.is_empty() {
            return Ok(None);
        }
        for suffix in suffixes {
            if self.declarations.tree.subscript_offset(suffix)?.is_some()
                || matches!(
                    self.declarations.tree.place_suffix(suffix)?,
                    crate::syntax::views::PlaceSuffix::Dereference
                )
            {
                return Ok(None);
            }
            let CheckedType::Nominal(nominal) = ty else {
                return Ok(None);
            };
            ty = match self.nominal(nominal)?.kind {
                // [TYPE-9] a Box has exactly one member, its content `inner`.
                CheckedNominalKind::Box { referent, .. } => {
                    let name = self
                        .declarations
                        .deferred_use_at(suffix, crate::DeferredUseRole::ProjectedField)?
                        .spelling();
                    if name != "inner" {
                        return Ok(None);
                    }
                    referent
                }
                CheckedNominalKind::Struct { .. } => {
                    self.resolve_struct_path(check_context, std::slice::from_ref(&suffix), ty)?
                        .1
                }
                _ => return Ok(None),
            };
        }
        Ok(Some(ty))
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
    /// The success payload one routed ordinal produces at a return, `None`
    /// for a direct failure variant, and the whole value for an ordinary
    /// forwarded Result or Option [FN-9].
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
        if self.success_payload_type(declared.ty).is_none() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let CheckedNominalKind::Enum { variants } = &self.nominal(result_nominal)?.kind else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let success = variants.iter().position(|variant| {
            matches!(
                variant.constructor,
                super::super::model::CheckedConstructor::Prelude(
                    BuiltinPreludeId::OK | BuiltinPreludeId::SOME
                )
            )
        });
        match value {
            CheckedExpression::ConstructEnum {
                nominal,
                variant,
                fields,
                ..
            } if *nominal == result_nominal => {
                if Some(*variant as usize) == success {
                    let [payload] = fields.as_slice() else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    Ok(Some(payload))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(Some(value)),
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
    /// [CALL-4] whether a value of this nominal type supplies a datum: some
    /// place reached from it through an owned descendant projection made of
    /// struct-field selections and `Box` `inner` steps [MSR-3] ends at a
    /// fragment integer or at a measured type — `result.width`,
    /// `made.storage.len`, or the `result.inner.len` every boxed
    /// construction record publishes. An enum payload step reaches none,
    /// because no route selects a variant of an unrouted result, and a value
    /// whose type is a type parameter declares no field below it.
    fn aggregate_supplies_data(
        &self,
        nominal: super::super::model::NominalId,
        symbolic: bool,
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
                if child.measured().is_some()
                    || matches!(child, CheckedType::Integer(_))
                    || symbolic && matches!(child, CheckedType::GenericInt(_))
                {
                    return Ok(true);
                }
                if let CheckedType::Nominal(inner) = child {
                    pending.push(inner);
                }
            }
        }
        Ok(false)
    }
    /// [CALL-4] whether a result of this type supplies a datum at all: a
    /// fragment integer as its own value, a measured type through its
    /// measures, or an aggregate through the places above.
    fn type_supplies_data(&self, ty: CheckedType, symbolic: bool) -> Result<bool, CheckStop> {
        Ok(match ty {
            ty if Checker::postcondition_fragment_type(ty, symbolic) => true,
            ty if super::expressions::flat_storage::measured_kind_of(ty).is_some() => true,
            CheckedType::Nominal(nominal) => {
                self.prelude_types
                    .get(nominal.0 as usize)
                    .and_then(|entry| *entry)
                    .is_none()
                    && self.aggregate_supplies_data(nominal, symbolic)?
            }
            _ => false,
        })
    }
    /// [FN-9] the success payload type of a `Result` or `Option` result type,
    /// `Ok` or `Some`, when that type is one of the two.
    fn success_payload_type(&self, ty: CheckedType) -> Option<CheckedType> {
        let CheckedType::Nominal(nominal) = ty else {
            return None;
        };
        match self
            .prelude_types
            .get(nominal.0 as usize)
            .and_then(|entry| *entry)?
        {
            super::PreludeType::Result(value, _) | super::PreludeType::Option(value) => Some(value),
            _ => None,
        }
    }
    /// Whether one declared result type can carry a route in this version:
    /// exactly `own Result<T, E>` or `own Option<T>` whose success payload T
    /// supplies data under [CALL-4] [FN-9].
    fn postcondition_route_carrier(
        &self,
        ty: CheckedType,
        symbolic: bool,
    ) -> Result<bool, CheckStop> {
        match self.success_payload_type(ty) {
            Some(payload) => self.type_supplies_data(payload, symbolic),
            None => Ok(false),
        }
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
        for id in &self.view.functions {
            let index = id.0 as usize;
            let signature = &self.signatures[index];
            if !eligible.contains(&signature.id) {
                continue;
            }
            if let Ok(path) = self.declarations.tree.path(signature.node) {
                by_function.entry(path.clone()).or_default().push(index);
            }
        }
        by_function
    }
    pub(super) fn postcondition_selectors_from_source(
        &self,
        signature: &FunctionSignature,
    ) -> Result<Vec<CheckedPostconditionSelector>, CheckStop> {
        let function = self.declarations.tree.path(signature.node)?;
        let mut selectors = Vec::new();
        for record in self
            .declarations
            .resolved
            .postconditions()
            .iter()
            .filter(|record| &record.function == function)
        {
            let selector = self.admit_postcondition_selector(
                record,
                signature,
                !signature.substitution.is_concrete(&self.elements),
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
    pub(super) fn build_checked_postcondition(
        &self,
        context: FunctionContext<'_, '_>,
        parameters: &[CheckedParameter],
        selector: CheckedPostconditionSelector,
        relation: RelationTemplate,
        body: &[CheckedStatement],
    ) -> Result<CheckedPostcondition, CheckStop> {
        let FunctionContext { function, .. } = context;
        if !function.substitution.is_concrete(&self.elements) {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        self.build_checked_postcondition_inner(context, parameters, selector, relation, body, false)
    }
    /// Builds the source-schema FN-9 handoff only when the written relation
    /// and selected returns are already in the ordinary concrete integer
    /// fragment. GenericInt remains an exact symbolic goal datum, never an L0
    /// term, so a postcondition over `T` is intentionally concrete-instance
    /// only.
    pub(super) fn build_checked_schema_postcondition(
        &self,
        context: FunctionContext<'_, '_>,
        parameters: &[CheckedParameter],
        selector: CheckedPostconditionSelector,
        relation: RelationTemplate,
        body: &[CheckedStatement],
    ) -> Result<Option<CheckedPostcondition>, CheckStop> {
        let FunctionContext { function, .. } = context;
        if function.substitution.is_concrete(&self.elements) {
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
                let ordinal = match &operand.datum {
                    RelationDatum::Measure(
                        _,
                        PostconditionPlace {
                            root: PostconditionPlaceRoot::ExitParameter { ordinal },
                            ..
                        },
                    )
                    | RelationDatum::Parameter {
                        ordinal,
                        denotation: ParameterDenotation::ExitState,
                        ..
                    } => *ordinal,
                    _ => return false,
                };
                function
                    .parameters
                    .get(ordinal as usize)
                    .is_some_and(|parameter| parameter_has_exit_state(function, parameter))
            });
        // [FN-9, CALL-4] the second admitted unrouted shape: a result ordinal
        // of a measured or aggregate type named only through the places its
        // owned descendant projections reach, and the routed shape whose
        // success payload is such a type. A measure is u64 whatever the
        // element type is and a named integer field has its declared type,
        // so such a clause is inside the concrete integer fragment exactly
        // when every operand below is. Without this a [PRE-1] construction
        // row's `ensures result.len == 0_u64` was published to no generic
        // body, and every window proof inside one started with no length at
        // all.
        let aggregate_result = selector.result_type.measured().is_some()
            || match selector.result_type {
                CheckedType::Nominal(nominal) => self.aggregate_supplies_data(nominal, false)?,
                _ => false,
            };
        if (!matches!(selector.result_type, CheckedType::Integer(_))
            && !exclusive_state_only
            && !aggregate_result)
            || relation
                .operands
                .iter()
                .any(|operand| !matches!(operand.ty(), CheckedType::Integer(_)))
        {
            return Ok(None);
        }
        // [CALL-4, FN-9] schema admission classifies only the result data
        // the relation names. An unnamed ordinal imposes no postcondition, so
        // its symbolic or affine value cannot disqualify an otherwise closed
        // integer relation over another ordinal.
        let direct_result_data = relation
            .operands
            .iter()
            .filter_map(|operand| match &operand.datum {
                RelationDatum::Result {
                    ordinal,
                    projections,
                    ..
                } => Some((*ordinal, projections.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        let measured_result_data = relation
            .operands
            .iter()
            .filter_map(|operand| match &operand.datum {
                RelationDatum::Measure(
                    _,
                    PostconditionPlace {
                        root: PostconditionPlaceRoot::Result { ordinal },
                        projections,
                        ..
                    },
                ) => Some((*ordinal, projections.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        let checked = self.build_checked_postcondition_inner(
            context, parameters, selector, relation, body, true,
        )?;
        // A state-only clause has no selected-result operand to classify.
        // Its return edges still enter the ordinary proof below; the value
        // returned on those edges (for example unit) is not evidence for it.
        let fragment_returns = exclusive_state_only
            || checked.selected_returns.iter().all(|selected| {
                direct_result_data.iter().all(|(ordinal, projections)| {
                    selected
                        .values
                        .get(*ordinal as usize)
                        .and_then(Option::as_ref)
                        .is_some_and(|value| {
                            returned_projection_is_fragment(value, projections, false)
                        })
                }) && measured_result_data.iter().all(|(ordinal, projections)| {
                    selected
                        .values
                        .get(*ordinal as usize)
                        .and_then(Option::as_ref)
                        .is_some_and(|value| {
                            returned_projection_is_fragment(value, projections, true)
                        })
                })
            });
        Ok(fragment_returns.then_some(checked))
    }
    fn build_checked_postcondition_inner(
        &self,
        context: FunctionContext<'_, '_>,
        parameters: &[CheckedParameter],
        selector: CheckedPostconditionSelector,
        relation: RelationTemplate,
        body: &[CheckedStatement],
        symbolic_schema: bool,
    ) -> Result<CheckedPostcondition, CheckStop> {
        let FunctionContext { function, .. } = context;
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
                GenericArgument::Type(ty) if ty.is_concrete(&self.elements) => {
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
        Checker::collect_postcondition_binding_info(body, &mut binding_info);

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
            context,
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
    #[allow(clippy::too_many_arguments)]
    fn collect_postcondition_returns(
        &self,
        context: FunctionContext<'_, '_>,
        selector: &CheckedPostconditionSelector,
        named: &[u32],
        statements: &[CheckedStatement],
        binding_info: &HashMap<BindingId, PostconditionBindingInfo>,
        selected: &mut Vec<SelectedPostconditionReturn>,
    ) -> Result<(), CheckStop> {
        let FunctionContext {
            check_context,
            function,
        } = context;
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
                            self.postcondition_return_datum(
                                check_context,
                                produced,
                                node_path,
                                binding_info,
                            )?
                        };
                        if datum.is_none() && named.contains(&ordinal) {
                            return self.declarations.invalid_postcondition_return(node_path);
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
                            context,
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
                        context,
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
    fn postcondition_return_datum(
        &self,
        check_context: &CheckContext<'_>,
        value: &CheckedExpression,
        statement: &crate::NodePath,
        binding_info: &HashMap<BindingId, PostconditionBindingInfo>,
    ) -> Result<Option<PostconditionReturnDatum>, CheckStop> {
        if let Some(place) = self.postcondition_return_place(value, statement, binding_info)? {
            return Ok(Some(PostconditionReturnDatum::Place(place)));
        }
        match value {
            CheckedExpression::Constant(value) => {
                let origin = self
                    .declarations
                    .postcondition_return_constant_origin(check_context, statement)?;
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
            // [FN-9, REF-4] a direct `return part^.len` is one exact
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
            // [FN-9, CALL-4] a returned construction hands back each field
            // operand, an atom [GRAM-9]; a projected result datum reads the
            // operand of the field it selects.
            CheckedExpression::ConstructStruct { fields, .. } => {
                let mut values = Vec::with_capacity(fields.len());
                for field in fields {
                    values.push(self.postcondition_return_datum(
                        check_context,
                        field,
                        statement,
                        binding_info,
                    )?);
                }
                Ok(Some(PostconditionReturnDatum::Construct { fields: values }))
            }
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
            let mut anchor = None;
            for (ordinal, (_, declared)) in
                record.result_binders.iter().zip(&signature.results).enumerate()
            {
                if self.type_supplies_data(declared.ty, symbolic)? {
                    anchor = Some(ordinal);
                    break;
                }
            }
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
                    .declarations
                    .issue_selector(record, SemanticIssueKind::InvalidPostconditionSelector);
            };
            return u32::try_from(named)
                .map_err(|_| SemanticCompilerFailure::CounterOverflow.into());
        }
        let mut carriers = Vec::new();
        for (ordinal, declared) in signature.results.iter().enumerate() {
            if self.postcondition_route_carrier(declared.ty, symbolic)? {
                carriers.push(ordinal);
            }
        }
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
                self.declarations.issue_selector(
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
            return self
                .declarations
                .issue_selector(record, SemanticIssueKind::InvalidPostconditionSelector);
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
            ty if Checker::postcondition_fragment_type(ty, symbolic) => {
                (SelectorAdmissionType::Fragment, ty)
            }
            // [FN-9] a `Result` or `Option` whose success payload supplies
            // data under [CALL-4]; its routed payload datum supplies exactly
            // the data an unrouted result ordinal of the payload type does.
            ty if self.postcondition_route_carrier(ty, symbolic)? => (
                SelectorAdmissionType::SuccessPayload,
                self.success_payload_type(ty)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?,
            ),
            // [CALL-4] a result of measured or aggregate type supplies the
            // places its owned descendant projections reach: every boxed
            // construction record's `ensures result.inner.len`, a
            // constructor's `ensures made.storage.len`, and an identifier's
            // `ensures atom.index < table^.spans.inner.len`.
            ty if self.type_supplies_data(ty, symbolic)? => (SelectorAdmissionType::Aggregate, ty),
            CheckedType::Generic(_) if symbolic => (SelectorAdmissionType::Symbolic, declared.ty),
            _ => (SelectorAdmissionType::Invalid, declared.ty),
        };
        // [FN-9] the success variant a route may name: the declared type's
        // own for a Result or an Option, and either for a symbolic ordinal
        // whose enum is not yet known.
        let success_variants: &[BuiltinPreludeId] = match self
            .success_payload_type(declared.ty)
            .and(match declared.ty {
                CheckedType::Nominal(nominal) => self
                    .prelude_types
                    .get(nominal.0 as usize)
                    .and_then(|entry| *entry),
                _ => None,
            }) {
            Some(super::PreludeType::Option(_)) => &[BuiltinPreludeId::SOME],
            Some(_) => &[BuiltinPreludeId::OK],
            None => &[BuiltinPreludeId::OK, BuiltinPreludeId::SOME],
        };
        self.declarations.validate_postcondition_selector(
            record,
            if state_only {
                SelectorAdmissionType::Symbolic
            } else {
                admission
            },
            ordinal,
            success_variants,
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
                        declaration: if variant == BuiltinPreludeId::SOME {
                            BuiltinPreludeId::SOME_VALUE
                        } else {
                            BuiltinPreludeId::OK_VALUE
                        },
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
}
