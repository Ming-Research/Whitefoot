use crate::syntax::grammar::{
    DecisionKind, GrammarNodeId, GrammarNodeKind, LookaheadPredicate, Production,
    SYNTAX_DATA_SPEC_HASH, grammar_node,
};
use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};
use crate::{ByteOffset, SourceId};

use crate::{ClassifiedBundle, ClassifiedToken};

use super::tree::DerivationExtent;
use super::{
    DecisionSelection, DerivationElement, DerivationTree, DiagnosticResult, DiagnosticSite, Frame,
    ParseCompilerFailure, ParseInvocationFailure, ParseLimit, ParseLimits, ParseOutcome,
    ParseResourceFailure, ParseStorage, ParsedBundle, ProbeContext, Work, diagnose_decision,
    direct_mismatch, select_arm,
};

#[derive(Clone, Copy)]
enum Task {
    Execute(GrammarNodeId),
    Continue(GrammarNodeId),
    Match(TerminalPredicate),
    Finish(Production),
}

enum Stop {
    Source(super::SyntaxIssue),
    Resource(ParseResourceFailure),
    Compiler(ParseCompilerFailure),
}

impl From<DiagnosticResult> for Stop {
    fn from(result: DiagnosticResult) -> Self {
        match result {
            DiagnosticResult::Issue(issue) => Self::Source(issue),
            DiagnosticResult::Resource(failure) => Self::Resource(failure),
            DiagnosticResult::Compiler(failure) => Self::Compiler(failure),
        }
    }
}

struct Parser<'classified> {
    classified: &'classified ClassifiedBundle,
    limits: ParseLimits,
    work: Work,
    tasks: Vec<Task>,
    frames: Vec<Frame>,
    elements: Vec<DerivationElement>,
    terminal_count: u64,
    production_count: u64,
    /// The grammar start deriving every source of this bundle: `program`
    /// for interface and implementation records, `graph_file` for the graph.
    start: Production,
}

impl<'classified> Parser<'classified> {
    fn new(
        classified: &'classified ClassifiedBundle,
        limits: ParseLimits,
        start: Production,
    ) -> Self {
        Self {
            classified,
            limits,
            work: Work::new(limits.max_work),
            tasks: Vec::new(),
            frames: Vec::new(),
            elements: Vec::new(),
            terminal_count: 0,
            production_count: 0,
            start,
        }
    }

    fn requested_next(len: usize, storage: ParseStorage) -> Result<u64, ParseResourceFailure> {
        u64::try_from(len)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(ParseResourceFailure::AddressSpaceExceeded {
                storage,
                requested: u64::MAX,
            })
    }

    fn push_task(&mut self, task: Task) -> Result<(), Stop> {
        let actual =
            Self::requested_next(self.tasks.len(), ParseStorage::Tasks).map_err(Stop::Resource)?;
        if actual > self.limits.max_tasks {
            return Err(Stop::Resource(ParseResourceFailure::LimitExceeded {
                limit: ParseLimit::Tasks,
                maximum: self.limits.max_tasks,
                actual,
            }));
        }
        self.tasks.try_reserve(1).map_err(|_| {
            Stop::Resource(ParseResourceFailure::StorageUnavailable {
                storage: ParseStorage::Tasks,
                requested: actual,
            })
        })?;
        self.tasks.push(task);
        Ok(())
    }

    fn push_frame(&mut self, production: Production, atom_only: bool) -> Result<(), Stop> {
        let actual = Self::requested_next(self.frames.len(), ParseStorage::Frames)
            .map_err(Stop::Resource)?;
        if actual > self.limits.max_frames {
            return Err(Stop::Resource(ParseResourceFailure::LimitExceeded {
                limit: ParseLimit::Frames,
                maximum: self.limits.max_frames,
                actual,
            }));
        }
        self.frames.try_reserve(1).map_err(|_| {
            Stop::Resource(ParseResourceFailure::StorageUnavailable {
                storage: ParseStorage::Frames,
                requested: actual,
            })
        })?;
        let in_contract = self.frames.last().is_some_and(|frame| frame.in_contract)
            || production == Production::ContractBlock;
        self.frames.push(Frame {
            production,
            element_start: self.elements.len(),
            child_count: 0,
            extent: None,
            atom_only,
            in_contract,
        });
        Ok(())
    }

    fn push_element(&mut self, element: DerivationElement) -> Result<(), Stop> {
        let actual = Self::requested_next(self.elements.len(), ParseStorage::Elements)
            .map_err(Stop::Resource)?;
        if actual > self.limits.max_elements {
            return Err(Stop::Resource(ParseResourceFailure::LimitExceeded {
                limit: ParseLimit::Elements,
                maximum: self.limits.max_elements,
                actual,
            }));
        }
        self.elements.try_reserve(1).map_err(|_| {
            Stop::Resource(ParseResourceFailure::StorageUnavailable {
                storage: ParseStorage::Elements,
                requested: actual,
            })
        })?;
        self.elements.push(element);
        Ok(())
    }

    fn current_context(&self) -> Result<ProbeContext, Stop> {
        let frame = self
            .frames
            .last()
            .ok_or(Stop::Compiler(ParseCompilerFailure::MissingProductionFrame))?;
        Ok(ProbeContext {
            production: frame.production,
            atom_only: frame.atom_only,
            in_contract: frame.in_contract,
        })
    }

    fn note_child_extent(
        frame: &mut Frame,
        source: SourceId,
        start: ByteOffset,
        end: ByteOffset,
    ) -> Result<(), Stop> {
        if frame.production.is_start() {
            return Ok(());
        }
        match frame.extent {
            None => frame.extent = Some((source, start, end)),
            Some((existing_source, existing_start, existing_end)) => {
                if existing_source != source {
                    return Err(Stop::Compiler(ParseCompilerFailure::CrossSourceProduction));
                }
                if start < existing_start || end < existing_end {
                    return Err(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData));
                }
                frame.extent = Some((source, existing_start, end));
            }
        }
        Ok(())
    }

    fn increment_child(frame: &mut Frame) -> Result<(), Stop> {
        frame.child_count = frame
            .child_count
            .checked_add(1)
            .ok_or(Stop::Compiler(ParseCompilerFailure::CounterOverflow))?;
        Ok(())
    }

    fn append_terminal(
        &mut self,
        token: ClassifiedToken,
        predicate: TerminalPredicate,
        source: SourceId,
    ) -> Result<(), Stop> {
        let id = token.token().id();
        if id.source() != source {
            return Err(Stop::Compiler(ParseCompilerFailure::TokenSourceMismatch));
        }
        let frame = self
            .frames
            .last_mut()
            .ok_or(Stop::Compiler(ParseCompilerFailure::MissingProductionFrame))?;
        Self::increment_child(frame)?;
        Self::note_child_extent(frame, source, id.start(), id.end())?;
        self.push_element(DerivationElement::Terminal {
            token: token.token(),
            predicate,
        })?;
        self.terminal_count = self
            .terminal_count
            .checked_add(1)
            .ok_or(Stop::Compiler(ParseCompilerFailure::CounterOverflow))?;
        Ok(())
    }

    fn finish_production(&mut self, expected: Production) -> Result<(), Stop> {
        let frame = self
            .frames
            .pop()
            .ok_or(Stop::Compiler(ParseCompilerFailure::MissingProductionFrame))?;
        if frame.production != expected {
            return Err(Stop::Compiler(
                ParseCompilerFailure::ProductionFrameMismatch,
            ));
        }
        let extent = if expected.is_start() {
            DerivationExtent::BundleRoot
        } else {
            let (source, start, end) = frame.extent.ok_or(Stop::Compiler(
                ParseCompilerFailure::MissingProductionExtent,
            ))?;
            DerivationExtent::Source { source, start, end }
        };
        let before_parent = self.elements.len();
        let subtree_elements = before_parent
            .checked_sub(frame.element_start)
            .and_then(|value| value.checked_add(1))
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(Stop::Compiler(ParseCompilerFailure::CounterOverflow))?;
        self.push_element(DerivationElement::Production {
            production: expected,
            child_count: frame.child_count,
            subtree_elements,
            extent,
        })?;
        self.production_count = self
            .production_count
            .checked_add(1)
            .ok_or(Stop::Compiler(ParseCompilerFailure::CounterOverflow))?;
        if let Some(parent) = self.frames.last_mut() {
            Self::increment_child(parent)?;
            if let DerivationExtent::Source { source, start, end } = extent {
                Self::note_child_extent(parent, source, start, end)?;
            }
        }
        Ok(())
    }

    fn begin_production(&mut self, production: Production, atom_only: bool) -> Result<(), Stop> {
        if production.is_start() {
            return Err(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData));
        }
        self.push_frame(production, atom_only)?;
        self.push_task(Task::Finish(production))?;
        self.push_task(Task::Execute(production.root()))?;
        Ok(())
    }

    fn schedule_selected(
        &mut self,
        node_id: GrammarNodeId,
        kind: DecisionKind,
        arm: u8,
    ) -> Result<(), Stop> {
        let node = grammar_node(node_id)
            .ok_or(Stop::Compiler(ParseCompilerFailure::MissingGrammarNode))?;
        match kind {
            DecisionKind::Choice => {
                let child = node
                    .children()
                    .get(usize::from(arm))
                    .copied()
                    .ok_or(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData))?;
                self.push_task(Task::Execute(child))
            }
            DecisionKind::Optional => match arm {
                0 => {
                    let child = node
                        .children()
                        .first()
                        .copied()
                        .ok_or(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData))?;
                    self.push_task(Task::Execute(child))
                }
                1 => Ok(()),
                _ => Err(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData)),
            },
            DecisionKind::Repeat0 | DecisionKind::Repeat1 => match arm {
                0 => {
                    let child = node
                        .children()
                        .first()
                        .copied()
                        .ok_or(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData))?;
                    self.push_task(Task::Continue(node_id))?;
                    self.push_task(Task::Execute(child))
                }
                1 => Ok(()),
                _ => Err(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData)),
            },
        }
    }

    fn failed_decision(
        &mut self,
        decision: crate::syntax::grammar::Decision,
        source: SourceId,
        source_len: u64,
        tokens: &[ClassifiedToken],
        cursor: usize,
    ) -> Stop {
        let context = match self.current_context() {
            Ok(context) => context,
            Err(stop) => return stop,
        };
        diagnose_decision(
            decision,
            context,
            DiagnosticSite {
                source,
                source_len,
                tokens,
                cursor,
                limits: self.limits,
            },
            &mut self.work,
        )
        .into()
    }

    fn execute_node(
        &mut self,
        node_id: GrammarNodeId,
        source: SourceId,
        source_len: u64,
        tokens: &[ClassifiedToken],
        cursor: usize,
    ) -> Result<(), Stop> {
        let node = grammar_node(node_id)
            .ok_or(Stop::Compiler(ParseCompilerFailure::MissingGrammarNode))?;
        match node.kind() {
            GrammarNodeKind::Production(production) => {
                self.begin_production(production, node.is_atom_only_reference())
            }
            GrammarNodeKind::TerminalSequence => {
                for terminal in node.terminals().iter().rev() {
                    let LookaheadPredicate::Terminal(predicate) = terminal else {
                        return Err(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData));
                    };
                    self.push_task(Task::Match(*predicate))?;
                }
                Ok(())
            }
            GrammarNodeKind::Sequence => {
                for child in node.children().iter().rev() {
                    self.push_task(Task::Execute(*child))?;
                }
                Ok(())
            }
            GrammarNodeKind::Group => {
                let child = node
                    .children()
                    .first()
                    .copied()
                    .ok_or(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData))?;
                self.push_task(Task::Execute(child))
            }
            GrammarNodeKind::RepeatOne => {
                let child = node
                    .children()
                    .first()
                    .copied()
                    .ok_or(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData))?;
                self.push_task(Task::Continue(node_id))?;
                self.push_task(Task::Execute(child))
            }
            GrammarNodeKind::Choice | GrammarNodeKind::Optional | GrammarNodeKind::RepeatZero => {
                let decision = node
                    .decision()
                    .copied()
                    .ok_or(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData))?;
                match select_arm(decision, tokens, cursor, &mut self.work) {
                    Ok(DecisionSelection::Arm(arm)) => {
                        self.schedule_selected(node_id, decision.kind(), arm)
                    }
                    Ok(DecisionSelection::NoMatch) => {
                        Err(self.failed_decision(decision, source, source_len, tokens, cursor))
                    }
                    Ok(DecisionSelection::Conflict) => {
                        Err(Stop::Compiler(ParseCompilerFailure::PredictiveConflict))
                    }
                    Err(result) => Err(result.into()),
                }
            }
        }
    }

    /// Begins one [PRE-1] declaration head, represented as a `fn_sig` node
    /// with the `fn_decl` generic header the record specifies.
    ///
    /// The generated child sequence is stated at
    /// `grammar::prelude_signature_children`.
    fn begin_prelude_signature(&mut self) -> Result<(), Stop> {
        let children = crate::syntax::grammar::prelude_signature_children()
            .ok_or(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData))?;
        self.push_frame(Production::FnSig, false)?;
        self.push_task(Task::Finish(Production::FnSig))?;
        for child in children.iter().rev() {
            self.push_task(Task::Execute(*child))?;
        }
        Ok(())
    }

    fn parse_source(
        &mut self,
        source: SourceId,
        source_len: u64,
        tokens: &[ClassifiedToken],
        declaration: Option<crate::source::PreludeSource>,
    ) -> Result<(), Stop> {
        if declaration == Some(crate::source::PreludeSource::Function) {
            self.push_frame(Production::Item, false)?;
            self.push_task(Task::Finish(Production::Item))?;
            self.push_task(Task::Match(TerminalPredicate::Fixed(
                FixedTerminal::Semicolon,
            )))?;
            self.begin_prelude_signature()?;
        } else {
            self.push_task(Task::Execute(self.start.root()))?;
        }
        let mut cursor = 0_usize;
        while let Some(task) = self.tasks.pop() {
            self.work.spend(1).map_err(Stop::Resource)?;
            match task {
                Task::Execute(node) => {
                    self.execute_node(node, source, source_len, tokens, cursor)?;
                }
                Task::Continue(node_id) => {
                    let node = grammar_node(node_id)
                        .ok_or(Stop::Compiler(ParseCompilerFailure::MissingGrammarNode))?;
                    let decision = node
                        .decision()
                        .copied()
                        .ok_or(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData))?;
                    match select_arm(decision, tokens, cursor, &mut self.work) {
                        Ok(DecisionSelection::Arm(arm)) => {
                            self.schedule_selected(node_id, decision.kind(), arm)?;
                        }
                        Ok(DecisionSelection::NoMatch) => {
                            return Err(
                                self.failed_decision(decision, source, source_len, tokens, cursor)
                            );
                        }
                        Ok(DecisionSelection::Conflict) => {
                            return Err(Stop::Compiler(ParseCompilerFailure::PredictiveConflict));
                        }
                        Err(result) => return Err(result.into()),
                    }
                }
                Task::Match(expected) => {
                    let Some(token) = tokens.get(cursor).copied() else {
                        let context = self.current_context()?;
                        return Err(direct_mismatch(
                            expected,
                            context,
                            DiagnosticSite {
                                source,
                                source_len,
                                tokens,
                                cursor,
                                limits: self.limits,
                            },
                            &mut self.work,
                        )
                        .into());
                    };
                    if !token.terminals().contains(expected) {
                        let context = self.current_context()?;
                        return Err(direct_mismatch(
                            expected,
                            context,
                            DiagnosticSite {
                                source,
                                source_len,
                                tokens,
                                cursor,
                                limits: self.limits,
                            },
                            &mut self.work,
                        )
                        .into());
                    }
                    self.append_terminal(token, expected, source)?;
                    cursor = cursor
                        .checked_add(1)
                        .ok_or(Stop::Compiler(ParseCompilerFailure::CounterOverflow))?;
                }
                Task::Finish(production) => self.finish_production(production)?,
            }
        }
        if cursor != tokens.len() {
            return Err(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData));
        }
        Ok(())
    }

    fn run(mut self) -> Result<DerivationTree, Stop> {
        self.push_frame(self.start, false)?;
        for (source, file) in self.classified.source_bundle().iter() {
            let tokens = self
                .classified
                .source_tokens(source)
                .ok_or(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData))?;
            self.parse_source(source, file.byte_len(), tokens, file.prelude())?;
            if self.frames.len() != 1 || !self.tasks.is_empty() {
                return Err(Stop::Compiler(
                    ParseCompilerFailure::ProductionFrameMismatch,
                ));
            }
        }
        self.finish_production(self.start)?;
        if !self.frames.is_empty() || self.elements.is_empty() {
            return Err(Stop::Compiler(
                ParseCompilerFailure::ProductionFrameMismatch,
            ));
        }
        let mut observed_terminals = 0_u64;
        let mut observed_productions = 0_u64;
        for element in &self.elements {
            match element {
                DerivationElement::Terminal { token, predicate } => {
                    let _source_bound_extent = token.id();
                    let _selected_predicate = *predicate;
                    observed_terminals = observed_terminals
                        .checked_add(1)
                        .ok_or(Stop::Compiler(ParseCompilerFailure::CounterOverflow))?;
                }
                DerivationElement::Production {
                    production,
                    subtree_elements,
                    extent,
                    ..
                } => {
                    if *subtree_elements == 0
                        || matches!(
                            (production, extent),
                            (start, DerivationExtent::Source { .. }) if start.is_start()
                        )
                        || matches!(
                            (production, extent),
                            (other, DerivationExtent::BundleRoot) if !other.is_start()
                        )
                    {
                        return Err(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData));
                    }
                    if let DerivationExtent::Source { source, start, end } = extent
                        && (start > end || self.classified.source_bundle().file(*source).is_none())
                    {
                        return Err(Stop::Compiler(ParseCompilerFailure::InvalidGrammarData));
                    }
                    observed_productions = observed_productions
                        .checked_add(1)
                        .ok_or(Stop::Compiler(ParseCompilerFailure::CounterOverflow))?;
                }
            }
        }
        if observed_terminals != self.terminal_count
            || observed_productions != self.production_count
        {
            return Err(Stop::Compiler(ParseCompilerFailure::CounterOverflow));
        }
        Ok(DerivationTree {
            elements: self.elements,
            terminal_count: self.terminal_count,
            production_count: self.production_count,
        })
    }
}

/// Derives the complete active-specification grammar with an iterative typed LL(2) parser.
///
/// The parser consumes retained predicate sets, never priority-selected token
/// kinds. It performs no recovery, backtracking, semantic lookup, canonical
/// formatting audit, or tree finalization, and no partial derivation escapes a
/// failure outcome.
#[must_use]
pub fn parse(classified: ClassifiedBundle, limits: ParseLimits) -> ParseOutcome {
    if classified.spec_hash() != SYNTAX_DATA_SPEC_HASH {
        return ParseOutcome::InvocationFailure(ParseInvocationFailure::SpecificationMismatch);
    }
    if classified.source_bundle().is_empty() {
        return ParseOutcome::InvocationFailure(ParseInvocationFailure::EmptySourceBundle);
    }
    parse_from(classified, limits, Production::Program)
}

/// Derives one module graph source from the `graph_file` start [GRAM-2, MOD-1].
///
/// The bundle holds exactly the graph record; the derivation root is the
/// `graph_file` node, and the same predictive interpreter, limits and
/// diagnostics apply as for `program` sources.
#[must_use]
pub fn parse_graph(classified: ClassifiedBundle, limits: ParseLimits) -> ParseOutcome {
    if classified.spec_hash() != SYNTAX_DATA_SPEC_HASH {
        return ParseOutcome::InvocationFailure(ParseInvocationFailure::SpecificationMismatch);
    }
    if classified.source_bundle().is_empty() {
        return ParseOutcome::InvocationFailure(ParseInvocationFailure::EmptySourceBundle);
    }
    parse_from(classified, limits, Production::GraphFile)
}

fn parse_from(
    classified: ClassifiedBundle,
    limits: ParseLimits,
    start: Production,
) -> ParseOutcome {
    match Parser::new(&classified, limits, start).run() {
        Ok(tree) => ParseOutcome::Complete(ParsedBundle { classified, tree }),
        Err(Stop::Source(issue)) => ParseOutcome::SourceIssue(issue),
        Err(Stop::Resource(failure)) => ParseOutcome::ResourceFailure(failure),
        Err(Stop::Compiler(failure)) => ParseOutcome::CompilerFailure(failure),
    }
}
