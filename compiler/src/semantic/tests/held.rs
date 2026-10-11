//! Structural observation contracts retain proof metadata and no executable token.
use crate::{SemanticOutcome, SemanticRule};
use super::{assert_accepts, assert_rule_kind, with_semantics};

#[test]
fn a_program_capture_export_item_has_no_snapshot_type_graph_constraint() {
    assert_accepts(b"fn capture_export<T: drop>(value: T) -> result: T pure {\n  return move value;\n}\n\nfn ordinary(value: Shared<u64>) -> result: Shared<u64> pure {\n  return capture_export::<Shared<u64>>(value: move value);\n}\n");
}

#[test]
fn forwarded_observations_retain_symbolic_and_concrete_origins() {
    let source = b"fn observe(value: &u64) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return value^;\n}\n\nfn forward(value: &u64) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return observe(value: value);\n}\n\nfn sample(state: &Shared<u64>) -> result: u64 reads(state) waits {\n  atomic current = &state^ {\n    return forward(value: current);\n  }\n}\n";
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else { panic!("the observation must check: {outcome:?}"); };
        let forward = checked.data.functions.iter().find(|function| function.name == "forward").expect("forward");
        assert_eq!(forward.held_parameters, [0]);
        assert_eq!(forward.held_calls.len(), 1);
        assert!(forward.held_calls[0].atomic.is_none());
        let sample = checked.data.functions.iter().find(|function| function.name == "sample").expect("sample");
        assert!(sample.held_parameters.is_empty());
        assert_eq!(sample.held_calls.len(), 1);
        assert!(sample.held_calls[0].atomic.is_some());
        assert_eq!(sample.held_calls[0].protected.len(), 1);
    });
}

#[test]
fn a_frozen_content_reference_is_outside_the_held_handle_region() {
    assert_rule_kind(b"fn observe(value: &u64) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return value^;\n}\n\nfn forward(value: &Frozen<u64>) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return observe(value: &value^.inner);\n}\n", SemanticRule::Hold1, |_| true);
}

#[test]
fn a_range_parameter_preserves_its_symbolic_held_cover() {
    assert_accepts(b"fn length(value: &[u64]) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return value^.len;\n}\n\nfn forward(value: &[u64]) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return length(value: value);\n}\n");
}

#[test]
fn a_protected_slot_admits_other_proved_disjoint_slot_writes() {
    assert_accepts(b"fn observe(value: &u64) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return value^;\n}\n\nfn sample(state: &Shared<Array<u64, 2>>) -> result: unit writes(state) waits {\n  atomic current = &state^ {\n    set current^[1_u64] = 3_u64;\n    let seen = observe(value: &current^[0_u64]);\n    set current^[1_u64] = seen;\n  }\n  return unit;\n}\n");
}

#[test]
fn a_protected_range_admits_a_write_at_its_excluded_end() {
    assert_accepts(b"fn length(value: &[u64]) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return value^.len;\n}\n\nfn sample(state: &Shared<Array<u64, 2>>) -> result: unit writes(state) waits {\n  atomic current = &state^ {\n    let count = length(value: &current^[0_u64..1_u64]);\n    set current^[1_u64] = count;\n  }\n  return unit;\n}\n");
}

#[test]
fn a_write_inside_the_protected_range_fails_at_the_observation_call() {
    super::assert_rule_at(b"fn length(value: &[u64]) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return value^.len;\n}\n\nfn sample(state: &Shared<Array<u64, 2>>) -> result: unit writes(state) waits {\n  atomic current = &state^ {\n    let count = length(value: &current^[0_u64..2_u64]);\n    set current^[1_u64] = count;\n  }\n  return unit;\n}\n", SemanticRule::Hold1, "length(value: &current^[0_u64..2_u64])");
}

#[test]
fn ordinary_index_disequality_facts_discharge_held_protection() {
    assert_accepts(b"fn observe(value: &u64) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return value^;\n}\n\nfn sample(state: &Shared<Array<u64, 2>>, first: u64, second: u64) -> result: unit writes(state) waits contract {\n  requires first < 2_u64;\n  requires second < 2_u64;\n  requires first != second;\n} {\n  atomic current = &state^ {\n    let seen = observe(value: &current^[first]);\n    set current^[second] = seen;\n  }\n  return unit;\n}\n");
}

#[test]
fn a_write_through_an_alias_to_the_protected_slot_is_rejected() {
    super::assert_rule_at(b"fn observe(value: &u64) -> result: u64 reads(value) contract {\n  requires held(value);\n} {\n  return value^;\n}\n\nfn sample(state: &Shared<Array<u64, 2>>) -> result: unit writes(state) waits {\n  atomic current = &state^ {\n    let alias = &current^[0_u64];\n    let seen = observe(value: alias);\n    set alias^ = seen;\n  }\n  return unit;\n}\n", SemanticRule::Hold1, "observe(value: alias)");
}
