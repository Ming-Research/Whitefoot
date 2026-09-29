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
//! have finished. A bound start's context constructs its result in a slot of
//! the starting frame and is counted in a group of its own, which the
//! start's await joins before it reads the slot [WAIT-2].
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

/// The group a bound start keeps for its one context [WAIT-2], named after
/// the start's value so its await can find it.
pub(super) fn bound_group(start: IrValueId) -> String {
    format!("%wf.ctx.group.{}", start.index())
}

/// Every bound start of a function, in block order.
pub(super) fn bound_starts(function: &IrFunction) -> Vec<IrValueId> {
    function
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
        .filter_map(|instruction| match instruction {
            IrInstruction::Define {
                result,
                operation: IrOperation::ContextStartBound { .. },
                ..
            } => Some(*result),
            _ => None,
        })
        .collect()
}

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

/// The entry-block lines that reserve the activation's group and one group
/// per bound start.
pub(super) fn context_group_prelude(function: &IrFunction) -> String {
    let mut prelude = String::new();
    if keeps_context_group(function) {
        prelude.push_str(&format!("  {GROUP} = alloca [2 x i64], align 8\n"));
    }
    for start in bound_starts(function) {
        prelude.push_str(&format!(
            "  {} = alloca [2 x i64], align 8\n",
            bound_group(start)
        ));
    }
    prelude
}

/// The groups' zeroing, which the frame's making emits once the frame exists,
/// so the zeroed words are the frame's own.
pub(super) fn context_group_initialization(function: &IrFunction) -> String {
    let mut initialization = String::new();
    if keeps_context_group(function) {
        initialization.push_str(&format!("  store [2 x i64] zeroinitializer, ptr {GROUP}\n"));
    }
    for start in bound_starts(function) {
        initialization.push_str(&format!(
            "  store [2 x i64] zeroinitializer, ptr {}\n",
            bound_group(start)
        ));
    }
    initialization
}

impl FunctionEmitter<'_, '_> {
    /// Starts one context over the wrapper `function` with `arguments`. A
    /// bound start's wrapper returns the marked call's result, which the
    /// context constructs in the starting frame's slot for `result` and its
    /// await reads back; an unbound one's unit result lands in the argument
    /// block.
    pub(super) fn emit_context_start(
        &mut self,
        result: IrValueId,
        function: u32,
        arguments: &[IrValueId],
        bound: bool,
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
        let result_field = field_types.len();
        if bound {
            field_types.push("ptr".to_owned());
        } else {
            let result_type = super::llvm_type_with_references(
                self.program,
                abi.result().ty(),
                &mut frame_references.types,
            )?;
            field_types.push(result_type);
        }
        let frame_type = format!("{{ {} }}", field_types.join(", "));
        // A wrapper is reached only through a start, never by a call inside a
        // budgeted component, so it has no budget-carrying variant.
        let (callee, budget) = self.callee_target(function, target.name(), result);
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
                    bound,
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
        let group = if bound {
            let slot = self.entry_slot(super::FunctionSlot::ContextResult(result))?;
            let field = format!("%{}", self.next_temporary()?);
            writeln!(
                self.output,
                "  {field} = getelementptr inbounds {frame_type}, ptr {frame}, i32 0, i32 {result_field}\n  store ptr {slot}, ptr {field}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            bound_group(result)
        } else {
            GROUP.to_owned()
        };
        writeln!(
            self.output,
            "  call void @wf__context_launch(ptr {group}, ptr {frame}, ptr {thunk})"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, IrType::Unit, IrConstant::Unit)
    }

    /// Waits for the context the bound start `start` started, then defines
    /// `result` from the slot the context constructed it in, exactly as a
    /// waiting call defines its result from its destination.
    pub(super) fn emit_context_await(
        &mut self,
        result: IrValueId,
        ty: IrType,
        start: IrValueId,
    ) -> Result<(), BackendFailure> {
        let wrapper = self
            .function
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
            .find_map(|instruction| match instruction {
                IrInstruction::Define {
                    result,
                    operation: IrOperation::ContextStartBound { function, .. },
                    ..
                } if *result == start => Some(*function),
                _ => None,
            })
            .ok_or(BackendFailure::InvalidIr)?;
        let target = self
            .program
            .functions()
            .get(wrapper as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        let ordinary = FunctionAbi::build(self.program, target)?;
        if ordinary.result().ty() != ty {
            return Err(BackendFailure::InvalidIr);
        }
        self.emit_group_join(result, &bound_group(start))?;
        let slot = self.entry_slot(super::FunctionSlot::ContextResult(start))?;
        let (destination, reads_back) = self.waiting_destination(result, ordinary.result())?;
        // A memory-only result (compiler/payload-enum-layout) moves by
        // memmove; it returns through a destination, so it is never read
        // back as a value.
        if self.is_memory_only(ty)? {
            if reads_back {
                return Err(BackendFailure::InvalidIr);
            }
            return self.copy_storage(ty, &slot, &destination);
        }
        let emitted = self.output.type_name(self.program, ty)?;
        let moved = format!("%{}", self.next_temporary()?);
        writeln!(
            self.output,
            "  {moved} = load {emitted}, ptr {slot}\n  store {emitted} {moved}, ptr {destination}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        if reads_back {
            writeln!(
                self.output,
                "  {} = load {emitted}, ptr {destination}",
                self.value_name(result)
            )
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        Ok(())
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
/// the driver. An unbound wrapper's unit result lands in the block's last
/// field; a bound one's in the slot whose address that field holds.
fn context_thunk(
    symbol: &str,
    frame: &ThunkFrame<'_>,
    abi: &FunctionAbi,
    callee: &str,
    bound: bool,
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
    let slot = if bound { "%slot.field" } else { "%slot" };
    writeln!(
        body,
        "  {slot} = getelementptr inbounds {}, ptr %frame, i32 0, i32 {}",
        frame.ty, frame.result
    )
    .map_err(|_| BackendFailure::TextEmission)?;
    if bound {
        body.push_str("  %slot = load ptr, ptr %slot.field\n");
    }
    write!(
        body,
        "  %parent = call ptr @llvm.coro.noop()\n  \
         %handle = call ptr @{callee}({})\n  \
         ret ptr %handle\n",
        rendered.join(", ")
    )
    .map_err(|_| BackendFailure::TextEmission)?;
    let mut module = Module::default();
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}
