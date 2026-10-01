//! [RANGE-2, RANGE-3, RANGE-5] the range judgment.
//!
//! Range facts are judged over the completed checked program, after every
//! function's ordinary entailment has succeeded, because a range proof
//! consumes what that judgment established and no ordinary obligation
//! consumes a range fact. One forward walk per function carries the facts
//! a function's requirements, loop invariants and callees' guarantees
//! establish, discharges every range fact owed at a call, a loop header or a
//! return, and checks each counted loop's cross-iteration certificate. A
//! certificate that holds is retained for the counted permission judgment
//! [PAR-2]; it grants nothing by itself.

mod facts;
mod solver;
mod walk;
mod world;

use crate::NodePath;

use super::model::CheckedFunction;

/// One range judgment failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RangeIssue {
    /// A range fact owed at `node` is not established [RANGE-3].
    Undischarged {
        node: NodePath,
        fact: String,
        site: &'static str,
        /// The conclusion the derivation left open, when the fact was written.
        relation: Option<NodePath>,
        capacity: bool,
    },
    /// A counted loop's certificate does not separate two iterations
    /// [RANGE-5].
    Apart { node: NodePath, failure: ApartFailure },
}

/// Why a certificate does not hold.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ApartFailure {
    /// A write of one iteration and an access of another are not separated.
    Overlap {
        write: NodePath,
        other: NodePath,
        other_write: bool,
    },
    /// An access reaches storage another iteration writes at no one element.
    Unplaced { access: NodePath },
    /// A `use` step does not form an instance.
    Use { step: NodePath, reason: &'static str },
    /// One pair's derivation reached a structural capacity.
    Capacity { write: NodePath, other: NodePath },
}

pub(crate) use super::range_facts::CheckedCertifiedLoop as CertifiedLoop;

/// The judgment of one function.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RangeJudgment {
    pub(crate) issues: Vec<RangeIssue>,
    pub(crate) certified: Vec<CertifiedLoop>,
}

/// Whether a function takes part: it states a range clause, or it calls a
/// function whose range requirements it owes.
fn takes_part(functions: &[CheckedFunction], function: &CheckedFunction) -> bool {
    if !function.range_facts.is_empty() {
        return true;
    }
    let mut calls = false;
    for_each_call(function.body.as_deref().unwrap_or_default(), &mut |callee| {
        if functions
            .get(callee.0 as usize)
            .is_some_and(|callee| !callee.range_facts.requirements.is_empty())
        {
            calls = true;
        }
    });
    calls
}

fn for_each_call(
    statements: &[super::model::CheckedStatement],
    visit: &mut dyn FnMut(super::model::FunctionId),
) {
    use super::model::{CheckedStatement, expression_children};
    fn expression(
        value: &super::model::CheckedExpression,
        visit: &mut dyn FnMut(super::model::FunctionId),
    ) {
        if let super::model::CheckedExpression::UserCall { function, .. } = value {
            visit(*function);
        }
        for child in expression_children(value) {
            expression(child, visit);
        }
    }
    for statement in statements {
        match statement {
            CheckedStatement::Let { value, .. }
            | CheckedStatement::DestructuringLet { value, .. }
            | CheckedStatement::Set { value, .. }
            | CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. }
            | CheckedStatement::Return { value, .. }
            | CheckedStatement::Give { value, .. } => expression(value, visit),
            CheckedStatement::PropagateLet { scrutinee, .. } => expression(scrutinee, visit),
            CheckedStatement::Match {
                scrutinee, arms, ..
            }
            | CheckedStatement::ValueMatchLet {
                scrutinee, arms, ..
            } => {
                expression(scrutinee, visit);
                for arm in arms {
                    for_each_call(&arm.body, visit);
                }
            }
            CheckedStatement::Loop { body, .. } => for_each_call(body, visit),
            CheckedStatement::CountedRange {
                lower, upper, body, ..
            } => {
                expression(lower, visit);
                expression(upper, visit);
                for_each_call(body, visit);
            }
            CheckedStatement::Atomic {
                target, guard, body, ..
            } => {
                expression(target, visit);
                if let Some(guard) = guard {
                    expression(guard, visit);
                }
                for_each_call(body, visit);
            }
            CheckedStatement::Proof(_) | CheckedStatement::Break { .. } => {}
        }
    }
}

/// Judges every selected function of a checked program, dense by function.
pub(crate) fn judge_program(functions: &[CheckedFunction], selected: &[bool]) -> Vec<RangeJudgment> {
    functions
        .iter()
        .enumerate()
        .map(|(index, function)| {
            if !selected.get(index).copied().unwrap_or(false)
                || function.body.is_none()
                || !takes_part(functions, function)
            {
                return RangeJudgment::default();
            }
            let mut walker = walk::Walker::new(functions, function);
            walker.run();
            RangeJudgment {
                issues: walker.issues,
                certified: walker.certified,
            }
        })
        .collect()
}
