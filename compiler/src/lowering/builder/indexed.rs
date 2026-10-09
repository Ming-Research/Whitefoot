//! Borrowed root-shaped storage shared by a leaf's indexed families.

use super::*;
use crate::semantic::{
    CheckedContainerRoot, CheckedPlaceStep, CheckedResolvedPlace, CheckedResolvedStep,
    IndexedReduction,
};

#[derive(Clone, Debug)]
pub(super) struct BlockOwner {
    pub place: CheckedResolvedPlace,
    pub path: Vec<CheckedPlaceStep>,
    pub ty: IrType,
}

impl PartialEq for BlockOwner {
    fn eq(&self, other: &Self) -> bool {
        self.ty == other.ty
            && self.place.path.len() == other.place.path.len()
            && self.place.contains(&other.place)
    }
}

impl Eq for BlockOwner {}

pub(super) struct BlockBinding {
    pub owner: BlockOwner,
    pub blocks: IrValueId,
    pub address: IrValueId,
}

impl IrBuilder<'_> {
    pub(super) fn indexed_place(
        &self,
        root: crate::semantic::CheckedPlaceRoot,
        path: &[CheckedPlaceStep],
    ) -> Option<CheckedResolvedPlace> {
        let steps = path
            .iter()
            .map(CheckedPlaceStep::place_step)
            .collect::<Vec<_>>();
        let places = self.places.as_ref()?.resolve(root, &steps);
        let [place] = places.as_slice() else {
            return None;
        };
        Some(place.clone())
    }

    pub(super) fn same_indexed_root(
        &self,
        left: &CheckedContainerRoot,
        right: &CheckedContainerRoot,
    ) -> bool {
        matches!(
            (self.indexed_place(left.root, &left.path), self.indexed_place(right.root, &right.path)),
            (Some(left), Some(right)) if left.path.len() == right.path.len() && left.contains(&right)
        )
    }

    pub(super) fn indexed_block_contains(
        &self,
        owner: &BlockOwner,
        root: &CheckedContainerRoot,
    ) -> bool {
        self.indexed_place(root.root, &root.path)
            .is_some_and(|place| {
                owner.place.contains(&place)
                    && place.path[owner.place.path.len()..]
                        .iter()
                        .all(|step| matches!(step, CheckedResolvedStep::Field(_)))
            })
    }

    /// Below a root-shaped block, stored families and inline ancestors use
    /// only field steps. Their resolved suffix survives reference aliases
    /// whose written path starts at a different depth.
    pub(super) fn indexed_block_fields(
        &self,
        owner: &BlockOwner,
        place: &CheckedResolvedPlace,
    ) -> Result<Vec<CheckedPlaceStep>, LoweringFailure> {
        if !owner.place.contains(place) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        place.path[owner.place.path.len()..]
            .iter()
            .map(|step| match step {
                CheckedResolvedStep::Field(field) => Ok(CheckedPlaceStep::Field(*field)),
                _ => Err(LoweringFailure::InvalidCheckedProgram),
            })
            .collect()
    }

    /// Inline fields belong to their nearest owning block. Keeping the
    /// enclosing fixed aggregate preserves references to those fields too.
    pub(super) fn indexed_owner(
        &self,
        family: &IndexedReduction,
    ) -> Result<BlockOwner, LoweringFailure> {
        let prefix = family
            .root
            .path
            .iter()
            .rposition(|step| matches!(step, CheckedPlaceStep::BoxReferent(_)))
            .map_or(0, |index| index + 1);
        let binding = family
            .root
            .binding()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let stored = self
            .bindings
            .get(&binding)
            .copied()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let mut ty = match self.value_type(stored)? {
            IrType::Address(referent) => referent.ty(),
            ty => ty,
        };
        for step in &family.root.path[..prefix] {
            ty = self.indexed_step(ty, step)?.1;
        }
        Ok(BlockOwner {
            place: self
                .indexed_place(family.root.root, &family.root.path[..prefix])
                .ok_or(LoweringFailure::InvalidCheckedProgram)?,
            path: family.root.path[..prefix].to_vec(),
            ty,
        })
    }

    pub(super) fn indexed_step(
        &self,
        ty: IrType,
        step: &CheckedPlaceStep,
    ) -> Result<(IrPlaceStep, IrType), LoweringFailure> {
        let IrType::Nominal(nominal) = ty else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        match (step, &self.nominals[nominal.index()].kind) {
            (CheckedPlaceStep::Field(field), IrNominalKind::Struct { fields }) => {
                let ty = fields
                    .get(*field as usize)
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?
                    .ty;
                Ok((
                    IrPlaceStep::Field {
                        nominal,
                        field: *field,
                    },
                    ty,
                ))
            }
            (CheckedPlaceStep::BoxReferent(checked), IrNominalKind::Box { referent, .. })
                if self.erased(*checked)? == nominal =>
            {
                Ok((IrPlaceStep::BoxReferent { nominal }, *referent))
            }
            _ => Err(LoweringFailure::InvalidCheckedProgram),
        }
    }

    pub(super) fn indexed_block_capture(
        &mut self,
        family: &IndexedReduction,
        range_type: IrType,
    ) -> Result<IrValueId, LoweringFailure> {
        let owner = self.indexed_owner(family)?;
        if let Some(binding) = self
            .indexed_blocks
            .iter()
            .rev()
            .find(|binding| binding.owner == owner)
        {
            return Ok(binding.blocks);
        }
        let binding = family
            .root
            .binding()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let mut address = self.bindings[&binding];
        if !matches!(self.value_type(address)?, IrType::Address(_)) {
            let referent = IrAddressed::of(self.value_type(address)?)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            address = self.define(
                IrType::Address(referent),
                IrOperation::AddressOf {
                    value: address,
                    referent,
                },
            )?;
        }
        let address = self.project_address_path(address, &owner.path)?;
        self.define(range_type, IrOperation::IndexedBlocks { address })
    }

    /// The current block takes precedence over the captured shared binding.
    /// This also routes an inner join and every ordinary measure read.
    pub(super) fn indexed_block_address(
        &mut self,
        place: &CheckedContainerRoot,
    ) -> Result<Option<IrValueId>, LoweringFailure> {
        let Some(base) = self.indexed_place(place.root, &[]) else {
            return Ok(None);
        };
        let Some(resolved) = self.indexed_place(place.root, &place.path) else {
            return Ok(None);
        };
        let found = self
            .indexed_blocks
            .iter()
            .rev()
            .find(|binding| binding.owner.place.contains(&resolved))
            .map(|binding| (binding.address, binding.owner.clone()));
        if let Some((address, owner)) = found {
            let path = if owner.place.path.len() <= base.path.len() {
                let mut path = self.indexed_block_fields(&owner, &base)?;
                path.extend_from_slice(&place.path);
                path
            } else {
                place.path[owner.place.path.len() - base.path.len()..].to_vec()
            };
            return self.project_address_path(address, &path).map(Some);
        }
        Ok(None)
    }

    pub(super) fn indexed_call_argument(
        &mut self,
        call: &NodePath,
        argument: usize,
        original: IrValueId,
    ) -> Result<IrValueId, LoweringFailure> {
        let mut roots = Vec::new();
        for family in self.indexed_block_families.clone() {
            for mapping in &family.calls {
                if mapping.call != *call || mapping.argument != argument {
                    continue;
                }
                let Some(binding) = self
                    .indexed_blocks
                    .iter()
                    .rev()
                    .find(|binding| self.indexed_block_contains(&binding.owner, &family.root))
                else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let owner = binding.owner.clone();
                let address = binding.address;
                if owner.place.contains(&mapping.actual) {
                    let path = self.indexed_block_fields(&owner, &mapping.actual)?;
                    return self.project_address_path(address, &path);
                }
                if !mapping.actual.contains(&owner.place) {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                let depth = owner.place.path.len() - mapping.actual.path.len();
                let IrType::Address(referent) = self.value_type(original)? else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let mut ty = referent.ty();
                let mut path = Vec::new();
                for step in &mapping.callee_root.path[..depth] {
                    let (projection, next) = self.indexed_step(ty, step)?;
                    path.push(projection);
                    ty = next;
                }
                if ty != owner.ty {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                let replacement = crate::ir::IrIndexedRootReference {
                    block: address,
                    path,
                };
                if !roots.contains(&replacement) {
                    roots.push(replacement);
                }
            }
        }
        if roots.is_empty() {
            Ok(original)
        } else {
            self.define(
                self.value_type(original)?,
                IrOperation::IndexedReference { original, roots },
            )
        }
    }
}
