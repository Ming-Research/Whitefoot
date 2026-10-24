//! `open_read` looks a `RelativePath` up one component at a time below its
//! root [PRE-2], on POSIX and Windows hosts, and on the native completion
//! route and the shared adapter. One program opens the file its argument names
//! from the working directory and exits with the count of bytes it read, 203
//! for a refused open and 204 for a path `relative_path` rejects. The harness
//! places a file beside the working directory, so a parent step or a link that
//! left it would read that file and exit 5. The conformance adapter is
//! POSIX-only, so this is the Windows evidence for the rule.

use std::path::Path;
use std::process::{Command, Output};

use super::support::{
    CompiledProgram, FixtureDirectory, build_program, compile_program, fixture_directory,
};
use crate::support::run_command;

const PROGRAM: &str = "io_open_and_read.wf";
/// The program's exit status for the five bytes every readable fixture holds.
const READ: i32 = 5;
const REFUSED: i32 = 203;
const REJECTED: i32 = 204;

/// A fixture whose working directory `root` holds a file, a directory with a
/// file and a file named like a parent step, and beside which `outside.txt`
/// lies.
fn tree() -> FixtureDirectory {
    let fixture = fixture_directory();
    fixture.write_nested("root/ok.txt", b"hello");
    fixture.write_nested("root/nested/deep.txt", b"hello");
    fixture.write_nested("root/..x", b"hello");
    fixture.write_nested("outside.txt", b"hello");
    fixture
}

fn run(
    program: &CompiledProgram,
    fixture: &FixtureDirectory,
    argument: &str,
    native: bool,
) -> Output {
    let mut command = Command::new(program.executable());
    command
        .current_dir(fixture.path().join("root"))
        .arg(argument)
        .env_remove("WF_IO_NO_NATIVE_RING")
        .env_remove("WF_REQUIRE_WINDOWS_IOCP");
    if !native {
        command.env("WF_IO_NO_NATIVE_RING", "1");
    }
    run_command(&mut command)
}

fn expect(program: &CompiledProgram, fixture: &FixtureDirectory, cases: &[(&str, i32)]) {
    for native in [true, false] {
        for (argument, expected) in cases {
            let output = run(program, fixture, argument, native);
            assert_eq!(
                output.status.code(),
                Some(*expected),
                "route {native}, path {argument:?}: {output:?}"
            );
            assert!(
                output.stderr.is_empty(),
                "route {native}, path {argument:?}: {output:?}"
            );
        }
    }
}

#[test]
fn a_path_is_looked_up_one_component_below_the_next() {
    let program = build_program(&compile_program(PROGRAM));
    let fixture = tree();
    let mut cases = vec![
        ("ok.txt", READ),
        ("./ok.txt", READ),
        ("nested/deep.txt", READ),
        ("./nested//./deep.txt", READ),
        ("nested/./deep.txt", READ),
        // A name that only begins with two dots is an ordinary component.
        ("..x", READ),
        ("./..x", READ),
        // A directory, a missing entry and a missing directory open nothing.
        ("nested", REFUSED),
        ("nested/", REFUSED),
        ("nested/.", REFUSED),
        (".", REFUSED),
        ("missing", REFUSED),
        ("nested/missing.txt", REFUSED),
        ("missing/deep.txt", REFUSED),
        // A regular file is not a directory to look a component up in.
        ("ok.txt/deep.txt", REFUSED),
    ];
    // Windows takes both separators for a component [PRE-2].
    if cfg!(windows) {
        cases.extend([
            ("nested\\deep.txt", READ),
            (".\\nested\\\\.\\deep.txt", READ),
            ("nested\\missing.txt", REFUSED),
        ]);
    }
    expect(&program, &fixture, &cases);
}

#[test]
fn a_parent_component_is_rejected_before_any_open() {
    let program = build_program(&compile_program(PROGRAM));
    let fixture = tree();
    let mut cases = vec![
        // The file exists beside the working directory, so a path that
        // reached it would read its bytes.
        ("../outside.txt", REJECTED),
        ("..", REJECTED),
        ("nested/..", REJECTED),
        ("nested/../ok.txt", REJECTED),
        ("nested/../../outside.txt", REJECTED),
        ("./../outside.txt", REJECTED),
        ("nested/deep.txt/..", REJECTED),
    ];
    if cfg!(windows) {
        cases.extend([
            ("..\\outside.txt", REJECTED),
            ("nested\\..\\ok.txt", REJECTED),
            ("nested/..\\ok.txt", REJECTED),
        ]);
    }
    expect(&program, &fixture, &cases);
}

/// A symbolic link at `at` to a file or to a directory, as the host makes it.
fn make_link(target: &Path, at: &Path, directory: bool) {
    #[cfg(unix)]
    {
        let _ = directory;
        std::os::unix::fs::symlink(target, at)
            .unwrap_or_else(|error| panic!("symlink {at:?}: {error}"));
    }
    #[cfg(windows)]
    {
        // Creating a symbolic link takes the privilege the hosted runner's
        // account holds.
        if directory {
            std::os::windows::fs::symlink_dir(target, at)
        } else {
            std::os::windows::fs::symlink_file(target, at)
        }
        .unwrap_or_else(|error| panic!("symlink {at:?}: {error}"));
    }
}

#[test]
fn a_link_at_any_component_is_refused() {
    let program = build_program(&compile_program(PROGRAM));
    let fixture = tree();
    let root = fixture.path().join("root");
    // A link to a file inside the root, one to the file beside it, one to a
    // directory inside it and one to the directory that holds it.
    make_link(&root.join("ok.txt"), &root.join("inner-file"), false);
    make_link(
        &fixture.path().join("outside.txt"),
        &root.join("escaping-file"),
        false,
    );
    make_link(&root.join("nested"), &root.join("inner-directory"), true);
    make_link(fixture.path(), &root.join("up"), true);
    expect(
        &program,
        &fixture,
        &[
            // The same shapes without a link are read.
            ("ok.txt", READ),
            ("nested/deep.txt", READ),
            // A link as the last component.
            ("inner-file", REFUSED),
            ("./inner-file", REFUSED),
            ("escaping-file", REFUSED),
            // A link as an earlier component, its target inside and outside.
            ("inner-directory/deep.txt", REFUSED),
            ("./inner-directory//deep.txt", REFUSED),
            ("up/outside.txt", REFUSED),
            ("up/root/ok.txt", REFUSED),
            // A link to a directory is no file to read either.
            ("inner-directory", REFUSED),
            ("up", REFUSED),
        ],
    );
}

/// A junction needs no privilege, and `open_read` refuses it like a symbolic
/// link, as the name operations refuse every reparse point [PRE-2].
#[cfg(windows)]
#[test]
fn a_junction_at_any_component_is_refused() {
    let program = build_program(&compile_program(PROGRAM));
    let fixture = tree();
    let root = fixture.path().join("root");
    for (name, target) in [
        ("inner-junction", root.join("nested")),
        ("up-junction", fixture.path().to_path_buf()),
    ] {
        #[allow(clippy::disallowed_methods, reason = "host tool, not a test program")]
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(root.join(name))
            .arg(&target)
            .output()
            .expect("run mklink");
        assert!(made.status.success(), "mklink /J {name}: {made:?}");
    }
    expect(
        &program,
        &fixture,
        &[
            ("inner-junction/deep.txt", REFUSED),
            ("up-junction/outside.txt", REFUSED),
            ("inner-junction", REFUSED),
            ("nested/deep.txt", READ),
        ],
    );
}
