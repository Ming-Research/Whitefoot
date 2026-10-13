//! Shards keep their immutable header separate from each ticket lock and value.
//! The lock fast paths and same-type identity ordering are emitted inline;
//! acquisition first counts the statement's waiting boundary. After that
//! boundary, only a contended ticket calls the runtime, without suspending.

use super::shared::{record_name, record_storage};
use super::*;
use crate::{IrElement, IrRecord, IrRecordKind};

impl FunctionEmitter<'_, '_> {
    fn shards_object(&self, object: IrValueId, element: IrElement) -> Result<(), BackendFailure> {
        let Some(IrType::Nominal(nominal)) = self.value_type(object) else {
            return Err(BackendFailure::InvalidIr);
        };
        if self.shared_state(nominal)? != (IrType::Shards { element }) {
            return Err(BackendFailure::InvalidIr);
        }
        Ok(())
    }

    pub(super) fn emit_shards_new(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal) {
            return Err(BackendFailure::InvalidIr);
        }
        let IrType::Shards { element } = self.shared_state(nominal)? else {
            return Err(BackendFailure::InvalidIr);
        };
        let element = self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        let element_type = self.output.type_name(self.program, element)?;
        self.names(&["wf__shards_new"]);
        writeln!(self.output,
            "  {} = call ptr @wf__shards_new(i64 ptrtoint (ptr getelementptr ({element_type}, ptr null, i64 1) to i64))",
            self.value_name(result))?;
        Ok(())
    }

    /// The object's stored stride belongs to its allocation, including zero-sized T.
    fn shards_element_pointer(
        &mut self,
        object: &str,
        index: &str,
    ) -> Result<String, BackendFailure> {
        let name = self.next_temporary()?;
        writeln!(self.output,
            "  %{name}.stride.ptr = getelementptr inbounds i8, ptr {object}, i64 24\n  %{name}.stride = load i64, ptr %{name}.stride.ptr, align 8\n  %{name}.offset = mul i64 {index}, %{name}.stride\n  %{name}.value.offset = add i64 %{name}.offset, 80\n  %{name} = getelementptr inbounds i8, ptr {object}, i64 %{name}.value.offset")?;
        Ok(format!("%{name}"))
    }

    /// A whole-state reference names its one measure word, eight bytes into
    /// the allocation; the array itself begins after the isolated header.
    pub(super) fn shards_state_element_pointer(
        &mut self,
        state: &str,
        index: &str,
    ) -> Result<String, BackendFailure> {
        let object = self.next_temporary()?;
        writeln!(self.output, "  %{object} = getelementptr inbounds i8, ptr {state}, i64 -8")?;
        self.shards_element_pointer(&format!("%{object}"), index)
    }

    /// The thread's user number is the scheduler's existing map-user number.
    /// Reading TLS cannot call into the scheduler or change drivers.
    fn shards_selected_index(&mut self, object: &str) -> Result<String, BackendFailure> {
        let name = self.next_temporary()?;
        self.names(&["wf__map_user"]);
        writeln!(self.output,
            "  %{name}.len.ptr = getelementptr inbounds i8, ptr {object}, i64 8\n  %{name}.len = load i64, ptr %{name}.len.ptr, align 8\n  %{name}.user = load i32, ptr @wf__map_user, align 4\n  %{name}.wide = zext i32 %{name}.user to i64\n  %{name} = urem i64 %{name}.wide, %{name}.len")?;
        Ok(format!("%{name}"))
    }

    pub(super) fn emit_shards_select(
        &mut self,
        result: IrValueId,
        ty: IrType,
        object: IrValueId,
        element: IrElement,
    ) -> Result<(), BackendFailure> {
        self.shards_object(object, element)?;
        if self
            .program
            .element(element)
            .and_then(IrAddressed::of)
            .map(IrType::Address)
            != Some(ty)
        {
            return Err(BackendFailure::InvalidIr);
        }
        let object = self.value_name(object);
        let index = self.shards_selected_index(&object)?;
        let pointer = self.shards_element_pointer(&object, &index)?;
        writeln!(
            self.output,
            "  {} = getelementptr i8, ptr {pointer}, i64 0",
            self.value_name(result)
        )?;
        Ok(())
    }

    fn shards_lock_pointer(&mut self, object: &str, index: &str) -> Result<String, BackendFailure> {
        let name = self.next_temporary()?;
        writeln!(self.output,
            "  %{name}.stride.ptr = getelementptr inbounds i8, ptr {object}, i64 24\n  %{name}.stride = load i64, ptr %{name}.stride.ptr, align 8\n  %{name}.offset = mul i64 {index}, %{name}.stride\n  %{name}.lock.offset = add i64 %{name}.offset, 64\n  %{name} = getelementptr inbounds i8, ptr {object}, i64 %{name}.lock.offset")?;
        Ok(format!("%{name}"))
    }

    fn shards_ticket(&mut self, lock: &str, take: bool) -> Result<(), BackendFailure> {
        let name = self.next_temporary()?;
        writeln!(self.output, "  %{name}.serving.ptr = getelementptr inbounds i8, ptr {lock}, i64 8")?;
        if take {
            self.names(&["wf__shards_wait"]);
            writeln!(self.output,
                "  %{name}.ticket = atomicrmw add ptr {lock}, i64 1 monotonic, align 8\n  %{name}.serving = load atomic i64, ptr %{name}.serving.ptr acquire, align 8\n  %{name}.ready = icmp eq i64 %{name}.serving, %{name}.ticket\n  br i1 %{name}.ready, label %{name}.held, label %{name}.wait")?;
            self.output.open_block(format!("{name}.wait"));
            writeln!(self.output, "  call void @wf__shards_wait(ptr {lock}, i64 %{name}.ticket)\n  br label %{name}.held")?;
            self.output.open_block(format!("{name}.held"));
        } else {
            // Only this holder advances serving; the next ticket's acquire
            // observes all of its writes through this release store.
            writeln!(self.output,
                "  %{name}.serving = load atomic i64, ptr %{name}.serving.ptr monotonic, align 8\n  %{name}.next = add i64 %{name}.serving, 1\n  store atomic i64 %{name}.next, ptr %{name}.serving.ptr release, align 8")?;
        }
        Ok(())
    }

    fn shards_hold_object(
        &mut self,
        object: &str,
        whole: &str,
        take: bool,
    ) -> Result<(), BackendFailure> {
        let name = self.next_temporary()?;
        writeln!(self.output, "  br i1 {whole}, label %{name}.whole, label %{name}.selected")?;
        self.output.open_block(format!("{name}.selected"));
        let index = self.shards_selected_index(object)?;
        let lock = self.shards_lock_pointer(object, &index)?;
        self.shards_ticket(&lock, take)?;
        writeln!(self.output, "  br label %{name}.done")?;
        self.output.open_block(format!("{name}.whole"));
        writeln!(self.output,
            "  %{name}.len.ptr = getelementptr inbounds i8, ptr {object}, i64 8\n  %{name}.len = load i64, ptr %{name}.len.ptr, align 8\n  br label %{name}.walk")?;
        self.output.open_block(format!("{name}.walk"));
        writeln!(self.output,
            "  %{name}.index = phi i64 [ 0, %{name}.whole ], [ %{name}.next, %{name}.step ]\n  %{name}.more = icmp ult i64 %{name}.index, %{name}.len\n  br i1 %{name}.more, label %{name}.body, label %{name}.done")?;
        self.output.open_block(format!("{name}.body"));
        let lock = self.shards_lock_pointer(object, &format!("%{name}.index"))?;
        self.shards_ticket(&lock, take)?;
        writeln!(self.output, "  br label %{name}.step")?;
        self.output.open_block(format!("{name}.step"));
        writeln!(self.output, "  %{name}.next = add i64 %{name}.index, 1\n  br label %{name}.walk")?;
        self.output.open_block(format!("{name}.done"));
        Ok(())
    }

    pub(super) fn emit_shards_hold(
        &mut self,
        result: IrValueId,
        object: IrValueId,
        element: IrElement,
        whole: bool,
        take: bool,
    ) -> Result<(), BackendFailure> {
        self.shards_object(object, element)?;
        self.shards_hold_object(
            &self.value_name(object),
            if whole { "true" } else { "false" },
            take,
        )?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    pub(super) fn emit_shards_group_target(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        index: u32,
        object: IrValueId,
        whole: bool,
    ) -> Result<(), BackendFailure> {
        let IrRecordKind::ShardsGroup { count } = record.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        if index >= count {
            return Err(BackendFailure::InvalidIr);
        }
        let name = self.next_temporary()?;
        let storage = record_storage(record.kind());
        let record = record_name(record);
        writeln!(self.output,
            "  %{name}.object = getelementptr inbounds {storage}, ptr {record}, i64 0, i32 {index}, i32 0\n  store ptr {}, ptr %{name}.object, align 8\n  %{name}.whole = getelementptr inbounds {storage}, ptr {record}, i64 0, i32 {index}, i32 1\n  store i64 {}, ptr %{name}.whole, align 8",
            self.value_name(object), u8::from(whole))?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    fn shards_group_member(
        &mut self,
        record: IrRecord,
        index: u32,
    ) -> Result<(String, String, String, String), BackendFailure> {
        let name = self.next_temporary()?;
        let storage = record_storage(record.kind());
        let record = record_name(record);
        writeln!(self.output,
            "  %{name}.object.ptr = getelementptr inbounds {storage}, ptr {record}, i64 0, i32 {index}, i32 0\n  %{name}.object = load ptr, ptr %{name}.object.ptr, align 8\n  %{name}.whole.ptr = getelementptr inbounds {storage}, ptr {record}, i64 0, i32 {index}, i32 1\n  %{name}.whole = load i64, ptr %{name}.whole.ptr, align 8")?;
        Ok((
            format!("%{name}.object"),
            format!("%{name}.whole"),
            format!("%{name}.object.ptr"),
            format!("%{name}.whole.ptr"),
        ))
    }

    pub(super) fn emit_shards_group(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        element: IrElement,
        take: bool,
    ) -> Result<(), BackendFailure> {
        let IrRecordKind::ShardsGroup { count } = record.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        self.program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        if take {
            // The static adjacent-comparison network is insertion sort. It
            // moves only this statement's private descriptors, never a header.
            for end in 1..count {
                for right in (1..=end).rev() {
                    let (a, aw, ap, awp) = self.shards_group_member(record, right - 1)?;
                    let (b, bw, bp, bwp) = self.shards_group_member(record, right)?;
                    let name = self.next_temporary()?;
                    writeln!(self.output,
                        "  %{name}.ai.ptr = getelementptr inbounds i8, ptr {a}, i64 16\n  %{name}.ai = load i64, ptr %{name}.ai.ptr, align 8\n  %{name}.bi.ptr = getelementptr inbounds i8, ptr {b}, i64 16\n  %{name}.bi = load i64, ptr %{name}.bi.ptr, align 8\n  %{name}.swap = icmp ugt i64 %{name}.ai, %{name}.bi\n  %{name}.first = select i1 %{name}.swap, ptr {b}, ptr {a}\n  %{name}.second = select i1 %{name}.swap, ptr {a}, ptr {b}\n  %{name}.first.whole = select i1 %{name}.swap, i64 {bw}, i64 {aw}\n  %{name}.second.whole = select i1 %{name}.swap, i64 {aw}, i64 {bw}\n  store ptr %{name}.first, ptr {ap}, align 8\n  store ptr %{name}.second, ptr {bp}, align 8\n  store i64 %{name}.first.whole, ptr {awp}, align 8\n  store i64 %{name}.second.whole, ptr {bwp}, align 8")?;
                }
            }
        }
        let mut previous = None;
        for index in 0..count {
            let (object, mut whole, _, _) = self.shards_group_member(record, index)?;
            let name = self.next_temporary()?;
            if let Some(previous) = previous {
                writeln!(self.output, "  %{name}.same = icmp eq ptr {object}, {previous}\n  br i1 %{name}.same, label %{name}.done, label %{name}.hold")?;
            } else {
                writeln!(self.output, "  br label %{name}.hold")?;
            }
            self.output.open_block(format!("{name}.hold"));
            // All aliases are adjacent after sorting. OR across them so a
            // whole target absorbs every selected target of the same object.
            for other in index + 1..count {
                let (candidate, candidate_whole, _, _) = self.shards_group_member(record, other)?;
                let merge = self.next_temporary()?;
                writeln!(self.output,
                    "  %{merge}.same = icmp eq ptr {object}, {candidate}\n  %{merge}.candidate = select i1 %{merge}.same, i64 {candidate_whole}, i64 0\n  %{merge} = or i64 {whole}, %{merge}.candidate")?;
                whole = format!("%{merge}");
            }
            writeln!(self.output, "  %{name}.whole = icmp ne i64 {whole}, 0")?;
            self.shards_hold_object(&object, &format!("%{name}.whole"), take)?;
            writeln!(self.output, "  br label %{name}.done")?;
            self.output.open_block(format!("{name}.done"));
            previous = Some(object);
        }
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }
}
