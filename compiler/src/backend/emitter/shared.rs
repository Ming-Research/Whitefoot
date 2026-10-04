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
use crate::{IrRecord, IrRecordKind, IrShared};

/// The frame storage of one record, by its kind.
fn record_storage(kind: IrRecordKind) -> String {
    match kind {
        IrRecordKind::TableEntry => format!("[{} x i8]", crate::backend::TABLE_ENTRY_SIZE),
        IrRecordKind::TableHold => format!("[{} x i8]", crate::backend::TABLE_HOLD_SIZE),
        IrRecordKind::KeyedEntries => "{ ptr, i64, i64 }".to_owned(),
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
/// keep, and the scratch place of each new key set, once per call.
pub(super) fn record_prelude(function: &IrFunction) -> String {
    let mut records = BTreeMap::new();
    let mut scratch = Vec::new();
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
        let record = match operation {
            IrOperation::TableLockEntry { record, .. }
            | IrOperation::TableEntrySlot { record, .. }
            | IrOperation::TableUnlockEntry { record, .. }
            | IrOperation::TableHoldBegin { record, .. }
            | IrOperation::TableHoldKey { record, .. }
            | IrOperation::TableHoldKeys { record, .. }
            | IrOperation::TableHoldWhole { record }
            | IrOperation::TableHoldTake { record }
            | IrOperation::TableHoldSlot { record, .. }
            | IrOperation::TableHoldRelease { record, .. }
            | IrOperation::TableHeldEntries { record, .. }
            | IrOperation::KeyedEntriesRecord { record, .. }
            | IrOperation::WatchBegin { record }
            | IrOperation::WatchObject { record, .. }
            | IrOperation::WatchTable { record, .. }
            | IrOperation::WatchPark { record } => *record,
            IrOperation::KeySetNew { .. } => {
                scratch.push(*result);
                continue;
            }
            _ => continue,
        };
        records.insert(record.index(), record);
    }
    let mut prelude = String::new();
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
    prelude
}

impl FunctionEmitter<'_, '_> {
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
                shape: IrShared::Table { entry },
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
                shape: IrShared::Table { .. },
                ..
            }
        ))
    }

    /// A table's entry type, refused unless the runtime reads whether an
    /// entry holds `Some` as it does: from the tag, the first `i32` of every
    /// enum with a payload, which differs from `None`'s, 0, the tag of a slot
    /// the runtime filled with zeros.
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
    fn emit_unit_call(
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
        self.names(&["wf__keyed_table_new"]);
        writeln!(
            self.output,
            "  {} = call ptr @wf__keyed_table_new(i64 ptrtoint (ptr getelementptr ({entry_type}, ptr null, i64 1) to i64), i64 ptrtoint (ptr getelementptr ({{ i8, {entry_type} }}, ptr null, i64 0, i32 1) to i64), i64 {})",
            self.value_name(result),
            self.value_name(capacity)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// How many entries of the table hold `Some`; under the statement's
    /// whole hold the runtime reads its entries' tags, an `i32` at offset 0
    /// whose `None` is 0, so the count sees the statement's own writes.
    pub(super) fn emit_keyed_table_count(
        &mut self,
        result: IrValueId,
        table: IrValueId,
    ) -> Result<(), BackendFailure> {
        let Some(IrType::Nominal(nominal)) = self.value_type(table) else {
            return Err(BackendFailure::InvalidIr);
        };
        self.checked_entry(nominal)?;
        self.names(&["wf__keyed_table_count"]);
        writeln!(
            self.output,
            "  {} = call i64 @wf__keyed_table_count(ptr {}, i64 0, i32 4, i64 0)",
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
        self.checked_entry(nominal)?;
        let arguments = format!(
            "ptr {}, ptr {}, i64 0, i32 4, i64 0",
            self.value_name(first),
            self.value_name(second)
        );
        self.emit_unit_call(result, "wf__keyed_table_swap", &arguments)
    }

    pub(super) fn emit_table_held_entry(
        &mut self,
        result: IrValueId,
        nominal: IrNominalId,
        table: IrValueId,
        key: IrValueId,
        write: bool,
    ) -> Result<(), BackendFailure> {
        self.checked_entry(nominal)?;
        let bare = self.bare(result);
        self.key_parts(&bare, key)?;
        self.names(&["wf__table_held_entry"]);
        writeln!(self.output, "  {} = call ptr @wf__table_held_entry(ptr {}, ptr %{bare}.key, i64 %{bare}.length, i32 {})", self.value_name(result), self.value_name(table), u32::from(write)).map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_table_held_entries(
        &mut self,
        result: IrValueId,
        table: IrValueId,
        set: IrValueId,
        record: IrRecord,
    ) -> Result<(), BackendFailure> {
        self.names(&["wf__table_held_entries"]);
        writeln!(self.output, "  call void @wf__table_held_entries(ptr {}, ptr {}, ptr {})\n  {} = getelementptr i8, ptr {}, i64 0", self.value_name(table), self.value_name(set), record_name(record), self.value_name(result), record_name(record)).map_err(|_| BackendFailure::TextEmission)
    }

    /// Locks one key's entry, keeping the lock in its record and the slot's
    /// address in the record's word.
    pub(super) fn emit_table_lock_entry(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        table: IrValueId,
        key: IrValueId,
        read: bool,
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
            read = u32::from(read),
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
        self.checked_entry(nominal)?;
        if record.kind() != IrRecordKind::TableEntry {
            return Err(BackendFailure::InvalidIr);
        }
        let bare = self.bare(result);
        self.names(&["wf__table_unlock_entry"]);
        writeln!(
            self.output,
            "  %{bare}.slot = load ptr, ptr {word}\n  %{bare}.tag = load i32, ptr %{bare}.slot\n  %{bare}.some = icmp ne i32 %{bare}.tag, 0\n  %{bare}.present = zext i1 %{bare}.some to i32\n  call void @wf__table_unlock_entry(ptr {record}, i32 %{bare}.present)",
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

    /// Adds one key to a hold and defines its position.
    pub(super) fn emit_table_hold_key(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        key: IrValueId,
    ) -> Result<(), BackendFailure> {
        if record.kind() != IrRecordKind::TableHold {
            return Err(BackendFailure::InvalidIr);
        }
        let bare = self.bare(result);
        self.key_parts(&bare, key)?;
        self.names(&["wf__table_hold_key"]);
        writeln!(
            self.output,
            "  {} = call i64 @wf__table_hold_key(ptr {}, ptr %{bare}.key, i64 %{bare}.length)",
            self.value_name(result),
            record_name(record),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Adds every key of a set to a hold and defines the first one's
    /// position.
    pub(super) fn emit_table_hold_keys(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        set: IrValueId,
    ) -> Result<(), BackendFailure> {
        if record.kind() != IrRecordKind::TableHold
            || self.value_type(set) != Some(IrType::Address(IrAddressed::KeySet))
        {
            return Err(BackendFailure::InvalidIr);
        }
        self.names(&["wf__table_hold_keys"]);
        writeln!(
            self.output,
            "  {} = call i64 @wf__table_hold_keys(ptr {}, ptr {})",
            self.value_name(result),
            record_name(record),
            self.value_name(set),
        )
        .map_err(|_| BackendFailure::TextEmission)
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

    /// The address of the entry added at a position of a taken hold.
    pub(super) fn emit_table_hold_slot(
        &mut self,
        result: IrValueId,
        nominal: IrNominalId,
        record: IrRecord,
        position: IrValueId,
    ) -> Result<(), BackendFailure> {
        self.checked_entry(nominal)?;
        if record.kind() != IrRecordKind::TableHold {
            return Err(BackendFailure::InvalidIr);
        }
        self.names(&["wf__table_hold_slot"]);
        writeln!(
            self.output,
            "  {} = call ptr @wf__table_hold_slot(ptr {}, i64 {})",
            self.value_name(result),
            record_name(record),
            self.value_name(position),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Gives up a hold: the runtime reads each entry's tag, an `i32` at
    /// offset 0 whose `None` is 0, to keep or remove it.
    pub(super) fn emit_table_hold_release(
        &mut self,
        result: IrValueId,
        nominal: IrNominalId,
        record: IrRecord,
    ) -> Result<(), BackendFailure> {
        self.checked_entry(nominal)?;
        if record.kind() != IrRecordKind::TableHold {
            return Err(BackendFailure::InvalidIr);
        }
        let arguments = format!("ptr {}, i64 0, i32 4, i64 0", record_name(record));
        self.emit_unit_call(result, "wf__table_hold_release", &arguments)
    }

    /// The address of the record of the entries an entry binding over a key
    /// set names.
    pub(super) fn emit_keyed_entries_record(
        &mut self,
        result: IrValueId,
        record: IrRecord,
    ) -> Result<(), BackendFailure> {
        if record.kind() != IrRecordKind::KeyedEntries {
            return Err(BackendFailure::InvalidIr);
        }
        writeln!(
            self.output,
            "  {} = getelementptr i8, ptr {}, i64 0",
            self.value_name(result),
            record_name(record)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Fills that record: the hold, the position of the set's first key in
    /// it, and the set's count.
    pub(super) fn emit_keyed_entries_fill(
        &mut self,
        result: IrValueId,
        entries: IrValueId,
        hold: IrRecord,
        position: IrValueId,
        set: IrValueId,
    ) -> Result<(), BackendFailure> {
        if !matches!(
            self.value_type(entries),
            Some(IrType::Address(IrAddressed::KeyedEntries { .. }))
        ) || self.value_type(set) != Some(IrType::Address(IrAddressed::KeySet))
            || hold.kind() != IrRecordKind::TableHold
        {
            return Err(BackendFailure::InvalidIr);
        }
        let bare = self.bare(result);
        writeln!(
            self.output,
            "  store ptr {hold}, ptr {entries}\n  %{bare}.position = getelementptr inbounds i8, ptr {entries}, i64 8\n  store i64 {position}, ptr %{bare}.position\n  %{bare}.length = load i64, ptr {set}\n  %{bare}.count = getelementptr inbounds i8, ptr {entries}, i64 16\n  store i64 %{bare}.length, ptr %{bare}.count",
            hold = record_name(hold),
            entries = self.value_name(entries),
            position = self.value_name(position),
            set = self.value_name(set),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
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
        self.names(&["wf__watch_park", "llvm.coro.save"]);
        writeln!(
            self.output,
            "  br label %{prefix}.try\n\
             {prefix}.try:\n  \
             %{prefix}.saved = call token @llvm.coro.save(ptr null)\n  \
             %{prefix}.parked = call i32 @wf__watch_park(ptr {}, ptr {HANDLE})\n  \
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
        if ty != IrType::Nominal(nominal) || self.value_type(object) != Some(ty) {
            return Err(BackendFailure::InvalidIr);
        }
        self.shared_state(nominal)?;
        self.names(&["wf__shared_share"]);
        writeln!(
            self.output,
            "  call void @wf__shared_share(ptr {object})\n  {} = getelementptr i8, ptr {object}, i64 0",
            self.value_name(result),
            object = self.value_name(object),
        )
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
    ) -> Result<(), BackendFailure> {
        if !matches!(self.value_type(object), Some(IrType::Nominal(nominal))
            if matches!(self.nominal(nominal)?.kind(), IrNominalKind::Shared { shape: IrShared::Object, .. }))
        {
            return Err(BackendFailure::InvalidIr);
        }
        let retries = entry == "wf__shared_acquire";
        let prefix = labels(prefix, result);
        self.names(&[entry, "llvm.coro.save"]);
        writeln!(
            self.output,
            "  br label %{prefix}.try\n\
             {prefix}.try:\n  \
             %{prefix}.saved = call token @llvm.coro.save(ptr null)\n  \
             %{prefix}.parked = call i32 @{entry}(ptr {}, i32 1, ptr {HANDLE})\n  \
             %{prefix}.suspends = icmp ne i32 %{prefix}.parked, 0\n  \
             br i1 %{prefix}.suspends, label %{prefix}.suspend, label %{prefix}.done\n\
             {prefix}.suspend:",
            self.value_name(object)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let resumed = if retries {
            format!("{prefix}.try")
        } else {
            format!("{prefix}.done")
        };
        self.emit_suspension(&format!("%{prefix}.saved"), &prefix, &resumed)?;
        self.output.open_block(format!("{prefix}.done"));
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
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

    /// Gives up this context's hold on the object.
    pub(super) fn emit_shared_unlock(
        &mut self,
        result: IrValueId,
        object: IrValueId,
    ) -> Result<(), BackendFailure> {
        self.names(&["wf__shared_unlock"]);
        writeln!(
            self.output,
            "  call void @wf__shared_unlock(ptr {}, i32 1)",
            self.value_name(object)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }
}
