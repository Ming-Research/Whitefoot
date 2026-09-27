//! Entry/exit measure contracts over a reference parameter share the normal
//! proof flow.
//!
//! v0.59 keyed these contracts on the `&uniq` permission marker. v0.60 has one
//! reference kind and reads write permission off the declared row [EFF-1], so
//! the subject here is a contract over a reference parameter whose row carries
//! `writes`: its entry state through `entry(p)` [MSR-3], its exit facts
//! [CALL-6, FN-9], and the pairwise refusal two overlapping arguments receive
//! [EFF-5].

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::with_semantics;

fn assert_rule(source: &[u8], rule: SemanticRule) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("expected {rule:?}, got {outcome:?}");
        };
        assert_eq!(issue.rule(), rule, "{issue:?}");
    });
}

fn assert_complete(source: &str) {
    with_semantics(source.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

#[test]
fn generic_unit_helper_publishes_only_proved_written_state_relations() {
    let source = r#"fn touch<T>(values: &Slots<T, 4>) -> result: unit writes(values) contract {
  requires deref(values).len >= 1_u64;
  ensures deref(values).len == deref(entry(values)).len;
} {
  let value = take_back(window: values);
  place_back(window: values, value: move value);
  return unit;
}

fn exercise<T>(values: &Slots<T, 4>) -> result: unit writes(values) contract {
  requires deref(values).len >= 1_u64;
} {
  touch::<T>(values: values);
  let value = take_back(window: values);
  place_back(window: values, value: move value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u64, 4>();
  place_back(window: &values, value: 7_u64);
  exercise::<u64>(values: &values);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    // A return must still prove the promised state, and a call with no
    // ensures must still lose the old length fact at its write boundary.
    // Keep the complete take/place body and make its promise false. This
    // isolates return-proof checking; abandoning an opaque linear T would
    // instead test the independent PROV-6 consumption restriction.
    assert_rule(
        source
            .replace(
                "ensures deref(values).len == deref(entry(values)).len;",
                "ensures deref(values).len == deref(entry(values)).len + 1_u64;",
            )
            .as_bytes(),
        SemanticRule::Fn9,
    );
    // Without the published relation the caller's own `take_back` has no
    // length fact left, and the PRE-1 row's requirement is undischarged at
    // that call. This was v0.59's [BLK-0] kernel-row requirement; a window
    // operation is an ordinary [PRE-1] record now, so its requirement is
    // [FN-8]'s.
    assert_rule(
        source
            .replace(
                "  ensures deref(values).len == deref(entry(values)).len;\n",
                "",
            )
            .as_bytes(),
        SemanticRule::Fn8,
    );
    // Unit is still not an admitted result datum.
    assert_rule(
        source
            .replace(
                "ensures deref(values).len == deref(entry(values)).len;",
                "ensures result == unit;",
            )
            .as_bytes(),
        SemanticRule::Fn9,
    );
}

const PUSH: &str = r#"fn push(values: &Slots<u64, 4>, value: u64) -> result: unit writes(values) contract {
  requires deref(values).len < deref(values).cap;
  ensures deref(values).len == deref(entry(values)).len + 1_u64;
  ensures deref(values).cap == deref(entry(values)).cap;
} {
  place_back(window: values, value: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u64, 4>();
  push(values: &values, value: 7_u64);
  invariant upper: values.len <= 1_u64;
  invariant lower: values.len >= 1_u64;
  let last = take_back(window: &values);
  invariant empty: values.len <= 0_u64;
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn a_written_push_proves_entry_and_exit_and_publishes_without_result() {
    assert_complete(PUSH);
    with_semantics(PUSH.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("{outcome:?}");
        };
        for function in &checked.data.functions {
            super::entailment::validate_derivations(&function.entailment);
            let mut state_clauses = std::collections::HashSet::new();
            for root in &function.entailment.derivations.roots {
                if matches!(
                    root.kind,
                    super::super::entailment::DerivationRootKind::PostconditionState { .. }
                ) {
                    let super::super::entailment::DerivationNode::PostconditionCall { detail } =
                        &function.entailment.derivations.nodes[root.node.0 as usize]
                    else {
                        panic!("a state root retains its call evidence");
                    };
                    assert!(
                        state_clauses.insert((
                            detail.call.clone(),
                            detail.summary.clone(),
                            detail.relation.clone()
                        )),
                        "binding a take result must not republish its state clauses"
                    );
                }
            }
        }
    });
}

#[test]
fn exit_state_cannot_be_its_own_increment() {
    let source = PUSH.replace(
        "deref(entry(values)).len + 1_u64",
        "deref(values).len + 1_u64",
    );
    assert_rule(source.as_bytes(), SemanticRule::Call6);
}

#[test]
fn entry_former_rejects_body_and_read_only_parameter() {
    let body = PUSH.replace(
        "return unit;",
        "let size = deref(entry(values)).len;\n  return unit;",
    );
    assert_rule(body.as_bytes(), SemanticRule::Msr3);
    // A reference parameter whose row declares no write of the path has no
    // exit state, so `entry` names nothing it could be distinguished from.
    let read_only = r#"fn observe(values: &Slots<u64, 4>) -> result: u64 reads(values) contract {
  ensures result == deref(entry(values)).len;
} {
  return deref(values).len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(read_only.as_bytes(), SemanticRule::Msr3);
}

#[test]
fn a_writing_call_without_ensures_requires_a_runtime_reread() {
    let source = PUSH
        .replace(
            "  ensures deref(values).len == deref(entry(values)).len + 1_u64;\n",
            "",
        )
        .replace(
            "  ensures deref(values).cap == deref(entry(values)).cap;\n",
            "",
        );
    assert_rule(source.as_bytes(), SemanticRule::Inv1);
    let guarded = source.replace(
        "  invariant upper: values.len <= 1_u64;\n  invariant lower: values.len >= 1_u64;\n  let last = take_back(window: &values);\n  invariant empty: values.len <= 0_u64;",
        "  let size = values.len;\n  if size > 0_u64 {\n    let last = take_back(window: &values);\n  }",
    );
    assert_complete(&guarded);
}

#[test]
fn whole_referent_assignment_kills_the_old_window_facts() {
    // v0.59 wrote this as `let old = replace deref(values) = move empty;`.
    // [SET-1] with [WIN-3]'s disposition is the successor: the assignment
    // releases the displaced affine window instead of reading it out.
    let source = r#"fn clear(values: &Slots<u64, 4>) -> result: unit writes(values) {
  let empty = slots_new::<u64, 4>();
  set deref(values) = move empty;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u64, 4>();
  place_back(window: &values, value: 7_u64);
  clear(values: &values);
  let last = take_back(window: &values);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(source.as_bytes(), SemanticRule::Fn8);
}

#[test]
fn exit_facts_publish_beside_multiple_results() {
    let source = r#"fn pop(values: &Slots<u64, 4>) -> (value: u64, count: u64) writes(values) contract {
  requires deref(values).len > 0_u64;
  ensures deref(values).len + 1_u64 == deref(entry(values)).len;
  ensures count == deref(values).len;
} {
  let value = take_back(window: values);
  let count = deref(values).len;
  return value, count;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u64, 4>();
  place_back(window: &values, value: 7_u64);
  let (value, count) = pop(values: &values);
  invariant empty: values.len <= 0_u64;
  invariant reported: count <= 0_u64;
  if value != 7_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn nested_field_effects_preserve_disjoint_support() {
    let source = r#"struct Pair {
  changed: Slots<u64, 4>;
  untouched: Slots<u64, 4>;
}

fn push(pair: &Pair, value: u64) -> result: unit writes(pair.changed) contract {
  requires deref(pair).changed.len < deref(pair).changed.cap;
  ensures deref(pair).changed.len == deref(entry(pair)).changed.len + 1_u64;
} {
  place_back(window: &deref(pair).changed, value: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let changed = slots_new::<u64, 4>();
  let untouched = slots_new::<u64, 4>();
  let pair = Pair(changed: move changed, untouched: move untouched);
  push(pair: &pair, value: 7_u64);
  invariant changed: pair.changed.len >= 1_u64;
  invariant unchanged: pair.untouched.len <= 0_u64;
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn a_writing_call_invalidates_a_surviving_reference() {
    // v0.59 refused the surviving loan at [OWN-5]. [EFF-5] clause 3 is the
    // successor: a live reference outside the call whose path has a proper
    // prefix among the call's substituted write paths becomes invalid after
    // the call, and the use of an invalid reference is [REF-2]'s rejection.
    let helper = PUSH.split("fn main()").next().unwrap();
    let source = format!(
        "{helper}{}",
        r#"fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u64, 4>();
  place_back(window: &values, value: 1_u64);
  place_back(window: &values, value: 2_u64);
  let seen = &values[0_u64];
  push(values: &values, value: 3_u64);
  let observed = deref(seen);
  return std::process::exit_status(code: 0_u8);
}
"#
    );
    assert_rule(source.as_bytes(), SemanticRule::Ref2);
}

// Retired with the borrow-mode result of [FN-1]: v0.59's
// `an_exit_only_clause_does_not_use_an_ordinary_borrow_result_as_a_datum`
// returned `&'r u64` beside an exit-only `ensures`, and v0.60 returns owned
// values only and refuses a returned reference at [REF-3]
// (`EscapingReference`); the exit-only clause whose result is not an admitted
// datum survives in
// `generic_unit_helper_publishes_only_proved_written_state_relations`.

#[test]
fn assigning_the_actual_after_a_call_kills_its_exit_only_relation() {
    let helper = PUSH
        .split("fn main()")
        .next()
        .unwrap()
        .replace("-> result: unit", "-> result: Slots<u64, 4>")
        .replace("  return unit;", "  return slots_new::<u64, 4>();");
    let source = format!(
        "{helper}{}",
        r#"fn overwrite(values: &Slots<u64, 4>) -> result: unit writes(values) contract {
  requires deref(values).len < deref(values).cap;
} {
  let replacement = push(values: values, value: 7_u64);
  set deref(values) = move replacement;
  let last = take_back(window: values);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#
    );
    assert_rule(source.as_bytes(), SemanticRule::Fn8);
}

#[test]
fn written_state_equality_requires_both_affine_bounds() {
    let source = r#"fn fill(slots: &Slots<u8, 8>, count: u64) -> result: unit writes(slots) contract {
  requires deref(slots).len == 0_u64;
  requires count <= deref(slots).cap - deref(slots).len;
  ensures deref(slots).len == count;
} {
  for (
    index in 0_u64..count,
    invariant filled: deref(slots).len >= index,
    invariant bounded: deref(slots).len <= index,
    invariant spare: deref(slots).cap + index >= deref(slots).len + count
  ) {
    place_back(window: slots, value: 0_u8);
  }
  invariant complete_min: deref(slots).len >= count;
  invariant complete_max: deref(slots).len <= count;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    for (header, fact) in [
        (
            "    invariant filled: deref(slots).len >= index,\n",
            "  invariant complete_min: deref(slots).len >= count;\n",
        ),
        (
            "    invariant bounded: deref(slots).len <= index,\n",
            "  invariant complete_max: deref(slots).len <= count;\n",
        ),
    ] {
        let missing_direction = source.replace(header, "").replace(fact, "");
        assert_rule(missing_direction.as_bytes(), SemanticRule::Fn9);
    }
}

#[test]
fn a_boxed_window_publishes_to_the_typed_referent() {
    let helper = PUSH.split("fn main()").next().unwrap();
    let source = format!(
        "{helper}{}",
        r#"fn main() -> status: std::process::ExitStatus pure {
  let empty = slots_new::<u64, 4>();
  let owner = box_new::<Slots<u64, 4>>(value: move empty);
  let filled = owner.inner.len;
  if filled < 4_u64 {
    push(values: &owner.inner, value: 7_u64);
    invariant changed: owner.inner.len >= 1_u64;
    let held = &owner.inner[0_u64];
    let observed = deref(held);
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#
    );
    assert_complete(&source);
    // Writing the whole window through its owner is a write of a proper
    // prefix of the element reference's path, so the reference is invalid at
    // its next use [REF-2].
    let surviving = source.replace(
        "    let held = &owner.inner[0_u64];\n    let observed = deref(held);",
        "    let held = &owner.inner[0_u64];\n    let fresh = slots_new::<u64, 4>();\n    set owner.inner = move fresh;\n    let observed = deref(held);",
    );
    assert_rule(surviving.as_bytes(), SemanticRule::Ref2);
}

#[test]
fn two_overlapping_written_arguments_are_refused_pairwise() {
    // [EFF-5] clause 1: the effects two arguments supply are compared, and
    // two overlapping paths at least one of which writes are a hard error at
    // the complete `call`. v0.59 spelled the same refusal as a loan conflict
    // between two `&uniq` actuals.
    let source = r#"fn copy_first(source: &Slots<u64, 4>, destination: &Slots<u64, 4>) -> result: unit reads(source), writes(destination) contract {
  requires deref(source).len > 0_u64;
  requires deref(destination).len < deref(destination).cap;
} {
  let value = deref(source)[0_u64];
  place_back(window: destination, value: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = slots_new::<u64, 4>();
  let second = slots_new::<u64, 4>();
  place_back(window: &first, value: 7_u64);
  copy_first(source: &first, destination: &second);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    let aliased = source.replace(
        "copy_first(source: &first, destination: &second);",
        "copy_first(source: &first, destination: &first);",
    );
    with_semantics(aliased.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("expected EFF-5, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Eff5, "{issue:?}");
        assert!(
            matches!(
                issue.kind(),
                SemanticIssueKind::OverlappingCallEffects { .. }
            ),
            "{issue:?}"
        );
    });
}
