#![allow(clippy::panic)]

// Retired with [STOR-4]: the `arenas` module's three tests all had the
// region-tied bump extent as their subject - `arena_new`, the `{ ptr, i8 }`
// arena node's layout qualification and `malloc` size expression, the
// `region 'r { .. }` block, and the `stor4-pos-arena-confined` conformance
// case the last of them read. v0.60 has no regions, no arenas and no [BLK-2]
// reservation, and that conformance case left the corpus with the rule. The
// successor is ordinary `Slots` storage built by the construction functions
// [OP-13] over the one heap [STOR-8]: a `Box` cell's own layout qualification
// and emission belong with the other storage shapes, and a frame-resident
// constant-capacity `Slots` [TYPE-9] needs no reservation of its own.
mod arithmetic_obligations;
mod arrays;
mod base64;
mod checked_division;
mod completion;
mod containers;
/// The §9.1 cost census, every case of which compiles `wfgrep`.
mod cost_shape;
mod counted_ranges;
mod deterministic_target;
mod division_obligations;
mod effect_attributes;
mod enumeration_records;
mod exhaustion;
mod float_conversion;
mod floating;
mod generics;
mod heap_programs;
mod integer_absolute;
mod integer_conversion;
mod integer_extended;
mod integer_negation;
mod loop_split;
mod owned_places;
mod parallel;
/// Range references over the three storage origins [REF-4, STOR-1], and the
/// compute kernels that take a range of work.
///
/// This module was `slices`. `Slice<T>` and `MutSlice<T>` are not types in
/// v0.60 - [REF-4]'s `&[T]` is a reference kind admitted only in parameter
/// position - so the name went with them, and four of its tests went with the
/// rules [VIEW-1], [VIEW-4] and [VIEW-6] gave them. Each retirement is
/// recorded beside the tests that replace it inside the module.
mod ranges;
// Retired with [OWN-6] and [OWN-14]: the `reborrows` module's four tests all
// had the reborrow as their subject - a callee taking `&uniq 'r T` and
// returning `&uniq 'r deref(target)`, the child chain through a `box<u64>`
// field, and the test-only reborrow-extension checker entry
// `emit_reborrow_extension` the last of them read. v0.60 has no permission
// markers, no region parameters and no loan extension: [REF-1] makes a
// reference a local name for a path and a step below it another path, so
// there is nothing to reborrow and nothing to emit, and [REF-3] refuses a
// returned reference outright with the restructuring `return an index and let
// the caller form the reference`. The write-back these cases observed is
// [EFF-5]'s substituted `writes` through an ordinary `&T` parameter, covered
// by `owned_places` and by the `references` semantic suite; the `replace`
// exchange they used is [SET-1] with [WIN-3]'s disposition, or [OP-11]'s
// `swap`.
mod reinterpret;
mod requires;
mod resource_enums;
mod result_abi;
mod stack_ledger;
mod system;
mod tail_calls;
mod target_frame;
/// [TYPE-9]'s storage shapes and the cell as the backend emits them: their
/// construction [OP-13], target qualification [STOR-6, OP-9] and
/// compiler-derived release [STOR-3, WIN-3].
///
/// This module was `buffers`. The `buffer<T>` storage class and its
/// `buffer_new` / `buffer_vacant` heads have no v0.60 spelling, and two of its
/// tests retired with the fallible store take [BLK-2] and the vacant run;
/// each retirement is recorded inside the module.
mod windows;
// Runtime source-claim tests were retired with the source-claim instruction:
// proofs are checked before lowering, so no backend latch or IR-mutation
// path exists for them. Resource-record latch coverage remains in `exhaustion`.

use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::lexer::{LexLimits, LexOutcome, lex};
use crate::{
    ACTIVE_KERNEL_SPEC_HASH, COMPLETION_BRIDGE_HEADER, COMPLETION_BRIDGE_SOURCE,
    COMPLETION_CONTRACT_HEADER, COMPLETION_FILE_ADAPTER_HEADER, COMPLETION_FILE_ADAPTER_SOURCE,
    COMPLETION_FILE_POSIX_HEADER, COMPLETION_FILE_POSIX_SOURCE, COMPLETION_LINUX_IO_URING_HEADER,
    COMPLETION_LINUX_IO_URING_SOURCE, COMPLETION_RUNTIME_SOURCE, COMPLETION_SOCKET_ADDRESS_HEADER,
    COMPLETION_WAIT_HOST_SOURCE, COMPLETION_WINDOWS_IOCP_HEADER, CanonicalLimits, CanonicalOutcome,
    FLOOR_RUNTIME_SOURCE, FinalizeLimits, FinalizeOutcome, HOST_LINK_LIBRARIES,
    HOST_OPTIMIZATION_ARGUMENTS, ORDINARY_VALUES_HEADER, ORDINARY_VALUES_LLVM,
    ORDINARY_VALUES_SOURCE, OverlapLowering, ParseLimits, ParseOutcome, ResolutionOutcome,
    SCHED_CORE_HEADER, SCHED_CORE_SOURCE, SCHED_ENTRY_HEADER, SCHED_ENTRY_SOURCE,
    SCHED_PRIM_HEADER, SCHED_PRIM_HOST_SOURCE, SemanticOutcome, SourceBundle, SourceInput,
    SourceLimits, TerminalLimits, TerminalOutcome, WINDOWS_RUNTIME_HEADER, audit_canonical,
    check_semantics_arithmetic_obligations, check_semantics_division_obligations,
    classify_terminals, compile as compile_program, emit_llvm, finalize, lower_checked,
    module_requires_parallel_runtime, parse, resolve,
};

const SOURCE_LIMITS: SourceLimits = SourceLimits {
    max_sources: 1_024,
    max_logical_path_bytes: 128,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_binding_bytes: 1_048_576,
};

const LEX_LIMITS: LexLimits = LexLimits {
    max_sources: 1_024,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_token_bytes: 16_384,
    max_tokens: 131_072,
    max_lexemes: 262_144,
};

const PARSE_LIMITS: ParseLimits = ParseLimits {
    max_work: 8_000_000,
    max_tasks: 131_072,
    max_frames: 8_192,
    max_elements: 262_144,
};

const FINALIZE_LIMITS: FinalizeLimits = FinalizeLimits {
    max_work: 8_000_000,
    max_roots: 131_072,
    max_shape_tasks: 131_072,
    max_nodes: 131_072,
    max_child_edges: 131_072,
    max_terminals: 131_072,
    max_sources: 1_024,
};

const CANONICAL_LIMITS: CanonicalLimits = CanonicalLimits {
    max_work: 8_000_000,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_gaps: 131_072,
    max_path_components: 8_192,
};

static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

/// The shipped default compilation, with ordinary overlap actualization off.
///
/// It is also the only *non-outlined* reference on the parallel path. Every
/// comparison that links one emitted module two ways has a defect in the
/// lowering itself on both sides and cannot see it; emitting one source both
/// ways and byte-comparing the two programs is what makes an overlap's
/// "changes nothing observable" claim a statement about the lowering rather
/// than about the linker.
fn emit(source: &[u8]) -> String {
    emit_lowered(source, OverlapLowering::Off)
}

/// [`emit`] with the [PAR-1 candidate] overlap lowering switched on, which is
/// what `whitefootc --par --par-scalar-leaf-limit off` compiles.
fn emit_with_overlap(source: &[u8]) -> String {
    emit_lowered(source, OverlapLowering::On)
}

/// The developer-channel permission ledger of one source, compiled the way
/// `whitefootc --par --par-scalar-leaf-limit off --par-ledger` compiles it.
///
/// It carries the judgment's own lines, which are the same with or without
/// `--par`, and after them the lines this lowering added about what it
/// actualized — which only a compilation that asked for actualization has.
fn compile_permission_ledger(source: &[u8]) -> Vec<String> {
    let inputs = [SourceInput::new("test.wf", source)];
    let (_, ledger) = crate::compile_with_permission_ledger(
        &inputs,
        crate::CompilerLimits::default(),
        OverlapLowering::On,
    )
    .expect("the ledger source must compile");
    ledger
}

/// The shared front half: check `source`, then lower and emit it under one
/// named overlap-lowering choice.
fn emit_lowered(source: &[u8], overlap: OverlapLowering) -> String {
    let inputs = [SourceInput::new("test.wf", source)];
    crate::compile_with_overlap(&inputs, crate::CompilerLimits::default(), overlap)
        .expect("ordinary compiler and executable builder must emit")
}

/// [`emit`] through the test-only checker entry that forces the
/// arithmetic-mode dissolution switch on [OP-2, ENT-6], so the emitted
/// module of the v0.31 candidate judgment can be compared against the
/// default v0.30 emission of the same source.
fn emit_arithmetic_obligations(source: &[u8]) -> String {
    let inputs = [SourceInput::new("test.wf", source)];
    let bundle = SourceBundle::with_prelude(&inputs, SOURCE_LIMITS).expect("valid test bundle");
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("backend test source must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) else {
        panic!("backend test source must classify");
    };
    let ParseOutcome::Complete(parsed) = parse(classified, PARSE_LIMITS) else {
        panic!("backend test source must parse");
    };
    let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
        panic!("backend test source must finalize");
    };
    let CanonicalOutcome::Complete(canonical) = audit_canonical(*finalized, CANONICAL_LIMITS)
    else {
        panic!("backend test source must be canonical");
    };
    let ResolutionOutcome::Complete(resolved) = resolve(canonical) else {
        panic!("backend test source must resolve");
    };
    let SemanticOutcome::Complete(checked) = check_semantics_arithmetic_obligations(&resolved)
    else {
        panic!("backend test source must check under the arithmetic switch");
    };
    assert!(
        checked
            .data
            .functions
            .iter()
            .filter(|function| function.name == "main")
            .all(|function| function.requirements.is_empty()),
        "the test build caller must discharge every selected precondition"
    );
    let ir = lower_checked(*checked, OverlapLowering::Off).expect("checked program must lower");
    let mut llvm = emit_llvm(&ir)
        .expect("lowered program must emit")
        .into_string();
    llvm.push_str(&crate::driver::launcher::render(&ir, "main").expect("ordinary test launcher"));
    llvm
}

/// [`emit`] through the test-only checker entry that forces the division
/// dissolution switch on [OP-2, ENT-6]. The shipped switch is on too, so this
/// entry emits from the same judgment as [`emit`] and records which judgment
/// its callers mean.
fn emit_division_obligations(source: &[u8]) -> String {
    let inputs = [SourceInput::new("test.wf", source)];
    let bundle = SourceBundle::with_prelude(&inputs, SOURCE_LIMITS).expect("valid test bundle");
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("backend test source must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) else {
        panic!("backend test source must classify");
    };
    let ParseOutcome::Complete(parsed) = parse(classified, PARSE_LIMITS) else {
        panic!("backend test source must parse");
    };
    let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
        panic!("backend test source must finalize");
    };
    let CanonicalOutcome::Complete(canonical) = audit_canonical(*finalized, CANONICAL_LIMITS)
    else {
        panic!("backend test source must be canonical");
    };
    let ResolutionOutcome::Complete(resolved) = resolve(canonical) else {
        panic!("backend test source must resolve");
    };
    let SemanticOutcome::Complete(checked) = check_semantics_division_obligations(&resolved) else {
        panic!("backend test source must check under the division switch");
    };
    assert!(
        checked
            .data
            .functions
            .iter()
            .filter(|function| function.name == "main")
            .all(|function| function.requirements.is_empty()),
        "the test build caller must discharge every selected precondition"
    );
    let ir = lower_checked(*checked, OverlapLowering::Off).expect("checked program must lower");
    let mut llvm = emit_llvm(&ir)
        .expect("lowered program must emit")
        .into_string();
    llvm.push_str(&crate::driver::launcher::render(&ir, "main").expect("ordinary test launcher"));
    llvm
}

fn compile(source: &[u8]) -> String {
    compile_sources(&[("test.wf", source)])
}

// `emit_reborrow_extension` retired with the `reborrows` module above: it was
// the test-only entry for the [OWN-6, OWN-14] reborrow-extension checker, and
// v0.60 has no loan extension for a second checker entry to name. The ordinary
// `emit` is the only judgment left for a reference-carrying source.

/// Compiles a source that must be rejected, returning the failure for rule
/// and detail assertions.
fn compile_rejection(source: &[u8]) -> crate::CompilationFailure {
    let inputs = [SourceInput::new("test.wf", source)];
    compile_program(&inputs, crate::CompilerLimits::default()).expect_err("source must be rejected")
}

fn compile_sources(sources: &[(&str, &[u8])]) -> String {
    let inputs = sources
        .iter()
        .map(|(path, source)| SourceInput::new(path, source))
        .collect::<Vec<_>>();
    crate::native_test_support::timed("whitefoot-compile", || {
        compile_program(&inputs, crate::CompilerLimits::default())
            .expect("normal compiler pipeline must emit")
    })
}

fn compile_and_run(llvm: &str) -> std::process::Output {
    compile_and_run_with(llvm, &[])
}

/// Creates one fresh directory for a test's own artifacts.
fn test_directory() -> PathBuf {
    let sequence = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "whitefoot-backend-test-{}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).expect("unique backend test directory");
    directory
}

/// Links one emitted module into an executable inside `directory`.
fn build_executable(llvm: &str, directory: &Path) -> PathBuf {
    build_linked_executable(llvm, None, &[], directory)
}

fn append_runtime_units_with_library_defines(
    command: &mut Command,
    directory: &Path,
    library_defines: &[String],
) -> Option<Vec<&'static str>> {
    let units = [
        ("ordinary_values.h", ORDINARY_VALUES_HEADER),
        ("ordinary_values.c", ORDINARY_VALUES_SOURCE),
        ("ordinary_values.ll", ORDINARY_VALUES_LLVM),
        ("sched/core.h", SCHED_CORE_HEADER),
        ("sched/prim.h", SCHED_PRIM_HEADER),
        ("sched/entry.h", SCHED_ENTRY_HEADER),
        ("sched/core.c", SCHED_CORE_SOURCE),
        ("sched/prim_host.c", SCHED_PRIM_HOST_SOURCE),
        ("sched/entry.c", SCHED_ENTRY_SOURCE),
        ("completion/contract.h", COMPLETION_CONTRACT_HEADER),
        ("completion/file_adapter.h", COMPLETION_FILE_ADAPTER_HEADER),
        ("completion/bridge.h", COMPLETION_BRIDGE_HEADER),
        ("completion/file_posix.h", COMPLETION_FILE_POSIX_HEADER),
        (
            "completion/socket_address.h",
            COMPLETION_SOCKET_ADDRESS_HEADER,
        ),
        (
            "completion/linux_io_uring.h",
            COMPLETION_LINUX_IO_URING_HEADER,
        ),
        ("completion/windows_iocp.h", COMPLETION_WINDOWS_IOCP_HEADER),
        ("windows_runtime.h", WINDOWS_RUNTIME_HEADER),
        ("completion/completion_runtime.c", COMPLETION_RUNTIME_SOURCE),
        ("completion/wait_host.c", COMPLETION_WAIT_HOST_SOURCE),
        ("completion/file_adapter.c", COMPLETION_FILE_ADAPTER_SOURCE),
        ("completion/file_posix.c", COMPLETION_FILE_POSIX_SOURCE),
        ("completion/completion_bridge.c", COMPLETION_BRIDGE_SOURCE),
        (
            "completion/linux_io_uring.c",
            COMPLETION_LINUX_IO_URING_SOURCE,
        ),
    ];
    std::fs::create_dir_all(directory.join("completion")).expect("stage completion directory");
    std::fs::create_dir_all(directory.join("sched")).expect("stage scheduler directory");
    for (name, source) in units {
        std::fs::write(directory.join(name), source).expect("write ordinary library unit");
    }
    let mut artifacts: Vec<_> = units.into_iter().map(|(name, _)| name).collect();
    command
        .arg("-I")
        .arg(directory)
        .arg("-I")
        .arg(directory.join("completion"));
    for (name, _) in units {
        if name.ends_with(".h") || (name == "ordinary_values.c" && !library_defines.is_empty()) {
            continue;
        }
        command
            .arg("-x")
            .arg(if name.ends_with(".ll") { "ir" } else { "c" })
            .arg(directory.join(name));
    }
    if !library_defines.is_empty() {
        // A scripted linked body interposes this library translation unit's
        // private dependencies. The WF module and native engine stay unchanged.
        let mut library = Command::new("/usr/bin/clang");
        library
            .arg("-std=c11")
            .arg("-c")
            .arg("-I")
            .arg(directory)
            .args(HOST_OPTIMIZATION_ARGUMENTS);
        for define in library_defines {
            library.arg(format!("-D{define}"));
        }
        let object = directory.join("ordinary_values.o");
        let reached = library
            .arg(directory.join("ordinary_values.c"))
            .arg("-o")
            .arg(&object)
            .output()
            .expect("compile scripted library body");
        assert!(
            reached.status.success(),
            "{}",
            String::from_utf8_lossy(&reached.stderr)
        );
        command.arg("-x").arg("none").arg(&object);
        artifacts.push("ordinary_values.o");
    }
    Some(artifacts)
}

/// Links one emitted module, optionally with one host translation unit, into
/// an executable inside `directory`.
///
/// The same ordinary emitted module links against the host's own facilities,
/// or an explicit test unit supplies scripted native facility results.
///
/// `defines` reaches the compiler-owned C units only, through the same
/// facility-substitution macros `compiler/Makefile` uses for the completion
/// harness. It changes which host call one adapter reaches, never the emitted
/// module and never the adapter's own logic, so a case that scripts a
/// facility still observes the shipped decoder over the bytes that facility
/// left behind.
fn build_linked_executable(
    llvm: &str,
    host: Option<&str>,
    defines: &[String],
    directory: &Path,
) -> PathBuf {
    build_linked_executable_with_library_defines(llvm, host, defines, &[], directory)
}

pub(super) fn build_linked_executable_with_library_defines(
    llvm: &str,
    host: Option<&str>,
    defines: &[String],
    library_defines: &[String],
    directory: &Path,
) -> PathBuf {
    crate::native_test_support::timed("native-build", || {
        build_linked_executable_inner(llvm, host, defines, library_defines, directory)
    })
}

fn build_linked_executable_inner(
    llvm: &str,
    host: Option<&str>,
    defines: &[String],
    library_defines: &[String],
    directory: &Path,
) -> PathBuf {
    let module = directory.join("program.ll");
    let executable = directory.join("program");
    std::fs::write(&module, llvm).expect("write backend test module");
    let mut command = Command::new("/usr/bin/clang");
    // The dialect the driver names, for the same reason: a test link must not
    // compile the compiler-owned C units in a dialect the shipped one does not.
    command.arg("-std=c11");
    for define in defines {
        command.arg(format!("-D{define}"));
    }
    command.arg("-x").arg("ir").arg(&module);
    let host_unit = host.map(|source| {
        let path = directory.join("host.c");
        std::fs::write(&path, source).expect("write deterministic host unit");
        path
    });
    if let Some(path) = host_unit.as_ref() {
        command.arg("-x").arg("c").arg(path);
    }
    // The exhaustion floor joins every link, exactly as the driver links it:
    // every program can run out of stack, so a test program dies the way a
    // shipped one does.
    command.arg("-pthread");
    // Every executable links the ordinary library and its private runtime
    // dependencies, using the same build inputs as the driver. Source
    // classification never selects a second linkage or callable ABI.
    let mut staged_units = Vec::new();
    if defines.is_empty() && library_defines.is_empty() {
        // These inputs and options are immutable for this test executable.
        // Keep each program and observer fresh, but compile the ordinary
        // library once. Macro-interposed cases retain their own C build below.
        let (sources, objects) = crate::native_test_support::append_runtime_objects(
            &mut command,
            directory,
            Some("c11"),
            None,
        );
        staged_units.extend(sources);
        staged_units.extend(objects);
    } else {
        let floor_unit = directory.join("wf_floor.c");
        std::fs::write(&floor_unit, FLOOR_RUNTIME_SOURCE).expect("write the floor runtime");
        command.arg("-x").arg("c").arg(&floor_unit);
        staged_units.push(floor_unit);
        if let Some(names) =
            append_runtime_units_with_library_defines(&mut command, directory, library_defines)
        {
            staged_units.extend(names.into_iter().map(|name| directory.join(name)));
        }
    }
    let compile = command
        .args(HOST_OPTIMIZATION_ARGUMENTS)
        .args(HOST_LINK_LIBRARIES)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("invoke host clang");
    if !compile.status.success() {
        panic!(
            "clang rejected emitted LLVM:\n{}\n{}",
            String::from_utf8_lossy(&compile.stderr),
            llvm
        );
    }
    std::fs::remove_file(&module).expect("remove backend test module");
    if let Some(path) = host_unit {
        std::fs::remove_file(path).expect("remove deterministic host unit");
    }
    for path in staged_units {
        std::fs::remove_file(path).expect("remove native runtime build input");
    }
    for staged in ["completion", "sched"] {
        std::fs::remove_dir(directory.join(staged)).expect("remove staged runtime directory");
    }
    executable
}

/// Runs one emitted module with exact argument bytes.
///
/// The bytes are passed as raw `OsStr`s so a test can hand the program an
/// argument that is not valid text [HOST-1].
fn compile_and_run_with(llvm: &str, arguments: &[&[u8]]) -> std::process::Output {
    compile_link_and_run(llvm, None, arguments)
}

/// Runs one emitted module, optionally linked against one host translation
/// unit.
fn compile_link_and_run(
    llvm: &str,
    host: Option<&str>,
    arguments: &[&[u8]],
) -> std::process::Output {
    let directory = test_directory();
    let executable = build_linked_executable(llvm, host, &[], &directory);
    let output = crate::native_test_support::timed("native-run", || {
        Command::new(&executable)
            .args(
                arguments
                    .iter()
                    .map(|bytes| std::ffi::OsStr::from_bytes(bytes)),
            )
            .output()
            .expect("run backend test executable")
    });
    std::fs::remove_file(&executable).expect("remove backend test executable");
    std::fs::remove_dir(&directory).expect("remove backend test directory");
    output
}

/// Runs one emitted module linked against one host translation unit and one
/// set of facility-substitution definitions, inside a directory of its own.
///
/// The program's working directory is the test's own directory rather than
/// the test runner's, because a case reaching this helper may end a program
/// at the trusted-computing-base defect arm and a host that writes a core
/// file writes it into the working directory of the process that died. The
/// directory is therefore removed whole rather than file by file.
fn compile_link_and_run_with(
    llvm: &str,
    host: Option<&str>,
    defines: &[String],
    arguments: &[&[u8]],
) -> std::process::Output {
    let directory = test_directory();
    let executable = build_linked_executable(llvm, host, defines, &directory);
    let output = crate::native_test_support::timed("native-run", || {
        Command::new(&executable)
            .current_dir(&directory)
            .args(
                arguments
                    .iter()
                    .map(|bytes| std::ffi::OsStr::from_bytes(bytes)),
            )
            .output()
            .expect("run backend test executable")
    });
    std::fs::remove_dir_all(&directory).expect("remove backend test directory");
    output
}

/// Returns the module as the host optimizer leaves it at the shipped level.
fn host_optimized_module(llvm: &str) -> String {
    let mut child = Command::new("/usr/bin/clang")
        .arg("-x")
        .arg("ir")
        .arg("-")
        .args(HOST_OPTIMIZATION_ARGUMENTS)
        .arg("-S")
        .arg("-emit-llvm")
        .arg("-o")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("invoke host clang");
    child
        .stdin
        .take()
        .expect("clang stdin must be available")
        .write_all(llvm.as_bytes())
        .expect("send module to host clang");
    let output = child.wait_with_output().expect("wait for host clang");
    if !output.status.success() {
        panic!(
            "clang rejected emitted LLVM:\n{}\n{llvm}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout).expect("optimized module is UTF-8")
}

/// Returns the definition of the program's entry inside one optimized module.
///
/// This is `@wf__main_body`, not `@main`. Since the exhaustion floor landed,
/// `@main` keeps the host's entry signature and does one thing — hand the
/// program to the floor runtime, which runs it on a stack the compiler sized.
/// The bootstrap, the inputs, and the call into the program all live in the
/// body, so the body is what every check about the entry's shape reads. The
/// caller's assertions are unchanged by the move; only where they look is.
fn optimized_main(module: &str) -> &str {
    let start = module
        .match_indices(" @wf__main_body(")
        .find_map(|(symbol_start, _)| {
            let line_start = module[..symbol_start]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            module[line_start..symbol_start]
                .starts_with("define")
                .then_some(line_start)
        })
        .expect("optimized module must still define the entry body");
    let end = module[start..]
        .find("\n}\n")
        .map(|offset| start + offset + 2)
        .expect("main definition must close");
    &module[start..end]
}

/// The LLVM type name the emitter gives the nominal whose stable spelling is
/// `spelling`: `Name` for a root-module declaration, `a.b.Name` for one of
/// module `pkg::a::b`, then its arguments [MOD-8].
pub(super) fn nominal_type(spelling: &str) -> String {
    format!("%wf.t.{}", link_name(spelling))
}

/// The LLVM global the emitter gives the constant named `spelling` [MOD-8].
pub(super) fn constant_global(spelling: &str) -> String {
    format!("@.wf_const.{}", link_name(spelling))
}

fn link_name(spelling: &str) -> String {
    crate::spec::sha256::digest(spelling.as_bytes())[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn emitted_function<'module>(module: &'module str, name: &str) -> &'module str {
    let symbol = format!(" @wf_{name}(");
    let function_start = module
        .match_indices(&symbol)
        .find_map(|(symbol_start, _)| {
            let line_start = module[..symbol_start]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            module[line_start..symbol_start]
                .starts_with("define ")
                .then_some(line_start)
        })
        .unwrap_or_else(|| panic!("missing emitted function {name}"));
    let function_end = module[function_start..]
        .find("\n}\n\n")
        .map(|offset| function_start + offset + 3)
        .expect("source function definition must close");
    &module[function_start..function_end]
}

/// The definition that carries one source function's emitted body: the
/// function's own definition, or, for a result returned in registers, the
/// internal destination-form body its public entry calls
/// (compiler/src/backend/abi.rs).
fn emitted_body<'module>(module: &'module str, name: &str) -> &'module str {
    let body = format!("{name}.body");
    if module.contains(&format!(" @wf_{body}(")) {
        emitted_function(module, &body)
    } else {
        emitted_function(module, name)
    }
}

/// The first-class aggregate type one emitted definition returns in
/// registers (compiler/src/backend/abi.rs), checked against its expected
/// scalar fields independently of the module's nominal numbering.
fn register_result_type<'function>(
    module: &str,
    function: &'function str,
    fields: &[&str],
) -> &'function str {
    let (result_type, _) = function
        .strip_prefix("define ")
        .and_then(|header| header.split_once(" @"))
        .expect("an emitted definition header");
    assert!(
        result_type.starts_with("%wf.t"),
        "the result returns as its named aggregate: {result_type}"
    );
    assert!(
        module.contains(&format!("{result_type} = type {{ {} }}", fields.join(", "))),
        "the returned aggregate has the expected scalar layout: {result_type}"
    );
    result_type
}

/// These fixtures construct every variant of a result with scalar fields.
/// Check the typed `%wf.result` construction and field writes independently
/// of a preliminary whole-aggregate store or the module's nominal numbering.
/// `function` is the definition that constructs the result, as
/// [`emitted_body`] finds it, and `%wf.result` is its destination.
fn assert_scalar_result_fields(module: &str, function: &str, fields: &[&str]) {
    let mut initialized = vec![false; fields.len()];
    for line in function.lines() {
        let Some((address, operation)) = line.trim().split_once(" = ") else {
            continue;
        };
        let Some(projection) = operation.strip_prefix("getelementptr inbounds ") else {
            continue;
        };
        let Some((result_type, field)) = projection.split_once(", ptr %wf.result, i32 0, i32 ")
        else {
            continue;
        };
        assert!(
            module.contains(&format!("{result_type} = type {{ {} }}", fields.join(", "))),
            "the result destination has the expected scalar layout: {line}"
        );
        let field = field.parse::<usize>().expect("result field ordinal");
        let field_type = fields.get(field).expect("declared result field");
        assert!(
            function.lines().any(|store| {
                store.trim().starts_with(&format!("store {field_type} "))
                    && store.ends_with(&format!(", ptr {address}"))
            }),
            "the selected result field is initialized: {line}"
        );
        initialized[field] = true;
    }
    assert!(
        initialized.iter().all(|field| *field),
        "the fixture writes the tag and every variant's scalar payload: {initialized:?}"
    );
}

/// One monomorphized instance of a compiler-owned [PRE-1] record.
///
/// Those records are emitted as ordinary out-of-line bodies, one per instance
/// (compiler/prelude-records), so the storage a construction row allocates and
/// the descriptor words a window row writes are in the row's own definition
/// rather than at the call.
fn emitted_prelude_row<'module>(module: &'module str, row: &str) -> &'module str {
    let symbol = format!(" @wf_{row}$instance$");
    let function_start = module
        .match_indices(&symbol)
        .find_map(|(symbol_start, _)| {
            let line_start = module[..symbol_start]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            module[line_start..symbol_start]
                .starts_with("define ")
                .then_some(line_start)
        })
        .unwrap_or_else(|| panic!("missing emitted prelude row {row}"));
    let function_end = module[function_start..]
        .find("\n}\n\n")
        .map(|offset| function_start + offset + 3)
        .expect("prelude row definition must close");
    &module[function_start..function_end]
}

fn emitted_drop_ids(function: &str) -> Vec<u32> {
    function
        .lines()
        .filter_map(|line| line.strip_prefix("  ; drop %v"))
        .map(|ordinal| ordinal.parse().expect("drop value must have an ordinal"))
        .collect()
}

#[test]
fn nominal_lowering_keeps_selected_tag_widths_and_initialized_payloads() {
    let source = br#"enum Flag {
  Off();
  On();
}

enum Payload {
  Empty();
  Value(number: i32);
  Wide(first: u64, last: u8);
}

fn empty_payload() -> result: Payload pure {
  return Payload::Empty();
}

fn number_payload() -> result: Payload pure {
  return Payload::Value(number: 42_i32);
}

fn wide_payload() -> result: Payload pure {
  return Payload::Wide(first: 511_u64, last: 127_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  let flag = Flag::On();
  match flag {
    Off() => {
      return std::process::exit_status(code: 1_u8);
    }
    On() => {
    }
  }
  let payload = number_payload();
  match payload {
    Empty() => {
      return std::process::exit_status(code: 2_u8);
    }
    Value(number: value) => {
      if value != 42_i32 {
        return std::process::exit_status(code: 3_u8);
      }
    }
    Wide(first: first_word, last: last_byte) => {
      return std::process::exit_status(code: 4_u8);
    }
  }
  match empty_payload() {
    Empty() => {
    }
    Value(number: value) => {
      return std::process::exit_status(code: 5_u8);
    }
    Wide(first: first_word, last: last_byte) => {
      return std::process::exit_status(code: 6_u8);
    }
  }
  match wide_payload() {
    Empty() => {
      return std::process::exit_status(code: 7_u8);
    }
    Value(number: value) => {
      return std::process::exit_status(code: 8_u8);
    }
    Wide(first: first_word, last: last_byte) => {
      if first_word != 511_u64 {
        return std::process::exit_status(code: 9_u8);
      }
      if last_byte != 127_u8 {
        return std::process::exit_status(code: 10_u8);
      }
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = emit(source);
    assert!(llvm.contains("switch i1"));
    assert!(llvm.contains("switch i32"));
    let constructors: [(&str, u32, &[usize]); 3] = [
        ("empty_payload", 0, &[]),
        ("number_payload", 1, &[1]),
        ("wide_payload", 2, &[2, 3]),
    ];
    for (name, tag, selected) in constructors {
        let body = emitted_function(&llvm, name);
        assert!(
            body.contains(&format!(
                "store {} zeroinitializer, ptr %wf.result",
                nominal_type("Payload")
            )),
            "the baseline constructor initializes the complete result: {body}"
        );
        assert!(!body.contains("poison"), "{body}");
        assert!(!body.contains("undef"), "{body}");
        for (field, field_type) in ["i32", "i32", "i64", "i8"].iter().enumerate() {
            let address = body.lines().find_map(|line| {
                let (address, operation) = line.trim().split_once(" = ")?;
                (operation.starts_with(&format!(
                    "getelementptr inbounds {}, ptr ",
                    nominal_type("Payload")
                )) && operation.ends_with(&format!(", i32 0, i32 {field}")))
                .then_some(address)
            });
            if field != 0 && !selected.contains(&field) {
                assert!(
                    address.is_none(),
                    "only selected fields receive writes after aggregate initialization: {body}"
                );
                continue;
            }
            let address =
                address.expect("the tag and each selected payload field have a destination");
            let stored = body
                .lines()
                .find(|line| {
                    line.trim().starts_with(&format!("store {field_type} "))
                        && line.ends_with(&format!(", ptr {address}"))
                })
                .expect("the tag and each selected payload field are initialized");
            if field == 0 {
                assert_eq!(stored.trim(), format!("store i32 {tag}, ptr {address}"));
            }
        }
    }
    assert!(llvm.contains("call void @abort()"));
    assert!(!llvm.contains(&format!("{} = type", nominal_type("Flag"))));
    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn checked_affine_cleanup_survives_lowering_and_emission() {
    let source = br#"nocopy struct Cell {
  value: i32;
}

struct Inner {
  selected: Cell;
  sibling: Cell;
}

struct Outer {
  inner: Inner;
  sibling: Cell;
}

enum Holder {
  Held(cell: Cell);
  Empty();
}

fn make() -> result: Cell pure {
  let cell = Cell(value: 1_i32);
  return move cell;
}

fn cleanup() -> result: unit pure {
  make();
  let first = Cell(value: 2_i32);
  let second = Cell(value: 3_i32);
  let selected = Cell(value: 4_i32);
  let inner_sibling = Cell(value: 5_i32);
  let inner = Inner(selected: move selected, sibling: move inner_sibling);
  let outer_sibling = Cell(value: 6_i32);
  let outer = Outer(inner: move inner, sibling: move outer_sibling);
  let taken = move outer.inner.selected;
  return unit;
}

fn cleanup_match(value: Holder, flag: Bool) -> result: i32 pure {
  match move value {
    Held(cell: item) => {
    }
    Empty() => {
    }
  }
  let selected = if flag {
    let temporary = Cell(value: 7_i32);
    give 1_i32;
  } else {
    give 0_i32;
  }
  return selected;
}

fn main() -> status: std::process::ExitStatus pure {
  cleanup();
  let cell = Cell(value: 8_i32);
  let holder = Holder::Held(cell: move cell);
  let flag = True();
  cleanup_match(value: move holder, flag: flag);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = emit(source);
    // `Cell` returns in registers, so `make`'s moves and drops are in its
    // destination-form body (compiler/src/backend/abi.rs).
    assert!(emitted_drop_ids(emitted_body(&llvm, "make")).is_empty());

    let cleanup = emitted_function(&llvm, "cleanup");
    let cleanup_drops = emitted_drop_ids(cleanup);
    assert_eq!(cleanup_drops.len(), 6);
    assert!(cleanup_drops[3] > cleanup_drops[4]);
    assert!(cleanup_drops[4] > cleanup_drops[5]);
    assert!(cleanup.contains("; ownership-consuming projection"));

    let cleanup_match = emitted_function(&llvm, "cleanup_match");
    assert_eq!(emitted_drop_ids(cleanup_match).len(), 2);

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn copy_place_set_executes_for_root_and_nested_struct_fields() {
    let source = br#"struct Inner {
  value: i32;
}

struct Outer {
  inner: Inner;
  other: i32;
}

fn main() -> status: std::process::ExitStatus pure {
  let number = 1_i32;
  let inner = Inner(value: 2_i32);
  let outer = Outer(inner: inner, other: 7_i32);
  let flag = True();
  if flag {
    set number = 42_i32;
    set outer.inner.value = number;
  } else {
    set number = 9_i32;
    set outer.inner.value = number;
  }
  let observed = outer.inner.value;
  if observed != 42_i32 {
    return std::process::exit_status(code: 1_u8);
  }
  let preserved = outer.other;
  if preserved != 7_i32 {
    return std::process::exit_status(code: 2_u8);
  }
  let selected = if flag {
    set number = 43_i32;
    give number;
  } else {
    set number = 10_i32;
    give number;
  }
  if selected != 43_i32 {
    return std::process::exit_status(code: 3_u8);
  }
  if number != 43_i32 {
    return std::process::exit_status(code: 4_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = emit(source);
    let main = emitted_function(&llvm, "main");
    assert!(main.contains(" = phi i32 "));
    assert!(main.contains(&format!(
        "store {} zeroinitializer, ptr ",
        nominal_type("Outer")
    )));
    assert!(main.contains(&format!(
        "store {} zeroinitializer, ptr ",
        nominal_type("Inner")
    )));
    let mut value_addresses = Vec::new();
    for line in main.lines() {
        let Some((address, operation)) = line.trim().split_once(" = ") else {
            continue;
        };
        let selects_value = operation.starts_with(&format!(
            "getelementptr inbounds {}, ptr ",
            nominal_type("Inner")
        )) && operation.ends_with(", i32 0, i32 0");
        let aliases_value = operation
            .strip_prefix("getelementptr i8, ptr ")
            .and_then(|tail| tail.strip_suffix(", i64 0"))
            .is_some_and(|base| value_addresses.contains(&base));
        if selects_value || aliases_value {
            value_addresses.push(address);
        }
    }
    let value_stores = main
        .lines()
        .filter(|line| {
            line.trim().starts_with("store i32 ")
                && line
                    .split_once(", ptr ")
                    .is_some_and(|(_, address)| value_addresses.contains(&address))
        })
        .count();
    assert_eq!(
        value_stores, 3,
        "the Inner.value constructor and both assignment branches store their scalar field"
    );

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// [OP-2] bare exact arithmetic is rejected when its domain is refuted; there
/// is no implicit runtime fallback.
#[test]
fn bare_infix_overflow_is_a_static_op2_rejection() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let hi = 2147483647_i32;
  let one = 1_i32;
  let overflowed = hi + one;
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile_rejection(source);
    assert_eq!(failure.rule_id(), Some("OP-2"));
    assert!(failure.detail().contains("hi +defined one"));
}

#[test]
fn conformance_loop_accumulators_have_ssa_phi_nodes() {
    for source in [
        include_bytes!("../../../tests/conformance/cases/gram6-pos-no-operators.wf").as_slice(),
        include_bytes!("../../../tests/conformance/cases/own1-pos-tagonly-copy.wf").as_slice(),
        include_bytes!("../../../tests/conformance/cases/type2-pos-twostate-enum-i1.wf").as_slice(),
    ] {
        let llvm = compile(source);
        let main = emitted_function(&llvm, "main");
        assert!(main.contains(" = phi "));
    }
}

#[test]
fn result_values_checked_arithmetic_and_propagation_execute_through_host_llvm() {
    let source = br#"enum StepError {
  Failed();
}

struct Pair {
  left: i32;
  right: i32;
}

struct Envelope {
  result: Result<i32, StepError>;
  residue: Pair;
}

fn step(value: i32) -> result: Result<i32, StepError> pure {
  if value < 0_i32 {
    let error = StepError::Failed();
    return Err<i32, StepError>(error: error);
  } else {
    return Ok<i32, StepError>(value: value);
  }
}

fn forward(value: i32) -> result: Result<i64, StepError> pure {
  let result = step(value: value);
  let accepted = propagate result;
  return Ok<i64, StepError>(value: 42_i64);
}

fn forward_field(value: i32) -> result: Result<i64, StepError> pure {
  let result = step(value: value);
  let residue = Pair(left: 1_i32, right: 2_i32);
  let envelope = Envelope(result: result, residue: residue);
  let accepted = propagate envelope.result;
  return Ok<i64, StepError>(value: 42_i64);
}

fn make_pair() -> result: Result<Pair, StepError> pure {
  let pair = Pair(left: 20_i32, right: 22_i32);
  return Ok<Pair, StepError>(value: pair);
}

fn main() -> status: std::process::ExitStatus pure {
  let arithmetic_result = 2147483647_i32 +checked 1_i32;
  match arithmetic_result {
    Ok(value: sum) => {
      return std::process::exit_status(code: 1_u8);
    }
    Err(error: overflow) => {
    }
  }
  let subtract_result = 0_u8 -checked 1_u8;
  match subtract_result {
    Ok(value: difference) => {
      return std::process::exit_status(code: 2_u8);
    }
    Err(error: underflow) => {
    }
  }
  let multiply_result = 6_i16 *checked 7_i16;
  match multiply_result {
    Ok(value: product) => {
      if product != 42_i16 {
        return std::process::exit_status(code: 3_u8);
      }
    }
    Err(error: product_error) => {
      return std::process::exit_status(code: 4_u8);
    }
  }
  let success = forward(value: 7_i32);
  match success {
    Ok(value: answer) => {
      if answer != 42_i64 {
        return std::process::exit_status(code: 5_u8);
      }
    }
    Err(error: failure_error) => {
      return std::process::exit_status(code: 6_u8);
    }
  }
  let failure = forward(value: -1_i32);
  match failure {
    Ok(value: unexpected) => {
      return std::process::exit_status(code: 7_u8);
    }
    Err(error: forwarded_error) => {
    }
  }
  let field_success = forward_field(value: 7_i32);
  match field_success {
    Ok(value: field_answer) => {
      if field_answer != 42_i64 {
        return std::process::exit_status(code: 8_u8);
      }
    }
    Err(error: field_failure) => {
      return std::process::exit_status(code: 9_u8);
    }
  }
  let field_failure = forward_field(value: -1_i32);
  match field_failure {
    Ok(value: field_unexpected) => {
      return std::process::exit_status(code: 10_u8);
    }
    Err(error: field_forwarded_error) => {
    }
  }
  let pair_result = make_pair();
  match pair_result {
    Ok(value: pair) => {
      let total = pair.left +wrap pair.right;
      if total != 42_i32 {
        return std::process::exit_status(code: 11_u8);
      }
    }
    Err(error: pair_error) => {
      return std::process::exit_status(code: 12_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    assert!(llvm.contains("@llvm.sadd.with.overflow.i32"));
    assert!(llvm.contains("@llvm.usub.with.overflow.i8"));
    assert!(llvm.contains("@llvm.smul.with.overflow.i16"));
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn integer_overflow_has_no_op2_runtime_record_path() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let hi = 127_i8;
  let one = 1_i8;
  let overflow = hi + one;
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile_rejection(source);
    assert_eq!(failure.rule_id(), Some("OP-2"));
    assert!(failure.detail().contains("hi +defined one"));
}
