//! Shared objects [SHARE-1] and atomic statements [SHARE-2, SHARE-3].
//!
//! A handle is one pointer to the object, whose runtime header in the
//! completion bridge precedes its state at [`SHARED_STATE_OFFSET`]. An atomic
//! statement acquires the object for writing, which may suspend its frame
//! exactly as a join does; a guard that reads false watches the object, which
//! always suspends, and the lowering acquires again when the frame resumes.
//!
//! [`SHARED_STATE_OFFSET`]: crate::backend::SHARED_STATE_OFFSET

use std::fmt::Write;

use super::frames::{HANDLE, labels};
use super::*;

impl FunctionEmitter<'_, '_> {
    /// The state type of a shared-object nominal.
    fn shared_state(&self, nominal: IrNominalId) -> Result<IrType, BackendFailure> {
        match self.nominal(nominal)?.kind() {
            IrNominalKind::Shared { state } => Ok(*state),
            _ => Err(BackendFailure::InvalidIr),
        }
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
    /// suspends until the runtime makes the context ready.
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
        let prefix = labels(prefix, result);
        self.names(&[entry, "llvm.coro.save"]);
        writeln!(
            self.output,
            "  %{prefix}.saved = call token @llvm.coro.save(ptr null)\n  \
             %{prefix}.parked = call i32 @{entry}(ptr {}, i32 1, ptr {HANDLE})\n  \
             %{prefix}.suspends = icmp ne i32 %{prefix}.parked, 0\n  \
             br i1 %{prefix}.suspends, label %{prefix}.suspend, label %{prefix}.done\n\
             {prefix}.suspend:",
            self.value_name(object)
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
