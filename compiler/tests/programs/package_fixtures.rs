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

/// A package of stand-ins: a `clang` that logs its arguments, prints the
/// version line given and exits 0, a manifest when given, and the compiler under
/// test. Its builds fail (the stand-in makes no object); what these tests read
/// is which commands the compiler started in the package and what it said.
struct StandInPackage {
    root: std::path::PathBuf,
    log: std::path::PathBuf,
}

/// The files each capability probe writes for the clang; a command naming one
/// is a probe.
const PROBE_FILES: [&str; 4] = [
    "preserve_none_probe.ll",
    "captures_probe.ll",
    "coro_end_probe.ll",
    "darwin_stack_probe.ll",
];

impl StandInPackage {
    /// `version_printf` is the format `printf` writes for `--version`, so a
    /// CRLF line ending can be given as `\r\n`.
    fn new(tag: &str, manifest: Option<&str>, version_printf: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "whitefoot-facts-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("create the package's bin directory");
        let log = root.join("clang-calls.log");
        write_script(
            &bin.join("clang"),
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = --version ]; then printf '{version_printf}'; fi\nexit 0\n",
                log.display()
            ),
        );
        #[cfg(target_os = "linux")]
        write_script(&bin.join("ld.lld"), "#!/bin/sh\nexit 0\n");
        if let Some(text) = manifest {
            std::fs::write(root.join("toolchain-manifest.json"), text).expect("write the manifest");
        }
        std::fs::copy(crate::compiler_under_test(), bin.join("whitefootc"))
            .expect("place the compiler in the package");
        std::fs::write(root.join("waiting.wf"), WAITING).expect("write the program");
        // The scripts ran once to become startable.
        let _ = std::fs::remove_file(&log);
        Self { root, log }
    }

    /// Builds the waiting witness with `extra` options before the sources.
    fn build(&self, extra: &[&str]) -> std::process::Output {
        use std::process::Command;
        use std::time::Duration;
        output_within(
            Command::new(self.root.join("bin").join("whitefootc"))
                .args(extra)
                .arg("-o")
                .arg(self.root.join("program"))
                .arg(self.root.join("waiting.wf")),
            Duration::from_secs(120),
        )
        .expect("run the build")
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn probes(&self) -> usize {
        self.calls()
            .iter()
            .filter(|call| PROBE_FILES.iter().any(|file| call.contains(file)))
            .count()
    }
}

impl Drop for StandInPackage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The manifest of a package whose clang is `version`, recording the answers
/// the stand-in would not give (a stand-in accepts every probe).
fn record_for(version: &str) -> String {
    format!(
        "{{\"llvm_version\":\"22.1.8\",\"files\":[{{\"path\":\"bin/clang\",\"size\":1}}],\"clang_facts\":{{\"clang_version\":\"{version}\",\
         \"preserve_none\":false,\"no_capture_attribute\":\"nocapture\",\"coro_end_result\":\"i1\",\
         \"darwin_stack_probe_helper\":false}}}}"
    )
}

/// Reference: the work's item (the package's record is taken in place of the
/// probes when it is for the clang that was found) and the unchanged probing
/// of every other case. A recorded package runs no probe in a real build; the
/// stand-in's log is the witness, and it fails on the compiler before the change
/// (which always probes).
#[test]
fn a_build_with_a_recorded_answer_for_this_clang_starts_no_probe_and_says_nothing() {
    let package = StandInPackage::new(
        "taken",
        Some(&record_for("clang version 22.1.8")),
        "clang version 22.1.8\\n",
    );
    let built = package.build(&[]);
    let calls = package.calls();
    assert_eq!(calls.first().map(String::as_str), Some("--version"), "{calls:?}");
    assert_eq!(package.probes(), 0, "a probe ran: {calls:?}");
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(!stderr.contains("were not taken"), "{stderr}");
    assert!(!stderr.contains("note:"), "{stderr}");
}

/// A Windows clang ends its version line with CRLF; the record made from the
/// compiler's own report holds the line without the carriage return, and must
/// still be recognised.
#[test]
fn a_version_line_ending_in_crlf_matches_its_record() {
    let package = StandInPackage::new(
        "crlf",
        Some(&record_for("clang version 22.1.8")),
        "clang version 22.1.8\\r\\nTarget: x86_64-w64-windows-gnu\\n",
    );
    let _ = package.build(&[]);
    assert_eq!(package.probes(), 0, "{:?}", package.calls());
}

/// Every way the record can fail to be taken probes the clang, says so once on
/// standard error in the requested format, and names the reason: another clang
/// version (both lines named), a manifest that is not JSON, an answer missing or
/// of the wrong type, no `clang_facts`, and a manifest that cannot be read.
#[test]
fn a_record_that_cannot_be_taken_probes_and_the_note_names_why() {
    let good = record_for("clang version 22.1.8");
    let cases: Vec<(&str, Option<String>, &[&str])> = vec![
        (
            "version",
            Some(record_for("clang version 21.0.0")),
            &["clang version 21.0.0", "clang version 22.1.8"],
        ),
        (
            "truncated",
            Some(good[..good.len() - 12].to_owned()),
            &["not valid JSON"],
        ),
        (
            "trailing-comma",
            Some(good.replace("\"size\":1}", "\"size\":1,}")),
            &["not valid JSON"],
        ),
        (
            "missing",
            Some(good.replace("\"coro_end_result\":\"i1\",", "")),
            &["coro_end_result"],
        ),
        (
            "wrong-type",
            Some(good.replace("\"preserve_none\":false", "\"preserve_none\":\"no\"")),
            &["preserve_none"],
        ),
        (
            "unknown-spelling",
            Some(good.replace("\"nocapture\"", "\"readonly\"")),
            &["no_capture_attribute"],
        ),
        ("no-facts", Some("{\"llvm_version\":\"22.1.8\"}".to_owned()), &["clang_facts"]),
        ("no-manifest", None, &["toolchain-manifest.json"]),
    ];
    for (tag, manifest, names) in cases {
        let package = StandInPackage::new(tag, manifest.as_deref(), "clang version 22.1.8\\n");
        let built = package.build(&[]);
        assert!(package.probes() >= 3, "{tag}: no probes ran: {:?}", package.calls());
        let stderr = String::from_utf8_lossy(&built.stderr);
        let notes: Vec<&str> = stderr
            .lines()
            .filter(|line| line.starts_with("whitefootc: note: "))
            .collect();
        assert_eq!(notes.len(), 1, "{tag}: {stderr}");
        for name in names {
            assert!(notes[0].contains(name), "{tag}: {name:?} not in {:?}", notes[0]);
        }

        let json = package.build(&["--diagnostic-format", "json"]);
        let stderr = String::from_utf8_lossy(&json.stderr);
        let notes: Vec<&str> = stderr
            .lines()
            .filter(|line| line.contains("\"kind\":\"Note\""))
            .collect();
        assert_eq!(notes.len(), 1, "{tag}: {stderr}");
        assert!(
            notes[0].starts_with('{')
                && notes[0].ends_with('}')
                && notes[0].contains("\"stage\":\"Driver\"")
                && notes[0].contains("\"category\":\"Toolchain\""),
            "{tag}: {}",
            notes[0]
        );
        // The note changes neither the verdict nor the status.
        assert_eq!(built.status.code(), json.status.code(), "{tag}");
    }
}

/// RFC 8259 reference: a number is `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`.
/// A manifest holding a value that is not a JSON number is not JSON, so its
/// record is not trusted.
#[test]
fn a_manifest_with_a_malformed_number_is_not_trusted() {
    let good = record_for("clang version 22.1.8");
    for (tag, bad) in [
        ("minus", "1-2"),
        ("zero", "01"),
        ("dot", "1."),
        ("exponent", "1e"),
        ("sign", "1+2"),
    ] {
        let manifest = good.replace("\"size\":1}", &format!("\"size\":{bad}}}"));
        assert_ne!(manifest, good);
        let package = StandInPackage::new(
            &format!("number-{tag}"),
            Some(&manifest),
            "clang version 22.1.8\\n",
        );
        let built = package.build(&[]);
        let stderr = String::from_utf8_lossy(&built.stderr);
        assert!(
            stderr.contains("not valid JSON") && package.probes() >= 3,
            "{bad}: the record was taken from a manifest that is not JSON: {stderr}"
        );
    }
}

/// With the reference being the item's own result for the other two
/// installations: a compiler beside no clang (a source build or a lightweight
/// package) probes and reports no reason and no note; the compiler inside a
/// bundled package (the selected-clang run of the release acceptance, which
/// names it by `WHITEFOOT_TEST_COMPILER`) takes the manifest's answers with no
/// probe.
#[test]
fn the_report_of_a_real_build_names_where_the_answers_came_from() {
    use std::process::Command;
    use std::time::Duration;
    let root = std::env::temp_dir().join(format!(
        "whitefoot-facts-report-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch directory");
    std::fs::write(root.join("waiting.wf"), WAITING).expect("write the program");
    let built = output_within(
        Command::new(crate::compiler_under_test())
            .arg("--report")
            .arg("-o")
            .arg(root.join("program"))
            .arg(root.join("waiting.wf")),
        Duration::from_secs(300),
    )
    .expect("run the build");
    assert!(built.status.success(), "{built:?}");
    let stdout = String::from_utf8_lossy(&built.stdout);
    let stderr = String::from_utf8_lossy(&built.stderr);
    let line = stdout
        .lines()
        .find(|line| line.starts_with("{\"build\":"))
        .unwrap_or_else(|| panic!("no report line: {stdout}"));
    let facts = line
        .split_once("\"toolchain_facts\":")
        .unwrap_or_else(|| panic!("no toolchain_facts: {line}"))
        .1;
    assert!(!stderr.contains("note:"), "{stderr}");
    if std::env::var_os("WHITEFOOT_TEST_COMPILER").is_some() {
        assert!(
            facts.starts_with("{\"source\":\"manifest\",\"reason\":null,\"probes_run\":0,"),
            "{facts}"
        );
    } else {
        assert!(
            facts.starts_with("{\"source\":\"probed\",\"reason\":null,\"probes_run\":")
                && facts.contains("\"clang_version\":null"),
            "{facts}"
        );
        assert!(!facts.contains("\"probes_run\":0,"), "{facts}");
    }
    let _ = std::fs::remove_dir_all(&root);
}
