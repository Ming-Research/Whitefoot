//! [MOD-7] interface and implementation correspondence.
//!
//! A definition repeats its interface declaration's complete signature and
//! contract. The two headers are compared as normalized token streams: every
//! resolved name becomes its target identity, so a type written through an
//! alias, through a qualified path or directly is one token; generic binders
//! become their ordinal of declaration, which renames them consistently; a
//! use of a contract `define` of one atom becomes that atom, so the erased
//! sharing such a `define` writes is not compared; parameter and result
//! labels keep their spellings, because callers write them; and `doc`
//! entries are not part of a header.

use std::collections::HashMap;

use crate::syntax::terminal::TerminalPredicate;
use crate::syntax::{FinalizedExtent, FinalizedTopology, NodeId};
use crate::{DeclarationId, Production};

use super::super::{
    DeclarationRecord, DeclarationRole, LexicalUseRecord, ResolutionCompilerFailure,
    ResolutionIssue, ResolutionIssueKind, ResolutionRule, ResolvedTarget,
};

/// One normalized header token.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    Open(Production),
    Close,
    /// A fixed terminal or literal, by exact spelling.
    Text(Vec<u8>),
    /// A name with no binding record: a label, field or selector name.
    Name(String),
    /// A resolved use of a declaration outside the header.
    Use(ResolvedTarget),
    /// A generic or contract-local binder, by declaration ordinal.
    Binder(usize),
    /// A use of such a binder.
    BinderUse(usize),
}

/// The byte range and source of one terminal, the key records are read by.
type Coordinate = (u32, u64);

/// Per-header state: generic binder ordinals and the expansion of every
/// contract `define` seen so far.
#[derive(Default)]
struct Scope {
    binders: HashMap<DeclarationId, usize>,
    expansions: HashMap<DeclarationId, Vec<Token>>,
}

struct Tables<'a> {
    topology: &'a FinalizedTopology,
    classified: &'a crate::ClassifiedBundle,
    direct: Vec<Vec<usize>>,
    uses: HashMap<Coordinate, &'a LexicalUseRecord>,
    declarations: HashMap<Coordinate, &'a DeclarationRecord>,
}

/// Checks every interface declaration against its definition [MOD-7] and
/// returns the first difference, in definition source order.
pub(super) fn check_correspondence<'a>(
    topology: &'a FinalizedTopology,
    classified: &'a crate::ClassifiedBundle,
    declarations: &'a [DeclarationRecord],
    uses: impl Iterator<Item = &'a LexicalUseRecord>,
    pairs: &[(usize, usize)],
    function_nodes: &HashMap<DeclarationId, NodeId>,
) -> Result<Option<ResolutionIssue>, ResolutionCompilerFailure> {
    if pairs.is_empty() {
        return Ok(None);
    }
    let mut direct = vec![Vec::new(); topology.nodes.len()];
    for (index, terminal) in topology.terminals.iter().enumerate() {
        let owner = terminal
            .owner
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        direct
            .get_mut(owner.index())
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .push(index);
    }
    let key = |coordinate: crate::SyntaxCoordinate| {
        (coordinate.source().ordinal(), coordinate.start().value())
    };
    let tables = Tables {
        topology,
        classified,
        direct,
        uses: uses
            .map(|usage| (key(usage.origin().coordinate()), usage))
            .collect(),
        declarations: declarations
            .iter()
            .map(|declaration| (key(declaration.origin().coordinate()), declaration))
            .collect(),
    };
    let mut issues = Vec::new();
    for (interface, definition) in pairs {
        let (Some(interface_node), Some(definition_node)) = (
            function_nodes.get(&declarations[*interface].id()),
            function_nodes.get(&declarations[*definition].id()),
        ) else {
            return Err(ResolutionCompilerFailure::InvalidRoleShape);
        };
        let expected = tables.header(*interface_node)?;
        let written = tables.header(*definition_node)?;
        if expected != written {
            issues.push(ResolutionIssue {
                rule: ResolutionRule::Mod7,
                origin: declarations[*definition].origin().clone(),
                kind: ResolutionIssueKind::Correspondence {
                    spelling: declarations[*definition].spelling().to_owned(),
                    reason: "the definition repeats its interface declaration's complete signature and contract: equal parameters, results, generics, effect row and clauses, compared by resolved identity",
                },
            });
        }
    }
    issues.sort_by_key(|issue| {
        let coordinate = issue.origin().coordinate();
        (coordinate.source().ordinal(), coordinate.start().value())
    });
    Ok(issues.into_iter().next())
}

impl Tables<'_> {
    /// The normalized header of one `fn_decl`: every child and terminal up to
    /// its closing `;`, its `doc` entry or its body.
    fn header(&self, function: NodeId) -> Result<Vec<Token>, ResolutionCompilerFailure> {
        let mut tokens = Vec::new();
        let mut scope = Scope::default();
        self.walk(function, true, &mut tokens, &mut scope)?;
        Ok(tokens)
    }

    /// Records one contract `define` [FN-8]. A `define` whose expression is
    /// one atom is erased sharing: every use, which is itself a bare atom,
    /// compares as that atom, so writing the atom in place instead
    /// corresponds. A `define` of a compound expression can only be named,
    /// so it compares as a binder with its expression.
    fn define(
        &self,
        node: NodeId,
        tokens: &mut Vec<Token>,
        scope: &mut Scope,
    ) -> Result<(), ResolutionCompilerFailure> {
        let terminals = self.direct.get(node.index()).map_or(&[][..], Vec::as_slice);
        let binder = terminals
            .iter()
            .find_map(|terminal| {
                let token = self.classified.tokens().get(*terminal)?.token();
                self.declarations
                    .get(&(token.id().source().ordinal(), token.id().start().value()))
            })
            .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
        let expression = self
            .child(node, Production::Expr)
            .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
        let children = self
            .topology
            .node_children(expression)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        if let [atom] = children
            && self
                .topology
                .node(*atom)
                .is_some_and(|record| record.production == Production::Atom)
        {
            let mut expansion = Vec::new();
            self.walk(*atom, false, &mut expansion, scope)?;
            scope.expansions.insert(binder.id(), expansion);
            return Ok(());
        }
        let ordinal = scope.binders.len();
        scope.binders.insert(binder.id(), ordinal);
        tokens.push(Token::Open(Production::ContractDefine));
        tokens.push(Token::Binder(ordinal));
        self.walk(expression, false, tokens, scope)?;
        tokens.push(Token::Close);
        Ok(())
    }

    fn child(&self, node: NodeId, production: Production) -> Option<NodeId> {
        self.topology
            .node_children(node)?
            .iter()
            .copied()
            .find(|child| {
                self.topology
                    .node(*child)
                    .is_some_and(|record| record.production == production)
            })
    }

    /// The erased `define` a bare atom names, when it names one: an `atom`
    /// whose only content is a `place` of one `pbase` IDENT.
    fn erased_define<'s>(&self, atom: NodeId, scope: &'s Scope) -> Option<&'s Vec<Token>> {
        let children = self.topology.node_children(atom)?;
        let [place] = children else {
            return None;
        };
        let place_children = self.topology.node_children(*place)?;
        let [base] = place_children else {
            return None;
        };
        if self.topology.node(*base)?.production != Production::Pbase
            || !self.topology.node_children(*base)?.is_empty()
        {
            return None;
        }
        let terminal = *self.direct.get(base.index())?.first()?;
        let token = self.classified.tokens().get(terminal)?.token();
        let usage = self
            .uses
            .get(&(token.id().source().ordinal(), token.id().start().value()))?;
        let ResolvedTarget::Source { declaration, .. } = usage.target() else {
            return None;
        };
        scope.expansions.get(&declaration)
    }

    fn start(&self, node: NodeId) -> Result<u64, ResolutionCompilerFailure> {
        match self
            .topology
            .node(node)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .extent
        {
            FinalizedExtent::Source { start, .. } => Ok(start.value()),
            FinalizedExtent::BundleRoot => Err(ResolutionCompilerFailure::InvalidCanonicalTree),
        }
    }

    fn walk(
        &self,
        node: NodeId,
        function: bool,
        tokens: &mut Vec<Token>,
        scope: &mut Scope,
    ) -> Result<(), ResolutionCompilerFailure> {
        let record = self
            .topology
            .node(node)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        // A qualified type or group names its declaration through the use
        // its owner carries; only its final TYPEID's use is compared.
        if record.production == Production::TypePath {
            let last = self
                .direct
                .get(node.index())
                .and_then(|terminals| terminals.last().copied())
                .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
            return self.terminal(last, tokens, scope);
        }
        if record.production == Production::ContractDefine {
            return self.define(node, tokens, scope);
        }
        if record.production == Production::Atom
            && let Some(expansion) = self.erased_define(node, scope)
        {
            tokens.extend(expansion.iter().cloned());
            return Ok(());
        }
        tokens.push(Token::Open(record.production));
        let mut items: Vec<(u64, Result<NodeId, usize>)> = Vec::new();
        for child in self
            .topology
            .node_children(node)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
        {
            items.push((self.start(*child)?, Ok(*child)));
        }
        for terminal in self.direct.get(node.index()).map_or(&[][..], Vec::as_slice) {
            let token = self
                .classified
                .tokens()
                .get(*terminal)
                .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
                .token();
            items.push((token.id().start().value(), Err(*terminal)));
        }
        items.sort_by_key(|(start, _)| *start);
        for (_, item) in items {
            match item {
                Ok(child) => {
                    let production = self
                        .topology
                        .node(child)
                        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
                        .production;
                    if function && matches!(production, Production::Doc | Production::Stmt) {
                        break;
                    }
                    self.walk(child, false, tokens, scope)?;
                }
                Err(terminal) => {
                    let token = self
                        .classified
                        .tokens()
                        .get(terminal)
                        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
                    // A function header ends at its `;` or at its body's `{`.
                    if function
                        && (token
                            .terminals()
                            .contains(TerminalPredicate::Fixed(crate::FixedTerminal::Semicolon))
                            || token.terminals().contains(TerminalPredicate::Fixed(
                                crate::FixedTerminal::LeftBrace,
                            )))
                    {
                        break;
                    }
                    self.terminal(terminal, tokens, scope)?;
                }
            }
        }
        tokens.push(Token::Close);
        Ok(())
    }

    fn terminal(
        &self,
        terminal: usize,
        tokens: &mut Vec<Token>,
        scope: &mut Scope,
    ) -> Result<(), ResolutionCompilerFailure> {
        let classified = self
            .classified
            .tokens()
            .get(terminal)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        let token = classified.token();
        let key = (token.id().source().ordinal(), token.id().start().value());
        let spelling = self
            .classified
            .token_bytes(token)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        if let Some(usage) = self.uses.get(&key) {
            if let ResolvedTarget::Source { declaration, .. } = usage.target()
                && let Some(ordinal) = scope.binders.get(&declaration)
            {
                tokens.push(Token::BinderUse(*ordinal));
                return Ok(());
            }
            tokens.push(Token::Use(usage.target()));
            return Ok(());
        }
        if let Some(declaration) = self.declarations.get(&key) {
            match declaration.role() {
                DeclarationRole::GenericType
                | DeclarationRole::ConstGeneric
                | DeclarationRole::FunctionParameter => {
                    let ordinal = scope.binders.len();
                    scope.binders.insert(declaration.id(), ordinal);
                    tokens.push(Token::Binder(ordinal));
                }
                // A parameter keeps its label, which callers write, and its
                // uses compare by position like a generic binder's.
                DeclarationRole::Parameter => {
                    let ordinal = scope.binders.len();
                    scope.binders.insert(declaration.id(), ordinal);
                    tokens.push(Token::Name(declaration.spelling().to_owned()));
                }
                _ => tokens.push(Token::Name(declaration.spelling().to_owned())),
            }
            return Ok(());
        }
        let is_name = [
            TerminalPredicate::Identifier,
            TerminalPredicate::TypeIdentifier,
            TerminalPredicate::Label,
            TerminalPredicate::OperationName,
        ]
        .iter()
        .any(|predicate| classified.terminals().contains(*predicate));
        tokens.push(if is_name {
            Token::Name(
                std::str::from_utf8(spelling)
                    .map_err(|_| ResolutionCompilerFailure::InvalidNameEncoding)?
                    .to_owned(),
            )
        } else {
            Token::Text(spelling.to_vec())
        });
        Ok(())
    }
}
