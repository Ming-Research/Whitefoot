//! Borrowed grammar views over canonical owned syntax. Node and terminal
//! identities are the parser's; these views make no resolution or type judgment.

use crate::syntax::terminal::TerminalPredicate;
use crate::syntax::{FinalizedExtent, FinalizedTopology, NodeId};
use crate::{ByteOffset, CanonicalSyntaxUnit, NodePath, Production, SyntaxCoordinate};

/// [GRAM-4] one conditional node's then-block and its alternative.
pub(crate) struct ConditionalBlocks {
    pub(crate) then_statements: Vec<NodeId>,
    pub(crate) alternative: ConditionalAlternative,
}

/// [GRAM-6] the three shapes an `if` alternative can take.
pub(crate) enum ConditionalAlternative {
    /// No `else`: the else-free `if`, whose alternative delivers and does
    /// nothing. [ERR-2] makes this the one spelling of the empty alternative.
    Absent,
    /// A braced `else` and the statements it owns.
    Block(Vec<NodeId>),
    /// `else if`: the nested conditional owning the rest of the chain.
    Chain(NodeId),
}

pub(crate) struct SyntaxView<'unit> {
    syntax: &'unit CanonicalSyntaxUnit,
    paths: Vec<NodePath>,
    /// Every node ordered by its path, so a path finds its node by binary
    /// search.
    by_path: Vec<NodeId>,
    direct_terminals: Vec<Vec<usize>>,
}

impl<'unit> SyntaxView<'unit> {
    pub(crate) fn new(syntax: &'unit CanonicalSyntaxUnit) -> Result<Self, SyntaxViewFailure> {
        let topology = Self::topology_of(syntax);
        let mut paths = Vec::with_capacity(topology.nodes.len());
        for index in 0..topology.nodes.len() {
            let mut node = NodeId::from_index(index).ok_or(SyntaxViewFailure::CounterOverflow)?;
            let mut components = Vec::new();
            while node != topology.root {
                let record = topology
                    .node(node)
                    .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
                components.push(record.child_ordinal);
                node = record
                    .parent
                    .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
            }
            components.reverse();
            paths.push(NodePath { components });
        }

        let mut by_path = (0..paths.len())
            .map(|index| NodeId::from_index(index).ok_or(SyntaxViewFailure::CounterOverflow))
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
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
            direct_terminals
                .get_mut(owner.index())
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?
                .push(terminal_index);
        }
        Ok(Self {
            syntax,
            paths,
            by_path,
            direct_terminals,
        })
    }

    fn topology(&self) -> &FinalizedTopology {
        Self::topology_of(self.syntax)
    }

    fn topology_of(syntax: &CanonicalSyntaxUnit) -> &FinalizedTopology {
        &syntax.finalized.topology
    }

    pub(crate) fn depth(&self, node: NodeId) -> Option<u32> {
        self.topology().node(node).map(|record| record.tree_depth)
    }

    pub(crate) fn is_single_token(&self, node: NodeId) -> bool {
        self.topology()
            .node(node)
            .is_some_and(|record| record.terminal_count == 1)
    }

    pub(crate) fn root(&self) -> NodeId {
        self.topology().root
    }

    pub(crate) fn production(&self, node: NodeId) -> Result<Production, SyntaxViewFailure> {
        self.topology()
            .node(node)
            .map(|record| record.production)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)
    }

    pub(crate) fn children(&self, node: NodeId) -> Result<&[NodeId], SyntaxViewFailure> {
        self.topology()
            .node_children(node)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)
    }

    pub(crate) fn children_with(
        &self,
        node: NodeId,
        production: Production,
    ) -> Result<Vec<NodeId>, SyntaxViewFailure> {
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

    pub(crate) fn only_child(&self, node: NodeId) -> Result<NodeId, SyntaxViewFailure> {
        let [child] = self.children(node)? else {
            return Err(SyntaxViewFailure::InvalidCanonicalTree);
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
    pub(crate) fn sole_expression_child(
        &self,
        expression: NodeId,
    ) -> Result<Option<NodeId>, SyntaxViewFailure> {
        if self
            .first_child_with(expression, Production::InfixTail)?
            .is_some()
        {
            return Ok(None);
        }
        self.only_child(expression).map(Some)
    }

    pub(crate) fn first_child_with(
        &self,
        node: NodeId,
        production: Production,
    ) -> Result<Option<NodeId>, SyntaxViewFailure> {
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
    pub(crate) fn is_body_less(&self, node: NodeId) -> Result<bool, SyntaxViewFailure> {
        Ok(match self.production(node)? {
            Production::FnSig => true,
            Production::FnDecl => self
                .direct_terminals
                .get(node.index())
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?
                .iter()
                .all(|terminal| {
                    self.syntax
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
    pub(crate) fn names_nominal(&self, node: NodeId) -> Result<bool, SyntaxViewFailure> {
        Ok(self
            .direct_token_with(node, TerminalPredicate::TypeIdentifier)?
            .is_some()
            || self.first_child_with(node, Production::TypePath)?.is_some())
    }

    /// The innermost node of a callee's qualified chain [GRAM-5, MOD-5]: the
    /// callee itself when it is unqualified, otherwise its last
    /// `callee_path`. The tail writes the final function name, or the
    /// `pack_use` and member that every unqualified callee writes directly.
    pub(crate) fn callee_tail(&self, callee: NodeId) -> Result<NodeId, SyntaxViewFailure> {
        let mut node = callee;
        while let Some(next) = self.first_child_with(node, Production::CalleePath)? {
            node = next;
        }
        Ok(node)
    }

    /// The `pack_use` a callee's tail writes, when it writes one.
    pub(crate) fn callee_application(
        &self,
        callee: NodeId,
    ) -> Result<Option<NodeId>, SyntaxViewFailure> {
        let tail = self.callee_tail(callee)?;
        self.first_child_with(tail, Production::PackUse)
    }

    /// Uppercase callees without an IDENT member selector are constructions:
    /// a struct or prelude constructor, or a type-owned variant written
    /// after its owner [TYPE-6]. The grammar shares their prefix with
    /// qualified member calls (strong LL(2)).
    pub(crate) fn is_constructor_call(&self, node: NodeId) -> Result<bool, SyntaxViewFailure> {
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
    pub(crate) fn group_application(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeId>, SyntaxViewFailure> {
        match self.first_child_with(node, Production::PackUse)? {
            Some(application) => Ok(Some(application)),
            None => self.first_child_with(node, Production::TypePath),
        }
    }

    pub(crate) fn argument_list(&self, node: NodeId) -> Result<Option<NodeId>, SyntaxViewFailure> {
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
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
            let head = self
                .callee_application(callee)?
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
            self.first_child_with(head, Production::Targs)
        } else {
            self.first_child_with(node, Production::Targs)
        }
    }

    pub(crate) fn constructor_descendants(
        &self,
        node: NodeId,
    ) -> Result<Vec<NodeId>, SyntaxViewFailure> {
        let mut result = Vec::new();
        for call in self.descendants_with(node, Production::Call)? {
            if self.is_constructor_call(call)? {
                result.push(call);
            }
        }
        Ok(result)
    }

    pub(crate) fn descendants_with(
        &self,
        node: NodeId,
        production: Production,
    ) -> Result<Vec<NodeId>, SyntaxViewFailure> {
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

    pub(crate) fn conditional_blocks(
        &self,
        node: NodeId,
    ) -> Result<ConditionalBlocks, SyntaxViewFailure> {
        ConditionalBlocks::read(self.topology(), node)
    }

    pub(crate) fn path(&self, node: NodeId) -> Result<&NodePath, SyntaxViewFailure> {
        self.paths
            .get(node.index())
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)
    }

    pub(crate) fn node_with_path(&self, path: &NodePath) -> Option<NodeId> {
        self.by_path
            .binary_search_by(|node| self.paths[node.index()].components().cmp(path.components()))
            .ok()
            .map(|position| self.by_path[position])
    }

    pub(crate) fn parent(&self, node: NodeId) -> Result<Option<NodeId>, SyntaxViewFailure> {
        self.topology()
            .node(node)
            .map(|record| record.parent)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)
    }

    pub(crate) fn coordinate(&self, node: NodeId) -> Result<SyntaxCoordinate, SyntaxViewFailure> {
        let record = self
            .topology()
            .node(node)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
        let FinalizedExtent::Source { source, start, end } = record.extent else {
            return Err(SyntaxViewFailure::InvalidCanonicalTree);
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
    pub(crate) fn is_prelude_node(&self, node: NodeId) -> Result<bool, SyntaxViewFailure> {
        let coordinate = self.coordinate(node)?;
        Ok(self
            .syntax
            .classified_bundle()
            .source_bundle()
            .file(coordinate.source())
            .is_some_and(|file| file.prelude().is_some()))
    }

    /// Copies the exact canonical source spelling owned by one production
    /// node. It is not a portable source identity or a second parser.
    pub(crate) fn source_spelling(&self, node: NodeId) -> Result<String, SyntaxViewFailure> {
        let coordinate = self.coordinate(node)?;
        let bundle = self.syntax.classified_bundle().source_bundle();
        let span = bundle
            .span(coordinate.source(), coordinate.start(), coordinate.end())
            .map_err(|_| SyntaxViewFailure::InvalidCanonicalTree)?;
        let bytes = bundle
            .span_bytes(span)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| SyntaxViewFailure::InvalidCanonicalTree)
    }

    /// Resolves one checked node path to the name a reader is shown for its
    /// source and its exact byte extent.
    ///
    /// The name is the display path — the host path the driver read the source
    /// from — because every consumer of this pair prints it for a person to
    /// act on. The bundle's own portable [`crate::LogicalPath`] stays the
    /// program-internal key that orders the bundle and detects a duplicate.
    /// The pair is stable only within this checked program.
    pub(crate) fn source_identity(
        &self,
        path: &NodePath,
    ) -> Result<(String, SyntaxCoordinate), SyntaxViewFailure> {
        let node = self
            .node_with_path(path)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
        let coordinate = self.coordinate(node)?;
        let display_path = self
            .syntax
            .classified_bundle()
            .source_bundle()
            .file(coordinate.source())
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?
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
    pub(crate) fn source_line(&self, path: &NodePath) -> Result<(String, u64), SyntaxViewFailure> {
        let (display_path, coordinate) = self.source_identity(path)?;
        let bundle = self.syntax.classified_bundle().source_bundle();
        let prefix = bundle
            .span(coordinate.source(), ByteOffset::new(0), coordinate.start())
            .map_err(|_| SyntaxViewFailure::InvalidCanonicalTree)?;
        let newlines = bundle
            .span_bytes(prefix)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        let line = u64::try_from(newlines)
            .map_err(|_| SyntaxViewFailure::InvalidCanonicalTree)?
            .saturating_add(1);
        Ok((display_path, line))
    }

    /// [`Self::source_spelling`] reached by node path rather than by node.
    pub(crate) fn path_spelling(&self, path: &NodePath) -> Result<String, SyntaxViewFailure> {
        let node = self
            .node_with_path(path)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
        self.source_spelling(node)
    }

    pub(crate) fn closing_brace_coordinate(
        &self,
        node: NodeId,
    ) -> Result<SyntaxCoordinate, SyntaxViewFailure> {
        let terminal = self
            .topology()
            .node(node)
            .and_then(|record| record.body_close)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.syntax.classified_bundle().tokens().get(index))
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?
            .token()
            .id();
        Ok(SyntaxCoordinate::new(
            terminal.source(),
            terminal.start(),
            terminal.end(),
        ))
    }

    pub(crate) fn direct_token_indices(&self, node: NodeId) -> Result<&[usize], SyntaxViewFailure> {
        self.direct_terminals
            .get(node.index())
            .map(Vec::as_slice)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)
    }

    pub(crate) fn token_bytes(&self, terminal: usize) -> Result<&'unit [u8], SyntaxViewFailure> {
        let classified = self.syntax.classified_bundle();
        classified
            .tokens()
            .get(terminal)
            .and_then(|token| classified.token_bytes(token.token()))
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)
    }

    pub(crate) fn direct_spelling(&self, node: NodeId) -> Result<Vec<u8>, SyntaxViewFailure> {
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
    pub(crate) fn direct_identifiers(&self, node: NodeId) -> Result<Vec<usize>, SyntaxViewFailure> {
        let classified = self.syntax.classified_bundle();
        let mut identifiers = Vec::new();
        for terminal in self.direct_token_indices(node)? {
            let token = classified
                .tokens()
                .get(*terminal)
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
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
    pub(crate) fn direct_tokens_matching(
        &self,
        node: NodeId,
        predicates: &[TerminalPredicate],
    ) -> Result<Vec<usize>, SyntaxViewFailure> {
        let classified = self.syntax.classified_bundle();
        let mut matches = Vec::new();
        for terminal in self.direct_token_indices(node)? {
            let token = classified
                .tokens()
                .get(*terminal)
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
            if predicates
                .iter()
                .any(|predicate| token.terminals().contains(*predicate))
            {
                matches.push(*terminal);
            }
        }
        Ok(matches)
    }

    pub(crate) fn direct_token_with(
        &self,
        node: NodeId,
        predicate: TerminalPredicate,
    ) -> Result<Option<usize>, SyntaxViewFailure> {
        let classified = self.syntax.classified_bundle();
        let mut found = None;
        for terminal in self.direct_token_indices(node)? {
            let token = classified
                .tokens()
                .get(*terminal)
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
            if token.terminals().contains(predicate) && found.replace(*terminal).is_some() {
                return Err(SyntaxViewFailure::InvalidCanonicalTree);
            }
        }
        Ok(found)
    }
}

/// An impossible shape in already canonical syntax, not a source verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SyntaxViewFailure {
    InvalidCanonicalTree,
    CounterOverflow,
}

impl ConditionalBlocks {
    /// Both statement sequences belong to the conditional node. The parser's
    /// brace extents distinguish them; a chained conditional is its own node.
    pub(crate) fn read(
        topology: &FinalizedTopology,
        node: NodeId,
    ) -> Result<Self, SyntaxViewFailure> {
        let record = topology
            .node(node)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
        let [Some((open, close)), else_range] = record.body_ranges() else {
            return Err(SyntaxViewFailure::InvalidCanonicalTree);
        };
        let children = topology
            .node_children(node)
            .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
        let mut then_statements = Vec::new();
        let mut else_statements = Vec::new();
        let mut chain = None;
        for &child in children {
            let record = topology
                .node(child)
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?;
            match record.production {
                Production::Stmt => {
                    if record.first_terminal > open
                        && record.last_terminal().is_some_and(|last| last < close)
                    {
                        then_statements.push(child);
                    } else {
                        else_statements.push(child);
                    }
                }
                Production::IfStmt | Production::ValueIf if chain.is_none() => chain = Some(child),
                _ => {}
            }
        }
        if else_range.is_none() && !else_statements.is_empty() {
            return Err(SyntaxViewFailure::InvalidCanonicalTree);
        }
        let alternative = match (else_range, record.has_else) {
            (Some(_), _) => ConditionalAlternative::Block(else_statements),
            (None, true) => {
                ConditionalAlternative::Chain(chain.ok_or(SyntaxViewFailure::InvalidCanonicalTree)?)
            }
            (None, false) => ConditionalAlternative::Absent,
        };
        Ok(Self {
            then_statements,
            alternative,
        })
    }
}

/// Written alternatives only; selected storage and reference rules belong
/// to semantic checking. Payload selection retains its variant token.
pub(crate) enum PlaceSuffix {
    Dereference,
    Member(MemberForm),
    Index { offset: NodeId },
    Range { start: NodeId, end: NodeId },
}

pub(crate) struct GraphForm {
    pub(crate) rows: Vec<NodeId>,
    pub(crate) entries: Vec<NodeId>,
}

pub(crate) struct ModuleRowForm {
    pub(crate) module: NodeId,
    pub(crate) dependencies: Vec<NodeId>,
}

pub(crate) struct EntryForm {
    pub(crate) name: usize,
    pub(crate) target: NodeId,
    pub(crate) no_heap: bool,
}

pub(crate) struct ModulePathForm {
    pub(crate) standard: bool,
    pub(crate) components: Vec<usize>,
}

pub(crate) struct ItemForm {
    pub(crate) node: NodeId,
    pub(crate) declaration: NodeId,
    pub(crate) production: Production,
}

impl SyntaxView<'_> {
    pub(crate) fn has_fixed(
        &self,
        node: NodeId,
        terminal: crate::FixedTerminal,
    ) -> Result<bool, SyntaxViewFailure> {
        Ok(self
            .direct_token_with(node, TerminalPredicate::Fixed(terminal))?
            .is_some())
    }

    pub(crate) fn place_suffix(&self, node: NodeId) -> Result<PlaceSuffix, SyntaxViewFailure> {
        if self.has_fixed(node, crate::FixedTerminal::Caret)? {
            return Ok(PlaceSuffix::Dereference);
        }
        if let Some(offset) = self.first_child_with(node, Production::Atom)? {
            return Ok(match self.first_child_with(node, Production::RangeTail)? {
                Some(tail) => PlaceSuffix::Range {
                    start: offset,
                    end: self
                        .first_child_with(tail, Production::Atom)?
                        .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?,
                },
                None => PlaceSuffix::Index { offset },
            });
        }
        Ok(PlaceSuffix::Member(MemberForm::read(
            self.syntax.classified_bundle(),
            &self.direct_tokens_matching(
                node,
                &[
                    TerminalPredicate::Identifier,
                    TerminalPredicate::TypeIdentifier,
                ],
            )?,
        )?))
    }

    pub(crate) fn subscript_offset(
        &self,
        suffix: NodeId,
    ) -> Result<Option<NodeId>, SyntaxViewFailure> {
        Ok(match self.place_suffix(suffix)? {
            PlaceSuffix::Index { offset } | PlaceSuffix::Range { start: offset, .. } => {
                Some(offset)
            }
            PlaceSuffix::Member(_) | PlaceSuffix::Dereference => None,
        })
    }

    pub(crate) fn last_subscript(
        &self,
        suffixes: &[NodeId],
    ) -> Result<Option<usize>, SyntaxViewFailure> {
        let mut last = None;
        for (position, &suffix) in suffixes.iter().enumerate() {
            if self.subscript_offset(suffix)?.is_some() {
                last = Some(position);
            }
        }
        Ok(last)
    }

    pub(crate) fn range_suffix_position(
        &self,
        suffixes: &[NodeId],
    ) -> Result<Option<usize>, SyntaxViewFailure> {
        for (position, &suffix) in suffixes.iter().enumerate() {
            if matches!(self.place_suffix(suffix)?, PlaceSuffix::Range { .. }) {
                return Ok(Some(position));
            }
        }
        Ok(None)
    }

    pub(crate) fn graph(&self) -> Result<GraphForm, SyntaxViewFailure> {
        if self.production(self.root())? != Production::GraphFile {
            return Err(SyntaxViewFailure::InvalidCanonicalTree);
        }
        Ok(GraphForm {
            rows: self.children_with(self.root(), Production::ModuleRow)?,
            entries: self.children_with(self.root(), Production::EntryDecl)?,
        })
    }

    pub(crate) fn module_row(&self, node: NodeId) -> Result<ModuleRowForm, SyntaxViewFailure> {
        let mut paths = self
            .children_with(node, Production::ModulePath)?
            .into_iter();
        Ok(ModuleRowForm {
            module: paths
                .next()
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?,
            dependencies: paths.collect(),
        })
    }

    pub(crate) fn graph_entry(&self, node: NodeId) -> Result<EntryForm, SyntaxViewFailure> {
        let [target] = self.children_with(node, Production::ModulePath)?[..] else {
            return Err(SyntaxViewFailure::InvalidCanonicalTree);
        };
        Ok(EntryForm {
            name: self
                .direct_token_with(node, TerminalPredicate::Identifier)?
                .ok_or(SyntaxViewFailure::InvalidCanonicalTree)?,
            target,
            no_heap: self.has_fixed(node, crate::FixedTerminal::NoHeap)?,
        })
    }

    pub(crate) fn module_path(&self, node: NodeId) -> Result<ModulePathForm, SyntaxViewFailure> {
        Ok(ModulePathForm {
            standard: self.has_fixed(node, crate::FixedTerminal::Std)?,
            components: self.direct_identifiers(node)?,
        })
    }

    pub(crate) fn items(&self) -> Result<Vec<ItemForm>, SyntaxViewFailure> {
        self.children(self.root())?
            .iter()
            .map(|&node| {
                let declaration = self.only_child(node)?;
                Ok(ItemForm {
                    node,
                    declaration,
                    production: self.production(declaration)?,
                })
            })
            .collect()
    }

    pub(crate) fn byte_range(&self, node: NodeId) -> Result<(usize, usize), SyntaxViewFailure> {
        let coordinate = self.coordinate(node)?;
        Ok((
            usize::try_from(coordinate.start().value())
                .map_err(|_| SyntaxViewFailure::CounterOverflow)?,
            usize::try_from(coordinate.end().value())
                .map_err(|_| SyntaxViewFailure::CounterOverflow)?,
        ))
    }

    pub(crate) fn documentation_ranges(
        &self,
        node: NodeId,
    ) -> Result<Vec<(usize, usize)>, SyntaxViewFailure> {
        let mut ranges = self
            .descendants_with(node, Production::Doc)?
            .into_iter()
            .map(|node| self.byte_range(node))
            .collect::<Result<Vec<_>, _>>()?;
        ranges.sort_unstable();
        Ok(ranges)
    }

    pub(crate) fn item_name(&self, node: NodeId) -> Result<Option<usize>, SyntaxViewFailure> {
        Ok(self
            .direct_tokens_matching(
                node,
                &[
                    TerminalPredicate::Identifier,
                    TerminalPredicate::TypeIdentifier,
                ],
            )?
            .first()
            .copied())
    }
}

/// One field or variant-qualified payload name, shared by place and state
/// paths. Which declaration the names select is a resolver/checker question.
pub(crate) struct MemberForm {
    pub(crate) variant: Option<usize>,
    pub(crate) field: usize,
}

impl MemberForm {
    pub(crate) fn read(
        classified: &crate::ClassifiedBundle,
        names: &[usize],
    ) -> Result<Self, SyntaxViewFailure> {
        let has = |index: usize, predicate| {
            classified
                .tokens()
                .get(index)
                .is_some_and(|token| token.terminals().contains(predicate))
        };
        match names {
            [field] if has(*field, TerminalPredicate::Identifier) => Ok(Self {
                variant: None,
                field: *field,
            }),
            [variant, field]
                if has(*variant, TerminalPredicate::TypeIdentifier)
                    && has(*field, TerminalPredicate::Identifier) =>
            {
                Ok(Self {
                    variant: Some(*variant),
                    field: *field,
                })
            }
            _ => Err(SyntaxViewFailure::InvalidCanonicalTree),
        }
    }
}

impl SyntaxView<'_> {
    /// Only the path's own suffixes count; an index operand has its own place.
    pub(crate) fn place_has_dereference(&self, place: NodeId) -> Result<bool, SyntaxViewFailure> {
        Ok(self
            .reference_step(&self.children_with(place, Production::Psuffix)?)?
            .is_some())
    }

    pub(crate) fn reference_step(
        &self,
        suffixes: &[NodeId],
    ) -> Result<Option<usize>, SyntaxViewFailure> {
        for (index, &suffix) in suffixes.iter().enumerate() {
            if matches!(self.place_suffix(suffix)?, PlaceSuffix::Dereference) {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
}
