//! Shared objects [SHARE-1] and atomic statements [SHARE-2, SHARE-3].
//!
//! A handle is one pointer to the object, whose runtime header in the
//! completion bridge precedes its state at [`SHARED_STATE_OFFSET`]. An atomic
//! statement acquires the object for writing, which may suspend its frame
//! exactly as a join does and asks again when the frame resumes; a guard that
//! reads false watches the object, which always suspends, and the lowering
//! acquires again when the frame resumes.
//!
//! [`SHARED_STATE_OFFSET`]: crate::backend::SHARED_STATE_OFFSET

use std::fmt::Write;

use super::frames::{HANDLE, labels};
use super::*;
use crate::IrShared;

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

    /// A map nominal's entry type, the `Option<V>` its nodes keep a slot of.
    fn shared_map_entry(&self, nominal: IrNominalId) -> Result<IrType, BackendFailure> {
        match self.nominal(nominal)?.kind() {
            IrNominalKind::Shared {
                shape: IrShared::Map { entry } | IrShared::State { entry },
                ..
            } => Ok(*entry),
            _ => Err(BackendFailure::InvalidIr),
        }
    }

    /// Whether a value is a map's handle or the address of a map's state.
    fn names_map(&self, value: IrValueId) -> Result<bool, BackendFailure> {
        let nominal = match self.value_type(value) {
            Some(IrType::Nominal(nominal))
            | Some(IrType::Address(IrAddressed::Nominal(nominal))) => nominal,
            _ => return Ok(false),
        };
        Ok(matches!(
            self.nominal(nominal)?.kind(),
            IrNominalKind::Shared {
                shape: IrShared::Map { .. } | IrShared::State { .. },
                ..
            }
        ))
    }

    /// [SHARE-1] a new map whose nodes keep a slot of its entry type, of
    /// that type's size and alignment, holding one handle.
    pub(super) fn emit_shared_map_new(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        capacity: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal) {
            return Err(BackendFailure::InvalidIr);
        }
        let entry = self.shared_map_entry(nominal)?;
        let entry_type = self.output.type_name(self.program, entry)?;
        self.names(&["wf__shared_map_new"]);
        writeln!(
            self.output,
            "  {} = call ptr @wf__shared_map_new(i64 ptrtoint (ptr getelementptr ({entry_type}, ptr null, i64 1) to i64), i64 ptrtoint (ptr getelementptr ({{ i8, {entry_type} }}, ptr null, i64 0, i32 1) to i64), i64 {})",
            self.value_name(result),
            self.value_name(capacity)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// The address of a map's state: the map itself.
    pub(super) fn emit_shared_map_state(
        &mut self,
        result: IrValueId,
        object: IrValueId,
    ) -> Result<(), BackendFailure> {
        if !self.names_map(object)? {
            return Err(BackendFailure::InvalidIr);
        }
        writeln!(
            self.output,
            "  {} = getelementptr i8, ptr {}, i64 0",
            self.value_name(result),
            self.value_name(object)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// A call of one of the runtime's map entries that takes the map and
    /// answers nothing: a hold, an unhold.
    pub(super) fn emit_shared_map_call(
        &mut self,
        result: IrValueId,
        object: IrValueId,
        entry: &'static str,
    ) -> Result<(), BackendFailure> {
        if !self.names_map(object)? {
            return Err(BackendFailure::InvalidIr);
        }
        self.names(&[entry]);
        writeln!(
            self.output,
            "  call void @{entry}(ptr {})",
            self.value_name(object)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    /// Locks the entry under the key's bytes and defines its slot's address;
    /// with `reads`, holds it beside the other statements that only read it.
    pub(super) fn emit_shared_map_lock(
        &mut self,
        result: IrValueId,
        object: IrValueId,
        key: IrValueId,
        held: bool,
        reads: bool,
    ) -> Result<(), BackendFailure> {
        if !self.names_map(object)? {
            return Err(BackendFailure::InvalidIr);
        }
        let key_type = self.output.type_name(
            self.program,
            self.value_type(key).ok_or(BackendFailure::InvalidIr)?,
        )?;
        let name = self.value_name(result);
        let bare = name.trim_start_matches('%');
        let key_name = self.value_name(key);
        let object_name = self.value_name(object);
        writeln!(
            self.output,
            "  %{bare}.key = extractvalue {key_type} {key_name}, 0\n  %{bare}.length = extractvalue {key_type} {key_name}, 1",
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        if reads {
            if held {
                return Err(BackendFailure::InvalidIr);
            }
            self.names(&["wf__shared_map_read"]);
            writeln!(
                self.output,
                "  {name} = call ptr @wf__shared_map_read(ptr {object_name}, ptr %{bare}.key, i64 %{bare}.length)",
            )
        } else {
            self.names(&["wf__shared_map_lock"]);
            writeln!(
                self.output,
                "  {name} = call ptr @wf__shared_map_lock(ptr {object_name}, ptr %{bare}.key, i64 %{bare}.length, i32 {held})",
                held = u32::from(held),
            )
        }
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Unlocks the entry whose slot `entry` addresses, telling the runtime
    /// whether the slot holds `Some`: its tag, the first `i32` of every enum
    /// with a payload, differs from `None`'s, which is 0, the tag of a slot
    /// the runtime filled with zeros. With `reads`, ends the statement's read
    /// of the entry, which it left as it was.
    pub(super) fn emit_shared_map_unlock(
        &mut self,
        result: IrValueId,
        object: IrValueId,
        entry: IrValueId,
        held: bool,
        reads: bool,
    ) -> Result<(), BackendFailure> {
        if !self.names_map(object)? {
            return Err(BackendFailure::InvalidIr);
        }
        if reads {
            if held {
                return Err(BackendFailure::InvalidIr);
            }
            self.names(&["wf__shared_map_unread"]);
            writeln!(
                self.output,
                "  call void @wf__shared_map_unread(ptr {object})",
                object = self.value_name(object),
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            return self.emit_constant(result, IrType::Unit, IrConstant::Unit);
        }
        let Some(IrType::Address(IrAddressed::Nominal(option))) = self.value_type(entry) else {
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
        let name = self.value_name(result);
        let bare = name.trim_start_matches('%').to_owned();
        self.names(&["wf__shared_map_unlock"]);
        writeln!(
            self.output,
            "  %{bare}.tag = load i32, ptr {entry}\n  %{bare}.some = icmp ne i32 %{bare}.tag, 0\n  %{bare}.present = zext i1 %{bare}.some to i32\n  call void @wf__shared_map_unlock(ptr {object}, i32 {held}, i32 %{bare}.present)",
            entry = self.value_name(entry),
            object = self.value_name(object),
            held = u32::from(held),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    /// How many entries of the state `state` addresses hold `Some`.
    pub(super) fn emit_shared_map_count(
        &mut self,
        result: IrValueId,
        state: IrValueId,
    ) -> Result<(), BackendFailure> {
        if !self.names_map(state)? {
            return Err(BackendFailure::InvalidIr);
        }
        self.names(&["wf__shared_map_count"]);
        writeln!(
            self.output,
            "  {} = call i64 @wf__shared_map_count(ptr {})",
            self.value_name(result),
            self.value_name(state)
        )
        .map_err(|_| BackendFailure::TextEmission)
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
        let entry = if self.names_map(object)? {
            "wf__shared_map_share"
        } else {
            "wf__shared_share"
        };
        self.names(&[entry]);
        writeln!(
            self.output,
            "  call void @{entry}(ptr {object})\n  {} = getelementptr i8, ptr {object}, i64 0",
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
            if matches!(self.nominal(nominal)?.kind(), IrNominalKind::Shared { .. }))
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

    /// Takes the object inside a map's or an entry's block, where the frame
    /// never suspends.
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
