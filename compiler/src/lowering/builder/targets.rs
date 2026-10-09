//! Mutation targets captured before right-hand-side evaluation [SET-1, SET-2].
//!
//! A prepared target carries only evaluated addresses, descriptors and offsets.
//! Its commit never evaluates a source expression again. Plain SSA field paths
//! need no runtime target evaluation; rebuilding one at commit uses the current
//! root so writes performed by the right-hand side to sibling fields survive.

use crate::semantic::CheckedWritablePlace;

use super::*;

pub(super) struct PreparedTarget<'target> {
    ty: IrType,
    kind: TargetStorage<'target>,
    /// [WIN-3] whether the value this commit overwrites is certainly still
    /// live, so the commit owes it its compiler-derived release [STOR-3].
    ///
    /// The checker carries this conclusion on the commit: even a reference
    /// or element target can have its old value read out atomically [OP-12].
    displaces_live_value: bool,
}

enum TargetStorage<'target> {
    Place(&'target CheckedWritablePlace),
    IndexedMark {
        address: IrValueId,
        private: IrValueId,
        constant: IrConstant,
    },
    Address {
        address: IrValueId,
        referent: IrAddressed,
    },
    Slice {
        slice: IrValueId,
        index: IrValueId,
        target_domain: IrTargetDomainObligation,
    },
}

impl IrBuilder<'_> {
    pub(super) fn prepare_target<'target>(
        &mut self,
        target: &'target CheckedSetTarget,
        displaces_live_value: bool,
    ) -> Result<PreparedTarget<'target>, LoweringFailure> {
        let ty = match target {
            // [REF-1, REF-4] a reference variable carries its address or
            // range descriptor as the binding's runtime value. Its written
            // type names the referent/element, so read the already-lowered
            // binding type rather than mistaking that logical type for the
            // representation replaced by this rebinding.
            CheckedSetTarget::Place(place) if place.mode.is_reference() => {
                if place.declares || !place.fields.is_empty() {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                let value = self
                    .bindings
                    .get(&place.binding)
                    .copied()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                self.value_type(value)?
            }
            _ => lower_type(self.erasure, target.ty())?,
        };
        let address_kind = |address, referent| TargetStorage::Address { address, referent };
        let kind = match target {
            CheckedSetTarget::Storage(root) => {
                let address = self.lower_place_address(root)?;
                let IrType::Address(referent) = self.value_type(address)? else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                if let Some((private, constant)) = self.indexed_mark(root) {
                    TargetStorage::IndexedMark {
                        address,
                        private,
                        constant,
                    }
                } else {
                    address_kind(address, referent)
                }
            }
            CheckedSetTarget::Place(place) => {
                if place.declares {
                    if !place.fields.is_empty() {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    TargetStorage::Place(place)
                } else if place.mode.is_reference() {
                    TargetStorage::Place(place)
                } else {
                    let storage = self
                        .bindings
                        .get(&place.binding)
                        .copied()
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                    if let Some(address) = self.addressed_target(target, storage)? {
                        let IrType::Address(referent) = self.value_type(address)? else {
                            return Err(LoweringFailure::InvalidCheckedProgram);
                        };
                        address_kind(address, referent)
                    } else {
                        TargetStorage::Place(place)
                    }
                }
            }
            // [REF-4, SET-1] one element position of the run a range names.
            CheckedSetTarget::RangeIndex(target) => {
                if !target.path.is_empty() {
                    let address = self.lower_range_address(
                        &target.root,
                        &target.offset,
                        &target.path,
                        target.target_domain,
                    )?;
                    let IrType::Address(referent) = self.value_type(address)? else {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    };
                    address_kind(address, referent)
                } else {
                    let slice = self.range_root(&target.root)?;
                    let index = self.expression(&target.offset)?;
                    let target_domain = target.target_domain.into();
                    self.check_target_offset(index, target_domain)?;
                    TargetStorage::Slice {
                        slice,
                        index,
                        target_domain,
                    }
                }
            }
        };
        Ok(PreparedTarget {
            ty,
            kind,
            displaces_live_value,
        })
    }

    /// [WIN-3] "Assigning over a live owned place releases the old value when it
    /// is affine."
    ///
    /// The release is the commit's, so this reads the displaced value after
    /// the right-hand side's effects and before the write, exactly where the
    /// old owner stops being reachable. What it releases is the ordinary
    /// [STOR-3] release of the target's own type: a cell frees its content
    /// and then its heap object, a type with no release action owes nothing,
    /// and a live linear target never reaches lowering because [WIN-3]
    /// makes that overwrite a hard error.
    ///
    /// The checker's post-RHS liveness record applies to every target shape
    /// [DIAG-2]: a revived binding or an atomic read-out displaces nothing.
    /// Structs use the same component records as checked scope cleanup;
    /// their own node is empty, so emitting only that node loses the fields'
    /// releases. Other releasing types keep their ordinary recursive action.
    pub(super) fn displaced_releases(
        &mut self,
        target: &PreparedTarget<'_>,
    ) -> Result<Vec<IrDrop>, LoweringFailure> {
        if !target.displaces_live_value
            || !crate::lowering::type_derives_release(self.nominals, self.elements, target.ty)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?
        {
            return Ok(Vec::new());
        }
        let previous = self.read_target(target)?;
        let mut drops = Vec::new();
        let mut pending = vec![(Vec::new(), target.ty)];
        while let Some((path, ty)) = pending.pop() {
            if !crate::lowering::type_derives_release(self.nominals, self.elements, ty)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?
            {
                continue;
            }
            if let IrType::Nominal(id) = ty
                && let IrNominalKind::Struct { fields } = &self
                    .nominals
                    .get(id.index())
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?
                    .kind
            {
                // The stack is LIFO; release fields in declaration order
                // [PROV-6], including each nested struct's owned components.
                for (index, field) in fields.iter().enumerate().rev() {
                    let mut child = path.clone();
                    child.push(u32::try_from(index).map_err(|_| LoweringFailure::CounterOverflow)?);
                    pending.push((child, field.ty));
                }
            } else {
                // Capture every component from the pre-write snapshot. The
                // commit can overwrite the target before this group runs.
                drops.push(self.lower_drop_subject(previous, &path, ty)?);
            }
        }
        Ok(drops)
    }

    fn check_target_offset(
        &self,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    ) -> Result<(), LoweringFailure> {
        if self.value_type(offset)?
            != (IrType::Integer {
                width: 64,
                signed: false,
            })
            || target_domain != IrTargetDomainObligation::ElementAddress
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(())
    }

    /// The read half of replacement occurs at commit, after the RHS's effects.
    pub(super) fn read_target(
        &mut self,
        target: &PreparedTarget<'_>,
    ) -> Result<IrValueId, LoweringFailure> {
        let value = match &target.kind {
            TargetStorage::Address { address, .. } => self.load_storage_value(*address)?,
            TargetStorage::Place(place) if !place.declares => {
                let root = self.binding_value(place.binding)?;
                if place.fields.is_empty() {
                    root
                } else {
                    self.project_struct_path(root, &place.fields, false)?
                }
            }
            TargetStorage::Slice {
                slice,
                index,
                target_domain,
            } => self.define(
                target.ty,
                IrOperation::SliceIndex {
                    slice: *slice,
                    offset: *index,
                    target_domain: *target_domain,
                },
            )?,
            TargetStorage::Place(_) | TargetStorage::IndexedMark { .. } => {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
        };
        if self.value_type(value)? != target.ty {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(value)
    }

    pub(super) fn write_target(
        &mut self,
        target: &PreparedTarget<'_>,
        value: IrValueId,
    ) -> Result<(), LoweringFailure> {
        if self.value_type(value)? != target.ty {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        match &target.kind {
            TargetStorage::IndexedMark {
                address,
                private,
                constant,
            } => {
                self.current_block_mut()?
                    .instructions
                    .push(IrInstruction::IndexedMark {
                        address: *address,
                        private: *private,
                        constant: *constant,
                        value_type: target.ty,
                    });
                Ok(())
            }
            TargetStorage::Address { address, referent } => {
                self.store_addressed(*address, value, *referent)
            }
            TargetStorage::Slice { slice, index, .. } => {
                self.current_block_mut()?
                    .instructions
                    .push(IrInstruction::StoreSlice {
                        slice: *slice,
                        index: *index,
                        value,
                    });
                Ok(())
            }
            TargetStorage::Place(place) => {
                if place.declares {
                    if self.bindings.insert(place.binding, value).is_some() {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    return self.promote_binding_if_needed(place.binding);
                }
                if place.mode.is_reference() {
                    if !place.fields.is_empty()
                        || self.bindings.insert(place.binding, value).is_none()
                    {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    return Ok(());
                }
                let storage = self
                    .bindings
                    .get(&place.binding)
                    .copied()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                let replacement = if place.fields.is_empty() {
                    value
                } else {
                    let root = self.load_storage_value(storage)?;
                    self.replace_struct_path(root, &place.fields, value)?
                };
                self.commit_root_storage(place.binding, storage, replacement)
            }
        }
    }
}
