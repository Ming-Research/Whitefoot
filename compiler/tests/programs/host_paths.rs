//! Host paths [PRE-2], end to end on POSIX and Windows hosts. One program
//! opens the directory its argument names through `open_host_directory` and
//! publishes an append-only log in it; another opens paths that files hold,
//! through byte windows in both encodings. The harness, not the program, is
//! the oracle: it reads the published bytes and checks that a refused open
//! created nothing, on the native completion route and on the shared adapter.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

use super::support::{CompiledProgram, build_program, compile_program, fixture_directory};
#[cfg(unix)]
use super::support::{close_path, reopen_path};
use crate::support::run_command;

fn run(program: &CompiledProgram, directory: &Path, arguments: &[&OsStr], native: bool) -> Output {
    let mut command = Command::new(program.executable());
    command
        .current_dir(directory)
        .args(arguments)
        .env_remove("WF_IO_NO_NATIVE_RING")
        .env_remove("WF_REQUIRE_WINDOWS_IOCP");
    if !native {
        command.env("WF_IO_NO_NATIVE_RING", "1");
    }
    run_command(&mut command)
}

/// Runs the data-directory program from `directory` with `argument` and
/// checks that it published the log in `data`, then removes the log.
fn publishes(
    program: &CompiledProgram,
    directory: &Path,
    argument: &OsStr,
    native: bool,
    data: &Path,
) {
    let published = run(program, directory, &[argument], native);
    assert_eq!(
        published.status.code(),
        Some(0),
        "route {native}, argument {argument:?}: {published:?}"
    );
    let listing = |path: &Path| -> Vec<String> {
        std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.path().display().to_string())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert!(
        data.join("appendonly.aof").exists(),
        "route {native}, argument {argument:?}, directory {directory:?}: no log; data {:?}, \
         directory {:?}, parent {:?}",
        listing(data),
        listing(directory),
        listing(data.parent().expect("a parent")),
    );
    assert_eq!(
        std::fs::read(data.join("appendonly.aof")).expect("the published log"),
        b"hello\n",
        "argument {argument:?}"
    );
    assert!(!data.join("appendonly.aof.tmp").exists());
    std::fs::remove_file(data.join("appendonly.aof")).expect("remove the log");
}

#[test]
fn a_host_path_selects_an_existing_data_directory_and_never_creates_one() {
    let program = build_program(&compile_program("host_paths.wf"));
    for native in [true, false] {
        let fixture = fixture_directory();
        let data = fixture.path().join("data");
        std::fs::create_dir(&data).expect("create the data directory");
        std::fs::write(
            data.join("appendonly.aof.tmp"),
            b"a stale, longer rewrite\n",
        )
        .expect("write a leftover temporary file");

        // An absolute path, through a working directory elsewhere.
        let elsewhere = fixture.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).expect("create the working directory");
        publishes(&program, &elsewhere, data.as_os_str(), native, &data);
        assert!(!elsewhere.join("appendonly.aof").exists());

        // A relative path starts at the working directory the invocation
        // supplies as the base, and a parent component leaves it.
        publishes(&program, fixture.path(), OsStr::new("data"), native, &data);
        publishes(&program, &elsewhere, OsStr::new("../data"), native, &data);

        #[cfg(windows)]
        windows_forms(&program, fixture.path(), &elsewhere, native, &data);

        // A missing directory is NotFound and stays missing.
        let missing = fixture.path().join("missing");
        let absent = run(&program, &elsewhere, &[missing.as_os_str()], native);
        assert_eq!(absent.status.code(), Some(10), "route {native}: {absent:?}");
        assert!(!missing.exists(), "a refused open created the directory");

        // A regular file where the directory belongs is NotDirectory.
        let file = fixture.path().join("file");
        std::fs::write(&file, b"not a directory").expect("write the file");
        let wrong = run(&program, &elsewhere, &[file.as_os_str()], native);
        assert_eq!(wrong.status.code(), Some(12), "route {native}: {wrong:?}");
        assert_eq!(std::fs::read(&file).expect("the file"), b"not a directory");

        // A directory below an unsearchable one is PermissionDenied. Mode
        // bits are POSIX; a Windows denial needs an access control list.
        #[cfg(unix)]
        {
            let locked = fixture.path().join("locked");
            std::fs::create_dir_all(locked.join("data")).expect("create the locked tree");
            close_path(&locked);
            let denied = run(
                &program,
                &elsewhere,
                &[locked.join("data").as_os_str()],
                native,
            );
            reopen_path(&locked, 0o755);
            assert_eq!(denied.status.code(), Some(11), "route {native}: {denied:?}");
            assert!(!locked.join("data").join("appendonly.aof").exists());
        }
    }
}

/// The Win32 path forms beyond an absolute and a relative path: root-relative
/// on the base's drive, drive-relative on the base's drive, and extended.
#[cfg(windows)]
fn windows_forms(
    program: &CompiledProgram,
    fixture: &Path,
    elsewhere: &Path,
    native: bool,
    data: &Path,
) {
    let spelled = data.to_str().expect("a textual fixture path");
    let (drive, rooted) = spelled.split_at(2);
    assert!(
        drive.ends_with(':'),
        "a drive-absolute fixture path: {spelled}"
    );
    publishes(program, elsewhere, OsStr::new(rooted), native, data);
    let drive_relative = format!("{drive}data");
    publishes(program, fixture, OsStr::new(&drive_relative), native, data);
    let extended = format!("\\\\?\\{spelled}");
    publishes(program, elsewhere, OsStr::new(&extended), native, data);
    // A device path names no directory of a volume: InvalidPath, 14.
    let device = run(program, elsewhere, &[OsStr::new("\\\\.\\NUL")], native);
    assert_eq!(device.status.code(), Some(14), "route {native}: {device:?}");
}

/// A path's native bytes, as `NativeBytes` reads them on this host.
#[cfg(unix)]
fn native_bytes(path: &Path) -> Vec<u8> {
    std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str()).to_vec()
}

#[cfg(windows)]
fn native_bytes(path: &Path) -> Vec<u8> {
    std::os::windows::ffi::OsStrExt::encode_wide(path.as_os_str())
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Native bytes that name no path on this host: a zero byte inside a POSIX
/// path, and an odd byte count on Windows, which holds no whole UTF-16 code.
#[cfg(unix)]
const INVALID_NATIVE: &[u8] = b"a\0b";
#[cfg(windows)]
const INVALID_NATIVE: &[u8] = b"abc";

#[test]
fn host_paths_from_bytes_open_in_both_encodings() {
    let program = build_program(&compile_program("host_path_bytes.wf"));
    for native in [true, false] {
        let fixture = fixture_directory();
        // Names outside ASCII, one beyond the Basic Multilingual Plane, so
        // the UTF-8 window converts and the native one carries surrogates
        // on Windows.
        let configured = fixture.path().join("c\u{fc}\u{4241}\u{1f600}.conf");
        std::fs::write(&configured, b"C").expect("write the configured file");
        let directory = fixture.path().join("d\u{fc}\u{4241}\u{1f600}");
        std::fs::create_dir(&directory).expect("create the configured directory");
        std::fs::write(directory.join("marker"), b"C").expect("write the marker");
        std::fs::create_dir(fixture.path().join("sub")).expect("create the side directory");

        // An absolute UTF-8 path, and a native path relative to the base
        // through a parent component.
        let utf8 = configured.to_str().expect("a textual fixture path");
        std::fs::write(fixture.path().join("utf8.path"), utf8.as_bytes())
            .expect("write the UTF-8 path");
        let relative = Path::new("sub").join("..").join("d\u{fc}\u{4241}\u{1f600}");
        std::fs::write(fixture.path().join("native.path"), native_bytes(&relative))
            .expect("write the native path");
        std::fs::write(fixture.path().join("invalid.path"), INVALID_NATIVE)
            .expect("write the invalid path");

        let output = run(&program, fixture.path(), &[], native);
        assert_eq!(output.status.code(), Some(0), "route {native}: {output:?}");
    }
}
