//! Host LLVM forms, discovered when the compiler runs rather than when it is built.
//!
//! A released compiler can run on a host whose clang accepts different forms
//! from the build host's clang. Probe the same executable the driver uses,
//! once on the first request for emission facts in each process.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

mod resolution;
pub use resolution::{Toolchain, resolved_toolchain};

/// The source-build default retained by native test helpers.
/// Production commands use [`resolved_toolchain`] instead.
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
    pub(crate) no_capture_attribute: &'static str,
    pub(crate) coro_end_result: &'static str,
}

impl ToolchainFacts {
    /// The call prefix matching the result type probed on this host at run time.
    pub(crate) fn coro_end_call(&self) -> &'static str {
        if self.coro_end_result == "void" {
            "call void"
        } else {
            "%wf.coro.end = call i1"
        }
    }

    fn probe(toolchain: &Toolchain) -> Result<Self, String> {
        if toolchain.packaged {
            // A failed loader/version launch is not evidence that a newer
            // optional LLVM spelling is unsupported.
            toolchain.cache_identity()?;
        }
        let conservative = Self {
            preserve_none: false,
            no_capture_attribute: "nocapture",
            coro_end_result: "i1",
        };
        let directory = match ProbeDirectory::new() {
            Ok(directory) => directory,
            Err(error) if toolchain.packaged => {
                return Err(format!(
                    "cannot create the bundled clang capability-probe directory: {error}"
                ));
            }
            Err(_) => return Ok(conservative),
        };
        let required = |result: io::Result<bool>, name: &str| match result {
            Ok(accepted) => Ok(accepted),
            Err(error) if toolchain.packaged => Err(format!(
                "cannot run bundled clang capability probe {name} with {}: {error}",
                toolchain.clang().display()
            )),
            Err(_) => Ok(false),
        };
        Ok(Self {
            preserve_none: required(
                preserve_none_supported(toolchain, &directory.0),
                "preserve_nonecc",
            )?,
            no_capture_attribute: if required(
                no_capture_supported(toolchain, &directory.0),
                "captures(none)",
            )? {
                "captures(none)"
            } else {
                "nocapture"
            },
            coro_end_result: if required(
                coro_end_void_supported(toolchain, &directory.0),
                "llvm.coro.end",
            )? {
                "void"
            } else {
                "i1"
            },
        })
    }
}

fn probed_facts() -> Result<&'static ToolchainFacts, String> {
    static FACTS: OnceLock<Result<ToolchainFacts, String>> = OnceLock::new();
    FACTS
        .get_or_init(|| ToolchainFacts::probe(resolved_toolchain()?))
        .as_ref()
        .map_err(Clone::clone)
}

/// Settle resolution and probes before any emission reads the infallible facts.
/// Source builds retain their conservative probe-launch fallback; packaged
/// resolution, scratch and launch errors stop with a toolchain diagnostic.
#[doc(hidden)]
pub fn prepare_toolchain() -> Result<(), String> {
    probed_facts().map(|_| ())
}

/// Internal emission invariant: public emission entries first call
/// prepare_toolchain, so packaged failures never reach this accessor.
pub(crate) fn facts() -> &'static ToolchainFacts {
    probed_facts().expect("emission must check prepare_toolchain before accessing LLVM facts")
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
fn coro_end_void_supported(toolchain: &Toolchain, directory: &Path) -> io::Result<bool> {
    let probe = directory.join("coro_end_probe.ll");
    fs::write(
        &probe,
        "declare void @llvm.coro.end(ptr, i1, token)\n\n\
         define void @probe() {\n  \
         call void @llvm.coro.end(ptr null, i1 false, token none)\n  ret void\n}\n",
    )?;
    let accepted = toolchain
        .command()
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
        .output()?;
    probe_status(toolchain, accepted.status)
}

/// Whether the host toolchain probed at run time accepts a
/// guaranteed tail call between functions of the calling convention without
/// callee-saved registers (compiler/match-dispatch-lowering).
///
/// LLVM 19 added the convention, and no version is pinned, so it is probed:
/// a two-function module with the convention and a `musttail` call between
/// them is handed to the assembler `whitefootc` runs. Where it is refused,
/// or no assembler can be run, the lowering uses the C convention.
fn preserve_none_supported(toolchain: &Toolchain, directory: &Path) -> io::Result<bool> {
    let probe = directory.join("preserve_none_probe.ll");
    fs::write(
        &probe,
        "define internal preserve_nonecc i64 @q(i64 %a) {\n  ret i64 %a\n}\n\n\
         define preserve_nonecc i64 @p(i64 %a) {\n  \
         %r = musttail call preserve_nonecc i64 @q(i64 %a)\n  ret i64 %r\n}\n",
    )?;
    let accepted = toolchain
        .command()
        .args(["-x", "ir", "-c", "-o"])
        .arg(directory.join("preserve_none_probe.o"))
        .arg(&probe)
        .output()?;
    probe_status(toolchain, accepted.status)
}

/// Which spelling of the no-capture parameter attribute the host toolchain
/// accepts at run time (compiler/backend-facts).
///
/// LLVM 21 renamed `nocapture` to `captures(none)`. No version is pinned, so
/// the spelling is probed rather than assumed: a one-function module carrying
/// the new spelling is handed to the assembler, and the old spelling is used
/// when it is refused. The old spelling is also the fallback where no
/// assembler can be run at all, because every LLVM that has the new one still
/// auto-upgrades the old.
fn no_capture_supported(toolchain: &Toolchain, directory: &Path) -> io::Result<bool> {
    const NEW: &str = "captures(none)";
    let probe = directory.join("captures_probe.ll");
    fs::write(
        &probe,
        format!("define void @p(ptr {NEW} %v) {{\n  ret void\n}}\n"),
    )?;
    let accepted = toolchain
        .command()
        .args(["-x", "ir", "-c", "-o"])
        .arg(directory.join("captures_probe.o"))
        .arg(&probe)
        .output()?;
    probe_status(toolchain, accepted.status)
}

fn probe_status(toolchain: &Toolchain, status: std::process::ExitStatus) -> io::Result<bool> {
    if toolchain.packaged && !matches!(status.code(), Some(0 | 1)) {
        return Err(io::Error::other(format!(
            "capability probe terminated with {status}"
        )));
    }
    Ok(status.success())
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
    }
}
