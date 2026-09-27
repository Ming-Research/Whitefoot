use crate::syntax::grammar::{
    Decision, DecisionKind, GrammarNodeId, GrammarNodeKind, LookaheadPredicate, Production,
    grammar_node,
};
use crate::syntax::terminal::TerminalPredicate;

use crate::ClassifiedToken;

use super::outcome::{
    FinalizeCompilerFailure, FinalizeLimit, FinalizeLimits, FinalizeResourceFailure,
    FinalizeStorage,
};
use super::topology::{FinalizedExtent, NodeId};

#[derive(Clone, Copy, Debug)]
pub(crate) enum CompletedKind {
    Terminal {
        predicate: TerminalPredicate,
    },
    Production {
        production: Production,
        node: NodeId,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Completed {
    pub(crate) kind: CompletedKind,
    pub(crate) element_start: usize,
    pub(crate) element_end: usize,
    pub(crate) first_terminal: u64,
    pub(crate) first_local_terminal: u64,
    pub(crate) terminal_count: u64,
    pub(crate) extent: FinalizedExtent,
}

pub(crate) struct FinalizeWork {
    used: u64,
    maximum: u64,
}

impl FinalizeWork {
    pub(crate) const fn new(maximum: u64) -> Self {
        Self { used: 0, maximum }
    }

    pub(crate) fn spend(&mut self, amount: u64) -> Result<(), FinalizeResourceFailure> {
        let actual =
            self.used
                .checked_add(amount)
                .ok_or(FinalizeResourceFailure::LimitExceeded {
                    limit: FinalizeLimit::Work,
                    maximum: self.maximum,
                    actual: u64::MAX,
                })?;
        if actual > self.maximum {
            return Err(FinalizeResourceFailure::LimitExceeded {
                limit: FinalizeLimit::Work,
                maximum: self.maximum,
                actual,
            });
        }
        self.used = actual;
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ShapeTask {
    Execute(GrammarNodeId),
    Continue(GrammarNodeId),
    Terminal(TerminalPredicate),
    Production(Production),
}

fn requested_next(len: usize) -> Result<u64, FinalizeResourceFailure> {
    u64::try_from(len)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(FinalizeResourceFailure::AddressSpaceExceeded {
            storage: FinalizeStorage::ShapeTasks,
            requested: u64::MAX,
        })
}

fn push_task(
    tasks: &mut Vec<ShapeTask>,
    task: ShapeTask,
    limits: FinalizeLimits,
) -> Result<(), FinalizeResourceFailure> {
    let actual = requested_next(tasks.len())?;
    if actual > limits.max_shape_tasks {
        return Err(FinalizeResourceFailure::LimitExceeded {
            limit: FinalizeLimit::ShapeTasks,
            maximum: limits.max_shape_tasks,
            actual,
        });
    }
    tasks
        .try_reserve(1)
        .map_err(|_| FinalizeResourceFailure::StorageUnavailable {
            storage: FinalizeStorage::ShapeTasks,
            requested: actual,
        })?;
    tasks.push(task);
    Ok(())
}

fn accepts(
    predicate: LookaheadPredicate,
    tokens: &[ClassifiedToken],
    cursor: usize,
    offset: usize,
) -> Result<bool, FinalizeCompilerFailure> {
    let index = cursor
        .checked_add(offset)
        .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
    Ok(match (tokens.get(index), predicate) {
        (Some(token), LookaheadPredicate::Terminal(expected)) => {
            token.terminals().contains(expected)
        }
        (None, LookaheadPredicate::SourceEnd) => true,
        _ => false,
    })
}

fn select_arm(
    decision: Decision,
    tokens: &[ClassifiedToken],
    cursor: usize,
    work: &mut FinalizeWork,
) -> Result<Option<u8>, ShapeFailure> {
    let mut selected = None;
    for row in decision.rows() {
        work.spend(1)?;
        let first = row
            .position(0)
            .ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
        let second = row
            .position(1)
            .ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
        if !accepts(first.predicate(), tokens, cursor, 0)?
            || !accepts(second.predicate(), tokens, cursor, 1)?
        {
            continue;
        }
        match selected {
            Some(arm) if arm != row.arm() => {
                return Err(FinalizeCompilerFailure::InvalidGrammarData.into());
            }
            Some(_) => {}
            None => selected = Some(row.arm()),
        }
    }
    Ok(selected)
}

fn selected_node(
    node_id: GrammarNodeId,
    decision: Decision,
    arm: u8,
) -> Result<Option<GrammarNodeId>, FinalizeCompilerFailure> {
    let node = grammar_node(node_id).ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
    match decision.kind() {
        DecisionKind::Choice => node
            .children()
            .get(usize::from(arm))
            .copied()
            .map(Some)
            .ok_or(FinalizeCompilerFailure::InvalidGrammarData),
        DecisionKind::Optional | DecisionKind::Repeat0 | DecisionKind::Repeat1 => match arm {
            0 => node
                .children()
                .first()
                .copied()
                .map(Some)
                .ok_or(FinalizeCompilerFailure::InvalidGrammarData),
            1 => Ok(None),
            _ => Err(FinalizeCompilerFailure::InvalidGrammarData),
        },
    }
}

enum ShapeFailure {
    Resource(FinalizeResourceFailure),
    Compiler(FinalizeCompilerFailure),
}

impl From<FinalizeResourceFailure> for ShapeFailure {
    fn from(value: FinalizeResourceFailure) -> Self {
        Self::Resource(value)
    }
}

impl From<FinalizeCompilerFailure> for ShapeFailure {
    fn from(value: FinalizeCompilerFailure) -> Self {
        Self::Compiler(value)
    }
}

pub(crate) enum ShapeResult {
    Complete,
    Resource(FinalizeResourceFailure),
    Compiler(FinalizeCompilerFailure),
}

fn verify(
    production: Production,
    prelude_signature: bool,
    children: &[Completed],
    source_tokens: &[ClassifiedToken],
    tasks: &mut Vec<ShapeTask>,
    limits: FinalizeLimits,
    work: &mut FinalizeWork,
) -> Result<(), ShapeFailure> {
    let mut token_cursor = usize::try_from(
        children
            .first()
            .ok_or(FinalizeCompilerFailure::InvalidProductionShape)?
            .first_local_terminal,
    )
    .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?;
    let mut child_cursor = 0_usize;
    tasks.clear();
    // A [PRE-1] declaration head uses an internal `fn_sig` node with the
    // `fn_decl` header; see `grammar::prelude_signature_children`.
    if prelude_signature {
        let spliced = crate::syntax::grammar::prelude_signature_children()
            .ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
        for child in spliced.iter().rev() {
            push_task(tasks, ShapeTask::Execute(*child), limits)?;
        }
    } else {
        push_task(tasks, ShapeTask::Execute(production.root()), limits)?;
    }
    while let Some(task) = tasks.pop() {
        work.spend(1)?;
        match task {
            ShapeTask::Terminal(expected) => {
                let Some(Completed {
                    kind: CompletedKind::Terminal { predicate, .. },
                    ..
                }) = children.get(child_cursor)
                else {
                    return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
                };
                if *predicate != expected {
                    return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
                }
                child_cursor = child_cursor
                    .checked_add(1)
                    .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
                token_cursor = token_cursor
                    .checked_add(1)
                    .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
            }
            ShapeTask::Production(expected) => {
                let Some(Completed {
                    kind:
                        CompletedKind::Production {
                            production: actual, ..
                        },
                    terminal_count,
                    ..
                }) = children.get(child_cursor)
                else {
                    return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
                };
                if *actual != expected {
                    return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
                }
                child_cursor = child_cursor
                    .checked_add(1)
                    .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
                token_cursor = token_cursor
                    .checked_add(
                        usize::try_from(*terminal_count)
                            .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?,
                    )
                    .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
            }
            ShapeTask::Execute(node_id) => {
                let node =
                    grammar_node(node_id).ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
                match node.kind() {
                    GrammarNodeKind::Production(nested) => {
                        push_task(tasks, ShapeTask::Production(nested), limits)?;
                    }
                    GrammarNodeKind::TerminalSequence => {
                        for terminal in node.terminals().iter().rev() {
                            let LookaheadPredicate::Terminal(predicate) = terminal else {
                                return Err(FinalizeCompilerFailure::InvalidGrammarData.into());
                            };
                            push_task(tasks, ShapeTask::Terminal(*predicate), limits)?;
                        }
                    }
                    GrammarNodeKind::Sequence => {
                        for child in node.children().iter().rev() {
                            push_task(tasks, ShapeTask::Execute(*child), limits)?;
                        }
                    }
                    GrammarNodeKind::Group => {
                        let child = node
                            .children()
                            .first()
                            .copied()
                            .ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
                        push_task(tasks, ShapeTask::Execute(child), limits)?;
                    }
                    GrammarNodeKind::RepeatOne => {
                        let child = node
                            .children()
                            .first()
                            .copied()
                            .ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
                        push_task(tasks, ShapeTask::Continue(node_id), limits)?;
                        push_task(tasks, ShapeTask::Execute(child), limits)?;
                    }
                    GrammarNodeKind::Choice
                    | GrammarNodeKind::Optional
                    | GrammarNodeKind::RepeatZero => {
                        let decision = node
                            .decision()
                            .copied()
                            .ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
                        let arm = select_arm(decision, source_tokens, token_cursor, work)?
                            .ok_or(FinalizeCompilerFailure::InvalidProductionShape)?;
                        if let Some(selected) = selected_node(node_id, decision, arm)? {
                            if decision.kind() == DecisionKind::Repeat0 {
                                push_task(tasks, ShapeTask::Continue(node_id), limits)?;
                            }
                            push_task(tasks, ShapeTask::Execute(selected), limits)?;
                        }
                    }
                }
            }
            ShapeTask::Continue(node_id) => {
                let node =
                    grammar_node(node_id).ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
                let decision = node
                    .decision()
                    .copied()
                    .ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
                let arm = select_arm(decision, source_tokens, token_cursor, work)?
                    .ok_or(FinalizeCompilerFailure::InvalidProductionShape)?;
                match arm {
                    0 => {
                        let child = node
                            .children()
                            .first()
                            .copied()
                            .ok_or(FinalizeCompilerFailure::InvalidGrammarData)?;
                        push_task(tasks, ShapeTask::Continue(node_id), limits)?;
                        push_task(tasks, ShapeTask::Execute(child), limits)?;
                    }
                    1 => {}
                    _ => return Err(FinalizeCompilerFailure::InvalidGrammarData.into()),
                }
            }
        }
    }
    if child_cursor != children.len() {
        return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
    }
    Ok(())
}

pub(crate) fn verify_production_shape(
    production: Production,
    prelude_signature: bool,
    children: &[Completed],
    source_tokens: &[ClassifiedToken],
    tasks: &mut Vec<ShapeTask>,
    limits: FinalizeLimits,
    work: &mut FinalizeWork,
) -> ShapeResult {
    match verify(
        production,
        prelude_signature,
        children,
        source_tokens,
        tasks,
        limits,
        work,
    ) {
        Ok(()) => ShapeResult::Complete,
        Err(ShapeFailure::Resource(failure)) => ShapeResult::Resource(failure),
        Err(ShapeFailure::Compiler(failure)) => ShapeResult::Compiler(failure),
    }
}
