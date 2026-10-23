//! Frozen content eligibility and consuming-place admission [SHARE-1], and
//! the part walk that also finds a self-changing host handle [SHARE-2].

use std::collections::{HashMap, HashSet};

use super::super::model::{CheckedNominalKind, CheckedReleaseClass, CheckedShared, CheckedType};
use super::{CheckContext, CheckStop, Checker, LocalBinding, TypeContext};
use crate::syntax::NodeId;
use crate::{DeclarationId, Production, SemanticIssueKind, SemanticRule};

impl TypeContext<'_> {
    pub(super) fn is_frozen_content_parameter(
        &self,
        parameter: DeclarationId,
    ) -> Result<bool, CheckStop> {
        for (name, node, parameters) in self
            .nominal_templates
            .iter()
            .map(|t| (&t.name, t.node, &t.generic_parameters))
            .chain(
                self.function_templates
                    .iter()
                    .map(|t| (&t.name, t.node, &t.generic_parameters)),
            )
        {
            if !matches!(name.as_str(), "Frozen" | "frozen_new" | "frozen_share")
                || !parameters
                    .iter()
                    .any(|p| p.key() == super::generics::GenericParameterKey::Source(parameter))
            {
                continue;
            }
            return Ok(self.declarations.tree.is_prelude_node(node)?);
        }
        Ok(false)
    }

    /// Depth-first declaration order, with nominal cycles visited once. Symbolic
    /// parameters are checked again at each concrete instantiation.
    pub(super) fn first_unfrozen_part(&self, ty: CheckedType) -> Result<Option<String>, CheckStop> {
        self.first_part(ty, false, false, |types, ty| {
            let CheckedType::Nominal(id) = ty else {
                return Ok(false);
            };
            Ok(match &types.nominal(id)?.kind {
                CheckedNominalKind::Shared {
                    shape: CheckedShared::Object,
                    ..
                } => true,
                CheckedNominalKind::Opaque => types
                    .host_opaque_template(id)?
                    .is_some_and(|host| host.fieldless),
                _ => false,
            })
        })
    }

    /// [SHARE-2, PRE-2] the first part of `ty`, `ty` included, that is a
    /// self-changing host handle, in the same walk as frozen content. A
    /// shared handle is a part without parts of its own: its state is reached
    /// only through an atomic statement's target. A map's entries are its
    /// elements [SHARE-1, OP-4], so a path of type `ConcurrentHashMap<V>`
    /// has each entry's `Option<V>` and its parts as parts.
    pub(super) fn first_self_changing_part(
        &self,
        ty: CheckedType,
    ) -> Result<Option<String>, CheckStop> {
        self.first_part(ty, true, false, |types, ty| {
            let CheckedType::Nominal(id) = ty else {
                return Ok(false);
            };
            if !matches!(types.nominal(id)?.kind, CheckedNominalKind::Opaque) {
                return Ok(false);
            }
            Ok(types
                .host_opaque_template(id)?
                .is_some_and(|host| {
                    crate::library::is_self_changing_host_handle(host.module, host.name)
                }))
        })
    }

    /// [SHARE-1] the first part of `state`, `state` included, that is the
    /// handle `handle`, in the frozen walk extended through the state behind
    /// each object handle: a value of `state` could then hold a handle to the
    /// object it is the state of.
    pub(super) fn first_own_handle_part(
        &self,
        state: CheckedType,
        handle: super::super::model::NominalId,
    ) -> Result<Option<String>, CheckStop> {
        self.first_part(state, false, true, |_, ty| {
            Ok(ty == CheckedType::Nominal(handle))
        })
    }

    /// [PRE-2] whether `id` is the host cancellation watch, whose `fired`
    /// member reads its state's flag in place.
    pub(super) fn is_cancel_watch(
        &self,
        id: super::super::model::NominalId,
    ) -> Result<bool, CheckStop> {
        Ok(self
            .host_opaque_template(id)?
            .is_some_and(|host| crate::library::is_cancel_watch(host.module, host.name)))
    }

    /// [PRE-2] whether a nominal template is the host cancellation watch's.
    pub(super) fn is_cancel_watch_template(
        &self,
        template: &super::NominalTemplate,
    ) -> Result<bool, CheckStop> {
        let Some((package, module)) = self.declarations.declaration_home(template.declaration)
        else {
            return Ok(false);
        };
        Ok(*package == crate::PackageKey::Standard
            && crate::library::is_cancel_watch(module, template.name.as_str()))
    }

    /// The host module, name and fieldlessness of an opaque nominal a host
    /// module declares [PRE-2], or `None` for any other opaque nominal.
    fn host_opaque_template(
        &self,
        id: super::super::model::NominalId,
    ) -> Result<Option<HostOpaque<'_>>, CheckStop> {
        let Some((template, _)) = &self.source_nominal_instances[id.0 as usize] else {
            return Ok(None);
        };
        let template = &self.nominal_templates[*template];
        let Some((package, module)) = self.declarations.declaration_home(template.declaration)
        else {
            return Ok(None);
        };
        if *package != crate::PackageKey::Standard || !crate::library::is_host_module(module) {
            return Ok(None);
        }
        let fieldless = self
            .declarations
            .tree
            .children_with(template.node, Production::Field)?
            .is_empty();
        Ok(Some(HostOpaque {
            module,
            name: template.name.as_str(),
            fieldless,
        }))
    }

    /// The first part of `ty` in depth-first declaration order, `ty` itself
    /// included, that `stops` selects, rendered as a path from the type's
    /// name; nominal cycles are visited once. With `map_entries` a map's
    /// entries are walked as its elements. The frozen walk leaves them out:
    /// a map is only ever a shared object's state or a `swap`'s exchange
    /// [TYPE-9], never a part of frozen content, and the frozen walk stops at
    /// the `Shared` handle that holds one. With `object_states` the walk goes
    /// on through the state behind each object handle.
    fn first_part(
        &self,
        ty: CheckedType,
        map_entries: bool,
        object_states: bool,
        stops: impl Fn(&Self, CheckedType) -> Result<bool, CheckStop>,
    ) -> Result<Option<String>, CheckStop> {
        let mut pending = vec![(ty, self.checked_type_name(ty)?)];
        let mut seen = HashSet::new();
        while let Some((ty, path)) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            if stops(self, ty)? {
                return Ok(Some(path));
            }
            match ty {
                CheckedType::Nominal(id) => match &self.nominal(id)?.kind {
                    CheckedNominalKind::Shared { state, shape } => {
                        if map_entries && let CheckedShared::Map { entry } = shape {
                            pending.push((*entry, format!("{path}[element]")));
                        }
                        if object_states && matches!(shape, CheckedShared::Object) {
                            pending.push((*state, format!("{path}[state]")));
                        }
                    }
                    CheckedNominalKind::Opaque => {}
                    CheckedNominalKind::Box { referent, .. } => {
                        pending.push((*referent, format!("{path}.inner")))
                    }
                    CheckedNominalKind::Struct { fields } => {
                        for field in fields.iter().rev() {
                            pending.push((field.ty, format!("{path}.{}", field.name)));
                        }
                    }
                    CheckedNominalKind::Enum { variants } => {
                        for variant in variants.iter().rev() {
                            for field in variant.fields.iter().rev() {
                                pending.push((
                                    field.ty,
                                    format!("{path}.{}.{}", variant.name, field.name),
                                ));
                            }
                        }
                    }
                },
                CheckedType::Array { element, .. }
                | CheckedType::Window { element, .. }
                | CheckedType::Buffer { element }
                | CheckedType::Segments { element }
                | CheckedType::Entries { element } => {
                    pending.push((self.element_type(element)?, format!("{path}[element]")))
                }
                _ => {}
            }
        }
        Ok(None)
    }

    pub(super) fn reject_frozen_part(
        &self,
        site: NodeId,
        ty: CheckedType,
    ) -> Result<(), CheckStop> {
        if let Some(part) = self.first_unfrozen_part(ty)? {
            return self.declarations.issue_node(SemanticRule::Share1, site, SemanticIssueKind::FrozenForbiddenPart {
                mechanical_fix: format!("replace `{part}` with immutable owned data or a Frozen handle before freezing the value"),
                part,
            });
        }
        Ok(())
    }

    pub(super) fn is_frozen_type(&self, ty: CheckedType) -> Result<bool, CheckStop> {
        Ok(
            matches!(ty, CheckedType::Nominal(id) if matches!(self.nominal(id)?.kind,
            CheckedNominalKind::Box { release: CheckedReleaseClass::Frozen, .. })),
        )
    }
}

impl Checker<'_, '_> {
    /// Inspect the complete selection before indexed/ordinary ownership routing.
    /// [DIAG-1] selects the earlier [OWN-1] for copy moves and bare affine
    /// uses; [SHARE-1] owns explicit non-copy consumes through frozen content.
    pub(super) fn reject_frozen_consume(
        &self,
        context: &CheckContext<'_>,
        place: NodeId,
        base: NodeId,
        suffixes: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
        explicit_move: bool,
    ) -> Result<(), CheckStop> {
        use crate::syntax::views::PlaceSuffix;
        use crate::{DeclarationClass, DeferredUseRole, LexicalUseRole, ResolvedTarget};
        if !explicit_move {
            return Ok(());
        }
        if !self.types.declarations.tree.children(base)?.is_empty() {
            return Ok(());
        }
        let usage = self
            .types
            .declarations
            .use_at(context, base, LexicalUseRole::PlaceBase)?;
        let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::Value,
        } = usage.target()
        else {
            return Ok(());
        };
        let Some(local) = bindings.get(&declaration) else {
            return Ok(());
        };
        if !local.live {
            return Ok(());
        }
        let mut ty = local.ty;
        // [DIAG-1] [SHARE-1] owns only a consume that no earlier rule refuses:
        // an explicit move of a place rooted in a live own-mode binding with
        // no `^` (OWN-1's move through a reference) and no element step
        // (WIN-3's element take).
        if local.mode.is_reference() {
            return Ok(());
        }
        let mut frozen_content = false;
        for &suffix in suffixes {
            match self.types.declarations.tree.place_suffix(suffix)? {
                PlaceSuffix::Member(_) => {
                    let name = self
                        .types
                        .declarations
                        .deferred_use_at(suffix, DeferredUseRole::ProjectedField)?
                        .spelling();
                    if name == "inner" && self.types.is_frozen_type(ty)? {
                        frozen_content = true;
                    }
                    let Some(member) = self.types.place_member(ty, name)? else {
                        return Ok(());
                    };
                    ty = member.ty();
                }
                _ => return Ok(()),
            }
        }
        if frozen_content && !self.types.is_copy_type(context, ty)? {
            return self.frozen_consume_issue(place);
        }
        Ok(())
    }

    pub(super) fn frozen_consume_issue<T>(&self, place: NodeId) -> Result<T, CheckStop> {
        self.types.declarations.issue_node(SemanticRule::Share1, place, SemanticIssueKind::FrozenContentConsume {
            mechanical_fix: "read copy-typed parts without move, borrow `&f.inner` or its descendants, or retain a nested frozen handle with frozen_share",
        })
    }
}

/// An opaque nominal a host module declares [PRE-2]: its module, its name and
/// whether its declaration has no fields.
struct HostOpaque<'a> {
    module: &'a [String],
    name: &'a str,
    fieldless: bool,
}
