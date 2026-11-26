//! Payloads explain the capability the ordinary ownership judgment found.

use super::*;

impl Checker<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantic::check) fn ownership_issue(
        &self,
        check_context: &CheckContext<'_>,
        place: NodeId,
        ty: CheckedType,
        copy: bool,
        reference: Option<CheckedMode>,
        repair: Option<String>,
    ) -> Result<SemanticIssueKind, CheckStop> {
        let binding = self.types.declarations.tree.source_spelling(place)?;
        let ty_name = self.types.checked_type_name(ty)?;
        let ty_name = if let Some(mode) = reference {
            self.types.checked_value_name(mode, ty)?
        } else {
            ty_name
        };
        let constant_noncopy =
            copy && reference.is_none() && !self.types.is_copy_type(check_context, ty)?;
        let reason = if constant_noncopy {
            format!("`{binding}` has type {ty_name}; this constant is not owned storage and cannot be consumed")
        } else if copy {
            format!("`{binding}` has type {ty_name}; every part is copy")
        } else if reference.is_some() {
            format!("`{binding}` has type {ty_name}; a reference owns no payload to consume")
        } else {
            let part = self.types.first_noncopy_part(check_context, ty)?;
            format!("`{binding}` has type {ty_name}; {part}")
        };
        if copy {
            Ok(SemanticIssueKind::MoveOfCopy {
                mechanical_fix: if constant_noncopy {
                    super::super::repairs::writer_ownership::constant().to_owned()
                } else {
                    repair.unwrap_or_else(|| {
                        super::super::repairs::writer_ownership::copy(&binding)
                    })
                },
                binding,
                ty: ty_name,
                reason,
            })
        } else {
            Ok(SemanticIssueKind::BareAffineUse {
                mechanical_fix: repair.unwrap_or_else(|| {
                    super::super::repairs::writer_ownership::consume(&binding)
                }),
                binding,
                ty: ty_name,
                reason,
            })
        }
    }
}

impl TypeContext<'_> {
    fn first_noncopy_part(
        &self,
        context: &CheckContext<'_>,
        ty: CheckedType,
    ) -> Result<String, CheckStop> {
        match ty {
            CheckedType::Nominal(id) => {
                let nominal = self.nominal(id)?;
                if nominal.nocopy || nominal.linear {
                    return Ok(format!(
                        "its type {} is declared non-copy",
                        self.checked_type_name(ty)?
                    ));
                }
                match &nominal.kind {
                    CheckedNominalKind::Struct { fields } => {
                        for field in fields {
                            if !self.is_copy_type(context, field.ty)? {
                                return Ok(format!(
                                    "its `{}` field has non-copy type {}",
                                    field.name,
                                    self.checked_type_name(field.ty)?
                                ));
                            }
                        }
                    }
                    CheckedNominalKind::Enum { variants } => {
                        for variant in variants {
                            for field in &variant.fields {
                                if !self.is_copy_type(context, field.ty)? {
                                    return Ok(format!(
                                        "its {} payload `{}` has non-copy type {}",
                                        variant.name,
                                        field.name,
                                        self.checked_type_name(field.ty)?
                                    ));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            CheckedType::Array { element, .. }
            | CheckedType::Buffer { element }
            | CheckedType::Window { element, .. }
            | CheckedType::Segments { element } => {
                let element = self.element_type(element)?;
                if !self.is_copy_type(context, element)? {
                    return Ok(format!(
                        "its element has non-copy type {}",
                        self.checked_type_name(element)?
                    ));
                }
            }
            _ => {}
        }
        Ok(format!(
            "its type {} has no copy capability",
            self.checked_type_name(ty)?
        ))
    }
}
