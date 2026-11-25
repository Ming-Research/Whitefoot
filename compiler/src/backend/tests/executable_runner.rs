use std::io::Write;
use std::process::{Command, Stdio};

use super::{build_linked_executable_inner, compile, compile_and_run, test_directory};

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
        if let Err(refusal) = host_code_generation(&module) {
            panic!("{label}: the host toolchain refused the module:\n{refusal}\n{module}");
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
