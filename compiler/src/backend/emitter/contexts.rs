//! [PAR-4] lowering of a started context and of the join every activation
//! that starts one owes before it leaves.
//!
//! A `mustpar` statement whose callee waits reaches the target as
//! [`IrOperation::ContextStart`] over a synthesized wrapper that takes the
//! call's arguments by value, makes the call and drops its result. The
//! wrapper waits, so it is a resumable frame like every waiting function
//! ([`super::frames`]). Here the start becomes: make a context whose arena
//! holds a block of those arguments, store them, and launch it on a thunk
//! that reads them back and calls the wrapper's ramp inside the new context,
//! which makes the context's outermost frame. The starting activation keeps a
//! two-word group in its own frame: the runtime counts the activation's
//! unfinished contexts there, and [`IrOperation::ContextJoin`], which the
//! lowering places before every exit, suspends the activation until they
//! have finished.
//!
//! Nothing here can be refused. Every marked waiting call runs as a context
//! of its own (design/compiler/waiting-contexts.md), so there is
//! no fallback that runs the call inline: running it inline would wait for
//! it. The runtime symbols are the completion bridge's, which every program
//! links, and they begin `wf__` so no source name can reach them.
//!
//! [`IrOperation::ContextStart`]: crate::IrOperation::ContextStart
//! [`IrOperation::ContextJoin`]: crate::IrOperation::ContextJoin

use std::fmt::Write;

use super::parallel::{ThunkFrame, thunk_arguments};
use super::{BackendFailure, FunctionEmitter};
use crate::backend::abi::FunctionAbi;
use crate::backend::emission::{FunctionBody, Linkage, Module, Parameter, References, Signature};
use crate::{IrConstant, IrFunction, IrInstruction, IrOperation, IrType, IrValueId};

/// The group one starting activation keeps, named in its entry prelude.
pub(super) const GROUP: &str = "%wf.ctx.group";

/// Whether a function starts or joins contexts, and so keeps a group.
pub(super) fn keeps_context_group(function: &IrFunction) -> bool {
    function.blocks().iter().any(|block| {
        block.instructions().iter().any(|instruction| {
            matches!(
                instruction,
                IrInstruction::Define {
                    operation: IrOperation::ContextStart { .. } | IrOperation::ContextJoin,
                    ..
                }
            )
        })
    })
}

/// The entry-block line that reserves the group.
pub(super) fn context_group_prelude() -> String {
    format!("  {GROUP} = alloca [2 x i64], align 8\n")
}

/// The group's zeroing, which the frame's making emits once the frame exists,
/// so the zeroed words are the frame's own.
pub(super) fn context_group_initialization() -> String {
    format!("  store [2 x i64] zeroinitializer, ptr {GROUP}\n")
}

impl FunctionEmitter<'_, '_> {
    /// Starts one context over the wrapper `function` with `arguments`.
    pub(super) fn emit_context_start(
        &mut self,
        result: IrValueId,
        function: u32,
        arguments: &[IrValueId],
    ) -> Result<(), BackendFailure> {
        let target = self
            .program
            .functions()
            .get(function as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        if !target.waits() {
            return Err(BackendFailure::InvalidIr);
        }
        let abi = FunctionAbi::build(self.program, target)?;
        if abi.parameters().len() != arguments.len() {
            return Err(BackendFailure::InvalidIr);
        }
        let mut field_types = Vec::with_capacity(arguments.len() + 1);
        let mut frame_references = References::default();
        let mut operands = Vec::with_capacity(arguments.len());
        for (argument, parameter) in arguments.iter().zip(abi.parameters()) {
            if self.value_type(*argument) != Some(parameter.ty()) {
                return Err(BackendFailure::InvalidIr);
            }
            let parameter_type = super::llvm_type_with_references(
                self.program,
                parameter.ty(),
                &mut frame_references.types,
            )?;
            operands.push(self.frame_operand(&parameter_type, *argument)?);
            field_types.push(parameter_type);
        }
        let result_type = super::llvm_type_with_references(
            self.program,
            abi.result().ty(),
            &mut frame_references.types,
        )?;
        let result_field = field_types.len();
        field_types.push(result_type);
        let frame_type = format!("{{ {} }}", field_types.join(", "));
        // A wrapper is reached only through a start, never by a call inside a
        // budgeted component, so it has no budget-carrying variant.
        let (callee, budget) = self.callee_target(function, target.name());
        if budget.is_some() {
            return Err(BackendFailure::InvalidIr);
        }
        self.output.references.extend(&frame_references);
        let thunk = self
            .parallel
            .register_context(self.function.name(), |symbol| {
                context_thunk(
                    symbol,
                    &ThunkFrame {
                        ty: &frame_type,
                        field_types: &field_types,
                        result: result_field,
                        budget: None,
                        references: &frame_references,
                    },
                    &abi,
                    &callee,
                )
            })?;
        self.output.symbol(thunk.trim_start_matches('@'));
        self.output.symbol("wf__context_prepare");
        self.output.symbol("wf__context_launch");
        let end = format!("%{}", self.next_temporary()?);
        let bytes = format!("%{}", self.next_temporary()?);
        let frame = format!("%{}", self.next_temporary()?);
        writeln!(
            self.output,
            "  {end} = getelementptr {frame_type}, ptr null, i32 1\n  {bytes} = ptrtoint ptr {end} to i64\n  {frame} = call ptr @wf__context_prepare(i64 {bytes})"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        for (index, operand) in operands.iter().enumerate() {
            let field = format!("%{}", self.next_temporary()?);
            writeln!(
                self.output,
                "  {field} = getelementptr inbounds {frame_type}, ptr {frame}, i32 0, i32 {index}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            self.store_frame_operand(operand, &field)?;
        }
        writeln!(
            self.output,
            "  call void @wf__context_launch(ptr {GROUP}, ptr {frame}, ptr {thunk})"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    /// Waits for every context this activation started.
    pub(super) fn emit_context_join(&mut self, result: IrValueId) -> Result<(), BackendFailure> {
        self.emit_frame_join(result)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }
}

/// A started context's thunk: reads the call's arguments out of the block the
/// start stored them in and makes the wrapper's frame, whose parent is the
/// no-op coroutine, because a context's outermost frame transfers back to
/// the driver. The wrapper's unit result lands in the block's last field.
fn context_thunk(
    symbol: &str,
    frame: &ThunkFrame<'_>,
    abi: &FunctionAbi,
    callee: &str,
) -> Result<Module, BackendFailure> {
    let mut signature = Signature::new(
        symbol.trim_start_matches('@'),
        "ptr",
        vec![Parameter::named("ptr", "%frame")],
    );
    signature.linkage = Linkage::Internal;
    let mut body = FunctionBody::default();
    body.references.extend(frame.references);
    body.symbol(callee);
    body.symbol("llvm.coro.noop");
    body.open_block("entry".to_owned());
    let mut rendered = thunk_arguments(&mut body, frame.ty, frame.field_types, abi);
    rendered.insert(0, "ptr %parent".to_owned());
    rendered.insert(0, "ptr %slot".to_owned());
    write!(
        body,
        "  %slot = getelementptr inbounds {}, ptr %frame, i32 0, i32 {}\n  \
         %parent = call ptr @llvm.coro.noop()\n  \
         %handle = call ptr @{callee}({})\n  \
         ret ptr %handle\n",
        frame.ty,
        frame.result,
        rendered.join(", ")
    )
    .map_err(|_| BackendFailure::TextEmission)?;
    let mut module = Module::default();
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}
