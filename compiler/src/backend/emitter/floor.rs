//! The resource-exhaustion floor's translation unit and the module's own
//! standalone answer for it.
//!
//! This is the structural twin of [`super::parallel`]'s runtime wiring, with
//! one deliberate difference: the parallel runtime joins the link only when a
//! module hands work out, and the floor joins it always. Every program can run
//! out of stack, so every program carries the unit that reports it.

use super::BackendFailure;
use crate::backend::emission::{FunctionBody, Linkage, Module, Parameter, Signature};
/// The floor runtime's source, carried inside the compiler.
///
/// Its bytes travel in the compiler binary and are written beside the module
/// at link time, so no installed path, build directory, or environment
/// variable decides which floor a program gets.
pub const FLOOR_RUNTIME_SOURCE: &str = include_str!("../wf_floor.c");

/// The Windows resource-exhaustion floor carried inside the compiler.
pub const FLOOR_WINDOWS_RUNTIME_SOURCE: &str = include_str!("../wf_floor_windows.c");

/// The stack every thread that runs Whitefoot code gets, as a number this side
/// of the link can read.
///
/// The definition is the C one; this is the same number, restated where the
/// stack ledger can derive a depth from it. Deriving one from anything else —
/// the host's limit, a guess, a second constant — would give the writer a
/// figure the program does not obey, so
/// `the_ledger_and_the_runtime_name_the_same_stack` pins the two together
/// instead of trusting the comment.
pub const FLOOR_STACK_BYTES: u64 = 1024 * 1024 * 1024;

/// The module's own definition of the floor entry point.
///
/// An emitted module is a complete program on its own — `--emit-llvm` output
/// must link and run without the compiler's driver — so the module defines
/// every runtime symbol it names. `weak` linkage lets the real definition in
/// [`FLOOR_RUNTIME_SOURCE`] replace this one whenever that unit is linked,
/// which for the floor is every ordinary build.
///
/// The call carries `noinline` so that a small enough program is not pasted
/// into this definition as well, leaving every module with a second copy of
/// the whole program for the sake of a definition that only exists to keep an
/// unlinked module runnable. The restriction belongs here rather than on the
/// entry itself, where it would also suppress the optimizer's hot/cold
/// splitting of the program's own failure arms.
///
/// Without the runtime there is no second stack to move to, so the honest
/// answer is to run the entry on the thread the host started: the program
/// still runs and still means the same thing, with the ceiling and the bare
/// host signal it had before the floor existed.
pub(super) fn floor_runtime_fallback() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    let mut signature = Signature::new(
        "wf__floor_run",
        "i32",
        vec![
            Parameter::named("i32", "%argc"),
            Parameter::named("ptr", "%argv"),
        ],
    );
    signature.linkage = Linkage::Weak;
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions(
        "  %status = call i32 @wf__main_body(i32 %argc, ptr %argv) noinline\n  ret i32 %status\n",
        &["wf__main_body"],
    );
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::{FLOOR_RUNTIME_SOURCE, FLOOR_STACK_BYTES};

    /// The runtime's constant and the ledger's are one number.
    ///
    /// They are written in two languages and cannot share a definition, so the
    /// agreement is machine-checked rather than remembered. A ledger deriving
    /// depths from a stack no thread actually has would be worse than no
    /// ledger: every number in it would be wrong by the same factor, and
    /// nothing in the output would say so.
    #[test]
    fn the_ledger_and_the_runtime_name_the_same_stack() {
        let spelling = format!(
            "#define WF_FLOOR_STACK_BYTES ((size_t){}u",
            FLOOR_STACK_BYTES
        );
        let megabytes = format!(
            "#define WF_FLOOR_STACK_BYTES ((size_t){}u * 1024u * 1024u)",
            FLOOR_STACK_BYTES / (1024 * 1024)
        );
        assert!(
            FLOOR_RUNTIME_SOURCE.contains(&spelling) || FLOOR_RUNTIME_SOURCE.contains(&megabytes),
            "the floor runtime does not define a {FLOOR_STACK_BYTES}-byte stack, so the \
             ledger would derive depths from a stack no thread has"
        );
    }
}
