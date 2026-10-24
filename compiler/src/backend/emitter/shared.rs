//! Shared objects, keyed tables and key sets [SHARE-1], and the locks of
//! atomic statements [SHARE-2, SHARE-3].
//!
//! A handle is one pointer to the object, whose runtime header in the
//! completion bridge precedes its state at [`SHARED_STATE_OFFSET`]. A table
//! is one pointer to the runtime's index, a key set its count and a pointer
//! to the runtime's store. A statement takes the object's own lock for
//! writing, which may suspend its frame exactly as a join does and asks again
//! when the frame resumes; the entries of its tables are locked through
//! records the frame keeps (compiler/waiting-contexts/state-locks), reserved
//! once in the entry block. A guard that reads false watches what it read,
//! which suspends, and the lowering takes the units again when the frame
//! resumes.
//!
//! [`SHARED_STATE_OFFSET`]: crate::backend::SHARED_STATE_OFFSET

use std::collections::BTreeMap;
use std::fmt::Write;

use super::frames::{HANDLE, labels};
use super::*;
use crate::{IrRecord, IrRecordKind, IrShared, IrWakeTest};

/// The frame storage of one record, by its kind.
fn record_storage(kind: IrRecordKind) -> String {
    match kind {
        IrRecordKind::TableEntry => format!("[{} x i8]", crate::backend::TABLE_ENTRY_SIZE),
        IrRecordKind::TableHold => format!("[{} x i8]", crate::backend::TABLE_HOLD_SIZE),
        IrRecordKind::Entries => "{ ptr, i64, i64 }".to_owned(),
        IrRecordKind::Watch => format!("[{} x i8]", crate::backend::WATCH_SIZE),
    }
}

/// The name of a record's storage.
fn record_name(record: IrRecord) -> String {
    format!("%wf.record.{}", record.index())
}

/// The frame word in which a lock of one entry keeps its slot's address.
fn record_slot_name(record: IrRecord) -> String {
    format!("%wf.record.{}.slot", record.index())
}

/// The scratch place a new key set is written to before it is a value.
fn key_set_scratch_name(result: IrValueId) -> String {
    format!("%wf.keyset.v{}", result.ordinal())
}

/// The entry-block lines that reserve every record a function's statements
/// keep, the scratch place of each new key set and the storage of each wake
/// test, once per call.
pub(super) fn record_prelude(function: &IrFunction) -> String {
    let mut records = BTreeMap::new();
    let mut scratch = Vec::new();
    let mut tests = Vec::new();
    for instruction in function
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
    {
        let IrInstruction::Define {
            result, operation, ..
        } = instruction
        else {
            continue;
        };
        if let IrOperation::TableHeldEntries {
            read_record: Some(record),
            ..
        } = operation
        {
            records.insert(record.index(), *record);
        }
        if let IrOperation::SharedWatch { test: Some(test), .. }
        | IrOperation::WatchObject { test: Some(test), .. } = operation
        {
            tests.push((*result, test));
        }
        let record = match operation {
            IrOperation::TableLockEntry { record, .. }
            | IrOperation::TableEntrySlot { record, .. }
            | IrOperation::TableUnlockEntry { record, .. }
            | IrOperation::TableHoldBegin { record, .. }
            | IrOperation::TableHoldRead { record }
            | IrOperation::TableHoldWhole { record }
            | IrOperation::TableHoldTake { record }
            | IrOperation::TableHoldRelease { record, .. }
            | IrOperation::TableHeldEntries { record, .. }
            | IrOperation::WatchBegin { record }
            | IrOperation::WatchObject { record, .. }
            | IrOperation::WatchTable { record, .. }
            | IrOperation::WatchCancel { record, .. }
            | IrOperation::WatchPark { record, .. } => *record,
            IrOperation::KeySetNew { .. } => {
                scratch.push(*result);
                continue;
            }
            _ => continue,
        };
        records.insert(record.index(), record);
    }
    let mut prelude = String::new();
    for block in function.blocks() {
        for instruction in block.instructions() {
            if let IrInstruction::Define {
                result,
                operation: IrOperation::ConcurrentHashMapScanWithin { .. },
                ..
            } = instruction
            {
                prelude.push_str(&format!(
                    "  %scan.needs.{} = alloca i32, align 4\n",
                    result.index()
                ));
            }
        }
    }
    for record in records.values() {
        prelude.push_str(&format!(
            "  {} = alloca {}, align 8\n",
            record_name(*record),
            record_storage(record.kind())
        ));
        if record.kind() == IrRecordKind::TableEntry {
            prelude.push_str(&format!(
                "  {} = alloca ptr, align 8\n",
                record_slot_name(*record)
            ));
        }
    }
    for result in scratch {
        prelude.push_str(&format!(
            "  {} = alloca {{ i64, ptr }}, align 8\n",
            key_set_scratch_name(result)
        ));
    }
    for (result, test) in tests {
        prelude.push_str(&super::wake::wake_test_storage(
            result,
            test.callback.is_some(),
        ));
        if test.callback.is_some() {
            prelude.push_str(&format!(
                "  {}.captures = alloca [{} x i64], align 8\n",
                super::wake::wake_test_name(result),
                test.values.len().saturating_sub(1).max(1),
            ));
        }
    }
    prelude
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn emit_snapshot_atomic(
        &mut self,
        result: IrValueId,
        begin: bool,
    ) -> Result<(), BackendFailure> {
        if snapshot::required(self.program) {
            self.emit_unit_call(result, if begin {
                "wf_snapshot_atomic_begin"
            } else {
                "wf_snapshot_atomic_end"
            }, "")
        } else {
            self.emit_constant(result, IrType::Unit, IrConstant::Unit)
        }
    }

    /// The state type of a shared-object nominal.
    fn shared_state(&self, nominal: IrNominalId) -> Result<IrType, BackendFailure> {
        match self.nominal(nominal)?.kind() {
            IrNominalKind::Shared {
                state,
                shape: IrShared::Object,
            } => Ok(*state),
            _ => Err(BackendFailure::InvalidIr),
        }
    }

    /// A table nominal's entry type, the `Option<V>` its nodes keep a slot
    /// of.
    fn table_entry(&self, nominal: IrNominalId) -> Result<IrType, BackendFailure> {
        match self.nominal(nominal)?.kind() {
            IrNominalKind::Shared {
                shape: IrShared::Map { entry },
                ..
            } => Ok(*entry),
            _ => Err(BackendFailure::InvalidIr),
        }
    }

    /// Whether a value is a table.
    fn names_table(&self, value: IrValueId) -> Result<bool, BackendFailure> {
        let Some(IrType::Nominal(nominal)) = self.value_type(value) else {
            return Ok(false);
        };
        Ok(matches!(
            self.nominal(nominal)?.kind(),
            IrNominalKind::Shared {
                shape: IrShared::Map { .. },
                ..
            }
        ))
    }

    /// A table's entry type, refused unless the runtime reads whether an
    /// entry holds `Some` as it does: from a count-sized tag at offset zero,
    /// which differs from `None`'s, 0, the tag of a slot the runtime filled
    /// with zeros.
    fn checked_entry(&self, nominal: IrNominalId) -> Result<IrType, BackendFailure> {
        let entry = self.table_entry(nominal)?;
        let IrType::Nominal(option) = entry else {
            return Err(BackendFailure::InvalidIr);
        };
        let option = self.nominal(option)?;
        let IrNominalKind::Enum { variants } = option.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let none = variants
            .iter()
            .find(|variant| variant.fields().is_empty())
            .ok_or(BackendFailure::InvalidIr)?;
        if none.tag() != 0 || option.is_tag_only_enum() {
            return Err(BackendFailure::InvalidIr);
        }
        Ok(entry)
    }

    /// The runtime receives the same physical tag width as emitted storage.
    fn entry_tag_width(&self, nominal: IrNominalId) -> Result<u8, BackendFailure> {
        let IrType::Nominal(entry) = self.checked_entry(nominal)? else {
            return Err(BackendFailure::InvalidIr);
        };
        enum_tag_width(self.nominal(entry)?)
    }

    /// The bytes and the length of the range `key` names, as two named
    /// values prefixed by `bare`.
    fn key_parts(&mut self, bare: &str, key: IrValueId) -> Result<(), BackendFailure> {
        let key_type = self.output.type_name(
            self.program,
            self.value_type(key).ok_or(BackendFailure::InvalidIr)?,
        )?;
        let key_name = self.value_name(key);
        writeln!(
            self.output,
            "  %{bare}.key = extractvalue {key_type} {key_name}, 0\n  %{bare}.length = extractvalue {key_type} {key_name}, 1",
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    fn bare(&self, result: IrValueId) -> String {
        self.value_name(result).trim_start_matches('%').to_owned()
    }

    /// One call that answers nothing, then the unit value the operation
    /// defines.
    pub(super) fn emit_unit_call(
        &mut self,
        result: IrValueId,
        entry: &'static str,
        arguments: &str,
    ) -> Result<(), BackendFailure> {
        self.names(&[entry]);
        writeln!(self.output, "  call void @{entry}({arguments})")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    /// [SHARE-1] a new table whose nodes keep a slot of its entry type, of
    /// that type's size and alignment.
    pub(super) fn emit_keyed_table_new(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        capacity: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal) {
            return Err(BackendFailure::InvalidIr);
        }
        let entry = self.checked_entry(nominal)?;
        let entry_type = self.output.type_name(self.program, entry)?;
        self.names(&["wf__map_new"]);
        writeln!(
            self.output,
            "  {} = call ptr @wf__map_new(i64 ptrtoint (ptr getelementptr ({entry_type}, ptr null, i64 1) to i64), i64 ptrtoint (ptr getelementptr ({{ i8, {entry_type} }}, ptr null, i64 0, i32 1) to i64), i64 {})",
            self.value_name(result),
            self.value_name(capacity)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// How many entries of the table hold `Some`; under the statement's
    /// whole hold the runtime reads its entries' count-sized tags at offset
    /// zero, whose `None` is 0, so the count sees the statement's own writes.
    pub(super) fn emit_keyed_table_count(
        &mut self,
        result: IrValueId,
        table: IrValueId,
    ) -> Result<(), BackendFailure> {
        let Some(IrType::Nominal(nominal)) = self.value_type(table) else {
            return Err(BackendFailure::InvalidIr);
        };
        let tag_bytes = self.entry_tag_width(nominal)?.div_ceil(8);
        self.names(&["wf__keyed_table_count"]);
        writeln!(
            self.output,
            "  {} = call i64 @wf__keyed_table_count(ptr {}, i64 0, i32 {tag_bytes}, i64 0)",
            self.value_name(result),
            self.value_name(table)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Exchanges two tables' entries, each keeping its identity; the
    /// runtime first settles the entries of a hold of either taken whole,
    /// reading each entry's tag as a hold's release does.
    pub(super) fn emit_keyed_table_swap(
        &mut self,
        result: IrValueId,
        first: IrValueId,
        second: IrValueId,
    ) -> Result<(), BackendFailure> {
        let (Some(IrType::Nominal(nominal)), true) = (
            self.value_type(first),
            self.value_type(first) == self.value_type(second),
        ) else {
            return Err(BackendFailure::InvalidIr);
        };
        let tag_bytes = self.entry_tag_width(nominal)?.div_ceil(8);
        let arguments = format!(
            "ptr {}, ptr {}, i64 0, i32 {tag_bytes}, i64 0",
            self.value_name(first),
            self.value_name(second)
        );
        self.emit_unit_call(result, "wf__keyed_table_swap", &arguments)
    }

    /// One step of a scan: the runtime reads each entry's count-sized tag at
    /// offset 0 whose `None` is 0, so the step sees the statement's own
    /// writes, as a count does.
    pub(super) fn emit_keyed_table_scan(
        &mut self,
        result: IrValueId,
        table: IrValueId,
        cursor: IrValueId,
        count: IrValueId,
        set: IrValueId,
    ) -> Result<(), BackendFailure> {
        let Some(IrType::Nominal(nominal)) = self.value_type(table) else {
            return Err(BackendFailure::InvalidIr);
        };
        let tag_bytes = self.entry_tag_width(nominal)?.div_ceil(8);
        if self.value_type(set) != Some(IrType::Address(IrAddressed::KeySet)) {
            return Err(BackendFailure::InvalidIr);
        }
        self.names(&["wf__keyed_table_scan"]);
        writeln!(
            self.output,
            "  {} = call i64 @wf__keyed_table_scan(ptr {}, i64 {}, i64 {}, ptr {}, i64 0, i32 {tag_bytes}, i64 0)",
            self.value_name(result),
            self.value_name(table),
            self.value_name(cursor),
            self.value_name(count),
            self.value_name(set),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Construct either ScanStep variant through the ordinary enum storage
    /// path, including any handler words selected by match dispatch.
    pub(super) fn emit_keyed_table_scan_within(
        &mut self,
        result: IrValueId,
        operands: [IrValueId; 5],
    ) -> Result<(), BackendFailure> {
        let [table, cursor, count, limit, set] = operands;
        let Some(IrType::Nominal(map_nominal)) = self.value_type(table) else {
            return Err(BackendFailure::InvalidIr);
        };
        let tag_bytes = self.entry_tag_width(map_nominal)?.div_ceil(8);
        let Some(IrType::Nominal(nominal)) = self.value_type(result) else {
            return Err(BackendFailure::InvalidIr);
        };
        let IrNominalKind::Enum { variants } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        if variants.len() != 2
            || variants.iter().enumerate().any(|(tag, variant)| {
                variant.tag() != tag as u32
                    || variant.fields().len() != 1
                    || variant.fields()[0].ty() != (IrType::Integer { width: 64, signed: false })
            })
            || self.value_type(set) != Some(IrType::Address(IrAddressed::KeySet))
        {
            return Err(BackendFailure::InvalidIr);
        }
        let payload = self.next_temporary()?;
        let tag = self.next_temporary()?;
        let is_next = self.next_temporary()?;
        self.names(&["wf__keyed_table_scan_within"]);
        writeln!(
            self.output,
            "  %{payload} = call i64 @wf__keyed_table_scan_within(ptr {}, i64 {}, i64 {}, i64 {}, ptr {}, ptr %scan.needs.{}, i64 0, i32 {tag_bytes}, i64 0)\n  %{tag} = load i32, ptr %scan.needs.{}\n  %{is_next} = icmp eq i32 %{tag}, 0",
            self.value_name(table),
            self.value_name(cursor),
            self.value_name(count),
            self.value_name(limit),
            self.value_name(set),
            result.index(),
            result.index(),
        )?;
        let next = format!("scan.next.{}", result.index());
        let needs = format!("scan.needs.block.{}", result.index());
        let done = format!("scan.done.{}", result.index());
        writeln!(self.output, "  br i1 %{is_next}, label %{next}, label %{needs}")?;
        self.output.open_block(next);
        self.construct_scalar_enum_at(result, 0, &format!("%{payload}"))?;
        writeln!(self.output, "  br label %{done}")?;
        self.output.open_block(needs);
        self.construct_scalar_enum_at(result, 1, &format!("%{payload}"))?;
        writeln!(self.output, "  br label %{done}")?;
        self.output.open_block(done);
        Ok(())
    }

    /// Empties a table: the runtime settles the entries its whole hold took,
    /// reading each tag as a hold's release does, and hands the old entries
    /// to the table's drop helper once a statement's hold is given up, or at
    /// once under the hold the table keeps for writes no statement holds it
    /// for.
    pub(super) fn emit_keyed_table_clear(
        &mut self,
        result: IrValueId,
        table: IrValueId,
    ) -> Result<(), BackendFailure> {
        let Some(IrType::Nominal(nominal)) = self.value_type(table) else {
            return Err(BackendFailure::InvalidIr);
        };
        let tag_bytes = self.entry_tag_width(nominal)?.div_ceil(8);
        let release = super::cleanup::drop_helper_symbol(self.nominal(nominal)?);
        self.names(&[release.as_str()]);
        let arguments = format!(
            "ptr {}, i64 0, i32 {tag_bytes}, i64 0, ptr @{release}",
            self.value_name(table)
        );
        self.emit_unit_call(result, "wf__keyed_table_clear", &arguments)
    }

    /// The table's own write hold, if any, is given up, reading each entry's
    /// tag as a hold's release does.
    pub(super) fn emit_keyed_table_settle(
        &mut self,
        result: IrValueId,
        table: IrValueId,
    ) -> Result<(), BackendFailure> {
        let Some(IrType::Nominal(nominal)) = self.value_type(table) else {
            return Err(BackendFailure::InvalidIr);
        };
        let tag_bytes = self.entry_tag_width(nominal)?.div_ceil(8);
        let arguments = format!("ptr {}, i64 0, i32 {tag_bytes}, i64 0", self.value_name(table));
        self.emit_unit_call(result, "wf__keyed_table_settle", &arguments)
    }

    pub(super) fn emit_table_held_entry(
        &mut self,
        result: IrValueId,
        nominal: IrNominalId,
        table: IrValueId,
        key: IrValueId,
        write: bool,
    ) -> Result<(), BackendFailure> {
        let IrType::Nominal(option) = self.checked_entry(nominal)? else {
            return Err(BackendFailure::InvalidIr);
        };
        let symbol = format!(".wf_table_none.{}", self.nominal(option)?.link_name());
        let bare = self.bare(result);
        self.key_parts(&bare, key)?;
        self.names(&["wf__table_held_entry"]);
        self.output.symbol(symbol.clone());
        writeln!(self.output, "  %{bare}.slot = call ptr @wf__table_held_entry(ptr {}, ptr %{bare}.key, i64 %{bare}.length, i32 {})\n  %{bare}.missing = icmp eq ptr %{bare}.slot, null\n  {} = select i1 %{bare}.missing, ptr @{symbol}, ptr %{bare}.slot", self.value_name(table), u32::from(write), self.value_name(result)).map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_table_held_entries(
        &mut self,
        result: IrValueId,
        table: IrValueId,
        set: IrValueId,
        record: IrRecord,
        read_record: Option<IrRecord>,
    ) -> Result<(), BackendFailure> {
        self.names(&["wf__table_held_entries"]);
        let read_record = read_record
            .map(record_name)
            .unwrap_or_else(|| "null".to_owned());
        writeln!(self.output, "  call void @wf__table_held_entries(ptr {}, ptr {}, ptr {}, ptr {})\n  {} = getelementptr i8, ptr {}, i64 0", self.value_name(table), self.value_name(set), record_name(record), read_record, self.value_name(result), record_name(record)).map_err(|_| BackendFailure::TextEmission)
    }

    /// Locks one key's entry, keeping the lock in its record and the slot's
    /// address in the record's word.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_table_lock_entry(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        table: IrValueId,
        key: IrValueId,
        read: bool,
        inserts: bool,
        stable_absence: bool,
    ) -> Result<(), BackendFailure> {
        if !self.names_table(table)? || record.kind() != IrRecordKind::TableEntry {
            return Err(BackendFailure::InvalidIr);
        }
        let bare = self.bare(result);
        self.key_parts(&bare, key)?;
        self.names(&["wf__table_lock_entry"]);
        writeln!(
            self.output,
            "  %{bare}.slot = call ptr @wf__table_lock_entry(ptr {table}, ptr %{bare}.key, i64 %{bare}.length, i32 {read}, ptr {record})\n  store ptr %{bare}.slot, ptr {word}",
            table = self.value_name(table),
            read = if read {1 + 2 * u32::from(stable_absence)} else if !inserts {4 + 2 * u32::from(stable_absence)} else {0},
            record = record_name(record),
            word = record_slot_name(record),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    /// The address of the entry a lock of one key holds.
    pub(super) fn emit_table_entry_slot(
        &mut self,
        result: IrValueId,
        nominal: IrNominalId,
        record: IrRecord,
    ) -> Result<(), BackendFailure> {
        self.checked_entry(nominal)?;
        if record.kind() != IrRecordKind::TableEntry {
            return Err(BackendFailure::InvalidIr);
        }
        writeln!(
            self.output,
            "  {} = load ptr, ptr {}",
            self.value_name(result),
            record_slot_name(record)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Ends a lock of one key, telling the runtime whether the entry holds
    /// `Some`.
    pub(super) fn emit_table_unlock_entry(
        &mut self,
        result: IrValueId,
        nominal: IrNominalId,
        record: IrRecord,
    ) -> Result<(), BackendFailure> {
        let tag_ty = format!("i{}", self.entry_tag_width(nominal)?);
        if record.kind() != IrRecordKind::TableEntry {
            return Err(BackendFailure::InvalidIr);
        }
        let bare = self.bare(result);
        self.names(&["wf__table_unlock_entry"]);
        writeln!(
            self.output,
            "  %{bare}.slot = load ptr, ptr {word}\n  %{bare}.tag = load {tag_ty}, ptr %{bare}.slot\n  %{bare}.some = icmp ne {tag_ty} %{bare}.tag, 0\n  %{bare}.present = zext i1 %{bare}.some to i32\n  call void @wf__table_unlock_entry(ptr {record}, i32 %{bare}.present)",
            word = record_slot_name(record),
            record = record_name(record),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    pub(super) fn emit_table_hold_begin(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        table: IrValueId,
    ) -> Result<(), BackendFailure> {
        if !self.names_table(table)? || record.kind() != IrRecordKind::TableHold {
            return Err(BackendFailure::InvalidIr);
        }
        let arguments = format!(
            "ptr {}, ptr {}",
            record_name(record),
            self.value_name(table)
        );
        self.emit_unit_call(result, "wf__table_hold_begin", &arguments)
    }

    /// A call of a hold's own entry that takes only the record: making the
    /// take whole, the take itself.
    pub(super) fn emit_table_hold_call(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        entry: &'static str,
    ) -> Result<(), BackendFailure> {
        if record.kind() != IrRecordKind::TableHold {
            return Err(BackendFailure::InvalidIr);
        }
        let arguments = format!("ptr {}", record_name(record));
        self.emit_unit_call(result, entry, &arguments)
    }

    /// Gives up a hold: the runtime reads each entry's count-sized tag at
    /// offset 0 whose `None` is 0, to keep or remove it.
    pub(super) fn emit_table_hold_release(
        &mut self,
        result: IrValueId,
        nominal: IrNominalId,
        record: IrRecord,
    ) -> Result<(), BackendFailure> {
        let tag_bytes = self.entry_tag_width(nominal)?.div_ceil(8);
        if record.kind() != IrRecordKind::TableHold {
            return Err(BackendFailure::InvalidIr);
        }
        let arguments = format!("ptr {}, i64 0, i32 {tag_bytes}, i64 0", record_name(record));
        self.emit_unit_call(result, "wf__table_hold_release", &arguments)
    }

    /// [OP-4] the address of entry `offset` of the entries the record at
    /// `entries` names: the slot its hold keeps at the set's first position
    /// plus the offset.
    pub(super) fn keyed_entries_element_pointer(
        &mut self,
        entries: IrValueId,
        offset: IrValueId,
    ) -> Result<String, BackendFailure> {
        let hold = self.next_temporary()?;
        let first = self.next_temporary()?;
        let base = self.next_temporary()?;
        let position = self.next_temporary()?;
        let pointer = self.next_temporary()?;
        self.names(&["wf__table_hold_slot"]);
        writeln!(
            self.output,
            "  %{hold} = load ptr, ptr {entries}\n  %{first} = getelementptr inbounds i8, ptr {entries}, i64 8\n  %{base} = load i64, ptr %{first}\n  %{position} = add i64 %{base}, {offset}\n  %{pointer} = call ptr @wf__table_hold_slot(ptr %{hold}, i64 %{position})",
            entries = self.value_name(entries),
            offset = self.value_name(offset),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{pointer}"))
    }

    /// [SHARE-1] an empty key set: the runtime writes it to the scratch
    /// place, which is then read as the value.
    pub(super) fn emit_key_set_new(
        &mut self,
        result: IrValueId,
        ty: IrType,
        capacity: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::KeySet {
            return Err(BackendFailure::InvalidIr);
        }
        let scratch = key_set_scratch_name(result);
        self.names(&["wf__key_set_new"]);
        writeln!(
            self.output,
            "  call void @wf__key_set_new(ptr {scratch}, i64 {})\n  {} = load {{ i64, ptr }}, ptr {scratch}",
            self.value_name(capacity),
            self.value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Inserts a key, defining the index of its first insertion.
    pub(super) fn emit_key_set_insert(
        &mut self,
        result: IrValueId,
        set: IrValueId,
        key: IrValueId,
    ) -> Result<(), BackendFailure> {
        if self.value_type(set) != Some(IrType::Address(IrAddressed::KeySet)) {
            return Err(BackendFailure::InvalidIr);
        }
        let bare = self.bare(result);
        self.key_parts(&bare, key)?;
        self.names(&["wf__key_set_insert"]);
        writeln!(
            self.output,
            "  {} = call i64 @wf__key_set_insert(ptr {}, ptr %{bare}.key, i64 %{bare}.length)",
            self.value_name(result),
            self.value_name(set),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Reads the set's retained heap bytes without changing its storage.
    pub(super) fn emit_key_set_bytes(
        &mut self,
        result: IrValueId,
        set: IrValueId,
    ) -> Result<(), BackendFailure> {
        if self.value_type(set) != Some(IrType::Address(IrAddressed::KeySet)) {
            return Err(BackendFailure::InvalidIr);
        }
        self.names(&["wf__key_set_bytes"]);
        writeln!(
            self.output,
            "  {} = call i64 @wf__key_set_bytes(ptr {})",
            self.value_name(result),
            self.value_name(set),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Copies a key's first bytes into a range, defining the key's length.
    pub(super) fn emit_key_set_read_key(
        &mut self,
        result: IrValueId,
        set: IrValueId,
        index: IrValueId,
        out: IrValueId,
    ) -> Result<(), BackendFailure> {
        if self.value_type(set) != Some(IrType::Address(IrAddressed::KeySet)) {
            return Err(BackendFailure::InvalidIr);
        }
        let bare = self.bare(result);
        self.key_parts(&bare, out)?;
        self.names(&["wf__key_set_read_key"]);
        writeln!(
            self.output,
            "  {} = call i64 @wf__key_set_read_key(ptr {}, i64 {}, ptr %{bare}.key, i64 %{bare}.length)",
            self.value_name(result),
            self.value_name(set),
            self.value_name(index),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Begins a watch, or watches one object or table in it.
    pub(super) fn emit_watch_call(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        watched: Option<IrValueId>,
        entry: &'static str,
    ) -> Result<(), BackendFailure> {
        if record.kind() != IrRecordKind::Watch {
            return Err(BackendFailure::InvalidIr);
        }
        let arguments = match watched {
            Some(value) => format!(
                "ptr {}, ptr {}",
                record_name(record),
                self.value_name(value)
            ),
            None => format!("ptr {}", record_name(record)),
        };
        self.emit_unit_call(result, entry, &arguments)
    }

    /// With every unit given up: the runtime answers 0 when a watched unit
    /// was written since the watch began, and 1 when it has parked the
    /// frame, which then suspends until a write wakes it; the lowering takes
    /// the units again either way.
    pub(super) fn emit_watch_park(
        &mut self,
        result: IrValueId,
        record: IrRecord,
    ) -> Result<(), BackendFailure> {
        if record.kind() != IrRecordKind::Watch {
            return Err(BackendFailure::InvalidIr);
        }
        let prefix = labels("park", result);
        let site = self.wait_site_operand(result);
        self.names(&["wf__watch_park", "llvm.coro.save"]);
        writeln!(
            self.output,
            "  br label %{prefix}.try\n\
             {prefix}.try:\n  \
             %{prefix}.saved = call token @llvm.coro.save(ptr null)\n  \
             %{prefix}.parked = call i32 @wf__watch_park(ptr {}, ptr {HANDLE}, ptr {site})\n  \
             %{prefix}.suspends = icmp ne i32 %{prefix}.parked, 0\n  \
             br i1 %{prefix}.suspends, label %{prefix}.suspend, label %{prefix}.done\n\
             {prefix}.suspend:",
            record_name(record)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_suspension(
            &format!("%{prefix}.saved"),
            &prefix,
            &format!("{prefix}.done"),
        )?;
        self.output.open_block(format!("{prefix}.done"));
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    /// A new object sized for its state, holding one handle.
    pub(super) fn emit_shared_new(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal) {
            return Err(BackendFailure::InvalidIr);
        }
        let state = self.shared_state(nominal)?;
        let state_type = self.output.type_name(self.program, state)?;
        self.names(&["wf__shared_new"]);
        writeln!(
            self.output,
            "  {} = call ptr @wf__shared_new(i64 ptrtoint (ptr getelementptr ({state_type}, ptr null, i64 1) to i64))",
            self.value_name(result)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// The address of the object's state, behind its header.
    pub(super) fn emit_shared_state(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        object: IrValueId,
    ) -> Result<(), BackendFailure> {
        let state = self.shared_state(nominal)?;
        if self.value_type(object) != Some(IrType::Nominal(nominal))
            || IrAddressed::of(state).map(IrType::Address) != Some(ty)
        {
            return Err(BackendFailure::InvalidIr);
        }
        writeln!(
            self.output,
            "  {} = getelementptr inbounds i8, ptr {}, i64 {}",
            self.value_name(result),
            self.value_name(object),
            crate::backend::SHARED_STATE_OFFSET
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// One further handle: the same pointer, counted once more.
    pub(super) fn emit_shared_retain(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        object: IrValueId,
    ) -> Result<(), BackendFailure> {
        let Some(IrType::Nominal(source)) = self.value_type(object) else {
            return Err(BackendFailure::InvalidIr);
        };
        let frozen = matches!(
            self.nominal(nominal)?.kind(),
            IrNominalKind::Box {
                release: crate::IrReleaseClass::Frozen,
                ..
            }
        );
        if ty != IrType::Nominal(nominal) {
            return Err(BackendFailure::InvalidIr);
        }
        let retain = if frozen {
            if source != nominal {
                return Err(BackendFailure::InvalidIr);
            }
            "wf__frozen_share"
        } else {
            if self.shared_state(source)? != self.shared_state(nominal)? {
                return Err(BackendFailure::InvalidIr);
            }
            "wf__shared_share"
        };
        self.names(&[retain]);
        writeln!(
            self.output,
            "  call void @{retain}(ptr {object})\n  {} = getelementptr i8, ptr {object}, i64 0",
            self.value_name(result),
            object = self.value_name(object),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_decision_cell_new(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal) || self.nominal(nominal)?.name != "DecisionCell" {
            return Err(BackendFailure::InvalidIr);
        }
        self.names(&["wf__decision_cell_new"]);
        writeln!(
            self.output,
            "  {} = call ptr @wf__decision_cell_new()",
            self.value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_decision_cell(
        &mut self,
        result: IrValueId,
        ty: IrType,
        object: IrValueId,
        winner: Option<u32>,
    ) -> Result<(), BackendFailure> {
        if !matches!(ty, IrType::Nominal(id) if self.nominal(id)?.name == "DecisionState")
            || !matches!(self.value_type(object), Some(IrType::Nominal(id)) if self.nominal(id)?.name == "DecisionCell")
        {
            return Err(BackendFailure::InvalidIr);
        }
        let IrType::Nominal(state) = ty else {
            return Err(BackendFailure::InvalidIr);
        };
        // The runtime answers the state's ordinal as an i32; the enum's own tag
        // may be narrower (target::enum_tag_width), so the answer is truncated.
        let tag = super::enum_tag_type(self.nominal(state)?)?;
        let entry = match winner {
            None => "wf__decision_cell_read",
            Some(1) => "wf__claim_written",
            Some(2) => "wf__claim_killed",
            _ => return Err(BackendFailure::InvalidIr),
        };
        self.names(&[entry]);
        let result = self.value_name(result);
        let object = self.value_name(object);
        if tag == "i32" {
            writeln!(self.output, "  {result} = call i32 @{entry}(ptr {object})")
        } else {
            writeln!(
                self.output,
                "  {result}.answer = call i32 @{entry}(ptr {object})\n  {result} = trunc i32 {result}.answer to {tag}"
            )
        }
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// An acquire or a watch: the runtime answers 0 when this context holds
    /// the object at once and 1 when it has parked the frame, which then
    /// suspends until the runtime makes the context ready. A resumed acquire
    /// asks again, since an unlock usually wakes a parked statement to try
    /// rather than handing it the object, and the runtime answers 0 at once
    /// when it did hand it over; a resumed watch continues, and the lowering
    /// acquires again after it.
    pub(super) fn emit_shared_wait(
        &mut self,
        result: IrValueId,
        object: IrValueId,
        entry: &'static str,
        prefix: &str,
        extra: &str,
    ) -> Result<(), BackendFailure> {
        if !matches!(self.value_type(object), Some(IrType::Nominal(nominal))
            if matches!(self.nominal(nominal)?.kind(), IrNominalKind::Shared { shape: IrShared::Object, .. }))
        {
            return Err(BackendFailure::InvalidIr);
        }
        let retries = entry == "wf__shared_acquire";
        let arguments = format!("ptr {}, i32 1, ptr {HANDLE}{extra}", self.value_name(object));
        self.emit_parking_call(result, entry, &arguments, prefix, retries)
    }

    /// A call of `entry` that answers 1 when it has parked the frame, which
    /// then suspends, and 0 otherwise; the resumed frame calls again when
    /// `retries`, and otherwise continues.
    fn emit_parking_call(
        &mut self,
        result: IrValueId,
        entry: &'static str,
        arguments: &str,
        prefix: &str,
        retries: bool,
    ) -> Result<(), BackendFailure> {
        let prefix = labels(prefix, result);
        self.names(&[entry, "llvm.coro.save"]);
        let snapshot = snapshot::required(self.program);
        let before = if snapshot && retries {
            self.names(&["wf_snapshot_atomic_begin"]);
            "  call void @wf_snapshot_atomic_begin()\n"
        } else {
            ""
        };
        let after = if snapshot && !retries {
            // The native watch has released its single object before it
            // returns either answer. Activation therefore occurs outside
            // every hold and before a parked frame can move executors.
            self.names(&["wf_snapshot_atomic_end"]);
            "  call void @wf_snapshot_atomic_end()\n"
        } else {
            ""
        };
        writeln!(
            self.output,
            "  br label %{prefix}.try\n\
             {prefix}.try:\n{before}  \
             %{prefix}.saved = call token @llvm.coro.save(ptr null)\n  \
             %{prefix}.parked = call i32 @{entry}({arguments})\n  \
             {after}%{prefix}.suspends = icmp ne i32 %{prefix}.parked, 0\n  \
             br i1 %{prefix}.suspends, label %{prefix}.suspend, label %{prefix}.done\n\
             {prefix}.suspend:"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        if snapshot && retries {
            // This attempt acquired nothing. Restore this executor's mask
            // before suspension; the retry arms whichever executor resumes.
            self.names(&["wf_snapshot_atomic_end"]);
            writeln!(self.output, "  call void @wf_snapshot_atomic_end()")?;
        }
        let resumed = if retries {
            format!("{prefix}.try")
        } else {
            format!("{prefix}.done")
        };
        self.emit_suspension(&format!("%{prefix}.saved"), &prefix, &resumed)?;
        self.output.open_block(format!("{prefix}.done"));
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    /// [SHARE-1] `shared_close`'s count: the runtime counts one handle out
    /// with an acquire-release decrement and says whether it was the last.
    pub(super) fn emit_shared_count_out(
        &mut self,
        result: IrValueId,
        ty: IrType,
        object: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Bool || !self.is_object_handle(object)? {
            return Err(BackendFailure::InvalidIr);
        }
        let prefix = labels("close", result);
        self.names(&["wf__shared_release"]);
        writeln!(
            self.output,
            "  %{prefix}.count = call i32 @wf__shared_release(ptr {})\n  {} = icmp ne i32 %{prefix}.count, 0",
            self.value_name(object),
            self.value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// [PRE-2] reads a cancellation state's flag without holding the state.
    pub(super) fn emit_cancel_flag(
        &mut self,
        result: IrValueId,
        ty: IrType,
        object: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Bool || !self.is_object_handle(object)? {
            return Err(BackendFailure::InvalidIr);
        }
        let prefix = labels("flag", result);
        self.names(&["wf__cancel_flag"]);
        writeln!(
            self.output,
            "  %{prefix}.byte = call i8 @wf__cancel_flag(ptr {})\n  {} = icmp ne i8 %{prefix}.byte, 0",
            self.value_name(object),
            self.value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// [SHARE-1] returns an object whose last handle was counted out, its
    /// state already moved out, to the runtime.
    pub(super) fn emit_shared_free(
        &mut self,
        result: IrValueId,
        object: IrValueId,
    ) -> Result<(), BackendFailure> {
        if !self.is_object_handle(object)? {
            return Err(BackendFailure::InvalidIr);
        }
        self.names(&["wf__shared_free"]);
        writeln!(
            self.output,
            "  call void @wf__shared_free(ptr {})",
            self.value_name(object)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    /// Whether `object` is a handle to a shared object, not to a table.
    fn is_object_handle(&self, object: IrValueId) -> Result<bool, BackendFailure> {
        let Some(IrType::Nominal(nominal)) = self.value_type(object) else {
            return Ok(false);
        };
        Ok(matches!(
            self.nominal(nominal)?.kind(),
            IrNominalKind::Shared {
                shape: IrShared::Object,
                ..
            }
        ))
    }

    /// Takes the object for a statement already holding an entry of one of
    /// its state's tables, where the frame never suspends.
    pub(super) fn emit_shared_take(
        &mut self,
        result: IrValueId,
        object: IrValueId,
    ) -> Result<(), BackendFailure> {
        if !matches!(self.value_type(object), Some(IrType::Nominal(nominal))
            if matches!(self.nominal(nominal)?.kind(), IrNominalKind::Shared { shape: IrShared::Object, .. }))
        {
            return Err(BackendFailure::InvalidIr);
        }
        self.names(&["wf__shared_take"]);
        writeln!(
            self.output,
            "  call void @wf__shared_take(ptr {}, i32 1)",
            self.value_name(object)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    /// A guard's watch of its one object, registering the guard's wake test
    /// when it has one ([`super::wake`]); the runtime evaluates it on the
    /// state each later write leaves.
    pub(super) fn emit_shared_watch(
        &mut self,
        result: IrValueId,
        object: IrValueId,
        test: Option<&IrWakeTest>,
    ) -> Result<(), BackendFailure> {
        let test = self.emit_wake_test(result, test)?;
        let site = self.wait_site_operand(result);
        self.emit_shared_wait(
            result,
            object,
            "wf__shared_watch",
            "watch",
            &format!(", {test}, ptr {site}"),
        )
    }

    /// Watches one object in a guard's watch of several units, with the
    /// guard's wake test for that object when it has one.
    pub(super) fn emit_watch_object(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        object: IrValueId,
        test: Option<&IrWakeTest>,
    ) -> Result<(), BackendFailure> {
        if record.kind() != IrRecordKind::Watch {
            return Err(BackendFailure::InvalidIr);
        }
        let test = self.emit_wake_test(result, test)?;
        let arguments = format!(
            "ptr {}, ptr {}, {test}",
            record_name(record),
            self.value_name(object)
        );
        self.emit_unit_call(result, "wf__watch_object", &arguments)
    }

    /// [PRE-2] joins the watch `record` to the flag watchers of the
    /// cancellation state `object` names, with the value its guard read.
    pub(super) fn emit_watch_cancel(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        object: IrValueId,
        seen: IrValueId,
    ) -> Result<(), BackendFailure> {
        if record.kind() != IrRecordKind::Watch
            || !self.is_object_handle(object)?
            || self.value_type(seen) != Some(IrType::Bool)
        {
            return Err(BackendFailure::InvalidIr);
        }
        let prefix = labels("seen", result);
        writeln!(
            self.output,
            "  %{prefix} = zext i1 {} to i32",
            self.value_name(seen)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let arguments = format!(
            "ptr {}, ptr {}, i32 %{prefix}",
            record_name(record),
            self.value_name(object)
        );
        self.emit_unit_call(result, "wf__watch_cancel", &arguments)
    }

    /// Gives up this context's hold on the object; a statement that wrote no
    /// part of its state releases it unwritten, which wakes no watch
    /// [SHARE-3].
    pub(super) fn emit_shared_unlock(
        &mut self,
        result: IrValueId,
        object: IrValueId,
        wrote: bool,
    ) -> Result<(), BackendFailure> {
        let entry = if wrote {
            "wf__shared_unlock"
        } else {
            "wf__shared_unlock_unwritten"
        };
        self.names(&[entry]);
        writeln!(
            self.output,
            "  call void @{entry}(ptr {}, i32 1)",
            self.value_name(object)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }
}
