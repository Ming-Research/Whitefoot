use crate::syntax::terminal::TerminalPredicate;
use crate::syntax::{FinalizedExtent, FinalizedTopology, NodeId};
use crate::{
    ByteOffset, NodePath, Production, ResolvedSyntaxUnit, SemanticCompilerFailure, SyntaxCoordinate,
};

/// [GRAM-4] one conditional node's then-block and its alternative.
pub(super) struct ConditionalBlocks {
    pub(super) then_statements: Vec<NodeId>,
    pub(super) alternative: ConditionalAlternative,
}

/// [GRAM-6] the three shapes an `if` alternative can take.
pub(super) enum ConditionalAlternative {
    /// No `else`: the else-free `if`, whose alternative delivers and does
    /// nothing. [ERR-2] makes this the one spelling of the empty alternative.
    Absent,
    /// A braced `else` and the statements it owns.
    Block(Vec<NodeId>),
    /// `else if`: the nested conditional owning the rest of the chain.
    Chain(NodeId),
}

pub(super) struct TreeView<'unit> {
    resolved: &'unit ResolvedSyntaxUnit,
    paths: Vec<NodePath>,
    /// Every node ordered by its path, so a path finds its node by binary
    /// search.
    by_path: Vec<NodeId>,
    direct_terminals: Vec<Vec<usize>>,
}

impl<'unit> TreeView<'unit> {
    pub(super) fn new(
        resolved: &'unit ResolvedSyntaxUnit,
    ) -> Result<Self, SemanticCompilerFailure> {
        let topology = Self::topology_of(resolved);
        let mut paths = Vec::with_capacity(topology.nodes.len());
        for index in 0..topology.nodes.len() {
            let mut node =
                NodeId::from_index(index).ok_or(SemanticCompilerFailure::CounterOverflow)?;
            let mut components = Vec::new();
            while node != topology.root {
                let record = topology
                    .node(node)
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                components.push(record.child_ordinal);
                node = record
                    .parent
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            }
            components.reverse();
            paths.push(NodePath { components });
        }

        let mut by_path = (0..paths.len())
            .map(|index| NodeId::from_index(index).ok_or(SemanticCompilerFailure::CounterOverflow))
            .collect::<Result<Vec<_>, _>>()?;
        by_path.sort_by(|left, right| {
            paths[left.index()]
                .components()
                .cmp(paths[right.index()].components())
        });

        let mut direct_terminals = vec![Vec::new(); topology.nodes.len()];
        for (terminal_index, terminal) in topology.terminals.iter().enumerate() {
            let owner = terminal
                .owner
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            direct_terminals
                .get_mut(owner.index())
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?
                .push(terminal_index);
        }
        Ok(Self {
            resolved,
            paths,
            by_path,
            direct_terminals,
        })
    }

    pub(super) fn topology(&self) -> &FinalizedTopology {
        Self::topology_of(self.resolved)
    }

    fn topology_of(resolved: &ResolvedSyntaxUnit) -> &FinalizedTopology {
        &resolved.syntax().finalized.topology
    }

    pub(super) fn root(&self) -> NodeId {
        self.topology().root
    }

    pub(super) fn production(&self, node: NodeId) -> Result<Production, SemanticCompilerFailure> {
        self.topology()
            .node(node)
            .map(|record| record.production)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)
    }

    pub(super) fn children(&self, node: NodeId) -> Result<&[NodeId], SemanticCompilerFailure> {
        self.topology()
            .node_children(node)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)
    }

    pub(super) fn children_with(
        &self,
        node: NodeId,
        production: Production,
    ) -> Result<Vec<NodeId>, SemanticCompilerFailure> {
        Ok(self
            .children(node)?
            .iter()
            .copied()
            .filter(|child| {
                self.production(*child)
                    .is_ok_and(|actual| actual == production)
            })
            .collect())
    }

    pub(super) fn only_child(&self, node: NodeId) -> Result<NodeId, SemanticCompilerFailure> {
        let [child] = self.children(node)? else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree);
        };
        Ok(*child)
    }

    /// The one child holding an `expr`'s complete written content, or `None`
    /// when the expression is the infix shape.
    ///
    /// [GRAM-5] `expr := atom infix_tail? | call | construct`. Three of those
    /// alternatives are a single child that names what the expression is
    /// written as; the infix one is two children producing a fresh operation
    /// result that is no written atom, call, or construct. A structural query
    /// asking which of the three shapes an expression has therefore has no
    /// answer for infix and must answer `None` — never fail the tree, which
    /// would turn a legal expression into an internal compiler failure.
    pub(super) fn sole_expression_child(
        &self,
        expression: NodeId,
    ) -> Result<Option<NodeId>, SemanticCompilerFailure> {
        if self
            .first_child_with(expression, Production::InfixTail)?
            .is_some()
        {
            return Ok(None);
        }
        self.only_child(expression).map(Some)
    }

    pub(super) fn first_child_with(
        &self,
        node: NodeId,
        production: Production,
    ) -> Result<Option<NodeId>, SemanticCompilerFailure> {
        for child in self.children(node)? {
            if self.production(*child)? == production {
                return Ok(Some(*child));
            }
        }
        Ok(None)
    }

    /// Whether a callable node writes no body: a function-kind formal's
    /// `fn_sig`, a PRE-1 record, or an interface `fn_decl` that ends in `;`
    /// or its `doc` entry [MOD-7].
    pub(super) fn is_body_less(&self, node: NodeId) -> Result<bool, SemanticCompilerFailure> {
        Ok(match self.production(node)? {
            Production::FnSig => true,
            Production::FnDecl => self
                .direct_terminals
                .get(node.index())
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?
                .iter()
                .all(|terminal| {
                    self.resolved
                        .syntax()
                        .classified_bundle()
                        .tokens()
                        .get(*terminal)
                        .is_none_or(|token| {
                            !token
                                .terminals()
                                .contains(TerminalPredicate::Fixed(crate::FixedTerminal::LeftBrace))
                        })
                }),
            _ => false,
        })
    }

    /// Whether a `type`, `cvalue` or destructuring target names a nominal:
    /// directly by its TYPEID, or through a qualified `type_path` [MOD-5].
    pub(super) fn names_nominal(&self, node: NodeId) -> Result<bool, SemanticCompilerFailure> {
        Ok(self
            .direct_token_with(node, TerminalPredicate::TypeIdentifier)?
            .is_some()
            || self.first_child_with(node, Production::TypePath)?.is_some())
    }

    /// The innermost node of a callee's qualified chain [GRAM-5, MOD-5]: the
    /// callee itself when it is unqualified, otherwise its last
    /// `callee_path`. The tail writes the final function name, or the
    /// `pack_use` and member that every unqualified callee writes directly.
    pub(super) fn callee_tail(&self, callee: NodeId) -> Result<NodeId, SemanticCompilerFailure> {
        let mut node = callee;
        while let Some(next) = self.first_child_with(node, Production::CalleePath)? {
            node = next;
        }
        Ok(node)
    }

    /// The `pack_use` a callee's tail writes, when it writes one.
    pub(super) fn callee_application(
        &self,
        callee: NodeId,
    ) -> Result<Option<NodeId>, SemanticCompilerFailure> {
        let tail = self.callee_tail(callee)?;
        self.first_child_with(tail, Production::PackUse)
    }

    /// Uppercase callees without an IDENT member selector are constructions:
    /// a struct or prelude constructor, or a type-owned variant written
    /// after its owner [TYPE-6]. The grammar shares their prefix with
    /// qualified member calls (strong LL(2)).
    pub(super) fn is_constructor_call(
        &self,
        node: NodeId,
    ) -> Result<bool, SemanticCompilerFailure> {
        if self.production(node)? != Production::Call {
            return Ok(false);
        }
        let Some(callee) = self.first_child_with(node, Production::Callee)? else {
            return Ok(false);
        };
        let tail = self.callee_tail(callee)?;
        Ok(self.first_child_with(tail, Production::PackUse)?.is_some()
            && self
                .direct_token_with(tail, TerminalPredicate::Identifier)?
                .is_none())
    }

    /// The group application a `gparam` or `binding_decl` writes: its
    /// `pack_use`, or the `type_path` of a qualified group, whose `targs`
    /// follow it in the same parent [GRAM-2, MOD-5].
    pub(super) fn group_application(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeId>, SemanticCompilerFailure> {
        match self.first_child_with(node, Production::PackUse)? {
            Some(application) => Ok(Some(application)),
            None => self.first_child_with(node, Production::TypePath),
        }
    }

    pub(super) fn argument_list(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeId>, SemanticCompilerFailure> {
        // A qualified group's `targs` are its parent's, after the path.
        if self.production(node)? == Production::TypePath
            && let Some(parent) = self.parent(node)?
            && matches!(
                self.production(parent)?,
                Production::Gparam | Production::BindingDecl
            )
        {
            return self.first_child_with(parent, Production::Targs);
        }
        if self.is_constructor_call(node)? {
            let callee = self
                .first_child_with(node, Production::Callee)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let head = self
                .callee_application(callee)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.first_child_with(head, Production::Targs)
        } else {
            self.first_child_with(node, Production::Targs)
        }
    }

    pub(super) fn constructor_descendants(
        &self,
        node: NodeId,
    ) -> Result<Vec<NodeId>, SemanticCompilerFailure> {
        let mut result = Vec::new();
        for call in self.descendants_with(node, Production::Call)? {
            if self.is_constructor_call(call)? {
                result.push(call);
            }
        }
        Ok(result)
    }

    pub(super) fn descendants_with(
        &self,
        node: NodeId,
        production: Production,
    ) -> Result<Vec<NodeId>, SemanticCompilerFailure> {
        let mut matches = Vec::new();
        let mut pending = self
            .children(node)?
            .iter()
            .rev()
            .copied()
            .collect::<Vec<_>>();
        while let Some(candidate) = pending.pop() {
            if self.production(candidate)? == production {
                matches.push(candidate);
            }
            pending.extend(self.children(candidate)?.iter().rev().copied());
        }
        Ok(matches)
    }

    /// [GRAM-4] the statement groups an `if_stmt` or `value_if` owns.
    ///
    /// Both blocks are `Stmt` children of the one conditional node, so only
    /// the brace pairs separate them: the then-block is the group inside the
    /// first pair and a braced `else` is the group inside the second. An
    /// `else if` chain owns one pair and an `else` terminal, and reaches its
    /// alternative through the nested conditional node instead.
    pub(super) fn conditional_blocks(
        &self,
        node: NodeId,
    ) -> Result<ConditionalBlocks, SemanticCompilerFailure> {
        let record = self
            .topology()
            .node(node)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let [Some(then_range), else_range] = record.body_ranges() else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree);
        };
        let mut then_statements = Vec::new();
        let mut else_statements = Vec::new();
        for statement in self.children_with(node, Production::Stmt)? {
            if self.within(statement, then_range)? {
                then_statements.push(statement);
            } else {
                else_statements.push(statement);
            }
        }
        // Only a braced `else` owns statements directly; with no second pair
        // every statement of this node belongs to the then-block.
        if else_range.is_none() && !else_statements.is_empty() {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree);
        }
        let alternative = match (else_range, record.has_else) {
            (Some(_), _) => ConditionalAlternative::Block(else_statements),
            (None, true) => {
                let nested = self
                    .children(node)?
                    .iter()
                    .copied()
                    .find(|child| {
                        self.production(*child).is_ok_and(|production| {
                            matches!(production, Production::IfStmt | Production::ValueIf)
                        })
                    })
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                ConditionalAlternative::Chain(nested)
            }
            (None, false) => ConditionalAlternative::Absent,
        };
        Ok(ConditionalBlocks {
            then_statements,
            alternative,
        })
    }

    fn within(
        &self,
        node: NodeId,
        (open, close): (u64, u64),
    ) -> Result<bool, SemanticCompilerFailure> {
        let record = self
            .topology()
            .node(node)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        Ok(record.first_terminal > open && record.last_terminal().is_some_and(|last| last < close))
    }

    pub(super) fn path(&self, node: NodeId) -> Result<&NodePath, SemanticCompilerFailure> {
        self.paths
            .get(node.index())
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)
    }

    pub(super) fn node_with_path(&self, path: &NodePath) -> Option<NodeId> {
        self.by_path
            .binary_search_by(|node| self.paths[node.index()].components().cmp(path.components()))
            .ok()
            .map(|position| self.by_path[position])
    }

    pub(super) fn parent(&self, node: NodeId) -> Result<Option<NodeId>, SemanticCompilerFailure> {
        self.topology()
            .node(node)
            .map(|record| record.parent)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)
    }

    pub(super) fn coordinate(
        &self,
        node: NodeId,
    ) -> Result<SyntaxCoordinate, SemanticCompilerFailure> {
        let record = self
            .topology()
            .node(node)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let FinalizedExtent::Source { source, start, end } = record.extent else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree);
        };
        Ok(SyntaxCoordinate::new(source, start, end))
    }

    /// Whether this node came from one of the fixed [PRE-1] prelude records
    /// rather than from a writer's source file.
    ///
    /// [STOR-8]'s no-heap refusals are about the *program*'s types and calls.
    /// The prelude declares `Box` and the runtime-capacity shapes in its own
    /// allocating rows, and those declarations are part of every unit, so a
    /// refusal that did not distinguish them would reject every no-heap
    /// program at the prelude.
    pub(super) fn is_prelude_node(&self, node: NodeId) -> Result<bool, SemanticCompilerFailure> {
        let coordinate = self.coordinate(node)?;
        Ok(self
            .resolved
            .syntax()
            .classified_bundle()
            .source_bundle()
            .file(coordinate.source())
            .is_some_and(|file| file.prelude().is_some()))
    }

    /// Copies the exact canonical source spelling owned by one production
    /// node. It is not a portable source identity or a second parser.
    pub(super) fn source_spelling(&self, node: NodeId) -> Result<String, SemanticCompilerFailure> {
        let coordinate = self.coordinate(node)?;
        let bundle = self.resolved.syntax().classified_bundle().source_bundle();
        let span = bundle
            .span(coordinate.source(), coordinate.start(), coordinate.end())
            .map_err(|_| SemanticCompilerFailure::InvalidCanonicalTree)?;
        let bytes = bundle
            .span_bytes(span)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| SemanticCompilerFailure::InvalidCanonicalTree)
    }

    /// Resolves one checked node path to the name a reader is shown for its
    /// source and its exact byte extent.
    ///
    /// The name is the display path — the host path the driver read the source
    /// from — because every consumer of this pair prints it for a person to
    /// act on. The bundle's own portable [`crate::LogicalPath`] stays the
    /// program-internal key that orders the bundle and detects a duplicate.
    /// The pair is stable only within this checked program.
    pub(super) fn source_identity(
        &self,
        path: &NodePath,
    ) -> Result<(String, SyntaxCoordinate), SemanticCompilerFailure> {
        let node = self
            .node_with_path(path)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let coordinate = self.coordinate(node)?;
        let display_path = self
            .resolved
            .syntax()
            .classified_bundle()
            .source_bundle()
            .file(coordinate.source())
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?
            .display_path()
            .to_owned();
        Ok((display_path, coordinate))
    }

    /// Resolves one checked node path to the name a reader is shown for its
    /// source and its one-based line number.
    ///
    /// The line is developer-channel presentation only: the non-normative
    /// permission ledger prints it. No mandatory record and no normative
    /// output reads it.
    pub(super) fn source_line(
        &self,
        path: &NodePath,
    ) -> Result<(String, u64), SemanticCompilerFailure> {
        let (display_path, coordinate) = self.source_identity(path)?;
        let bundle = self.resolved.syntax().classified_bundle().source_bundle();
        let prefix = bundle
            .span(coordinate.source(), ByteOffset::new(0), coordinate.start())
            .map_err(|_| SemanticCompilerFailure::InvalidCanonicalTree)?;
        let newlines = bundle
            .span_bytes(prefix)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        let line = u64::try_from(newlines)
            .map_err(|_| SemanticCompilerFailure::InvalidCanonicalTree)?
            .saturating_add(1);
        Ok((display_path, line))
    }

    /// [`Self::source_spelling`] reached by node path rather than by node.
    pub(super) fn path_spelling(&self, path: &NodePath) -> Result<String, SemanticCompilerFailure> {
        let node = self
            .node_with_path(path)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        self.source_spelling(node)
    }

    pub(super) fn closing_brace_coordinate(
        &self,
        node: NodeId,
    ) -> Result<SyntaxCoordinate, SemanticCompilerFailure> {
        let terminal = self
            .topology()
            .node(node)
            .and_then(|record| record.body_close)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| {
                self.resolved
                    .syntax()
                    .classified_bundle()
                    .tokens()
                    .get(index)
            })
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?
            .token()
            .id();
        Ok(SyntaxCoordinate::new(
            terminal.source(),
            terminal.start(),
            terminal.end(),
        ))
    }

    pub(super) fn direct_token_indices(
        &self,
        node: NodeId,
    ) -> Result<&[usize], SemanticCompilerFailure> {
        self.direct_terminals
            .get(node.index())
            .map(Vec::as_slice)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)
    }

    pub(super) fn token_bytes(
        &self,
        terminal: usize,
    ) -> Result<&'unit [u8], SemanticCompilerFailure> {
        let classified = self.resolved.syntax().classified_bundle();
        classified
            .tokens()
            .get(terminal)
            .and_then(|token| classified.token_bytes(token.token()))
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)
    }

    pub(super) fn direct_spelling(&self, node: NodeId) -> Result<Vec<u8>, SemanticCompilerFailure> {
        let mut spelling = Vec::new();
        for terminal in self.direct_token_indices(node)? {
            spelling.extend_from_slice(self.token_bytes(*terminal)?);
        }
        Ok(spelling)
    }

    /// Returns every IDENT token owned directly by one node, in source order.
    ///
    /// Unlike the single-token reader, this leaves the expected number of
    /// identifiers to the owning production's checker.
    pub(super) fn direct_identifiers(
        &self,
        node: NodeId,
    ) -> Result<Vec<usize>, SemanticCompilerFailure> {
        let classified = self.resolved.syntax().classified_bundle();
        let mut identifiers = Vec::new();
        for terminal in self.direct_token_indices(node)? {
            let token = classified
                .tokens()
                .get(*terminal)
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            if token.terminals().contains(TerminalPredicate::Identifier) {
                identifiers.push(*terminal);
            }
        }
        Ok(identifiers)
    }

    /// Every direct token of one node matching any of the given predicates,
    /// in source order. The single-token reader below rejects a second match
    /// of one predicate, which suits the productions carrying one such
    /// token; a candidate-grammar `const` operation carries two terms of one
    /// terminal class [CONST-1].
    pub(super) fn direct_tokens_matching(
        &self,
        node: NodeId,
        predicates: &[TerminalPredicate],
    ) -> Result<Vec<usize>, SemanticCompilerFailure> {
        let classified = self.resolved.syntax().classified_bundle();
        let mut matches = Vec::new();
        for terminal in self.direct_token_indices(node)? {
            let token = classified
                .tokens()
                .get(*terminal)
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            if predicates
                .iter()
                .any(|predicate| token.terminals().contains(*predicate))
            {
                matches.push(*terminal);
            }
        }
        Ok(matches)
    }

    pub(super) fn direct_token_with(
        &self,
        node: NodeId,
        predicate: TerminalPredicate,
    ) -> Result<Option<usize>, SemanticCompilerFailure> {
        let classified = self.resolved.syntax().classified_bundle();
        let mut found = None;
        for terminal in self.direct_token_indices(node)? {
            let token = classified
                .tokens()
                .get(*terminal)
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            if token.terminals().contains(predicate) && found.replace(*terminal).is_some() {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree);
            }
        }
        Ok(found)
    }
}
