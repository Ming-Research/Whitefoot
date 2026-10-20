//! Temporary (coord-wfbl-04-03): which stack-exhaustion witness keeps a physical
//! frame under the ordinary optimizer. Removed with the witness's final form.

use std::os::unix::process::ExitStatusExt;

use super::support::{compile_and_run, compile_sources};

fn observe(name: &str, source: &[u8]) {
    let output = compile_and_run(&compile_sources(&[(name, source)]));
    eprintln!(
        "VARIANT {name}: signal={:?} code={:?} stdout={:?} stderr={:?}",
        output.status.signal(),
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.signal(), Some(6), "{name}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "{\"resource\":\"stack\"}\n",
        "{name}"
    );
}

#[test]
fn variant_a() {
    observe(
        "a",
        include_bytes!("../../../tests/package-fixtures/tmp-stack-a.wf"),
    );
}

#[test]
fn variant_b() {
    observe(
        "b",
        include_bytes!("../../../tests/package-fixtures/tmp-stack-b.wf"),
    );
}

#[test]
fn variant_c() {
    observe(
        "c",
        include_bytes!("../../../tests/package-fixtures/tmp-stack-c.wf"),
    );
}
