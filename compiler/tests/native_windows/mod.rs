//! Compiler/runtime observations on real Windows. These share the corpus
//! executable and host support; they are not source-language verdicts.

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use whitefoot::{
    Architecture, CompilerLimits, HOST_OPTIMIZATION_ARGUMENTS, SourceInput, compile, stack_ledger,
};

use crate::programs::support::{compile_program_with_overlap, fixture_directory};
use crate::support::{
    CLANG, LINK_LIBRARIES, append_runtime_objects, observe_layout, run_command, spine_source,
    wide_frame_source,
};

fn native_link(
    directory: &Path,
    module: &Path,
    host: &str,
    wide: bool,
    small: bool,
) -> std::path::PathBuf {
    let observer = directory.join("observer.c");
    std::fs::write(&observer, host).expect("write native observer");
    let executable = directory.join(if small { "small.exe" } else { "native.exe" });
    let mut command = Command::new(CLANG);
    command
        .arg(module)
        .arg("-x")
        .arg("c")
        .arg(&observer)
        .args(HOST_OPTIMIZATION_ARGUMENTS)
        .arg("-std=c11")
        .arg(if wide {
            "-DWF_TEST_WIDE=1"
        } else {
            "-DWF_TEST_WIDE=0"
        });
    if !small {
        command.arg("-DWF_TEST_STACK_BYTES=1073741824u");
    }
    let llvm = std::fs::read_to_string(module).expect("read emitted module dependencies");
    let needs_heap = llvm.contains("@wf__heap_take(") || llvm.contains("@wf__heap_give(");
    let (_, objects) = append_runtime_objects(&mut command, directory, None, None, needs_heap);
    if small {
        static FLOOR: OnceLock<Vec<u8>> = OnceLock::new();
        let bytes = FLOOR.get_or_init(|| {
            let temporary = fixture_directory();
            let source = temporary.path().join("floor.c");
            let object = temporary.path().join("floor.o");
            let production = "#define WF_FLOOR_STACK_BYTES ((size_t)1024u * 1024u * 1024u)";
            assert_eq!(
                whitefoot::FLOOR_WINDOWS_RUNTIME_SOURCE
                    .matches(production)
                    .count(),
                1
            );
            std::fs::write(
                &source,
                whitefoot::FLOOR_WINDOWS_RUNTIME_SOURCE.replace(
                    production,
                    "#define WF_FLOOR_STACK_BYTES ((size_t)1024u * 1024u)",
                ),
            )
            .expect("test stack reservation");
            let result = run_command(
                Command::new(CLANG)
                    .args(HOST_OPTIMIZATION_ARGUMENTS)
                    .arg("-c")
                    .arg(&source)
                    .arg("-o")
                    .arg(&object),
            );
            assert!(result.status.success(), "small floor: {result:?}");
            std::fs::read(object).expect("small floor object")
        });
        std::fs::write(&objects[0], bytes).expect("select small floor object");
    }
    let result = run_command(command.args(LINK_LIBRARIES).arg("-o").arg(&executable));
    assert!(result.status.success(), "native Windows link: {result:?}");
    executable
}

#[test]
fn each_layout_fold_executes_a_real_non_owner_worker() {
    let llvm = compile_program_with_overlap("par_layout.wf");
    let (observed, host) = observe_layout(&llvm);
    let temporary = fixture_directory();
    let module = temporary.path().join("layout.ll");
    std::fs::write(&module, observed).expect("observed layout module");
    let executable = native_link(temporary.path(), &module, &host, false, false);
    let output = run_command(Command::new(executable).env("WF_WORKERS", "4"));
    assert!(
        output.status.success(),
        "observed Windows folds: {output:?}"
    );
    assert_eq!(output.stdout, b"420a993efa7437a1 41fa962893d45299\n");
    assert!(output.stderr.is_empty());
}

fn field(text: &str, prefix: &str) -> u64 {
    text.split_whitespace()
        .find_map(|word| word.strip_prefix(prefix))
        .expect("measured stack field")
        .parse()
        .expect("numeric stack field")
}

#[test]
fn entry_and_worker_stacks_have_real_reservations_guarantees_and_exhaustion_boundaries() {
    const SMALL: u64 = 1024 * 1024;
    for (wide, source) in [(false, spine_source(0)), (true, wide_frame_source(0, 1024))] {
        let directory = fixture_directory();
        let llvm = compile(
            &[SourceInput::new("stack.wf", &source)],
            CompilerLimits::default(),
        )
        .expect("stack geometry compiles")
        .replacen(
            "define i32 @wf__main_body(",
            "define i32 @wf__unused_main_body(",
            1,
        )
        .replacen("define i32 @main(", "define i32 @wf__unused_main(", 1)
            + "\ndeclare i32 @wf__main_body(i32, ptr)\n";
        let module = directory.path().join("ledger.ll");
        let assembly = directory.path().join("ledger.s");
        std::fs::write(&module, llvm).expect("stack geometry module");
        let result = run_command(
            Command::new(CLANG)
                .arg(&module)
                .args(HOST_OPTIMIZATION_ARGUMENTS)
                .args(["-S", "-fstack-usage", "-Wno-override-module", "-o"])
                .arg(&assembly),
        );
        assert!(result.status.success(), "machine stack report: {result:?}");
        let usage =
            std::fs::read_to_string(directory.path().join("ledger.su")).expect("stack usage");
        let machine = std::fs::read_to_string(&assembly).expect("measured assembly");
        let ledger = stack_ledger(&usage, &machine, SMALL, Architecture::HOST);
        let cycle = ledger
            .iter()
            .find(|line| {
                line.starts_with("STACK cycle")
                    && line.split_whitespace().nth(2) == Some("wf_spine")
            })
            .expect("recursive machine frame");
        let frame = cycle
            .split_whitespace()
            .zip(cycle.split_whitespace().skip(1))
            .find_map(|(number, unit)| (unit == "B/level").then_some(number))
            .expect("cycle width")
            .parse::<u64>()
            .expect("numeric frame width");
        assert!(frame > 0);
        if wide {
            assert!(frame >= 8192, "the live array must remain: {frame}");
        }
        let host = include_str!("../../src/backend/tests/stack_boundary.c");
        // Link the exact measured assembly, never a separately optimized module.
        let executable = native_link(directory.path(), &assembly, host, wide, true);
        let run = |image: &Path, mode: &str, depth: u64| {
            run_command(
                Command::new(image)
                    .env("WF_WORKERS", "4")
                    .arg(mode)
                    .arg(depth.to_string()),
            )
        };
        let abort = run(&executable, "abort-control", 0);
        let abort_code = abort.status.code().expect("Windows abort disposition") as u32;
        assert!(
            matches!(abort_code, 3 | 0xc0000409),
            "CRT abort control: {abort:?}"
        );
        for thread in ["entry", "worker"] {
            let bounds = run(&executable, &format!("bounds-{thread}"), 0);
            assert!(bounds.status.success(), "{wide} {thread}: {bounds:?}");
            assert!(bounds.stderr.is_empty());
            let text = String::from_utf8(bounds.stdout).expect("ASCII stack bounds");
            assert!(text.starts_with(&format!("{thread}\n")), "{text}");
            assert!(field(&text, "stack=") >= SMALL);
            let room = field(&text, "room=");
            let allowance = 2 * field(&text, "page=") + 4096 + 2 * frame;
            assert!(allowance < room / 10);
            let levels = room / frame;
            let slack = allowance.div_ceil(frame);
            for (depth, exhausted) in [(levels - slack, false), (levels + slack, true)] {
                let output = run(&executable, thread, depth);
                assert_eq!(
                    output.stdout,
                    format!("{thread}\n").as_bytes(),
                    "{output:?}"
                );
                if exhausted {
                    assert_eq!(output.status.code(), abort.status.code(), "{output:?}");
                    assert_eq!(output.stderr, b"{\"resource\":\"stack\"}\n");
                } else {
                    assert!(output.status.success(), "{output:?}");
                    assert!(output.stderr.is_empty());
                }
            }
        }
        if !wide {
            let production = native_link(directory.path(), &assembly, host, false, false);
            for thread in ["entry", "worker"] {
                let bounds = run(&production, &format!("bounds-{thread}"), 0);
                assert!(bounds.status.success(), "default provisioning: {bounds:?}");
                assert!(bounds.stderr.is_empty());
                let text = String::from_utf8(bounds.stdout).expect("ASCII default stack bounds");
                assert!(text.starts_with(&format!("{thread}\n")));
                assert!(field(&text, "stack=") >= 1024 * 1024 * 1024);
            }
        }
    }
}
