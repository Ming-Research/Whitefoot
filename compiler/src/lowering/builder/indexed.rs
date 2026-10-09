//! Borrowed root-shaped storage shared by a leaf's indexed families.

use super::*;
use crate::semantic::{CheckedContainerRoot, CheckedPlaceStep, IndexedReduction};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BlockOwner {
    pub root: crate::semantic::CheckedPlaceRoot,
    pub path: Vec<CheckedPlaceStep>,
    pub ty: IrType,
}

pub(super) struct BlockBinding {
    pub owner: BlockOwner,
    pub blocks: IrValueId,
    pub address: IrValueId,
}

impl IrBuilder<'_> {
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
            root: family.root.root,
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
        let found = self
            .indexed_blocks
            .iter()
            .rev()
            .find(|binding| {
                binding.owner.root == place.root && place.path.starts_with(&binding.owner.path)
            })
            .map(|binding| (binding.address, binding.owner.path.len()));
        if let Some((address, prefix)) = found {
            return self
                .project_address_path(address, &place.path[prefix..])
                .map(Some);
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
                let owner = self.indexed_owner(&family)?;
                let Some(binding) = self
                    .indexed_blocks
                    .iter()
                    .rev()
                    .find(|binding| binding.owner == owner)
                else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let suffix = &family.root.path[owner.path.len()..];
                // Substitute the checked callee-to-root path. The caller may
                // spell the actual through a reference alias, so its written
                // path depth is not the actual's resolved path depth.
                let Some(depth) = mapping.callee_root.path.len().checked_sub(suffix.len()) else {
                    let inside = suffix.len() - mapping.callee_root.path.len();
                    return self.project_address_path(binding.address, &suffix[..inside]);
                };
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
                    block: binding.address,
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
