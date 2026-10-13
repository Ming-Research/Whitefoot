use std::path::{Path, PathBuf};
use std::process::Command;

use super::support::{build_program, compile_and_run, compile_program, fixture_directory};

/// The directory of this module's programs and its oracle.
fn float_text_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler package must live directly under the repository root")
        .join("tests")
        .join("programs")
        .join("float_text")
}

/// The first reply line that differs from the expected one, with the request
/// that asked for it, or `None` when the replies are the expected ones.
fn first_difference(requests: &str, expected: &str, actual: &str) -> Option<String> {
    let mut asked = requests.lines();
    let mut wanted = expected.lines();
    let mut received = actual.lines();
    let mut number = 0_usize;
    loop {
        number += 1;
        let request = asked.next();
        let want = wanted.next();
        let got = received.next();
        if request.is_none() && want.is_none() && got.is_none() {
            return None;
        }
        if want != got {
            let shown: String = request.unwrap_or("").chars().take(80).collect();
            return Some(format!(
                "line {number}, request {shown:?}: expected {want:?}, got {got:?}"
            ));
        }
    }
}

#[test]
fn first_difference_names_the_first_wrong_line() {
    assert_eq!(first_difference("Ra\nRb\n", "x\ny\n", "x\ny\n"), None);
    let difference = first_difference("Ra\nRb\n", "x\ny\n", "x\nz\n").expect("a difference");
    assert!(difference.starts_with("line 2, request \"Rb\""), "{difference}");
    assert!(first_difference("Ra\n", "x\n", "x\ny\n").is_some());
    assert!(first_difference("Ra\nRb\n", "x\ny\n", "x\n").is_some());
}

/// `i64_write` against text and extents computed by hand, through the ordinary
/// pipeline: the whole 32-byte buffer is compared with its expected image,
/// which also shows that no byte outside the window changes.
#[test]
fn integers_are_written_as_decimal_text_into_the_callers_window() {
    let llvm = compile_program("float_text/integer_write.wf");
    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// Reading and writing through the public interface against the
/// exact-rational oracle. `reference.py generate` writes, for each family of
/// requests (the named cases and the sweeps), the request lines and the reply
/// line each must get; `driver.wf` answers the requests on standard input,
/// and its output is compared with the expected one line by line. Run in CI
/// with the rest of the corpus; python3 is a gate prerequisite.
#[test]
#[ignore = "std::float_text declares no f64_read, f64_shortest or f64_significant yet: they wait for the decimal right shift and front shift (board item float-text-stage1-own)"]
fn reads_and_writes_agree_with_the_exact_rational_oracle() {
    let llvm = compile_program("float_text/driver.wf");
    let program = build_program(&llvm);
    let directory = fixture_directory();
    let generated = Command::new("python3")
        .arg("-I")
        .arg("-B")
        .arg(float_text_directory().join("reference.py"))
        .arg("generate")
        .arg(directory.path())
        .output()
        .expect("run the oracle");
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let mut families: Vec<PathBuf> = std::fs::read_dir(directory.path())
        .expect("read the generated families")
        .map(|entry| entry.expect("a generated file").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "in"))
        .collect();
    families.sort();
    assert!(!families.is_empty(), "the oracle generated no family");
    for requests in families {
        let name = requests.display().to_string();
        let asked = std::fs::read_to_string(&requests).expect("read the requests");
        let expected =
            std::fs::read_to_string(requests.with_extension("out")).expect("read the replies");
        let output = program.run_with_piped_input(asked.as_bytes(), true);
        assert!(
            output.status.success(),
            "{name}: {:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty(), "{name}: unexpected standard error");
        let actual = String::from_utf8(output.stdout).expect("the replies are text");
        if let Some(difference) = first_difference(&asked, &expected, &actual) {
            panic!("{name}: {difference}");
        }
    }
}
