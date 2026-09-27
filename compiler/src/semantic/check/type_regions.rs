//! Declaration-local region positions and their actual-type correspondence.
//!
//! [FORM-8, TYPE-5] follows the finite written type tree, never nominal fields.
//! In particular, instantiating an opaque `T` does not add formal positions.
//! The stored shape has no temporary nominal IDs: constructor templates keep
//! it after their symbolic nominal checkpoint has been restored.

use crate::syntax::NodeId;
use crate::{DeclarationClass, DeclarationId, LexicalUseRole, Production, ResolvedTarget};

use super::super::model::{CheckedNominalKind, CheckedType};
use super::{CheckStop, Checker, PreludeType, SemanticCompilerFailure};

#[derive(Clone, Copy)]
pub(super) struct RegionPosition {
    pub(super) formal: DeclarationId,
}

#[derive(Clone)]
pub(super) struct TypeRegionShape {
    regions: Vec<RegionPosition>,
    arguments: Vec<Self>,
}

impl TypeRegionShape {
    pub(super) fn determines(&self, formal: DeclarationId) -> bool {
        self.regions
            .iter()
            .any(|position| position.formal == formal)
            || self
                .arguments
                .iter()
                .any(|argument| argument.determines(formal))
    }
}

impl<'unit> Checker<'unit> {
    /// [BLK-4] the complete value type, including phantom nominal brands and
    /// instantiated element types. This is independent of the declaration's
    /// region-argument matching shape: an opaque `T` can carry confinement.
    pub(super) fn confinement_regions(
        &self,
        ty: CheckedType,
    ) -> Result<Vec<DeclarationId>, CheckStop> {
        let mut pending = vec![ty];
        let mut visited = std::collections::HashSet::new();
        let mut regions = Vec::new();
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            let (axes, arguments) = self.type_region_axes(ty)?;
            regions.extend(axes.into_iter().map(|axis| axis.formal));
            pending.extend(arguments);
            if let CheckedType::Nominal(id) = ty {
                match &self.nominal(id)?.kind {
                    CheckedNominalKind::Struct { fields } => {
                        pending.extend(fields.iter().map(|field| field.ty));
                    }
                    CheckedNominalKind::Enum { variants } => {
                        pending.extend(
                            variants
                                .iter()
                                .flat_map(|variant| variant.fields.iter().map(|field| field.ty)),
                        );
                    }
                    _ => {}
                }
            }
        }
        regions.sort_by_key(|region| region.index());
        regions.dedup();
        Ok(regions)
    }

    /// An instantiated type parameter can carry a caller's store brand even
    /// though that brand's source declaration belongs to a different body.
    /// Input values and explicit type arguments supply those regions for
    /// this invocation [FN-2, OWN-3].
    pub(super) fn check_confined_destination(
        &self,
        function: &super::FunctionSignature,
        ty: CheckedType,
        destination: Option<DeclarationId>,
        node: NodeId,
    ) -> Result<(), CheckStop> {
        // [BLK-4] had no successor: v0.60 has one heap [STOR-8], no region
        // block, and no confined value, so no type names a region a
        // destination could escape. The walk is retained only so that its
        // two remaining callers keep one entry point while the region-shape
        // apparatus is removed with the storage package.
        let _ = (ty, function, destination, node);
        Ok(())
    }

    /// The name's own region and type-argument axes, without its fields.
    fn type_region_axes(
        &self,
        ty: CheckedType,
    ) -> Result<(Vec<RegionPosition>, Vec<CheckedType>), CheckStop> {
        let one_region = |formal| vec![RegionPosition { formal }];
        let axes = match ty {
            CheckedType::Nominal(id) => {
                if let Some((template, substitution)) = self.source_nominal_instance_entry(id)? {
                    let template = &self.nominal_templates[template];
                    let regions = substitution
                        .region_arguments()
                        .iter()
                        .map(|(_, formal)| RegionPosition { formal: *formal })
                        .collect();
                    let arguments = template
                        .generic_parameters
                        .iter()
                        .filter_map(|parameter| match parameter {
                            super::generics::GenericParameter::Type { declaration, .. } => {
                                substitution.type_argument(*declaration)
                            }
                            _ => None,
                        })
                        .collect();
                    (regions, arguments)
                } else {
                    match self.nominal(id)?.kind {
                        CheckedNominalKind::Box {
                            region: brand,
                            referent,
                            ..
                        } => (brand.map(one_region).unwrap_or_default(), vec![referent]),
                        _ => match self.prelude_type(id) {
                            Some(PreludeType::Option(value)) => (vec![], vec![value]),
                            Some(PreludeType::Result(ok, error)) => (vec![], vec![ok, error]),
                            _ => (vec![], vec![]),
                        },
                    }
                }
            }
            CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => {
                (vec![], vec![self.element_type(element)?])
            }
            CheckedType::Buffer { element } => (vec![], vec![self.element_type(element)?]),
            _ => (vec![], vec![]),
        };
        Ok(axes)
    }

    /// `source` is present for a concrete function instance so written `T`
    /// remains opaque. Constructor templates use symbolic checked types,
    /// where generic leaves already carry that distinction.
    pub(super) fn type_region_shape(
        &self,
        ty: CheckedType,
        source: Option<NodeId>,
    ) -> Result<TypeRegionShape, CheckStop> {
        if let Some(source) = source
            && self.tree.names_nominal(source)?
            && matches!(
                self.use_at(source, LexicalUseRole::Type)?.target(),
                ResolvedTarget::Source {
                    class: DeclarationClass::GenericType,
                    ..
                }
            )
        {
            return Ok(TypeRegionShape {
                regions: vec![],
                arguments: vec![],
            });
        }
        let (regions, types) = self.type_region_axes(ty)?;
        let mut sources = Vec::new();
        if let Some(source) = source {
            sources = self.tree.children_with(source, Production::Type)?;
            if let Some(targs) = self.tree.argument_list(source)? {
                for argument in self.tree.children_with(targs, Production::Targ)? {
                    if let Some(ty) = self.tree.first_child_with(argument, Production::Type)? {
                        sources.extend(self.behavior_type_sources(ty)?);
                    }
                }
            }
            if sources.len() != types.len() {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
        }
        let arguments = types
            .into_iter()
            .enumerate()
            .map(|(index, ty)| self.type_region_shape(ty, sources.get(index).copied()))
            .collect::<Result<_, _>>()?;
        Ok(TypeRegionShape { regions, arguments })
    }
}
