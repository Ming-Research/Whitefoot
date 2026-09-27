use crate::ByteOffset;
use crate::syntax::grammar::Production;
use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};

use crate::syntax::parser::tree::DerivationExtent;
use crate::syntax::parser::{DerivationElement, ParsedBundle};

use super::outcome::{
    BundleSourceExtent, FinalizeCompilerFailure, FinalizeLimit, FinalizeLimits, FinalizeOutcome,
    FinalizeResourceFailure, FinalizeStorage, FinalizedBundle,
};
use super::shape::{
    Completed, CompletedKind, FinalizeWork, ShapeResult, ShapeTask, verify_production_shape,
};
use super::topology::{FinalizedExtent, FinalizedTopology, NodeId, NodeRecord, TerminalRecord};

enum Stop {
    Resource(FinalizeResourceFailure),
    Compiler(FinalizeCompilerFailure),
}

impl From<FinalizeResourceFailure> for Stop {
    fn from(value: FinalizeResourceFailure) -> Self {
        Self::Resource(value)
    }
}

impl From<FinalizeCompilerFailure> for Stop {
    fn from(value: FinalizeCompilerFailure) -> Self {
        Self::Compiler(value)
    }
}

fn requested_next(len: usize, storage: FinalizeStorage) -> Result<u64, Stop> {
    u64::try_from(len)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(
            FinalizeResourceFailure::AddressSpaceExceeded {
                storage,
                requested: u64::MAX,
            }
            .into(),
        )
}

fn check_limit(actual: u64, maximum: u64, limit: FinalizeLimit) -> Result<(), Stop> {
    if actual > maximum {
        return Err(FinalizeResourceFailure::LimitExceeded {
            limit,
            maximum,
            actual,
        }
        .into());
    }
    Ok(())
}

struct Finalizer<'parsed> {
    parsed: &'parsed ParsedBundle,
    limits: FinalizeLimits,
    work: FinalizeWork,
    roots: Vec<Completed>,
    shape_tasks: Vec<ShapeTask>,
    nodes: Vec<NodeRecord>,
    children: Vec<NodeId>,
    terminals: Vec<TerminalRecord>,
    source_extents: Vec<BundleSourceExtent>,
}

impl<'parsed> Finalizer<'parsed> {
    fn new(parsed: &'parsed ParsedBundle, limits: FinalizeLimits) -> Self {
        Self {
            parsed,
            limits,
            work: FinalizeWork::new(limits.max_work),
            roots: Vec::new(),
            shape_tasks: Vec::new(),
            nodes: Vec::new(),
            children: Vec::new(),
            terminals: Vec::new(),
            source_extents: Vec::new(),
        }
    }

    fn push_root(&mut self, root: Completed) -> Result<(), Stop> {
        let actual = requested_next(self.roots.len(), FinalizeStorage::Roots)?;
        check_limit(actual, self.limits.max_roots, FinalizeLimit::Roots)?;
        self.roots.try_reserve(1).map_err(|_| {
            Stop::Resource(FinalizeResourceFailure::StorageUnavailable {
                storage: FinalizeStorage::Roots,
                requested: actual,
            })
        })?;
        self.roots.push(root);
        Ok(())
    }

    fn push_terminal(&mut self, terminal: TerminalRecord) -> Result<(), Stop> {
        let actual = requested_next(self.terminals.len(), FinalizeStorage::Terminals)?;
        check_limit(actual, self.limits.max_terminals, FinalizeLimit::Terminals)?;
        self.terminals.try_reserve(1).map_err(|_| {
            Stop::Resource(FinalizeResourceFailure::StorageUnavailable {
                storage: FinalizeStorage::Terminals,
                requested: actual,
            })
        })?;
        self.terminals.push(terminal);
        Ok(())
    }

    fn push_node(&mut self, node: NodeRecord) -> Result<NodeId, Stop> {
        let actual = requested_next(self.nodes.len(), FinalizeStorage::Nodes)?;
        check_limit(actual, self.limits.max_nodes, FinalizeLimit::Nodes)?;
        let id =
            NodeId::from_index(self.nodes.len()).ok_or(FinalizeCompilerFailure::CounterOverflow)?;
        self.nodes.try_reserve(1).map_err(|_| {
            Stop::Resource(FinalizeResourceFailure::StorageUnavailable {
                storage: FinalizeStorage::Nodes,
                requested: actual,
            })
        })?;
        self.nodes.push(node);
        Ok(id)
    }

    fn push_child(&mut self, child: NodeId) -> Result<(), Stop> {
        let actual = requested_next(self.children.len(), FinalizeStorage::ChildEdges)?;
        check_limit(
            actual,
            self.limits.max_child_edges,
            FinalizeLimit::ChildEdges,
        )?;
        self.children.try_reserve(1).map_err(|_| {
            Stop::Resource(FinalizeResourceFailure::StorageUnavailable {
                storage: FinalizeStorage::ChildEdges,
                requested: actual,
            })
        })?;
        self.children.push(child);
        Ok(())
    }

    fn build_source_extents(&mut self) -> Result<(), Stop> {
        let classified = &self.parsed.classified;
        let source_count = classified.source_bundle().len();
        let expected_offsets = source_count
            .checked_add(1)
            .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
        if classified.source_offsets.len() != expected_offsets
            || classified.source_offsets.first() != Some(&0)
        {
            return Err(FinalizeCompilerFailure::InvalidTokenCoverage.into());
        }
        let mut previous_end = 0_usize;
        for (source, file) in self.parsed.classified.source_bundle().iter() {
            self.work.spend(1)?;
            let source_index = usize::try_from(source.ordinal())
                .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?;
            let start = *classified
                .source_offsets
                .get(source_index)
                .ok_or(FinalizeCompilerFailure::InvalidTokenCoverage)?;
            let end = *classified
                .source_offsets
                .get(
                    source_index
                        .checked_add(1)
                        .ok_or(FinalizeCompilerFailure::CounterOverflow)?,
                )
                .ok_or(FinalizeCompilerFailure::InvalidTokenCoverage)?;
            let partition = classified
                .tokens()
                .get(start..end)
                .ok_or(FinalizeCompilerFailure::InvalidTokenCoverage)?;
            if start != previous_end
                || partition
                    .iter()
                    .any(|token| token.token().id().source() != source)
            {
                return Err(FinalizeCompilerFailure::InvalidTokenCoverage.into());
            }
            self.work.spend(
                u64::try_from(partition.len())
                    .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?,
            )?;
            previous_end = end;
            let actual = requested_next(self.source_extents.len(), FinalizeStorage::SourceExtents)?;
            check_limit(actual, self.limits.max_sources, FinalizeLimit::Sources)?;
            self.source_extents.try_reserve(1).map_err(|_| {
                Stop::Resource(FinalizeResourceFailure::StorageUnavailable {
                    storage: FinalizeStorage::SourceExtents,
                    requested: actual,
                })
            })?;
            self.source_extents.push(BundleSourceExtent::new(
                source,
                ByteOffset::new(file.byte_len()),
            ));
        }
        if previous_end != classified.tokens().len() {
            return Err(FinalizeCompilerFailure::InvalidTokenCoverage.into());
        }
        Ok(())
    }

    fn terminal(
        &mut self,
        element_index: usize,
        token: crate::lexer::Token,
        predicate: TerminalPredicate,
    ) -> Result<(), Stop> {
        let ordinal = u64::try_from(self.terminals.len())
            .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?;
        let index =
            usize::try_from(ordinal).map_err(|_| FinalizeCompilerFailure::CounterOverflow)?;
        let classified = self
            .parsed
            .classified
            .tokens()
            .get(index)
            .ok_or(FinalizeCompilerFailure::InvalidTokenCoverage)?;
        let actual = token.id();
        let expected = classified.token().id();
        if actual.source() != expected.source()
            || actual.start() != expected.start()
            || actual.end() != expected.end()
            || token.kind() != classified.token().kind()
        {
            return Err(FinalizeCompilerFailure::InvalidTokenCoverage.into());
        }
        if !classified.terminals().contains(predicate) {
            return Err(FinalizeCompilerFailure::InvalidTerminalPredicate.into());
        }
        let source_index = usize::try_from(actual.source().ordinal())
            .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?;
        let source_start = *self
            .parsed
            .classified
            .source_offsets
            .get(source_index)
            .ok_or(FinalizeCompilerFailure::InvalidTokenCoverage)?;
        let local = index
            .checked_sub(source_start)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(FinalizeCompilerFailure::InvalidTokenCoverage)?;
        self.push_terminal(TerminalRecord {
            element_index,
            source: actual.source(),
            local_ordinal: local,
            owner: None,
        })?;
        self.push_root(Completed {
            kind: CompletedKind::Terminal { predicate },
            element_start: element_index,
            element_end: element_index,
            first_terminal: ordinal,
            first_local_terminal: local,
            terminal_count: 1,
            extent: FinalizedExtent::Source {
                source: actual.source(),
                start: actual.start(),
                end: actual.end(),
            },
        })
    }

    fn checked_children(&self, element_index: usize, child_count: u32) -> Result<usize, Stop> {
        let count =
            usize::try_from(child_count).map_err(|_| FinalizeCompilerFailure::CounterOverflow)?;
        let start = self
            .roots
            .len()
            .checked_sub(count)
            .ok_or(FinalizeCompilerFailure::InvalidPostorder)?;
        if count == 0 {
            if element_index != 0 {
                return Err(FinalizeCompilerFailure::InvalidPostorder.into());
            }
            return Ok(start);
        }
        let children = &self.roots[start..];
        if children
            .last()
            .is_none_or(|child| child.element_end.checked_add(1) != Some(element_index))
        {
            return Err(FinalizeCompilerFailure::InvalidPostorder.into());
        }
        for pair in children.windows(2) {
            if pair[0].element_end.checked_add(1) != Some(pair[1].element_start) {
                return Err(FinalizeCompilerFailure::InvalidPostorder.into());
            }
        }
        Ok(start)
    }

    fn finalized_extent(
        production: Production,
        children: &[Completed],
    ) -> Result<(FinalizedExtent, u64, u64, u64), Stop> {
        if production.is_start() {
            let terminal_count = children.iter().try_fold(0_u64, |total, child| {
                total
                    .checked_add(child.terminal_count)
                    .ok_or(FinalizeCompilerFailure::CounterOverflow)
            })?;
            return Ok((FinalizedExtent::BundleRoot, 0, 0, terminal_count));
        }
        let first = children
            .first()
            .ok_or(FinalizeCompilerFailure::InvalidSourceExtent)?;
        let last = children
            .last()
            .ok_or(FinalizeCompilerFailure::InvalidSourceExtent)?;
        let FinalizedExtent::Source {
            source,
            start,
            end: _,
        } = first.extent
        else {
            return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
        };
        let mut previous_end = start;
        let mut terminal_count = 0_u64;
        for child in children {
            let FinalizedExtent::Source {
                source: child_source,
                start: child_start,
                end: child_end,
            } = child.extent
            else {
                return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
            };
            if child_source != source || child_start < previous_end || child_end < child_start {
                return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
            }
            previous_end = child_end;
            terminal_count = terminal_count
                .checked_add(child.terminal_count)
                .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
        }
        let FinalizedExtent::Source { end, .. } = last.extent else {
            return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
        };
        if terminal_count == 0 {
            return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
        }
        Ok((
            FinalizedExtent::Source { source, start, end },
            first.first_terminal,
            first.first_local_terminal,
            terminal_count,
        ))
    }

    fn check_declared_extent(
        declared: DerivationExtent,
        actual: FinalizedExtent,
    ) -> Result<(), Stop> {
        let agrees = match (declared, actual) {
            (DerivationExtent::BundleRoot, FinalizedExtent::BundleRoot) => true,
            (
                DerivationExtent::Source {
                    source: left_source,
                    start: left_start,
                    end: left_end,
                },
                FinalizedExtent::Source {
                    source: right_source,
                    start: right_start,
                    end: right_end,
                },
            ) => left_source == right_source && left_start == right_start && left_end == right_end,
            _ => false,
        };
        if !agrees {
            return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
        }
        Ok(())
    }

    /// A graph root holds module rows and then entries, all from its one source.
    fn check_graph_shape(children: &[Completed]) -> Result<(), Stop> {
        let mut graph_source = None;
        for child in children {
            let CompletedKind::Production { production, .. } = child.kind else {
                return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
            };
            if !matches!(production, Production::ModuleRow | Production::EntryDecl) {
                return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
            }
            let FinalizedExtent::Source { source, .. } = child.extent else {
                return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
            };
            if graph_source.is_some_and(|previous| previous != source) {
                return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
            }
            graph_source = Some(source);
        }
        Ok(())
    }

    fn check_program_shape(children: &[Completed]) -> Result<(), Stop> {
        let mut previous_source = None;
        for child in children {
            let CompletedKind::Production { production, .. } = child.kind else {
                return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
            };
            if production != Production::Item {
                return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
            }
            let FinalizedExtent::Source { source, .. } = child.extent else {
                return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
            };
            if previous_source.is_some_and(|previous| source < previous) {
                return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
            }
            previous_source = Some(source);
        }
        Ok(())
    }

    fn production(
        &mut self,
        element_index: usize,
        production: Production,
        child_count: u32,
        subtree_elements: u64,
        declared_extent: DerivationExtent,
    ) -> Result<(), Stop> {
        let root_start = self.checked_children(element_index, child_count)?;
        let element_start = self.roots[root_start..]
            .first()
            .map_or(element_index, |child| child.element_start);
        let observed_subtree = element_index
            .checked_sub(element_start)
            .and_then(|value| value.checked_add(1))
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
        if observed_subtree != subtree_elements {
            return Err(FinalizeCompilerFailure::InvalidPostorder.into());
        }
        let (extent, first_terminal, first_local_terminal, terminal_count) =
            Self::finalized_extent(production, &self.roots[root_start..])?;
        Self::check_declared_extent(declared_extent, extent)?;

        if production == Production::Program {
            Self::check_program_shape(&self.roots[root_start..])?;
        } else if production == Production::GraphFile {
            Self::check_graph_shape(&self.roots[root_start..])?;
        } else {
            let FinalizedExtent::Source { source, .. } = extent else {
                return Err(FinalizeCompilerFailure::InvalidSourceExtent.into());
            };
            let source_tokens = self
                .parsed
                .classified
                .source_tokens(source)
                .ok_or(FinalizeCompilerFailure::InvalidTokenCoverage)?;
            // PRE-1 supplies a declaration head without a source body. Its
            // internal fn_sig subtree includes the fn_decl generic header
            // through `grammar::prelude_signature_children`; the surrounding
            // record consists exactly of that head and its table semicolon.
            let prelude_record = self
                .parsed
                .classified
                .source_bundle()
                .file(source)
                .is_some_and(|file| file.prelude() == Some(crate::source::PreludeSource::Function));
            let prelude_signature_record = production == Production::Item && prelude_record;
            let shape = if prelude_signature_record {
                match &self.roots[root_start..] {
                    [
                        Completed {
                            kind:
                                CompletedKind::Production {
                                    production: Production::FnSig,
                                    ..
                                },
                            ..
                        },
                        Completed {
                            kind:
                                CompletedKind::Terminal {
                                    predicate: TerminalPredicate::Fixed(FixedTerminal::Semicolon),
                                },
                            ..
                        },
                    ] => ShapeResult::Complete,
                    _ => ShapeResult::Compiler(FinalizeCompilerFailure::InvalidProductionShape),
                }
            } else {
                verify_production_shape(
                    production,
                    prelude_record && production == Production::FnSig,
                    &self.roots[root_start..],
                    source_tokens,
                    &mut self.shape_tasks,
                    self.limits,
                    &mut self.work,
                )
            };
            match shape {
                ShapeResult::Complete => {}
                ShapeResult::Resource(failure) => return Err(failure.into()),
                ShapeResult::Compiler(failure) => return Err(failure.into()),
            }
        }

        let child_start = u32::try_from(self.children.len())
            .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?;
        let production_children = self.roots[root_start..]
            .iter()
            .filter(|child| matches!(child.kind, CompletedKind::Production { .. }))
            .count();
        let node_id =
            NodeId::from_index(self.nodes.len()).ok_or(FinalizeCompilerFailure::CounterOverflow)?;
        let mut body_open = None;
        let mut body_close = None;
        let mut else_open = None;
        let mut else_close = None;
        let mut has_else = false;
        let mut production_ordinal = 0_u32;
        for index in root_start..self.roots.len() {
            let child = self.roots[index];
            match child.kind {
                CompletedKind::Terminal { predicate, .. } => {
                    let index = usize::try_from(child.first_terminal)
                        .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?;
                    let terminal = self
                        .terminals
                        .get_mut(index)
                        .ok_or(FinalizeCompilerFailure::InvalidTokenCoverage)?;
                    if terminal.owner.replace(node_id).is_some() {
                        return Err(FinalizeCompilerFailure::InvalidParentTopology.into());
                    }
                    // Braces are paired in source order, so an `if` with a
                    // braced `else` keeps both blocks instead of only the
                    // last. Every other production owns exactly one pair.
                    match predicate {
                        TerminalPredicate::Fixed(FixedTerminal::LeftBrace) => {
                            if body_open.is_none() {
                                body_open = Some(child.first_terminal);
                            } else if else_open.is_none() {
                                else_open = Some(child.first_terminal);
                            } else {
                                return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
                            }
                        }
                        TerminalPredicate::Fixed(FixedTerminal::RightBrace) => {
                            if body_close.is_none() {
                                body_close = Some(child.first_terminal);
                            } else if else_close.is_none() {
                                else_close = Some(child.first_terminal);
                            } else {
                                return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
                            }
                        }
                        TerminalPredicate::Fixed(FixedTerminal::Else) => {
                            has_else = true;
                        }
                        _ => {}
                    }
                }
                CompletedKind::Production { node, .. } => {
                    let record = self
                        .nodes
                        .get_mut(node.index())
                        .ok_or(FinalizeCompilerFailure::InvalidParentTopology)?;
                    if record.parent.replace(node_id).is_some() {
                        return Err(FinalizeCompilerFailure::InvalidParentTopology.into());
                    }
                    record.child_ordinal = production_ordinal;
                    self.push_child(node)?;
                    production_ordinal = production_ordinal
                        .checked_add(1)
                        .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
                }
            }
        }
        let node = NodeRecord {
            production,
            parent: None,
            child_ordinal: 0,
            child_start,
            child_count: u32::try_from(production_children)
                .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?,
            tree_depth: 0,
            format_depth: 0,
            top_item: None,
            first_terminal,
            terminal_count,
            extent,
            body_open,
            body_close,
            else_open,
            else_close,
            has_else,
        };
        let actual_id = self.push_node(node)?;
        if actual_id != node_id {
            return Err(FinalizeCompilerFailure::CounterOverflow.into());
        }
        self.roots.truncate(root_start);
        self.push_root(Completed {
            kind: CompletedKind::Production {
                production,
                node: node_id,
            },
            element_start,
            element_end: element_index,
            first_terminal,
            first_local_terminal,
            terminal_count,
            extent,
        })
    }

    fn assign_depths(&mut self, root: NodeId) -> Result<(), Stop> {
        let root_record = self
            .nodes
            .get_mut(root.index())
            .ok_or(FinalizeCompilerFailure::InvalidRoot)?;
        root_record.tree_depth = 0;
        root_record.format_depth = 0;
        root_record.top_item = None;
        for parent_index in (0..self.nodes.len()).rev() {
            self.work.spend(1)?;
            let parent_id =
                NodeId::from_index(parent_index).ok_or(FinalizeCompilerFailure::CounterOverflow)?;
            let parent = *self
                .nodes
                .get(parent_index)
                .ok_or(FinalizeCompilerFailure::InvalidParentTopology)?;
            let start = usize::try_from(parent.child_start)
                .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?;
            let end = start
                .checked_add(
                    usize::try_from(parent.child_count)
                        .map_err(|_| FinalizeCompilerFailure::CounterOverflow)?,
                )
                .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
            for child_index in start..end {
                let child_id = *self
                    .children
                    .get(child_index)
                    .ok_or(FinalizeCompilerFailure::InvalidParentTopology)?;
                let child_snapshot = *self
                    .nodes
                    .get(child_id.index())
                    .ok_or(FinalizeCompilerFailure::InvalidParentTopology)?;
                if child_snapshot.parent != Some(parent_id) {
                    return Err(FinalizeCompilerFailure::InvalidParentTopology.into());
                }
                if parent.body_open.is_some() != parent.body_close.is_some()
                    || parent.else_open.is_some() != parent.else_close.is_some()
                {
                    return Err(FinalizeCompilerFailure::InvalidProductionShape.into());
                }
                let inside_body = parent.body_ranges().iter().flatten().any(|(open, close)| {
                    child_snapshot.first_terminal > *open
                        && child_snapshot
                            .last_terminal()
                            .is_some_and(|last| last < *close)
                });
                let child = self
                    .nodes
                    .get_mut(child_id.index())
                    .ok_or(FinalizeCompilerFailure::InvalidParentTopology)?;
                child.tree_depth = parent
                    .tree_depth
                    .checked_add(1)
                    .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
                child.format_depth = parent
                    .format_depth
                    .checked_add(u32::from(inside_body))
                    .ok_or(FinalizeCompilerFailure::CounterOverflow)?;
                child.top_item = if parent_id == root {
                    Some(child_id)
                } else {
                    parent.top_item
                };
            }
        }
        Ok(())
    }

    fn run(mut self) -> Result<FinalizedTopology, Stop> {
        self.build_source_extents()?;
        for (element_index, element) in self.parsed.tree.elements.iter().enumerate() {
            self.work.spend(1)?;
            match *element {
                DerivationElement::Terminal { token, predicate } => {
                    self.terminal(element_index, token, predicate)?;
                }
                DerivationElement::Production {
                    production,
                    child_count,
                    subtree_elements,
                    extent,
                } => self.production(
                    element_index,
                    production,
                    child_count,
                    subtree_elements,
                    extent,
                )?,
            }
        }
        let [root_completion] = self.roots.as_slice() else {
            return Err(FinalizeCompilerFailure::InvalidRoot.into());
        };
        let CompletedKind::Production {
            production: root_production,
            node: root,
        } = root_completion.kind
        else {
            return Err(FinalizeCompilerFailure::InvalidRoot.into());
        };
        if !root_production.is_start() {
            return Err(FinalizeCompilerFailure::InvalidRoot.into());
        }
        if root_completion.element_start != 0
            || root_completion.element_end.checked_add(1) != Some(self.parsed.tree.elements.len())
        {
            return Err(FinalizeCompilerFailure::InvalidRoot.into());
        }
        if self.terminals.len() != self.parsed.classified.tokens().len()
            || u64::try_from(self.terminals.len()).ok() != Some(self.parsed.tree.terminal_count)
            || u64::try_from(self.nodes.len()).ok() != Some(self.parsed.tree.production_count)
            || self.nodes.len().checked_sub(1) != Some(self.children.len())
        {
            return Err(FinalizeCompilerFailure::CountDisagreement.into());
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if NodeId::from_index(index) == Some(root) {
                if node.parent.is_some() || node.production != root_production {
                    return Err(FinalizeCompilerFailure::InvalidRoot.into());
                }
            } else if node.parent.is_none() {
                return Err(FinalizeCompilerFailure::InvalidParentTopology.into());
            }
        }
        if self
            .terminals
            .iter()
            .any(|terminal| terminal.owner.is_none())
        {
            return Err(FinalizeCompilerFailure::InvalidParentTopology.into());
        }
        self.assign_depths(root)?;
        Ok(FinalizedTopology {
            nodes: self.nodes,
            children: self.children,
            terminals: self.terminals,
            source_extents: self.source_extents,
            root,
        })
    }
}

/// Finalizes one complete private active-specification derivation in linear space and work.
#[must_use]
pub fn finalize(parsed: ParsedBundle, limits: FinalizeLimits) -> FinalizeOutcome {
    match Finalizer::new(&parsed, limits).run() {
        Ok(topology) => FinalizeOutcome::Complete(Box::new(FinalizedBundle { parsed, topology })),
        Err(Stop::Resource(failure)) => FinalizeOutcome::ResourceFailure(failure),
        Err(Stop::Compiler(failure)) => FinalizeOutcome::CompilerFailure(failure),
    }
}
