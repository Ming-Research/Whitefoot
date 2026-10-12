//! Proof-only reads of callable contracts [EFF-5]. Formation reuses the
//! ordinary typed FN-8/FN-9/RANGE-1 clauses and TYPE-11 templates; no body,
//! runtime effect, reference invalidation or caller fact depends on these.

use super::requires::{ExpandedClauseDatum, ExpandedClauseExpression};
use super::*;
use crate::semantic::model::{CheckedEffectStep, CheckedMeasure};
use crate::semantic::places::{CaptureId, CapturedTerm, CapturedValue, PlaceStep};
use crate::semantic::range_facts::{
    CheckedRangeClause, CheckedRangePlace, CheckedRangeProjection, CheckedRangeRoot,
    CheckedRangeStep, CheckedRangeTerm,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ContractReadPath {
    pub(super) root: DeclarationId,
    pub(super) steps: Vec<ContractReadStep>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ContractReadStep {
    /// Row selectors retain the same parameter substitution as EFF-1 paths.
    Row(CheckedEffectStep),
    /// Literal and const positions already have a captured value.
    Captured(PlaceStep),
    /// A finite typed range position, kept intact until caller substitution.
    Computed(CheckedRangeTerm),
}

impl Checker<'_, '_> {
    /// All currently available signatures precede body checking, so forward
    /// and recursive calls see the same footprint. Operand-directed and
    /// formal signatures formed later use `contract_read_footprint` at a call.
    pub(super) fn collect_contract_reads(
        &mut self,
        check_context: &CheckContext<'_>,
    ) -> Result<(), CheckStop> {
        let mut cursor = 0;
        while cursor < self.types.view.functions.len() {
            let id = self.types.view.functions[cursor];
            let signature = self.types.signatures[id.0 as usize].clone();
            let reads = self.contract_read_footprint(check_context, &signature)?;
            self.types.signatures[id.0 as usize].contract_reads = Some(reads);
            cursor += 1;
        }
        Ok(())
    }

    pub(super) fn contract_read_footprint(
        &mut self,
        check_context: &CheckContext<'_>,
        signature: &FunctionSignature,
    ) -> Result<Vec<ContractReadPath>, CheckStop> {
        if let Some(reads) = &signature.contract_reads {
            return Ok(reads.clone());
        }
        // Contract formation owns ordinary body scratch (captures, anchors,
        // range clauses). Give it a separate frame, restoring the caller's
        // scratch on every result, including a source or capability failure.
        let saved = std::mem::take(self.body);
        let result = self.form_contract_read_footprint(check_context, signature);
        *self.body = saved;
        result
    }

    fn form_contract_read_footprint(
        &mut self,
        check_context: &CheckContext<'_>,
        signature: &FunctionSignature,
    ) -> Result<Vec<ContractReadPath>, CheckStop> {
        let check_context = CheckContext {
            writing_module: self.types.declarations.resolved.declaration(signature.declaration)
                .and_then(crate::DeclarationRecord::module),
            ..*check_context
        };
        let context = FunctionContext { check_context: &check_context, function: signature };
        let mut reads = Vec::new();
        let mut bindings = HashMap::new();
        let mut parameters = Vec::new();
        for (ordinal, parameter) in signature.parameters.iter().enumerate() {
            let binding = BindingId(u32::try_from(ordinal)
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?);
            let local = Checker::parameter_local(parameter, binding)?;
            if let Some(reference) = &local.reference {
                self.body.record_reference_origins(binding, &reference.paths);
                self.body.note_anchor(binding, parameter.declaration);
            }
            bindings.insert(parameter.declaration, local);
            parameters.push(CheckedParameter {
                name: parameter.name.clone(), declaration: parameter.declaration,
                node_path: parameter.node_path.clone(), binding, mode: parameter.mode,
                ty: parameter.ty, range_element: parameter.mode.is_range()
                    .then(|| self.types.intern_element(parameter.ty)).transpose()?,
            });
        }
        let mut next_binding = u32::try_from(parameters.len())
            .map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
        let mut next_loop = 0;
        let mut binding_names = parameters.iter().map(|p| p.name.clone()).collect();
        let mut counters = ControlCounters {
            next_binding: &mut next_binding, next_loop: &mut next_loop,
            binding_names: &mut binding_names,
        };
        let unsupplied_window = self.types.has_unsupplied_window_type_parameter(signature)?;
        if let Some(block) = self.types.declarations.tree
            .first_child_with(signature.node, Production::ContractBlock)?
            .filter(|_| !unsupplied_window)
        {
            let checked = self.check_requires(context, block, &mut bindings.clone(), &mut counters)?;
            for requirement in checked.requirements {
                collect_goal(&requirement.template.root, signature, &mut reads);
            }
            for clause in checked.range { collect_range_clause(&clause, signature, &mut reads); }
            let selectors = self.postcondition_selectors_for_signature(signature)?;
            for postcondition in self.check_range_postconditions(context, &selectors, &bindings)? {
                collect_range_clause(&postcondition.clause, signature, &mut reads);
            }
            for selector in self.types.declarations.relation_postcondition_selectors(selectors)? {
                let expression = self.expand_postcondition_clause(
                    context, &selector, &mut bindings.clone(), &mut counters)?;
                collect_expanded(&expression, signature, &mut reads);
            }
        }
        // Both entry and published exit/result invariant clauses use the
        // existing TYPE-11 substitution. Owned result roots supply no
        // reference argument and therefore add no compared read.
        for requirement in self.implicit_type_invariant_requirements(&check_context, signature)? {
            collect_goal(&requirement.template.root, signature, &mut reads);
        }
        self.append_range_type_invariant_contracts(&check_context, signature, &parameters)?;
        for clause in &self.body.range_facts.requirements {
            collect_range_clause(clause, signature, &mut reads);
        }
        for postcondition in &self.body.range_facts.postconditions {
            collect_range_clause(&postcondition.clause, signature, &mut reads);
        }
        Ok(reads)
    }
}

fn add_read(signature: &FunctionSignature, ordinal: u32, steps: Vec<ContractReadStep>,
    reads: &mut Vec<ContractReadPath>) {
    let Some(parameter) = signature.parameters.get(ordinal as usize)
        .filter(|parameter| parameter.mode.is_reference()) else { return; };
    let read = ContractReadPath { root: parameter.declaration, steps };
    if !reads.contains(&read) { reads.push(read); }
}

fn ordinary_read(signature: &FunctionSignature, ordinal: u32, projections: &[GoalProjection],
    reads: &mut Vec<ContractReadPath>) {
    let Some(parameter) = signature.parameters.get(ordinal as usize)
        .filter(|parameter| parameter.mode.is_reference()) else { return; };
    // The formal's first caret denotes the referent the actual already
    // names; further Deref projections denote owned Box contents.
    let projections = if matches!(projections.first(), Some(GoalProjection::Deref)) {
        &projections[1..]
    } else {
        // A bare reference holder supplies no observation of its referent.
        let _ = parameter;
        return;
    };
    let steps = projections.iter().map(|projection| match *projection {
        GoalProjection::Deref => ContractReadStep::Row(CheckedEffectStep::Deref),
        GoalProjection::Field(field) => ContractReadStep::Row(CheckedEffectStep::Field(field)),
        GoalProjection::Payload { variant, field } =>
            ContractReadStep::Row(CheckedEffectStep::Payload { variant, field }),
        GoalProjection::Measure(measure) => ContractReadStep::Row(CheckedEffectStep::Measure(measure)),
        GoalProjection::FormalSubscript { ordinal } => ContractReadStep::Row(
            CheckedEffectStep::Index(signature.parameters[ordinal as usize].declaration)),
        GoalProjection::FormalPage { ordinal } => ContractReadStep::Row(
            CheckedEffectStep::Page(signature.parameters[ordinal as usize].declaration)),
        projection => ContractReadStep::Captured(projection.place_step()),
    }).collect();
    add_read(signature, ordinal, steps, reads);
}

fn collect_goal(expression: &GoalExpression, signature: &FunctionSignature,
    reads: &mut Vec<ContractReadPath>) {
    match expression {
        GoalExpression::Datum(GoalDatum::Parameter { ordinal, projections, .. }) =>
            ordinary_read(signature, *ordinal, projections, reads),
        GoalExpression::Operation { arguments, .. } => {
            for argument in arguments { collect_goal(argument, signature, reads); }
        }
        _ => {}
    }
}

fn collect_expanded(expression: &ExpandedClauseExpression, signature: &FunctionSignature,
    reads: &mut Vec<ContractReadPath>) {
    match expression {
        ExpandedClauseExpression::Datum(ExpandedClauseDatum::Parameter { ordinal, projections, .. }) =>
            ordinary_read(signature, *ordinal, projections, reads),
        ExpandedClauseExpression::Operation { arguments, .. } => {
            for argument in arguments { collect_expanded(argument, signature, reads); }
        }
        _ => {}
    }
}

fn collect_range_clause(clause: &CheckedRangeClause, signature: &FunctionSignature,
    reads: &mut Vec<ContractReadPath>) {
    for read in &clause.contract_reads {
        collect_range_term(read, signature, reads);
    }
    for binder in &clause.binders {
        collect_range_term(&binder.start, signature, reads);
        collect_range_term(&binder.end, signature, reads);
    }
    for relation in clause.guards.iter().chain(&clause.conclusions) {
        collect_range_term(&relation.left, signature, reads);
        collect_range_term(&relation.right, signature, reads);
    }
}

fn range_base(place: &CheckedRangePlace) -> Option<(u32, Vec<ContractReadStep>)> {
    let CheckedRangeRoot::Binding(binding) = place.root else { return None; };
    let steps = place.path.iter().filter_map(|step| match step {
        CheckedRangeStep::Referent => None,
        CheckedRangeStep::Field(field) => Some(ContractReadStep::Row(CheckedEffectStep::Field(*field))),
        CheckedRangeStep::BoxContent => Some(ContractReadStep::Row(CheckedEffectStep::Deref)),
    }).collect();
    Some((binding.0, steps))
}

fn has_bound(term: &CheckedRangeTerm) -> bool {
    match term {
        CheckedRangeTerm::Bound(_) => true,
        CheckedRangeTerm::Sum { terms, .. } => terms.iter().any(|(_, term)| has_bound(term)),
        CheckedRangeTerm::Read { indices, .. }
        | CheckedRangeTerm::ValueProjection { indices, .. } => indices.iter().any(has_bound),
        CheckedRangeTerm::SegmentLength { segment, .. } => has_bound(segment),
        _ => false,
    }
}

fn range_position(term: &CheckedRangeTerm, signature: &FunctionSignature) -> ContractReadStep {
    let captured = match term {
        CheckedRangeTerm::Constant(value) => u64::try_from(*value).ok()
            .map(CapturedTerm::Literal),
        CheckedRangeTerm::ConstGeneric { declaration, .. } => Some(CapturedTerm::Const(*declaration)),
        CheckedRangeTerm::Value(CheckedRangeRoot::Binding(binding)) => {
            if let Some(parameter) = signature.parameters.get(binding.0 as usize) {
                return ContractReadStep::Row(CheckedEffectStep::Index(parameter.declaration));
            }
            None
        }
        _ => None,
    };
    match captured {
        Some(term) => ContractReadStep::Captured(PlaceStep::Index(
            CapturedValue::new(CaptureId::ValueDetermined, term))),
        None => ContractReadStep::Computed(term.clone()),
    }
}

fn collect_range_term(term: &CheckedRangeTerm, signature: &FunctionSignature,
    reads: &mut Vec<ContractReadPath>) {
    match term {
        CheckedRangeTerm::Sum { terms, .. } => {
            for (_, term) in terms { collect_range_term(term, signature, reads); }
        }
        CheckedRangeTerm::Measure { place, measure, .. } => {
            if let Some((ordinal, mut steps)) = range_base(place) {
                steps.push(ContractReadStep::Row(CheckedEffectStep::Measure(*measure)));
                add_read(signature, ordinal, steps, reads);
            }
        }
        CheckedRangeTerm::SegmentLength { place, segment } => {
            collect_range_term(segment, signature, reads);
            if let Some((ordinal, mut steps)) = range_base(place) {
                if !has_bound(segment) {
                    steps.push(range_position(segment, signature));
                    steps.push(ContractReadStep::Row(CheckedEffectStep::Measure(CheckedMeasure::Length)));
                }
                add_read(signature, ordinal, steps, reads);
            }
        }
        CheckedRangeTerm::Read { place, indices, projection, .. } => {
            for index in indices { collect_range_term(index, signature, reads); }
            if let Some((ordinal, mut steps)) = range_base(place) {
                let mut widened = false;
                // A run has one outer index; Segments has two. Remaining
                // indices occur in the owned projection below that element.
                let nested = projection.iter().filter(|p| matches!(p, CheckedRangeProjection::Index(_))).count();
                for index in indices.iter().take(indices.len().saturating_sub(nested)) {
                    if has_bound(index) { widened = true; break; }
                    steps.push(range_position(index, signature));
                }
                if !widened {
                    for projection in projection {
                        match *projection {
                            CheckedRangeProjection::Index(index) => {
                                let term = &indices[index as usize];
                                if has_bound(term) { break; }
                                steps.push(range_position(term, signature));
                            }
                            CheckedRangeProjection::Field(field) => steps.push(ContractReadStep::Row(CheckedEffectStep::Field(field))),
                            CheckedRangeProjection::BoxContent => steps.push(ContractReadStep::Row(CheckedEffectStep::Deref)),
                            CheckedRangeProjection::Measure(measure) => steps.push(ContractReadStep::Row(CheckedEffectStep::Measure(measure))),
                            CheckedRangeProjection::Payload { variant, field, .. } => steps.push(ContractReadStep::Row(CheckedEffectStep::Payload { variant, field })),
                            CheckedRangeProjection::Tag(_) => break,
                        }
                    }
                }
                add_read(signature, ordinal, steps, reads);
            }
        }
        _ => {}
    }
}

/// The fixed arithmetic image of an erased range position, retaining the
/// existing goal identities. This is proof data, never an evaluated operand.
pub(super) fn range_term_goal(term: &CheckedRangeTerm, signature: &FunctionSignature,
    call: u32, position: &mut u32, pending: &mut Vec<(CapturedValue, GoalExpression)>)
    -> Option<GoalExpression> {
    use crate::semantic::model::{CheckedIntegerOperation as Op, IntegerType};
    let integer = CheckedType::Integer(IntegerType::U64);
    let operation = |operation, arguments| GoalExpression::Operation {
        row: GoalOperation::Integer { operation, operand_type: integer },
        type_arguments: Vec::new(), const_arguments: Vec::new(), result: integer, arguments,
    };
    let datum = |root: CheckedRangeRoot, mut projections: Vec<GoalProjection>, ty| {
        let CheckedRangeRoot::Binding(binding) = root else { return None; };
        let parameter = signature.parameters.get(binding.0 as usize)?;
        if parameter.mode.is_reference()
            && !matches!(projections.first(), Some(GoalProjection::Deref)) {
            projections.insert(0, GoalProjection::Deref);
        }
        Some(GoalExpression::Datum(GoalDatum::Parameter {
            ordinal: binding.0, projections, ty,
        }))
    };
    match term {
        CheckedRangeTerm::Constant(value) => Some(range_constant_goal(*value)),
        CheckedRangeTerm::ConstGeneric { declaration, ty } => Some(GoalExpression::Datum(
            GoalDatum::Literal(CheckedValue::ConstGeneric { declaration: *declaration, ty: *ty }))),
        CheckedRangeTerm::Value(root) => {
            let CheckedRangeRoot::Binding(binding) = root else { return None; };
            datum(*root, Vec::new(), signature.parameters.get(binding.0 as usize)?.ty)
        }
        CheckedRangeTerm::ValueProjection { root, indices, projection: suffix, element } => {
            let mut path = Vec::new();
            let nested = suffix.iter().filter(|p| matches!(p, CheckedRangeProjection::Index(_))).count();
            for index in indices.iter().take(indices.len().checked_sub(nested)?) {
                path.push(range_index_projection(index, signature, call, position, pending)?);
            }
            for step in suffix { path.push(range_owned_projection(step, indices, signature, call, position, pending)?); }
            datum(*root, path, CheckedType::Integer(*element))
        }
        CheckedRangeTerm::Measure { place, measure, .. } => {
            let mut path = range_goal_path(place);
            path.push(GoalProjection::Measure(*measure));
            datum(place.root, path, integer)
        }
        CheckedRangeTerm::SegmentLength { place, segment } => {
            let mut path = range_goal_path(place);
            path.push(range_index_projection(segment, signature, call, position, pending)?);
            path.push(GoalProjection::Measure(CheckedMeasure::Length));
            datum(place.root, path, integer)
        }
        CheckedRangeTerm::Read { place, indices, projection: suffix, element, .. } => {
            let mut path = range_goal_path(place);
            let nested = suffix.iter().filter(|p| matches!(p, CheckedRangeProjection::Index(_))).count();
            for index in indices.iter().take(indices.len().checked_sub(nested)?) {
                path.push(range_index_projection(index, signature, call, position, pending)?);
            }
            for step in suffix { path.push(range_owned_projection(step, indices, signature, call, position, pending)?); }
            datum(place.root, path, CheckedType::Integer(*element))
        }
        CheckedRangeTerm::Sum { terms, constant: offset } => {
            let mut value = range_constant_goal(*offset);
            for (coefficient, term) in terms {
                let term = range_term_goal(term, signature, call, position, pending)?;
                let weighted = if *coefficient == 1 { term }
                    else { operation(Op::MultiplyExact, vec![range_constant_goal(*coefficient), term]) };
                value = operation(Op::AddExact, vec![value, weighted]);
            }
            Some(value)
        }
        CheckedRangeTerm::Bound(_) | CheckedRangeTerm::Iteration(_) => None,
    }
}

fn range_goal_path(place: &CheckedRangePlace) -> Vec<GoalProjection> {
    place.path.iter().map(|step| match *step {
        CheckedRangeStep::Referent | CheckedRangeStep::BoxContent => GoalProjection::Deref,
        CheckedRangeStep::Field(field) => GoalProjection::Field(field),
    }).collect()
}

fn range_index_projection(term: &CheckedRangeTerm, signature: &FunctionSignature,
    call: u32, position: &mut u32, pending: &mut Vec<(CapturedValue, GoalExpression)>)
    -> Option<GoalProjection> {
    Some(match term {
        CheckedRangeTerm::Value(CheckedRangeRoot::Binding(binding)) =>
            GoalProjection::FormalSubscript { ordinal: binding.0 },
        CheckedRangeTerm::Constant(value) if u64::try_from(*value).is_ok() =>
            GoalProjection::Subscript(CapturedValue::new(CaptureId::ValueDetermined,
                CapturedTerm::Literal(*value as u64))),
        CheckedRangeTerm::ConstGeneric { declaration, .. } => GoalProjection::Subscript(
            CapturedValue::new(CaptureId::ValueDetermined, CapturedTerm::Const(*declaration))),
        other => {
            let goal = range_term_goal(other, signature, call, position, pending)?;
            let captured = CapturedValue::new(CaptureId::ContractRead {
                call, position: *position,
            }, CapturedTerm::Opaque);
            *position = position.checked_add(1)?;
            pending.push((captured, goal));
            GoalProjection::Subscript(captured)
        }
    })
}

fn range_owned_projection(step: &CheckedRangeProjection, indices: &[CheckedRangeTerm],
    signature: &FunctionSignature, call: u32, position: &mut u32,
    pending: &mut Vec<(CapturedValue, GoalExpression)>) -> Option<GoalProjection> {
    Some(match *step {
        CheckedRangeProjection::Field(field) => GoalProjection::Field(field),
        CheckedRangeProjection::BoxContent => GoalProjection::Deref,
        CheckedRangeProjection::Measure(measure) => GoalProjection::Measure(measure),
        CheckedRangeProjection::Payload { variant, field, .. } =>
            GoalProjection::Payload { variant, field },
        CheckedRangeProjection::Index(index) => range_index_projection(
            &indices[index as usize], signature, call, position, pending)?,
        CheckedRangeProjection::Tag(_) => return None,
    })
}

/// Mathematical constants may exceed any source integer type. Express their
/// existing i128 value through erased mathematical arithmetic over u64 literals,
/// without evaluating it or introducing a target operation.
fn range_constant_goal(value: i128) -> GoalExpression {
    use crate::semantic::model::CheckedIntegerOperation as Op;
    let literal = |bits| GoalExpression::Datum(GoalDatum::Literal(
        CheckedValue::Integer { ty: crate::semantic::model::IntegerType::U64, bits }));
    let operation = |operation, arguments| GoalExpression::Operation {
        row: GoalOperation::Integer { operation,
            operand_type: CheckedType::Integer(crate::semantic::model::IntegerType::U64) },
        type_arguments: Vec::new(), const_arguments: Vec::new(),
        result: CheckedType::Integer(crate::semantic::model::IntegerType::U64), arguments,
    };
    // Negative values are -(abs(value + 1)) - 1: even i128::MIN has
    // no intermediate positive value beyond the checker's i128 arithmetic.
    let magnitude = if value < 0 { (value + 1).unsigned_abs() } else { value as u128 };
    let high = (magnitude >> 64) as u64;
    let low = magnitude as u64;
    let positive = if high == 0 { literal(low) } else {
        let radix = operation(Op::AddExact, vec![literal(u64::MAX), literal(1)]);
        operation(Op::AddExact, vec![
            operation(Op::MultiplyExact, vec![literal(high), radix]), literal(low)])
    };
    if value < 0 {
        operation(Op::SubtractExact, vec![
            operation(Op::SubtractExact, vec![literal(0), positive]), literal(1)])
    } else { positive }
}

/// A result has no actual or established fact at the call's entry. Keep such
/// a position as an opaque index; the ordinary overlap judgment still proves
/// structural separation and asks no fabricated result fact.
pub(super) fn has_result(term: &CheckedRangeTerm) -> bool {
    match term {
        CheckedRangeTerm::Value(root) | CheckedRangeTerm::ValueProjection { root, .. }
            if matches!(root, CheckedRangeRoot::Result(_)) => true,
        CheckedRangeTerm::Measure { place, .. } => matches!(place.root, CheckedRangeRoot::Result(_)),
        CheckedRangeTerm::SegmentLength { place, segment } =>
            matches!(place.root, CheckedRangeRoot::Result(_)) || has_result(segment),
        CheckedRangeTerm::Read { place, indices, .. } =>
            matches!(place.root, CheckedRangeRoot::Result(_)) || indices.iter().any(has_result),
        CheckedRangeTerm::ValueProjection { indices, .. } => indices.iter().any(has_result),
        CheckedRangeTerm::Sum { terms, .. } => terms.iter().any(|(_, term)| has_result(term)),
        _ => false,
    }
}
