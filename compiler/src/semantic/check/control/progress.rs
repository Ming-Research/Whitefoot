//! [TERM-1] the progress each ordinary `loop_stmt` owes.
//!
//! A loop writes a rank in its header, waits on every path from its header
//! back to its header, moves a reference cursor into owned structure, or
//! derives a rank from its leading exit tests. The rank's descent is a proof-only
//! obligation the semantic proof checker discharges at the backedge, against
//! snapshots it takes at the start of each iteration; nothing here reaches
//! lowering.

use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{SemanticIssueKind, SemanticRule};

use super::super::super::model::{
    BindingId, CheckedAffineExpression, CheckedAffineExpressionKind, CheckedAffineRelation,
    CheckedEnumType, CheckedExpression, CheckedIntegerOperation, CheckedLoopId,
    CheckedLoopProgress, CheckedMode, CheckedProgressSnapshot, CheckedSetTarget, CheckedStatement,
    CheckedType, CheckedValue, IntegerType,
};
use super::super::super::places::{PlaceStep, ResolvedPlace};
use super::super::{CheckStop, Checker};
use super::ControlCounters;
use super::proofs::affine_integer_value;
use crate::NodePath;

/// The repair a loop with no progress takes [DIAG-1].
pub(crate) const TERM1_GIVE_THE_LOOP_AN_EXIT_TEST: &str = "write a rank the body lowers, such as `loop (decreases count - i) {`, or begin the body with an exit test a rank derives from, such as `if i >= count { break; }` for a cursor `i` that rises to `count`, or move a reference cursor into a `Box` below its referent on every path back to the header, or wait on every path back to the header";

/// The repair a loop takes whose only exit tests leave on an equality, which
/// derives no rank [DIAG-1].
pub(crate) const TERM1_ORDER_THE_EQUALITY_EXIT: &str = "this exit test leaves the loop on `==`, from which no rank derives, since a cursor that steps past the value it is compared with never meets it: compare with an order instead, such as `if i >= count { break; }` for a cursor `i` that rises to `count`";

/// Continuing while `low < high` or `low <= high`: the rank is `high - low`.
struct Rank {
    low: Operand,
    high: Operand,
}

/// What the checker knows of a loop's progress before reading its body: the
/// rank its header writes and whether a reference cursor descends into owned
/// structure on every backedge.
pub(super) struct ProgressEvidence {
    pub(super) written: Option<CheckedAffineExpression>,
    pub(super) descends: bool,
}

/// One rank operand: its affine form and the read that produced it.
#[derive(Clone)]
struct Operand {
    affine: CheckedAffineExpression,
    read: CheckedExpression,
}

impl<'unit> Checker<'_, 'unit> {
    /// [TERM-1] the progress of one checked loop body.
    pub(super) fn loop_progress(
        &mut self,
        id: CheckedLoopId,
        node: NodeId,
        header: ProgressEvidence,
        statements: &[CheckedStatement],
        can_continue: bool,
        counters: &mut ControlCounters<'_>,
    ) -> Result<CheckedLoopProgress, CheckStop> {
        if !can_continue {
            return Ok(CheckedLoopProgress::NoBackedge);
        }
        let ProgressEvidence { written, descends } = header;
        if let Some(rank) = written {
            return written_rank_progress(rank, counters);
        }
        let sites = super::waiting::WaitSites {
            must_wait_calls: &self.body.must_wait_calls,
            joins: &self.body.spawn_joins,
        };
        if super::waiting::iteration_waits(statements, &sites) {
            return Ok(CheckedLoopProgress::Waits);
        }
        // A structural descent owes no proof, so it is tried before a rank
        // that a leading test happens to derive.
        if descends && !self.writes_shape(statements) {
            return Ok(CheckedLoopProgress::Structural);
        }
        let node_path = self.types.declarations.tree.path(node)?.clone();
        let Derived {
            ranks,
            equality_exit,
        } = derived_ranks(id, statements, &node_path);
        if !ranks.is_empty() {
            return self.rank_progress(ranks, counters);
        }
        // A leading equality exit is the likely intent, so the rejection
        // names it and suggests the order that derives a rank.
        if let Some(test) = equality_exit.and_then(|path| {
            self.types.declarations.tree.node_with_path(&path)
        }) {
            return self.types.declarations.issue_node(
                SemanticRule::Term1,
                test,
                SemanticIssueKind::LoopWithoutProgress {
                    mechanical_fix: TERM1_ORDER_THE_EQUALITY_EXIT,
                },
            );
        }
        self.types.declarations.issue_node(
            SemanticRule::Term1,
            node,
            SemanticIssueKind::LoopWithoutProgress {
                mechanical_fix: TERM1_GIVE_THE_LOOP_AN_EXIT_TEST,
            },
        )
    }

    /// Snapshots every operand that can change and states the descent each
    /// rank owes: the backedge discharges the loop when it proves the descent
    /// of one of them, the first being the one diagnostics show.
    fn rank_progress(
        &mut self,
        ranks: Vec<Rank>,
        counters: &mut ControlCounters<'_>,
    ) -> Result<CheckedLoopProgress, CheckStop> {
        let mut snapshots = Vec::new();
        let mut owed = Vec::new();
        let mut alternatives = Vec::new();
        for rank in ranks {
            let Rank { low, high } = rank;
            let low_before = snapshot(&low, &mut snapshots, counters, "at the exit test")?;
            let high_before = snapshot(&high, &mut snapshots, counters, "at the exit test")?;
            let (low, high) = (low.affine, high.affine);
            let node_path = high.node_path.clone();
            let relation =
                |left: &CheckedAffineExpression, right: &CheckedAffineExpression, bound| {
                    CheckedAffineRelation {
                        node_path: node_path.clone(),
                        left: left.clone(),
                        right: right.clone(),
                        bound,
                        equality: false,
                    }
                };
            // `high - low` falls: `(high - low) - (high_before - low_before) <= -1`.
            let descent = relation(
                &subtract(&node_path, high.clone(), low.clone()),
                &subtract(&node_path, high_before.clone(), low_before.clone()),
                -1,
            );
            // Either side moving strictly toward the other while the other
            // side does not move away implies the descent; so does the
            // descent itself.
            alternatives.push(vec![descent.clone()]);
            alternatives.push(vec![
                relation(&low_before, &low, -1),
                relation(&high, &high_before, 0),
            ]);
            alternatives.push(vec![
                relation(&high, &high_before, -1),
                relation(&low_before, &low, 0),
            ]);
            if owed.is_empty() {
                owed.push(descent);
            }
        }
        Ok(CheckedLoopProgress::Rank {
            snapshots,
            owed,
            alternatives,
        })
    }
}

impl Checker<'_, '_> {
    /// [TERM-1] whether some statement of `statements` may add a `Box` cell
    /// to a value: a commit whose target is not a reference rebinding and
    /// whose type is not a scalar, or a call whose row writes anything.
    fn writes_shape(&self, statements: &[CheckedStatement]) -> bool {
        statements.iter().any(|statement| match statement {
            CheckedStatement::Set { target, value, .. } => {
                let rebinding = matches!(
                    target,
                    CheckedSetTarget::Place(place)
                        if place.fields.is_empty() && place.mode != CheckedMode::Own
                );
                (!rebinding && !scalar(target.ty())) || self.call_writes(value)
            }
            CheckedStatement::Let { value, .. }
            | CheckedStatement::DestructuringLet { value, .. }
            | CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. }
            | CheckedStatement::Give { value, .. }
            | CheckedStatement::Return { value, .. } => self.call_writes(value),
            CheckedStatement::PropagateLet { scrutinee, .. } => self.call_writes(scrutinee),
            CheckedStatement::Match {
                scrutinee, arms, ..
            }
            | CheckedStatement::ValueMatchLet {
                scrutinee, arms, ..
            } => self.call_writes(scrutinee) || arms.iter().any(|arm| self.writes_shape(&arm.body)),
            CheckedStatement::Loop { body, .. }
            | CheckedStatement::CountedRange { body, .. }
            | CheckedStatement::Atomic { body, .. } => self.writes_shape(body),
            CheckedStatement::Proof(_) | CheckedStatement::Break { .. } => false,
        })
    }

    /// Whether `expression` is a call whose row writes anything [EFF-1].
    fn call_writes(&self, expression: &CheckedExpression) -> bool {
        let CheckedExpression::UserCall {
            function,
            formal_effects,
            ..
        } = expression
        else {
            return false;
        };
        formal_effects
            .as_ref()
            .map(|effects| !effects.writes.is_empty())
            .or_else(|| {
                self.types
                    .signatures
                    .get(function.0 as usize)
                    .map(|signature| !signature.declared_effects.writes.is_empty())
            })
            .unwrap_or(true)
    }
}

/// A value no write of which can add or remove a `Box` cell.
fn scalar(ty: CheckedType) -> bool {
    matches!(
        ty,
        CheckedType::Unit
            | CheckedType::Bool
            | CheckedType::Integer(_)
            | CheckedType::Float(_)
            | CheckedType::GenericInt(_)
            | CheckedType::GenericFloat(_)
    )
}

/// [TERM-1] whether `backedge`, a cursor's place set on a backedge, lies
/// strictly inside `header`, its place set when the iteration began: every
/// backedge place extends one header place by owned steps only, at least one
/// of which enters a `Box`.
pub(super) fn strictly_inside(header: &[ResolvedPlace], backedge: &[ResolvedPlace]) -> bool {
    !backedge.is_empty()
        && backedge.iter().all(|place| {
            header.iter().any(|anchor| {
                place.root == anchor.root
                    && place.path.len() > anchor.path.len()
                    && place.path.starts_with(&anchor.path)
                    && {
                        let suffix = &place.path[anchor.path.len()..];
                        suffix.iter().all(|step| {
                            matches!(
                                step,
                                PlaceStep::Field(_)
                                    | PlaceStep::Deref
                                    | PlaceStep::Payload { .. }
                                    | PlaceStep::Index(_)
                            )
                        }) && suffix.contains(&PlaceStep::Deref)
                    }
            })
        })
}

/// [TERM-1] a written rank R: every backedge owes `R' < R0` and `0 <= R'`,
/// where R0 reads each operand of R from its snapshot.
fn written_rank_progress(
    rank: CheckedAffineExpression,
    counters: &mut ControlCounters<'_>,
) -> Result<CheckedLoopProgress, CheckStop> {
    let mut snapshots = Vec::new();
    let before = snapshot_leaves(&rank, &mut snapshots, counters)?;
    let node_path = rank.node_path.clone();
    // Zero in the type of the rank's first operand, so the floor renders in
    // the writer's own terms.
    let ty = rank
        .postorder()
        .find(|leaf| {
            !matches!(
                leaf.kind,
                CheckedAffineExpressionKind::Add(..)
                    | CheckedAffineExpressionKind::Subtract(..)
                    | CheckedAffineExpressionKind::MultiplyByConstant { .. }
            )
        })
        .map_or(IntegerType::U64, operand_type);
    let zero = CheckedAffineExpression {
        node_path: node_path.clone(),
        kind: CheckedAffineExpressionKind::Constant { value: 0, ty },
    };
    let descent = CheckedAffineRelation {
        node_path: node_path.clone(),
        left: rank.clone(),
        right: before,
        bound: -1,
        equality: false,
    };
    let floor = CheckedAffineRelation {
        node_path,
        left: zero,
        right: rank,
        bound: 0,
        equality: false,
    };
    Ok(CheckedLoopProgress::Rank {
        snapshots,
        owed: vec![descent, floor],
        alternatives: Vec::new(),
    })
}

/// `rank` with each operand that can change replaced by its value at the
/// start of the iteration; one snapshot serves every occurrence of an
/// operand.
fn snapshot_leaves(
    rank: &CheckedAffineExpression,
    snapshots: &mut Vec<CheckedProgressSnapshot>,
    counters: &mut ControlCounters<'_>,
) -> Result<CheckedAffineExpression, CheckStop> {
    let mut taken: Vec<(CheckedAffineExpressionKind, CheckedAffineExpression)> = Vec::new();
    let mut values = Vec::new();
    for expression in rank.postorder() {
        let kind = match &expression.kind {
            CheckedAffineExpressionKind::Add(_, _)
            | CheckedAffineExpressionKind::Subtract(_, _) => {
                let right = Box::new(
                    values
                        .pop()
                        .ok_or(crate::SemanticCompilerFailure::InvalidCanonicalTree)?,
                );
                let left = Box::new(
                    values
                        .pop()
                        .ok_or(crate::SemanticCompilerFailure::InvalidCanonicalTree)?,
                );
                if matches!(expression.kind, CheckedAffineExpressionKind::Add(_, _)) {
                    CheckedAffineExpressionKind::Add(left, right)
                } else {
                    CheckedAffineExpressionKind::Subtract(left, right)
                }
            }
            CheckedAffineExpressionKind::MultiplyByConstant {
                constant,
                constant_ty,
                ..
            } => CheckedAffineExpressionKind::MultiplyByConstant {
                constant: *constant,
                constant_ty: *constant_ty,
                value: Box::new(
                    values
                        .pop()
                        .ok_or(crate::SemanticCompilerFailure::InvalidCanonicalTree)?,
                ),
            },
            leaf @ (CheckedAffineExpressionKind::Local { .. }
            | CheckedAffineExpressionKind::Measure(_)) => {
                if let Some((_, before)) = taken.iter().find(|(kind, _)| kind == leaf) {
                    before.kind.clone()
                } else {
                    let read = match leaf {
                        CheckedAffineExpressionKind::Measure(measure) => (**measure).clone(),
                        CheckedAffineExpressionKind::Local { binding, ty } => {
                            CheckedExpression::Binding {
                                carrier: expression.node_path.clone(),
                                binding: *binding,
                                ty: CheckedType::Integer(*ty),
                                consume_root: false,
                            }
                        }
                        _ => {
                            return Err(crate::SemanticCompilerFailure::InvalidCanonicalTree.into());
                        }
                    };
                    let operand = Operand {
                        affine: expression.clone(),
                        read,
                    };
                    let before =
                        snapshot(&operand, snapshots, counters, "when the iteration began")?;
                    taken.push((leaf.clone(), before.clone()));
                    before.kind.clone()
                }
            }
            leaf @ (CheckedAffineExpressionKind::Constant { .. }
            | CheckedAffineExpressionKind::ConstGeneric { .. }) => leaf.clone(),
        };
        values.push(CheckedAffineExpression {
            node_path: expression.node_path.clone(),
            kind,
        });
    }
    values
        .pop()
        .ok_or_else(|| crate::SemanticCompilerFailure::InvalidCanonicalTree.into())
}

/// The value an operand held at the start of the iteration: a constant is
/// itself, and anything else is read from a proof-only snapshot binding.
fn snapshot(
    operand: &Operand,
    snapshots: &mut Vec<CheckedProgressSnapshot>,
    counters: &mut ControlCounters<'_>,
    when: &str,
) -> Result<CheckedAffineExpression, CheckStop> {
    let read = operand.read.clone();
    let operand = &operand.affine;
    if let CheckedAffineExpressionKind::Constant { .. } = operand.kind {
        return Ok(operand.clone());
    }
    let ty = operand_type(operand);
    let name = match &operand.kind {
        CheckedAffineExpressionKind::Local { binding, .. } => {
            counters.binding_names.get(binding.0 as usize).map_or_else(
                || format!("the operand {when}"),
                |name| format!("{name} {when}"),
            )
        }
        CheckedAffineExpressionKind::Measure(_) => format!("the measure {when}"),
        _ => format!("the operand {when}"),
    };
    let binding = Checker::allocate_binding(counters.next_binding)?;
    counters.binding_names.push(name);
    snapshots.push(CheckedProgressSnapshot {
        binding,
        ty,
        value: operand.clone(),
        read,
    });
    Ok(CheckedAffineExpression {
        node_path: operand.node_path.clone(),
        kind: CheckedAffineExpressionKind::Local { binding, ty },
    })
}

fn operand_type(operand: &CheckedAffineExpression) -> IntegerType {
    match &operand.kind {
        CheckedAffineExpressionKind::Constant { ty, .. }
        | CheckedAffineExpressionKind::Local { ty, .. }
        | CheckedAffineExpressionKind::ConstGeneric { ty, .. } => *ty,
        CheckedAffineExpressionKind::Add(left, _)
        | CheckedAffineExpressionKind::Subtract(left, _) => operand_type(left),
        CheckedAffineExpressionKind::MultiplyByConstant { value, .. } => operand_type(value),
        CheckedAffineExpressionKind::Measure(_) => IntegerType::U64,
    }
}

fn subtract(
    node_path: &NodePath,
    left: CheckedAffineExpression,
    right: CheckedAffineExpression,
) -> CheckedAffineExpression {
    CheckedAffineExpression {
        node_path: node_path.clone(),
        kind: CheckedAffineExpressionKind::Subtract(Box::new(left), Box::new(right)),
    }
}

/// [TERM-1] the ranks a body's leading exit tests derive. The leading
/// statements are `let` bindings whose initializers call nothing and exit
/// tests: `if` statements one of whose arms leaves the loop and whose
/// condition is a comparison or a `Bool` binding one of those `let`s bound to
/// one. A test whose continuing arm is empty keeps the prefix open, so no
/// statement before a later test writes a rank operand; the first test with
/// a nonempty continuing arm is the last one.
fn derived_ranks(
    id: CheckedLoopId,
    statements: &[CheckedStatement],
    node_path: &NodePath,
) -> Derived {
    // Each `let` before an exit test, by the binding it introduces. An
    // operand that names one is read through to its initializer, which is
    // evaluated in the same iteration before the test.
    let mut comparisons: HashMap<BindingId, &CheckedExpression> = HashMap::new();
    let mut ranks = Vec::new();
    let mut equality_exit = None;
    for statement in statements {
        match statement {
            CheckedStatement::Let { binding, value, .. } => {
                if calls_anything(value) {
                    break;
                }
                comparisons.insert(*binding, value);
            }
            CheckedStatement::Match {
                scrutinee,
                enum_type: CheckedEnumType::Bool,
                arms,
                ..
            } => {
                // A call in the condition could write an operand of a later
                // test, so it ends the leading statements; any other
                // condition is a test that derives a rank only when it is a
                // comparison.
                if calls_anything(scrutinee) {
                    break;
                }
                let condition = match scrutinee {
                    CheckedExpression::Binding { binding, .. } => {
                        comparisons.get(binding).copied().unwrap_or(scrutinee)
                    }
                    other => other,
                };
                let breaking: Vec<u32> = arms
                    .iter()
                    .filter(|arm| breaks_loop(&arm.body, id))
                    .map(|arm| arm.tag)
                    .collect();
                // Bool's `True` arm carries tag 1; exactly one arm leaves.
                let [breaking_tag] = breaking.as_slice() else {
                    break;
                };
                let continuing_when = *breaking_tag == 0;
                if let Some(rank) =
                    comparison_rank(condition, continuing_when, node_path, &comparisons)
                {
                    ranks.push(rank);
                } else if equality_exit.is_none() {
                    equality_exit = leaves_on_equality(condition, continuing_when);
                }
                let continuing_empty = arms
                    .iter()
                    .filter(|arm| arm.tag != *breaking_tag)
                    .all(|arm| arm.body.is_empty());
                if !continuing_empty {
                    break;
                }
            }
            _ => break,
        }
    }
    Derived {
        ranks,
        equality_exit,
    }
}

/// What a body's leading exit tests give: the ranks they derive, and the
/// first test that leaves on an integer equality, which derives none.
struct Derived {
    ranks: Vec<Rank>,
    equality_exit: Option<NodePath>,
}

/// The comparison's node when `condition` is an integer comparison under
/// which the loop continues exactly while its operands differ.
fn leaves_on_equality(condition: &CheckedExpression, continuing_when: bool) -> Option<NodePath> {
    let CheckedExpression::IntegerOperation {
        carrier,
        operation,
        ..
    } = condition
    else {
        return None;
    };
    let continuing = if continuing_when {
        *operation
    } else {
        negated(*operation)?
    };
    (continuing == CheckedIntegerOperation::NotEqual).then(|| carrier.clone())
}

/// Whether an arm leaves the loop on every path: it ends in a `break` of
/// this loop or of one around it, or in a `return`. Loop identities are
/// allocated outside in, so an enclosing loop's is smaller.
fn breaks_loop(body: &[CheckedStatement], id: CheckedLoopId) -> bool {
    match body.last() {
        Some(CheckedStatement::Break { target, .. }) => target.0 <= id.0,
        Some(CheckedStatement::Return { .. }) => true,
        _ => false,
    }
}

/// The rank of `condition`, read as the condition under which the loop
/// continues when `continuing_when` is true and as its negation otherwise.
fn comparison_rank(
    condition: &CheckedExpression,
    continuing_when: bool,
    node_path: &NodePath,
    lets: &HashMap<BindingId, &CheckedExpression>,
) -> Option<Rank> {
    let CheckedExpression::IntegerOperation {
        operation,
        arguments,
        ..
    } = condition
    else {
        return None;
    };
    let [left, right] = arguments.as_slice() else {
        return None;
    };
    let left = affine_operand(through_lets(left, lets), node_path, lets)?;
    let right = affine_operand(through_lets(right, lets), node_path, lets)?;
    // The continuing relation, normalized to `low < high` or `low <= high`.
    let continuing = if continuing_when {
        *operation
    } else {
        negated(*operation)?
    };
    match continuing {
        CheckedIntegerOperation::Less | CheckedIntegerOperation::LessEqual => Some(Rank {
            low: left,
            high: right,
        }),
        CheckedIntegerOperation::Greater | CheckedIntegerOperation::GreaterEqual => Some(Rank {
            low: right,
            high: left,
        }),
        // `x != 0` over an unsigned `x` continues while `0 < x`.
        CheckedIntegerOperation::NotEqual => {
            if is_zero(&right.affine) && unsigned(&left.affine) {
                Some(Rank {
                    low: right,
                    high: left,
                })
            } else if is_zero(&left.affine) && unsigned(&right.affine) {
                Some(Rank {
                    low: left,
                    high: right,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// An operand naming a binding one of the test's preceding `let`s
/// introduced, read through to that `let`'s initializer.
fn through_lets<'expression>(
    operand: &'expression CheckedExpression,
    lets: &HashMap<BindingId, &'expression CheckedExpression>,
) -> &'expression CheckedExpression {
    let mut current = operand;
    while let CheckedExpression::Binding { binding, .. } = current
        && let Some(initializer) = lets.get(binding)
    {
        current = initializer;
    }
    current
}

fn negated(operation: CheckedIntegerOperation) -> Option<CheckedIntegerOperation> {
    Some(match operation {
        CheckedIntegerOperation::Less => CheckedIntegerOperation::GreaterEqual,
        CheckedIntegerOperation::LessEqual => CheckedIntegerOperation::Greater,
        CheckedIntegerOperation::Greater => CheckedIntegerOperation::LessEqual,
        CheckedIntegerOperation::GreaterEqual => CheckedIntegerOperation::Less,
        CheckedIntegerOperation::Equal => CheckedIntegerOperation::NotEqual,
        CheckedIntegerOperation::NotEqual => CheckedIntegerOperation::Equal,
        _ => return None,
    })
}

fn is_zero(operand: &CheckedAffineExpression) -> bool {
    matches!(
        operand.kind,
        CheckedAffineExpressionKind::Constant { value: 0, .. }
    )
}

fn unsigned(operand: &CheckedAffineExpression) -> bool {
    !operand_type(operand).signed()
}

/// [TERM-1] an exit-test operand in the rank vocabulary: an integer literal
/// or named const, an integer binding, a measure read, or an exact sum,
/// difference or literal multiple of such operands, each read through the
/// test's preceding `let`s.
fn affine_operand(
    expression: &CheckedExpression,
    node_path: &NodePath,
    lets: &HashMap<BindingId, &CheckedExpression>,
) -> Option<Operand> {
    Some(Operand {
        affine: affine_form(expression, node_path, lets)?,
        read: expression.clone(),
    })
}

fn affine_form(
    expression: &CheckedExpression,
    node_path: &NodePath,
    lets: &HashMap<BindingId, &CheckedExpression>,
) -> Option<CheckedAffineExpression> {
    let kind = match expression {
        CheckedExpression::Constant(CheckedValue::Integer { ty, bits })
        | CheckedExpression::NamedConstant {
            value: CheckedValue::Integer { ty, bits },
            ..
        } => CheckedAffineExpressionKind::Constant {
            value: affine_integer_value(*ty, *bits),
            ty: *ty,
        },
        CheckedExpression::Binding {
            binding,
            ty: CheckedType::Integer(ty),
            ..
        } => CheckedAffineExpressionKind::Local {
            binding: *binding,
            ty: *ty,
        },
        CheckedExpression::ContainerMeasure { .. }
        | CheckedExpression::ArrayMeasure { .. }
        | CheckedExpression::BufferMeasure { .. }
        | CheckedExpression::RangeMeasure { .. }
        | CheckedExpression::RangeElementMeasure { .. } => {
            CheckedAffineExpressionKind::Measure(Box::new(expression.clone()))
        }
        CheckedExpression::IntegerOperation {
            operation,
            arguments,
            ..
        } => {
            let [left, right] = arguments.as_slice() else {
                return None;
            };
            let left = affine_form(through_lets(left, lets), node_path, lets)?;
            let right = affine_form(through_lets(right, lets), node_path, lets)?;
            match operation {
                CheckedIntegerOperation::AddExact | CheckedIntegerOperation::AddDefined => {
                    CheckedAffineExpressionKind::Add(Box::new(left), Box::new(right))
                }
                CheckedIntegerOperation::SubtractExact
                | CheckedIntegerOperation::SubtractDefined => {
                    CheckedAffineExpressionKind::Subtract(Box::new(left), Box::new(right))
                }
                CheckedIntegerOperation::MultiplyExact
                | CheckedIntegerOperation::MultiplyDefined => {
                    let (constant, value) = match (&left.kind, &right.kind) {
                        (CheckedAffineExpressionKind::Constant { value, ty }, _) => {
                            ((*value, *ty), right)
                        }
                        (_, CheckedAffineExpressionKind::Constant { value, ty }) => {
                            ((*value, *ty), left)
                        }
                        _ => return None,
                    };
                    CheckedAffineExpressionKind::MultiplyByConstant {
                        constant: constant.0,
                        constant_ty: constant.1,
                        value: Box::new(value),
                    }
                }
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(CheckedAffineExpression {
        node_path: node_path.clone(),
        kind,
    })
}

/// Whether evaluating `expression` makes a call, which could write a rank
/// operand before the exit test reads it. A call's arguments are atoms
/// [GRAM-9], so a call can stand only at an initializer's top.
fn calls_anything(expression: &CheckedExpression) -> bool {
    matches!(expression, CheckedExpression::UserCall { .. })
}
