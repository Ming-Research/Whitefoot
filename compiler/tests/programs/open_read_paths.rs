//! `open_read` looks a `RelativePath` up one component at a time below its
//! root [PRE-2], on POSIX and Windows hosts, and on the native completion
//! route and the shared adapter. One program opens the file its argument names
//! from the working directory and exits with the count of bytes it read, 203
//! for a refused open and 204 for a path `relative_path` rejects. The harness
//! places a file beside the working directory, so a parent step or a link that
//! left it would read that file and exit 5. The conformance adapter is
//! POSIX-only, so this is the Windows evidence for the rule.

use std::ffi::OsStr;
use std::fmt::Debug;
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
    argument: impl AsRef<OsStr>,
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

fn expect<S: AsRef<OsStr> + Debug>(
    program: &CompiledProgram,
    fixture: &FixtureDirectory,
    cases: &[(S, i32)],
) {
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

/// Component names are bytes on a POSIX host, so a name that is not UTF-8
/// text, and one that holds a backslash, are ordinary components [PRE-2]:
/// `x\..\y` is one component of a POSIX path, not a parent step.
#[cfg(unix)]
#[test]
fn a_component_is_a_byte_string_on_a_posix_host() {
    use std::os::unix::ffi::OsStrExt;

    let program = build_program(&compile_program(PROGRAM));
    let fixture = tree();
    fixture.write(b"root/non-text-\xff\xfe", b"hello");
    std::fs::create_dir(fixture.path().join("root").join(OsStr::from_bytes(b"dir-\xff")))
        .expect("create a directory with a non-text name");
    std::fs::write(
        fixture
            .path()
            .join("root")
            .join(OsStr::from_bytes(b"dir-\xff"))
            .join(OsStr::from_bytes(b"in-\xfe")),
        b"hello",
    )
    .expect("write below it");
    fixture.write_nested("root/x\\..\\y", b"hello");
    fixture.write_nested("root/nested/a\\b", b"hello");
    let cases: Vec<(&OsStr, i32)> = vec![
        (OsStr::from_bytes(b"non-text-\xff\xfe"), READ),
        (OsStr::from_bytes(b"dir-\xff/in-\xfe"), READ),
        (OsStr::from_bytes(b"./dir-\xff//in-\xfe"), READ),
        (OsStr::from_bytes(b"dir-\xff/missing"), REFUSED),
        (OsStr::from_bytes(b"x\\..\\y"), READ),
        (OsStr::from_bytes(b"nested/a\\b"), READ),
        // The Windows spelling of a parent step is no step here, and the
        // entries it would name do not exist, so nothing is read.
        (OsStr::from_bytes(b"nested\\..\\ok.txt"), REFUSED),
        (OsStr::from_bytes(b"..\\outside.txt"), REFUSED),
    ];
    expect(&program, &fixture, &cases);
}

/// A component the host has no name for, a very long path and a long run of
/// empty and `.` components end in a refusal or a read, never a crash or a
/// hang [PRE-2].
#[cfg(unix)]
#[test]
fn an_oversized_or_repeated_component_ends_in_an_ordinary_outcome() {
    let program = build_program(&compile_program(PROGRAM));
    let fixture = tree();
    // A chain of 100 directories with a file at its end.
    let chain = vec!["d"; 100].join("/");
    fixture.write_nested(&format!("root/{chain}/leaf.txt"), b"hello");
    let long = |length: usize| "x".repeat(length);
    let dots = "./".repeat(1500);
    let slashes = "/".repeat(1500);
    let cases: Vec<(String, i32)> = vec![
        (format!("{chain}/leaf.txt"), READ),
        (format!("{dots}nested/deep.txt"), READ),
        (format!("nested{slashes}deep.txt"), READ),
        // Longer than a file name may be on any host, and longer than the
        // walk copies out of a path, first, middle and last.
        (format!("nested/{}", long(300)), REFUSED),
        (format!("{}/deep.txt", long(300)), REFUSED),
        (format!("nested/{}/deep.txt", long(1100)), REFUSED),
        (format!("{}/deep.txt", long(1023)), REFUSED),
        (format!("{}/deep.txt", long(1024)), REFUSED),
        (format!("{}/deep.txt", long(5000)), REFUSED),
        // The empty path names no file, and a file is no directory.
        (String::new(), REFUSED),
        ("ok.txt/".to_string(), REFUSED),
        ("ok.txt/.".to_string(), REFUSED),
    ];
    expect(&program, &fixture, &cases);
}

/// A waiting facility at the last component is not waited for, and a link
/// that leads nowhere or back to itself ends the lookup [PRE-2].
#[cfg(unix)]
#[test]
fn a_fifo_or_a_looping_link_is_refused_without_waiting() {
    let program = build_program(&compile_program(PROGRAM));
    let fixture = tree();
    let root = fixture.path().join("root");
    for name in ["fifo", "nested/fifo"] {
        let made = run_command(Command::new("mkfifo").arg(root.join(name)));
        assert!(made.status.success(), "mkfifo {name}: {made:?}");
    }
    make_link(Path::new("loop"), &root.join("loop"), true);
    make_link(Path::new("no-such-entry"), &root.join("dangling"), false);
    make_link(Path::new("loop-b"), &root.join("loop-a"), true);
    make_link(Path::new("loop-a"), &root.join("loop-b"), true);
    expect(
        &program,
        &fixture,
        &[
            ("fifo", REFUSED),
            ("nested/fifo", REFUSED),
            ("./nested//fifo", REFUSED),
            ("loop", REFUSED),
            ("loop/deep.txt", REFUSED),
            ("loop/loop/loop/x", REFUSED),
            ("loop-a/loop-b/x", REFUSED),
            ("dangling", REFUSED),
            ("dangling/x", REFUSED),
            ("nested/deep.txt", READ),
        ],
    );
}

/// An entry that is swapped for a link to a directory outside the root while
/// the program looks its path up is never followed: a run reads the bytes
/// below the root or is refused, and never reads the outside file, which has
/// another length. A lookup that resolved the path in one host call, or that
/// checked a component before it opened it, would read the outside file in
/// some of the runs [PRE-2].
#[cfg(unix)]
#[test]
fn a_directory_swapped_for_a_link_during_the_lookup_is_never_followed() {
    use std::sync::atomic::{AtomicBool, Ordering};

    const OUTSIDE: i32 = 7;
    let program = build_program(&compile_program(PROGRAM));
    let fixture = tree();
    let root = fixture.path().join("root");
    fixture.write_nested("root/real/sub/f", b"hello");
    fixture.write_nested("outdir/sub/f", b"ABCDEFG");
    let direct = root.join("d");
    let real = root.join("real");
    let outdir = fixture.path().join("outdir");
    // `d` is a real directory, then missing, then a link to the outside
    // directory, in turn, until the runs are over.
    std::os::unix::fs::symlink(&outdir, &direct).expect("link d");
    let stop = AtomicBool::new(false);
    let mut counts = [0usize; 3];
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                std::fs::remove_file(&direct).expect("unlink the link d");
                std::fs::rename(&real, &direct).expect("put the directory at d");
                std::thread::yield_now();
                std::fs::rename(&direct, &real).expect("take the directory from d");
                std::os::unix::fs::symlink(&outdir, &direct).expect("link d again");
                std::thread::yield_now();
            }
        });
        for round in 0..300 {
            let native = round % 2 == 0;
            let output = run(&program, &fixture, "d/sub/f", native);
            match output.status.code() {
                Some(READ) => counts[0] += 1,
                Some(REFUSED) => counts[1] += 1,
                Some(OUTSIDE) => counts[2] += 1,
                other => {
                    stop.store(true, Ordering::Relaxed);
                    panic!("round {round}: unexpected status {other:?}: {output:?}");
                }
            }
        }
        stop.store(true, Ordering::Relaxed);
    });
    assert_eq!(
        counts[2], 0,
        "the outside file was read: below root {}, refused {}, outside {}",
        counts[0], counts[1], counts[2]
    );
    assert!(
        counts[0] > 0 && counts[1] > 0,
        "the swap never changed the outcome, so the race was not exercised: {counts:?}"
    );
}
