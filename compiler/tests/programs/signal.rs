use super::support::{compile_and_run, compile_program};

#[test]
fn fir_filter_executes_with_nested_fixed_array_state() {
    let llvm = compile_program("fir_filter.wf");
    // Re-derived for v0.60: the delay line is a `Slots<f64, 8>` and its block
    // is header-first, `{ i64 len, [8 x double] slots }` [WIN-1, STOR-1], so
    // the frame slot the program addresses carries that shape rather than the
    // bare element array v0.59's fixed run emitted.
    // The complete window may be an independent qualified frame root; its
    // own header-first address no longer depends on an outer frame GEP.
    assert!(llvm.contains("getelementptr inbounds { i64, [8 x double] }"));
    // Both enclosing records are addressed directly; element updates no
    // longer require reconstructing the DelayLine and FirFilter values.
    assert!(llvm.contains(&format!(
        "getelementptr inbounds {}",
        super::support::nominal_type("DelayLine")
    )));
    assert!(llvm.contains(&format!(
        "getelementptr inbounds {}",
        super::support::nominal_type("FirFilter")
    )));
    assert!(!llvm.contains("call void @wf_trap"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
