//! The witness programs the release package acceptance builds with the packaged
//! compiler (`.github/toolchain/check-package.sh`, `package-acceptance.yml`).
//! They are built here with the gate's own toolchain so a language change that
//! breaks one fails the gate, not the next release run, and so the result the
//! acceptance expects is stated where a failure can be traced to the source.

use std::os::unix::process::ExitStatusExt;

use super::support::{compile_and_run, compile_sources, output_within};

const WAITING: &[u8] = include_bytes!("../../../tests/package-fixtures/waiting.wf");
const STACK_EXHAUSTION: &[u8] =
    include_bytes!("../../../tests/package-fixtures/stack-exhaustion.wf");

/// An owned enum crosses a direct waiting call and a bound spawn whose result
/// is used, so the build lowers coroutines; exit 1 or 2 names a wrong result.
#[test]
fn the_waiting_witness_runs_to_a_zero_exit() {
    let output = compile_and_run(&compile_sources(&[("waiting.wf", WAITING)]));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// Every activation holds a frame larger than a guard page, and the recursion
/// is deeper than the entry's stack, so the program ends only if each frame is
/// probed into the guard: the floor's abort, with its one record on standard
/// error. An unprobed frame would step over the guard and end in a host fault
/// with no record. Each activation sums its array after the recursive call, so
/// the optimizer must keep the whole array in the frame; a reader that takes
/// one element lets the optimizer shrink the frame, and the program then
/// returns normally.
#[test]
fn the_stack_exhaustion_witness_ends_in_the_floor_abort_and_its_record() {
    let output = compile_and_run(&compile_sources(&[(
        "stack-exhaustion.wf",
        STACK_EXHAUSTION,
    )]));
    assert_eq!(output.status.signal(), Some(6), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "{\"resource\":\"stack\"}\n",
        "{output:?}"
    );
}

/// Writes an executable script and waits until the kernel stops refusing to
/// start it as busy, which a thread forking while the file was open for writing
/// can cause on Linux.
fn write_script(path: &std::path::Path, text: &str) {
    use std::io::ErrorKind;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::time::{Duration, Instant};
    std::fs::write(path, text).expect("write a script");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("make the script executable");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match output_within(Command::new(path).arg("--version"), Duration::from_secs(30)) {
            Err(error) if error.kind() == ErrorKind::ExecutableFileBusy => {
                assert!(Instant::now() < deadline, "{} stayed busy", path.display());
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("cannot start {}: {error}", path.display()),
            Ok(_) => return,
        }
    }
}

/// A command that never builds natively settles no toolchain, so the package
/// beside the compiler is asked nothing: no `--version`, no probe, and no note
/// about probing. A build in the same package does start its clang, so the log
/// would show a start by the check.
#[test]
fn a_check_starts_no_clang_of_the_package_beside_the_compiler() {
    use std::process::Command;
    use std::time::Duration;
    let root = std::env::temp_dir().join(format!(
        "whitefoot-check-in-a-package-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).expect("create the package's bin directory");
    let log = root.join("clang-calls.log");
    write_script(
        &bin.join("clang"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = --version ]; then printf '%s\\n' 'clang version 22.1.8'; fi\nexit 0\n",
            log.display()
        ),
    );
    // A Linux package names its linker beside the clang; a macOS one finds the
    // system's.
    #[cfg(target_os = "linux")]
    write_script(&bin.join("ld.lld"), "#!/bin/sh\nexit 0\n");
    // The package marker, recording no clang answers.
    std::fs::write(root.join("toolchain-manifest.json"), "{}").expect("write the manifest");
    std::fs::copy(crate::compiler_under_test(), bin.join("whitefootc"))
        .expect("place the compiler in the package");
    std::fs::write(root.join("waiting.wf"), WAITING).expect("write the program");
    // The scripts ran once to become startable.
    let _ = std::fs::remove_file(&log);

    let compiler = bin.join("whitefootc");
    let checked = output_within(
        Command::new(&compiler)
            .arg("--check")
            .arg(root.join("waiting.wf")),
        Duration::from_secs(120),
    )
    .expect("run the check");
    assert!(checked.status.success(), "{checked:?}");
    assert!(
        !log.exists(),
        "a check started the package's clang: {}",
        std::fs::read_to_string(&log).unwrap_or_default()
    );
    assert!(
        !String::from_utf8_lossy(&checked.stderr).contains("were not taken"),
        "{checked:?}"
    );

    // The control: a build prepares the toolchain, so the package's clang is
    // asked, first for its version. (The stand-in clang makes no object, so the
    // build itself fails, which this test does not ask about.)
    let _ = output_within(
        Command::new(&compiler)
            .arg("-o")
            .arg(root.join("program"))
            .arg(root.join("waiting.wf")),
        Duration::from_secs(120),
    )
    .expect("run the build");
    let calls = std::fs::read_to_string(&log).expect("a build starts the package's clang");
    assert_eq!(calls.lines().next(), Some("--version"), "{calls}");
    let _ = std::fs::remove_dir_all(&root);
}
