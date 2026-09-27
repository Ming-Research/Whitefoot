use crate::{
    LexicalUseRole, ResolutionIssueKind, ResolutionOutcome, ResolutionRule, SemanticIssueKind,
    SemanticLocation, SemanticOutcome, SemanticRule,
};

use super::{
    assert_rule, assert_rule_at, assert_rule_kind, with_resolution, with_semantics,
    with_semantics_dark,
};
use crate::semantic::entailment::{
    CallGoalDisposition, DerivationNode, FlowEventKind, FunctionPostconditionProof,
    PostconditionDisposition, TermKind,
};
use crate::semantic::model::{CheckedBodyDisposition, CheckedExpression, CheckedStatement};
use crate::semantic::places::PlaceStep;

fn assert_complete(source: &[u8]) {
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

fn assert_fn9_unproved(source: &[u8]) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("undischarged postcondition must be an FN-9 source issue: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn9);
        let SemanticIssueKind::UndischargedPostcondition(detail) = issue.kind() else {
            panic!("FN-9 issue must carry its proof disposition: {issue:?}");
        };
        assert_eq!(
            detail.disposition,
            crate::PostconditionProofDisposition::Unproved
        );
    });
}

fn assert_fn9_refuted(source: &[u8]) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("refuted postcondition must be an FN-9 source issue: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn9);
        let SemanticIssueKind::UndischargedPostcondition(detail) = issue.kind() else {
            panic!("FN-9 issue must carry its proof disposition: {issue:?}");
        };
        assert_eq!(
            detail.disposition,
            crate::PostconditionProofDisposition::Refuted
        );
    });
}

#[test]
fn direct_range_measure_returns_prove_only_the_matching_postcondition() {
    assert_complete(
        br#"fn count(part: &[u8]) -> result: u64 reads(part) contract {
  ensures result == deref(part).len;
} {
  return deref(part).len;
}
"#,
    );

    assert_fn9_refuted(
        br#"fn count(part: &[u8]) -> result: u64 reads(part) contract {
  requires deref(part).len <= 18446744073709551614_u64;
  ensures result == deref(part).len + 1_u64;
} {
  return deref(part).len;
}
"#,
    );

    assert_rule(
        br#"fn first(part: &[u8]) -> result: u64 reads(part) contract {
  requires deref(part).len > 0_u64;
  ensures result == deref(part).len;
} {
  let byte = deref(part)[0_u64];
  return cvt::<u8, u64>(byte);
}
"#,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionReturn,
    );
}

/// [FN-9, ENT-2(b), TYPE-9] a direct scalar return may be the measure of the
/// complete place below its owner. Box content and ordinary fields remain
/// distinct projections of that place; introducing a local binding must not
/// change which relation the selected return can prove.
#[test]
fn direct_box_content_measure_returns_preserve_the_complete_place() {
    assert_complete(
        br#"struct Holder {
  value: u64;
}

struct Wrapped {
  cells: Box<Slots<u8>>;
}

fn slots(owner: Box<Slots<u8>>) -> result: u64 pure contract {
  ensures result == owner.inner.len;
} {
  return owner.inner.len;
}

fn array(owner: Box<Array<u8>>) -> result: u64 pure contract {
  ensures result == owner.inner.len;
} {
  return owner.inner.len;
}

fn nested(owner: Box<Box<Slots<u8>>>) -> result: u64 pure contract {
  ensures result == owner.inner.inner.len;
} {
  return owner.inner.inner.len;
}

fn referenced(owner: &Box<Slots<u8>>) -> result: u64 reads(owner.inner) contract {
  ensures result == deref(owner).inner.len;
} {
  return deref(owner).inner.len;
}

fn boxed_indexed(owner: Box<Array<Slots<u8, 2>, 2>>) -> result: u64 pure contract {
  ensures result == owner.inner[0_u64].len;
} {
  return owner.inner[0_u64].len;
}

fn wrapped(owner: Wrapped) -> result: u64 pure contract {
  ensures result == owner.cells.inner.len;
} {
  return owner.cells.inner.len;
}

fn through_local(owner: Box<Slots<u8>>) -> result: u64 pure contract {
  ensures result == owner.inner.len;
} {
  let length = owner.inner.len;
  return length;
}

fn inline(owner: Slots<u8, 4>) -> result: u64 pure contract {
  ensures result == owner.len;
} {
  return owner.len;
}

fn indexed(owner: Array<Slots<u8, 2>, 2>) -> result: u64 pure contract {
  ensures result == owner[0_u64].len;
} {
  return owner[0_u64].len;
}

fn field(owner: Holder) -> result: u64 pure contract {
  ensures result == owner.value;
} {
  return owner.value;
}
"#,
    );

    assert_fn9_refuted(
        br#"fn wrong(owner: Box<Slots<u8>>) -> result: u64 pure contract {
  requires owner.inner.len <= 18446744073709551614_u64;
  ensures result == owner.inner.len + 1_u64;
} {
  return owner.inner.len;
}
"#,
    );

    assert_fn9_refuted(
        br#"fn distinct(left: Box<Slots<u8>>, right: Box<Slots<u8>>) -> result: u64 pure contract {
  requires left.inner.len == 0_u64;
  requires right.inner.len == 1_u64;
  ensures result == right.inner.len;
} {
  return left.inner.len;
}
"#,
    );

    assert_fn9_refuted(
        br#"fn distinct_references(left: &Box<Slots<u8>>, right: &Box<Slots<u8>>) -> result: u64 reads(left.inner) contract {
  requires deref(left).inner.len == 0_u64;
  requires deref(right).inner.len == 1_u64;
  ensures result == deref(right).inner.len;
} {
  return deref(left).inner.len;
}
"#,
    );

    assert_fn9_refuted(
        br#"fn distinct_indices(owner: Array<Slots<u8, 2>, 2>) -> result: u64 pure contract {
  requires owner[0_u64].len == 0_u64;
  requires owner[1_u64].len == 1_u64;
  ensures result == owner[1_u64].len;
} {
  return owner[0_u64].len;
}
"#,
    );

    assert_fn9_refuted(
        br#"struct Pair {
  left: Box<Slots<u8>>;
  right: Box<Slots<u8>>;
}

fn distinct_fields(owner: Pair) -> result: u64 pure contract {
  requires owner.left.inner.len == 0_u64;
  requires owner.right.inner.len == 1_u64;
  ensures result == owner.right.inner.len;
} {
  return owner.left.inner.len;
}
"#,
    );
}

/// A declared postcondition over a reference parameter substitutes through a
/// Box content projection written as `&deref(boxed).inner` at the call. The
/// wrapper publishes the resulting concrete `.inner.len` relation.
#[test]
fn a_box_content_actual_preserves_an_imported_length_postcondition() {
    let source = br#"fn append_one(values: &Slots<u8, 4>) -> result: unit writes(values.next), writes(values.len) contract {
  requires deref(values).len == 0_u64;
  requires 1_u64 <= deref(values).cap;
  ensures deref(values).len == 1_u64;
} {
  place_back(window: values, value: 7_u8);
  return unit;
}

fn through_box(slots: &Box<Slots<u8, 4>>) -> result: unit writes(slots.inner.next), writes(slots.inner.len) contract {
  requires deref(slots).inner.len == 0_u64;
  requires 1_u64 <= deref(slots).inner.cap;
  ensures deref(slots).inner.len == 1_u64;
} {
  let appended = append_one(values: &deref(slots).inner);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

/// A generic wrapper around `grow` publishes its ordinary integer result
/// relation together with the two measured relations it derives from the
/// supplied row. A second generic caller retains that relation after replacing
/// both the stored type and const ceiling; the measured relations cannot hide
/// the ordinary result relation from [S12]'s concrete summary.
#[test]
fn a_generic_grow_wrapper_publishes_its_integer_result_relation() {
    let source = br#"struct Holder<T, const ceiling: u64> {
  storage: Box<Slots<T>>;
}

fn reserve<T, const ceiling: u64>(values: &Holder<T, ceiling>, total: u64) -> capacity: u64 writes(values.storage) contract {
  requires total <= ceiling;
  ensures capacity == deref(values).storage.inner.cap;
  ensures capacity >= total;
  ensures deref(values).storage.inner.len == deref(entry(values)).storage.inner.len;
} {
  let current = deref(values).storage.inner.cap;
  if current >= total {
    return current;
  }
  grow(cell: &deref(values).storage, capacity: total);
  return total;
}

fn forward<T, const ceiling: u64>(values: &Holder<T, ceiling>) -> capacity: u64 writes(values.storage) contract {
  requires 1_u64 <= ceiling;
  ensures capacity >= 1_u64;
} {
  let capacity = reserve::<T, ceiling>(values: values, total: 1_u64);
  return capacity;
}

fn needs_one(value: u64) -> result: unit pure contract {
  requires value >= 1_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let storage = box_slots_new::<u64>(capacity: 0_u64);
  let holder = Holder<u64, 3>(storage: move storage);
  let opened = forward::<u64, 3>(values: &holder);
  needs_one(value: opened);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

/// A fresh runtime-capacity Slots value has length zero. FN-9 must retain the
/// `.inner` projection and selected Box result type when checking that fact.
#[test]
fn a_fresh_boxed_slots_result_proves_only_its_zero_length() {
    let source = |length: &str| {
        format!(
            "fn make() -> made: Box<Slots<u8>> pure contract {{
  ensures made.inner.len == {length};
}} {{
  let local = box_slots_new::<u8>(capacity: 4_u64);
  return move local;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"
        )
    };
    assert_complete(source("0_u64").as_bytes());
    assert_fn9_refuted(source("1_u64").as_bytes());
}

fn postcondition_proof(source: &[u8], function: &str) -> FunctionPostconditionProof {
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("postcondition proof fixture must check completely: {outcome:?}");
        };
        checked
            .data
            .functions
            .iter()
            .find(|candidate| candidate.name == function)
            .unwrap_or_else(|| panic!("function {function} must exist"))
            .entailment
            .postconditions
            .first()
            .cloned()
            .unwrap_or_else(|| panic!("function {function} must retain a postcondition proof"))
    })
}

fn dispositions(proof: &FunctionPostconditionProof) -> Vec<PostconditionDisposition> {
    proof.exits.iter().map(|exit| exit.disposition).collect()
}

const ORDINARY_MAIN: &str = "fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";

#[test]
fn ordinary_main_smoke() {
    assert_complete(ORDINARY_MAIN.as_bytes());
}

#[test]
fn requires_smoke() {
    let source = format!(
        "fn identity(value: i32) -> out: i32 pure contract {{\n  requires value == value;\n}} {{\n  return value;\n}}\n\n{ORDINARY_MAIN}"
    );
    assert_complete(source.as_bytes());
}

#[test]
fn ensures_smoke() {
    let source = format!(
        "fn identity(value: i32) -> out: i32 pure contract {{\n  ensures out == value;\n}} {{\n  return value;\n}}\n\n{ORDINARY_MAIN}"
    );
    assert_complete(source.as_bytes());
}

#[test]
fn published_relations_keep_distinct_scalar_result_ordinals_separate() {
    let source = format!(
        "fn pair() -> (zero: i32, one: i32) pure contract {{\n  ensures zero == 0_i32;\n  ensures one == 1_i32;\n}} {{\n  return 0_i32, 1_i32;\n}}\n\n{ORDINARY_MAIN}"
    );
    assert_complete(source.as_bytes());
}

#[test]
fn contradictory_relations_on_one_scalar_result_ordinal_still_reject() {
    let source = format!(
        "fn contradictory() -> (zero: i32, spare: i32) pure contract {{\n  ensures zero == 0_i32;\n  ensures spare == 1_i32;\n  ensures zero == 1_i32;\n}} {{\n  return 0_i32, 1_i32;\n}}\n\n{ORDINARY_MAIN}"
    );
    assert_rule(
        source.as_bytes(),
        SemanticRule::Call6,
        SemanticIssueKind::ContradictoryPublishedRelations {
            relations: vec![
                "ensures zero == 0_i32;".to_owned(),
                "ensures spare == 1_i32;".to_owned(),
                "ensures zero == 1_i32;".to_owned(),
            ],
            mechanical_fix: "state one consistent relation set: a contract whose clauses cannot hold together publishes every fact at every caller",
        },
    );
}

#[test]
fn a_computed_constant_offset_is_not_an_fn9_relation_operand() {
    let source = br#"fn shifted(value: u8) -> result: u8 pure contract {
  define next = value +wrap 1_u8;
  ensures result == next;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionRelation,
    );
}

#[test]
fn a_true_computed_constant_offset_is_still_outside_the_fn9_relation_form() {
    let source = br#"fn shifted(value: u8) -> result: u8 pure contract {
  define next = value -wrap 1_u8;
  ensures result == next;
} {
  let next = value -wrap 1_u8;
  return next;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionRelation,
    );
}

#[test]
fn an_uncomputed_fn9_relation_still_publishes_to_its_caller() {
    let source = br#"const values: Array<u8, 8> =[0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8];

fn identity(value: u64) -> result: u64 pure contract {
  ensures result == value;
} {
  return value;
}

fn select(index: u64) -> result: u8 pure contract {
  requires index < 8_u64;
} {
  let selected = identity(value: index);
  return values[selected];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

/// Contract clauses remain static proof input: `requires` admits the first
/// protected read, and the proved `ensures` relation admits the caller's
/// second protected read.
#[test]
fn contract_clauses_remain_available_to_the_originating_proof_context() {
    let source = br#"const lookup: Array<u8, 8> =[0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8];

fn pick(table: Array<u8, 8>, index: u64) -> value: u64 pure contract {
  requires index < table.len;
  ensures value <= 7_u64;
} {
  let selected = table[index];
  let widened = cvt::<u8, u64>(selected);
  if widened <= 7_u64 {
    return widened;
  } else {
    return 7_u64;
  }
}

fn caller(table: Array<u8, 8>) -> result: u8 pure contract {
  requires table.len >= 1_u64;
} {
  let value = pick(table: table, index: 0_u64);
  return lookup[value];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

/// FN-9 publishes exactly the relations a callee proves.  The caller can use
/// both equality and a weaker ordering relation without restating either as a
/// body assertion.
#[test]
fn verified_exact_and_weak_ensures_are_consumed_by_the_caller() {
    let source = br#"fn exact(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn weak(value: i32) -> result: i32 pure contract {
  ensures result <= value;
} {
  return value;
}

fn need_same(left: i32, right: i32) -> result: unit pure contract {
  requires left == right;
} {
  return unit;
}

fn need_ordered(left: i32, right: i32) -> result: unit pure contract {
  requires left <= right;
} {
  return unit;
}

fn caller(value: i32) -> result: unit pure {
  let exact_result = exact(value: value);
  need_same(left: exact_result, right: value);
  let weak_result = weak(value: value);
  need_ordered(left: weak_result, right: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn a_non_bool_ensures_predicate_cites_op5() {
    let source = format!(
        "fn invalid(value: i32) -> out: i32 pure contract {{\n  ensures value;\n}} {{\n  return value;\n}}\n\n{ORDINARY_MAIN}"
    );
    assert_rule(
        source.as_bytes(),
        SemanticRule::Op5,
        SemanticIssueKind::InvalidPredicateCondition,
    );
}

#[test]
fn plural_ensures_are_proved_and_published_as_independent_relations() {
    let source = format!(
        "fn identity(value: i32) -> out: i32 pure contract {{\n  ensures out == value;\n  ensures out >= value;\n}} {{\n  return value;\n}}\n\n{ORDINARY_MAIN}"
    );
    with_semantics_dark(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("plural ensures fixture must check: {outcome:?}");
        };
        let identity = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "identity")
            .expect("identity function");
        assert_eq!(identity.postconditions.len(), 2);
        assert_eq!(identity.entailment.postconditions.len(), 2);
        for (ordinal, proof) in identity.entailment.postconditions.iter().enumerate() {
            assert_eq!(proof.relation_ordinal as usize, ordinal);
            assert!(proof.aggregate.discharged);
            assert!(proof.summary.is_some());
        }
        let component = checked
            .data
            .postcondition_schedule
            .components
            .iter()
            .find(|component| component.functions.contains(&identity.id))
            .expect("identity component");
        assert_eq!(component.summaries.len(), 2);
        assert_eq!(component.summaries[0].relation_ordinal, 0);
        assert_eq!(component.summaries[1].relation_ordinal, 1);
    });
}

#[test]
fn one_failed_ensure_withholds_every_summary_in_its_component() {
    let source = format!(
        "fn identity(value: i32) -> out: i32 pure contract {{\n  ensures out == value;\n  ensures out >= 0_i32;\n}} {{\n  return value;\n}}\n\n{ORDINARY_MAIN}"
    );
    with_semantics_dark(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("dark plural failure fixture must remain inspectable: {outcome:?}");
        };
        let identity = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "identity")
            .expect("identity function");
        let [first, second] = identity.entailment.postconditions.as_slice() else {
            panic!("both relation proofs must be retained");
        };
        assert!(first.aggregate.discharged);
        assert!(!second.aggregate.discharged);
        assert!(first.summary.is_none());
        assert!(second.summary.is_none());
        let component = checked
            .data
            .postcondition_schedule
            .components
            .iter()
            .find(|component| component.functions.contains(&identity.id))
            .expect("identity component");
        assert!(component.summaries.is_empty());
    });
    assert_fn9_unproved(source.as_bytes());
}

#[test]
fn one_failed_relation_withholds_every_summary_in_a_mutual_scc() {
    let source = format!(
        "fn left(value: i32) -> out: i32 pure contract {{\n  ensures out == value;\n}} {{\n  let ignored = right(value: value);\n  return value;\n}}\n\nfn right(value: i32) -> out: i32 pure contract {{\n  ensures out == value;\n  ensures out >= 0_i32;\n}} {{\n  let ignored = left(value: value);\n  return value;\n}}\n\n{ORDINARY_MAIN}"
    );
    with_semantics_dark(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("dark mutual failure fixture must remain inspectable: {outcome:?}");
        };
        let members = checked
            .data
            .functions
            .iter()
            .filter(|function| matches!(function.name.as_str(), "left" | "right"))
            .collect::<Vec<_>>();
        assert_eq!(members.len(), 2);
        let component = checked
            .data
            .postcondition_schedule
            .components
            .iter()
            .find(|component| component.functions.contains(&members[0].id))
            .expect("mutual component");
        assert_eq!(component.functions.len(), 2);
        assert!(component.summaries.is_empty());
        assert!(members.iter().all(|function| {
            function
                .entailment
                .postconditions
                .iter()
                .all(|proof| proof.summary.is_none())
        }));
        let left = members
            .iter()
            .find(|function| function.name == "left")
            .expect("left function");
        assert!(left.entailment.postconditions[0].aggregate.discharged);
        let right = members
            .iter()
            .find(|function| function.name == "right")
            .expect("right function");
        assert!(right.entailment.postconditions[0].aggregate.discharged);
        assert!(!right.entailment.postconditions[1].aggregate.discharged);
    });
    assert_fn9_unproved(source.as_bytes());
}

#[test]
fn an_inhabited_routed_ensure_without_a_selected_exit_is_rejected() {
    let source = format!(
        "fn only_error(value: i32) -> out: Result<i32, i32> pure contract {{\n  ensures when Ok(value: payload): payload == value;\n}} {{\n  return Err<i32, i32>(error: value);\n}}\n\n{ORDINARY_MAIN}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("inhabited empty route must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn9);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::NoSelectedNormalExit { .. }
        ));
    });
}

#[test]
fn an_uninhabited_routed_ensure_needs_no_exit_and_publishes_no_summary() {
    let source = format!(
        "fn impossible(value: i32) -> out: Result<i32, i32> pure contract {{\n  requires value == 0_i32;\n  requires value != 0_i32;\n  ensures when Ok(value: payload): payload == value;\n}} {{\n  return Err<i32, i32>(error: value);\n}}\n\n{ORDINARY_MAIN}"
    );
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("uninhabited empty route must be accepted: {outcome:?}");
        };
        let impossible = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "impossible")
            .expect("impossible function");
        assert!(matches!(
            impossible.body_disposition,
            CheckedBodyDisposition::Uninhabited { .. }
        ));
        assert!(impossible.entailment.postconditions.is_empty());
        assert!(
            checked
                .data
                .postcondition_schedule
                .components
                .iter()
                .all(|component| component
                    .summaries
                    .iter()
                    .all(|summary| summary.function != impossible.id))
        );
    });
}

#[test]
fn a_checked_plain_postcondition_is_proved_at_its_selected_exit() {
    let source = br#"fn identity(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    let proof = postcondition_proof(source, "identity");
    assert_eq!(
        dispositions(&proof),
        vec![PostconditionDisposition::Discharged]
    );
    assert!(proof.aggregate.discharged);
}
#[test]
fn entry_requirements_prove_postconditions_in_the_originating_context() {
    let source = br#"fn constrained(value: i32) -> result: i32 pure contract {
  requires value == 1_i32;
  ensures result == 1_i32;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    let proof = postcondition_proof(source, "constrained");
    assert_eq!(
        dispositions(&proof),
        vec![PostconditionDisposition::Discharged]
    );
    assert!(proof.aggregate.discharged);
}

#[test]
fn entry_image_writes_are_retained_and_prevent_false_discharge() {
    let source = br#"fn changed(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  set value = 1_i32;
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let proof = postcondition_proof(source, "changed");
    assert_eq!(
        dispositions(&proof),
        vec![PostconditionDisposition::Unproved]
    );
    let image = &proof.exits[0].entry_images[0];
    assert!(image.invalidation.is_some());
    assert!(!proof.aggregate.discharged);
    assert_rule_at(source, SemanticRule::Fn9, "return value;");
}

/// [FN-9] an entry image becomes permanently unavailable on the first
/// structural edge whose [ENT-5] kill overlaps it, and a call whose declared
/// row writes the referent is such an edge [EFF-5]. The contract-free `plain`
/// exhibits the same write and mints no entry image at all.
///
/// v0.59 spelled the invalidating event as a *consume of the holder*,
/// `overwrite(out: move out)` over a `&uniq i32`, and located the event at
/// that consume's carrier. A reference is a local name for a path in v0.60
/// [REF-1], it is not storage that a `move` can consume, and write permission
/// comes from the callee's row rather than from a permission marker, so the
/// invalidating edge is the ordinary projected call write.
#[test]
fn a_projected_call_write_invalidates_its_postcondition_entry_image() {
    let source = br#"fn overwrite(out: &i32) -> result: unit writes(out) {
  set deref(out) = 1_i32;
  return unit;
}

fn transfer(out: &i32) -> result: i32 writes(out) contract {
  ensures result == deref(out);
} {
  let before = deref(out);
  overwrite(out: out);
  return before;
}

fn plain(out: &i32) -> result: i32 writes(out) {
  let before = deref(out);
  overwrite(out: out);
  return before;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("projected-write fixture must check completely: {outcome:?}");
        };
        let transfer = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "transfer")
            .expect("transfer function");
        let proof = transfer
            .entailment
            .postconditions
            .first()
            .expect("transfer postcondition proof");
        let invalidation = proof.exits[0].entry_images[0]
            .invalidation
            .expect("the projected call write invalidates its entry image");
        let event = &transfer.entailment.derivations.events[invalidation.0 as usize];
        assert_eq!(
            event.kind,
            FlowEventKind::PostconditionEntryImageInvalidation
        );
        // The retired half of this case located the event at the *carrier of
        // the holder consume* and asserted it was not the call node. That
        // consume was v0.59's `move` of a `&uniq` holder, whose rules (OWN-2,
        // OWN-5, OWN-6) are succeeded by [REF-1] and [REF-2] with [EFF-5]: a
        // reference is not storage, so there is no consume carrier to locate,
        // and the invalidating edge is the call itself. What survives is that
        // the invalidating statement is the one direct call to `overwrite`.
        let call = transfer
            .body
            .as_deref()
            .expect("WF body")
            .iter()
            .find_map(|statement| {
                let call = match statement {
                    CheckedStatement::Evaluate { value: call, .. }
                    | CheckedStatement::DropExpression { value: call, .. } => call,
                    _ => return None,
                };
                let CheckedExpression::UserCall {
                    call, arguments, ..
                } = call
                else {
                    return None;
                };
                let [CheckedExpression::Binding { .. }] = arguments.as_slice() else {
                    return None;
                };
                Some(call)
            })
            .expect("transfer has one direct reference-argument call");
        assert_eq!(event.node_path.as_ref(), Some(call));

        let plain = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "plain")
            .expect("plain function");
        assert!(plain.entailment.postconditions.is_empty());
        assert!(
            plain
                .entailment
                .derivations
                .events
                .iter()
                .all(|event| { event.kind != FlowEventKind::PostconditionEntryImageInvalidation })
        );
    });
}

#[test]
fn an_ordinary_loop_uses_the_exact_first_invalidation_event_without_a_snapshot() {
    let source = br#"fn looped(value: i32, stop: Bool) -> result: i32 pure contract {
  ensures result == value;
} {
  loop @again {
    set value = 1_i32;
    set value = 2_i32;
    if stop {
      break @again;
    }
  }
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("ordinary-loop fixture must check completely: {outcome:?}");
        };
        let function = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "looped")
            .expect("looped function");
        let proof = function
            .entailment
            .postconditions
            .first()
            .expect("looped postcondition proof");
        let invalidation = proof.exits[0].entry_images[0]
            .invalidation
            .expect("the continuing write invalidates the entry image");
        let event = &function.entailment.derivations.events[invalidation.0 as usize];
        assert_eq!(
            event.kind,
            FlowEventKind::PostconditionEntryImageInvalidation
        );
        let set = function
            .body
            .as_deref()
            .expect("WF body")
            .iter()
            .find_map(|statement| match statement {
                CheckedStatement::Loop { body, .. } => body.iter().find_map(|statement| {
                    let CheckedStatement::Set { node_path, .. } = statement else {
                        return None;
                    };
                    Some(node_path)
                }),
                _ => None,
            })
            .expect("loop body set");
        assert_eq!(event.node_path.as_ref(), Some(set));
        assert!(
            function
                .entailment
                .derivations
                .events
                .iter()
                .all(|event| event.kind != FlowEventKind::Snapshot)
        );
    });
}

#[test]
fn counted_append_proves_the_admitted_result_and_refutes_only_the_blinded_invalid_exit() {
    // Both runs reach the function as [REF-4] range references, whose one
    // measure `len` is the [OP-15] member of the referent; the callee's row
    // carries the write permission the retiring `&uniq MutSlice<u8>` marker
    // used to carry [REF-1, EFF-1].
    let source = br#"fn append_bytes(destination: &[u8], capacity: u64, filled: u64, text: &[u8]) -> result: u64 reads(text), writes(destination) contract {
  requires capacity == deref(destination).len;
  requires filled <= capacity;
  ensures result <= capacity;
} {
  let spare = deref(destination).len;
  let admitted = filled <= spare;
  let length = deref(text).len;
  if admitted {
    for @append (at in filled..spare) {
      let taken = at -wrap filled;
      let done = taken >= length;
      if done {
        return at;
      }
      let byte = deref(text)[taken];
      set deref(destination)[at] = byte;
    }
    return spare;
  } else {
    return filled;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    let proof = postcondition_proof(source, "append_bytes");
    assert_eq!(
        dispositions(&proof),
        vec![
            PostconditionDisposition::Discharged,
            PostconditionDisposition::Discharged,
            PostconditionDisposition::Discharged,
        ]
    );
}

/// [MSR-3] a measure of a parameter is that parameter's entry datum, which
/// contains no place and which no [ENT-5] event kills, so neither an element
/// write nor the consume of the parameter's own root takes it away. What the
/// second half of this case pinned before the entry placement landed — an
/// unproved relation after the root was consumed — was the cost of reading
/// the live term where the rule says the entry value; the relation is now
/// discharged, and it is the same relation the writer stated.
///
/// A parameter of fragment type is not a measured value and has no measure
/// datum, so its entry image still invalidates; the third half is that
/// boundary, and the conformance case
/// `msr3-neg-a-parameter-value-written-back-loses-its-entry-image` carries
/// the same judgment as a source verdict.
#[test]
fn measure_entry_datums_survive_element_writes_and_root_replacement() {
    let element = br#"fn kept(values: Slots<u8, 2>) -> result: u64 pure contract {
  define size = values.len;
  requires size >= 1_u64;
  ensures result == size;
} {
  set values[0_u64] = 1_u8;
  return values.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(element);
    let proof = postcondition_proof(element, "kept");
    assert_eq!(
        dispositions(&proof),
        vec![PostconditionDisposition::Discharged]
    );
    assert!(
        proof.exits[0]
            .entry_images
            .iter()
            .all(|image| image.invalidation.is_none())
    );

    let replacement = br#"fn consume(values: Slots<u8, 2>) -> result: unit pure {
  return unit;
}

fn replaced(values: Slots<u8, 2>) -> result: u64 pure contract {
  define size = values.len;
  ensures result == size;
} {
  let size = values.len;
  let ignored = consume(values: move values);
  return size;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(replacement);
    let proof = postcondition_proof(replacement, "replaced");
    assert_eq!(
        dispositions(&proof),
        vec![PostconditionDisposition::Discharged]
    );
    assert!(
        proof.exits[0]
            .entry_images
            .iter()
            .all(|image| image.invalidation.is_none())
    );

    // A fragment parameter has no measure and therefore no entry datum: its
    // entry image is the live place, and writing the parameter back takes it.
    let fragment = br#"fn shifted(count: u64) -> result: u64 pure contract {
  ensures result == count;
} {
  set count = 1_u64;
  return count;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let proof = postcondition_proof(fragment, "shifted");
    assert_eq!(
        dispositions(&proof),
        vec![PostconditionDisposition::Unproved]
    );
    assert!(
        proof.exits[0]
            .entry_images
            .iter()
            .any(|image| image.invalidation.is_some())
    );
}

#[test]
fn selected_exits_aggregate_only_when_every_exit_in_the_view_discharges() {
    let source = br#"fn branch(value: i32, choose: Bool) -> result: i32 pure contract {
  ensures result == value;
} {
  if choose {
    return value;
  } else {
    return value;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    let proof = postcondition_proof(source, "branch");
    assert_eq!(proof.exits.len(), 2);
    assert!(
        proof
            .exits
            .windows(2)
            .all(|pair| pair[0].statement.components() < pair[1].statement.components())
    );
    assert!(proof.aggregate.discharged);
}

#[test]
fn an_earlier_verified_postcondition_discharges_a_fresh_direct_result() {
    let independent = br#"fn callee(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn caller(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let ignored = callee(value: value);
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(independent);

    let dependent = br#"fn callee(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn caller(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let called = callee(value: value);
  return called;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(dependent);
}

#[test]
fn an_earlier_ok_summary_is_available_when_its_payload_is_selected() {
    let source = br#"fn callee(value: i32) -> result: Result<i32, Overflow> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  return Ok<i32, Overflow>(value: value);
}

fn direct(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  match callee(value: value) {
    Ok(value: payload) => {
      return payload;
    }
    Err(error: problem) => {
      return value;
    }
  }
}

fn delivered(value: i32) -> result: i32 pure {
  let selected = match callee(value: value) {
    Ok(value: payload) => {
      give payload;
    }
    Err(error: problem) => {
      give value;
    }
  }
  return selected;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn a_borrowed_formal_substitution_consumes_its_one_formal_deref() {
    let source = br#"struct Pair {
  value: i32;
}

fn observe(pair: &Pair) -> result: i32 reads(pair.value) contract {
  ensures result == deref(pair).value;
} {
  return deref(pair).value;
}

fn caller(pair: &Pair) -> result: i32 reads(pair.value) contract {
  ensures result == deref(pair).value;
} {
  let observed = observe(pair: pair);
  return observed;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

/// [MSR-3, CALL-2] consuming an own actual leaves its immutable pre-transfer
/// call datum alive, so the direct-result relation must publish. It does not
/// restore the caller's non-measure entry image: [FN-9] still requires that
/// place to remain stable. The former no-route assertion came from a moved
/// reference holder, which is not the own-argument transport tested here.
#[test]
fn a_consumed_actual_preserves_its_call_datum_but_not_its_entry_image() {
    let source = br#"nocopy struct Pair {
  kept: i32;
  changed: i32;
}

fn touch(pair: Pair) -> result: i32 pure contract {
  ensures result == pair.kept;
} {
  return pair.kept;
}

fn caller(pair: Pair) -> result: i32 pure contract {
  ensures result == pair.kept;
} {
  let observed = touch(pair: move pair);
  return observed;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_fn9_unproved(source);
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("consumed-actual fixture must remain inspectable: {outcome:?}");
        };
        let touch = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "touch")
            .expect("touch function");
        assert!(
            touch
                .entailment
                .postconditions
                .first()
                .is_some_and(|proof| proof.aggregate.discharged),
            "the callee summary premise is independently available"
        );
        let caller = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "caller")
            .expect("caller function");
        assert!(
            caller.entailment.derivations.nodes.iter().any(|node| {
                let DerivationNode::PostconditionDirectResult { relation, .. } = node else {
                    return false;
                };
                relation.terms().iter().any(|term| {
                    matches!(
                        &caller.entailment.inventory.terms[term.0 as usize],
                        TermKind::CallDatum {
                            formal: 0,
                            projections,
                            measure: None,
                            ..
                        } if projections.as_slice() == [PlaceStep::Field(0)]
                    )
                })
            }),
            "the direct result is related to pair.kept's immutable call datum"
        );
    });
}

/// [MSR-3] an own operand's call datum survives consumption of the object
/// from which it was copied. The copy precedes the call: [EFF-5] now rejects
/// a content read alongside a consume of its owner in the same call, as the
/// separate negative case below checks.
#[test]
fn a_copied_content_actual_survives_a_cross_formal_owner_move_as_a_call_datum() {
    // [STOR-8] allocation is total, so the cell arrives from `box_new` with no
    // failure arm, no store provider and no region; [TYPE-9] spells its
    // content as the field `inner`.
    let source = br#"fn observe(value: i32, owner: Box<i32>) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn caller() -> result: i32 pure contract {
  ensures result == 1_i32;
} {
  let owner = box_new::<i32>(value: 1_i32);
  let copied = owner.inner;
  let observed = observe(value: copied, owner: move owner);
  return observed;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    // `caller`'s own `ensures result == 1_i32` stays unproved for an
    // unrelated reason: `box_new` publishes no value equality on its content
    // [PRE-1], so the program is still rejected. What changed is the route
    // below: the callee's relation now reaches the caller at all.
    assert_fn9_unproved(source);
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("cell-deref M fixture must remain inspectable: {outcome:?}");
        };
        let caller = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "caller")
            .expect("caller function");
        assert!(
            caller
                .entailment
                .derivations
                .nodes
                .iter()
                .any(|node| { matches!(node, DerivationNode::PostconditionDirectResult { .. }) }),
            "the call datum publishes the relation the consumed holder used to delete"
        );
    });
}

#[test]
fn an_inline_content_read_conflicts_with_consuming_its_owner() {
    let source = br#"fn observe(value: i32, owner: Box<i32>) -> result: i32 pure {
  return value;
}

fn caller() -> result: i32 pure {
  let owner = box_new::<i32>(value: 1_i32);
  return observe(value: owner.inner, owner: move owner);
}
"#;
    super::assert_rule_kind(source, SemanticRule::Eff5, |_| true);
}

/// P0 closes the two pre-move equalities while their common box referent is
/// still live. The resulting `observed == expected` theorem names only the two
/// copied scalar bindings, so consuming the original owner cannot invalidate
/// it.
#[test]
fn an_owner_move_preserves_a_materialized_holder_free_s12_consequence() {
    let source = br#"fn observe(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn sink(owner: Box<i32>) -> result: unit pure {
  return unit;
}

fn guard(left: i32, right: i32) -> result: unit pure contract {
  requires left == right;
} {
  return unit;
}

fn caller() -> result: unit pure {
  let owner = box_new::<i32>(value: 1_i32);
  let expected = owner.inner;
  let observed = observe(value: owner.inner);
  sink(owner: move owner);
  guard(left: observed, right: expected);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("a holder-free pre-move consequence must survive: {outcome:?}");
        };
        let caller = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "caller")
            .expect("caller function exists");
        super::entailment::validate_derivations(&caller.entailment);
        let [goal] = caller.entailment.call_goals.as_slice() else {
            panic!("caller retains exactly the guard requirement");
        };
        assert_eq!(goal.disposition, CallGoalDisposition::Discharged);
        let root = goal
            .derivation
            .expect("the discharged guard retains its derivation");
        let mut seen = vec![false; caller.entailment.derivations.nodes.len()];
        let mut stack = vec![root];
        let mut materialized = false;
        let mut postcondition = false;
        while let Some(node) = stack.pop() {
            let index = node.0 as usize;
            if seen[index] {
                continue;
            }
            seen[index] = true;
            let retained = &caller.entailment.derivations.nodes[index];
            materialized |= matches!(retained, DerivationNode::MaterializedBound { .. });
            postcondition |= matches!(retained, DerivationNode::PostconditionCall { .. });
            stack.extend(retained.parent_ids());
        }
        assert!(
            materialized,
            "the surviving scalar equality is pre-kill materialized"
        );
        assert!(
            postcondition,
            "the equality retains the verified callee result as a parent"
        );
    });
}

#[test]
fn a_referent_write_kills_an_s12_relation_that_still_reads_that_referent() {
    let source = br#"fn observe(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn guard(left: i32, right: i32) -> result: unit pure contract {
  requires left == right;
} {
  return unit;
}

fn caller() -> result: unit pure {
  let source = 1_i32;
  let observed = observe(value: source);
  let writer = &source;
  set deref(writer) = 2_i32;
  guard(left: observed, right: source);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_at(
        source,
        SemanticRule::Fn8,
        "guard(left: observed, right: source)",
    );
}

#[test]
fn an_ordinary_fallback_survives_when_a_neighboring_s12_candidate_dies() {
    let source = br#"fn observe(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn sink(owner: Box<i32>) -> result: unit pure {
  return unit;
}

fn guard(left: i32, right: i32) -> result: unit pure contract {
  requires left == right;
} {
  return unit;
}

fn caller() -> result: unit pure {
  let owner = box_new::<i32>(value: 1_i32);
  let expected = owner.inner;
  let observed = observe(value: owner.inner);
  if observed == expected {
    sink(owner: move owner);
    guard(left: observed, right: expected);
  } else {
    sink(owner: move owner);
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn a_joined_holder_free_consequence_survives_the_original_owner_move() {
    let source = br#"fn observe(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn sink(owner: Box<i32>) -> result: unit pure {
  return unit;
}

fn guard(left: i32, right: i32) -> result: unit pure contract {
  requires left == right;
} {
  return unit;
}

fn caller(choose: Bool) -> result: unit pure {
  let owner = box_new::<i32>(value: 1_i32);
  let expected = owner.inner;
  let observed = observe(value: owner.inner);
  if choose {
    let left_path = 0_u8;
  } else {
    let right_path = 0_u8;
  }
  sink(owner: move owner);
  guard(left: observed, right: expected);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn a_direct_same_binding_call_result_establishes_only_after_the_target_kill() {
    let source = br#"fn choose(ignored: i32, value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn guard(left: i32, right: i32) -> result: unit pure contract {
  requires left == right;
} {
  return unit;
}

fn caller(slot: i32, replacement: i32) -> result: unit pure {
  set slot = choose(ignored: slot, value: replacement);
  guard(left: slot, right: replacement);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("same-binding receiver fixture must remain inspectable: {outcome:?}");
        };
        let caller = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "caller")
            .expect("caller function");
        assert_eq!(
            caller
                .entailment
                .derivations
                .nodes
                .iter()
                .filter(|node| matches!(node, DerivationNode::PostconditionDirectReceiver { .. }))
                .count(),
            1,
            "the exact receiver route is retained once in the source context"
        );
    });
}

#[test]
fn direct_same_binding_near_misses_retain_no_receiver_root_or_special_event() {
    let source = br#"fn echo(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn choose(first: i32, second: i32, value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn mentions_receiver(slot: i32) -> result: i32 pure contract {
  ensures result == slot;
} {
  set slot = echo(value: slot);
  return slot;
}

fn repeated_receiver(slot: i32, replacement: i32) -> result: unit pure {
  set slot = choose(first: slot, second: slot, value: replacement);
  return unit;
}

fn distinct_receiver(slot: i32, other: i32, replacement: i32) -> result: unit pure {
  set slot = choose(first: other, second: other, value: replacement);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("same-binding near misses must remain inspectable: {outcome:?}");
        };
        for name in [
            "mentions_receiver",
            "repeated_receiver",
            "distinct_receiver",
        ] {
            let function = checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .unwrap_or_else(|| panic!("{name} function"));
            assert!(function.entailment.derivations.nodes.iter().all(|node| {
                !matches!(node, DerivationNode::PostconditionDirectReceiver { .. })
            }));
            assert!(
                function
                    .entailment
                    .derivations
                    .events
                    .iter()
                    .all(|event| { event.kind != FlowEventKind::PostconditionReceiverWrite })
            );
        }
    });
}

#[test]
fn a_selected_payload_assignment_proves_the_downstream_requirement() {
    let source = br#"fn selected(value: i32) -> result: Result<i32, Overflow> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  return Ok<i32, Overflow>(value: value);
}

fn guard(left: i32, right: i32) -> result: unit pure contract {
  requires left == right;
} {
  return unit;
}

fn caller(outer: i32, replacement: i32) -> result: unit pure {
  match selected(value: replacement) {
    Ok(value: payload) => {
      set outer = payload;
      guard(left: outer, right: replacement);
    }
    Err(error: problem) => {
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("selected receiver fixture must remain inspectable: {outcome:?}");
        };
        let caller = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "caller")
            .expect("caller function");
        super::entailment::validate_derivations(&caller.entailment);
        assert!(
            caller
                .entailment
                .derivations
                .nodes
                .iter()
                .any(|node| { matches!(node, DerivationNode::ResultTransport { .. }) }),
            "the downstream requirement retains its payload selection"
        );
    });
}

#[test]
fn ordinary_payload_assignments_need_no_special_receiver_event() {
    let source = br#"struct Cell {
  value: i32;
}

fn selected(value: i32) -> result: Result<i32, Overflow> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  return Ok<i32, Overflow>(value: value);
}

fn nonfirst(outer: i32, replacement: i32) -> result: unit pure {
  match selected(value: replacement) {
    Ok(value: payload) => {
      let intervening = 0_i32;
      set outer = payload;
    }
    Err(error: problem) => {
    }
  }
  return unit;
}

fn additional_write(outer: i32, replacement: i32) -> result: unit pure {
  match selected(value: replacement) {
    Ok(value: payload) => {
      set outer = payload;
      set outer = replacement;
    }
    Err(error: problem) => {
    }
  }
  return unit;
}

fn call_actual(outer: i32) -> result: unit pure {
  match selected(value: outer) {
    Ok(value: payload) => {
      set outer = payload;
    }
    Err(error: problem) => {
    }
  }
  return unit;
}

fn projected(cell: Cell, replacement: i32) -> result: unit pure {
  match selected(value: replacement) {
    Ok(value: payload) => {
      set cell.value = payload;
    }
    Err(error: problem) => {
    }
  }
  return unit;
}

fn computed(outer: i32, replacement: i32) -> result: unit pure {
  match selected(value: replacement) {
    Ok(value: payload) => {
      set outer = iand(payload, 0_i32);
    }
    Err(error: problem) => {
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("selected receiver near misses must remain inspectable: {outcome:?}");
        };
        for name in [
            "nonfirst",
            "additional_write",
            "call_actual",
            "projected",
            "computed",
        ] {
            let function = checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .unwrap_or_else(|| panic!("{name} function"));
            assert!(
                function
                    .entailment
                    .derivations
                    .events
                    .iter()
                    .all(|event| { event.kind != FlowEventKind::PostconditionReceiverWrite })
            );
        }
    });
}

#[test]
fn a_checked_ok_postcondition_selects_its_direct_payload() {
    let source = br#"fn selected(value: i32) -> result: Result<i32, Overflow> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  return Ok<i32, Overflow>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn an_ok_selector_rejects_an_empty_selected_exit_set() {
    let source = br#"fn unselected() -> result: Result<i32, Overflow> pure contract {
  ensures when Ok(value: payload): payload == 0_i32;
} {
  let error = Overflow();
  return Err<i32, Overflow>(error: error);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::NoSelectedNormalExit {
            residual: "no selected normal exit",
            mechanical_fix: "no `return` of this function delivers a value this clause's route selects: return such a value on some path, or delete the clause",
        },
    );
}

#[test]
fn an_ok_selector_verifies_a_moved_whole_result_return() {
    let source = br#"fn stored(value: i32) -> result: Result<i32, Box<u8>> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  let outcome = Ok<i32, Box<u8>>(value: value);
  return move outcome;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

fn checked_conversion_read_source(
    parameters: &str,
    requirements: &str,
    body: &str,
    size: usize,
) -> String {
    let contract = if requirements.is_empty() {
        String::new()
    } else {
        format!(" contract {{\n{requirements}\n}}")
    };
    format!(
        "const values: Array<u8, {size}> =[{}];\n\nfn read({parameters}) -> result: u8 pure{contract} {{\n{body}\n}}\n\n{ORDINARY_MAIN}",
        vec!["0_u8"; size].join(", ")
    )
}

const CHECKED_CONVERSION_READ: &str = "  match pending {\n    Ok(value: small) => {\n      let restored = cvt::<u8, u64>(small);\n      return values[restored];\n    }\n    Err(error: refused) => {\n      return 0_u8;\n    }\n  }";

#[test]
fn checked_integer_results_capture_before_input_mutation() {
    let source = checked_conversion_read_source(
        "index: u64",
        "  requires index < 4_u64;",
        &format!(
            "  let pending = cvt.checked::<u64, u8>(index);\n  set index = 1000_u64;\n{CHECKED_CONVERSION_READ}"
        ),
        4,
    );
    assert_complete(source.as_bytes());
    let stale = source.replace("return values[restored];", "return values[index];");
    assert_rule_kind(stale.as_bytes(), SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

#[test]
fn checked_integer_results_use_existing_delivery_routes() {
    // Both integer operands and Result<integer, NarrowError> are copy values;
    // explicit `move` is rejected by OWN-1 rather than being another route.
    for setup in [
        "  let pending = cvt.checked::<u64, u8>(index);",
        "  let original = cvt.checked::<u64, u8>(index);\n  let pending = original;\n  set original = cvt.checked::<u64, u8>(other);",
        "  let pending = cvt.checked::<u64, u8>(other);\n  set pending = cvt.checked::<u64, u8>(index);",
        "  let pending = if choose {\n    give cvt.checked::<u64, u8>(index);\n  } else {\n    give cvt.checked::<u64, u8>(index);\n  }",
    ] {
        let source = checked_conversion_read_source(
            "index: u64, other: u64, choose: Bool",
            "  requires index < 4_u64;",
            &format!("{setup}\n{CHECKED_CONVERSION_READ}"),
            4,
        );
        assert_complete(source.as_bytes());
    }
    let direct = checked_conversion_read_source(
        "index: u64",
        "  requires index < 4_u64;",
        &CHECKED_CONVERSION_READ.replace("match pending", "match cvt.checked::<u64, u8>(index)"),
        4,
    );
    assert_complete(direct.as_bytes());
}

#[test]
fn checked_integer_results_keep_only_the_replacement_context() {
    let source = checked_conversion_read_source(
        "index: u64, other: u64",
        "  requires index < 4_u64;",
        &format!(
            "  let pending = cvt.checked::<u64, u8>(index);\n  set pending = cvt.checked::<u64, u8>(other);\n{CHECKED_CONVERSION_READ}"
        ),
        4,
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

#[test]
fn checked_integer_result_joins_retain_the_common_weaker_bound() {
    let body = format!(
        "  let pending = cvt.checked::<u64, u8>(first);\n  if choose {{\n    set pending = cvt.checked::<u64, u8>(second);\n  }}\n{CHECKED_CONVERSION_READ}"
    );
    for size in [8, 4] {
        let source = checked_conversion_read_source(
            "first: u64, second: u64, choose: Bool",
            "  requires first < 4_u64;\n  requires second < 8_u64;",
            &body,
            size,
        );
        if size == 8 {
            assert_complete(source.as_bytes());
        } else {
            assert_rule_kind(source.as_bytes(), SemanticRule::Op4, |kind| {
                matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
            });
        }
    }
}

#[test]
fn checked_integer_results_do_not_combine_unselected_contexts() {
    let source = checked_conversion_read_source(
        "",
        "",
        &format!(
            "  let unrelated = cvt.checked::<u64, u8>(0_u64);\n  let pending = cvt.checked::<u64, u8>(4_u64);\n{CHECKED_CONVERSION_READ}"
        ),
        4,
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

#[test]
fn checked_integer_results_reuse_admitted_computed_value_images() {
    for (requirements, operand) in [
        ("", "iand(index, 3_u64)"),
        ("  requires index < 3_u64;", "index + 1_u64"),
    ] {
        let source = checked_conversion_read_source(
            "index: u64",
            requirements,
            &format!(
                "  let input = {operand};\n  let pending = cvt.checked::<u64, u8>(input);\n  set input = 1000_u64;\n  set index = 1000_u64;\n{CHECKED_CONVERSION_READ}"
            ),
            4,
        );
        assert_complete(source.as_bytes());
        with_semantics_dark(source.as_bytes(), |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("checked computed image must remain inspectable: {outcome:?}");
            };
            for function in &checked.data.functions {
                super::entailment::validate_derivations(&function.entailment);
            }
        });
    }
}

#[test]
fn checked_integer_results_do_not_add_conditional_affine_transport() {
    // Each operand's own bound leaves the sum's [ENT-3.S7] interval at
    // `index <= 6`; only the affine requirement bounds it below the four
    // slots, so the direct read needs the affine image the checked payload
    // does not carry.
    let parameters = "first: u64, second: u64";
    let requirements = "  requires first <= 3_u64;\n  requires second <= 3_u64;\n  requires first + second <= 3_u64;";
    let direct = checked_conversion_read_source(
        parameters,
        requirements,
        "  let index = first + second;\n  return values[index];",
        4,
    );
    assert_complete(direct.as_bytes());
    let captured = checked_conversion_read_source(
        parameters,
        requirements,
        &format!(
            "  let index = first + second;\n  let pending = cvt.checked::<u64, u8>(index);\n  set index = 1000_u64;\n{CHECKED_CONVERSION_READ}"
        ),
        4,
    );
    assert_rule_kind(captured.as_bytes(), SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

#[test]
fn checked_integer_results_capture_measure_and_constant_element_images() {
    let source = br#"const choices: Array<u64, 2> =[1_u64, 3_u64];

fn measured(values: Slots<u8, 4>) -> result: Result<u8, NarrowError> pure contract {
  requires values.len < 4_u64;
  ensures when Ok(value: small): small < 4_u8;
} {
  return cvt.checked::<u64, u8>(values.len);
}

fn indexed(offset: u64) -> result: Result<u8, NarrowError> pure contract {
  requires offset < 2_u64;
  ensures when Ok(value: small): small < 4_u8;
} {
  return cvt.checked::<u64, u8>(choices[offset]);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn checked_integer_results_survive_propagation_and_forwarded_return() {
    let source = br#"const values: Array<u8, 4> =[0_u8, 0_u8, 0_u8, 0_u8];

fn narrow(index: u64) -> result: Result<u8, NarrowError> pure contract {
  requires index < 4_u64;
  ensures when Ok(value: small): small < 4_u8;
} {
  return cvt.checked::<u64, u8>(index);
}

fn forward(index: u64) -> result: Result<u8, NarrowError> pure contract {
  requires index < 4_u64;
  ensures when Ok(value: small): small < 4_u8;
} {
  return narrow(index: index);
}

fn propagated(index: u64) -> result: Result<u8, NarrowError> pure contract {
  requires index < 4_u64;
} {
  let pending = cvt.checked::<u64, u8>(index);
  set index = 1000_u64;
  let small = propagate pending;
  let restored = cvt::<u8, u64>(small);
  return Ok<u8, NarrowError>(value: values[restored]);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn checked_integer_result_loop_kills_remove_previous_iteration_evidence() {
    let source = checked_conversion_read_source(
        "other: u64, stop: Bool",
        "",
        &format!(
            "  let pending = cvt.checked::<u64, u8>(0_u64);\n  loop @again {{\n    if stop {{\n      break @again;\n    }}\n    set pending = cvt.checked::<u64, u8>(other);\n    set stop = True();\n  }}\n{CHECKED_CONVERSION_READ}"
        ),
        4,
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Op4, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedBoundsObligation { .. })
    });
}

#[test]
fn checked_integer_indirect_operands_supply_type_bounds_without_storage_equality() {
    let source =
        br#"fn captured(values: Array<u8, 1>) -> result: Result<u64, NarrowError> pure contract {
  ensures when Ok(value: payload): payload <= 255_u64;
} {
  let pending = cvt.checked::<u8, u64>(values[0_u64]);
  set values[0_u64] = 200_u8;
  return pending;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    let unsupported_equality = String::from_utf8(source.to_vec())
        .expect("UTF-8 source")
        .replace("payload <= 255_u64", "payload == 200_u64");
    assert_fn9_unproved(unsupported_equality.as_bytes());
}

#[test]
fn checked_float_success_does_not_transport_an_opaque_domain_goal() {
    let source = br#"fn repeated(value: f64) -> result: i32 pure {
  match cvt.checked::<f64, i32>(value) {
    Ok(value: payload) => {
      return cvt::<f64, i32>(value);
    }
    Err(error: refused) => {
      return 0_i32;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Op6, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedConversionDomainObligation {
                residual,
                disposition: crate::StaticObligationDisposition::Unproved,
                ..
            } if residual == "cvt.defined::<f64, i32>(value)"
        )
    });
}

#[test]
fn relation_length_rejects_a_named_constant_root() {
    let source = br#"const values: Array<i32, 1> =[0_i32];

fn length() -> result: u64 pure contract {
  define size = values.len;
  ensures result == size;
} {
  return 1_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_at(source, SemanticRule::Fn9, "result == size");
}

#[test]
fn projected_result_is_rejected_at_the_complete_final_relation() {
    let source = br#"fn projected(value: i32) -> result: i32 pure contract {
  ensures result.field == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_at(source, SemanticRule::Fn9, "result.field == value");
}

#[test]
fn a_nonbare_result_use_in_an_ensures_expression_is_still_rejected() {
    let source = br#"fn hidden(value: i32) -> result: i32 pure contract {
  ensures deref(result) == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_at(source, SemanticRule::Fn9, "deref(result) == value");
}

#[test]
fn selected_returns_retain_deref_field_and_field_length_places() {
    let source = br#"struct Pair {
  value: i32;
}

struct Values {
  items: Slots<u8, 2>;
}

fn from_cell(owner: Box<Pair>) -> result: i32 pure contract {
  ensures result == owner.inner.value;
} {
  return owner.inner.value;
}

fn from_shared(owner: &Pair) -> result: i32 reads(owner.value) contract {
  ensures result == deref(owner).value;
} {
  return deref(owner).value;
}

fn field_length(values: Values) -> result: u64 pure contract {
  define size = values.items.len;
  ensures result == size;
} {
  return values.items.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

fn measured_call_return_source(generic: bool, bind_result: bool, expected: u64) -> String {
    let (parameters, element, arguments) = if generic {
        ("<T>", "T", "::<Box<u64>>")
    } else {
        ("", "Box<u64>", "")
    };
    let build_arguments = if generic { "::<T>" } else { "" };
    let returned = if bind_result {
        format!("let filled = build{build_arguments}(value: move value);\n  return move filled;")
    } else {
        format!("return build{build_arguments}(value: move value);")
    };
    format!(
        "fn build{parameters}(value: {element}) -> result: Slots<{element}, 1> pure contract {{\n  ensures result.len == 1_u64;\n}} {{\n  let vacant = slots_new::<{element}, 1>();\n  place_back(window: &vacant, value: move value);\n  return move vacant;\n}}\n\nfn singleton{parameters}(value: {element}) -> result: Slots<{element}, 1> pure contract {{\n  ensures result.len == {expected}_u64;\n}} {{\n  {returned}\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  let value = box_new::<u64>(value: 17_u64);\n  let items = singleton{arguments}(value: move value);\n  return std::process::exit_status(code: 0_u8);\n}}\n"
    )
}

#[test]
fn a_direct_measured_call_return_reports_its_unsupported_result_datum() {
    for generic in [false, true] {
        let source = measured_call_return_source(generic, false, 1);
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("an unnamed call result has no FN-9 return datum: {outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Fn9);
            assert!(matches!(
                issue.kind(),
                SemanticIssueKind::InvalidPostconditionReturn
            ));
        });
        assert_rule_at(
            source.as_bytes(),
            SemanticRule::Fn9,
            if generic {
                "return build::<T>(value: move value);"
            } else {
                "return build(value: move value);"
            },
        );
    }
}

#[test]
fn a_bound_measured_call_return_preserves_the_kernel_result_relation() {
    for generic in [false, true] {
        assert_complete(measured_call_return_source(generic, true, 1).as_bytes());
    }
}

/// [CALL-4, FN-9] an unnamed affine result ordinal imposes no condition on a
/// generic schema relation over a different, measured ordinal. The published
/// length must therefore reach a destructuring-let binder in the generic
/// caller even though its sibling result contains the symbolic element type.
#[test]
fn an_unnamed_generic_result_does_not_suppress_a_named_measure_summary() {
    let source = br#"struct Entry<T> {
  payload: Box<T>;
}

fn build<T>(first: Box<T>, replacement: Box<T>) -> (slots: Slots<Option<Entry<T>>, 1>, returned: Box<T>) pure contract {
  ensures slots.len == 1_u64;
} {
  let stored = Entry<T>(payload: move first);
  let occupied = Some<Entry<T>>(value: move stored);
  let slots = slots_new::<Option<Entry<T>>, 1>();
  place_back(window: &slots, value: move occupied);
  return move slots, move replacement;
}

fn compose<T>(first: Box<T>, replacement: Box<T>) -> (slots: Slots<Option<Entry<T>>, 1>, returned: Box<T>) pure contract {
  ensures slots.len >= 1_u64;
} {
  let (slots, returned) = build::<T>(first: move first, replacement: move replacement);
  return move slots, move returned;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn a_measured_postcondition_cannot_omit_a_direct_call_return() {
    for returned in ["slots_new::<u8, 1>()", "empty()"] {
        let source = format!(
            "fn empty() -> result: Slots<u8, 1> pure {{\n  return slots_new::<u8, 1>();\n}}\n\nfn choose(keep: Bool) -> result: Slots<u8, 1> pure contract {{\n  ensures result.len == 1_u64;\n}} {{\n  if keep {{\n    let vacant = slots_new::<u8, 1>();\n    place_back(window: &vacant, value: 17_u8);\n    let full = move vacant;\n    return move full;\n  }}\n  return {returned};\n}}\n\n{ORDINARY_MAIN}"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("a proved branch must not hide an invalid selected return");
            };
            assert_eq!(issue.rule(), SemanticRule::Fn9);
            assert!(matches!(
                issue.kind(),
                SemanticIssueKind::InvalidPostconditionReturn
            ));
        });
        assert_rule_at(
            source.as_bytes(),
            SemanticRule::Fn9,
            &format!("return {returned};"),
        );
    }
}

fn measured_recursive_return_source(bind_result: bool) -> String {
    let returned = if bind_result {
        "let forwarded = forward(items: move items, again: again);\n    return move forwarded;"
    } else {
        "return forward(items: move items, again: again);"
    };
    format!(
        "fn forward(items: Slots<u8, 1>, again: Bool) -> result: Slots<u8, 1> pure contract {{\n  requires items.len == 1_u64;\n  ensures result.len == 1_u64;\n}} {{\n  if again {{\n    {returned}\n  }}\n  return move items;\n}}\n\n{ORDINARY_MAIN}"
    )
}

#[test]
fn a_measured_recursive_return_requires_a_named_result_datum() {
    let source = measured_recursive_return_source(false);
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("a recursive call has no implicit return datum");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn9);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::InvalidPostconditionReturn
        ));
    });
    assert_rule_at(
        source.as_bytes(),
        SemanticRule::Fn9,
        "return forward(items: move items, again: again);",
    );
}

#[test]
fn a_bound_measured_recursive_return_cannot_assume_its_own_summary() {
    let source = measured_recursive_return_source(true);
    assert_fn9_unproved(source.as_bytes());
    assert_rule_at(
        source.as_bytes(),
        SemanticRule::Fn9,
        "return move forwarded;",
    );
}

#[test]
fn a_bound_measured_call_return_still_refutes_a_wrong_postcondition() {
    for generic in [false, true] {
        let source = measured_call_return_source(generic, true, 0);
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("a wrong length relation must fail FN-9: {outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Fn9);
            let SemanticIssueKind::UndischargedPostcondition(detail) = issue.kind() else {
                panic!("a selected return must retain its proof failure: {issue:?}");
            };
            assert_eq!(
                detail.disposition,
                crate::PostconditionProofDisposition::Refuted
            );
        });
        assert_rule_at(source.as_bytes(), SemanticRule::Fn9, "return move filled;");
    }
}

#[test]
fn an_unselected_error_skips_other_measured_call_return_datums() {
    for late_route in [false, true] {
        let (results, success, failure) = if late_route {
            (
                "items: Slots<u8, 1>, status: Result<u64, u8>",
                "move full, Ok<u64, u8>(value: 0_u64)",
                "slots_new::<u8, 1>(), Err<u64, u8>(error: 1_u8)",
            )
        } else {
            (
                "status: Result<u64, u8>, items: Slots<u8, 1>",
                "Ok<u64, u8>(value: 0_u64), move full",
                "Err<u64, u8>(error: 1_u8), slots_new::<u8, 1>()",
            )
        };
        let source = format!(
            "fn build(keep: Bool) -> ({results}) pure contract {{\n  ensures when status is Ok(value: accepted): items.len == 1_u64;\n}} {{\n  if keep {{\n    let empty = slots_new::<u8, 1>();\n    place_back(window: &empty, value: 17_u8);\n    let full = move empty;\n    return {success};\n  }}\n  return {failure};\n}}\n\n{ORDINARY_MAIN}"
        );
        assert_complete(source.as_bytes());
    }
}

#[test]
fn a_holder_alias_does_not_change_the_selected_return_term_identity() {
    let source = br#"struct Pair {
  value: i32;
}

fn from_shared_alias(owner: &Pair) -> result: i32 reads(owner.value) contract {
  ensures result == deref(owner).value;
} {
  let aliased = owner;
  return deref(aliased).value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let proof = postcondition_proof(source, "from_shared_alias");
    assert_eq!(
        dispositions(&proof),
        vec![PostconditionDisposition::Unproved]
    );
    assert_rule_at(source, SemanticRule::Fn9, "return deref(aliased).value;");
}

#[test]
fn a_concrete_const_substitution_is_retained_with_a_selected_length() {
    let source = br#"fn count<const n: u64>(values: Slots<u8, n>) -> result: u64 pure contract {
  ensures result == result;
} {
  return values.len;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u8, 1>();
  let one = count::<1>(values: move values);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn an_actual_may_publish_an_additional_ensures() {
    let source = br#"interface Maker {
  fn make() -> result: i32 pure;
}

fn make() -> result: i32 pure contract {
  ensures result == 1_i32;
} {
  return 1_i32;
}

binding Made : Maker {
  make = make;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    // [FN-4] permits stronger actual postconditions. The former equality-only
    // signature test rejected this legal refinement.
    assert_complete(source);
}

#[test]
fn an_actual_that_does_not_establish_the_promised_ensures_is_fn4() {
    let source = br#"interface Maker {
  fn make() -> result: i32 pure contract {
    ensures result == 1_i32;
  };
}

fn make() -> result: i32 pure contract {
  ensures result == 2_i32;
} {
  return 2_i32;
}

binding Made : Maker {
  make = make;
}
"#;
    assert_rule_at(source, SemanticRule::Fn4, "make = make;");
}

#[test]
fn an_invalid_formal_header_precedes_the_postcondition_proof_boundary() {
    let source = br#"interface Invalid<fn make() -> result: u64 pure> {
}

fn identity(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn3,
        SemanticIssueKind::type_mismatch(
            "an interface header contains only flat type and const parameters",
            "a nonmatching behavior argument",
        ),
    );
}

#[test]
fn retired_law_syntax_precedes_the_postcondition_proof_boundary() {
    let source = br#"contract InvalidLaw {
  fn combine(x: u64, y: u64) -> result: u64 pure;
  law identity(combine, unit);
}

fn identity(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    // D7 retires the law mechanism. Preserve the old source as a grammar
    // rejection; it cannot supply or bypass any postcondition proof.
    super::assert_parse_rule(source, crate::SyntaxRule::Gram2);
}

#[test]
fn invalid_selector_precedes_an_unresolved_name_in_its_entry() {
    let source = br#"fn invalid() -> result: unit pure contract {
  ensures result == missing;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionSelector,
    );
}

#[test]
fn every_concrete_selector_is_admitted_before_any_entry_lookup() {
    let source = br#"fn first(value: i32) -> result: i32 pure contract {
  ensures result == missing;
} {
  return value;
}

fn second() -> result: unit pure contract {
  ensures result == result;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionSelector,
    );
}

#[test]
fn admitted_selector_forwards_the_original_entry_lookup_issue() {
    let source = br#"fn unresolved(value: i32) -> result: i32 pure contract {
  ensures result == missing;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::ResolutionIssue { issue } = outcome else {
            panic!("expected delayed resolution issue, got {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type5);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::UnresolvedUse {
                spelling,
                role: LexicalUseRole::PlaceBase,
                ..
            } if spelling == "missing"
        ));
    });
}

#[test]
fn entry_inventory_precedes_a_poisoned_body_constructor() {
    let source = br#"fn poisoned(value: i32) -> result: i32 pure contract {
  define cvt = value == value;
  ensures result == value;
} {
  return Missing();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected contract-definition inventory issue, got {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Form3);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::ReservedName { spelling, .. } if spelling == "cvt"
        ));
    });
}

#[test]
fn unused_generic_entry_issue_precedes_its_body_semantics() {
    let source = br#"fn generic<T: drop>(value: T) -> result: T pure contract {
  ensures result == missing;
} {
  return slots_new::<u8, 1>();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::ResolutionIssue { issue } = outcome else {
            panic!("unused generic entry lookup must win, got {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type5);
    });
}

#[test]
fn a_successfully_resolved_foreign_variant_is_an_fn9_source_issue() {
    // [TYPE-6] a route label resolves against its result ordinal's enum, so
    // the resolved variant here belongs to the result's own source enum;
    // FN-9 then admits only the prelude `Ok` route.
    let source = br#"enum Foreign {
  ForeignCase(value: i32);
}

fn selected(value: i32) -> result: Foreign pure contract {
  ensures when ForeignCase(value: payload): payload == value;
} {
  return Foreign::ForeignCase(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("foreign selector variant must be FN-9, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn9);
        assert_eq!(
            issue.kind(),
            &SemanticIssueKind::InvalidPostconditionSelector
        );
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        assert_eq!(&source[start..end], b"ForeignCase(value: payload)");
    });
}

#[test]
fn concrete_generic_instances_do_not_reuse_symbolic_selector_class() {
    let source = br#"fn identity<T: drop>(value: T) -> result: T pure contract {
  ensures result == result;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let good = identity::<i32>(value: 1_i32);
  let flag = True();
  let bad = identity::<Bool>(value: flag);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionSelector,
    );
}

#[test]
fn delayed_entry_lookup_precedes_unrelated_entry_form_semantics() {
    let source = br#"fn probe(value: i32) -> result: i32 pure contract {
  ensures result == missing;
} {
  return value;
}

fn main() -> result: i32 pure {
  return 0_i32;
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::ResolutionIssue { issue } = outcome else {
            panic!("delayed lookup must precede unrelated semantics, got {outcome:?}");
        };
        assert_eq!(issue.rule(), ResolutionRule::Type5);
        assert!(matches!(
            issue.kind(),
            ResolutionIssueKind::UnresolvedUse { spelling, .. } if spelling == "missing"
        ));
    });
}

#[test]
fn selector_preflight_precedes_unrelated_entry_form_semantics() {
    let source = br#"fn invalid() -> result: unit pure contract {
  ensures result == result;
} {
  return unit;
}

fn main() -> result: i32 pure {
  return 0_i32;
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionSelector,
    );
}

#[test]
fn unused_numeric_bounds_preserve_selector_class_information() {
    let source = br#"fn invalid<T: Float>(value: T) -> result: T pure contract {
  ensures feq(result, value);
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionSelector,
    );
}

#[test]
fn unavailable_generic_type_argument_does_not_invent_a_selector_instance() {
    let source = br#"fn generic<T: drop>(value: T) -> result: T pure contract {
  define cvt = value == value;
  ensures result == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let unavailable = generic::<Missing>(value: unit);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!(
                "contract-definition inventory must beat an unavailable type argument: {outcome:?}"
            );
        };
        assert_eq!(issue.rule(), ResolutionRule::Form3);
    });
}

#[test]
fn unavailable_const_argument_does_not_invent_a_selector_instance() {
    let source = br#"fn generic<T: drop, const n: u64>(value: T) -> result: T pure contract {
  define cvt = value == value;
  ensures result == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let unavailable = generic::<unit, missing>(value: unit);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_resolution(source, |outcome| {
        let ResolutionOutcome::SourceIssue { issue, .. } = outcome else {
            panic!(
                "contract-definition inventory must beat an unavailable const argument: {outcome:?}"
            );
        };
        assert_eq!(issue.rule(), ResolutionRule::Form3);
    });
}

#[test]
fn unrelated_invalid_constant_does_not_suppress_an_independent_selector() {
    let source = br#"const bad: u8 = 1_u16;

fn invalid() -> result: unit pure contract {
  ensures result == missing;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionSelector,
    );
}

#[test]
fn transitive_invalid_constant_does_not_become_a_compiler_failure() {
    let source = br#"const bad: u8 = 1_u16;

const aliased: u8 = bad;

fn invalid() -> result: unit pure contract {
  ensures result == result;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionSelector,
    );
}

/// An unavailable header must report its own premise failure before the
/// unresolved selector. A full array now admits an affine element, so use
/// the real PROV-6 failure of forwarding an affine parameter to a copy bound.
#[test]
fn unavailable_symbolic_header_does_not_forward_its_entry_issue() {
    let source = br#"struct CopyOnly<T: copy> {
  value: T;
}

fn unavailable<T: drop>(value: CopyOnly<T>) -> result: T pure contract {
  ensures result == missing;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("ordinary header issue must win, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Prov6);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::LinearityBoundMismatch {
                bound: "copy",
                actual: "affine",
                ..
            }
        ));
    });
}

#[test]
fn unavailable_record_does_not_suppress_a_later_independent_selector() {
    let source = br#"struct CopyOnly<T: copy> {
  value: T;
}

fn unavailable<T: drop>(value: CopyOnly<T>) -> result: T pure contract {
  ensures result == missing;
} {
  return value;
}

fn invalid() -> result: unit pure contract {
  ensures result == result;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionSelector,
    );
}

#[test]
fn invalid_unrelated_function_template_does_not_suppress_selector_admission() {
    let source = br#"fn broken<const n: Bool>() -> result: unit pure {
  return unit;
}

fn invalid() -> result: unit pure contract {
  ensures result == result;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionSelector,
    );
}

/// The symbolic affine parameter cannot satisfy CopyOnly's copy bound,
/// although the selected concrete argument i32 can. Checking just that
/// concrete instance would miss the referenced template's PROV-6 failure.
#[test]
fn referenced_generic_nominal_must_pass_its_symbolic_template_judgment() {
    let source = br#"struct CopyOnly<T: copy> {
  value: T;
}

struct Invalid<T: drop> {
  values: CopyOnly<T>;
}

fn probe(value: Invalid<i32>) -> result: unit pure contract {
  ensures result == missing;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("the referenced symbolic nominal premise must win: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Prov6);
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::LinearityBoundMismatch {
                bound: "copy",
                actual: "affine",
                ..
            }
        ));
    });
}

#[test]
fn unrelated_invalid_generic_nominal_does_not_suppress_selector_admission() {
    let source = br#"struct CopyOnly<T: copy> {
  value: T;
}

struct Invalid<T: drop> {
  values: CopyOnly<T>;
}

fn invalid() -> result: unit pure contract {
  ensures result == missing;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Fn9,
        SemanticIssueKind::InvalidPostconditionSelector,
    );
}

#[test]
fn postcondition_components_are_callee_before_caller_and_publish_atomically() {
    let source = br#"fn top(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let ignored = bridge(value: value);
  return value;
}

fn leaf(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn middle(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let ignored = leaf(value: value);
  return value;
}

fn bridge(value: i32) -> result: i32 pure {
  let ignored = middle(value: value);
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("independent call chain must check completely: {outcome:?}");
        };
        let mut scheduled = Vec::new();
        for (ordinal, component) in checked
            .data
            .postcondition_schedule
            .components
            .iter()
            .enumerate()
        {
            assert_eq!(component.ordinal as usize, ordinal);
            assert!(
                component
                    .functions
                    .windows(2)
                    .all(|pair| pair[0].0 < pair[1].0)
            );
            assert!(component.summaries.windows(2).all(|pair| (
                pair[0].function.0,
                pair[0].relation_ordinal
            ) < (
                pair[1].function.0,
                pair[1].relation_ordinal
            )));
            assert!(component.summaries.iter().all(|summary| {
                summary.component == component.ordinal
                    && component.functions.contains(&summary.function)
            }));
            scheduled.extend(component.functions.iter().copied());
        }
        scheduled.sort_unstable_by_key(|function| function.0);
        assert_eq!(
            scheduled,
            checked
                .data
                .functions
                .iter()
                .map(|function| function.id)
                .collect::<Vec<_>>()
        );
        let function = |name: &str| {
            checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .unwrap_or_else(|| panic!("missing {name}"))
        };
        let component = |id| {
            checked
                .data
                .postcondition_schedule
                .components
                .iter()
                .find(|component| component.functions.contains(&id))
                .expect("every concrete function belongs to one component")
        };
        let leaf = function("leaf");
        let middle = function("middle");
        let bridge = function("bridge");
        let top = function("top");
        assert!(component(leaf.id).ordinal < component(middle.id).ordinal);
        assert!(component(middle.id).ordinal < component(bridge.id).ordinal);
        assert!(component(bridge.id).ordinal < component(top.id).ordinal);
        assert!(component(bridge.id).summaries.is_empty());
        for function in [leaf, middle, top] {
            let proof = function
                .entailment
                .postconditions
                .first()
                .expect("postcondition proof");
            let summary = proof.summary.as_ref().expect("published summary");
            assert_eq!(summary.function, function.id);
            assert_eq!(summary.block, proof.block);
            assert_eq!(summary.relation_ordinal, 0);
            assert_eq!(summary.component, component(function.id).ordinal);
            assert_eq!(component(function.id).summaries, vec![summary.clone()]);
        }
    });
    assert_complete(source);
}

#[test]
fn an_independently_proved_mutual_component_publishes_summaries_in_function_order() {
    let source = br#"fn first(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let ignored = second(value: value);
  return value;
}

fn second(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let ignored = first(value: value);
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("independent mutual recursion must check completely: {outcome:?}");
        };
        let component = checked
            .data
            .postcondition_schedule
            .components
            .iter()
            .find(|component| component.functions.len() == 2)
            .expect("one mutual component");
        assert!(
            component
                .functions
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0)
        );
        assert_eq!(component.summaries.len(), 2);
        assert!(component.summaries.windows(2).all(|pair| (
            pair[0].function.0,
            pair[0].relation_ordinal
        ) < (
            pair[1].function.0,
            pair[1].relation_ordinal
        )));
        for summary in &component.summaries {
            let proof = checked.data.functions[summary.function.0 as usize]
                .entailment
                .postconditions
                .first()
                .expect("mutual proof");
            assert_eq!(proof.summary.as_ref(), Some(summary));
            assert!(proof.aggregate.discharged);
        }
    });
    assert_complete(source);
}

#[test]
fn an_independently_proved_self_recursive_component_publishes_its_summary() {
    let source = br#"fn recursive(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let ignored = recursive(value: value);
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("independent recursion must check completely: {outcome:?}");
        };
        let recursive = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "recursive")
            .expect("recursive function");
        let component = checked
            .data
            .postcondition_schedule
            .components
            .iter()
            .find(|component| component.functions.contains(&recursive.id))
            .expect("recursive component");
        assert_eq!(component.functions, vec![recursive.id]);
        let summary = recursive
            .entailment
            .postconditions
            .first()
            .and_then(|proof| proof.summary.as_ref())
            .expect("independent recursive summary publishes");
        assert_eq!(component.summaries, vec![summary.clone()]);
    });
    assert_complete(source);
}

#[test]
fn one_failed_mutual_member_withholds_the_whole_component_summary_batch() {
    let source = br#"fn left(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let ignored = right(value: value);
  return value;
}

fn right(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let called = left(value: value);
  return called;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("dark mutual component must retain failure metadata: {outcome:?}");
        };
        let members = checked
            .data
            .functions
            .iter()
            .filter(|function| matches!(function.name.as_str(), "left" | "right"))
            .collect::<Vec<_>>();
        assert_eq!(members.len(), 2);
        let component = checked
            .data
            .postcondition_schedule
            .components
            .iter()
            .find(|component| component.functions.contains(&members[0].id))
            .expect("mutual component");
        assert_eq!(component.functions.len(), 2);
        assert!(component.summaries.is_empty());
        assert!(members.iter().all(|function| {
            function
                .entailment
                .postconditions
                .first()
                .is_some_and(|proof| proof.summary.is_none())
        }));
        assert!(
            members
                .iter()
                .find(|function| function.name == "left")
                .unwrap()
                .entailment
                .postconditions
                .first()
                .unwrap()
                .aggregate
                .discharged
        );
        assert!(
            !members
                .iter()
                .find(|function| function.name == "right")
                .unwrap()
                .entailment
                .postconditions
                .first()
                .unwrap()
                .aggregate
                .discharged
        );
    });
    assert_fn9_unproved(source);
}

#[test]
fn a_seedless_mutual_postcondition_cycle_publishes_no_summary() {
    let source = br#"fn first(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let called = second(value: value);
  return called;
}

fn second(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let called = first(value: value);
  return called;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("dark seedless cycle must retain dispositions: {outcome:?}");
        };
        let component = checked
            .data
            .postcondition_schedule
            .components
            .iter()
            .find(|component| component.functions.len() == 2)
            .expect("seedless mutual component");
        assert!(component.summaries.is_empty());
        for function in component
            .functions
            .iter()
            .map(|id| &checked.data.functions[id.0 as usize])
        {
            let proof = function.entailment.postconditions.first().unwrap();
            assert!(!proof.aggregate.discharged);
            assert!(proof.summary.is_none());
        }
    });
    assert_fn9_unproved(source);
}

#[test]
fn concrete_generic_instances_receive_distinct_verified_summary_identities() {
    let source = br#"fn identity<T: Int>(value: T) -> result: T pure contract {
  ensures result == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let small = identity::<i32>(value: 1_i32);
  let wide = identity::<u64>(value: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("concrete generic summaries must check completely: {outcome:?}");
        };
        let instances = checked
            .data
            .functions
            .iter()
            .filter(|function| function.name == "identity")
            .collect::<Vec<_>>();
        assert_eq!(instances.len(), 2);
        let summaries = instances
            .iter()
            .map(|function| {
                function
                    .entailment
                    .postconditions
                    .first()
                    .and_then(|proof| proof.summary.as_ref())
                    .expect("concrete summary")
            })
            .collect::<Vec<_>>();
        assert_ne!(summaries[0].function, summaries[1].function);
        assert_ne!(summaries[0].component, summaries[1].component);
    });
    assert_complete(source);
}

#[test]
fn a_concrete_instance_named_only_by_an_uninstantiated_generic_still_checks_fn9() {
    let source = br#"fn bad<T: Int>(value: T) -> result: T pure contract {
  ensures result < value;
} {
  return value;
}

fn wrapper<U: drop>() -> result: unit pure {
  let ignored = bad::<u8>(value: 0_u8);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("the replayed bad<u8> postcondition must reject: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn9);
        let SemanticIssueKind::UndischargedPostcondition(detail) = issue.kind() else {
            panic!("FN-9 must retain its concrete proof disposition: {issue:?}");
        };
        assert_eq!(
            detail.disposition,
            crate::PostconditionProofDisposition::Refuted
        );
        // [FN-2] the instance is named as a call writes it, never by the
        // internal symbol that keys its lowering.
        assert_eq!(detail.concrete_function, "bad::<u8>");
    });
}

#[test]
fn a_unit_without_writer_postconditions_still_publishes_prelude_contracts() {
    let source = br#"fn helper(value: i32) -> result: i32 pure {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let ignored = helper(value: 1_i32);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("no-postcondition control must check completely: {outcome:?}");
        };
        // v0.58 PRE-1 records contribute ordinary signature contracts even
        // when every writer-defined function has no postcondition.
        assert!(
            checked
                .data
                .functions
                .iter()
                .filter(|function| function.body.is_some())
                .all(|function| function.postconditions.is_empty())
        );
        let declared = checked
            .data
            .functions
            .iter()
            .filter(|function| function.body.is_none() && !function.postconditions.is_empty())
            .collect::<Vec<_>>();
        assert!(!declared.is_empty());
        for function in declared {
            for proof in &function.entailment.postconditions {
                assert!(proof.aggregate.discharged);
                assert!(proof.summary.is_some());
                assert!(proof.exits.is_empty());
            }
        }
    });
    assert_complete(source);
}

/// [OP-10, PRE-1, ENT-3.S12] `append` publishes the destination's exit
/// length above the source's captured entry length. A known nonempty source
/// therefore proves a later positive-length requirement after `append` has
/// reset that source to empty.
#[test]
fn append_transfers_a_known_source_entry_length_to_the_destination() {
    let source = br#"fn take_after_append() -> result: u8 pure {
  let source = slots_new::<u8, 4>();
  place_back(window: &source, value: 7_u8);
  let destination = slots_new::<u8, 4>();
  append(destination: &destination, source: &source);
  return take_back(window: &destination);
}
"#;
    assert_complete(source);
}

/// The cross-window relation is a lower bound, not a claim that every append
/// makes its destination nonempty. An empty or conditionally filled source
/// supplies no proof of `take_back`'s positive-length requirement.
#[test]
fn append_does_not_invent_a_positive_destination_length() {
    let empty = br#"fn take_after_empty_append() -> result: u8 pure {
  let source = slots_new::<u8, 4>();
  let destination = slots_new::<u8, 4>();
  append(destination: &destination, source: &source);
  return take_back(window: &destination);
}
"#;
    assert_rule_kind(empty, SemanticRule::Fn8, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallRequirement(_))
    });

    let unknown = br#"fn take_after_unknown_append(fill: Bool) -> result: u8 pure {
  let source = slots_new::<u8, 4>();
  if fill {
    place_back(window: &source, value: 7_u8);
  }
  let destination = slots_new::<u8, 4>();
  append(destination: &destination, source: &source);
  return take_back(window: &destination);
}
"#;
    assert_rule_kind(unknown, SemanticRule::Fn8, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedCallRequirement(_))
    });
}

#[test]
fn a_conditional_unique_call_keeps_the_other_branch_measure_image() {
    // [MSR-3] the callee's exit measure is `deref(values).len` and its entry
    // measure is `deref(entry(values)).len`; write permission comes from the
    // row rather than from the retiring `&uniq` marker [REF-1, EFF-1], and the
    // window is replaced by [SET-1] under [WIN-3]'s disposition.
    for branches in [
        "    if turn == 0_u64 {\n      touch(values: &values);\n    } else {\n      invariant untouched: values.len >= 16_u64;\n    }",
        "    if turn == 0_u64 {\n      invariant untouched: values.len >= 16_u64;\n    } else {\n      touch(values: &values);\n    }",
    ] {
        let source = format!(
            r#"fn touch(values: &Slots<u8, 16>) -> result: unit writes(values) contract {{
  requires deref(values).len >= 1_u64;
  ensures deref(values).len == deref(entry(values)).len;
}} {{
  set deref(values)[0_u64] = 7_u8;
  return unit;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let seed = array_filled::<u8, 16>(value: 0_u8);
  let values = slots_from_array::<u8, 16>(values: seed);
  let turn = 0_u64;
  loop @rounds (
    invariant room: values.len >= 16_u64
  ) {{
    if turn >= 2_u64 {{
      break @rounds;
    }}
{branches}
    let spare = array_filled::<u8, 16>(value: 0_u8);
    let fresh = slots_from_array::<u8, 16>(values: spare);
    set values = move fresh;
    set turn = turn +wrap 1_u64;
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
        assert_complete(source.as_bytes());
    }
}

/// A callee whose row writes the referent and publishes no length relation
/// kills the caller's measure image: the invariant stated before the call
/// holds and the one restated after it does not.
///
/// v0.59 spelled the replacement `let previous = replace deref(values) = move
/// empty;`. The `replace` statement is retired [SET-2]; [SET-1] writes the
/// same place and [WIN-3] releases the affine value the target held.
#[test]
fn a_referent_replacement_still_kills_its_own_branch_measure_image() {
    let source = br#"fn clear(values: &Slots<u8, 16>) -> result: unit writes(values) {
  let empty = slots_new::<u8, 16>();
  set deref(values) = move empty;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u8, 16>();
  place_back(window: &values, value: 7_u8);
  invariant before: values.len >= 1_u64;
  clear(values: &values);
  invariant stale: values.len >= 1_u64;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("a changed referent must lose its old measure: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Inv1);
        let SemanticIssueKind::UndischargedLocalInvariant { name, .. } = issue.kind() else {
            panic!("the old measure must fail at the local invariant: {issue:?}");
        };
        assert_eq!(name, "stale");
    });
}

/// Asserts that one source is rejected by [FN-9] at a selected return.
///
/// The disposition is deliberately not pinned. [DIAG-3] fixes that it is
/// "exactly `unproved` or `refuted`" and that entry-image unavailability fixes
/// `unproved`, but which of the two a provable contradiction reaches depends
/// on the [ENT-4] closure rather than on a rule, and this test is about the
/// two states being two terms.
fn assert_fn9_rejects(source: &[u8]) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("the exit relation must be rejected: {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn9);
        assert!(
            matches!(
                issue.kind(),
                SemanticIssueKind::UndischargedPostcondition(_)
            ),
            "FN-9 must reject at the relation: {issue:?}"
        );
    });
}

/// The entry and the exit measure of a written reference parameter are two
/// distinct terms, and both are read at the selected return whether the
/// callee's own call is that return or a statement before it.
///
/// [MSR-3]'s table gives `ensures, deref(reference parameter the row writes)`
/// the exit-state term and `ensures, deref(entry(reference parameter the row
/// writes))` the immutable entry datum, and says so in one sentence: "Entry
/// and exit measures are distinct terms even when both project from the same
/// formal and actual." [FN-9] reads the bare measure "over that parameter's
/// resolved referent immediately before each selected return, after the
/// return's ordinary effects and kills", so a `return take_back(window: free);`
/// must apply that call's own boundary -- its projected write kills and its
/// published exit relation -- before the clause is queried, exactly as the
/// two-statement form does (compiler/checker-facts, pending).
///
/// Under `requires deref(free).len == 2_u64` the entry length is two and
/// `take_back`'s `ensures deref(window).len + 1_u64 == deref(entry(window)).len`
/// makes the exit length one. So `+ 1_u64 == entry` and `== 1_u64` hold and
/// `== entry` and `== 2_u64` do not, in both body forms. A reading that took
/// the entry state at the exit would accept `== entry` and `== 2_u64` and
/// reject the other two.
#[test]
fn entry_and_exit_measures_are_two_states_at_a_returned_call_and_at_a_statement() {
    let program = |clause: &str, body: &str| {
        format!(
            r#"fn take_one(free: &Slots<u8, 4>) -> taken: u8 writes(free.last), writes(free.len) contract {{
  requires deref(free).len == 2_u64;
  ensures {clause};
}} {{
{body}
}}

fn main() -> status: std::process::ExitStatus pure {{
  let free = slots_new::<u8, 4>();
  place_back(window: &free, value: 1_u8);
  place_back(window: &free, value: 2_u8);
  let value = take_one(free: &free);
  return std::process::exit_status(code: value);
}}
"#
        )
    };
    let returned_call = "  return take_back(window: free);";
    let own_statement = "  let one = take_back(window: free);\n  return one;";
    for body in [returned_call, own_statement] {
        assert_complete(
            program("deref(free).len + 1_u64 == deref(entry(free)).len", body).as_bytes(),
        );
        assert_fn9_rejects(program("deref(free).len == deref(entry(free)).len", body).as_bytes());
        assert_complete(program("deref(free).len == 1_u64", body).as_bytes());
        assert_fn9_rejects(program("deref(free).len == 2_u64", body).as_bytes());
    }
}

/// [MSR-2, MSR-3] a postcondition over a window descriptor survives a write
/// to one element reached through the same reference holder. Writes that can
/// change the descriptor, replace the whole referent, or reach it through a
/// joined holder still invalidate the relation, as does rebinding the holder
/// on which the substitution itself depends.
#[test]
fn postcondition_measure_candidates_distinguish_holder_rebinding_from_descendant_writes() {
    let descendant_write = br#"fn take_at(window: &Ring<Box<u64>, 4>, at: u64) -> taken: Box<u64> writes(window) contract {
  requires at + 2_u64 <= deref(window).len;
  ensures deref(window).len + 1_u64 == deref(entry(window)).len;
} {
  let end = take_back(window: window);
  swap(first: &deref(window)[at], second: &end);
  return move end;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(descendant_write);

    let descriptor_write =
        br#"fn take_twice(window: &Ring<u64, 4>) -> taken: u64 writes(window) contract {
  requires deref(window).len >= 2_u64;
  ensures deref(window).len + 1_u64 == deref(entry(window)).len;
} {
  let taken = take_back(window: window);
  let extra = take_back(window: window);
  return taken;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_fn9_rejects(descriptor_write);

    let whole_replacement = br#"fn clear(window: &Ring<u64, 4>) -> result: unit writes(window) {
  let empty = ring_new::<u64, 4>();
  set deref(window) = move empty;
  return unit;
}

fn replace_after_take(window: &Ring<u64, 4>) -> taken: u64 writes(window) contract {
  requires deref(window).len >= 2_u64;
  ensures deref(window).len + 1_u64 == deref(entry(window)).len;
} {
  let taken = take_back(window: window);
  clear(window: window);
  return taken;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_fn9_rejects(whole_replacement);

    let joined_write = br#"fn clear(window: &Ring<u64, 4>) -> result: unit writes(window) {
  let empty = ring_new::<u64, 4>();
  set deref(window) = move empty;
  return unit;
}

fn maybe_replace(first: &Ring<u64, 4>, second: &Ring<u64, 4>, choose: Bool) -> taken: u64 writes(first), writes(second) contract {
  requires deref(first).len >= 2_u64;
  ensures deref(first).len + 1_u64 == deref(entry(first)).len;
} {
  let taken = take_back(window: first);
  let selected = if choose {
    give first;
  } else {
    give second;
  }
  clear(window: selected);
  return taken;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_fn9_rejects(joined_write);

    let holder_rebinding = br#"fn rebind_after_take(first: &Ring<u64, 4>, second: &Ring<u64, 4>) -> taken: u64 writes(first) contract {
  requires deref(first).len >= 2_u64;
  ensures deref(first).len + 1_u64 == deref(entry(first)).len;
} {
  let selected = first;
  let taken = take_back(window: selected);
  set selected = &deref(second);
  return taken;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_fn9_rejects(holder_rebinding);
}
