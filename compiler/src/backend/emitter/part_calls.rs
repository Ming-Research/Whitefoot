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
//! every function. The callee itself keeps the C convention, so its other
//! callers and the callable ABI that linked C bodies share are unchanged.
//! The entry calls the callee's body with a call-site `alwaysinline`, so
//! where the host can see that body the entry becomes a copy of it in the
//! entry's convention, which the host inlines into a part exactly where it
//! would have inlined the callee, and calls through the entry only where it
//! would have called. A register-returned definition's entry calls its
//! internal destination-form body and loads the result, as its public symbol
//! does, because forcing only the public symbol inline would leave the body
//! a second caller, taking the single-caller inlining that public symbol
//! relies on. In a fragment build the entry is defined in its callee's
//! fragment, where that body is.
//!
//! The rule covers calls to functions this module defines. A linked
//! declaration's body is C the host sees only at link time, if at all, so
//! its entry would forward one call: inlined, it restores the C call it
//! wraps, and kept out of line it adds a call and blocks the inlining a
//! full-LTO link gives the runtime's C bodies. A waiting callee is entered
//! by a frame transfer, not an ordinary call [WAIT-1].

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

impl FunctionEmitter<'_, '_> {
    /// The symbol and the convention keyword an ordinary call to the
    /// `function`th function takes, where it would otherwise name `symbol`
    /// in the C convention.
    ///
    /// Within the header or an arm of a split dispatch loop, a call to a
    /// definition of this module by its source symbol, carrying no budget,
    /// names the callee's call entry when the host generates the entry's
    /// convention, and the entry is recorded for emission; every other call
    /// is unchanged. This is the one place the rule is decided.
    pub(super) fn ordinary_call_target(
        &mut self,
        function: u32,
        symbol: String,
        budgeted: bool,
    ) -> (String, &'static str) {
        let in_part = self
            .dispatch
            .as_ref()
            .is_some_and(|dispatch| dispatch.part != Part::Enclosing);
        let Some(callee) = self.program.functions().get(function as usize) else {
            return (symbol, "");
        };
        let applies = in_part
            && !budgeted
            && !callee.blocks().is_empty()
            && !callee.waits()
            && symbol == source_symbol(callee.name());
        match &mut self.call_entries {
            Some(entries) if applies => {
                entries.insert(function);
                (entry_symbol(callee.name()), ENTRY_CONVENTION)
            }
            _ => (symbol, ""),
        }
    }
}

/// The call entry of the `ordinal`th function of `program`: an internal
/// function of the entry convention with the callee's callable ABI, which
/// calls the callee's body, inline where the host can, and returns its
/// result, anchored to the callee's source symbol so a fragment build
/// defines it beside that body.
pub(super) fn emit_call_entry(
    program: &IrProgram,
    target: TargetLayout,
    ordinal: u32,
    frontiers: &RecursiveFrontiers,
) -> Result<Module, BackendFailure> {
    let function = program
        .functions()
        .get(ordinal as usize)
        .ok_or(BackendFailure::InvalidIr)?;
    if function.blocks().is_empty() || function.waits() {
        return Err(BackendFailure::InvalidIr);
    }
    let public = FunctionAbi::build(program, function)?;
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
    // A budgeted component's member keeps its source symbol for the entry
    // that obtains its budget, which has no separate body.
    let register_returned = frontiers.grain(ordinal as usize).is_none()
        && matches!(public.result(), ResultAbi::StoredValue(_));
    let frame = if register_returned {
        FunctionFramePlan::returned_value(target, program, public.result().ty())?.render(
            target,
            program,
            &mut references,
        )?
    } else {
        String::new()
    };
    let symbol = source_symbol(function.name());
    let mut signature = Signature::new(entry_symbol(function.name()), result.clone(), parameters);
    signature.linkage = Linkage::Internal;
    signature.convention = ENTRY_CONVENTION;
    signature.references = references;
    let mut output = FunctionBody::default();
    output.open_block("entry".to_owned());
    if register_returned {
        let body = result_body_symbol(&symbol);
        let arguments = ordinary_call_arguments(program, function, &public.body())?;
        output.symbol(body.clone());
        write!(
            output,
            "  call void @{body}({arguments}) alwaysinline\n  \
             %wf.returned = load {result}, ptr {RESULT_POINTER}\n  ret {result} %wf.returned\n"
        )?;
    } else {
        let arguments = ordinary_call_arguments(program, function, &public)?;
        output.symbol(symbol.clone());
        if public.result().uses_destination() {
            write!(output, "  call void @{symbol}({arguments}) alwaysinline\n  ret void\n")?;
        } else {
            write!(
                output,
                "  %wf.entry = call {result} @{symbol}({arguments}) alwaysinline\n  \
                 ret {result} %wf.entry\n"
            )?;
        }
    }
    let mut entity = signature.define(output, &frame)?;
    entity.anchor = Some(symbol);
    let mut module = Module::default();
    module.define(entity);
    module.text("\n");
    Ok(module)
}
