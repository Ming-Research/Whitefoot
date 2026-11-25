use std::io::Write;
use std::process::{Command, Stdio};

use super::{
    build_linked_executable_inner, compile, compile_and_run, emit_with_overlap, test_directory,
};

/// Runs the host toolchain's code generator over `module` and returns what it
/// said, so a module it refuses fails with the toolchain's own words.
///
/// `clang -x ir -S` parses the LLVM text and then runs the same back end `llc`
/// does, so acceptance here is acceptance by the consumer the compiler's own
/// native builds use.
fn host_code_generation(module: &str) -> Result<String, String> {
    #[allow(clippy::disallowed_methods, reason = "host toolchain, not a test program")]
    let mut child = Command::new(crate::clang_executable())
        .args(["-x", "ir", "-", "-S", "-o", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("invoke host clang");
    child
        .stdin
        .take()
        .expect("clang stdin must be available")
        .write_all(module.as_bytes())
        .expect("send module to host clang");
    let output = child.wait_with_output().expect("wait for host clang");
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// A module the compiler writes for source with no executable runner is a
/// library: it must still be valid LLVM, whether the source has no `main` at
/// all or a `main` whose signature the bundled runner does not call.
#[test]
fn a_module_without_an_executable_runner_is_accepted_by_the_host_toolchain() {
    for (label, source) in [
        (
            "no main",
            &b"fn answer() -> result: u8 pure {\n  return 7_u8;\n}\n"[..],
        ),
        (
            "main without a runner",
            &b"fn main() -> result: u8 pure {\n  return 7_u8;\n}\n"[..],
        ),
    ] {
        let module = compile(source);
        assert!(
            crate::require_executable_runner(&module).is_err(),
            "{label}: the module must be a library"
        );
        // The floor entry and the program body it calls exist together or not
        // at all, so no library module names a body it does not define.
        for symbol in ["@wf__main_body", "@wf__floor_run"] {
            assert!(!module.contains(symbol), "{label}: names {symbol}\n{module}");
        }
        if let Err(refusal) = host_code_generation(&module) {
            panic!("{label}: the host toolchain refused the module:\n{refusal}\n{module}");
        }
    }
    // The same two symbols are present, defined, when a runner is generated.
    let module = compile(b"fn main() -> result: unit pure {\n  return unit;\n}\n");
    for definition in [
        "define i32 @wf__main_body(",
        "define weak i32 @wf__floor_run(",
    ] {
        assert!(module.contains(definition), "{definition}\n{module}");
    }
    if let Err(refusal) = host_code_generation(&module) {
        panic!("the host toolchain refused a module with a runner:\n{refusal}\n{module}");
    }
}

/// The symbols a module carries only beside the executable runner.
const RUNNER_SYMBOLS: [&str; 2] = ["@wf__main_body", "@wf__floor_run"];

/// A `main` whose requirement the build caller cannot prove gets no executable
/// runner, and the module that results is a library too: the floor entry and
/// the body it calls are written only with a runner (the second route to a
/// library, besides a source with no `main` or an unrecognized signature).
#[test]
fn a_main_with_an_unprovable_requirement_is_a_library_the_host_toolchain_accepts() {
    let module = compile(
        br#"fn main() -> status: std::process::ExitStatus pure contract {
  requires 0_u64 == 1_u64;
} {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert!(crate::require_executable_runner(&module).is_err());
    for symbol in RUNNER_SYMBOLS {
        assert!(!module.contains(symbol), "names {symbol}\n{module}");
    }
    if let Err(refusal) = host_code_generation(&module) {
        panic!("the host toolchain refused the module:\n{refusal}\n{module}");
    }
}

/// Programs the repository keeps, with their `main` renamed so the module is a
/// library of the same code: heap containers, and ranges run in parallel under
/// the overlap lowering the compute programs use. The library carries every
/// runtime reference these bodies make, with no runner symbol among them.
#[test]
fn a_library_of_a_real_program_is_accepted_by_the_host_toolchain() {
    for (name, source) in [
        (
            "growable_vec.wf",
            &include_str!("../../../../tests/programs/growable_vec.wf")[..],
        ),
        (
            "prefix.wf",
            &include_str!("../../../../tests/programs/compute/prefix.wf")[..],
        ),
    ] {
        assert_eq!(source.matches("fn main(").count(), 1, "{name}");
        let library = emit_with_overlap(source.replace("fn main(", "fn entry_point(").as_bytes());
        for symbol in RUNNER_SYMBOLS {
            assert!(!library.contains(symbol), "{name}: names {symbol}");
        }
        if let Err(refusal) = host_code_generation(&library) {
            panic!("{name}: the host toolchain refused the library:\n{refusal}");
        }
        // The same code with its `main` keeps the runner and stays accepted.
        let program = emit_with_overlap(source.as_bytes());
        for definition in [
            "define i32 @wf__main_body(",
            "define weak i32 @wf__floor_run(",
        ] {
            assert!(program.contains(definition), "{name}: {definition}");
        }
        if let Err(refusal) = host_code_generation(&program) {
            panic!("{name}: the host toolchain refused the program:\n{refusal}");
        }
    }
}

/// The fragments of a split module (`--fragments`) follow the module: a
/// library's fragments carry neither runner symbol, a program's runner symbols
/// are defined together in one fragment, and every fragment is accepted by the
/// host toolchain on its own, as each is compiled to its own object.
#[test]
fn the_fragments_of_a_module_follow_its_executable_runner() {
    let library_source = &b"fn answer() -> result: u8 pure {\n  return 7_u8;\n}\n"[..];
    let program_source = &b"fn answer() -> result: u8 pure {\n  return 7_u8;\n}\n\nfn main() -> result: unit pure {\n  return unit;\n}\n"[..];
    for (label, source, has_runner) in [
        ("library", library_source, false),
        ("program", program_source, true),
    ] {
        let inputs = [crate::SourceInput::new("test.wf", source)];
        let (whole, _) = crate::compile_for_emission(
            &inputs,
            crate::CompilerLimits::default(),
            crate::OverlapLowering::Off,
            None,
            true,
        )
        .unwrap_or_else(|failure| panic!("{label} compiles for fragments: {failure}"));
        for granularity in [
            crate::FragmentGranularity::Function,
            crate::FragmentGranularity::Module,
        ] {
            let fragments = crate::split_module(&whole, granularity).expect("the module splits");
            let mut defining_both = 0;
            for fragment in &fragments {
                if let Err(refusal) = host_code_generation(fragment) {
                    panic!("{label} {granularity:?}: a fragment was refused:\n{refusal}\n{fragment}");
                }
                let body = fragment.contains("define i32 @wf__main_body(");
                let floor = fragment.contains("define weak i32 @wf__floor_run(");
                assert_eq!(body, floor, "{label} {granularity:?}: split apart\n{fragment}");
                defining_both += usize::from(body);
                if !has_runner {
                    for symbol in RUNNER_SYMBOLS {
                        assert!(!fragment.contains(symbol), "{label}: {symbol}\n{fragment}");
                    }
                }
            }
            assert_eq!(defining_both, usize::from(has_runner), "{label} {granularity:?}");
        }
    }
}

#[test]
fn a_checked_library_entry_stops_before_native_build() {
    let module = compile(b"fn main() -> result: u8 pure {\n  return 7_u8;\n}\n");
    let directory = test_directory();
    let failure = build_linked_executable_inner(&module, None, &[], &[], "", &directory)
        .expect_err("a u8 entry has no executable runner");
    assert_eq!(
        failure,
        crate::require_executable_runner(&module).expect_err("no executable runner")
    );
    assert!(failure.contains("repair: select a function"), "{failure}");
    assert_eq!(
        std::fs::read_dir(&directory).expect("build directory").count(),
        0
    );
    std::fs::remove_dir(&directory).expect("remove unused build directory");
}

#[test]
fn unit_and_exit_status_repairs_build_and_run() {
    for (source, expected) in [
        (&b"fn main() -> result: unit pure {\n  return unit;\n}\n"[..], 0),
        (
            &b"fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 7_u8);\n}\n"[..],
            7,
        ),
    ] {
        let module = compile(source);
        assert_eq!(compile_and_run(&module).status.code(), Some(expected));
    }
}
