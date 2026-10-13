//! Host LLVM forms, discovered when the compiler runs rather than when it is built.
//!
//! A released compiler can run on a host whose clang accepts different forms
//! from the build host's clang. Probe the same executable the driver uses,
//! once on the first request for emission facts in each process.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The host clang used by the driver, runtime probes and native test helpers.
pub const fn clang_executable() -> &'static str {
    if cfg!(target_os = "windows") {
        "clang"
    } else {
        "/usr/bin/clang"
    }
}

/// LLVM forms accepted by the clang on the running compiler's host.
#[derive(Clone, Copy)]
pub(crate) struct ToolchainFacts {
    pub(crate) preserve_none: bool,
    pub(crate) no_callee_saved_registers: bool,
    pub(crate) no_capture_attribute: &'static str,
    pub(crate) coro_end_result: &'static str,
}

impl ToolchainFacts {
    /// A private dispatch ABI extension, qualified on definitions and calls.
    /// Other targets and the C-convention fallback keep their current ABI.
    pub(crate) fn dispatch_attribute(&self, triple: &str) -> &'static str {
        if triple.starts_with("x86_64") && self.preserve_none && self.no_callee_saved_registers {
            " \"no_callee_saved_registers\""
        } else {
            ""
        }
    }

    /// The call prefix matching the result type probed on this host at run time.
    pub(crate) fn coro_end_call(&self) -> &'static str {
        if self.coro_end_result == "void" {
            "call void"
        } else {
            "%wf.coro.end = call i1"
        }
    }

    fn probe() -> Self {
        let Ok(directory) = ProbeDirectory::new() else {
            return Self {
                preserve_none: false,
                no_callee_saved_registers: false,
                no_capture_attribute: "nocapture",
                coro_end_result: "i1",
            };
        };
        let preserve_none = preserve_none_supported(&directory.0);
        Self {
            preserve_none,
            no_callee_saved_registers: cfg!(target_arch = "x86_64")
                && preserve_none
                && no_callee_saved_registers_supported(&directory.0),
            no_capture_attribute: no_capture_attribute(&directory.0),
            coro_end_result: coro_end_result(&directory.0),
        }
    }
}

/// Lazily probes the host facts once per process; concurrent emitters wait
/// for the same value, including conservative fallbacks when clang cannot run.
pub(crate) fn facts() -> &'static ToolchainFacts {
    static FACTS: OnceLock<ToolchainFacts> = OnceLock::new();
    FACTS.get_or_init(ToolchainFacts::probe)
}

/// Owns only a freshly created directory. The process ID separates concurrent
/// compiler processes; the sequence also avoids stale directories after PID
/// reuse and separates the tests' scratch modules from the runtime probes.
struct ProbeDirectory(PathBuf);

impl ProbeDirectory {
    fn new() -> io::Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "whitefoot-toolchain-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for ProbeDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The result type of `llvm.coro.end` in the host toolchain, probed at run
/// time (compiler/backend-facts): `i1` until LLVM changed it to `void`. A
/// module that calls the newer form is verified by the clang `whitefootc`
/// runs, without generating code: the verifier checks the
/// intrinsic's type at a call, while some versions accept a bare declaration
/// of either form (Apple clang 21), and a call outside a coroutine can crash
/// code generation. Verification is requested explicitly, since release
/// builds of clang 18 to 20 skip it on IR input by default.
fn coro_end_result(directory: &Path) -> &'static str {
    const OLD: &str = "i1";
    const NEW: &str = "void";
    let probe = directory.join("coro_end_probe.ll");
    if fs::write(
        &probe,
        "declare void @llvm.coro.end(ptr, i1, token)\n\n\
         define void @probe() {\n  \
         call void @llvm.coro.end(ptr null, i1 false, token none)\n  ret void\n}\n",
    )
    .is_err()
    {
        return OLD;
    }
    let accepted = Command::new(clang_executable())
        .args([
            "-x",
            "ir",
            "-S",
            "-emit-llvm",
            "-fverify-intermediate-code",
            "-o",
        ])
        .arg(directory.join("coro_end_probe.verified.ll"))
        .arg(&probe)
        .output()
        .is_ok_and(|output| output.status.success());
    if accepted { NEW } else { OLD }
}

/// Whether the host toolchain probed at run time accepts a
/// guaranteed tail call between preserve_none functions
/// (compiler/match-dispatch-lowering). On x86-64 this convention alone still
/// preserves RBP; the separate semantic probe qualifies its removal.
///
/// LLVM 19 added the convention, and supported vendor hosts differ from the
/// Linux CI pin, so it is probed:
/// a two-function module with the convention and a `musttail` call between
/// them is handed to the assembler `whitefootc` runs. Where it is refused,
/// or no assembler can be run, the lowering uses the C convention.
fn preserve_none_supported(directory: &Path) -> bool {
    let probe = directory.join("preserve_none_probe.ll");
    if fs::write(
        &probe,
        "define internal preserve_nonecc i64 @q(i64 %a) {\n  ret i64 %a\n}\n\n\
         define preserve_nonecc i64 @p(i64 %a) {\n  \
         %r = musttail call preserve_nonecc i64 @q(i64 %a)\n  ret i64 %r\n}\n",
    )
    .is_err()
    {
        return false;
    }
    Command::new(clang_executable())
        .args(["-x", "ir", "-c", "-o"])
        .arg(directory.join("preserve_none_probe.o"))
        .arg(&probe)
        .output()
        .is_ok_and(|output| output.status.success())
}

/// String attributes can be silently ignored, so successful assembly alone
/// does not qualify this private ABI. Model the actual always-inlined
/// dispatcher: its indirect musttail call becomes the C entry's ordinary
/// call. Explicit RBP clobbers require preservation under preserve_nonecc
/// alone, and matching call attributes must move preservation to the entry.
fn no_callee_saved_registers_module(definition_attribute: &str, call_attribute: &str) -> String {
    format!(
        "@probe_targets = global ptr @probe_direct\n\
         @probe_indirect_targets = global ptr @probe_indirect\n\
         define internal preserve_nonecc i64 @probe_handler(ptr %targets, i64 %value) noinline{definition_attribute} {{\n  \
         call void asm sideeffect \"\", \"~{{rbp}}\"()\n  ret i64 %value\n}}\n\
         define internal preserve_nonecc i64 @probe_direct(ptr %targets, i64 %value) noinline{definition_attribute} {{\n  \
         call void asm sideeffect \"\", \"~{{rbp}}\"()\n  \
         %result = musttail call preserve_nonecc i64 @probe_handler(ptr %targets, i64 %value){call_attribute}\n  \
         ret i64 %result\n}}\n\
         define internal preserve_nonecc i64 @probe_indirect(ptr %targets, i64 %value) noinline{definition_attribute} {{\n  \
         call void asm sideeffect \"\", \"~{{rbp}}\"()\n  \
         %target = load ptr, ptr %targets\n  \
         %result = musttail call preserve_nonecc i64 %target(ptr %targets, i64 %value){call_attribute}\n  \
         ret i64 %result\n}}\n\
         define internal preserve_nonecc i64 @probe_dispatch(ptr %targets, i64 %value) alwaysinline{definition_attribute} {{\n  \
         %target = load ptr, ptr %targets\n  \
         %result = musttail call preserve_nonecc i64 %target(ptr %targets, i64 %value){call_attribute}\n  \
         ret i64 %result\n}}\n\
         define i64 @probe_entry(ptr %targets, i64 %value) {{\n  \
         %result = call preserve_nonecc i64 @probe_dispatch(ptr %targets, i64 %value){call_attribute}\n  \
         ret i64 %result\n}}\n"
    )
}

fn no_callee_saved_registers_assembly(
    directory: &Path,
    name: &str,
    definition_attribute: &str,
    call_attribute: &str,
) -> Option<String> {
    let input = directory.join(format!("{name}.ll"));
    let output = directory.join(format!("{name}.s"));
    fs::write(
        &input,
        no_callee_saved_registers_module(definition_attribute, call_attribute),
    )
    .ok()?;
    let status = Command::new(clang_executable())
        .args([
            "-x",
            "ir",
            "-S",
            "-O2",
            "-fomit-frame-pointer",
            "-fverify-intermediate-code",
            "-o",
        ])
        .arg(&output)
        .arg(&input)
        .output()
        .ok()?;
    status
        .status
        .success()
        .then(|| fs::read_to_string(output).ok())?
}

fn probe_function_assembly<'a>(assembly: &'a str, name: &str) -> Option<&'a str> {
    let mut start = None;
    let mut offset = 0;
    for line in assembly.split_inclusive('\n') {
        let label = line.split('#').next()?.trim();
        if label == format!("{name}:") || label == format!("_{name}:") {
            start = Some(offset + line.len());
        } else if let Some(start) = start
            && label.ends_with(':')
            && (label.starts_with("probe_")
                || label.starts_with("_probe_")
                || label.starts_with(".Lfunc_end")
                || label.starts_with("Lfunc_end"))
        {
            return Some(&assembly[start..offset]);
        }
        offset += line.len();
    }
    start.map(|start| &assembly[start..])
}

fn rbp_preservation(assembly: &str) -> (usize, usize) {
    let mut pushes = 0;
    let mut pops = 0;
    for line in assembly.lines() {
        let mut words = line.split_whitespace();
        let instruction = words.next();
        if words.next() == Some("%rbp") {
            match instruction {
                Some("pushq" | "push") => pushes += 1,
                Some("popq" | "pop") => pops += 1,
                _ => {}
            }
        }
    }
    (pushes, pops)
}

fn no_callee_saved_registers_contract(assembly: &str) -> bool {
    ["probe_handler", "probe_direct", "probe_indirect"]
        .into_iter()
        .all(|name| {
            probe_function_assembly(assembly, name).is_some_and(|body| {
                // The empty inline asm has only a clobber, so no emitted
                // instruction needs RBP. Reject memory saves as well as pushes.
                !body.lines().any(|line| {
                    line.split('#')
                        .next()
                        .is_some_and(|instruction| instruction.contains("%rbp"))
                })
            })
        })
        && probe_function_assembly(assembly, "probe_entry")
            .is_some_and(|body| rbp_preservation(body) == (1, 1))
}

fn no_callee_saved_registers_supported(directory: &Path) -> bool {
    const ATTRIBUTE: &str = " \"no_callee_saved_registers\"";
    no_callee_saved_registers_assembly(directory, "callee_saved_probe", ATTRIBUTE, ATTRIBUTE)
        .is_some_and(|assembly| no_callee_saved_registers_contract(&assembly))
}

/// Which spelling of the no-capture parameter attribute the host toolchain
/// accepts at run time (compiler/backend-facts).
///
/// LLVM 21 renamed `nocapture` to `captures(none)`. Supported vendor hosts
/// differ from the Linux CI pin, so the spelling is probed rather than assumed:
/// a one-function module carrying
/// the new spelling is handed to the assembler, and the old spelling is used
/// when it is refused. The old spelling is also the fallback where no
/// assembler can be run at all, because every LLVM that has the new one still
/// auto-upgrades the old.
fn no_capture_attribute(directory: &Path) -> &'static str {
    const OLD: &str = "nocapture";
    const NEW: &str = "captures(none)";
    let probe = directory.join("captures_probe.ll");
    if fs::write(
        &probe,
        format!("define void @p(ptr {NEW} %v) {{\n  ret void\n}}\n"),
    )
    .is_err()
    {
        return OLD;
    }
    let accepted = Command::new(clang_executable())
        .args(["-x", "ir", "-c", "-o"])
        .arg(directory.join("captures_probe.o"))
        .arg(&probe)
        .output()
        .is_ok_and(|output| output.status.success());
    if accepted { NEW } else { OLD }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Output;

    // Calls, not just declarations: some clang versions accept a declaration
    // with the wrong intrinsic type. Never generate code for this module,
    // since a coro.end call outside a coroutine can crash code generation.
    fn coroutine_module(result: &str, no_capture: &str) -> String {
        format!(
            "declare {result} @llvm.coro.end(ptr, i1, token)\n\
             define void @check(ptr {no_capture} %argument) {{\n  \
             call {result} @llvm.coro.end(ptr null, i1 false, token none)\n  \
             ret void\n}}\n"
        )
    }

    fn verify(directory: &Path, name: &str, module: &str) -> io::Result<Output> {
        let input = directory.join(format!("{name}.ll"));
        fs::write(&input, module).expect("write verification module");
        Command::new(clang_executable())
            .args([
                "-x",
                "ir",
                "-S",
                "-emit-llvm",
                "-fverify-intermediate-code",
                "-o",
            ])
            .arg(directory.join(format!("{name}.verified.ll")))
            .arg(input)
            .output()
    }

    #[test]
    fn runtime_facts_match_host_clang() {
        let facts = facts();
        let directory = ProbeDirectory::new().expect("create verification directory");
        let chosen = coroutine_module(facts.coro_end_result, facts.no_capture_attribute);
        // The probes' own fallback stands where clang cannot run; this test
        // needs clang as its oracle, as every native test does.
        let accepted = verify(&directory.0, "chosen", &chosen).unwrap_or_else(|error| {
            panic!(
                "cannot run {} to check the probed forms: {error}",
                clang_executable()
            )
        });
        assert!(
            accepted.status.success(),
            "runtime-selected forms must verify: {chosen}\n{}",
            String::from_utf8_lossy(&accepted.stderr)
        );

        let other_result = match facts.coro_end_result {
            "i1" => "void",
            "void" => "i1",
            other => panic!("unexpected coro.end result: {other}"),
        };
        let rejected = verify(
            &directory.0,
            "other",
            &coroutine_module(other_result, facts.no_capture_attribute),
        )
        .expect("run clang after the chosen form verified");
        let diagnostic = String::from_utf8_lossy(&rejected.stderr);
        assert!(
            !rejected.status.success() && diagnostic.contains("llvm.coro.end"),
            "the other coro.end result must be rejected at the intrinsic: {diagnostic}"
        );

        // Exercise the convention independently of the emitter's expected
        // text, including its conservative C-convention choice on older clang.
        let input = directory.0.join("tail.ll");
        fs::write(
            &input,
            "define internal preserve_nonecc i64 @callee(i64 %value) {\n  \
             ret i64 %value\n}\n\
             define preserve_nonecc i64 @caller(i64 %value) {\n  \
             %result = musttail call preserve_nonecc i64 @callee(i64 %value)\n  \
             ret i64 %result\n}\n",
        )
        .expect("write tail-call verification module");
        let tail = Command::new(clang_executable())
            .args(["-x", "ir", "-c", "-o"])
            .arg(directory.0.join("tail.o"))
            .arg(input)
            .output()
            .expect("run clang after the chosen form verified");
        assert_eq!(
            facts.preserve_none,
            tail.status.success(),
            "runtime convention fact must agree with host code generation: {}",
            String::from_utf8_lossy(&tail.stderr)
        );

        if cfg!(target_arch = "x86_64") && facts.preserve_none {
            const ATTRIBUTE: &str = " \"no_callee_saved_registers\"";
            let assembly = no_callee_saved_registers_assembly(
                &directory.0,
                "register_contract",
                ATTRIBUTE,
                ATTRIBUTE,
            );
            assert_eq!(
                facts.no_callee_saved_registers,
                assembly
                    .as_deref()
                    .is_some_and(no_callee_saved_registers_contract),
                "the runtime ABI fact must match definition and call-side preservation"
            );
            // An ignored string attribute behaves like this legacy module;
            // the explicit RBP clobbers must reject its per-handler save set.
            let legacy =
                no_callee_saved_registers_assembly(&directory.0, "register_legacy", "", "")
                    .expect("preserve_nonecc already generated code");
            assert!(
                !no_callee_saved_registers_contract(&legacy),
                "the semantic probe must reject the legacy ABI: {legacy}"
            );
            if facts.no_callee_saved_registers {
                let missing_call = no_callee_saved_registers_assembly(
                    &directory.0,
                    "register_missing_call",
                    ATTRIBUTE,
                    "",
                );
                assert!(
                    !missing_call
                        .as_deref()
                        .is_some_and(no_callee_saved_registers_contract),
                    "the semantic probe must detect a missing indirect call mask"
                );
            }
        } else {
            assert!(!facts.no_callee_saved_registers);
        }
    }

    #[test]
    fn register_contract_requires_parts_and_one_boundary_save_pair() {
        let accepted = "probe_handler:\n\tretq\n.Lfunc_end0:\n\
                        probe_direct:\n\tjmp probe_handler\n.Lfunc_end1:\n\
                        probe_indirect:\n\tjmpq *%rax\n.Lfunc_end2:\n\
                        probe_entry:\n\tpushq %rbp\n\tcallq *%rax\n\tpopq %rbp\n\tretq\n.Lfunc_end3:\n";
        assert!(no_callee_saved_registers_contract(accepted));
        for rejected in [
            accepted.replace("probe_handler:\n", "probe_handler:\n\tpushq %rbp\n"),
            accepted.replace("probe_direct:\n", "probe_direct:\n\tpopq %rbp\n"),
            accepted.replace("probe_indirect:\n", "probe_indirect:\n\tpushq %rbp\n"),
            accepted.replace("probe_handler:\n", "probe_handler:\n\tmovq %rbp, (%rsp)\n"),
            accepted.replace("\tpushq %rbp\n", ""),
            accepted.replace("\tpopq %rbp\n", ""),
            accepted.replace("probe_handler:", "missing_handler:"),
            accepted.replace("probe_direct:", "missing_direct:"),
            accepted.replace("probe_indirect:", "missing_indirect:"),
            accepted.replace("probe_entry:", "missing_entry:"),
        ] {
            assert!(!no_callee_saved_registers_contract(&rejected), "{rejected}");
        }
    }
}
