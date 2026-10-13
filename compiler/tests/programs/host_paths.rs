//! A data directory selected by host path [PRE-2], end to end: the program
//! opens the directory its argument names through `open_host_directory` and
//! publishes an append-only log in it. The harness, not the program, is the
//! oracle: it reads the published bytes and checks that a refused open
//! created nothing, on the native completion ring and on the shared adapter.

use std::path::Path;
use std::process::{Command, Output};

use super::support::{
    CompiledProgram, build_program, close_path, compile_program, fixture_directory, reopen_path,
};
use crate::support::run_command;

fn run(program: &CompiledProgram, directory: &Path, argument: &Path, native: bool) -> Output {
    let mut command = Command::new(program.executable());
    command
        .current_dir(directory)
        .arg(argument)
        .env_remove("WF_IO_NO_NATIVE_RING");
    if !native {
        command.env("WF_IO_NO_NATIVE_RING", "1");
    }
    run_command(&mut command)
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
        let published = run(&program, &elsewhere, &data, native);
        assert_eq!(
            published.status.code(),
            Some(0),
            "route {native}: {published:?}"
        );
        assert_eq!(
            std::fs::read(data.join("appendonly.aof")).expect("the published log"),
            b"hello\n"
        );
        assert!(!data.join("appendonly.aof.tmp").exists());
        assert!(!elsewhere.join("appendonly.aof").exists());

        // A relative path starts at the working directory the invocation
        // supplies as the base.
        std::fs::remove_file(data.join("appendonly.aof")).expect("remove the log");
        let relative = run(&program, fixture.path(), Path::new("data"), native);
        assert_eq!(
            relative.status.code(),
            Some(0),
            "route {native}: {relative:?}"
        );
        assert_eq!(
            std::fs::read(data.join("appendonly.aof")).expect("the published log"),
            b"hello\n"
        );

        // A missing directory is NotFound and stays missing.
        let missing = fixture.path().join("missing");
        let absent = run(&program, &elsewhere, &missing, native);
        assert_eq!(absent.status.code(), Some(10), "route {native}: {absent:?}");
        assert!(!missing.exists(), "a refused open created the directory");

        // A regular file where the directory belongs is NotDirectory.
        let file = fixture.path().join("file");
        std::fs::write(&file, b"not a directory").expect("write the file");
        let wrong = run(&program, &elsewhere, &file, native);
        assert_eq!(wrong.status.code(), Some(12), "route {native}: {wrong:?}");
        assert_eq!(std::fs::read(&file).expect("the file"), b"not a directory");

        // A directory below an unsearchable one is PermissionDenied.
        let locked = fixture.path().join("locked");
        std::fs::create_dir_all(locked.join("data")).expect("create the locked tree");
        close_path(&locked);
        let denied = run(&program, &elsewhere, &locked.join("data"), native);
        reopen_path(&locked, 0o755);
        assert_eq!(denied.status.code(), Some(11), "route {native}: {denied:?}");
        assert!(!locked.join("data").join("appendonly.aof").exists());
    }
}
