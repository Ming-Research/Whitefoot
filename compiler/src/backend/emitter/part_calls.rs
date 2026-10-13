//! An ordinary call made from a part of a split dispatch loop goes to an
//! entry of its callee in the convention whose callee preserves almost every
//! register (compiler/match-dispatch-lowering).
//!
//! The parts carry the loop's state in argument registers, which the C
//! convention lets a callee overwrite, so a part that makes any C-convention
//! call keeps that state elsewhere around the call, and the host sets that
//! up where the arm begins, on its fast path too. Through the entry the state
//! stays in its registers and the callee saves what it overwrites.
//!
//! Each function a part calls gets one entry, emitted once per module after
//! every function: a small internal function of the entry convention marked
//! `noinline`, whose body is one C-convention call of the callee with the
//! same arguments and its result returned. The host cannot inline it into a
//! part, so the part's call stays in the entry convention and the part keeps
//! its state in its argument registers, while the entry saves what the C
//! call overwrites. The entry holds no copy of the callee's body: a copy was
//! inlined back into the parts, which restored the C-convention calls inside
//! them. The callee itself keeps the C convention, so its other callers and
//! the callable ABI that linked C bodies share are unchanged.
//!
//! The rule covers calls to functions this module defines whose result is a
//! value or goes through the caller's destination:
//!
//! - A linked declaration is not covered: this module defines no body for
//!   its entry to wrap, and the rule stays with the functions the module
//!   defines.
//! - A register-returned definition (compiler/result-registers) is an entry
//!   over its internal destination-form body, which the host optimizes alone
//!   before inlining it into that one caller; its call keeps that inlining.
//! - A waiting callee is entered by a frame transfer, not an ordinary call
//!   [WAIT-1].

use super::dispatch::Part;
use super::*;

/// The keyword, with its trailing space, of every call entry's convention.
const ENTRY_CONVENTION: &str = "preserve_mostcc ";

/// The symbol of the call entry of the function `name`.
///
/// It is compiler-owned: a `wf__` symbol holds a spelling no source
/// symbol has [FORM-3], and no other compiler-owned symbol begins
/// `wf__most_`.
fn entry_symbol(name: &str) -> String {
    format!("wf__most_{name}")
}

/// Whether a call to `callee` that names `symbol`, carries a budget when
/// `budgeted` and returns as `result` is one the rule covers.
fn takes_entry(callee: &IrFunction, symbol: &str, budgeted: bool, result: ResultAbi) -> bool {
    !budgeted
        && !callee.blocks().is_empty()
        && !callee.waits()
        && !matches!(result, ResultAbi::StoredValue(_))
        && symbol == source_symbol(callee.name())
}

impl FunctionEmitter<'_, '_> {
    /// The symbol and the convention keyword an ordinary call to the
    /// `function`th function takes, where it would otherwise name `symbol`
    /// in the C convention with its result crossing as `result`.
    ///
    /// Within the header or an arm of a split dispatch loop, a call the rule
    /// covers names the callee's call entry when the host generates the
    /// entry's convention, and the entry is recorded for emission; every
    /// other call is unchanged. This is the one place the rule is decided.
    pub(super) fn ordinary_call_target(
        &mut self,
        function: u32,
        symbol: String,
        budgeted: bool,
        result: ResultAbi,
    ) -> (String, &'static str) {
        let in_part = self
            .dispatch
            .as_ref()
            .is_some_and(|dispatch| dispatch.part != Part::Enclosing);
        let Some(callee) = self.program.functions().get(function as usize) else {
            return (symbol, "");
        };
        let applies = in_part && takes_entry(callee, &symbol, budgeted, result);
        match &mut self.call_entries {
            Some(entries) if applies => {
                entries.insert(function);
                (entry_symbol(callee.name()), ENTRY_CONVENTION)
            }
            _ => (symbol, ""),
        }
    }
}

/// The call entry of the `callee`th function of `program`: an internal
/// function of the entry convention with the callee's callable ABI, marked
/// `noinline`, which calls the callee in the C convention and returns its
/// result.
pub(super) fn emit_call_entry(program: &IrProgram, callee: u32) -> Result<Module, BackendFailure> {
    let function = program
        .functions()
        .get(callee as usize)
        .ok_or(BackendFailure::InvalidIr)?;
    let public = FunctionAbi::build(program, function)?;
    let symbol = source_symbol(function.name());
    if !takes_entry(function, &symbol, false, public.result()) {
        return Err(BackendFailure::InvalidIr);
    }
    let mut references = References::default();
    let result = if public.result().uses_destination() {
        "void".to_owned()
    } else {
        llvm_type_with_references(program, public.result().ty(), &mut references.types)?
    };
    let mut parameters = Vec::new();
    if public.result().uses_destination() {
        parameters.push(Parameter::named("ptr", RESULT_POINTER));
    }
    for ((value, _), parameter) in function.parameters().iter().zip(public.parameters()) {
        parameters.extend(incoming_parameters(
            program,
            *value,
            *parameter,
            "",
            &mut references,
        )?);
    }
    let mut signature = Signature::new(entry_symbol(function.name()), result.clone(), parameters);
    signature.linkage = Linkage::Internal;
    signature.convention = ENTRY_CONVENTION;
    signature.suffix.push_str(" noinline");
    signature.references = references;
    let arguments = ordinary_call_arguments(program, function, &public)?;
    let mut output = FunctionBody::default();
    output.open_block("entry".to_owned());
    output.symbol(symbol.clone());
    if public.result().uses_destination() {
        write!(output, "  call void @{symbol}({arguments})\n  ret void\n")?;
    } else {
        write!(
            output,
            "  %wf.entry = call {result} @{symbol}({arguments})\n  \
             ret {result} %wf.entry\n"
        )?;
    }
    let entity = signature.define(output, "")?;
    let mut module = Module::default();
    module.define(entity);
    module.text("\n");
    Ok(module)
}
