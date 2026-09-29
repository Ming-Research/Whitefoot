//! Waiting functions [WAIT-1] lowered to resumable frames
//! (design/compiler/waiting-contexts.md).
//!
//! A waiting function is an LLVM switched-resume coroutine. Its definition is
//! the ramp: `ptr @f(ptr %wf.result, ptr %wf.coro.parent, parameters...)`,
//! which takes its frame from the running context's arena, copies the
//! arguments it was handed indirectly into that frame, and suspends before
//! the body runs. The result is always constructed through `%wf.result`,
//! whatever the function's ordinary ABI returns it in.
//!
//! A call of a waiting function makes the callee's frame and transfers into
//! it; the callee, when it returns, transfers back into `%wf.coro.parent`.
//! Both transfers are a resume placed immediately before a suspension, which
//! LLVM 18's coroutine split turns into a tail call, so a chain of calls that
//! return does not grow the native stack. The caller releases the callee's
//! frame before it reads the result.
//!
//! A call of a waiting host function is its `.start`, which submits the
//! operation into the running context's operation block or answers at once,
//! then, only for an operation still pending, `wf__context_wait` and a
//! suspension until the record is complete, and for any submitted operation
//! its `.finish`, which reads the record into the result.
//!
//! Every suspension returns to the context driver in the completion bridge,
//! which resumes the frame a context parked when the context is ready.

use std::fmt::Write;

use super::{BackendFailure, FunctionEmitter, FunctionSlot, RESULT_POINTER};
use crate::backend::abi::{FunctionAbi, ResultAbi};
use crate::backend::emission::{Module, Parameter, Signature};
use crate::{IrFunction, IrType, IrValueId};

/// A waiting definition's second parameter: the frame it transfers back into
/// when it returns, which is the no-op coroutine for a context's outermost
/// frame.
pub(super) const PARENT: &str = "%wf.coro.parent";

/// The frame a waiting definition is running in, as `llvm.coro.begin` names it.
pub(super) const HANDLE: &str = "%wf.coro.handle";

/// The block every suspension that is not resumed but destroyed leaves by.
const DESTROY: &str = "wf.coro.destroy";

/// The block every suspension returns to the resumer from.
const SUSPENDED: &str = "wf.coro.suspended";

/// The block a return transfers back into the caller from.
pub(super) const FINAL: &str = "wf.coro.final";

/// The block that makes the frame, before the body's entry block.
pub(super) const ENTRY: &str = "wf.coro.entry";

/// The block the body continues in after the ramp's first suspension.
const START: &str = "wf.coro.start";

/// The coroutine intrinsics and the context entries a module with a waiting
/// definition names, including the two a launcher of a waiting entry calls
/// and the no-op coroutine it gives the entry's frame as its parent.
pub(super) fn frame_runtime_declarations() -> Module {
    let mut module = Module::default();
    let declarations: [(&str, &str, &[&str]); 20] = [
        ("llvm.coro.id", "token", &["i32", "ptr", "ptr", "ptr"]),
        ("llvm.coro.alloc", "i1", &["token"]),
        ("llvm.coro.size.i64", "i64", &[]),
        ("llvm.coro.begin", "ptr", &["token", "ptr"]),
        ("llvm.coro.save", "token", &["ptr"]),
        ("llvm.coro.suspend", "i8", &["token", "i1"]),
        ("llvm.coro.free", "ptr", &["token", "ptr"]),
        ("llvm.coro.end", "i1", &["ptr", "i1", "token"]),
        ("llvm.coro.resume", "void", &["ptr"]),
        ("llvm.coro.destroy", "void", &["ptr"]),
        ("llvm.coro.noop", "ptr", &[]),
        ("wf__context_frame_allocate", "ptr", &["i64"]),
        ("wf__context_frame_release", "void", &["ptr"]),
        ("wf__context_operation", "ptr", &[]),
        ("wf__context_wait", "i32", &["ptr", "ptr"]),
        ("wf__context_prepare", "ptr", &["i64"]),
        ("wf__context_launch", "void", &["ptr", "ptr", "ptr"]),
        ("wf__context_join_wait", "i32", &["ptr", "ptr"]),
        ("wf__context_root_begin", "void", &[]),
        ("wf__context_root_run", "void", &["ptr"]),
    ];
    for (name, result, parameters) in declarations {
        module.declare(Signature::new(
            name,
            result,
            parameters
                .iter()
                .map(|ty| Parameter::unnamed(*ty))
                .collect(),
        ));
    }
    module
}

/// Whether the program defines a waiting function, and so names the frame
/// runtime.
pub(super) fn program_has_frames(program: &crate::IrProgram) -> bool {
    program
        .functions()
        .iter()
        .any(|function| function.waits() && !function.blocks().is_empty())
}

/// A waiting host function's two entries: `.start` and `.finish`, each with
/// the function's own ABI and then the running context's operation block.
pub(super) fn host_entry_symbols(symbol: &str) -> (String, String) {
    (format!("{symbol}.start"), format!("{symbol}.finish"))
}

/// The ABI a waiting function is defined and called under: its ordinary
/// parameters, and its result always constructed through a destination.
pub(super) fn waiting_abi(
    program: &crate::IrProgram,
    function: &IrFunction,
) -> Result<FunctionAbi, BackendFailure> {
    Ok(FunctionAbi::build(program, function)?.waiting())
}

/// The labels one waiting call or join uses, unique by the value it defines.
pub(super) fn labels(prefix: &str, value: IrValueId) -> String {
    format!("wf.{prefix}.v{}", value.ordinal())
}

impl FunctionEmitter<'_, '_> {
    /// The frame's making, in the definition's first block: the frame from
    /// the running context's arena unless the optimizer elides it into the
    /// caller's, then the body's entry block.
    pub(super) fn emit_frame_entry(&mut self) -> Result<(), BackendFailure> {
        self.output.open_block(ENTRY.to_owned());
        self.names(&[
            "llvm.coro.id",
            "llvm.coro.alloc",
            "llvm.coro.size.i64",
            "llvm.coro.begin",
            "wf__context_frame_allocate",
        ]);
        writeln!(
            self.output,
            "  %wf.coro.id = call token @llvm.coro.id(i32 16, ptr null, ptr null, ptr null)\n  \
             %wf.coro.needs = call i1 @llvm.coro.alloc(token %wf.coro.id)\n  \
             br i1 %wf.coro.needs, label %wf.coro.allocate, label %wf.coro.begin\n\
             wf.coro.allocate:\n  \
             %wf.coro.size = call i64 @llvm.coro.size.i64()\n  \
             %wf.coro.memory = call ptr @wf__context_frame_allocate(i64 %wf.coro.size)\n  \
             br label %wf.coro.begin\n\
             wf.coro.begin:\n  \
             %wf.coro.selected = phi ptr [ null, %{ENTRY} ], [ %wf.coro.memory, %wf.coro.allocate ]\n  \
             {HANDLE} = call ptr @llvm.coro.begin(token %wf.coro.id, ptr %wf.coro.selected)"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output
            .push_str(&super::contexts::context_group_initialization(
                self.function,
            ));
        writeln!(
            self.output,
            "  br label %{}",
            super::block_label(
                crate::IrBlockId::from_index(0).ok_or(BackendFailure::CounterOverflow)?
            )
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// The ramp's one suspension, after the body's entry block has copied
    /// every argument it was handed indirectly into the frame: a caller's
    /// temporary lives only until the ramp returns. The body continues when
    /// the caller transfers into the frame.
    pub(super) fn emit_frame_start(&mut self) -> Result<(), BackendFailure> {
        self.names(&["llvm.coro.suspend"]);
        writeln!(
            self.output,
            "  %wf.coro.initial = call i8 @llvm.coro.suspend(token none, i1 false)\n  \
             switch i8 %wf.coro.initial, label %{SUSPENDED} [ i8 0, label %{START} i8 1, label %{DESTROY} ]"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(START.to_owned());
        Ok(())
    }

    /// The return every exit of the body branches to, the release of the
    /// frame, and the one block every suspension returns to the resumer from.
    pub(super) fn emit_frame_exit(&mut self) -> Result<(), BackendFailure> {
        self.names(&[
            "llvm.coro.save",
            "llvm.coro.resume",
            "llvm.coro.suspend",
            "llvm.coro.free",
            "llvm.coro.end",
            "wf__context_frame_release",
        ]);
        writeln!(
            self.output,
            "{FINAL}:\n  \
             %wf.coro.final.saved = call token @llvm.coro.save(ptr null)\n  \
             call void @llvm.coro.resume(ptr {PARENT})\n  \
             %wf.coro.final.state = call i8 @llvm.coro.suspend(token %wf.coro.final.saved, i1 true)\n  \
             switch i8 %wf.coro.final.state, label %{SUSPENDED} [ i8 1, label %{DESTROY} ]\n\
             {DESTROY}:\n  \
             %wf.coro.freed = call ptr @llvm.coro.free(token %wf.coro.id, ptr {HANDLE})\n  \
             %wf.coro.owned = icmp ne ptr %wf.coro.freed, null\n  \
             br i1 %wf.coro.owned, label %wf.coro.release, label %{SUSPENDED}\n\
             wf.coro.release:\n  \
             call void @wf__context_frame_release(ptr %wf.coro.freed)\n  \
             br label %{SUSPENDED}\n\
             {SUSPENDED}:\n  \
             %wf.coro.end = call i1 @llvm.coro.end(ptr null, i1 false, token none)\n  \
             ret ptr {HANDLE}"
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// A suspension the frame's own call parked for: resumed, it continues
    /// in `resumed`; destroyed, it leaves by the frame's release.
    pub(super) fn emit_suspension(
        &mut self,
        saved: &str,
        prefix: &str,
        resumed: &str,
    ) -> Result<(), BackendFailure> {
        self.names(&["llvm.coro.suspend"]);
        writeln!(
            self.output,
            "  %{prefix}.state = call i8 @llvm.coro.suspend(token {saved}, i1 false)\n  \
             switch i8 %{prefix}.state, label %{SUSPENDED} [ i8 0, label %{resumed} i8 1, label %{DESTROY} ]"
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Where a waiting call constructs its result, and whether the value is
    /// then read back as an SSA value.
    pub(super) fn waiting_destination(
        &mut self,
        result: IrValueId,
        abi: ResultAbi,
    ) -> Result<(String, bool), BackendFailure> {
        match abi {
            ResultAbi::Destination(_) => Ok((self.value_place(result)?, false)),
            ResultAbi::StoredValue(_) | ResultAbi::Value(_) => {
                if self.storage.slot(result).is_some() {
                    Ok((self.value_place(result)?, true))
                } else {
                    Ok((self.entry_slot(FunctionSlot::WaitingResult(result))?, true))
                }
            }
        }
    }

    /// One call of a waiting function, from a waiting function: a transfer
    /// into the callee's frame for a source definition, or a host operation's
    /// start, wait and finish.
    pub(super) fn emit_waiting_call(
        &mut self,
        result: IrValueId,
        ty: IrType,
        function: u32,
        arguments: &[IrValueId],
    ) -> Result<(), BackendFailure> {
        if !self.function.waits() || self.grain.is_some() {
            return Err(BackendFailure::InvalidIr);
        }
        let target = self
            .program
            .functions()
            .get(function as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        let ordinary = FunctionAbi::build(self.program, target)?;
        if ordinary.result().ty() != ty || ordinary.parameters().len() != arguments.len() {
            return Err(BackendFailure::InvalidIr);
        }
        let host = target.blocks().is_empty();
        if host && !ordinary.result().uses_destination() {
            // Every waiting host entry constructs its result through a
            // pointer; a register-returned one would need a second shape.
            return Err(BackendFailure::InvalidIr);
        }
        let (destination, reads_back) = self.waiting_destination(result, ordinary.result())?;
        let mut rendered = vec![format!("ptr {destination}")];
        let mut operands = Vec::with_capacity(arguments.len());
        for (argument, parameter) in arguments.iter().zip(ordinary.parameters()) {
            if self.value_type(*argument) != Some(parameter.ty()) {
                return Err(BackendFailure::InvalidIr);
            }
            if parameter.is_indirect() {
                let address = self.value_place(*argument)?;
                operands.push(format!("ptr {address}"));
            } else {
                let operand = self.value_name(*argument);
                operands.push(self.value_argument(*parameter, &operand)?);
            }
        }
        let symbol = self.callee_symbol(function, target.name());
        if host {
            self.emit_host_wait(result, &symbol, &destination, &operands)?;
        } else {
            rendered.push(format!("ptr {HANDLE}"));
            rendered.extend(operands);
            self.emit_transfer(result, &symbol, &rendered)?;
        }
        if reads_back {
            let emitted = self.output.type_name(self.program, ty)?;
            writeln!(
                self.output,
                "  {} = load {emitted}, ptr {destination}",
                self.value_name(result)
            )
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        Ok(())
    }

    /// The callee's frame, a transfer into it, and its release once it has
    /// transferred back.
    fn emit_transfer(
        &mut self,
        result: IrValueId,
        symbol: &str,
        arguments: &[String],
    ) -> Result<(), BackendFailure> {
        let prefix = labels("call", result);
        self.output.symbol(symbol.to_owned());
        self.names(&["llvm.coro.save", "llvm.coro.resume", "llvm.coro.destroy"]);
        writeln!(
            self.output,
            "  %{prefix}.frame = call ptr @{symbol}({})\n  \
             %{prefix}.saved = call token @llvm.coro.save(ptr null)\n  \
             call void @llvm.coro.resume(ptr %{prefix}.frame)",
            arguments.join(", ")
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let returned = format!("{prefix}.returned");
        self.emit_suspension(&format!("%{prefix}.saved"), &prefix, &returned)?;
        self.output.open_block(returned);
        writeln!(
            self.output,
            "  call void @llvm.coro.destroy(ptr %{prefix}.frame)"
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// A host operation's start, and for a submitted one the wait and a
    /// suspension until its record completes, then its finish.
    fn emit_host_wait(
        &mut self,
        result: IrValueId,
        symbol: &str,
        destination: &str,
        operands: &[String],
    ) -> Result<(), BackendFailure> {
        let prefix = labels("host", result);
        let (start, finish) = host_entry_symbols(symbol);
        let mut arguments = vec![format!("ptr {destination}")];
        arguments.extend(operands.iter().cloned());
        arguments.push(format!("ptr %{prefix}.operation"));
        let arguments = arguments.join(", ");
        self.names(&[
            "wf__context_operation",
            "wf__context_wait",
            "llvm.coro.save",
        ]);
        self.output.symbol(start.clone());
        self.output.symbol(finish.clone());
        // The start answers 0 when it wrote the result itself, 1 when its
        // operation has already completed, and 2 when it is still pending.
        writeln!(
            self.output,
            "  %{prefix}.operation = call ptr @wf__context_operation()\n  \
             %{prefix}.started = call i32 @{start}({arguments})\n  \
             switch i32 %{prefix}.started, label %{prefix}.wait [ i32 0, label %{prefix}.done i32 1, label %{prefix}.finish ]\n\
             {prefix}.wait:\n  \
             %{prefix}.saved = call token @llvm.coro.save(ptr null)\n  \
             %{prefix}.parked = call i32 @wf__context_wait(ptr %{prefix}.operation, ptr {HANDLE})\n  \
             %{prefix}.suspends = icmp ne i32 %{prefix}.parked, 0\n  \
             br i1 %{prefix}.suspends, label %{prefix}.suspend, label %{prefix}.finish\n\
             {prefix}.suspend:"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_suspension(
            &format!("%{prefix}.saved"),
            &prefix,
            &format!("{prefix}.finish"),
        )?;
        writeln!(
            self.output,
            "{prefix}.finish:\n  \
             call void @{finish}({arguments})\n  \
             br label %{prefix}.done"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{prefix}.done"));
        Ok(())
    }

    /// [WAIT-3] the join before an exit: a suspension until every context
    /// this activation started has finished, when any has not.
    pub(super) fn emit_frame_join(&mut self, result: IrValueId) -> Result<(), BackendFailure> {
        self.emit_group_join(result, super::contexts::GROUP)
    }

    /// Suspends this frame until every context counted in `group` has
    /// finished.
    pub(super) fn emit_group_join(
        &mut self,
        result: IrValueId,
        group: &str,
    ) -> Result<(), BackendFailure> {
        let prefix = labels("join", result);
        self.names(&["wf__context_join_wait", "llvm.coro.save"]);
        writeln!(
            self.output,
            "  %{prefix}.saved = call token @llvm.coro.save(ptr null)\n  \
             %{prefix}.parked = call i32 @wf__context_join_wait(ptr {}, ptr {HANDLE})\n  \
             %{prefix}.suspends = icmp ne i32 %{prefix}.parked, 0\n  \
             br i1 %{prefix}.suspends, label %{prefix}.suspend, label %{prefix}.done\n\
             {prefix}.suspend:",
            group
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_suspension(
            &format!("%{prefix}.saved"),
            &prefix,
            &format!("{prefix}.done"),
        )?;
        self.output.open_block(format!("{prefix}.done"));
        Ok(())
    }

    /// Records the runtime entries and intrinsics a frame's lines name, so a
    /// link fragment that holds this definition declares them.
    pub(super) fn names(&mut self, symbols: &[&str]) {
        for symbol in symbols {
            self.output.symbol(*symbol);
        }
    }

    /// The return of a waiting definition: its result is already in
    /// `%wf.result`, so it transfers back into its caller.
    pub(super) fn emit_frame_return(&mut self) -> Result<(), BackendFailure> {
        writeln!(self.output, "  br label %{FINAL}").map_err(|_| BackendFailure::TextEmission)
    }
}

/// A waiting definition's signature: the frame it returns, the destination
/// and parent before the ordinary parameters, and the coroutine attribute.
pub(super) fn waiting_signature(symbol: String, mut parameters: Vec<Parameter>) -> Signature {
    parameters.insert(0, Parameter::named("ptr", PARENT));
    parameters.insert(0, Parameter::named("ptr", RESULT_POINTER));
    let mut signature = Signature::new(symbol, "ptr", parameters);
    signature.suffix.push_str(" presplitcoroutine");
    signature
}

/// A waiting host function's two declarations, which keep their parameter
/// names in whole-module output as every linked declaration does.
pub(super) fn host_declarations(symbol: &str, parameters: &[Parameter]) -> Module {
    let (start, finish) = host_entry_symbols(symbol);
    let mut with_operation = parameters.to_vec();
    with_operation.push(Parameter::named("ptr", "%wf.operation"));
    let mut module = Module::default();
    module.declare_named(Signature::new(start, "i32", with_operation.clone()));
    module.declare_named(Signature::new(finish, "void", with_operation));
    module
}
