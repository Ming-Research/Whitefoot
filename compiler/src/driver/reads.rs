//! [MOD-8] What one module verdict reads of other modules: the declarations
//! of their interface records its check reached, each named by module, role
//! and spelling and identified by a digest of its meaning.
//!
//! A module's check reads every interface record of its dependency closure,
//! and it needs each of them to hold its own judgments, since a defect in
//! any of them rejects the check. Beyond that, it reads only the
//! declarations its own records name and, transitively, the declarations
//! their items name: a signature's types, a type's fields, a contract's
//! constants. An edit to an interface that changes none of those, such as a
//! reworded `doc` entry or an unused declaration's body of text, leaves the
//! verdict as it was, and the verdict's record says so without the check
//! running again [MOD-8].
//!
//! A declaration's digest is the canonical text of its item with every
//! `doc` string emptied, preceded by its record's alias header, whose
//! bindings give the item's names their targets [MOD-4]. The text is
//! canonical [FORM-2], so a digest moves with any written change to the
//! item except a documentation string, and with any change to an alias.

use std::collections::{BTreeMap, BTreeSet};

use super::{CompilerLimits, canonical_syntax};
use crate::source::SourceBundle;
use crate::syntax::terminal::TerminalPredicate;
use crate::syntax::{FinalizedExtent, NodeId};
use crate::{DeclarationRole, Production, ResolvedTarget, SourceInput};

/// One declaration by its item's role and spelling, as a record declares it.
pub(super) type ItemName = (String, String);

/// The role name of an item's declaration production.
const fn item_role(production: Production) -> Option<&'static str> {
    Some(match production {
        Production::FnDecl => "fn",
        Production::StructDecl => "struct",
        Production::EnumDecl => "enum",
        Production::InterfaceDecl => "interface",
        Production::BindingDecl => "binding",
        Production::ConstDecl => "const",
        _ => return None,
    })
}

/// The role name of a declaration that heads an item.
const fn declaration_role(role: DeclarationRole) -> Option<&'static str> {
    Some(match role {
        DeclarationRole::Function => "fn",
        DeclarationRole::Struct => "struct",
        DeclarationRole::Enum => "enum",
        DeclarationRole::Interface => "interface",
        DeclarationRole::Binding => "binding",
        DeclarationRole::NamedConst => "const",
        _ => return None,
    })
}

/// The digest of each declaration of one record, by role and spelling.
pub(super) type Digests = BTreeMap<ItemName, [u8; 32]>;

/// What the checks that read one record take from it: the digest of each
/// declaration, and the record's text with every `doc` string emptied.
#[derive(Clone)]
struct Reading {
    digests: Digests,
    meaning: Vec<u8>,
}

thread_local! {
    /// [`reading`] by the SHA-256 of the record's bytes: a pure function of
    /// those bytes, which one invocation's checks ask again for each module
    /// whose closure holds the record.
    static READINGS: std::cell::RefCell<std::collections::HashMap<[u8; 32], Option<Reading>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn reading(bytes: &[u8], limits: CompilerLimits) -> Option<Reading> {
    let key = crate::spec::sha256::digest(bytes);
    if let Some(known) = READINGS.with(|readings| readings.borrow().get(&key).cloned()) {
        return known;
    }
    let reading = record_reading(bytes, limits);
    READINGS.with(|known| known.borrow_mut().insert(key, reading.clone()));
    reading
}

/// The digest of every top-level declaration one record writes, or `None`
/// when the record does not pass the syntax stages, which no verdict that
/// read it can then stand on.
pub(super) fn declaration_digests(bytes: &[u8], limits: CompilerLimits) -> Option<Digests> {
    reading(bytes, limits).map(|reading| reading.digests)
}

/// The record's canonical text with every `doc` string emptied, or `None`
/// when the record does not pass the syntax stages. Only an edit to a
/// documentation string leaves it unchanged, so it stands for everything a
/// check concludes from the record: repeated or reordered declarations and
/// an alias header change it even where the digests of the declarations,
/// which name each declaration once, stay the same [MOD-8].
pub(super) fn record_meaning(bytes: &[u8], limits: CompilerLimits) -> Option<Vec<u8>> {
    reading(bytes, limits).map(|reading| reading.meaning)
}

/// `bytes[start..end]` with each `doc` entry in `docs`, sorted and inside
/// that range, written as an empty `doc` entry.
fn without_docs(bytes: &[u8], (start, end): (usize, usize), docs: &[(usize, usize)]) -> Vec<u8> {
    let mut text = Vec::with_capacity(end.saturating_sub(start));
    let mut at = start;
    for &(doc_start, doc_end) in docs {
        text.extend_from_slice(bytes.get(at..doc_start).unwrap_or_default());
        text.extend_from_slice(b"doc \"\";");
        at = doc_end;
    }
    text.extend_from_slice(bytes.get(at..end).unwrap_or_default());
    text
}

fn record_reading(bytes: &[u8], limits: CompilerLimits) -> Option<Reading> {
    let input = [SourceInput::new("interface.wf", bytes)];
    let bundle = SourceBundle::with_limits(&input, limits.source).ok()?;
    let canonical = canonical_syntax(&bundle, limits, false).ok()?;
    let topology = &canonical.finalized.topology;
    let classified = canonical.classified_bundle();
    let tokens = classified.tokens();
    let mut direct = vec![Vec::new(); topology.nodes.len()];
    for (index, terminal) in topology.terminals.iter().enumerate() {
        if let Some(owner) = terminal.owner
            && let Some(slot) = direct.get_mut(owner.index())
        {
            slot.push(index);
        }
    }
    let extent = |node: NodeId| match topology.node(node).map(|record| record.extent) {
        Some(FinalizedExtent::Source { start, end, .. }) => Some((
            usize::try_from(start.value()).ok()?,
            usize::try_from(end.value()).ok()?,
        )),
        _ => None,
    };
    // Every `doc` entry under `node`, whose string no judgment reads.
    let docs_under = |node: NodeId| {
        let mut docs = Vec::new();
        let mut pending = vec![node];
        while let Some(node) = pending.pop() {
            let children = topology.node_children(node).unwrap_or(&[]);
            for child in children {
                if topology.node(*child).map(|record| record.production) == Some(Production::Doc) {
                    if let Some(range) = extent(*child) {
                        docs.push(range);
                    }
                } else {
                    pending.push(*child);
                }
            }
        }
        docs.sort_unstable();
        docs
    };
    let meaning = without_docs(bytes, (0, bytes.len()), &docs_under(topology.root));
    let mut header = Vec::new();
    let mut items = Vec::new();
    for item in topology.node_children(topology.root).unwrap_or(&[]) {
        let Some(&declaration) = topology.node_children(*item).and_then(<[NodeId]>::first) else {
            continue;
        };
        let Some((start, end)) = extent(*item) else {
            continue;
        };
        let production = topology.node(declaration).map(|record| record.production);
        let Some(role) = production.and_then(item_role) else {
            // An alias or heap declaration binds the record's names.
            header.extend_from_slice(bytes.get(start..end).unwrap_or_default());
            header.push(b'\n');
            continue;
        };
        let spelling = direct
            .get(declaration.index())
            .into_iter()
            .flatten()
            .find_map(|terminal| {
                let token = tokens.get(*terminal)?;
                (token.terminals().contains(TerminalPredicate::Identifier)
                    || token
                        .terminals()
                        .contains(TerminalPredicate::TypeIdentifier))
                .then(|| std::str::from_utf8(classified.token_bytes(token.token())?).ok())
                .flatten()
                .map(str::to_owned)
            });
        let Some(spelling) = spelling else {
            continue;
        };
        let text = without_docs(bytes, (start, end), &docs_under(*item));
        items.push(((role.to_owned(), spelling), text));
    }
    let mut digests = BTreeMap::new();
    for (name, text) in items {
        let mut material = b"declaration 1\nheader\n".to_vec();
        material.extend_from_slice(&header);
        material.extend_from_slice(b"item\n");
        material.extend_from_slice(&text);
        digests.insert(name, crate::spec::sha256::digest(&material));
    }
    Some(Reading { digests, meaning })
}

/// The declarations of other modules that one check of `target` reached:
/// every declaration its own records name, and every declaration the items
/// of reached declarations name, by module, role and spelling. Items are
/// followed by the keys resolution minted for them. PRE-1 declarations are
/// the compiler's own and belong to no module.
pub(super) fn read_declarations(
    resolved: &crate::ResolvedSyntaxUnit,
    target: crate::ModuleId,
) -> Option<BTreeSet<(crate::ModuleId, ItemName)>> {
    let bundle = resolved.syntax().classified_bundle().source_bundle();
    let module_of = |key: &crate::ItemKey| match key {
        crate::ItemKey::Declared {
            home: crate::ItemHome::Module { package, path, .. },
            ..
        } => bundle
            .modules()
            .iter()
            .position(|module| module.is_at(*package, path)),
        _ => None,
    };
    let item_of = |node: &crate::NodePath| resolved.item_key(*node.components().first()?);
    // The items each item's uses name.
    let mut named = BTreeMap::<&crate::ItemKey, BTreeSet<&crate::ItemKey>>::new();
    let uses = resolved.lexical_uses().iter().chain(
        resolved
            .postconditions()
            .iter()
            .flat_map(|record| &record.provisional_uses),
    );
    for record in uses {
        let ResolvedTarget::Source { declaration, .. } = record.target() else {
            continue;
        };
        let Some(from) = item_of(record.origin().node()) else {
            continue;
        };
        let to = resolved.declaration(declaration)?.key().item();
        if from != to {
            named.entry(from).or_default().insert(to);
        }
    }
    let topology = &resolved.syntax().finalized.topology;
    let items = topology.node_children(topology.root)?.len();
    let mut pending = (0..items)
        .filter_map(|ordinal| resolved.item_key(u32::try_from(ordinal).ok()?))
        .filter(|key| module_of(key) == Some(target.index()))
        .collect::<Vec<_>>();
    let mut visited = pending.iter().copied().collect::<BTreeSet<_>>();
    let mut reached = BTreeSet::new();
    while let Some(item) = pending.pop() {
        for next in named.get(item).into_iter().flatten().copied() {
            if !visited.insert(next) {
                continue;
            }
            // A PRE-1 item is the compiler's own; its meaning is the
            // compiler's identity, which scopes every record.
            let Some(module) = module_of(next) else {
                continue;
            };
            let crate::ItemKey::Declared { role, spelling, .. } = next else {
                return None;
            };
            reached.insert((
                crate::ModuleId::from_index(module)?,
                (declaration_role(*role)?.to_owned(), spelling.clone()),
            ));
            pending.push(next);
        }
    }
    Some(reached)
}
