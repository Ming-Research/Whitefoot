//! Source identities of retained products, resolved in the current unit.

use std::collections::BTreeMap;

use super::{Identity, IdentityKind, record_enum};
use crate::syntax::views::SyntaxView;
use crate::{DeclarationKey, ItemHome, ItemKey, NodePath, ResolvedSyntaxUnit, SourceOrigin};

/// A source reference whose meaning survives dense inventory renumbering.
pub(crate) enum SourceIdentity {
    Declaration(String),
    Module(String),
    Item(String),
    Node { item: String, path: Vec<u32> },
}

record_enum!(SourceIdentity {
    0 => Declaration(key),
    1 => Module(key),
    2 => Item(key),
    3 => Node { item, path },
});

/// The interface declaration and implementation of a callable denote one
/// boundary. Other item kinds have only one declaration in their module.
pub(crate) fn boundary_item(key: &ItemKey) -> ItemKey {
    match key {
        ItemKey::Declared {
            home: ItemHome::Module { package, path, .. },
            role,
            spelling,
        } => ItemKey::Declared {
            home: ItemHome::Module {
                package: *package,
                path: path.clone(),
                record: crate::SourceRole::Implementation,
            },
            role: *role,
            spelling: spelling.clone(),
        },
        other => other.clone(),
    }
}

fn boundary_declaration(key: &DeclarationKey) -> DeclarationKey {
    match key {
        DeclarationKey::Item(item) => DeclarationKey::Item(boundary_item(item)),
        DeclarationKey::Local {
            item,
            path,
            ordinal,
        } => DeclarationKey::Local {
            item: boundary_item(item),
            path: path.clone(),
            ordinal: *ordinal,
        },
    }
}

fn implementation(key: &ItemKey) -> bool {
    matches!(
        key,
        ItemKey::Declared {
            home: ItemHome::Module {
                record: crate::SourceRole::Implementation,
                ..
            },
            ..
        }
    )
}

pub(crate) struct SourceIdentities<'a> {
    resolved: &'a ResolvedSyntaxUnit,
    view: &'a SyntaxView<'a>,
    declarations: BTreeMap<String, u32>,
    items: BTreeMap<String, u32>,
    modules: BTreeMap<String, u32>,
}

impl<'a> SourceIdentities<'a> {
    pub(crate) fn new(resolved: &'a ResolvedSyntaxUnit, view: &'a SyntaxView<'a>) -> Option<Self> {
        let mut declarations = BTreeMap::new();
        // Select definitions when present, as the ordinary checker does;
        // a module-only check selects the dependency's interface instead.
        for definition in [false, true] {
            for declaration in resolved.declarations() {
                if implementation(declaration.key().item()) == definition {
                    declarations.insert(
                        boundary_declaration(declaration.key()).to_string(),
                        u32::try_from(declaration.id().index()).ok()?,
                    );
                }
            }
        }
        let mut items = BTreeMap::new();
        for definition in [false, true] {
            for (ordinal, item) in resolved.item_keys() {
                if implementation(item) == definition {
                    items.insert(boundary_item(item).to_string(), ordinal);
                }
            }
        }
        let modules = resolved
            .syntax()
            .classified_bundle()
            .source_bundle()
            .modules()
            .iter()
            .enumerate()
            .map(|(index, module)| Some((module.qualified_name(), u32::try_from(index).ok()?)))
            .collect::<Option<_>>()?;
        Some(Self {
            resolved,
            view,
            declarations,
            items,
            modules,
        })
    }

    pub(crate) fn name(&self, (kind, index): Identity) -> Option<SourceIdentity> {
        Some(match kind {
            IdentityKind::Declaration => {
                let declaration = self
                    .resolved
                    .declaration(crate::DeclarationId::from_index(index as usize)?)?;
                SourceIdentity::Declaration(boundary_declaration(declaration.key()).to_string())
            }
            IdentityKind::Module => SourceIdentity::Module(
                self.resolved
                    .syntax()
                    .classified_bundle()
                    .source_bundle()
                    .module(crate::ModuleId::from_index(index as usize)?)?
                    .qualified_name(),
            ),
            IdentityKind::Item => {
                SourceIdentity::Item(boundary_item(self.resolved.item_key(index)?).to_string())
            }
            IdentityKind::Node => {
                let node = crate::syntax::NodeId::from_index(index as usize)?;
                let key = self.resolved.occurrence_key(self.view.path(node).ok()?)?;
                SourceIdentity::Node {
                    item: boundary_item(&key.item).to_string(),
                    path: key.path,
                }
            }
            _ => return None,
        })
    }

    pub(crate) fn resolve(&self, name: &SourceIdentity) -> Option<Identity> {
        Some(match name {
            SourceIdentity::Declaration(key) => {
                (IdentityKind::Declaration, *self.declarations.get(key)?)
            }
            SourceIdentity::Module(key) => (IdentityKind::Module, *self.modules.get(key)?),
            SourceIdentity::Item(key) => (IdentityKind::Item, *self.items.get(key)?),
            SourceIdentity::Node { item, path } => {
                let mut components = vec![*self.items.get(item)?];
                components.extend_from_slice(path);
                let node = self.view.node_with_path(&NodePath { components })?;
                (IdentityKind::Node, u32::try_from(node.index()).ok()?)
            }
        })
    }

    /// Read the current spelling coordinate, including changes before this
    /// role inside a declaration. Old byte offsets are never translated.
    pub(crate) fn origin(&self, path: &NodePath, role: u32, subtoken: u32) -> Option<SourceOrigin> {
        let node = self.view.node_with_path(path)?;
        let matches = |origin: &&SourceOrigin| {
            origin.node() == path
                && origin.role_ordinal() == role
                && origin.subtoken_ordinal() == subtoken
        };
        self.resolved
            .declarations_at(node)
            .map(|record| record.origin())
            .chain(
                self.resolved
                    .dependent_declarations_at(node)
                    .map(|record| record.origin()),
            )
            .chain(
                self.resolved
                    .lexical_uses_at(node)
                    .map(|record| record.origin()),
            )
            .chain(
                self.resolved
                    .deferred_uses_at(node)
                    .map(|record| record.origin()),
            )
            .chain(self.resolved.postconditions().iter().flat_map(|record| {
                record
                    .result_binders
                    .iter()
                    .map(|candidate| &candidate.origin)
                    .chain(
                        record
                            .fields
                            .iter()
                            .flat_map(|field| [&field.origin, &field.candidate.origin]),
                    )
                    .chain(record.provisional_uses.iter().map(|usage| usage.origin()))
                    .chain(record.selector_uses.iter().map(|usage| &usage.origin))
            }))
            .find(matches)
            .cloned()
    }
}
