//! [SHARE-2, SHARE-3] an atomic statement, lowered to the takes of its
//! state's lock units, its block, and the release of every unit it took on
//! each edge that leaves the block (compiler/waiting-contexts/state-locks).
//!
//! A state's units are each table the state holds at a path of fields alone,
//! whose entries are locked by key or the table whole, and one unit for the
//! rest of the state, which is the object's own lock; they are ordered by the
//! first field of each in declaration order. A statement takes a unit before
//! its guard or the first statement of its block that reaches it, and with
//! it every earlier unit the statement reaches anywhere, so each wait is for
//! a lock after every lock the waiter holds. A flag in the frame records
//! which units the path took, and every edge leaving the block releases
//! those, so every take precedes every release. A table with header entries
//! is taken before the guard and the block, all its header entries together,
//! since the take reads the keys the statement reads when it begins.
//!
//! A statement reached through a handle takes a handle of its own first, so
//! the object stays live until the statement completes whatever the block
//! does with the target place [SHARE-2]; an edge leaving the block releases
//! that handle after the units, which drops the state when it was the last.
//! A guard that reads false watches the units it read, releases everything,
//! and takes its units again once a statement that writes one has ended.

use std::collections::{BTreeSet, HashMap};

use crate::semantic::{
    BindingId, CheckedArrayRoot, CheckedContainerRoot, CheckedDrop, CheckedEntryBinding,
    CheckedEntryIndex, CheckedEnumType, CheckedExpression, CheckedPlaceStep,
    CheckedRangeElementPlace, CheckedRangeSource, CheckedSetTarget, CheckedStatement, CheckedType,
};
use crate::{
    IrAddressed, IrConstant, IrDrop, IrDropSubject, IrMatchTarget, IrNominalId, IrNominalKind,
    IrOperation, IrRecord, IrRecordKind, IrShared, IrTerminator, IrType, IrValueId,
    LoweringFailure,
};

use super::{GiveTarget, IrBuilder, lower_type};

const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};

/// One atomic statement whose block is being lowered.
#[derive(Clone)]
pub(super) struct AtomicRegion {
    /// The statement's own handle to the object, or the handle a caller
    /// lends it.
    object: IrValueId,
    nominal: IrNominalId,
    /// The state's type and its units in their order.
    state: IrType,
    shapes: Vec<UnitShape>,
    /// What the statement takes of each unit; `None` for a unit it never
    /// reaches.
    units: Vec<Option<Unit>>,
    binding: BindingId,
    /// Each entry binder, with its table's unit and how a use finds its
    /// entry.
    entries: HashMap<BindingId, (usize, EntrySlot)>,
    /// Whether the statement holds a handle of its own, which each edge
    /// leaving the block releases.
    owned: bool,
}

/// One lock unit of a state, as its declaration fixes it.
#[derive(Clone)]
enum UnitShape {
    /// The state outside its tables.
    Object,
    /// The table at the path of fields `fields` below the state.
    Table {
        fields: Vec<u32>,
        nominal: IrNominalId,
    },
}

/// What a statement takes of one unit, with the frame flag recording that
/// the path took it, a `Bool` place.
#[derive(Clone)]
struct Unit {
    lock: Lock,
    flag: IrValueId,
}

#[derive(Clone)]
enum Lock {
    /// The object's own lock, whose take suspends the frame when it must
    /// wait only while the statement holds nothing yet.
    Object { suspends: bool },
    /// One table, at the address `field` of the state's field holding it.
    Table {
        nominal: IrNominalId,
        field: IrValueId,
        take: TableTake,
    },
}

#[derive(Clone)]
enum TableTake {
    /// One key's entry alone, the lock kept in `record`; with `read`, beside
    /// the other statements that only read it.
    Entry {
        record: IrRecord,
        key: IrValueId,
        read: bool,
    },
    /// A hold of the header's entries of the table: each single key, at the
    /// position of its order among them, then each key set, with the address
    /// of the set and of its entries' record; and the table whole when
    /// `whole`.
    Hold {
        record: IrRecord,
        whole: bool,
        keys: Vec<IrValueId>,
        sets: Vec<(IrValueId, IrValueId)>,
    },
}

/// How a use of an entry binder finds its entry.
#[derive(Clone, Copy)]
enum EntrySlot {
    /// The entry the lock in `record` holds.
    Entry {
        nominal: IrNominalId,
        record: IrRecord,
    },
    /// The entry added at `position` to the hold `record`.
    Held {
        nominal: IrNominalId,
        record: IrRecord,
        position: u64,
    },
    /// Entries over a key set, whose binder names their record from the
    /// statement's start.
    Set,
}

/// A place an expression reaches: a binding, and the fields below it that
/// begin the path to the place, or `None` for the binding's whole referent.
struct Root {
    binding: BindingId,
    fields: Option<Vec<u32>>,
}

impl Root {
    fn whole(binding: BindingId) -> Self {
        Self {
            binding,
            fields: None,
        }
    }

    fn path(binding: BindingId, steps: &[CheckedPlaceStep]) -> Self {
        Self {
            binding,
            fields: Some(
                steps
                    .iter()
                    .map_while(|step| match step {
                        CheckedPlaceStep::Field(field) => Some(*field),
                        _ => None,
                    })
                    .collect(),
            ),
        }
    }
}

/// One entry binding of the header, read when the statement begins.
struct Header {
    binding: BindingId,
    unit: usize,
    field: IrValueId,
    index: HeaderIndex,
    referent: CheckedType,
    read: bool,
}

#[derive(Clone, Copy)]
enum HeaderIndex {
    Key(IrValueId),
    Set(IrValueId),
}

impl IrBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_atomic(
        &mut self,
        target: &CheckedExpression,
        borrowed: bool,
        binding: BindingId,
        state: CheckedType,
        entries: &[CheckedEntryBinding],
        guard: Option<&CheckedExpression>,
        body: &[CheckedStatement],
        fallthrough_drops: &[CheckedDrop],
        give_target: Option<GiveTarget>,
    ) -> Result<(), LoweringFailure> {
        // [SHARE-2] no atomic statement lies in another's block.
        if !self.atomics.is_empty() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let state_type = lower_type(self.erasure, state)?;
        let referent = IrAddressed::of(state_type).ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let target_value = self.expression(target)?;
        let IrType::Address(IrAddressed::Nominal(nominal)) = self.value_type(target_value)? else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        match self
            .nominals
            .get(nominal.index())
            .map(|nominal| &nominal.kind)
        {
            Some(IrNominalKind::Shared {
                state,
                shape: IrShared::Object,
            }) if *state == state_type => {}
            _ => return Err(LoweringFailure::InvalidCheckedProgram),
        }
        // The statement's own handle, or the handle a caller lends it.
        let loaded = self.define(
            IrType::Nominal(nominal),
            IrOperation::Load {
                address: target_value,
                referent: IrAddressed::Nominal(nominal),
            },
        )?;
        let object = if borrowed {
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
        let enclosing = self.bindings.keys().copied().collect::<Vec<_>>();
        let state_address = self.define(
            IrType::Address(referent),
            IrOperation::SharedState { nominal, object },
        )?;
        self.bind_reference(binding, state_address)?;

        // Which units the guard and the block reach, and which tables they
        // reach other than through an entry binder: those it takes whole.
        let shapes = self.state_units(state_type)?;
        let mut roots = Vec::new();
        if let Some(guard) = guard {
            expression_roots(guard, &mut roots);
        }
        for statement in body {
            statement_roots_deep(statement, &mut roots);
        }
        let mut reached = BTreeSet::new();
        let mut whole = BTreeSet::new();
        for root in roots.iter().filter(|root| root.binding == binding) {
            for unit in self.units_at(&shapes, state_type, root.fields.as_deref())? {
                if matches!(shapes[unit], UnitShape::Table { .. }) {
                    whole.insert(unit);
                }
                reached.insert(unit);
            }
        }

        // The entry binders the guard reads: each is taken alone and never
        // read beside others, since a shared read of an absent key holds no
        // cell and an insert of it could end unseen by the guard's watch.
        let mut guard_roots = Vec::new();
        if let Some(guard) = guard {
            expression_roots(guard, &mut guard_roots);
        }

        // The header: each entry binding's table field and index, read when
        // the statement begins [SHARE-2].
        let mut headers = Vec::with_capacity(entries.len());
        for entry in entries {
            let fields = table_fields(&entry.table, binding)?;
            let unit = shapes
                .iter()
                .position(|shape| {
                    matches!(shape, UnitShape::Table { fields: table, .. } if *table == fields)
                })
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            let field = self.expression(&entry.table)?;
            let index = match &entry.index {
                CheckedEntryIndex::Key(key) => {
                    let key = self.expression(key)?;
                    if !matches!(self.value_type(key)?, IrType::Range { .. }) {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    HeaderIndex::Key(key)
                }
                CheckedEntryIndex::Set(set) => {
                    let set = self.expression(set)?;
                    if self.value_type(set)? != IrType::Address(IrAddressed::KeySet) {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    HeaderIndex::Set(set)
                }
            };
            reached.insert(unit);
            headers.push(Header {
                binding: entry.binding,
                unit,
                field,
                index,
                referent: entry.referent,
                read: entry.reads && !guard_roots.iter().any(|root| root.binding == entry.binding),
            });
        }

        // What the statement takes of each unit it reaches.
        let first = reached.iter().next().copied();
        let mut units = vec![None; shapes.len()];
        let mut slots = HashMap::new();
        for &index in &reached {
            let lock = match &shapes[index] {
                UnitShape::Object => Lock::Object {
                    suspends: first == Some(index),
                },
                UnitShape::Table {
                    fields,
                    nominal: table,
                } => {
                    let table = *table;
                    let mine = headers
                        .iter()
                        .filter(|header| header.unit == index)
                        .collect::<Vec<_>>();
                    let field = match mine.first() {
                        Some(header) => header.field,
                        None => {
                            let path = fields
                                .iter()
                                .copied()
                                .map(CheckedPlaceStep::Field)
                                .collect::<Vec<_>>();
                            self.project_address_path(state_address, &path)?
                        }
                    };
                    let take =
                        self.table_take(table, index, &mine, whole.contains(&index), &mut slots)?;
                    Lock::Table {
                        nominal: table,
                        field,
                        take,
                    }
                }
            };
            let untaken =
                self.define(IrType::Bool, IrOperation::Constant(IrConstant::Bool(false)))?;
            let flag = self.define(
                IrType::Address(IrAddressed::Bool),
                IrOperation::AddressOf {
                    value: untaken,
                    referent: IrAddressed::Bool,
                },
            )?;
            units[index] = Some(Unit { lock, flag });
        }
        let region = AtomicRegion {
            object,
            nominal,
            state: state_type,
            shapes,
            units,
            binding,
            entries: slots,
            owned: !borrowed,
        };

        // [SHARE-2] the statement reads each index atom when it begins, and
        // a table's take reads its header keys' bytes and key sets: every
        // table with header entries is taken before the guard and the block,
        // with each unit before it the statement reaches, so nothing the
        // block writes or releases can change the keys the take reads.
        let headed = headers.iter().map(|header| header.unit).max();
        if let Some(guard) = guard {
            self.lower_guard(&region, guard, headed)?;
        } else if let Some(last) = headed {
            self.take_units_through(&region, last)?;
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
        self.bindings
            .retain(|binding, _| enclosing.contains(binding));
        Ok(())
    }

    /// How a statement takes the table unit `index`, a `table` nominal, from
    /// the header entries `mine` on it: one key's entry alone when that is
    /// all and the table is not taken whole, and otherwise a hold. Records
    /// how each of those binders finds its entry in `slots`.
    fn table_take(
        &mut self,
        table: IrNominalId,
        index: usize,
        mine: &[&Header],
        whole: bool,
        slots: &mut HashMap<BindingId, (usize, EntrySlot)>,
    ) -> Result<TableTake, LoweringFailure> {
        if let ([header], false) = (mine, whole)
            && let HeaderIndex::Key(key) = header.index
        {
            let record = self.record(IrRecordKind::TableEntry)?;
            slots.insert(
                header.binding,
                (
                    index,
                    EntrySlot::Entry {
                        nominal: table,
                        record,
                    },
                ),
            );
            return Ok(TableTake::Entry {
                record,
                key,
                read: header.read,
            });
        }
        let record = self.record(IrRecordKind::TableHold)?;
        let mut keys = Vec::new();
        let mut sets = Vec::new();
        for header in mine {
            if let HeaderIndex::Key(key) = header.index {
                let position =
                    u64::try_from(keys.len()).map_err(|_| LoweringFailure::CounterOverflow)?;
                slots.insert(
                    header.binding,
                    (
                        index,
                        EntrySlot::Held {
                            nominal: table,
                            record,
                            position,
                        },
                    ),
                );
                keys.push(key);
            }
        }
        for header in mine {
            if let HeaderIndex::Set(set) = header.index {
                let entries = self.keyed_entries_record(header.binding, header.referent)?;
                slots.insert(header.binding, (index, EntrySlot::Set));
                sets.push((set, entries));
            }
        }
        Ok(TableTake::Hold {
            record,
            whole,
            keys,
            sets,
        })
    }

    /// The guard: its units, with every table with header entries through the
    /// unit `headed` (lower_atomic), taken again each time a statement that
    /// wrote one wakes it, and its value, read until it holds [SHARE-3].
    fn lower_guard(
        &mut self,
        region: &AtomicRegion,
        guard: &CheckedExpression,
        headed: Option<usize>,
    ) -> Result<(), LoweringFailure> {
        let mut roots = Vec::new();
        expression_roots(guard, &mut roots);
        let mut read = self.region_units(region, &roots)?;
        // A guard reading nothing of the state no statement can change; it
        // watches every unit the statement reaches.
        if read.is_empty() {
            read = (0..region.units.len())
                .filter(|index| region.units[*index].is_some())
                .collect();
        }
        let acquire = self.new_block(&[])?.0;
        self.terminate(IrTerminator::Jump {
            target: acquire,
            arguments: Vec::new(),
            drops: Vec::new(),
        })?;
        self.current = Some(acquire);
        if let Some(last) = read.iter().next_back().copied().max(headed) {
            self.take_units_through(region, last)?;
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
        let tables = read.iter().any(|index| {
            matches!(
                region.units[*index].as_ref().map(|unit| &unit.lock),
                Some(Lock::Table { .. })
            )
        });
        if tables {
            // Each unit the guard read is watched while still held, so a
            // write after the release is never missed.
            let record = self.record(IrRecordKind::Watch)?;
            self.define(IrType::Unit, IrOperation::WatchBegin { record })?;
            for &index in &read {
                match region.units[index].as_ref().map(|unit| &unit.lock) {
                    Some(Lock::Object { .. }) => {
                        self.define(
                            IrType::Unit,
                            IrOperation::WatchObject {
                                record,
                                object: region.object,
                            },
                        )?;
                    }
                    Some(Lock::Table { nominal, field, .. }) => {
                        let table = self.define(
                            IrType::Nominal(*nominal),
                            IrOperation::Load {
                                address: *field,
                                referent: IrAddressed::Nominal(*nominal),
                            },
                        )?;
                        self.define(IrType::Unit, IrOperation::WatchTable { record, table })?;
                    }
                    None => return Err(LoweringFailure::InvalidCheckedProgram),
                }
            }
            self.release_units(region, false)?;
            self.define(IrType::Unit, IrOperation::WatchPark { record })?;
        } else {
            // The guard read the object alone: the object's own watch ends
            // its hold once everything else is given up.
            self.release_units(region, true)?;
            let object = region
                .units
                .iter()
                .flatten()
                .find(|unit| matches!(unit.lock, Lock::Object { .. }))
                .ok_or(LoweringFailure::InvalidCheckedProgram)?
                .flag;
            self.define(
                IrType::Unit,
                IrOperation::SharedWatch {
                    object: region.object,
                },
            )?;
            let untaken =
                self.define(IrType::Bool, IrOperation::Constant(IrConstant::Bool(false)))?;
            self.store_addressed(object, untaken, IrAddressed::Bool)?;
        }
        self.terminate(IrTerminator::Jump {
            target: acquire,
            arguments: Vec::new(),
            drops: Vec::new(),
        })?;
        self.current = Some(proceed);
        Ok(())
    }

    /// Before a statement of an atomic block: takes every unit it reaches,
    /// with the earlier units the statement reaches anywhere, and names each
    /// entry it uses.
    pub(super) fn take_units_for(
        &mut self,
        statement: &CheckedStatement,
    ) -> Result<(), LoweringFailure> {
        let Some(region) = self.atomics.last().cloned() else {
            return Ok(());
        };
        let mut roots = Vec::new();
        statement_roots(statement, &mut roots);
        let reached = self.region_units(&region, &roots)?;
        if let Some(&last) = reached.iter().next_back() {
            self.take_units_through(&region, last)?;
        }
        self.bind_entries(&region, &roots)
    }

    /// The units of `region` the places `roots` reach.
    fn region_units(
        &self,
        region: &AtomicRegion,
        roots: &[Root],
    ) -> Result<BTreeSet<usize>, LoweringFailure> {
        let mut reached = BTreeSet::new();
        for root in roots {
            if root.binding == region.binding {
                reached.extend(self.units_at(
                    &region.shapes,
                    region.state,
                    root.fields.as_deref(),
                )?);
            } else if let Some((unit, _)) = region.entries.get(&root.binding) {
                reached.insert(*unit);
            }
        }
        Ok(reached)
    }

    /// Takes, in their order, every unit up to `last` that the statement
    /// reaches and the path has not taken.
    fn take_units_through(
        &mut self,
        region: &AtomicRegion,
        last: usize,
    ) -> Result<(), LoweringFailure> {
        for index in 0..=last {
            let Some(unit) = region.units.get(index).cloned().flatten() else {
                continue;
            };
            let done = self.branch_on_flag(unit.flag, false)?;
            match &unit.lock {
                Lock::Object { suspends: true } => {
                    self.define(
                        IrType::Unit,
                        IrOperation::SharedAcquire {
                            object: region.object,
                        },
                    )?;
                }
                Lock::Object { suspends: false } => {
                    self.define(
                        IrType::Unit,
                        IrOperation::SharedTake {
                            object: region.object,
                        },
                    )?;
                }
                Lock::Table {
                    nominal,
                    field,
                    take,
                } => {
                    let table = self.define(
                        IrType::Nominal(*nominal),
                        IrOperation::Load {
                            address: *field,
                            referent: IrAddressed::Nominal(*nominal),
                        },
                    )?;
                    self.take_table(table, take)?;
                }
            }
            self.set_flag(unit.flag, true)?;
            self.terminate(IrTerminator::Jump {
                target: done,
                arguments: Vec::new(),
                drops: Vec::new(),
            })?;
            self.current = Some(done);
        }
        Ok(())
    }

    fn take_table(&mut self, table: IrValueId, take: &TableTake) -> Result<(), LoweringFailure> {
        match take {
            TableTake::Entry { record, key, read } => {
                self.define(
                    IrType::Unit,
                    IrOperation::TableLockEntry {
                        record: *record,
                        table,
                        key: *key,
                        read: *read,
                    },
                )?;
            }
            TableTake::Hold {
                record,
                whole,
                keys,
                sets,
            } => {
                let record = *record;
                self.define(IrType::Unit, IrOperation::TableHoldBegin { record, table })?;
                for key in keys {
                    self.define(U64, IrOperation::TableHoldKey { record, key: *key })?;
                }
                for (set, entries) in sets {
                    let position =
                        self.define(U64, IrOperation::TableHoldKeys { record, set: *set })?;
                    self.define(
                        IrType::Unit,
                        IrOperation::KeyedEntriesFill {
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
                self.define(IrType::Unit, IrOperation::TableHoldTake { record })?;
            }
        }
        Ok(())
    }

    /// Branches on the flag at `flag`: the current block becomes the one
    /// reached when it holds `when`, and the returned block is where both
    /// paths meet, which the caller jumps to.
    fn branch_on_flag(
        &mut self,
        flag: IrValueId,
        when: bool,
    ) -> Result<crate::IrBlockId, LoweringFailure> {
        let taken = self.define(
            IrType::Bool,
            IrOperation::Load {
                address: flag,
                referent: IrAddressed::Bool,
            },
        )?;
        let work = self.new_block(&[])?.0;
        let done = self.new_block(&[])?.0;
        let (on_true, on_false) = if when { (work, done) } else { (done, work) };
        self.terminate(IrTerminator::Match {
            scrutinee: taken,
            enum_type: crate::lowering::lower_enum_type(self.erasure, CheckedEnumType::Bool)?,
            targets: vec![
                IrMatchTarget {
                    tag: 1,
                    block: on_true,
                },
                IrMatchTarget {
                    tag: 0,
                    block: on_false,
                },
            ],
        })?;
        self.current = Some(work);
        Ok(done)
    }

    fn set_flag(&mut self, flag: IrValueId, value: bool) -> Result<(), LoweringFailure> {
        let value = self.define(IrType::Bool, IrOperation::Constant(IrConstant::Bool(value)))?;
        self.store_addressed(flag, value, IrAddressed::Bool)
    }

    /// Names, for the code that follows, the entry of each entry binder the
    /// places `roots` use, whose unit is taken.
    fn bind_entries(
        &mut self,
        region: &AtomicRegion,
        roots: &[Root],
    ) -> Result<(), LoweringFailure> {
        for root in roots {
            let Some((_, slot)) = region.entries.get(&root.binding).copied() else {
                continue;
            };
            let address = match slot {
                EntrySlot::Set => continue,
                EntrySlot::Entry { nominal, record } => {
                    let referent = self.table_entry(nominal)?;
                    self.define(
                        IrType::Address(referent),
                        IrOperation::TableEntrySlot { nominal, record },
                    )?
                }
                EntrySlot::Held {
                    nominal,
                    record,
                    position,
                } => {
                    let referent = self.table_entry(nominal)?;
                    let position = self.define(
                        U64,
                        IrOperation::Constant(IrConstant::Integer {
                            ty: U64,
                            bits: position,
                        }),
                    )?;
                    self.define(
                        IrType::Address(referent),
                        IrOperation::TableHoldSlot {
                            nominal,
                            record,
                            position,
                        },
                    )?
                }
            };
            self.bindings.insert(root.binding, address);
            self.promote_binding_if_needed(root.binding)?;
        }
        Ok(())
    }

    /// Releases every unit the path took, the object's aside when
    /// `keep_object`, clearing each one's flag.
    fn release_units(
        &mut self,
        region: &AtomicRegion,
        keep_object: bool,
    ) -> Result<(), LoweringFailure> {
        for unit in region.units.iter().rev().flatten() {
            if keep_object && matches!(unit.lock, Lock::Object { .. }) {
                continue;
            }
            let done = self.branch_on_flag(unit.flag, true)?;
            match &unit.lock {
                Lock::Object { .. } => {
                    self.define(
                        IrType::Unit,
                        IrOperation::SharedUnlock {
                            object: region.object,
                        },
                    )?;
                }
                Lock::Table {
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
                Lock::Table {
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
            self.set_flag(unit.flag, false)?;
            self.terminate(IrTerminator::Jump {
                target: done,
                arguments: Vec::new(),
                drops: Vec::new(),
            })?;
            self.current = Some(done);
        }
        Ok(())
    }

    /// Binds a statement's binder to the address of what it names. The
    /// binder carries the address itself, as a borrow parameter does, so a
    /// use of it as a value is that address, not a load through it.
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

    /// The record of the entries an entry binder over a key set names, whose
    /// referent is `referent`, bound to its binder from the statement's
    /// start.
    fn keyed_entries_record(
        &mut self,
        binding: BindingId,
        referent: CheckedType,
    ) -> Result<IrValueId, LoweringFailure> {
        let IrType::KeyedEntries { element } = lower_type(self.erasure, referent)? else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let record = self.record(IrRecordKind::KeyedEntries)?;
        let entries = self.define(
            IrType::Address(IrAddressed::KeyedEntries { element }),
            IrOperation::KeyedEntriesRecord { record, element },
        )?;
        self.bind_reference(binding, entries)?;
        Ok(entries)
    }

    /// The entry type, `Option<V>`, of the table nominal `nominal`.
    fn table_entry(&self, nominal: IrNominalId) -> Result<IrAddressed, LoweringFailure> {
        match self
            .nominals
            .get(nominal.index())
            .map(|nominal| &nominal.kind)
        {
            Some(IrNominalKind::Shared {
                shape: IrShared::Table { entry },
                ..
            }) => IrAddressed::of(*entry).ok_or(LoweringFailure::InvalidCheckedProgram),
            _ => Err(LoweringFailure::InvalidCheckedProgram),
        }
    }

    /// The table nominal `ty` is, if it is one.
    fn table_nominal(&self, ty: IrType) -> Option<IrNominalId> {
        let IrType::Nominal(nominal) = ty else {
            return None;
        };
        matches!(
            self.nominals
                .get(nominal.index())
                .map(|nominal| &nominal.kind),
            Some(IrNominalKind::Shared {
                shape: IrShared::Table { .. },
                ..
            })
        )
        .then_some(nominal)
    }

    /// The field types of `ty` when it is a struct holding a table at a path
    /// of fields.
    fn fields_with_tables(&self, ty: IrType) -> Option<Vec<IrType>> {
        let IrType::Nominal(nominal) = ty else {
            return None;
        };
        let Some(IrNominalKind::Struct { fields }) = self
            .nominals
            .get(nominal.index())
            .map(|nominal| &nominal.kind)
        else {
            return None;
        };
        let fields = fields.iter().map(|field| field.ty).collect::<Vec<_>>();
        fields
            .iter()
            .any(|field| {
                self.table_nominal(*field).is_some() || self.fields_with_tables(*field).is_some()
            })
            .then_some(fields)
    }

    /// The paths of fields at which `ty` holds a table, the empty path when
    /// `ty` is one, in declaration order.
    pub(super) fn table_paths(&self, ty: IrType) -> Result<Vec<Vec<u32>>, LoweringFailure> {
        fn collect(
            builder: &IrBuilder<'_>,
            ty: IrType,
            path: &mut Vec<u32>,
            paths: &mut Vec<Vec<u32>>,
        ) -> Result<(), LoweringFailure> {
            if builder.table_nominal(ty).is_some() {
                paths.push(path.clone());
                return Ok(());
            }
            if let Some(fields) = builder.fields_with_tables(ty) {
                for (index, field) in fields.into_iter().enumerate() {
                    path.push(u32::try_from(index).map_err(|_| LoweringFailure::CounterOverflow)?);
                    collect(builder, field, path, paths)?;
                    path.pop();
                }
            }
            Ok(())
        }
        let mut paths = Vec::new();
        collect(self, ty, &mut Vec::new(), &mut paths)?;
        Ok(paths)
    }

    /// The table at `path` in the value `value`, or `value` itself for the
    /// empty path.
    pub(super) fn table_at(
        &mut self,
        value: IrValueId,
        path: &[u32],
    ) -> Result<IrValueId, LoweringFailure> {
        if path.is_empty() {
            Ok(value)
        } else {
            self.project_struct_path(value, path, false)
        }
    }

    /// The value `value` with `table` at `path`, or `table` itself for the
    /// empty path.
    pub(super) fn with_table_at(
        &mut self,
        value: IrValueId,
        path: &[u32],
        table: IrValueId,
    ) -> Result<IrValueId, LoweringFailure> {
        if path.is_empty() {
            Ok(table)
        } else {
            self.replace_struct_path(value, path, table)
        }
    }

    /// Whether `ty` holds a part outside every table it holds at a path of
    /// fields.
    fn holds_plain_part(&self, ty: IrType) -> bool {
        if self.table_nominal(ty).is_some() {
            return false;
        }
        match self.fields_with_tables(ty) {
            Some(fields) => fields.iter().any(|field| self.holds_plain_part(*field)),
            None => true,
        }
    }

    /// The units of a state of type `state` in their order: each table it
    /// holds at a path of fields, and the rest of it, each placed at its
    /// first field in declaration order (compiler/waiting-contexts/state-locks).
    fn state_units(&self, state: IrType) -> Result<Vec<UnitShape>, LoweringFailure> {
        fn collect(
            builder: &IrBuilder<'_>,
            ty: IrType,
            path: &mut Vec<u32>,
            units: &mut Vec<UnitShape>,
        ) -> Result<(), LoweringFailure> {
            if let Some(nominal) = builder.table_nominal(ty) {
                units.push(UnitShape::Table {
                    fields: path.clone(),
                    nominal,
                });
                return Ok(());
            }
            if let Some(fields) = builder.fields_with_tables(ty) {
                for (index, field) in fields.into_iter().enumerate() {
                    path.push(u32::try_from(index).map_err(|_| LoweringFailure::CounterOverflow)?);
                    collect(builder, field, path, units)?;
                    path.pop();
                }
                return Ok(());
            }
            if !units.iter().any(|unit| matches!(unit, UnitShape::Object)) {
                units.push(UnitShape::Object);
            }
            Ok(())
        }
        let mut units = Vec::new();
        collect(self, state, &mut Vec::new(), &mut units)?;
        if units.is_empty() {
            units.push(UnitShape::Object);
        }
        Ok(units)
    }

    /// The units a use of the state at the path of fields `fields` below it
    /// reaches: each table at or below the path, and the object's unit when
    /// the path reaches a part outside every table. `None` is the whole
    /// state.
    fn units_at(
        &self,
        shapes: &[UnitShape],
        state: IrType,
        fields: Option<&[u32]>,
    ) -> Result<Vec<usize>, LoweringFailure> {
        let object = shapes
            .iter()
            .position(|shape| matches!(shape, UnitShape::Object));
        let fields = fields.unwrap_or(&[]);
        let mut ty = state;
        let mut depth = 0;
        while depth < fields.len() {
            if self.table_nominal(ty).is_some() {
                break;
            }
            let Some(children) = self.fields_with_tables(ty) else {
                // A path into a part that holds no table.
                return Ok(object.into_iter().collect());
            };
            ty = *children
                .get(fields[depth] as usize)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            depth += 1;
        }
        let prefix = &fields[..depth];
        let mut units = shapes
            .iter()
            .enumerate()
            .filter(|(_, shape)| {
                matches!(shape, UnitShape::Table { fields: table, .. } if table.starts_with(prefix))
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if self.holds_plain_part(ty) {
            units.extend(object);
        }
        Ok(units)
    }

    /// A new record of `kind` in this function's frame.
    fn record(&mut self, kind: IrRecordKind) -> Result<IrRecord, LoweringFailure> {
        let index = self.records;
        self.records = index
            .checked_add(1)
            .ok_or(LoweringFailure::CounterOverflow)?;
        Ok(IrRecord { index, kind })
    }

    /// Before an edge that leaves the blocks of the atomic statements from
    /// `depth` inward: each releases its units and its own handle [SHARE-2].
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
        self.release_units(region, false)?;
        if !region.owned {
            return Ok(());
        }
        self.append_drops(vec![IrDrop {
            subject: IrDropSubject::Value(region.object),
            ty: IrType::Nominal(region.nominal),
        }])
    }

    /// How many atomic statements enclose the statement being lowered.
    pub(super) fn atomic_depth(&self) -> usize {
        self.atomics.len()
    }
}

/// The path of fields below the state binder `binding` that an entry
/// binding's table reference `&s^.t` names [SHARE-2].
fn table_fields(
    table: &CheckedExpression,
    binding: BindingId,
) -> Result<Vec<u32>, LoweringFailure> {
    let CheckedExpression::BorrowAddressed { root, .. } = table else {
        return Err(LoweringFailure::InvalidCheckedProgram);
    };
    if root.binding() != Some(binding) {
        return Err(LoweringFailure::InvalidCheckedProgram);
    }
    root.path
        .iter()
        .map(|step| match step {
            CheckedPlaceStep::Field(field) => Ok(*field),
            _ => Err(LoweringFailure::InvalidCheckedProgram),
        })
        .collect()
}

/// The places a statement's own expressions reach, not those of the blocks
/// it holds.
fn statement_roots(statement: &CheckedStatement, roots: &mut Vec<Root>) {
    match statement {
        CheckedStatement::Let { value, .. }
        | CheckedStatement::DestructuringLet { value, .. }
        | CheckedStatement::Evaluate { value, .. }
        | CheckedStatement::DropExpression { value, .. }
        | CheckedStatement::Return { value, .. }
        | CheckedStatement::Give { value, .. } => expression_roots(value, roots),
        CheckedStatement::PropagateLet { scrutinee, .. }
        | CheckedStatement::Match { scrutinee, .. }
        | CheckedStatement::ValueMatchLet { scrutinee, .. } => expression_roots(scrutinee, roots),
        CheckedStatement::Set { target, value, .. } => {
            match target {
                CheckedSetTarget::Place(place) => roots.push(Root {
                    binding: place.binding,
                    fields: Some(place.fields.clone()),
                }),
                CheckedSetTarget::RangeIndex(place) => element_roots(place, roots),
                CheckedSetTarget::Storage(root) => container_roots(root, roots),
            }
            expression_roots(value, roots);
        }
        CheckedStatement::CountedRange { lower, upper, .. } => {
            expression_roots(lower, roots);
            expression_roots(upper, roots);
        }
        CheckedStatement::Loop { .. }
        | CheckedStatement::Proof(_)
        | CheckedStatement::Break { .. }
        | CheckedStatement::Atomic { .. } => {}
    }
}

/// The places a statement and every block it holds reach.
fn statement_roots_deep(statement: &CheckedStatement, roots: &mut Vec<Root>) {
    statement_roots(statement, roots);
    match statement {
        CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
            for statement in arms.iter().flat_map(|arm| &arm.body) {
                statement_roots_deep(statement, roots);
            }
        }
        CheckedStatement::Loop { body, .. } | CheckedStatement::CountedRange { body, .. } => {
            for statement in body {
                statement_roots_deep(statement, roots);
            }
        }
        _ => {}
    }
}

fn expression_roots(expression: &CheckedExpression, roots: &mut Vec<Root>) {
    match expression {
        CheckedExpression::Constant(_) | CheckedExpression::NamedConstant { .. } => {}
        CheckedExpression::Binding { binding, .. }
        | CheckedExpression::DerefAddressed { binding, .. } => roots.push(Root::whole(*binding)),
        CheckedExpression::Project {
            binding, fields, ..
        } => roots.push(Root {
            binding: *binding,
            fields: Some(fields.clone()),
        }),
        CheckedExpression::BoxTake { binding, path, .. } => {
            roots.push(Root::path(*binding, path));
            steps_roots(path, roots);
        }
        CheckedExpression::UserCall { arguments, .. }
        | CheckedExpression::IntegerOperation { arguments, .. }
        | CheckedExpression::FloatOperation { arguments, .. }
        | CheckedExpression::BooleanOperation { arguments, .. }
        | CheckedExpression::EnumEquality { arguments, .. }
        | CheckedExpression::ConstructStruct {
            fields: arguments, ..
        }
        | CheckedExpression::ConstructEnum {
            fields: arguments, ..
        } => {
            for argument in arguments {
                expression_roots(argument, roots);
            }
        }
        // A field read through a reference, `s^.f.g`, reaches the place at
        // that path and not the whole referent.
        CheckedExpression::ProjectValue { value, .. } => match projected_root(expression) {
            Some((binding, fields)) => roots.push(Root {
                binding,
                fields: Some(fields),
            }),
            None => expression_roots(value, roots),
        },
        CheckedExpression::NumericConversion { value, .. }
        | CheckedExpression::Reinterpret { value, .. }
        | CheckedExpression::BoxDeref { value, .. } => expression_roots(value, roots),
        CheckedExpression::ArrayMeasure { root, .. } => array_roots(root, roots),
        CheckedExpression::ArrayIndex { root, offset, .. } => {
            array_roots(root, roots);
            expression_roots(offset, roots);
        }
        CheckedExpression::BufferMeasure { root, .. } => {
            roots.push(Root::path(root.binding, &root.path));
            steps_roots(&root.path, roots);
        }
        CheckedExpression::BufferIndex { root, offset, .. } => {
            roots.push(Root::path(root.binding, &root.path));
            steps_roots(&root.path, roots);
            expression_roots(offset, roots);
        }
        CheckedExpression::RangeOf {
            source, start, end, ..
        } => {
            match source {
                CheckedRangeSource::Storage(root) => container_roots(root, roots),
                CheckedRangeSource::Range(root) => roots.push(Root::whole(root.binding)),
                CheckedRangeSource::Element(place) => element_roots(place, roots),
            }
            expression_roots(start, roots);
            expression_roots(end, roots);
        }
        CheckedExpression::RangeMeasure { root, .. } => roots.push(Root::whole(root.binding)),
        CheckedExpression::RangeElementMeasure { place, .. }
        | CheckedExpression::RangeIndex { place, .. }
        | CheckedExpression::BorrowRangeIndex { place, .. } => element_roots(place, roots),
        CheckedExpression::ContainerMeasure { root, .. }
        | CheckedExpression::ReadStorage { root, .. }
        | CheckedExpression::BorrowAddressed { root, .. } => container_roots(root, roots),
        CheckedExpression::BorrowSegment { root, segment, .. } => {
            container_roots(root, roots);
            if let Some(offset) = segment.offset() {
                expression_roots(offset, roots);
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
    roots.push(Root::whole(place.root.binding));
    expression_roots(&place.offset, roots);
    steps_roots(&place.path, roots);
}

fn array_roots(root: &CheckedArrayRoot, roots: &mut Vec<Root>) {
    if let CheckedArrayRoot::Binding { binding, fields } = root {
        roots.push(Root {
            binding: *binding,
            fields: Some(fields.clone()),
        });
    }
}

fn steps_roots(steps: &[CheckedPlaceStep], roots: &mut Vec<Root>) {
    for step in steps {
        if let CheckedPlaceStep::Subscript(subscript) = step {
            expression_roots(&subscript.offset, roots);
        }
    }
}
