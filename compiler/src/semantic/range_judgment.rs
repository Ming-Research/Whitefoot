//! [RANGE-2, RANGE-3, RANGE-5] the range judgment.
//!
//! Range facts are judged over the completed checked program, after every
//! function's ordinary entailment has either discharged its records or left
//! only deferrable integer goals. The walk proves each deferred record at
//! its own program point before assuming a local invariant's target. One forward walk per function carries the facts
//! a function's requirements, its loops' invariants and its callees'
//! postconditions establish, discharges every range fact owed at a call, a
//! loop's entry, a back edge or an exit, and checks each counted loop's
//! cross-iteration certificate. A
//! certificate that holds is retained for the counted permission judgment
//! [PAR-2]; it grants nothing by itself.

mod constants;
mod facts;
pub(crate) use constants::judge as judge_constant_invariant;
mod solver;
mod walk;
mod world;

use crate::NodePath;

use super::model::{CheckedFunction, CheckedNominal};

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
        /// The structural ceiling the derivation reached, if it reached one.
        capacity: Option<&'static str>,
    },
    /// No exit of an inhabited instance selects the range postcondition
    /// written at `node` [RANGE-3].
    NoSelectedExit { node: NodePath },
    /// A counted loop's certificate does not separate two iterations
    /// [RANGE-5].
    Apart {
        node: NodePath,
        failure: ApartFailure,
    },
    /// The judgment at `node` needs what this checker does not implement,
    /// so the function's verdict is an unsupported capability, never a
    /// rejection: the derivation left the checker's `i128` arithmetic,
    /// where the specified arithmetic is exact [RANGE-3], or a loop nest was
    /// deeper, or a header's written set took more walks to settle, than the
    /// checker follows, where RANGE-2 forgets only what the body can write; or
    /// atomic targets may alias and the walk cannot represent that relation.
    Unsupported {
        node: NodePath,
        feature: super::UnsupportedSemanticFeature,
    },
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
    Use {
        step: NodePath,
        reason: &'static str,
    },
    /// One pair's derivation reached a structural ceiling.
    Capacity {
        write: NodePath,
        other: NodePath,
        ceiling: &'static str,
    },
}

pub(crate) use super::range_facts::CheckedCertifiedLoop as CertifiedLoop;

/// The judgment of one function.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RangeJudgment {
    pub(crate) issues: Vec<RangeIssue>,
    pub(crate) certified: Vec<CertifiedLoop>,
    pub(crate) discharged: Vec<usize>,
}

/// Whether a function takes part: it states a range clause, or it calls a
/// function with range requirements or postconditions.
pub(crate) fn takes_part(
    function: &CheckedFunction,
    boundary: impl Fn(super::model::FunctionId) -> bool,
) -> bool {
    if !function.range_facts.is_empty() {
        return true;
    }
    let mut calls = false;
    for_each_call(
        function.body.as_deref().unwrap_or_default(),
        &mut |callee, _| {
            calls |= boundary(callee);
        },
    );
    calls
}

/// Visits every ordinary call in `statements` with its callee and its call
/// node.
pub(crate) fn for_each_call(
    statements: &[super::model::CheckedStatement],
    visit: &mut dyn FnMut(super::model::FunctionId, &crate::NodePath),
) {
    use super::model::{CheckedStatement, expression_children};
    fn expression(
        value: &super::model::CheckedExpression,
        visit: &mut dyn FnMut(super::model::FunctionId, &crate::NodePath),
    ) {
        if let super::model::CheckedExpression::UserCall { function, call, .. } = value {
            visit(*function, call);
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
                targets,
                guard,
                body,
                ..
            } => {
                for target in targets
                    .iter()
                    .flat_map(crate::semantic::CheckedTarget::expressions)
                {
                    expression(target, visit);
                }
                if let Some(guard) = guard {
                    expression(guard, visit);
                }
                for_each_call(body, visit);
            }
            CheckedStatement::Proof(_)
            | CheckedStatement::Break { .. }
            | CheckedStatement::Continue { .. } => {}
        }
    }
}

/// Judges every selected function of a checked program, dense by function.
pub(crate) fn judge_program(
    functions: &[&CheckedFunction],
    nominals: &[CheckedNominal],
    selected: &[bool],
    constants: &[super::model::CheckedConstant],
) -> Vec<RangeJudgment> {
    functions
        .iter()
        .enumerate()
        .map(|(index, function)| {
            if !selected.get(index).copied().unwrap_or(false)
                || function.body.is_none()
                || !takes_part(function, |callee| {
                    functions.get(callee.0 as usize).is_some_and(|callee| {
                        !callee.range_facts.requirements.is_empty()
                            || !callee.range_facts.postconditions.is_empty()
                    })
                })
                || deferred_records(function).is_none()
            {
                return RangeJudgment::default();
            }
            let mut walker = walk::Walker::new(
                functions,
                nominals,
                function,
                deferred_records(function).unwrap_or_default(),
                constants,
            );
            walker.run();
            // A walk that forgot more than RANGE-2 does cannot reject: what
            // it left unproved may hold.
            let issues = match (&walker.issues[..], walker.imprecise.take()) {
                ([], _) | (_, None) => walker.issues,
                (_, Some(node)) => vec![RangeIssue::Unsupported {
                    node,
                    feature: super::UnsupportedSemanticFeature::RangeLoopNesting,
                }],
            };
            RangeJudgment {
                issues,
                certified: walker.certified,
                discharged: walker
                    .deferred
                    .into_iter()
                    .filter_map(|(index, answer)| (answer == Some(true)).then_some(index))
                    .collect(),
            }
        })
        .collect()
}

/// Only functions whose complete remaining ordinary debt is deferrable proceed.
fn deferred_records(function: &CheckedFunction) -> Option<Vec<(usize, Option<bool>)>> {
    use super::entailment::ObligationFamily;
    use super::obligations::ObligationSubject;
    let mut pending = Vec::new();
    for (index, record) in function.obligations.iter().enumerate() {
        let answer = function.entailment.answers.get(index).copied().flatten()?;
        if answer.discharged(&function.entailment) {
            continue;
        }
        match record.subject {
            ObligationSubject::Source {
                family:
                    ObligationFamily::Bounds
                    | ObligationFamily::IntegerDomain
                    | ObligationFamily::ConversionDomain,
                ..
            }
            | ObligationSubject::LoopInvariant { .. } => pending.push((index, None)),
            ObligationSubject::SourceProof => {
                let super::obligations::RecordAnswer::SourceProof(proof) = answer else {
                    return None;
                };
                let check = &function.entailment.source_proofs.get(proof)?.check;
                // A failed target can be deferred; an invalid ordinary certificate cannot.
                if check.redundant
                    || check.source_failure.is_some()
                    || check.certificate_failure.is_some()
                    || check.residual_failure.is_some()
                    || check.first_unproved_premise.is_some()
                {
                    return None;
                }
                pending.push((index, None));
            }
            ObligationSubject::CallRequirement { .. } => {
                let super::obligations::RecordAnswer::CallGoal(goal) = answer else {
                    return None;
                };
                let goal = &function.entailment.call_goals.get(goal)?.goal.root;
                use super::goal::{GoalExpression, GoalOperation};
                use super::model::CheckedIntegerOperation as Op;
                if !matches!(
                    goal,
                    GoalExpression::Operation {
                        row: GoalOperation::Integer {
                            operation: Op::Equal
                                | Op::NotEqual
                                | Op::Less
                                | Op::LessEqual
                                | Op::Greater
                                | Op::GreaterEqual,
                            ..
                        },
                        ..
                    }
                ) {
                    return None;
                }
                pending.push((index, None));
            }
            _ => return None,
        }
    }
    Some(pending)
}
