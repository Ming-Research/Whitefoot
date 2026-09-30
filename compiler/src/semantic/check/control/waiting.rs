//! [WAIT-1, TERM-1] the waits a body executes on its paths.
//!
//! A wait is a call, not a spawn, of a function that writes `must_wait`, an
//! atomic statement that has a guard, or the join of a spawn. Two judgments
//! read them: a `must_wait` function executes one on every path from its
//! body's start to each exit, and a loop that owes no rank executes one on
//! every path from its body's start back to its header. They differ only in
//! the spawn written as an expression statement, which is joined when its
//! activation leaves: before every exit of the function, but not inside the
//! iteration that started it.

use crate::{NodePath, SemanticIssueKind, SemanticRule};

use super::super::super::model::{CheckedExpression, CheckedLoopId, CheckedStatement};
use super::super::{CheckStop, Checker, FunctionSignature};

/// The wait sites of one body, by the node paths the checker recorded.
pub(super) struct WaitSites<'body> {
    /// Every call, not a spawn, whose callee writes `must_wait`.
    pub(super) must_wait_calls: &'body [NodePath],
    /// The spawns whose join counts: every spawn for a function's exits, the
    /// spawns a `let_stmt` binds for a loop's iteration.
    pub(super) joins: &'body [NodePath],
}

impl WaitSites<'_> {
    fn below(list: &[NodePath], node: &NodePath) -> bool {
        list.iter()
            .any(|site| site.components().starts_with(node.components()))
    }

    /// Whether a statement with no nested block executes a wait before it
    /// completes or leaves: a call inside a `return`'s value or a `propagate`
    /// initializer runs before the edge it takes.
    fn in_statement(&self, node: &NodePath) -> bool {
        Self::below(self.must_wait_calls, node) || Self::below(self.joins, node)
    }

    fn in_scrutinee(&self, scrutinee: &CheckedExpression) -> bool {
        matches!(
            scrutinee,
            CheckedExpression::UserCall { call, .. } if Self::below(self.must_wait_calls, call)
        )
    }
}

/// [TERM-1] whether every path through `statements` that can reach the
/// loop's header again executes a wait.
pub(super) fn iteration_waits(statements: &[CheckedStatement], sites: &WaitSites<'_>) -> bool {
    for statement in statements {
        if iteration_statement_waits(statement, sites) {
            return true;
        }
        if matches!(
            statement,
            CheckedStatement::Break { .. } | CheckedStatement::Return { .. }
        ) {
            return true;
        }
    }
    false
}

fn iteration_statement_waits(statement: &CheckedStatement, sites: &WaitSites<'_>) -> bool {
    match statement {
        CheckedStatement::Let { node_path, .. }
        | CheckedStatement::DestructuringLet { node_path, .. }
        | CheckedStatement::PropagateLet { node_path, .. }
        | CheckedStatement::Set { node_path, .. }
        | CheckedStatement::Evaluate { node_path, .. }
        | CheckedStatement::DropExpression { node_path, .. } => sites.in_statement(node_path),
        // An unguarded atomic statement waits only for its own point of
        // effect, never for another context's step [SHARE-3].
        CheckedStatement::Atomic { guard, .. } => guard.is_some(),
        CheckedStatement::Match {
            scrutinee, arms, ..
        } => {
            sites.in_scrutinee(scrutinee)
                || arms.iter().all(|arm| iteration_waits(&arm.body, sites))
        }
        _ => false,
    }
}

/// What the paths through one block do: whether every exit of the function
/// they take follows a wait, whether they reach the block's end and have
/// waited when they do, and the `break` edges they take with the same fact.
struct Paths {
    exits_wait: bool,
    end: Option<bool>,
    breaks: Vec<(CheckedLoopId, bool)>,
}

/// [WAIT-1] whether every path from the start of a function body to each of
/// its exits, a `return` or a propagating exit, executes a wait.
pub(super) fn body_must_wait(statements: &[CheckedStatement], sites: &WaitSites<'_>) -> bool {
    let paths = block_paths(statements, false, sites);
    paths.exits_wait && paths.end.is_none_or(|waited| waited)
}

fn block_paths(statements: &[CheckedStatement], entry: bool, sites: &WaitSites<'_>) -> Paths {
    let mut paths = Paths {
        exits_wait: true,
        end: Some(entry),
        breaks: Vec::new(),
    };
    for statement in statements {
        let Some(waited) = paths.end else {
            break;
        };
        let step = statement_paths(statement, waited, sites);
        paths.exits_wait &= step.exits_wait;
        paths.breaks.extend(step.breaks);
        paths.end = step.end;
    }
    paths
}

fn statement_paths(statement: &CheckedStatement, waited: bool, sites: &WaitSites<'_>) -> Paths {
    let straight = |end| Paths {
        exits_wait: true,
        end,
        breaks: Vec::new(),
    };
    match statement {
        CheckedStatement::Let { node_path, .. }
        | CheckedStatement::DestructuringLet { node_path, .. }
        | CheckedStatement::Set { node_path, .. }
        | CheckedStatement::Evaluate { node_path, .. }
        | CheckedStatement::DropExpression { node_path, .. } => {
            straight(Some(waited || sites.in_statement(node_path)))
        }
        CheckedStatement::Proof(_) => straight(Some(waited)),
        CheckedStatement::PropagateLet { node_path, .. } => {
            let after = waited || sites.in_statement(node_path);
            Paths {
                exits_wait: after,
                end: Some(after),
                breaks: Vec::new(),
            }
        }
        CheckedStatement::Return { node_path, .. } => Paths {
            exits_wait: waited || sites.in_statement(node_path),
            end: None,
            breaks: Vec::new(),
        },
        // A `give` ends its arm and delivers to the value initializer's end.
        CheckedStatement::Give { node_path, .. } => {
            straight(Some(waited || sites.in_statement(node_path)))
        }
        CheckedStatement::Break { target, .. } => Paths {
            exits_wait: true,
            end: None,
            breaks: vec![(*target, waited)],
        },
        CheckedStatement::Match {
            scrutinee, arms, ..
        }
        | CheckedStatement::ValueMatchLet {
            scrutinee, arms, ..
        } => {
            let entry = waited || sites.in_scrutinee(scrutinee);
            let mut joined = Paths {
                exits_wait: true,
                end: None,
                breaks: Vec::new(),
            };
            for arm in arms {
                let arm = block_paths(&arm.body, entry, sites);
                joined.exits_wait &= arm.exits_wait;
                joined.breaks.extend(arm.breaks);
                joined.end = match (joined.end, arm.end) {
                    (Some(left), Some(right)) => Some(left && right),
                    (left, right) => left.or(right),
                };
            }
            joined
        }
        // Every later iteration starts having waited at least as much as the
        // first, so the first iteration's paths decide every exit; the loop
        // ends only by a `break` that leaves it.
        CheckedStatement::Loop { id, body, .. } => {
            let inner = block_paths(body, waited, sites);
            leave_loop(*id, inner, None)
        }
        // A counted loop may run its body no time at all, so its end is
        // reached having waited only as much as it was entered.
        CheckedStatement::CountedRange { id, body, .. } => {
            let inner = block_paths(body, waited, sites);
            leave_loop(*id, inner, Some(waited))
        }
        CheckedStatement::Atomic { guard, body, .. } => {
            block_paths(body, waited || guard.is_some(), sites)
        }
    }
}

/// The paths after a loop: its own `break` edges and, for a counted loop,
/// its normal completion reach its end; other `break` edges pass outward.
fn leave_loop(id: CheckedLoopId, inner: Paths, completion: Option<bool>) -> Paths {
    let mut end = completion;
    let mut breaks = Vec::new();
    for (target, waited) in inner.breaks {
        if target == id {
            end = Some(end.map_or(waited, |other| other && waited));
        } else {
            breaks.push((target, waited));
        }
    }
    Paths {
        exits_wait: inner.exits_wait,
        end,
        breaks,
    }
}

/// [DIAG-1] the repair for a waiting kind whose body executes no wait.
pub(crate) const WAIT1_REMOVE_THE_WAITING_KIND: &str = "the body executes no wait: remove the waiting kind after the effect row, or call a waiting function in the body";

/// [DIAG-1] the repair for `must_wait` over a body with a path that waits
/// nowhere.
pub(crate) const WAIT1_WRITE_MAY_WAIT: &str = "a path from the body's start to an exit executes no wait: write `may_wait`, or execute a wait on that path before its exit";

/// [DIAG-1] the repair for `may_wait` over a body that waits on every path.
pub(crate) const WAIT1_WRITE_MUST_WAIT: &str = "every path from the body's start to an exit executes a wait: write `must_wait`, so that every call of the function is a wait";

impl Checker<'_, '_> {
    /// [WAIT-1] the declared waiting kind against the body: a waiting
    /// function executes a wait, and it writes `must_wait` exactly when every
    /// path from its body's start to an exit executes one.
    pub(in crate::semantic::check) fn check_wait_kind(
        &self,
        signature: &FunctionSignature,
        statements: &[CheckedStatement],
    ) -> Result<(), CheckStop> {
        if !signature.waits {
            return Ok(());
        }
        let declared = if signature.must_wait {
            "must_wait"
        } else {
            "may_wait"
        };
        let (body, mechanical_fix) = if self.body.waiting.calls.is_empty() {
            ("no wait", WAIT1_REMOVE_THE_WAITING_KIND)
        } else {
            let sites = WaitSites {
                must_wait_calls: &self.body.must_wait_calls,
                joins: &self.body.waiting.context_starts,
            };
            match (body_must_wait(statements, &sites), signature.must_wait) {
                (true, true) | (false, false) => return Ok(()),
                (false, true) => (
                    "a path to an exit that executes no wait",
                    WAIT1_WRITE_MAY_WAIT,
                ),
                (true, false) => ("a wait on every path to an exit", WAIT1_WRITE_MUST_WAIT),
            }
        };
        self.types.declarations.issue_node(
            SemanticRule::Wait1,
            signature.node,
            SemanticIssueKind::WaitKindMismatch {
                declared,
                body,
                mechanical_fix,
            },
        )
    }
}
