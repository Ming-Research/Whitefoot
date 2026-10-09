//! RANGE-2 goal selection uses the terms at the source site, before ordinary
//! entailment expands a live binding into its initializer's goal identity.
use std::collections::BTreeSet;

use crate::NodePath;

mod calls;

use super::super::model::*;
use super::super::places::{PlaceMap, PlaceRoot};

pub(super) fn excluded_sites<'a>(
    function: &CheckedFunction,
    functions: &'a [&'a CheckedFunction],
    nominals: &'a [CheckedNominal],
    elements: &'a [CheckedType],
) -> Selection<'a> {
    let mut selection = Selection {
        excluded: BTreeSet::new(),
        requirements: BTreeSet::new(),
        functions,
        nominals,
        elements,
        places: PlaceMap::for_function(function),
    };
    for place in function.requirement_places.iter().flatten() {
        selection.expression(place);
    }
    selection.statements(function.body.as_deref().unwrap_or_default());
    selection
}

pub(super) struct Selection<'a> {
    excluded: BTreeSet<NodePath>,
    requirements: BTreeSet<(NodePath, NodePath, Option<u32>)>,
    functions: &'a [&'a CheckedFunction],
    nominals: &'a [CheckedNominal],
    elements: &'a [CheckedType],
    places: PlaceMap,
}

impl Selection<'_> {
    pub(super) fn excludes(&self, record: &super::super::obligations::ObligationRecord) -> bool {
        self.excluded.contains(&record.site)
            || match &record.subject {
                super::super::obligations::ObligationSubject::CallRequirement {
                    requires_clause,
                    subject,
                    ..
                } => self.requirements.contains(&(
                    record.site.clone(),
                    requires_clause.clone(),
                    *subject,
                )),
                _ => false,
            }
    }

    fn goal(&mut self, site: &NodePath, admitted: bool) {
        if !admitted {
            self.excluded.insert(site.clone());
        }
    }

    fn path(&mut self, path: &[CheckedPlaceStep], mut admitted: bool, mut below_element: bool) {
        for step in path {
            if let CheckedPlaceStep::Subscript(index) = step {
                admitted &= term(&index.offset);
                self.goal(&index.obligation, admitted);
                admitted &= element_base(index.base_type, below_element);
                below_element = true;
            }
        }
    }

    fn element(&mut self, place: &CheckedRangeElementPlace) {
        let admitted = term(&place.offset);
        self.goal(&place.obligation, admitted);
        self.path(&place.path, admitted, true);
    }

    fn affine(&mut self, relation: &CheckedAffineRelation) {
        let mut admitted = true;
        for value in relation.left.postorder().chain(relation.right.postorder()) {
            if let CheckedAffineExpressionKind::Measure(measure) = &value.kind {
                admitted &= term(measure);
                self.expression(measure);
            }
        }
        self.goal(&relation.node_path, admitted);
    }

    fn expression(&mut self, expression: &CheckedExpression) {
        match expression {
            CheckedExpression::UserCall {
                function,
                call,
                arguments,
                formal_contract,
                requirements,
                ..
            } => {
                // Substitution erases intermediate types and formal-index
                // dependencies. Read the template with its actuals before
                // those details are lost; each requirement stays independent.
                if let Some(callee) = self.functions.get(function.0 as usize) {
                    let boundary = formal_contract
                        .as_ref()
                        .map_or(callee.requirements.as_slice(), |contract| {
                            contract.requirements.as_slice()
                        });
                    let terms = calls::Terms::new(
                        &callee.parameters,
                        arguments,
                        self.nominals,
                        self.elements,
                        &self.places,
                    );
                    for (template, requirement) in boundary.iter().zip(requirements) {
                        if !terms.goal(&template.template.root) {
                            self.requirements.insert((
                                call.clone(),
                                requirement.requires_clause.clone(),
                                requirement.subject,
                            ));
                        }
                    }
                }
            }
            CheckedExpression::IntegerOperation {
                carrier,
                operation,
                operand_type,
                arguments,
                ..
            } if operation.is_exact() => {
                self.goal(carrier, domain(*operation, *operand_type, arguments));
            }
            CheckedExpression::NumericConversion {
                carrier,
                mode: CheckedConversionMode::Exact,
                source,
                destination,
                value,
                ..
            } => {
                self.goal(
                    carrier,
                    matches!(
                        (source, destination),
                        (
                            CheckedNumericType::Integer(_) | CheckedNumericType::GenericInteger(_),
                            CheckedNumericType::Integer(_) | CheckedNumericType::GenericInteger(_)
                        )
                    ) && term(value),
                );
            }
            CheckedExpression::ArrayIndex {
                obligation, offset, ..
            } => {
                self.goal(obligation, term(offset));
            }
            CheckedExpression::BufferIndex {
                root,
                obligation,
                offset,
                ..
            } => {
                self.path(&root.path, true, false);
                self.goal(obligation, term(offset) && range_path(&root.path, false));
                for offset in offsets(&root.path) {
                    self.expression(offset);
                }
            }
            CheckedExpression::ContainerMeasure { root, .. }
            | CheckedExpression::ReadStorage { root, .. }
            | CheckedExpression::BorrowAddressed { root, .. } => self.path(&root.path, true, false),
            CheckedExpression::BufferMeasure { root, .. } => {
                self.path(&root.path, true, false);
                for offset in offsets(&root.path) {
                    self.expression(offset);
                }
            }
            CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. }
            | CheckedExpression::RangeElementMeasure { place, .. } => self.element(place),
            CheckedExpression::RangeOf { source, .. } => match source {
                CheckedRangeSource::Storage(root) => self.path(&root.path, true, false),
                CheckedRangeSource::Element(place) => self.element(place),
                CheckedRangeSource::Range(_) => {}
            },
            CheckedExpression::BorrowSegment { root, segment, .. } => {
                match root {
                    CheckedSegmentSource::Storage(root) => self.path(&root.path, true, false),
                    CheckedSegmentSource::Element(place) => self.element(place),
                }
                if let CheckedSegmentSelect::One(index) | CheckedSegmentSelect::Page(index) =
                    segment
                {
                    self.goal(
                        &index.obligation,
                        term(&index.offset) && root.offsets().all(term),
                    );
                }
            }
            _ => {}
        }
        for child in expression_children(expression) {
            self.expression(child);
        }
    }

    fn statements(&mut self, statements: &[CheckedStatement]) {
        for statement in statements {
            match statement {
                CheckedStatement::Let { value, .. }
                | CheckedStatement::DestructuringLet { value, .. }
                | CheckedStatement::Evaluate { value, .. }
                | CheckedStatement::DropExpression { value, .. }
                | CheckedStatement::Return { value, .. }
                | CheckedStatement::Give { value, .. } => self.expression(value),
                CheckedStatement::Set { target, value, .. } => {
                    match target {
                        CheckedSetTarget::Place(_) => {}
                        CheckedSetTarget::RangeIndex(place) => {
                            self.element(place);
                            for offset in place.offsets() {
                                self.expression(offset);
                            }
                        }
                        CheckedSetTarget::Storage(root) => {
                            self.path(&root.path, true, false);
                            for offset in root.offsets() {
                                self.expression(offset);
                            }
                        }
                    }
                    self.expression(value);
                }
                CheckedStatement::PropagateLet { scrutinee, .. } => self.expression(scrutinee),
                CheckedStatement::Match {
                    scrutinee, arms, ..
                }
                | CheckedStatement::ValueMatchLet {
                    scrutinee, arms, ..
                } => {
                    self.expression(scrutinee);
                    for arm in arms {
                        self.statements(&arm.body);
                    }
                }
                CheckedStatement::CountedRange {
                    lower,
                    upper,
                    invariants,
                    body,
                    ..
                } => {
                    self.expression(lower);
                    self.expression(upper);
                    for invariant in invariants {
                        self.affine(&invariant.relation);
                    }
                    self.statements(body);
                }
                CheckedStatement::Loop {
                    invariants, body, ..
                } => {
                    for invariant in invariants {
                        self.affine(&invariant.relation);
                    }
                    self.statements(body);
                }
                CheckedStatement::Proof(proof) => {
                    self.affine(&proof.target);
                    // The obligation's site is the statement, not its target.
                    if self.excluded.contains(&proof.target.node_path) {
                        self.excluded.insert(proof.node_path.clone());
                    }
                }
                CheckedStatement::Atomic {
                    targets,
                    guard,
                    body,
                    ..
                } => {
                    for target in targets
                        .iter()
                        .flat_map(crate::semantic::CheckedTarget::expressions)
                    {
                        self.expression(target);
                    }
                    if let Some(guard) = guard {
                        self.expression(guard);
                    }
                    self.statements(body);
                }
                CheckedStatement::Break { .. } | CheckedStatement::Continue { .. } => {}
            }
        }
    }
}

fn constant(value: &CheckedExpression) -> bool {
    matches!(
        value,
        CheckedExpression::Constant(CheckedValue::Integer { .. })
            | CheckedExpression::NamedConstant {
                value: CheckedValue::Integer { .. },
                ..
            }
    )
}

fn domain(
    operation: CheckedIntegerOperation,
    ty: CheckedType,
    arguments: &[CheckedExpression],
) -> bool {
    use CheckedIntegerOperation as Op;
    match (operation, arguments) {
        (Op::AddExact | Op::SubtractExact, [left, right]) => term(left) && term(right),
        (Op::MultiplyExact, [left, right]) => {
            term(left) && term(right) && (constant(left) || constant(right))
        }
        (Op::DivideExact | Op::RemainderExact, [left, right]) => {
            // With a constant operand the signed corner exclusion reduces
            // to a comparison or true. Two variable operands need a disjunction.
            if matches!(ty, CheckedType::Integer(ty) if !ty.signed()) {
                term(right)
            } else {
                term(left) && term(right) && (constant(left) || constant(right))
            }
        }
        (Op::NegateExact | Op::AbsoluteExact, [value]) => term(value),
        (Op::ShiftLeftExact | Op::ShiftRightExact, [_, amount]) => term(amount),
        _ => false,
    }
}

fn element(expression: &CheckedExpression) -> bool {
    match expression {
        CheckedExpression::ArrayIndex { root, offset, .. } => {
            matches!(root, CheckedArrayRoot::Binding { .. }) && term(offset)
        }
        CheckedExpression::BufferIndex { root, offset, .. } => {
            term(offset) && range_path(&root.path, false)
        }
        CheckedExpression::RangeIndex { place, .. } => {
            term(&place.offset) && range_path(&place.path, true)
        }
        CheckedExpression::ReadStorage { root, .. } => {
            matches!(root.root, PlaceRoot::Binding(_))
                && root
                    .path
                    .iter()
                    .any(|step| matches!(step, CheckedPlaceStep::Subscript(_)))
                && range_path(&root.path, false)
        }
        CheckedExpression::ProjectValue { value, .. }
        | CheckedExpression::BoxDeref { value, .. } => element(value),
        _ => false,
    }
}

/// The written RANGE-1 terms. Binding initializers are deliberately irrelevant:
/// a scalar binding is a term, and a scalar binding is not a literal coefficient.
pub(super) fn term(expression: &CheckedExpression) -> bool {
    if !matches!(
        expression.ty(),
        CheckedType::Integer(_) | CheckedType::GenericInt(_)
    ) {
        return false;
    }
    match expression {
        CheckedExpression::Constant(_)
        | CheckedExpression::NamedConstant { .. }
        | CheckedExpression::Binding { .. } => true,
        CheckedExpression::ArrayMeasure { measure, root, .. } => {
            *measure != CheckedMeasure::Head && matches!(root, CheckedArrayRoot::Binding { .. })
        }
        CheckedExpression::RangeMeasure { measure, .. } => *measure != CheckedMeasure::Head,
        CheckedExpression::BufferMeasure { measure, root } => {
            *measure != CheckedMeasure::Head && range_path(&root.path, false)
        }
        CheckedExpression::ContainerMeasure { measure, root } => {
            *measure != CheckedMeasure::Head
                && matches!(root.root, PlaceRoot::Binding(_))
                && range_path(&root.path, false)
        }
        CheckedExpression::RangeElementMeasure { measure, place, .. } => {
            *measure != CheckedMeasure::Head && term(&place.offset) && range_path(&place.path, true)
        }
        CheckedExpression::IntegerOperation {
            operation,
            arguments,
            ..
        } => {
            matches!(
                operation,
                CheckedIntegerOperation::AddExact
                    | CheckedIntegerOperation::SubtractExact
                    | CheckedIntegerOperation::MultiplyExact
            ) && domain(*operation, expression.ty(), arguments)
        }
        _ => element(expression),
    }
}

fn offsets(path: &[CheckedPlaceStep]) -> impl Iterator<Item = &CheckedExpression> {
    path.iter().filter_map(|step| match step {
        CheckedPlaceStep::Subscript(index) => Some(&index.offset),
        CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
    })
}

fn element_base(ty: CheckedType, below_element: bool) -> bool {
    matches!(
        ty,
        CheckedType::Array { .. }
            | CheckedType::Buffer { .. }
            | CheckedType::Segments { .. }
            | CheckedType::Window {
                shape: WindowShape::Slots | WindowShape::Paged,
                ..
            }
    ) || (below_element
        && matches!(
            ty,
            CheckedType::Window {
                shape: WindowShape::Ring,
                ..
            }
        ))
}

fn range_path(path: &[CheckedPlaceStep], mut below_element: bool) -> bool {
    path.iter().all(|step| match step {
        CheckedPlaceStep::Subscript(index) => {
            let admitted = element_base(index.base_type, below_element) && term(&index.offset);
            below_element = true;
            admitted
        }
        CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => true,
    })
}
