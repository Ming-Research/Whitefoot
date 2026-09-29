//! [OP-9, STOR-6] each allocation's discharged length bound, installed on
//! the call that allocates, where target layout later qualifies it.

use std::collections::HashMap;

use crate::{NodePath, SemanticCompilerFailure};

use super::super::model::{CheckedExpression, CheckedFunction, CheckedSetTarget, CheckedStatement};
use super::{CheckStop, Checker};

impl Checker<'_, '_> {
    pub(super) fn install_source_allocation_bounds(
        functions: &mut [CheckedFunction],
    ) -> Result<(), CheckStop> {
        for function in functions {
            let bounds = function
                .entailment
                .obligations
                .iter()
                .filter(|outcome| {
                    outcome.family == super::super::entailment::ObligationFamily::AllocationFit
                        && outcome.discharged
                })
                .map(|outcome| {
                    let upper = outcome
                        .allocation_length_upper_bound
                        .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                    Ok((outcome.node_path.clone(), upper))
                })
                .collect::<Result<HashMap<_, _>, SemanticCompilerFailure>>()?;
            if let Some(body) = &mut function.body {
                Checker::install_statement_allocation_bounds(body, &bounds)?;
            }
        }
        Ok(())
    }

    fn install_statement_allocation_bounds(
        statements: &mut [CheckedStatement],
        bounds: &HashMap<NodePath, u64>,
    ) -> Result<(), SemanticCompilerFailure> {
        for statement in statements {
            match statement {
                CheckedStatement::Let { value, .. }
                | CheckedStatement::DestructuringLet { value, .. }
                | CheckedStatement::Evaluate { value, .. }
                | CheckedStatement::DropExpression { value, .. }
                | CheckedStatement::Return { value, .. }
                | CheckedStatement::Give { value, .. } => {
                    Checker::install_expression_allocation_bounds(value, bounds)?;
                }
                CheckedStatement::PropagateLet { scrutinee, .. } => {
                    Checker::install_expression_allocation_bounds(scrutinee, bounds)?;
                }
                CheckedStatement::Set { target, value, .. } => {
                    match target {
                        CheckedSetTarget::Place(_) => {}
                        CheckedSetTarget::RangeIndex(target) => {
                            for offset in target.offsets_mut() {
                                Checker::install_expression_allocation_bounds(offset, bounds)?;
                            }
                        }
                        CheckedSetTarget::Storage(target) => {
                            for offset in target.offsets_mut() {
                                Checker::install_expression_allocation_bounds(offset, bounds)?;
                            }
                        }
                    }
                    Checker::install_expression_allocation_bounds(value, bounds)?;
                }
                CheckedStatement::Match {
                    scrutinee, arms, ..
                }
                | CheckedStatement::ValueMatchLet {
                    scrutinee, arms, ..
                } => {
                    Checker::install_expression_allocation_bounds(scrutinee, bounds)?;
                    for arm in arms {
                        Checker::install_statement_allocation_bounds(&mut arm.body, bounds)?;
                    }
                }
                CheckedStatement::Loop { body, .. } => {
                    Checker::install_statement_allocation_bounds(body, bounds)?;
                }
                CheckedStatement::CountedRange {
                    lower, upper, body, ..
                } => {
                    Checker::install_expression_allocation_bounds(lower, bounds)?;
                    Checker::install_expression_allocation_bounds(upper, bounds)?;
                    Checker::install_statement_allocation_bounds(body, bounds)?;
                }
                CheckedStatement::Atomic {
                    target,
                    guard,
                    body,
                    ..
                } => {
                    Checker::install_expression_allocation_bounds(target, bounds)?;
                    if let Some(guard) = guard {
                        Checker::install_expression_allocation_bounds(guard, bounds)?;
                    }
                    Checker::install_statement_allocation_bounds(body, bounds)?;
                }
                CheckedStatement::Proof(_) => {}
                CheckedStatement::Break { .. } => {}
            }
        }
        Ok(())
    }

    fn install_expression_allocation_bounds(
        expression: &mut CheckedExpression,
        bounds: &HashMap<NodePath, u64>,
    ) -> Result<(), SemanticCompilerFailure> {
        match expression {
            CheckedExpression::UserCall {
                call,
                arguments,
                allocation,
                ..
            } => {
                if let Some(allocation) = allocation
                    && let Some(upper) = bounds.get(call).copied()
                {
                    allocation.install_source_length_upper_bound(upper);
                }
                for argument in arguments {
                    Checker::install_expression_allocation_bounds(argument, bounds)?;
                }
            }
            CheckedExpression::IntegerOperation { arguments, .. }
            | CheckedExpression::FloatOperation { arguments, .. }
            | CheckedExpression::BooleanOperation { arguments, .. }
            | CheckedExpression::EnumEquality { arguments, .. }
            | CheckedExpression::ConstructStruct {
                fields: arguments, ..
            }
            | CheckedExpression::ConstructEnum {
                fields: arguments, ..
            } => {
                for argument in arguments {
                    Checker::install_expression_allocation_bounds(argument, bounds)?;
                }
            }
            CheckedExpression::NumericConversion { value, .. }
            | CheckedExpression::Reinterpret { value, .. }
            | CheckedExpression::BoxDeref { value, .. }
            | CheckedExpression::ProjectValue { value, .. } => {
                Checker::install_expression_allocation_bounds(value, bounds)?;
            }
            CheckedExpression::BoxTake { .. } => {}
            CheckedExpression::ReadStorage { root, .. } => {
                for offset in root.offsets_mut() {
                    Checker::install_expression_allocation_bounds(offset, bounds)?;
                }
            }
            CheckedExpression::ArrayIndex { offset, .. }
            | CheckedExpression::BufferIndex { offset, .. } => {
                Checker::install_expression_allocation_bounds(offset, bounds)?;
            }
            CheckedExpression::RangeElementMeasure { place, .. }
            | CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => {
                for offset in place.offsets_mut() {
                    Checker::install_expression_allocation_bounds(offset, bounds)?;
                }
            }
            CheckedExpression::BorrowSegment { root, segment, .. } => {
                for offset in root.offsets_mut() {
                    Checker::install_expression_allocation_bounds(offset, bounds)?;
                }
                if let Some(offset) = segment.offset_mut() {
                    Checker::install_expression_allocation_bounds(offset, bounds)?;
                }
            }
            CheckedExpression::RangeOf {
                source, start, end, ..
            } => {
                match source {
                    super::super::model::CheckedRangeSource::Storage(root) => {
                        for offset in root.offsets_mut() {
                            Checker::install_expression_allocation_bounds(offset, bounds)?;
                        }
                    }
                    super::super::model::CheckedRangeSource::Element(place) => {
                        for offset in place.offsets_mut() {
                            Checker::install_expression_allocation_bounds(offset, bounds)?;
                        }
                    }
                    super::super::model::CheckedRangeSource::Range(_) => {}
                }
                Checker::install_expression_allocation_bounds(start, bounds)?;
                Checker::install_expression_allocation_bounds(end, bounds)?;
            }
            CheckedExpression::Constant(_)
            | CheckedExpression::NamedConstant { .. }
            | CheckedExpression::Binding { .. }
            | CheckedExpression::ArrayMeasure { .. }
            | CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::ContainerMeasure { .. }
            | CheckedExpression::RangeMeasure { .. }
            | CheckedExpression::BorrowAddressed { .. }
            | CheckedExpression::DerefAddressed { .. }
            | CheckedExpression::Project { .. } => {}
        }
        Ok(())
    }
}
