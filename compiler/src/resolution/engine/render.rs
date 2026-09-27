//! [MOD-6, MOD-8] the read-only interface renderer.
//!
//! It prints one module's public interface from the resolved unit: each
//! public declaration of the module's interface record in source order, then
//! the complete definition of every struct, enum, interface or binding those
//! declarations reach through a written name, transitively and in first-reached
//! order, private fields included. Every resolved name prints as its qualified
//! identity, `pkg::a::b::Name` (a variant as `pkg::a::b::Enum::Variant`),
//! whatever alias or path wrote it, and `doc` entries are left out.
//!
//! Equal renderings of two revisions mean the same public names, signatures,
//! contracts and complete representations. A representation change behind an
//! unchanged interface file, such as a dependency's field, shows in the text,
//! so comparing renderings is a conservative revision comparison: it can
//! report a change no client can observe, such as a renamed private field,
//! and never misses one. The text is generated for reading and comparing; it
//! supplies no declaration to any check.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::syntax::NodeId;
use crate::syntax::terminal::TerminalPredicate;
use crate::{DeclarationId, ModuleId, Production, SourceRole};

use super::super::{
    DeclarationRecord, DeclarationRole, LexicalUseRecord, ResolutionCompilerFailure, ResolvedTarget,
};

type Coordinate = (u32, u64);

struct Renderer<'a> {
    topology: &'a crate::syntax::FinalizedTopology,
    classified: &'a crate::ClassifiedBundle,
    direct: Vec<Vec<usize>>,
    uses: HashMap<Coordinate, &'a LexicalUseRecord>,
    declarations: HashMap<Coordinate, &'a DeclarationRecord>,
    by_id: HashMap<DeclarationId, &'a DeclarationRecord>,
    /// The top-level item each type-domain or function declaration's own
    /// name belongs to.
    items: HashMap<DeclarationId, NodeId>,
    /// The enum a variant belongs to.
    owners: HashMap<DeclarationId, DeclarationId>,
}

/// Renders one module's public interface; see the module documentation.
pub(in crate::resolution) fn render_interface<'a>(
    topology: &'a crate::syntax::FinalizedTopology,
    classified: &'a crate::ClassifiedBundle,
    declarations: &'a [DeclarationRecord],
    uses: &'a [LexicalUseRecord],
    module: ModuleId,
) -> Result<String, ResolutionCompilerFailure> {
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
    let mut renderer = Renderer {
        topology,
        classified,
        direct,
        uses: uses
            .iter()
            .map(|usage| (key(usage.origin().coordinate()), usage))
            .collect(),
        declarations: declarations
            .iter()
            .map(|declaration| (key(declaration.origin().coordinate()), declaration))
            .collect(),
        by_id: declarations
            .iter()
            .map(|declaration| (declaration.id(), declaration))
            .collect(),
        items: HashMap::new(),
        owners: HashMap::new(),
    };
    let top_items = topology
        .node_children(topology.root)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
        .to_vec();
    for item in &top_items {
        renderer.index_item(*item)?;
    }
    let mut text = String::new();
    let name = classified
        .source_bundle()
        .module(module)
        .map_or_else(|| "pkg".to_owned(), crate::ModuleRecord::qualified_name);
    text.push_str("module ");
    text.push_str(&name);
    text.push('\n');
    let mut rendered = HashSet::new();
    let mut reached = VecDeque::new();
    for item in &top_items {
        let Some(file) = renderer.item_file(*item)? else {
            continue;
        };
        if file.module() != module
            || file.role() != SourceRole::Interface
            || !renderer.is_public(*item)?
        {
            continue;
        }
        rendered.insert(*item);
        renderer.render_item(*item, &mut text, &mut reached)?;
    }
    let mut header = false;
    while let Some(declaration) = reached.pop_front() {
        let Some(item) = renderer.items.get(&declaration).copied() else {
            continue;
        };
        if !rendered.insert(item) {
            continue;
        }
        if !header {
            text.push_str("reached\n");
            header = true;
        }
        renderer.render_item(item, &mut text, &mut reached)?;
    }
    Ok(text)
}

impl Renderer<'_> {
    fn production(&self, node: NodeId) -> Result<Production, ResolutionCompilerFailure> {
        self.topology
            .node(node)
            .map(|record| record.production)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)
    }

    /// The first terminal a node spans, which orders it among its siblings.
    fn first_terminal(&self, node: NodeId) -> Result<usize, ResolutionCompilerFailure> {
        self.topology
            .node(node)
            .and_then(|record| usize::try_from(record.first_terminal).ok())
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)
    }

    fn coordinate(&self, terminal: usize) -> Result<Coordinate, ResolutionCompilerFailure> {
        let token = self
            .classified
            .tokens()
            .get(terminal)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .token();
        Ok((token.id().source().ordinal(), token.id().start().value()))
    }

    fn spelling(&self, terminal: usize) -> Result<&[u8], ResolutionCompilerFailure> {
        let token = self
            .classified
            .tokens()
            .get(terminal)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .token();
        self.classified
            .token_bytes(token)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)
    }

    /// The source record an item belongs to.
    fn item_file(
        &self,
        item: NodeId,
    ) -> Result<Option<&crate::SourceFile>, ResolutionCompilerFailure> {
        let first = self.first_terminal(item)?;
        let source = self
            .topology
            .terminals
            .get(first)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .source;
        Ok(self.classified.source_bundle().file(source))
    }

    /// Whether an item's first terminal is `public`.
    fn is_public(&self, item: NodeId) -> Result<bool, ResolutionCompilerFailure> {
        let first = self.first_terminal(item)?;
        Ok(self
            .classified
            .tokens()
            .get(first)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .terminals()
            .contains(TerminalPredicate::Fixed(crate::FixedTerminal::Public)))
    }

    /// Records which item every declaration named directly by the item's
    /// declaration node, or by one of its variants, belongs to.
    fn index_item(&mut self, item: NodeId) -> Result<(), ResolutionCompilerFailure> {
        let mut pending = vec![item];
        let mut primary = None;
        let mut variants = Vec::new();
        while let Some(node) = pending.pop() {
            let production = self.production(node)?;
            if matches!(
                production,
                Production::Stmt | Production::ContractBlock | Production::Doc
            ) {
                continue;
            }
            for terminal in self.direct.get(node.index()).cloned().unwrap_or_default() {
                let coordinate = self.coordinate(terminal)?;
                let Some(declaration) = self.declarations.get(&coordinate).copied() else {
                    continue;
                };
                match declaration.role() {
                    DeclarationRole::Function
                    | DeclarationRole::Struct
                    | DeclarationRole::Enum
                    | DeclarationRole::Interface
                    | DeclarationRole::Binding
                    | DeclarationRole::NamedConst => {
                        primary.get_or_insert(declaration.id());
                        self.items.insert(declaration.id(), item);
                    }
                    DeclarationRole::Variant => {
                        variants.push(declaration.id());
                        self.items.insert(declaration.id(), item);
                    }
                    _ => {}
                }
            }
            if let Some(children) = self.topology.node_children(node) {
                pending.extend(children.iter().copied());
            }
        }
        if let Some(owner) = primary {
            for variant in variants {
                self.owners.insert(variant, owner);
            }
        }
        Ok(())
    }

    /// A declaration's qualified identity.
    fn qualified(&self, declaration: DeclarationId) -> String {
        let Some(record) = self.by_id.get(&declaration) else {
            return String::new();
        };
        let owner = self
            .owners
            .get(&declaration)
            .map(|owner| self.qualified(*owner));
        match owner {
            Some(owner) => format!("{owner}::{}", record.spelling()),
            None => {
                let module = record
                    .module()
                    .and_then(|module| self.classified.source_bundle().module(module))
                    .map_or_else(|| "pkg".to_owned(), crate::ModuleRecord::qualified_name);
                format!("{module}::{}", record.spelling())
            }
        }
    }

    fn render_item(
        &self,
        item: NodeId,
        text: &mut String,
        reached: &mut VecDeque<DeclarationId>,
    ) -> Result<(), ResolutionCompilerFailure> {
        let mut words = Vec::new();
        self.walk(item, &mut words, reached)?;
        // A `doc` entry ends its declaration with its own `;`, which the
        // rendering leaves out with the entry, so a declaration that ended
        // there ends with the `;` it would otherwise have written.
        if words.last().is_some_and(|word| word != ";" && word != "}") {
            words.push(";".to_owned());
        }
        text.push_str(&words.join(" "));
        text.push('\n');
        Ok(())
    }

    fn walk(
        &self,
        node: NodeId,
        words: &mut Vec<String>,
        reached: &mut VecDeque<DeclarationId>,
    ) -> Result<(), ResolutionCompilerFailure> {
        let production = self.production(node)?;
        if production == Production::Doc {
            return Ok(());
        }
        // A qualified name prints as the identity its last name resolves to.
        if production == Production::TypePath {
            let last = self
                .direct
                .get(node.index())
                .and_then(|terminals| terminals.last().copied())
                .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
            return self.terminal(last, words, reached);
        }
        let mut items: Vec<(usize, Result<NodeId, usize>)> = Vec::new();
        for child in self
            .topology
            .node_children(node)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
        {
            items.push((self.first_terminal(*child)?, Ok(*child)));
        }
        for terminal in self.direct.get(node.index()).map_or(&[][..], Vec::as_slice) {
            items.push((*terminal, Err(*terminal)));
        }
        items.sort_by_key(|(start, _)| *start);
        for (_, item) in items {
            match item {
                Ok(child) => self.walk(child, words, reached)?,
                Err(terminal) => self.terminal(terminal, words, reached)?,
            }
        }
        Ok(())
    }

    fn terminal(
        &self,
        terminal: usize,
        words: &mut Vec<String>,
        reached: &mut VecDeque<DeclarationId>,
    ) -> Result<(), ResolutionCompilerFailure> {
        let coordinate = self.coordinate(terminal)?;
        if let Some(usage) = self.uses.get(&coordinate)
            && let ResolvedTarget::Source { declaration, .. } = usage.target()
            && let Some(record) = self.by_id.get(&declaration)
            && record.module().is_some()
            && self.items.contains_key(&declaration)
        {
            if matches!(
                record.role(),
                DeclarationRole::Struct
                    | DeclarationRole::Enum
                    | DeclarationRole::Interface
                    | DeclarationRole::Binding
                    | DeclarationRole::NamedConst
            ) {
                reached.push_back(declaration);
            }
            if record.role() == DeclarationRole::Variant
                && let Some(owner) = self.owners.get(&declaration)
            {
                reached.push_back(*owner);
            }
            words.push(self.qualified(declaration));
            return Ok(());
        }
        // A declared name prints as its qualified identity too, so a reached
        // definition says which module's declaration it is.
        if let Some(declaration) = self.declarations.get(&coordinate)
            && matches!(
                declaration.role(),
                DeclarationRole::Function
                    | DeclarationRole::Struct
                    | DeclarationRole::Enum
                    | DeclarationRole::Interface
                    | DeclarationRole::Binding
                    | DeclarationRole::NamedConst
            )
            && declaration.module().is_some()
        {
            words.push(self.qualified(declaration.id()));
            return Ok(());
        }
        let spelling = self.spelling(terminal)?;
        words.push(
            std::str::from_utf8(spelling)
                .map_err(|_| ResolutionCompilerFailure::InvalidNameEncoding)?
                .to_owned(),
        );
        Ok(())
    }
}
