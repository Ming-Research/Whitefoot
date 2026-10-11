//! Profitability selection for layout-bounded transfers.
//!
//! A local field producer can benefit from keeping its store boundaries in
//! the emitted copy. Other sources retain one intrinsic before LLVM inlining.
//! This is a function-local MAY property, not reaching-store, alias, lifetime
//! or machine-width evidence. Either transfer form remains overlap-safe.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum TransferSource {
    Value(IrValueId),
    Address(IrValueId),
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum AddressOrigin {
    Local { binding: IrValueId, field: bool },
    Unknown,
}

#[derive(Default)]
pub(super) struct LocalTransferSources {
    values: HashSet<IrValueId>,
    addresses: HashSet<IrValueId>,
}

impl LocalTransferSources {
    pub(super) fn build(function: &IrFunction, incoming: &[Vec<Incoming>]) -> Self {
        // AddressOf owns local backing. Only inline projections preserve that
        // identity: following a Box or collection descriptor reaches storage
        // whose producer is outside this local policy. Unknown is explicit so
        // an address phi cannot silently discard an external incoming source.
        let mut roots: HashMap<IrValueId, BTreeSet<AddressOrigin>> = HashMap::new();
        for &(value, ty) in function.parameters() {
            if matches!(ty, IrType::Address(_)) {
                roots.insert(value, BTreeSet::from([AddressOrigin::Unknown]));
            }
        }
        for block in function.blocks() {
            for instruction in block.instructions() {
                let IrInstruction::Define {
                    result,
                    ty,
                    operation,
                } = instruction
                else {
                    continue;
                };
                if !matches!(ty, IrType::Address(_)) {
                    continue;
                }
                match operation {
                    IrOperation::AddressOf { .. } => {
                        roots.insert(
                            *result,
                            BTreeSet::from([AddressOrigin::Local {
                                binding: *result,
                                field: false,
                            }]),
                        );
                    }
                    IrOperation::ProjectAddress {
                        projection: crate::IrPlaceStep::Field { .. }
                            | crate::IrPlaceStep::EnumVariant { .. },
                        ..
                    } => {}
                    _ => {
                        roots.insert(*result, BTreeSet::from([AddressOrigin::Unknown]));
                    }
                }
            }
        }
        loop {
            let mut changed = false;
            for (block_index, block) in function.blocks().iter().enumerate() {
                for (position, &(value, ty)) in block.parameters().iter().enumerate() {
                    if !matches!(ty, IrType::Address(_)) {
                        continue;
                    }
                    let origins: BTreeSet<_> = incoming[block_index]
                        .iter()
                        .filter_map(|edge| edge.arguments.get(position))
                        .flat_map(|argument| roots.get(argument).into_iter().flatten().copied())
                        .collect();
                    let existing = roots.entry(value).or_default();
                    let before = existing.len();
                    existing.extend(origins);
                    changed |= existing.len() != before;
                }
                for instruction in block.instructions() {
                    let IrInstruction::Define {
                        result,
                        operation: IrOperation::ProjectAddress {
                            address,
                            projection: crate::IrPlaceStep::Field { .. }
                                | crate::IrPlaceStep::EnumVariant { .. },
                        },
                        ..
                    } = instruction
                    else {
                        continue;
                    };
                    let origins: BTreeSet<_> = roots
                        .get(address)
                        .into_iter()
                        .flatten()
                        .map(|origin| match *origin {
                            AddressOrigin::Local { binding, .. } => {
                                AddressOrigin::Local { binding, field: true }
                            }
                            AddressOrigin::Unknown => AddressOrigin::Unknown,
                        })
                        .collect();
                    let existing = roots.entry(*result).or_default();
                    let before = existing.len();
                    existing.extend(origins);
                    changed |= existing.len() != before;
                }
            }
            if !changed {
                break;
            }
        }

        let mut result = Self::default();
        let mut written = HashSet::new();
        loop {
            let mut changed = false;
            for (block_index, block) in function.blocks().iter().enumerate() {
                for (position, &(value, ty)) in block.parameters().iter().enumerate() {
                    if !matches!(ty, IrType::Address(_))
                        && incoming[block_index].iter().any(|edge| {
                            edge.arguments
                                .get(position)
                                .is_some_and(|argument| result.values.contains(argument))
                        })
                    {
                        changed |= result.values.insert(value);
                    }
                }
                for instruction in block.instructions() {
                    match instruction {
                        IrInstruction::Define {
                            result: value,
                            operation,
                            ..
                        } => {
                            let produced = match operation {
                                IrOperation::ConstructStruct { .. }
                                | IrOperation::ConstructEnum { .. }
                                | IrOperation::InsertStruct { .. } => true,
                                IrOperation::ProjectStruct { aggregate, .. }
                                | IrOperation::ProjectVariant { aggregate, .. } => {
                                    result.values.contains(aggregate)
                                }
                                IrOperation::Load { address, .. } => {
                                    locally_written(&roots, &written, *address)
                                }
                                IrOperation::AddressOf { value: initial, .. } => {
                                    if result.values.contains(initial) {
                                        changed |= written.insert(*value);
                                    }
                                    false
                                }
                                _ => false,
                            };
                            if produced {
                                changed |= result.values.insert(*value);
                            }
                        }
                        IrInstruction::Store { address, value, .. } => {
                            for origin in roots.get(address).into_iter().flatten() {
                                if let AddressOrigin::Local { binding, field } = origin
                                    && (*field || result.values.contains(value))
                                {
                                    changed |= written.insert(*binding);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            if !changed {
                break;
            }
        }
        result.addresses.extend(
            roots
                .keys()
                .copied()
                .filter(|address| locally_written(&roots, &written, *address)),
        );
        result
    }
}

fn locally_written(
    roots: &HashMap<IrValueId, BTreeSet<AddressOrigin>>,
    written: &HashSet<IrValueId>,
    address: IrValueId,
) -> bool {
    let Some(origins) = roots.get(&address) else {
        return false;
    };
    !origins.contains(&AddressOrigin::Unknown)
        && origins.iter().any(|origin| match origin {
            AddressOrigin::Local { binding, .. } => written.contains(binding),
            AddressOrigin::Unknown => false,
        })
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn has_local_transfer_source(&self, source: TransferSource) -> bool {
        match source {
            TransferSource::Address(address) => self.transfer_sources.addresses.contains(&address),
            TransferSource::Value(value) => {
                let Some(slot) = self.storage.slot(value) else {
                    return false;
                };
                let slot = self.storage.allocation_root(slot);
                // Follow the storage actually read at this operation. A value
                // marker cannot turn an external read-through or incoming
                // parameter pointer into locally field-written storage.
                if let Some(address) = self.storage.read_through(slot)
                    && !self.snapshot_copies.contains(&slot)
                {
                    return self.transfer_sources.addresses.contains(&address);
                }
                if self.incoming_places.contains_key(&slot)
                    && !self.incoming_backing.contains_key(&slot)
                {
                    return false;
                }
                self.transfer_sources.values.contains(&value)
            }
        }
    }
}
