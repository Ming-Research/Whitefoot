//! Recursive summaries remain component-scoped through proof receipt reuse.

use super::{CacheDirectory, CompilerLimits, SourceInput};

const MAIN: &str = r#"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

fn recursive_program(base: u64) -> String {
    format!(
        r#"fn left(depth: u64) -> result: u64 pure contract {{
  ensures result == 1_u64;
}} {{
  if 0_u64 < depth {{
    let next = right(depth: depth - 1_u64);
    return next;
  }}
  return 1_u64;
}}

fn right(depth: u64) -> result: u64 pure contract {{
  ensures result == 1_u64;
}} {{
  if 0_u64 < depth {{
    let next = left(depth: depth - 1_u64);
    return next;
  }}
  return {base}_u64;
}}

{MAIN}"#
    )
}

/// An accepted member of a failed component must not leave a receipt that
/// claims an acceptance founded on an unverified induction hypothesis.
#[test]
fn a_failed_recursive_component_records_no_member_receipts() {
    assert_component_receipts(
        &recursive_program(1),
        &recursive_program(0),
        "[FN-9]",
        "failed-recursive-receipts",
    );
}

fn assert_component_receipts(valid: &str, invalid: &str, rule: &str, name: &str) {
    let directory = CacheDirectory::new(name);
    let cache = directory.open();
    let analyze = |source: &str| {
        let inputs = [SourceInput::new("recursive.wf", source.as_bytes())];
        let fresh = crate::driver::check(&inputs, CompilerLimits::default())
            .map_err(|failure| failure.to_string());
        let before = cache.receipt_counts();
        let cached = crate::driver::check_with_cache(&inputs, CompilerLimits::default(), &cache)
            .map_err(|failure| failure.to_string());
        assert_eq!(cached, fresh, "receipts must preserve the verdict");
        let after = cache.receipt_counts();
        (cached, (after.0 - before.0, after.1 - before.1))
    };
    assert_eq!(analyze(MAIN).0, Ok(()));
    for _ in 0..2 {
        let (verdict, (_, stored)) = analyze(invalid);
        assert!(
            verdict
                .as_ref()
                .is_err_and(|failure| failure.contains(rule)),
            "{verdict:?}"
        );
        assert_eq!(stored, 0, "neither recursive member may publish a receipt");
    }
    let (verdict, (_, stored)) = analyze(valid);
    assert_eq!(verdict, Ok(()));
    assert_eq!(stored, 2, "both recursive members now verify together");
    let (verdict, (reused, stored)) = analyze(valid);
    assert_eq!(verdict, Ok(()));
    assert!(reused >= 3, "the warm check reuses both members and main");
    assert_eq!(stored, 0);
    let (verdict, (reused, stored)) = analyze(invalid);
    assert!(
        verdict
            .as_ref()
            .is_err_and(|failure| failure.contains(rule)),
        "{verdict:?}"
    );
    assert!(reused >= 2, "the unchanged peer and main retain their receipts");
    assert_eq!(stored, 0, "the changed failing member receives no receipt");
}

fn recursive_range_program(base: u64) -> String {
    format!(
        r#"fn left(out: &[u64], depth: u64) -> done: unit writes(out) contract {{
  ensures forall cleared(k in 0_u64..out^.len): out^[k] == 0_u64;
}} {{
  right(out: out, depth: depth);
  return unit;
}}

fn right(out: &[u64], depth: u64) -> done: unit writes(out) contract {{
  ensures forall cleared(k in 0_u64..out^.len): out^[k] == 0_u64;
}} {{
  if 0_u64 < depth {{
    left(out: out, depth: depth - 1_u64);
    return unit;
  }}
  let length = out^.len;
  for (
    k in 0_u64..length,
    invariant forall filled(q in 0_u64..k): out^[q] == {base}_u64
  ) {{
    set out^[k] = {base}_u64;
  }}
  return unit;
}}

{MAIN}"#
    )
}

/// A component with only range clauses still owns one atomic receipt
/// boundary; ordinary entailment success cannot cache its unverified peer.
#[test]
fn a_failed_range_only_recursive_component_records_no_member_receipts() {
    assert_component_receipts(
        &recursive_range_program(0),
        &recursive_range_program(1),
        "[RANGE-3]",
        "failed-recursive-range-receipts",
    );
}
