//! [SHARE-2, SHARE-3] The one object an atomic statement holds, taken in
//! lock units: one per map the state reaches through struct fields alone,
//! and the object's own lock for every other part. A statement takes the
//! units its guard and block reach in field order, each at its first use
//! together with every earlier unit it reaches, and releases them on every
//! leaving edge (compiler/waiting-contexts/state-locks).
use super::lock_ledger::AtomicLedger;
use super::{GiveTarget, IrBuilder, lower_type};
use crate::semantic::{
    BindingId, CheckedArrayRoot, CheckedContainerRoot, CheckedDrop, CheckedEffectStep,
    CheckedEnumType, CheckedExpression, CheckedPlaceStep, CheckedRangeElementPlace,
    CheckedRangeSource, CheckedSetTarget, CheckedStatement, CheckedTarget,
};
use crate::{
    IrAddressed, IrConstant, IrDrop, IrDropSubject, IrMatchTarget, IrNominalId, IrNominalKind,
    IrOperation, IrRecord, IrRecordKind, IrShared, IrTerminator, IrType, IrValueId,
    LoweringFailure, NodePath,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::rc::Rc;
const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};
#[derive(Clone)]
pub(super) struct AtomicRegion {
    binding: BindingId,
    object: IrValueId,
    nominal: IrNominalId,
    /// The address of the object's state, the root of every map unit's path.
    state: IrValueId,
    owned: bool,
    /// The units the guard and block reach, in take order.
    units: Vec<Unit>,
    /// Every map's field path below the state, whether the guard and block
    /// reach it or not, and whether the state has a part no map covers.
    maps: Vec<Vec<u32>>,
    has_object: bool,
    /// Each unit's position in `units`, maps first in `maps` order and the
    /// object's own lock last.
    positions: Vec<Option<usize>>,
    ledger: Rc<RefCell<AtomicLedger>>,
}
#[derive(Clone)]
struct Unit {
    /// The map's field path below the state; `None` for the object's own
    /// lock.
    path: Option<Vec<u32>>,
    /// Whether the guard or block may write the unit, so that leaving the
    /// block releases it as written [SHARE-3].
    wrote: bool,
    flag: Option<IrValueId>,
    lock: Lock,
}
#[derive(Clone)]
enum Lock {
    Object,
    /// The entry under a key the guard and block do not write.
    Entry {
        nominal: IrNominalId,
        table: IrValueId,
        record: IrRecord,
        key: IrValueId,
        key_binding: BindingId,
        read: bool,
        inserts: bool,
        stable_absence: bool,
    },
    /// The whole map.
    Whole {
        nominal: IrNominalId,
        table: IrValueId,
        record: IrRecord,
        read: bool,
    },
}
/// One place a guard or block reaches through a binding: the struct fields
/// below the binding's referent, then what follows them.
#[derive(Clone, Copy)]
struct Reach<'a> {
    binding: BindingId,
    fields: &'a [CheckedPlaceStep],
    tail: Tail<'a>,
}
#[derive(Clone, Copy)]
enum Tail<'a> {
    /// The whole value the fields select.
    Whole,
    /// One subscript of the value the fields select, by this offset.
    Key(&'a CheckedExpression),
    /// Another selection below the fields.
    Below,
}
impl Reach<'_> {
    fn path(&self) -> Vec<u32> {
        self.fields
            .iter()
            .filter_map(|step| match step {
                CheckedPlaceStep::Field(field) => Some(*field),
                _ => None,
            })
            .collect()
    }
}
/// What the guard and block do with one unit.
#[derive(Clone, Default)]
struct UnitUse {
    reached: bool,
    guard: bool,
    body: bool,
    written: bool,
    whole: bool,
    keys: Vec<(BindingId, CheckedExpression)>,
}
impl IrBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_atomic(
        &mut self,
        site: &NodePath,
        target: &CheckedTarget,
        guard: Option<&CheckedExpression>,
        body: &[CheckedStatement],
        fallthrough_drops: &[CheckedDrop],
        give_target: Option<GiveTarget>,
    ) -> Result<(), LoweringFailure> {
        if !self.atomics.is_empty() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let enclosing = self.bindings.keys().copied().collect::<HashSet<_>>();
        self.define(IrType::Unit, IrOperation::SnapshotAtomicBegin)?;
        let ledger_site = self
            .lock_ledger_sources
            .get(site)
            .cloned()
            .unwrap_or_else(|| "<unknown site>".to_owned());
        let ledger = Rc::new(RefCell::new(AtomicLedger::new(ledger_site.clone())));
        let guard_site = guard
            .and_then(CheckedExpression::carrier)
            .and_then(|path| self.lock_ledger_sources.get(path))
            .cloned()
            .unwrap_or_else(|| ledger_site.clone());
        let address = self.expression(&target.handle)?;
        let IrType::Address(IrAddressed::Nominal(nominal)) = self.value_type(address)? else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let loaded = self.define(
            IrType::Nominal(nominal),
            IrOperation::Load {
                address,
                referent: IrAddressed::Nominal(nominal),
            },
        )?;
        let object = if target.kept_by_place {
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
        let state_type = lower_type(self.erasure, target.state)?;
        let referent = IrAddressed::of(state_type).ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let state = self.define(
            IrType::Address(referent),
            IrOperation::SharedState { nominal, object },
        )?;
        if target.reads {
            self.readonly_atomic_roots.insert(target.binding);
        }
        self.bind_reference(target.binding, state)?;
        let maps = self.map_paths(state_type)?;
        let object_path = self.object_path(state_type);
        let has_object = object_path.is_some() || maps.is_empty();
        let uses = self.unit_uses(target, guard, body, &maps, has_object, &enclosing);
        // Take order: each map at its path, the object's lock at the first
        // part, in field order, that no map covers.
        let mut order = (0..uses.len())
            .filter(|unit| uses[*unit].reached)
            .collect::<Vec<_>>();
        order.sort_by_key(|unit| {
            if *unit < maps.len() {
                maps[*unit].clone()
            } else {
                object_path.clone().unwrap_or_default()
            }
        });
        let several = order.len() > 1;
        let mut positions = vec![None; uses.len()];
        let mut units = Vec::with_capacity(order.len());
        let mut eager = None;
        let mut eager_direct = BTreeMap::new();
        for unit in order {
            let used = &uses[unit];
            let position = units.len();
            positions[unit] = Some(position);
            if used.guard {
                eager_direct.insert(position, guard_site.clone());
            } else if !used.body {
                // A unit only effects or facts reach has no executable first
                // use to trigger a lazy take, but the statement still holds it.
                eager_direct.insert(position, target.ledger_site.clone());
            }
            if eager_direct.contains_key(&position) {
                eager = Some(position);
            }
            let (path, lock) = if unit < maps.len() {
                let path = maps[unit].clone();
                ledger.borrow_mut().push_group(
                    format!(
                        "{}^{}",
                        target.ledger_name,
                        path.iter().map(|field| format!(".{field}")).collect::<String>()
                    ),
                    "map".to_owned(),
                );
                let steps = path
                    .iter()
                    .copied()
                    .map(CheckedPlaceStep::Field)
                    .collect::<Vec<_>>();
                let field = self.project_address_path(state, &steps)?;
                let IrType::Address(field_referent) = self.value_type(field)? else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let table_type = field_referent.ty();
                let map = self
                    .table_nominal(table_type)
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                let table = self.define(
                    table_type,
                    IrOperation::Load {
                        address: field,
                        referent: field_referent,
                    },
                )?;
                let read = !used.written;
                let lock = match used.keys.as_slice() {
                    [(key_binding, key)] if !used.whole => {
                        let key = self.expression(key)?;
                        Lock::Entry {
                            nominal: map,
                            table,
                            record: self.record(IrRecordKind::TableEntry)?,
                            key,
                            key_binding: *key_binding,
                            read: read && !used.guard,
                            // Guards keep the existing reservation until
                            // their watch is registered.
                            inserts: used.written || used.guard,
                            stable_absence: several,
                        }
                    }
                    _ => Lock::Whole {
                        nominal: map,
                        table,
                        record: self.record(IrRecordKind::TableHold)?,
                        read: read && !used.guard,
                    },
                };
                (Some(path), lock)
            } else {
                ledger
                    .borrow_mut()
                    .push_group(format!("{}^", target.ledger_name), target.ledger_state.clone());
                (None, Lock::Object)
            };
            units.push(Unit {
                path,
                wrote: used.written,
                flag: None,
                lock,
            });
        }
        // Eager units need no frame flag. Lazy state uses one naturally
        // aligned word.
        for (position, unit) in units.iter_mut().enumerate() {
            if eager.is_none_or(|last| position > last) {
                let zero = self.u64_constant(0)?;
                unit.flag = Some(self.define(
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
                )?);
            }
        }
        let region = AtomicRegion {
            binding: target.binding,
            object,
            nominal,
            state,
            owned: !target.kept_by_place,
            units,
            maps,
            has_object,
            positions,
            ledger: ledger.clone(),
        };
        // The guard's and the block's map subscripts read the region's entry
        // units, so it is in place before either is lowered.
        self.atomics.push(region.clone());
        let lowered = (|| {
            if let Some(guard) = guard {
                let mut reach = Vec::new();
                expression_reach(guard, &mut reach);
                let read = region.positions_of(&reach);
                self.lower_guard(&region, guard, &read, eager, &eager_direct, &ledger_site)?;
            } else if let Some(last) = eager {
                self.take_units_through(&region, last, &eager_direct, true)?;
            }
            ledger.borrow_mut().body_entry = self.current;
            self.lower_statements(body, give_target)
        })();
        self.atomics.pop();
        lowered?;
        if self.current.is_some() {
            let drops = self.lower_drops(fallthrough_drops)?;
            self.append_drops(drops)?;
            self.leave_atomic(&region)?;
        }
        self.bindings.retain(|b, _| enclosing.contains(b));
        self.lock_ledger
            .extend(ledger.borrow().render(self.function_name, &self.blocks));
        Ok(())
    }
    /// What the guard and block do with each unit, maps in `maps` order
    /// and then the object's own lock. A unit's entry is locked alone only
    /// when every reach of the map subscripts it by one key binding the
    /// statement neither declares nor writes; any other reach takes the
    /// whole map. The parts the state type's invariants and a proof
    /// statement name count as reached [SHARE-3].
    fn unit_uses(
        &self,
        target: &CheckedTarget,
        guard: Option<&CheckedExpression>,
        body: &[CheckedStatement],
        maps: &[Vec<u32>],
        has_object: bool,
        enclosing: &HashSet<BindingId>,
    ) -> Vec<UnitUse> {
        let object = maps.len();
        let mut uses = vec![UnitUse::default(); maps.len() + 1];
        let mut guard_reach = Vec::new();
        if let Some(guard) = guard {
            expression_reach(guard, &mut guard_reach);
        }
        let mut body_reach = Vec::new();
        block_reach(body, &mut body_reach);
        let stable = |binding: BindingId| {
            binding != target.binding
                && enclosing.contains(&binding)
                && !target.written_roots.contains(&binding)
        };
        for (reaches, in_guard) in [(&guard_reach, true), (&body_reach, false)] {
            for reach in reaches.iter().filter(|reach| reach.binding == target.binding) {
                let path = reach.path();
                let fields_only = reach.fields.len() == path.len();
                for unit in units_of(maps, has_object, reach) {
                    let used = &mut uses[unit];
                    used.reached = true;
                    if in_guard {
                        used.guard = true;
                    } else {
                        used.body = true;
                    }
                    if unit == object {
                        continue;
                    }
                    match reach.tail {
                        Tail::Key(offset) if fields_only && maps[unit] == path => {
                            match key_binding_of(offset).filter(|key| stable(*key)) {
                                Some(key) => {
                                    if !used.keys.iter().any(|(known, _)| *known == key) {
                                        used.keys.push((key, offset.clone()));
                                    }
                                }
                                None => used.whole = true,
                            }
                        }
                        _ => used.whole = true,
                    }
                }
            }
        }
        for write in &target.writes {
            let fields = write
                .iter()
                .map_while(|step| match step {
                    CheckedEffectStep::Field(field) => Some(*field),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let covers = fields.len() == write.len();
            let mut written = Vec::new();
            if let Some(unit) = maps.iter().position(|map| *map == fields) {
                written.push(unit);
            } else {
                if has_object {
                    written.push(object);
                }
                if covers {
                    written.extend(
                        maps.iter()
                            .enumerate()
                            .filter(|(_, map)| map.starts_with(&fields))
                            .map(|(unit, _)| unit),
                    );
                }
            }
            for unit in written {
                let used = &mut uses[unit];
                if !used.reached {
                    used.reached = true;
                    used.whole = true;
                }
                used.written = true;
            }
        }
        if !target.invariants.is_empty() || block_has_proof(body) {
            for (unit, used) in uses.iter_mut().enumerate() {
                if unit == object && !has_object {
                    continue;
                }
                used.reached = true;
                used.whole = true;
            }
        }
        uses
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
    pub(super) fn take_units_for(
        &mut self,
        statement: &CheckedStatement,
    ) -> Result<(), LoweringFailure> {
        let Some(region) = self.atomics.last().cloned() else {
            return Ok(());
        };
        let mut reach = Vec::new();
        statement_reach(statement, &mut reach);
        let reached = region.positions_of(&reach);
        if let Some(last) = reached.last().copied() {
            let source = statement
                .ledger_path()
                .and_then(|path| self.lock_ledger_sources.get(path))
                .cloned()
                .unwrap_or_else(|| "<unknown site>".to_owned());
            let direct = reached
                .into_iter()
                .map(|unit| (unit, source.clone()))
                .collect();
            self.take_units_through(&region, last, &direct, false)?;
        }
        Ok(())
    }
    /// Takes every unit up to `last` not yet taken, in order. Before the
    /// guard every unit through `last` is taken; in the block an eager
    /// unit is already held.
    fn take_units_through(
        &mut self,
        region: &AtomicRegion,
        last: usize,
        direct: &BTreeMap<usize, String>,
        before_guard: bool,
    ) -> Result<(), LoweringFailure> {
        for (index, unit) in region.units.iter().enumerate().take(last + 1) {
            if unit.flag.is_none() && !before_guard {
                continue;
            }
            // Observe this request before the taken-word branch. That branch
            // can skip the take only when this execution already holds it.
            if let Some(block) = self.current {
                region
                    .ledger
                    .borrow_mut()
                    .request(index, block, direct, before_guard);
            }
            let done = unit
                .flag
                .map(|flag| self.group_flag(flag, true))
                .transpose()?;
            match &unit.lock {
                // The first unit holds nothing before it, so it waits and
                // suspends as a first object's acquire does.
                Lock::Object => {
                    self.define(
                        IrType::Unit,
                        if index == 0 {
                            IrOperation::SharedAcquire {
                                object: region.object,
                            }
                        } else {
                            IrOperation::SharedTake {
                                object: region.object,
                            }
                        },
                    )?;
                }
                Lock::Entry {
                    table,
                    record,
                    key,
                    read,
                    inserts,
                    stable_absence,
                    ..
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
                Lock::Whole {
                    table,
                    record,
                    read,
                    ..
                } => {
                    let record = *record;
                    self.define(
                        IrType::Unit,
                        IrOperation::TableHoldBegin {
                            record,
                            table: *table,
                        },
                    )?;
                    self.define(IrType::Unit, IrOperation::TableHoldWhole { record })?;
                    if *read {
                        self.define(IrType::Unit, IrOperation::TableHoldRead { record })?;
                    }
                    self.define(IrType::Unit, IrOperation::TableHoldTake { record })?;
                }
            }
            if let Some(flag) = unit.flag {
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
    /// The entry slot a subscript of a map the statement locks by one entry
    /// selects: `fields` are the subscripted map's struct fields below the
    /// held state, and `offset` must read the unit's key binding.
    pub(super) fn atomic_entry_slot(
        &mut self,
        fields: &[u32],
        offset: &CheckedExpression,
    ) -> Result<Option<IrValueId>, LoweringFailure> {
        let Some(region) = self.atomics.last() else {
            return Ok(None);
        };
        let Some(Lock::Entry {
            nominal,
            record,
            key_binding,
            ..
        }) = region
            .units
            .iter()
            .find(|unit| unit.path.as_deref() == Some(fields))
            .map(|unit| &unit.lock)
        else {
            return Ok(None);
        };
        let (nominal, record) = (*nominal, *record);
        if key_binding_of(offset) != Some(*key_binding) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let referent = self.table_entry(nominal)?;
        Ok(Some(self.define(
            IrType::Address(referent),
            IrOperation::TableEntrySlot { nominal, record },
        )?))
    }
    /// Whether `address` is the state the innermost statement holds, so that
    /// a place's field path below it names its map units.
    pub(super) fn is_held_state(&self, address: IrValueId) -> bool {
        self.atomics
            .last()
            .is_some_and(|region| region.state == address)
    }
    /// Gives up every unit, last first. After a false guard, `watched`, the
    /// statement wrote nothing; on leaving its block it releases a unit as
    /// written when the block or guard may write it.
    fn release_units(
        &mut self,
        region: &AtomicRegion,
        watched: bool,
    ) -> Result<(), LoweringFailure> {
        for unit in region.units.iter().rev() {
            let done = unit
                .flag
                .map(|flag| self.group_flag(flag, false))
                .transpose()?;
            match &unit.lock {
                Lock::Object => {
                    self.define(
                        IrType::Unit,
                        IrOperation::SharedUnlock {
                            object: region.object,
                            wrote: unit.wrote && !watched,
                        },
                    )?;
                }
                Lock::Entry {
                    nominal,
                    record,
                    read,
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
                Lock::Whole {
                    nominal, record, ..
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
            if let Some(flag) = unit.flag {
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
        self.define(IrType::Unit, IrOperation::SnapshotAtomicEnd)?;
        Ok(())
    }
    fn lower_guard(
        &mut self,
        region: &AtomicRegion,
        guard: &CheckedExpression,
        read: &BTreeSet<usize>,
        eager: Option<usize>,
        eager_direct: &BTreeMap<usize, String>,
        ledger_site: &str,
    ) -> Result<(), LoweringFailure> {
        let mut read = read.clone();
        let mut direct = eager_direct.clone();
        if read.is_empty() {
            read.extend(0..region.units.len());
            // The watch fallback really takes every unit before evaluating
            // a guard that reads none. It is an eager request, not a body use.
            direct.extend(read.iter().map(|index| (*index, ledger_site.to_owned())));
        }
        let acquire = self.new_block(&[])?.0;
        self.terminate(IrTerminator::Jump {
            target: acquire,
            arguments: Vec::new(),
            drops: Vec::new(),
        })?;
        self.current = Some(acquire);
        self.define(IrType::Unit, IrOperation::SnapshotAtomicBegin)?;
        if let Some(last) = read.last().copied().max(eager) {
            self.take_units_through(region, last, &direct, true)?;
        }
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
        let bindings = [region.binding];
        if region.units.len() == 1 && matches!(region.units[0].lock, Lock::Object) {
            let test = self.wake_test(guard, region.binding, region.state, &bindings)?;
            self.define(
                IrType::Unit,
                IrOperation::SharedWatch {
                    object: region.object,
                    test,
                },
            )?;
        } else {
            let record = self.record(IrRecordKind::Watch)?;
            self.define(IrType::Unit, IrOperation::WatchBegin { record })?;
            for index in read {
                match &region.units[index].lock {
                    Lock::Object => {
                        let test =
                            self.wake_test(guard, region.binding, region.state, &bindings)?;
                        self.define(
                            IrType::Unit,
                            IrOperation::WatchObject {
                                record,
                                object: region.object,
                                test,
                            },
                        )?;
                    }
                    Lock::Entry { table, .. } | Lock::Whole { table, .. } => {
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
            self.release_units(region, true)?;
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
    /// The field path of every map a value of type `ty` holds through
    /// struct fields alone, in field order: the paths a shared state's map
    /// units take, and the maps a write keeps where they stand.
    pub(super) fn map_paths(&self, ty: IrType) -> Result<Vec<Vec<u32>>, LoweringFailure> {
        let mut paths = Vec::new();
        self.collect_map_paths(ty, &mut Vec::new(), &mut paths)?;
        Ok(paths)
    }
    fn collect_map_paths(
        &self,
        ty: IrType,
        prefix: &mut Vec<u32>,
        paths: &mut Vec<Vec<u32>>,
    ) -> Result<(), LoweringFailure> {
        if self.table_nominal(ty).is_some() {
            paths.push(prefix.clone());
            return Ok(());
        }
        if let IrType::Nominal(nominal) = ty
            && let IrNominalKind::Struct { fields } = &self.nominals[nominal.index()].kind
        {
            for (index, field) in fields.iter().enumerate() {
                prefix.push(u32::try_from(index).map_err(|_| LoweringFailure::CounterOverflow)?);
                self.collect_map_paths(field.ty, prefix, paths)?;
                prefix.pop();
            }
        }
        Ok(())
    }
    /// The path of the first part, in field order, that no map covers.
    fn object_path(&self, ty: IrType) -> Option<Vec<u32>> {
        if self.table_nominal(ty).is_some() {
            return None;
        }
        if let IrType::Nominal(nominal) = ty
            && let IrNominalKind::Struct { fields } = &self.nominals[nominal.index()].kind
        {
            return fields.iter().enumerate().find_map(|(index, field)| {
                let mut path = self.object_path(field.ty)?;
                path.insert(0, u32::try_from(index).ok()?);
                Some(path)
            });
        }
        Some(Vec::new())
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
        if let Some(block) = self.current {
            region.ledger.borrow_mut().exits.insert(block);
        }
        self.release_units(region, false)?;
        if region.owned {
            self.append_drops(vec![IrDrop {
                subject: IrDropSubject::Value(region.object),
                ty: IrType::Nominal(region.nominal),
            }])?;
        }
        Ok(())
    }
    pub(super) fn atomic_depth(&self) -> usize {
        self.atomics.len()
    }
}
impl AtomicRegion {
    /// The positions of the units these reaches through the statement's
    /// binding select.
    fn positions_of(&self, reaches: &[Reach<'_>]) -> BTreeSet<usize> {
        reaches
            .iter()
            .filter(|reach| reach.binding == self.binding)
            .flat_map(|reach| units_of(&self.maps, self.has_object, reach))
            .filter_map(|unit| self.positions[unit])
            .collect()
    }
}
/// The units one reach selects, maps by their index in `maps` and the
/// object's own lock as `maps.len()`: a map's own path selects that map, and
/// any other path the object's lock together with every map below it that
/// the reach covers whole.
fn units_of(maps: &[Vec<u32>], has_object: bool, reach: &Reach<'_>) -> Vec<usize> {
    let path = reach.path();
    if reach.fields.len() == path.len()
        && let Some(unit) = maps.iter().position(|map| *map == path)
    {
        return vec![unit];
    }
    let mut units = Vec::new();
    if has_object {
        units.push(maps.len());
    }
    if reach.fields.len() == path.len() && matches!(reach.tail, Tail::Whole) {
        units.extend(
            maps.iter()
                .enumerate()
                .filter(|(_, map)| map.starts_with(&path))
                .map(|(unit, _)| unit),
        );
    }
    units
}
/// The binding a map key offset reads as a whole, `k` or `k^`.
fn key_binding_of(offset: &CheckedExpression) -> Option<BindingId> {
    match offset {
        CheckedExpression::Binding { binding, .. }
        | CheckedExpression::DerefAddressed { binding, .. } => Some(*binding),
        _ => None,
    }
}
fn block_has_proof(statements: &[CheckedStatement]) -> bool {
    statements.iter().any(|statement| match statement {
        CheckedStatement::Proof(_) => true,
        CheckedStatement::Loop { body, .. } | CheckedStatement::CountedRange { body, .. } => {
            block_has_proof(body)
        }
        CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
            arms.iter().any(|arm| block_has_proof(&arm.body))
        }
        _ => false,
    })
}
/// Executable reaches throughout a block, reusing the same direct-expression
/// walk that triggers lazy acquisition while lowering each statement.
fn block_reach<'a>(statements: &'a [CheckedStatement], reach: &mut Vec<Reach<'a>>) {
    for statement in statements {
        statement_reach(statement, reach);
        match statement {
            CheckedStatement::Loop { body, .. }
            | CheckedStatement::CountedRange { body, .. } => block_reach(body, reach),
            CheckedStatement::Match { arms, .. }
            | CheckedStatement::ValueMatchLet { arms, .. } => {
                for arm in arms {
                    block_reach(&arm.body, reach);
                }
            }
            CheckedStatement::Let { .. }
            | CheckedStatement::DestructuringLet { .. }
            | CheckedStatement::PropagateLet { .. }
            | CheckedStatement::Evaluate { .. }
            | CheckedStatement::DropExpression { .. }
            | CheckedStatement::Set { .. }
            | CheckedStatement::Proof(_)
            | CheckedStatement::Return { .. }
            | CheckedStatement::Give { .. }
            | CheckedStatement::Break { .. }
            | CheckedStatement::Continue { .. }
            | CheckedStatement::Atomic { .. } => {}
        }
    }
}
/// The places a statement's own expressions reach, not those of the blocks
/// it holds.
fn statement_reach<'a>(statement: &'a CheckedStatement, reach: &mut Vec<Reach<'a>>) {
    match statement {
        CheckedStatement::Let { value, .. }
        | CheckedStatement::DestructuringLet { value, .. }
        | CheckedStatement::Evaluate { value, .. }
        | CheckedStatement::DropExpression { value, .. }
        | CheckedStatement::Return { value, .. }
        | CheckedStatement::Give { value, .. } => expression_reach(value, reach),
        CheckedStatement::PropagateLet { scrutinee, .. }
        | CheckedStatement::Match { scrutinee, .. }
        | CheckedStatement::ValueMatchLet { scrutinee, .. } => expression_reach(scrutinee, reach),
        CheckedStatement::Set { target, value, .. } => {
            match target {
                CheckedSetTarget::Place(place) => reach.push(Reach {
                    binding: place.binding,
                    fields: &[],
                    tail: Tail::Whole,
                }),
                CheckedSetTarget::RangeIndex(place) => element_reach(place, reach),
                CheckedSetTarget::Storage(root) => container_reach(root, reach),
            }
            expression_reach(value, reach);
        }
        CheckedStatement::CountedRange { lower, upper, .. } => {
            expression_reach(lower, reach);
            expression_reach(upper, reach);
        }
        CheckedStatement::Loop { .. }
        | CheckedStatement::Proof(_)
        | CheckedStatement::Break { .. }
        | CheckedStatement::Continue { .. }
        | CheckedStatement::Atomic { .. } => {}
    }
}
fn whole(binding: BindingId) -> Reach<'static> {
    Reach {
        binding,
        fields: &[],
        tail: Tail::Whole,
    }
}
fn expression_reach<'a>(expression: &'a CheckedExpression, reach: &mut Vec<Reach<'a>>) {
    match expression {
        CheckedExpression::Constant(_) | CheckedExpression::NamedConstant { .. } => {}
        CheckedExpression::Binding { binding, .. }
        | CheckedExpression::DerefAddressed { binding, .. }
        | CheckedExpression::Project { binding, .. } => reach.push(whole(*binding)),
        CheckedExpression::BoxTake { binding, path, .. } => {
            path_reach(*binding, path, reach);
            steps_reach(path, reach);
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
                expression_reach(argument, reach);
            }
        }
        // A field read through a reference, `s^.f.g`, reaches the place at
        // that path and not the whole referent; its fields are not kept, so
        // it reaches the object's own lock and every map below the root.
        CheckedExpression::ProjectValue { value, .. } => match projected_root(expression) {
            Some(binding) => reach.push(Reach {
                binding,
                fields: &[],
                tail: Tail::Below,
            }),
            None => expression_reach(value, reach),
        },
        CheckedExpression::NumericConversion { value, .. }
        | CheckedExpression::Reinterpret { value, .. }
        | CheckedExpression::BoxDeref { value, .. } => expression_reach(value, reach),
        CheckedExpression::ArrayMeasure { root, .. } => array_reach(root, reach),
        CheckedExpression::ArrayIndex { root, offset, .. } => {
            array_reach(root, reach);
            expression_reach(offset, reach);
        }
        CheckedExpression::BufferMeasure { root, .. } => {
            path_reach(root.binding, &root.path, reach);
            steps_reach(&root.path, reach);
        }
        CheckedExpression::BufferIndex { root, offset, .. } => {
            path_reach(root.binding, &root.path, reach);
            steps_reach(&root.path, reach);
            expression_reach(offset, reach);
        }
        CheckedExpression::RangeOf {
            source, start, end, ..
        } => {
            match source {
                CheckedRangeSource::Storage(root) => container_reach(root, reach),
                CheckedRangeSource::Range(root) => reach.push(whole(root.binding)),
                CheckedRangeSource::Element(place) => element_reach(place, reach),
            }
            expression_reach(start, reach);
            expression_reach(end, reach);
        }
        CheckedExpression::RangeMeasure { root, .. } => {
            if let Some(formation) = root.formation.as_deref() {
                expression_reach(formation, reach);
            } else {
                reach.push(whole(root.binding));
            }
        }
        CheckedExpression::RangeElementMeasure { place, .. }
        | CheckedExpression::RangeIndex { place, .. }
        | CheckedExpression::BorrowRangeIndex { place, .. } => element_reach(place, reach),
        CheckedExpression::ContainerMeasure { root, .. }
        | CheckedExpression::ReadStorage { root, .. }
        | CheckedExpression::BorrowAddressed { root, .. } => container_reach(root, reach),
        CheckedExpression::BorrowSegment { root, segment, .. } => {
            match root {
                crate::semantic::CheckedSegmentSource::Storage(root) => {
                    container_reach(root, reach)
                }
                crate::semantic::CheckedSegmentSource::Element(place) => {
                    element_reach(place, reach)
                }
            }
            if let Some(offset) = segment.offset() {
                expression_reach(offset, reach);
            }
        }
    }
}
/// The binding a chain of field reads through a reference's referent,
/// `s^.f.g`, selects below.
fn projected_root(expression: &CheckedExpression) -> Option<BindingId> {
    match expression {
        CheckedExpression::DerefAddressed { binding, .. } => Some(*binding),
        CheckedExpression::ProjectValue { value, .. } => projected_root(value),
        _ => None,
    }
}
/// The reach of a place `binding` and `path` select: its leading struct
/// fields, then a subscript's offset or another selection.
fn path_reach<'a>(binding: BindingId, path: &'a [CheckedPlaceStep], reach: &mut Vec<Reach<'a>>) {
    let fields = path
        .iter()
        .take_while(|step| matches!(step, CheckedPlaceStep::Field(_)))
        .count();
    let tail = match path.get(fields) {
        None => Tail::Whole,
        Some(CheckedPlaceStep::Subscript(subscript)) => Tail::Key(&subscript.offset),
        Some(_) => Tail::Below,
    };
    reach.push(Reach {
        binding,
        fields: &path[..fields],
        tail,
    });
}
fn container_reach<'a>(root: &'a CheckedContainerRoot, reach: &mut Vec<Reach<'a>>) {
    if let Some(binding) = root.binding() {
        path_reach(binding, &root.path, reach);
    }
    steps_reach(&root.path, reach);
}
fn element_reach<'a>(place: &'a CheckedRangeElementPlace, reach: &mut Vec<Reach<'a>>) {
    if let Some(formation) = place.root.formation.as_deref() {
        expression_reach(formation, reach);
    } else {
        reach.push(whole(place.root.binding));
    }
    expression_reach(&place.offset, reach);
    steps_reach(&place.path, reach);
}
fn array_reach<'a>(root: &'a CheckedArrayRoot, reach: &mut Vec<Reach<'a>>) {
    if let CheckedArrayRoot::Binding { binding, .. } = root {
        reach.push(Reach {
            binding: *binding,
            fields: &[],
            tail: Tail::Below,
        });
    }
}
fn steps_reach<'a>(steps: &'a [CheckedPlaceStep], reach: &mut Vec<Reach<'a>>) {
    for step in steps {
        if let CheckedPlaceStep::Subscript(subscript) = step {
            expression_reach(&subscript.offset, reach);
        }
    }
}
