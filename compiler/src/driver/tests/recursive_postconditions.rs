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
    let next_depth = depth - 1_u64;
    let next = right(depth: next_depth);
    return next;
  }}
  return 1_u64;
}}

fn right(depth: u64) -> result: u64 pure contract {{
  ensures result == 1_u64;
}} {{
  if 0_u64 < depth {{
    let next_depth = depth - 1_u64;
    let next = left(depth: next_depth);
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
    let next_depth = depth - 1_u64;
    left(out: out, depth: next_depth);
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

/// A caller's source verdict depends on the foreign interface, so a foreign
/// body edit preserves that verdict. Composition must still validate the
/// foreign member before any cached caller acceptance can authorize a program.
#[test]
fn a_foreign_recursive_member_failure_preserves_only_the_callers_cached_verdict() {
    const GRAPH: &[u8] = include_bytes!(
        "../../../../tests/conformance/cases/fn9-pos-recursive-cross-module-written/modules.wfg"
    );
    const CALLER_INTERFACE: &[u8] = include_bytes!(
        "../../../../tests/conformance/cases/fn9-pos-recursive-cross-module-written/module.wfm"
    );
    const CALLER_BODY: &[u8] = include_bytes!(
        "../../../../tests/conformance/cases/fn9-pos-recursive-cross-module-written/main.wf"
    );
    const RELAY_INTERFACE: &[u8] = include_bytes!(
        "../../../../tests/conformance/cases/fn9-pos-recursive-cross-module-written/relay/module.wfm"
    );
    const VALID_RELAY: &[u8] = include_bytes!(
        "../../../../tests/conformance/cases/fn9-pos-recursive-cross-module-written/relay/relay.wf"
    );
    const INVALID_RELAY: &[u8] = include_bytes!(
        "../../../../tests/conformance/cases/fn9-neg-recursive-cross-module-member-return/relay/relay.wf"
    );
    let records = |relay_body: &'static [u8]| -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("relay/module.wfm", RELAY_INTERFACE),
            ("relay/relay.wf", relay_body),
            ("module.wfm", CALLER_INTERFACE),
            ("main.wf", CALLER_BODY),
        ]
    };
    let directory = CacheDirectory::new("foreign-recursive-member");
    let cache = directory.open();
    let original = records(VALID_RELAY);
    let cold = super::verdicts(GRAPH, &original, None);
    assert!(cold.iter().all(|(_, outcome, reused)| {
        matches!(outcome, crate::driver::CheckOutcome::Accepted { .. }) && !reused
    }));
    let seeded = super::verdicts(GRAPH, &original, Some(&cache));
    assert_eq!(seeded, cold, "the cold cached check verifies every subject");
    let warm = super::verdicts(GRAPH, &original, Some(&cache));
    assert!(warm.iter().all(|(_, outcome, reused)| {
        matches!(outcome, crate::driver::CheckOutcome::Accepted { .. }) && *reused
    }));

    // All records other than the foreign implementation retain identical bytes.
    let edited = records(INVALID_RELAY);
    let fresh_failure = super::verdicts(GRAPH, &edited, None);
    let cached_failure = super::verdicts(GRAPH, &edited, Some(&cache));
    assert_eq!(
        cached_failure
            .iter()
            .map(|(subject, outcome, _)| (subject, outcome))
            .collect::<Vec<_>>(),
        fresh_failure
            .iter()
            .map(|(subject, outcome, _)| (subject, outcome))
            .collect::<Vec<_>>(),
        "the previous composition acceptance cannot hide the foreign failure"
    );
    let (_, fresh_caller, _) = fresh_failure
        .iter()
        .find(|(subject, _, _)| subject == "pkg")
        .expect("the caller module's fresh source verdict");
    assert!(matches!(
        fresh_caller,
        crate::driver::CheckOutcome::Accepted { .. }
    ));
    let (_, caller, reused) = cached_failure
        .iter()
        .find(|(subject, _, _)| subject == "pkg")
        .expect("the caller module's separate source verdict");
    assert!(matches!(caller, crate::driver::CheckOutcome::Accepted { .. }));
    assert!(*reused, "the caller still reads the same foreign interface");
    for subject in ["pkg::relay", "main"] {
        let (_, outcome, reused) = cached_failure
            .iter()
            .find(|(name, _, _)| name == subject)
            .expect("the foreign module and named composition are both checked");
        let crate::driver::CheckOutcome::Rejected { rule, failure, .. } = outcome else {
            panic!("{subject} must reject its foreign member: {outcome:?}");
        };
        assert_eq!(rule.as_deref(), Some("FN-9"));
        assert!(failure.starts_with("relay/relay.wf:"), "{failure}");
        assert!(!reused, "the foreign edit invalidates {subject}'s verdict");
    }
    assert_eq!(
        super::verdicts(GRAPH, &original, Some(&cache)),
        warm,
        "restoring the foreign implementation reuses the valid composition"
    );
}
