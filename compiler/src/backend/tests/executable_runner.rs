use super::{build_linked_executable_inner, compile, compile_and_run, test_directory};

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
