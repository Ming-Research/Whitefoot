//! Atomic type groups, ordered takes, and release on every leaving edge.
use super::{GiveTarget, IrBuilder, lower_type};
use crate::semantic::{
    BindingId, CheckedArrayRoot, CheckedContainerRoot, CheckedDrop, CheckedEnumType,
    CheckedExpression, CheckedPlaceStep, CheckedRangeElementPlace, CheckedRangeSource,
    CheckedSetTarget, CheckedStatement, CheckedTarget, CheckedTargetKind, CheckedType,
};
use crate::{
    IrAddressed, IrConstant, IrDrop, IrDropSubject, IrMatchTarget, IrNominalId, IrNominalKind,
    IrOperation, IrRecord, IrRecordKind, IrShared, IrTerminator, IrType, IrValueId,
    LoweringFailure,
};
use std::collections::{BTreeSet, HashMap};
const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};
#[derive(Clone)]
pub(super) struct AtomicRegion {
    targets: Vec<TargetRegion>,
    groups: Vec<Group>,
    bindings: HashMap<BindingId, usize>,
    target_bindings: HashMap<BindingId, usize>,
}
#[derive(Clone)]
struct TargetRegion {
    object: IrValueId,
    nominal: IrNominalId,
    owned: bool,
    lock: Lock,
}
#[derive(Clone)]
struct Group {
    targets: Vec<usize>,
    record: Option<IrRecord>,
    flag: Option<IrValueId>,
}
#[derive(Clone)]
enum Lock {
    Object,
    Map {
        nominal: IrNominalId,
        table: IrValueId,
        take: TableTake,
    },
}
#[derive(Clone)]
enum TableTake {
    Entry {
        record: IrRecord,
        key: IrValueId,
        read: bool,
        inserts: bool,
        stable_absence: bool,
    },
    Hold {
        record: IrRecord,
        whole: bool,
        key: Option<IrValueId>,
        set: Option<(IrValueId, IrValueId)>,
        read: bool,
    },
}
#[derive(Clone, Copy)]
struct Root {
    binding: BindingId,
}
impl Root {
    fn whole(binding: BindingId) -> Self {
        Self { binding }
    }
    fn path(binding: BindingId, _steps: &[CheckedPlaceStep]) -> Self {
        Self { binding }
    }
}
impl IrBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_atomic(
        &mut self,
        targets: &[CheckedTarget],
        guard: Option<&CheckedExpression>,
        body: &[CheckedStatement],
        fallthrough_drops: &[CheckedDrop],
        give_target: Option<GiveTarget>,
    ) -> Result<(), LoweringFailure> {
        if !self.atomics.is_empty() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let enclosing = self.bindings.keys().copied().collect::<Vec<_>>();
        let mut grouped = std::collections::BTreeMap::new();
        for (index, target) in targets.iter().enumerate() {
            grouped
                .entry(target.lock_order.clone())
                .or_insert_with(Vec::new)
                .push(index);
        }
        let mut guard_roots = Vec::new();
        if let Some(guard) = guard {
            expression_bindings(guard, &mut guard_roots);
        }
        let eager = grouped
            .values()
            .enumerate()
            .filter(|(_, indexes)| {
                indexes.iter().any(|i| {
                    matches!(
                        targets[*i].kind,
                        CheckedTargetKind::MapEntry(_) | CheckedTargetKind::MapSet(_)
                    ) || guard_roots.iter().any(|r| r.binding == targets[*i].binding)
                })
            })
            .map(|(i, _)| i)
            .max();
        let mut region = AtomicRegion {
            targets: Vec::new(),
            groups: Vec::new(),
            bindings: HashMap::new(),
            target_bindings: HashMap::new(),
        };
        for (group_index, indexes) in grouped.into_values().enumerate() {
            let grouped = indexes.len() > 1;
            let mut members = Vec::new();
            for index in indexes {
                let target = &targets[index];
                let state = lower_type(self.erasure, target.state)?;
                let address = self.expression(&target.handle)?;
                let IrType::Address(IrAddressed::Nominal(nominal)) = self.value_type(address)?
                else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let loaded = self.define(
                    IrType::Nominal(nominal),
                    IrOperation::Load {
                        address,
                        referent: IrAddressed::Nominal(nominal),
                    },
                )?;
                let object = if target.borrowed {
                    loaded
                } else {
                    self.define(
                        IrType::Nominal(nominal),
                        IrOperation::SharedRetain {
                            nominal,
                            object: loaded,
                        },
                    )?
                };
                let referent =
                    IrAddressed::of(state).ok_or(LoweringFailure::InvalidCheckedProgram)?;
                let field = self.define(
                    IrType::Address(referent),
                    IrOperation::SharedState { nominal, object },
                )?;
                if target.reads {
                    self.readonly_atomic_roots.insert(target.binding);
                }
                let read = target.reads && !guard_roots.iter().any(|r| r.binding == target.binding);
                let lock = match &target.kind {
                    CheckedTargetKind::Object => {
                        self.bind_reference(target.binding, field)?;
                        Lock::Object
                    }
                    kind => {
                        let IrType::Nominal(map_nominal) = state else {
                            return Err(LoweringFailure::InvalidCheckedProgram);
                        };
                        let table = self.define(
                            state,
                            IrOperation::Load {
                                address: field,
                                referent,
                            },
                        )?;
                        let take = match kind {
                            CheckedTargetKind::MapEntry(key) if !grouped => {
                                let key = self.expression(key)?;
                                let record = self.record(IrRecordKind::TableEntry)?;
                                TableTake::Entry {
                                    record,
                                    key,
                                    read,
                                    // Guards keep the existing reservation until their watch is registered.
                                    inserts: target.inserts
                                        || guard_roots.iter().any(|r| r.binding == target.binding),
                                    stable_absence: targets.len() > 1,
                                }
                            }
                            _ => {
                                let record = self.record(IrRecordKind::TableHold)?;
                                let (whole, key, set) = match kind {
                                    CheckedTargetKind::MapWhole => {
                                        self.bind_reference(target.binding, field)?;
                                        (true, None, None)
                                    }
                                    CheckedTargetKind::MapEntry(key) => {
                                        (false, Some(self.expression(key)?), None)
                                    }
                                    CheckedTargetKind::MapSet(set) => {
                                        let set = self.expression(set)?;
                                        let entries =
                                            self.entries_record(target.binding, target.referent)?;
                                        (false, None, Some((set, entries)))
                                    }
                                    _ => return Err(LoweringFailure::InvalidCheckedProgram),
                                };
                                TableTake::Hold {
                                    record,
                                    whole,
                                    key,
                                    set,
                                    read,
                                }
                            }
                        };
                        Lock::Map {
                            nominal: map_nominal,
                            table,
                            take,
                        }
                    }
                };
                region.bindings.insert(target.binding, group_index);
                region
                    .target_bindings
                    .insert(target.binding, region.targets.len());
                members.push(region.targets.len());
                region.targets.push(TargetRegion {
                    object,
                    nominal,
                    owned: !target.borrowed,
                    lock,
                });
            }
            // Eager groups need no frame flag. Lazy state uses one naturally aligned word.
            let flag = if eager.is_some_and(|last| group_index <= last) {
                None
            } else {
                let zero = self.u64_constant(0)?;
                Some(self.define(
                    IrType::Address(IrAddressed::Integer {
                        width: 64,
                        signed: false,
                    }),
                    IrOperation::AddressOf {
                        value: zero,
                        referent: IrAddressed::Integer {
                            width: 64,
                            signed: false,
                        },
                    },
                )?)
            };
            let record = if grouped {
                Some(
                    self.record(IrRecordKind::AtomicGroup {
                        count: u32::try_from(members.len())
                            .map_err(|_| LoweringFailure::CounterOverflow)?,
                    })?,
                )
            } else {
                None
            };
            region.groups.push(Group {
                targets: members,
                record,
                flag,
            });
        }
        if let Some(guard) = guard {
            self.lower_guard(&region, guard, eager)?;
        } else if let Some(last) = eager {
            self.take_groups_through(&region, last)?;
        }
        self.atomics.push(region.clone());
        let lowered = self.lower_statements(body, give_target);
        self.atomics.pop();
        lowered?;
        if self.current.is_some() {
            let drops = self.lower_drops(fallthrough_drops)?;
            self.append_drops(drops)?;
            self.leave_atomic(&region)?;
        }
        self.bindings.retain(|b, _| enclosing.contains(b));
        Ok(())
    }
    fn u64_constant(&mut self, bits: u64) -> Result<IrValueId, LoweringFailure> {
        self.define(
            U64,
            IrOperation::Constant(IrConstant::Integer { ty: U64, bits }),
        )
    }
    fn group_flag(
        &mut self,
        flag: IrValueId,
        take: bool,
    ) -> Result<crate::IrBlockId, LoweringFailure> {
        let value = self.define(
            U64,
            IrOperation::Load {
                address: flag,
                referent: IrAddressed::Integer {
                    width: 64,
                    signed: false,
                },
            },
        )?;
        let zero = self.u64_constant(0)?;
        let value = self.define(
            IrType::Bool,
            IrOperation::Integer {
                operation: crate::IrIntegerOperation::Equal,
                operand_type: U64,
                arguments: vec![value, zero],
            },
        )?;
        let work = self.new_block(&[])?.0;
        let done = self.new_block(&[])?.0;
        self.terminate(IrTerminator::Match {
            scrutinee: value,
            enum_type: crate::lowering::lower_enum_type(self.erasure, CheckedEnumType::Bool)?,
            targets: vec![
                IrMatchTarget {
                    tag: 1,
                    block: if take { work } else { done },
                },
                IrMatchTarget {
                    tag: 0,
                    block: if take { done } else { work },
                },
            ],
        })?;
        self.current = Some(work);
        Ok(done)
    }
    fn set_group_flag(&mut self, flag: IrValueId, bits: u64) -> Result<(), LoweringFailure> {
        let v = self.u64_constant(bits)?;
        self.store_addressed(
            flag,
            v,
            IrAddressed::Integer {
                width: 64,
                signed: false,
            },
        )
    }
    fn reached_groups(&self, region: &AtomicRegion, roots: &[Root]) -> BTreeSet<usize> {
        roots
            .iter()
            .filter_map(|r| region.bindings.get(&r.binding).copied())
            .collect()
    }
    pub(super) fn take_units_for(
        &mut self,
        statement: &CheckedStatement,
    ) -> Result<(), LoweringFailure> {
        let Some(region) = self.atomics.last().cloned() else {
            return Ok(());
        };
        let mut roots = Vec::new();
        statement_bindings(statement, &mut roots);
        if let Some(last) = self.reached_groups(&region, &roots).last().copied() {
            self.take_groups_through(&region, last)?;
        }
        self.bind_entries(&region, &roots)
    }
    fn prepare_hold(&mut self, table: IrValueId, take: &TableTake) -> Result<(), LoweringFailure> {
        let TableTake::Hold {
            record,
            whole,
            key,
            set,
            read,
        } = take
        else {
            return Ok(());
        };
        let record = *record;
        self.define(IrType::Unit, IrOperation::TableHoldBegin { record, table })?;
        if let Some(key) = key {
            self.define(U64, IrOperation::TableHoldKey { record, key: *key })?;
        }
        if let Some((set, entries)) = set {
            let position = self.define(U64, IrOperation::TableHoldKeys { record, set: *set })?;
            self.define(
                IrType::Unit,
                IrOperation::EntriesFill {
                    entries: *entries,
                    hold: record,
                    position,
                    set: *set,
                },
            )?;
        }
        if *whole {
            self.define(IrType::Unit, IrOperation::TableHoldWhole { record })?;
        }
        if *read {
            self.define(IrType::Unit, IrOperation::TableHoldRead { record })?;
        }
        Ok(())
    }
    fn take_groups_through(
        &mut self,
        region: &AtomicRegion,
        last: usize,
    ) -> Result<(), LoweringFailure> {
        for (index, group) in region.groups.iter().enumerate().take(last + 1) {
            // An eager group is already held in the body. Guard acquisition runs before the region is pushed.
            if group.flag.is_none() && !self.atomics.is_empty() {
                continue;
            }
            let done = group
                .flag
                .map(|flag| self.group_flag(flag, true))
                .transpose()?;
            if let Some(record) = group.record {
                for (ordinal, member) in group.targets.iter().enumerate() {
                    let target = &region.targets[*member];
                    let hold = match &target.lock {
                        Lock::Object => None,
                        Lock::Map { table, take, .. } => {
                            self.prepare_hold(*table, take)?;
                            match take {
                                TableTake::Hold { record, .. } => Some(*record),
                                _ => return Err(LoweringFailure::InvalidCheckedProgram),
                            }
                        }
                    };
                    self.define(
                        IrType::Unit,
                        IrOperation::AtomicGroupTarget {
                            record,
                            index: u32::try_from(ordinal)
                                .map_err(|_| LoweringFailure::CounterOverflow)?,
                            object: target.object,
                            hold,
                        },
                    )?;
                }
                self.define(IrType::Unit, IrOperation::AtomicGroupTake { record })?;
            } else {
                let target = &region.targets[group.targets[0]];
                match &target.lock {
                    Lock::Object => {
                        self.define(
                            IrType::Unit,
                            if index == 0 {
                                IrOperation::SharedAcquire {
                                    object: target.object,
                                }
                            } else {
                                IrOperation::SharedTake {
                                    object: target.object,
                                }
                            },
                        )?;
                    }
                    Lock::Map { table, take, .. } => match take {
                        TableTake::Entry {
                            record,
                            key,
                            read,
                            inserts,
                            stable_absence,
                        } => {
                            self.define(
                                IrType::Unit,
                                IrOperation::TableLockEntry {
                                    record: *record,
                                    table: *table,
                                    key: *key,
                                    read: *read,
                                    inserts: *inserts,
                                    stable_absence: *stable_absence,
                                },
                            )?;
                        }
                        TableTake::Hold { record, .. } => {
                            self.prepare_hold(*table, take)?;
                            self.define(
                                IrType::Unit,
                                IrOperation::TableHoldTake { record: *record },
                            )?;
                        }
                    },
                }
            }
            if let Some(flag) = group.flag {
                self.set_group_flag(flag, 1)?;
            }
            if let Some(done) = done {
                self.terminate(IrTerminator::Jump {
                    target: done,
                    arguments: Vec::new(),
                    drops: Vec::new(),
                })?;
                self.current = Some(done);
            }
        }
        Ok(())
    }
    fn bind_entries(
        &mut self,
        region: &AtomicRegion,
        roots: &[Root],
    ) -> Result<(), LoweringFailure> {
        for root in roots {
            let Some(group) = region.bindings.get(&root.binding) else {
                continue;
            };
            // Locate this binding's written target in its type group.
            let Some(target_index) = self.atomic_binding_target(region, root.binding) else {
                continue;
            };
            let target = &region.targets[target_index];
            let address = match &target.lock {
                Lock::Map {
                    nominal,
                    take: TableTake::Entry { record, .. },
                    ..
                } => {
                    let referent = self.table_entry(*nominal)?;
                    Some(self.define(
                        IrType::Address(referent),
                        IrOperation::TableEntrySlot {
                            nominal: *nominal,
                            record: *record,
                        },
                    )?)
                }
                Lock::Map {
                    nominal,
                    take:
                        TableTake::Hold {
                            record,
                            key: Some(_),
                            ..
                        },
                    ..
                } => {
                    let referent = self.table_entry(*nominal)?;
                    let position = self.u64_constant(0)?;
                    Some(self.define(
                        IrType::Address(referent),
                        IrOperation::TableHoldSlot {
                            nominal: *nominal,
                            record: *record,
                            position,
                        },
                    )?)
                }
                _ => None,
            };
            let _ = group;
            if let Some(address) = address {
                self.bindings.insert(root.binding, address);
                self.promote_binding_if_needed(root.binding)?;
            }
        }
        Ok(())
    }
    fn atomic_binding_target(&self, region: &AtomicRegion, binding: BindingId) -> Option<usize> {
        region.target_bindings.get(&binding).copied()
    }
    fn release_groups(&mut self, region: &AtomicRegion) -> Result<(), LoweringFailure> {
        for group in region.groups.iter().rev() {
            let done = group
                .flag
                .map(|flag| self.group_flag(flag, false))
                .transpose()?;
            if let Some(record) = group.record {
                let nominal = match &region.targets[group.targets[0]].lock {
                    Lock::Object => None,
                    Lock::Map { nominal, .. } => Some(*nominal),
                };
                self.define(
                    IrType::Unit,
                    IrOperation::AtomicGroupRelease { record, nominal },
                )?;
            } else {
                let target = &region.targets[group.targets[0]];
                match &target.lock {
                    Lock::Object => {
                        self.define(
                            IrType::Unit,
                            IrOperation::SharedUnlock {
                                object: target.object,
                            },
                        )?;
                    }
                    Lock::Map {
                        nominal,
                        take: TableTake::Entry { record, read, .. },
                        ..
                    } => {
                        self.define(
                            IrType::Unit,
                            IrOperation::TableUnlockEntry {
                                nominal: *nominal,
                                record: *record,
                                read: *read,
                            },
                        )?;
                    }
                    Lock::Map {
                        nominal,
                        take: TableTake::Hold { record, .. },
                        ..
                    } => {
                        self.define(
                            IrType::Unit,
                            IrOperation::TableHoldRelease {
                                nominal: *nominal,
                                record: *record,
                            },
                        )?;
                    }
                }
            }
            if let Some(flag) = group.flag {
                self.set_group_flag(flag, 0)?;
            }
            if let Some(done) = done {
                self.terminate(IrTerminator::Jump {
                    target: done,
                    arguments: Vec::new(),
                    drops: Vec::new(),
                })?;
                self.current = Some(done);
            }
        }
        Ok(())
    }
    fn lower_guard(
        &mut self,
        region: &AtomicRegion,
        guard: &CheckedExpression,
        eager: Option<usize>,
    ) -> Result<(), LoweringFailure> {
        let mut roots = Vec::new();
        expression_bindings(guard, &mut roots);
        let mut read = self.reached_groups(region, &roots);
        if read.is_empty() {
            read.extend(0..region.groups.len());
        }
        let acquire = self.new_block(&[])?.0;
        self.terminate(IrTerminator::Jump {
            target: acquire,
            arguments: Vec::new(),
            drops: Vec::new(),
        })?;
        self.current = Some(acquire);
        if let Some(last) = read.last().copied().max(eager) {
            self.take_groups_through(region, last)?;
        }
        self.bind_entries(region, &roots)?;
        let holds = self.expression(guard)?;
        let proceed = self.new_block(&[])?.0;
        let watch = self.new_block(&[])?.0;
        self.terminate(IrTerminator::Match {
            scrutinee: holds,
            enum_type: crate::lowering::lower_enum_type(self.erasure, CheckedEnumType::Bool)?,
            targets: vec![
                IrMatchTarget {
                    tag: 1,
                    block: proceed,
                },
                IrMatchTarget {
                    tag: 0,
                    block: watch,
                },
            ],
        })?;
        self.current = Some(watch);
        if region.targets.len() == 1 && matches!(region.targets[0].lock, Lock::Object) {
            self.define(
                IrType::Unit,
                IrOperation::SharedWatch {
                    object: region.targets[0].object,
                },
            )?;
        } else {
            let record = self.record(IrRecordKind::Watch)?;
            self.define(IrType::Unit, IrOperation::WatchBegin { record })?;
            for index in read {
                for member in &region.groups[index].targets {
                    let target = &region.targets[*member];
                    match &target.lock {
                        Lock::Object => {
                            self.define(
                                IrType::Unit,
                                IrOperation::WatchObject {
                                    record,
                                    object: target.object,
                                },
                            )?;
                        }
                        Lock::Map { table, .. } => {
                            self.define(
                                IrType::Unit,
                                IrOperation::WatchTable {
                                    record,
                                    table: *table,
                                },
                            )?;
                        }
                    }
                }
            }
            self.release_groups(region)?;
            self.define(IrType::Unit, IrOperation::WatchPark { record })?;
        }
        self.terminate(IrTerminator::Jump {
            target: acquire,
            arguments: Vec::new(),
            drops: Vec::new(),
        })?;
        self.current = Some(proceed);
        Ok(())
    }
    fn bind_reference(
        &mut self,
        binding: BindingId,
        address: IrValueId,
    ) -> Result<(), LoweringFailure> {
        if self.bindings.insert(binding, address).is_some() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.promote_binding_if_needed(binding)
    }
    fn entries_record(
        &mut self,
        binding: BindingId,
        referent: CheckedType,
    ) -> Result<IrValueId, LoweringFailure> {
        let IrType::Entries { element } = lower_type(self.erasure, referent)? else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let record = self.record(IrRecordKind::Entries)?;
        let entries = self.define(
            IrType::Address(IrAddressed::Entries { element }),
            IrOperation::EntriesRecord { record, element },
        )?;
        self.bind_reference(binding, entries)?;
        Ok(entries)
    }
    fn table_entry(&self, nominal: IrNominalId) -> Result<IrAddressed, LoweringFailure> {
        match &self.nominals[nominal.index()].kind {
            IrNominalKind::Shared {
                shape: IrShared::Map { entry },
                ..
            } => IrAddressed::of(*entry).ok_or(LoweringFailure::InvalidCheckedProgram),
            _ => Err(LoweringFailure::InvalidCheckedProgram),
        }
    }
    pub(super) fn table_nominal(&self, ty: IrType) -> Option<IrNominalId> {
        let IrType::Nominal(nominal) = ty else {
            return None;
        };
        matches!(
            self.nominals[nominal.index()].kind,
            IrNominalKind::Shared {
                shape: IrShared::Map { .. },
                ..
            }
        )
        .then_some(nominal)
    }
    pub(super) fn record(&mut self, kind: IrRecordKind) -> Result<IrRecord, LoweringFailure> {
        let index = self.records;
        self.records = index
            .checked_add(1)
            .ok_or(LoweringFailure::CounterOverflow)?;
        Ok(IrRecord { index, kind })
    }
    pub(super) fn leave_atomics(&mut self, depth: usize) -> Result<(), LoweringFailure> {
        let regions = self
            .atomics
            .get(depth..)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?
            .to_vec();
        for region in regions.iter().rev() {
            self.leave_atomic(region)?;
        }
        Ok(())
    }
    fn leave_atomic(&mut self, region: &AtomicRegion) -> Result<(), LoweringFailure> {
        self.release_groups(region)?;
        self.append_drops(
            region
                .targets
                .iter()
                .filter(|t| t.owned)
                .map(|t| IrDrop {
                    subject: IrDropSubject::Value(t.object),
                    ty: IrType::Nominal(t.nominal),
                })
                .collect(),
        )
    }
    pub(super) fn atomic_depth(&self) -> usize {
        self.atomics.len()
    }
}
/// The places a statement's own expressions reach, not those of the blocks
/// it holds.
fn statement_bindings(statement: &CheckedStatement, roots: &mut Vec<Root>) {
    match statement {
        CheckedStatement::Let { value, .. }
        | CheckedStatement::DestructuringLet { value, .. }
        | CheckedStatement::Evaluate { value, .. }
        | CheckedStatement::DropExpression { value, .. }
        | CheckedStatement::Return { value, .. }
        | CheckedStatement::Give { value, .. } => expression_bindings(value, roots),
        CheckedStatement::PropagateLet { scrutinee, .. }
        | CheckedStatement::Match { scrutinee, .. }
        | CheckedStatement::ValueMatchLet { scrutinee, .. } => {
            expression_bindings(scrutinee, roots)
        }
        CheckedStatement::Set { target, value, .. } => {
            match target {
                CheckedSetTarget::Place(place) => roots.push(Root {
                    binding: place.binding,
                }),
                CheckedSetTarget::RangeIndex(place) => element_roots(place, roots),
                CheckedSetTarget::Storage(root) => container_roots(root, roots),
            }
            expression_bindings(value, roots);
        }
        CheckedStatement::CountedRange { lower, upper, .. } => {
            expression_bindings(lower, roots);
            expression_bindings(upper, roots);
        }
        CheckedStatement::Loop { .. }
        | CheckedStatement::Proof(_)
        | CheckedStatement::Break { .. }
        | CheckedStatement::Continue { .. }
        | CheckedStatement::Atomic { .. } => {}
    }
}

fn expression_bindings(expression: &CheckedExpression, roots: &mut Vec<Root>) {
    match expression {
        CheckedExpression::Constant(_) | CheckedExpression::NamedConstant { .. } => {}
        CheckedExpression::Binding { binding, .. }
        | CheckedExpression::DerefAddressed { binding, .. } => roots.push(Root::whole(*binding)),
        CheckedExpression::Project { binding, .. } => roots.push(Root { binding: *binding }),
        CheckedExpression::BoxTake { binding, path, .. } => {
            roots.push(Root::path(*binding, path));
            steps_roots(path, roots);
        }
        CheckedExpression::UserCall { arguments, .. }
        | CheckedExpression::IntegerOperation { arguments, .. }
        | CheckedExpression::FloatOperation { arguments, .. }
        | CheckedExpression::BooleanOperation { arguments, .. }
        | CheckedExpression::ValueEquality { arguments, .. }
        | CheckedExpression::ConstructStruct {
            fields: arguments, ..
        }
        | CheckedExpression::ConstructEnum {
            fields: arguments, ..
        } => {
            for argument in arguments {
                expression_bindings(argument, roots);
            }
        }
        // A field read through a reference, `s^.f.g`, reaches the place at
        // that path and not the whole referent.
        CheckedExpression::ProjectValue { value, .. } => match projected_root(expression) {
            Some((binding, _fields)) => roots.push(Root { binding }),
            None => expression_bindings(value, roots),
        },
        CheckedExpression::NumericConversion { value, .. }
        | CheckedExpression::Reinterpret { value, .. }
        | CheckedExpression::BoxDeref { value, .. } => expression_bindings(value, roots),
        CheckedExpression::ArrayMeasure { root, .. } => array_roots(root, roots),
        CheckedExpression::ArrayIndex { root, offset, .. } => {
            array_roots(root, roots);
            expression_bindings(offset, roots);
        }
        CheckedExpression::BufferMeasure { root, .. } => {
            roots.push(Root::path(root.binding, &root.path));
            steps_roots(&root.path, roots);
        }
        CheckedExpression::BufferIndex { root, offset, .. } => {
            roots.push(Root::path(root.binding, &root.path));
            steps_roots(&root.path, roots);
            expression_bindings(offset, roots);
        }
        CheckedExpression::RangeOf {
            source, start, end, ..
        } => {
            match source {
                CheckedRangeSource::Storage(root) => container_roots(root, roots),
                CheckedRangeSource::Range(root) => roots.push(Root::whole(root.binding)),
                CheckedRangeSource::Element(place) => element_roots(place, roots),
            }
            expression_bindings(start, roots);
            expression_bindings(end, roots);
        }
        CheckedExpression::RangeMeasure { root, .. } => {
            if let Some(formation) = root.formation.as_deref() {
                expression_bindings(formation, roots);
            } else {
                roots.push(Root::whole(root.binding));
            }
        }
        CheckedExpression::RangeElementMeasure { place, .. }
        | CheckedExpression::RangeIndex { place, .. }
        | CheckedExpression::BorrowRangeIndex { place, .. } => element_roots(place, roots),
        CheckedExpression::ContainerMeasure { root, .. }
        | CheckedExpression::ReadStorage { root, .. }
        | CheckedExpression::BorrowAddressed { root, .. } => container_roots(root, roots),
        CheckedExpression::BorrowSegment { root, segment, .. } => {
            match root {
                crate::semantic::CheckedSegmentSource::Storage(root) => {
                    container_roots(root, roots)
                }
                crate::semantic::CheckedSegmentSource::Element(place) => {
                    element_roots(place, roots)
                }
            }
            if let Some(offset) = segment.offset() {
                expression_bindings(offset, roots);
            }
        }
    }
}

/// The binding and the path of fields a chain of field reads through a
/// reference's referent selects, `s^.f.g`.
fn projected_root(expression: &CheckedExpression) -> Option<(BindingId, Vec<u32>)> {
    match expression {
        CheckedExpression::DerefAddressed { binding, .. } => Some((*binding, Vec::new())),
        CheckedExpression::ProjectValue { value, field, .. } => {
            let (binding, mut fields) = projected_root(value)?;
            fields.push(*field);
            Some((binding, fields))
        }
        _ => None,
    }
}

fn container_roots(root: &CheckedContainerRoot, roots: &mut Vec<Root>) {
    if let Some(binding) = root.binding() {
        roots.push(Root::path(binding, &root.path));
    }
    steps_roots(&root.path, roots);
}

fn element_roots(place: &CheckedRangeElementPlace, roots: &mut Vec<Root>) {
    if let Some(formation) = place.root.formation.as_deref() {
        expression_bindings(formation, roots);
    } else {
        roots.push(Root::whole(place.root.binding));
    }
    expression_bindings(&place.offset, roots);
    steps_roots(&place.path, roots);
}

fn array_roots(root: &CheckedArrayRoot, roots: &mut Vec<Root>) {
    if let CheckedArrayRoot::Binding { binding, .. } = root {
        roots.push(Root { binding: *binding });
    }
}

fn steps_roots(steps: &[CheckedPlaceStep], roots: &mut Vec<Root>) {
    for step in steps {
        if let CheckedPlaceStep::Subscript(subscript) = step {
            expression_bindings(&subscript.offset, roots);
        }
    }
}
