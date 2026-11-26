//! Host LLVM forms, discovered when the compiler runs rather than when it is built.
//!
//! A released compiler can run on a host whose clang accepts different forms
//! from the build host's clang. Probe the same executable the driver uses,
//! once on the first request for emission facts in each process. A bundled
//! package fixes its clang, so its release records the answers in
//! `toolchain-manifest.json`; a compiler running from the package takes them
//! when they were recorded for the clang it finds, and probes otherwise.

use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

mod recorded_facts;
mod resolution;
pub use resolution::{Toolchain, resolved_toolchain};

/// LLVM forms accepted by the clang on the running compiler's host.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ToolchainFacts {
    pub(crate) preserve_none: bool,
    pub(crate) no_capture_attribute: &'static str,
    pub(crate) coro_end_result: &'static str,
    pub(crate) darwin_stack_probe_helper: bool,
}

/// Where one process's LLVM forms came from.
#[derive(Clone, Debug, Eq, PartialEq)]
enum FactsSource {
    /// The package's manifest recorded them for the clang that was found.
    Manifest,
    /// The clang was probed. `reason` says why a package's recorded answers
    /// were not taken; it is empty for a source build, which has none.
    Probed { reason: Option<String> },
}

/// The forms with where they came from and what finding them cost.
struct ResolvedFacts {
    facts: ToolchainFacts,
    source: FactsSource,
    probes_run: usize,
    /// The first line of the package clang's `--version`; none for a source
    /// build, whose clang is never asked.
    clang_version: Option<String>,
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

    /// The forms the selected clang accepts, found by probing it.
    #[cfg(test)]
    fn probe(toolchain: &Toolchain) -> Result<Self, String> {
        Self::probe_counted(toolchain).map(|(facts, _)| facts)
    }

    /// The forms the selected clang accepts, found by probing it, and how many
    /// probe commands that took.
    fn probe_counted(toolchain: &Toolchain) -> Result<(Self, usize), String> {
        if toolchain.packaged {
            // A failed loader/version launch is not evidence that a newer
            // optional LLVM spelling is unsupported.
            toolchain.cache_identity()?;
        }
        let conservative = Self {
            preserve_none: false,
            no_capture_attribute: "nocapture",
            coro_end_result: "i1",
            darwin_stack_probe_helper: false,
        };
        let directory = match ProbeDirectory::new() {
            Ok(directory) => directory,
            Err(error) if toolchain.packaged => {
                return Err(format!(
                    "cannot create the bundled clang capability-probe directory: {error}"
                ));
            }
            Err(_) => return Ok((conservative, 0)),
        };
        let run = Cell::new(0_usize);
        let required = |result: io::Result<bool>, name: &str| {
            run.set(run.get() + 1);
            match result {
                Ok(accepted) => Ok(accepted),
                Err(error) if toolchain.packaged => Err(format!(
                    "cannot run bundled clang capability probe {name} with {}: {error}",
                    toolchain.clang().display()
                )),
                Err(_) => Ok(false),
            }
        };
        let facts = Self {
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
            // Other hosts keep their existing attribute and run no additional
            // command. Unknown support on Darwin retains inline page probing.
            darwin_stack_probe_helper: {
                #[cfg(target_os = "macos")]
                {
                    required(
                        darwin_stack_probe_helper_supported(toolchain, &directory.0),
                        "__chkstk_darwin",
                    )?
                }
                #[cfg(not(target_os = "macos"))]
                {
                    false
                }
            },
        };
        Ok((facts, run.get()))
    }
}

/// The forms for `toolchain`: a package's recorded answers when they were
/// recorded for the clang it carries, else the probes' answers, with the
/// reason a package's record was not taken.
fn resolve(toolchain: &Toolchain) -> Result<ResolvedFacts, String> {
    if !toolchain.packaged {
        let (facts, probes_run) = ToolchainFacts::probe_counted(toolchain)?;
        return Ok(ResolvedFacts {
            facts,
            source: FactsSource::Probed { reason: None },
            probes_run,
            clang_version: None,
        });
    }
    // A package whose clang cannot even report its version is broken, and no
    // probe answer would be evidence about it.
    let line = toolchain.version_line()?;
    let clang_version = String::from_utf8_lossy(line)
        .trim_end_matches('\r')
        .to_owned();
    match recorded_for(toolchain, &clang_version) {
        Ok(facts) => Ok(ResolvedFacts {
            facts,
            source: FactsSource::Manifest,
            probes_run: 0,
            clang_version: Some(clang_version),
        }),
        Err(reason) => {
            let (facts, probes_run) = ToolchainFacts::probe_counted(toolchain)?;
            Ok(ResolvedFacts {
                facts,
                source: FactsSource::Probed {
                    reason: Some(reason),
                },
                probes_run,
                clang_version: Some(clang_version),
            })
        }
    }
}

/// What the package's manifest records for the clang whose version is
/// `clang_version`, or why it records nothing usable.
fn recorded_for(toolchain: &Toolchain, clang_version: &str) -> Result<ToolchainFacts, String> {
    let path = toolchain
        .manifest()
        .ok_or_else(|| "the installation has no toolchain-manifest.json".to_owned())?;
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    recorded_facts::recorded(&text, clang_version)
}

fn resolved_facts() -> Result<&'static ResolvedFacts, String> {
    static FACTS: OnceLock<Result<ResolvedFacts, String>> = OnceLock::new();
    FACTS
        .get_or_init(|| resolve(resolved_toolchain()?))
        .as_ref()
        .map_err(Clone::clone)
}

fn probed_facts() -> Result<&'static ToolchainFacts, String> {
    resolved_facts().map(|resolved| &resolved.facts)
}

/// Settle resolution and the LLVM forms before any emission reads the
/// infallible facts. Source builds retain their conservative probe-launch
/// fallback; packaged resolution, scratch and launch errors stop with a
/// toolchain diagnostic.
#[doc(hidden)]
pub fn prepare_toolchain() -> Result<(), String> {
    resolved_facts().map(|_| ())
}

/// Where this process's LLVM forms came from and what they are, for the
/// driver's report. `None` before [`prepare_toolchain`] has succeeded.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolchainFactsReport {
    /// `manifest` when the package's recorded answers were taken, `probed`
    /// when the clang was asked.
    pub source: &'static str,
    /// Why a package's recorded answers were not taken, when they were not.
    pub reason: Option<String>,
    /// How many probe commands ran.
    pub probes_run: usize,
    /// The first line of the package clang's `--version`; none for a source
    /// build.
    pub clang_version: Option<String>,
    /// Whether the clang takes `preserve_nonecc` with a guaranteed tail call.
    pub preserve_none: bool,
    /// The spelling of the no-capture parameter attribute.
    pub no_capture_attribute: &'static str,
    /// The result type of `llvm.coro.end`, `void` or `i1`.
    pub coro_end_result: &'static str,
    /// Whether the clang lowers `__chkstk_darwin`; asked on macOS only.
    pub darwin_stack_probe_helper: Option<bool>,
}

/// The report of the forms [`prepare_toolchain`] settled.
#[doc(hidden)]
#[must_use]
pub fn toolchain_facts_report() -> Option<ToolchainFactsReport> {
    resolved_facts().ok().map(report_of)
}

fn report_of(resolved: &ResolvedFacts) -> ToolchainFactsReport {
    let (source, reason) = match &resolved.source {
        FactsSource::Manifest => ("manifest", None),
        FactsSource::Probed { reason } => ("probed", reason.clone()),
    };
    ToolchainFactsReport {
        source,
        reason,
        probes_run: resolved.probes_run,
        clang_version: resolved.clang_version.clone(),
        preserve_none: resolved.facts.preserve_none,
        no_capture_attribute: resolved.facts.no_capture_attribute,
        coro_end_result: resolved.facts.coro_end_result,
        darwin_stack_probe_helper: cfg!(target_os = "macos")
            .then_some(resolved.facts.darwin_stack_probe_helper),
    }
}

/// The sentence the driver prints when a package's recorded answers were not
/// taken and the clang was probed instead.
#[doc(hidden)]
#[must_use]
pub fn toolchain_probe_note() -> Option<String> {
    let report = toolchain_facts_report()?;
    let reason = report.reason?;
    Some(format!(
        "the clang answers recorded for this package were not taken ({reason}); probed the package's clang at run time ({} probes)",
        report.probes_run
    ))
}

/// Internal invariant: every production path prepares before reading facts.
///
/// `backend::emitter::emit_module` prepares before its stack attributes and
/// the private `FunctionEmitter` read reference/environment attributes or
/// coroutine forms. `frames::frame_runtime_declarations` is visible only to
/// that emitter module and is called only from `emit_module`.
/// `handler_words::prepare_dispatch_layout` prepares before its private
/// `census` constructs a `FunctionEmitter` and calls `plan_dispatch`, which
/// reaches the private `dispatch::convention`. `FunctionEmitter::new` is
/// private to the emitter and has no other production construction path.
///
/// `driver::build_module_entry_for_emission` prepares before reading facts for
/// the entry-module cache key. The driver's `compile_selected` reaches the
/// two prepared backend entries above; target-neutral lowering reads no facts.
/// Each preparation propagates failure before these reads, and the shared
/// `OnceLock` retains that result, so packaged failures never reach the expect.
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
    #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
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
    #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
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
    #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
    let accepted = toolchain
        .command()
        .args(["-x", "ir", "-c", "-o"])
        .arg(directory.join("captures_probe.o"))
        .arg(&probe)
        .output()?;
    probe_status(toolchain, accepted.status)
}

/// Some Darwin LLVM consumers accept the helper spelling in IR but reject it
/// during native prologue lowering. Generate an actual large, live frame:
/// passing its address to an unknown callee prevents optimization from
/// deleting or shrinking the allocation. No link or execution is needed.
#[cfg(target_os = "macos")]
fn darwin_stack_probe_helper_supported(toolchain: &Toolchain, directory: &Path) -> io::Result<bool> {
    let probe = directory.join("darwin_stack_probe.ll");
    fs::write(
        &probe,
        "declare void @consume(ptr)\n\n\
         define void @probe() #0 {\n  \
         %frame = alloca [65536 x i8], align 16\n  \
         call void @consume(ptr %frame)\n  ret void\n}\n\n\
         attributes #0 = { \"probe-stack\"=\"__chkstk_darwin\" }\n",
    )?;
    #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
    let accepted = toolchain
        .command()
        .args(["-x", "ir", "-O2", "-c", "-o"])
        .arg(directory.join("darwin_stack_probe.o"))
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

    #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
    fn verify(directory: &Path, name: &str, module: &str) -> io::Result<Output> {
        let input = directory.join(format!("{name}.ll"));
        fs::write(&input, module).expect("write verification module");
        resolved_toolchain()
            .expect("resolved toolchain")
            .command()
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
        prepare_toolchain().expect("prepare host toolchain before reading emission facts");
        let facts = facts();
        let toolchain = resolved_toolchain().expect("prepared toolchain");
        if std::env::var_os("WHITEFOOT_TEST_COMPILER").is_some() {
            assert!(
                toolchain.packaged,
                "a run against a packaged compiler resolves the package's toolchain"
            );
        }
        if toolchain.packaged {
            // The package's answers, taken from its manifest or probed, must
            // be what the package's clang gives when it is asked afresh.
            let asked = ToolchainFacts::probe(toolchain).expect("probe the packaged clang");
            assert_eq!(facts.preserve_none, asked.preserve_none);
            assert_eq!(facts.no_capture_attribute, asked.no_capture_attribute);
            assert_eq!(facts.coro_end_result, asked.coro_end_result);
            assert_eq!(
                facts.darwin_stack_probe_helper,
                asked.darwin_stack_probe_helper
            );
        } else {
            let report = toolchain_facts_report().expect("a prepared toolchain reports");
            assert_eq!(report.source, "probed");
            assert_eq!(report.reason, None, "a source build has no package record");
            assert!(report.probes_run >= 3, "{report:?}");
            #[cfg(target_os = "macos")]
            {
                // Apple clang's native large-frame lowering supports its helper.
                // This also rejects an always-false capability probe: matching
                // emitter text to the same false fact would not expose that bug.
                assert!(
                    facts.darwin_stack_probe_helper,
                    "the macOS source-build default clang must lower __chkstk_darwin"
                );
            }
        }
        let directory = ProbeDirectory::new().expect("create verification directory");
        let chosen = coroutine_module(facts.coro_end_result, facts.no_capture_attribute);
        // The probes' own fallback stands where clang cannot run; this test
        // needs clang as its oracle, as every native test does.
        let accepted = verify(&directory.0, "chosen", &chosen).unwrap_or_else(|error| {
            panic!(
                "cannot run {} to check the probed forms: {error}",
                toolchain.clang().display()
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
        #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
        let tail = toolchain
            .command()
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

    /// Every step that starts clang takes the resolved toolchain, so that a test
    /// binary placed in a package builds with the package's clang. Only the
    /// resolution code names the source-build default, and a path or helper
    /// naming it anywhere else would pass through the host's clang instead.
    #[test]
    fn only_resolution_names_the_source_build_clang() {
        let needles = [["/usr/bin/", "clang"].concat(), ["clang_", "executable"].concat()];
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let resolution = root.join("src/toolchain/resolution.rs");
        let mut pending = vec![root.join("src"), root.join("tests")];
        let mut scanned = 0_usize;
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).expect("read a source directory") {
                let path = entry.expect("read a directory entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|extension| extension == "rs")
                    && path != resolution
                {
                    scanned += 1;
                    let text = fs::read_to_string(&path).expect("read a source file");
                    for needle in &needles {
                        assert!(
                            !text.contains(needle.as_str()),
                            "{} names {needle:?}; start clang through resolved_toolchain()",
                            path.display()
                        );
                    }
                }
            }
        }
        assert!(scanned > 100, "the scan found only {scanned} source files");
    }
}
