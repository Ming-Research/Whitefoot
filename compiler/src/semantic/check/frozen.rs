//! Frozen content eligibility and consuming-place admission [SHARE-1].

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
        let mut pending = vec![(ty, self.checked_type_name(ty)?)];
        let mut seen = HashSet::new();
        while let Some((ty, path)) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            match ty {
                CheckedType::Nominal(id) => match &self.nominal(id)?.kind {
                    CheckedNominalKind::Shared {
                        shape: CheckedShared::Object,
                        ..
                    } => return Ok(Some(path)),
                    CheckedNominalKind::Shared {
                        shape: CheckedShared::Map { .. },
                        ..
                    } => {}
                    CheckedNominalKind::Opaque => {
                        if let Some((template, _)) = &self.source_nominal_instances[id.0 as usize] {
                            let template = &self.nominal_templates[*template];
                            let declaration = template.declaration;
                            if self.declarations.declaration_home(declaration).is_some_and(
                                |(package, module)| {
                                    *package == crate::PackageKey::Standard
                                        && crate::library::is_host_module(module)
                                },
                            ) && self
                                .declarations
                                .tree
                                .children_with(template.node, Production::Field)?
                                .is_empty()
                            {
                                return Ok(Some(path));
                            }
                        }
                    }
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
    /// Run before indexed/ordinary ownership routing so SHARE-1 owns the error
    /// even for copy parts, measures and slots below frozen content.
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
        let mut ty = local.ty;
        let mut reference = local.mode.is_reference();
        let mut frozen_content = false;
        for &suffix in suffixes {
            match self.types.declarations.tree.place_suffix(suffix)? {
                PlaceSuffix::Dereference if reference => {
                    if let Some(info) = &local.reference {
                        for path in &info.paths {
                            if self
                                .types
                                .frozen_member_on_resolved_path(context, path, bindings)?
                                .is_some()
                            {
                                if explicit_move {
                                    return self.frozen_consume_issue(place);
                                }
                                frozen_content = true;
                            }
                        }
                    }
                    reference = false;
                }
                PlaceSuffix::Member(_) if !reference => {
                    let name = self
                        .types
                        .declarations
                        .deferred_use_at(suffix, DeferredUseRole::ProjectedField)?
                        .spelling();
                    if name == "inner" && self.types.is_frozen_type(ty)? {
                        if explicit_move {
                            return self.frozen_consume_issue(place);
                        }
                        frozen_content = true;
                    }
                    let Some(member) = self.types.place_member(ty, name)? else {
                        return Ok(());
                    };
                    ty = member.ty();
                }
                PlaceSuffix::Index { .. } | PlaceSuffix::Range { .. } => {
                    ty = match ty {
                        CheckedType::Array { element, .. }
                        | CheckedType::Window { element, .. }
                        | CheckedType::Buffer { element }
                        | CheckedType::Segments { element } => self.types.element_type(element)?,
                        _ => return Ok(()),
                    };
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
