use crate::{
    OverlapLowering, SemanticIssueKind, SemanticLocation, SemanticOutcome, SemanticRule,
    lower_checked,
};

use super::super::goal::{GoalExpression, GoalOperation};
use super::super::model::{
    CheckedExpression, CheckedIntegerOperation, CheckedNominalKind, CheckedStatement, CheckedType,
    CheckedValue, IntegerType,
};

use super::{assert_parse_rule, assert_rule, with_semantics};

fn assert_behavior_rule(source: &str, rule: SemanticRule) {
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("expected {rule:?}, got {outcome:?}");
        };
        assert_eq!(issue.rule(), rule, "{issue:?}");
    });
}

fn assert_behavior_site(source: &str, rule: SemanticRule, expected: &str) {
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("expected {rule:?}");
        };
        assert_eq!(issue.rule(), rule, "{issue:?}");
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).unwrap();
        let end = usize::try_from(coordinate.end().value()).unwrap();
        assert_eq!(&source[start..end], expected);
    });
}

const BOUNDED_GROUP: &str = r#"interface Key<K: copy> {
  fn hash(value: K) -> result: u64 pure contract {
    ensures result <= 99_u64;
  };
}

fn constant_hash(input: u64) -> output: u64 pure contract {
  ensures output <= 99_u64;
} {
  return 17_u64;
}

binding ScalarKey : Key<u64> {
  hash = constant_hash;
}

fn apply<interface Key<K>>(value: K) -> out: u64 pure contract {
  ensures out <= 99_u64;
} {
  let result = Key::hash(value: value);
  return result;
}

fn main() -> status: std::process::ExitStatus pure {
  let result = apply::<ScalarKey>(value: 123_u64);
  let bounded = result + 1_u64;
  if bounded == 18_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#;

#[test]
fn group_contracts_publish_only_after_implied_actual_proofs() {
    for mode in [OverlapLowering::Off, OverlapLowering::On] {
        with_semantics(BOUNDED_GROUP.as_bytes(), |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("{outcome:?}");
            };
            assert!(
                checked
                    .data
                    .functions
                    .iter()
                    .all(|function| !function.formal_hypothesis)
            );
            assert!(!checked.data.contract_queries.is_empty());
            for query in &checked.data.contract_queries {
                assert!(!query.site.components().is_empty());
                assert!(!query.goal.components().is_empty());
                super::entailment::validate_derivations(&query.proof);
                let [outcome] = query.proof.contract_goals.as_slice() else {
                    panic!("one isolated FN-4 goal per retained proof namespace");
                };
                assert_eq!(
                    outcome.disposition,
                    super::super::entailment::CallGoalDisposition::Discharged
                );
                assert!(outcome.derivation.is_some());
            }
            let lowered = lower_checked(*checked, mode).expect("direct-call lowering");
            assert_eq!(
                lowered
                    .functions()
                    .iter()
                    .filter(|function| !function.blocks().is_empty())
                    .count(),
                3
            );
        });
    }
    // FN-4 uses the fixed integer entailment fragment, so equivalent written
    // bounds need not have byte-identical clause trees.
    with_semantics(
        BOUNDED_GROUP
            .replace("ensures output <= 99_u64;", "ensures output < 100_u64;")
            .as_bytes(),
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{outcome:?}"
            )
        },
    );
    assert_behavior_rule(
        &BOUNDED_GROUP.replace("return 17_u64;", "return 100_u64;"),
        SemanticRule::Fn9,
    );
}

#[test]
fn behavior_contract_implication_admits_only_weaker_requires_and_stronger_ensures() {
    let source = r#"interface Refined {
  fn refine(value: u64) -> result: u64 pure contract {
    requires value <= 10_u64;
    ensures result <= 10_u64;
  };
}

fn refined(value: u64) -> result: u64 pure contract {
  requires value <= 11_u64;
  ensures result <= 9_u64;
} {
  return 9_u64;
}

binding Refinement : Refined {
  refine = refined;
}
"#;
    with_semantics(source.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });

    // The formal caller establishes only `value <= 10`, which does not
    // establish this stronger actual requirement.
    assert_behavior_rule(
        &source.replace("requires value <= 11_u64;", "requires value <= 9_u64;"),
        SemanticRule::Fn4,
    );
    // The actual's independently proved `result <= 11` does not establish
    // the tighter promise exposed by the interface.
    assert_behavior_rule(
        &source.replace("ensures result <= 9_u64;", "ensures result <= 11_u64;"),
        SemanticRule::Fn4,
    );
}

#[test]
fn behavior_contract_implication_keeps_entry_exit_and_result_datums_distinct() {
    let source = r#"interface Grower {
  fn grow(values: &Slots<u64, 4>) -> result: u64 writes(values) contract {
    requires deref(values).len < deref(values).cap;
    ensures result == deref(entry(values)).len;
  };
}

fn grow_and_count(values: &Slots<u64, 4>) -> result: u64 writes(values) contract {
  requires deref(values).len < deref(values).cap;
  ensures result == deref(values).len;
} {
  place_back(window: values, value: 7_u64);
  return deref(values).len;
}

binding CountedGrow : Grower {
  grow = grow_and_count;
}
"#;
    // The supplied body proves its own exit-state relation, but that relation
    // does not imply the interface's entry-state promise.
    assert_behavior_rule(source, SemanticRule::Fn4);
}

#[test]
fn behavior_contract_implication_uses_routed_payload_types_and_unrouted_premises() {
    let equivalent_route = r#"interface RoutedBound {
  fn choose(value: i32) -> outcome: Result<i32, i32> pure contract {
    requires value < 100_i32;
    ensures when Ok(value: selected): selected <= 99_i32;
  };
}

fn choose(value: i32) -> outcome: Result<i32, i32> pure contract {
  requires value < 100_i32;
  ensures when Ok(value: selected): selected < 100_i32;
} {
  return Ok<i32, i32>(value: value);
}

binding RoutedChoice : RoutedBound {
  choose = choose;
}
"#;
    with_semantics(equivalent_route.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });

    let combined = r#"interface RoutedPair {
  fn pair() -> (limit: u64, outcome: Result<u64, u8>) pure contract {
    ensures when outcome is Ok(value: selected): selected <= limit;
  };
}

fn pair() -> (limit: u64, outcome: Result<u64, u8>) pure contract {
  ensures limit >= 10_u64;
  ensures when outcome is Ok(value: selected): selected <= 9_u64;
} {
  return 10_u64, Ok<u64, u8>(value: 9_u64);
}

binding RoutedPairChoice : RoutedPair {
  pair = pair;
}
"#;
    with_semantics(combined.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

#[test]
fn behavior_contract_implication_does_not_use_a_different_result_route() {
    let source = r#"interface RoutedPair {
  fn pair() -> (first: Result<u64, u8>, second: Result<u64, u8>) pure contract {
    ensures when first is Ok(value: selected): selected <= 9_u64;
  };
}

fn pair() -> (first: Result<u64, u8>, second: Result<u64, u8>) pure contract {
  ensures when second is Ok(value: selected): selected <= 9_u64;
} {
  return Ok<u64, u8>(value: 9_u64), Ok<u64, u8>(value: 9_u64);
}

binding RoutedPairChoice : RoutedPair {
  pair = pair;
}
"#;
    assert_behavior_rule(source, SemanticRule::Fn4);
}

#[test]
fn bound_atomic_updates_use_the_formal_result_route_boundary() {
    let bound = r#"interface Transform {
  fn update(value: Result<u64, Box<u64>>) -> result: Result<u64, Box<u64>> pure;
}

fn routed_update(value: Result<u64, Box<u64>>) -> result: Result<u64, Box<u64>> pure contract {
  ensures when Ok(value: payload): payload == payload;
} {
  match move value {
    Ok(value: payload) => {
      return Ok<u64, Box<u64>>(value: payload);
    }
    Err(error: problem) => {
      return Err<u64, Box<u64>>(error: move problem);
    }
  }
}

binding RoutedTransform : Transform {
  update = routed_update;
}

fn apply<interface Transform>(value: Result<u64, Box<u64>>) -> result: Result<u64, Box<u64>> pure {
  set value = Transform::update(value: move value);
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  let owner = box_new::<u64>(value: 7_u64);
  let wrapped = Err<u64, Box<u64>>(error: move owner);
  let retained = apply::<RoutedTransform>(value: move wrapped);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(bound.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });

    // The selected actual's own direct boundary still carries its routed
    // result. Only the bound call above is governed by the unrouted formal.
    let direct = r#"fn routed_update(value: Result<u64, Box<u64>>) -> result: Result<u64, Box<u64>> pure contract {
  ensures when Ok(value: payload): payload == payload;
} {
  match move value {
    Ok(value: payload) => {
      return Ok<u64, Box<u64>>(value: payload);
    }
    Err(error: problem) => {
      return Err<u64, Box<u64>>(error: move problem);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let owner = box_new::<u64>(value: 7_u64);
  let wrapped = Err<u64, Box<u64>>(error: move owner);
  set wrapped = routed_update(value: move wrapped);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_behavior_rule(direct, SemanticRule::Own1);
}

#[test]
fn bound_call_rows_keep_formal_and_actual_parameter_namespaces_distinct() {
    let source = r#"struct Pair {
  left: u64;
  right: u64;
}

interface Mixer {
  fn mix(target: &Pair, aliased: &Pair) -> result: unit reads(aliased.right), writes(target.left);
}

fn mix(destination: &Pair, observer: &Pair) -> result: unit reads(observer.right), writes(destination.left) {
  let observed = deref(observer).right;
  set deref(destination).left = observed;
  return unit;
}

binding PairMixer : Mixer {
  mix = mix;
}

fn apply<interface Mixer>(value: &Pair) -> result: unit reads(value.right), writes(value.left) {
  return Mixer::mix(target: value, aliased: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = Pair(left: 0_u64, right: 1_u64);
  let result = apply::<PairMixer>(value: &pair);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "disjoint formal paths must survive rebasing: {outcome:?}"
        );
    });

    // The same holder supplied twice makes the formal whole-object read
    // overlap its field write. The actual uses different binder names, so
    // this also guards declaration-identity rebasing at the retained edge.
    assert_behavior_rule(
        &source.replace("reads(aliased.right)", "reads(aliased)"),
        SemanticRule::Eff5,
    );
}

#[test]
fn formal_writes_cover_actual_reads_by_parameter_and_path() {
    let source = r#"struct Pair {
  left: u64;
  right: u64;
}

interface Inspect {
  fn inspect(value: &Pair) -> result: u64 writes(value);
}

fn visit_left(input: &Pair) -> result: u64 reads(input.left) {
  return deref(input).left;
}

binding ReadLeft : Inspect {
  inspect = visit_left;
}

fn apply<interface Inspect>(data: &Pair) -> result: u64 writes(data) {
  return Inspect::inspect(value: data);
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = Pair(left: 7_u64, right: 9_u64);
  let named = apply::<ReadLeft>(data: &pair);
  let raw = apply::<fn visit_left>(data: &pair);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let field_formal = source
        .replace("writes(value)", "writes(value.left)")
        .replace("writes(data)", "writes(data.left)");
    // Both binding forms normalize renamed parameters and retain the formal
    // write at the call, so apply's declared write is still exhibited. Cover
    // both a proper prefix and the exact field read by the actual.
    for accepted in [source.to_owned(), field_formal.clone()] {
        with_semantics(accepted.as_bytes(), |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{outcome:?}"
            );
        });
    }

    let writing_actual = source
        .replace("writes(value)", "reads(value)")
        .replace("writes(data)", "reads(data)")
        .replace("reads(input.left)", "writes(input.left)")
        .replace(
            "  return deref(input).left;",
            "  set deref(input).left = 1_u64;\n  return deref(input).left;",
        );
    let sibling_read = field_formal
        .replace("reads(input.left)", "reads(input.right)")
        .replace("return deref(input).left;", "return deref(input).right;");
    let uncovered_read = source
        .replace("writes(value)", "pure")
        .replace("writes(data)", "pure");
    for rejected in [writing_actual, sibling_read, uncovered_read] {
        assert_behavior_site(&rejected, SemanticRule::Fn4, "inspect = visit_left;");
    }

    // A wider formal does not let the actual pad its own exact row with an
    // unexhibited write, even though that declared write would refine it.
    assert_behavior_rule(
        &source.replace("reads(input.left)", "writes(input.left)"),
        SemanticRule::Eff2,
    );
}

#[test]
fn bound_allocating_actuals_keep_allocation_metadata_outside_fn4_rows() {
    let source = br#"interface Factory {
  fn make(value: u64) -> result: Box<u64> pure;
}

binding Allocate : Factory {
  make = box_new::<u64>;
}

binding WrappedAllocate : Factory {
  make = heap_leaf;
}

fn produce<interface Factory>(value: u64) -> result: Box<u64> pure {
  return Factory::make(value: value);
}

fn frame_constructions() -> result: Array<u64, 2> pure {
  let filled = array_filled::<u64, 2>(value: 3_u64);
  let occupied = slots_from_array::<u64, 2>(values: filled);
  let restored = slots_into_array::<u64, 2>(values: move occupied);
  let vacant = slots_new::<u64, 2>();
  let ring = ring_new::<u64, 2>();
  return restored;
}

fn heap_leaf(value: u64) -> result: Box<u64> pure {
  return box_new::<u64>(value: value);
}

fn heap_transitive(value: u64) -> result: Box<u64> pure {
  return heap_leaf(value: value);
}

fn heap_forward(value: u64) -> result: Box<u64> pure {
  return heap_forward_target(value: value);
}

fn heap_forward_target(value: u64) -> result: Box<u64> pure {
  return box_new::<u64>(value: value);
}

fn heap_cycle_left(stop: Bool, value: u64) -> result: Box<u64> pure {
  if stop {
    return heap_cycle_right(stop: stop, value: value);
  } else {
    return box_new::<u64>(value: value);
  }
}

fn heap_cycle_right(stop: Bool, value: u64) -> result: Box<u64> pure {
  return heap_cycle_left(stop: stop, value: value);
}

fn frame_cycle_left(stop: Bool) -> result: unit pure {
  if stop {
    return frame_cycle_right(stop: stop);
  } else {
    return unit;
  }
}

fn frame_cycle_right(stop: Bool) -> result: unit pure {
  return frame_cycle_left(stop: stop);
}

fn main() -> status: std::process::ExitStatus pure {
  let made = produce::<Allocate>(value: 7_u64);
  let wrapped = produce::<WrappedAllocate>(value: 8_u64);
  let frame = frame_constructions();
  let transitive = heap_transitive(value: 11_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("allocation is not an FN-4 row mismatch: {outcome:?}");
        };
        let produce = checked
            .data
            .functions
            .iter()
            .filter(|function| function.name == "produce")
            .collect::<Vec<_>>();
        assert_eq!(produce.len(), 2, "both concrete wrappers must be checked");
        let mut selected_actuals = Vec::new();
        for produce in produce {
            assert!(
                produce.allocates,
                "EFF-3 must retain each selected actual's allocation"
            );
            let [
                CheckedStatement::Return {
                    value:
                        CheckedExpression::UserCall {
                            function,
                            formal_effects: Some(effects),
                            ..
                        },
                    ..
                },
            ] = produce.body.as_deref().expect("wrapper body")
            else {
                panic!("wrapper return must retain its bound call");
            };
            assert!(
                effects.allocates,
                "the executable actual's allocation metadata must survive the formal boundary"
            );
            selected_actuals.push(
                checked
                    .data
                    .functions
                    .get(function.0 as usize)
                    .filter(|selected| selected.id == *function)
                    .expect("selected actual")
                    .name
                    .as_str(),
            );
        }
        selected_actuals.sort_unstable();
        assert_eq!(selected_actuals, ["box_new", "heap_leaf"]);
        let allocation_fact = |name| {
            checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .unwrap_or_else(|| panic!("missing {name} function"))
                .allocates
        };
        assert!(
            !allocation_fact("frame_constructions"),
            "frame constructors and conversions must not become heap allocation metadata"
        );
        assert!(
            allocation_fact("heap_leaf"),
            "a heap-taking PRE-1 row must carry allocation metadata"
        );
        assert!(
            allocation_fact("heap_transitive"),
            "ordinary callers must inherit allocation metadata transitively"
        );
        assert!(
            allocation_fact("heap_forward"),
            "forward callers must inherit allocation metadata"
        );
        assert!(
            allocation_fact("heap_cycle_left") && allocation_fact("heap_cycle_right"),
            "every member of an allocating recursive component must inherit allocation metadata"
        );
        assert!(
            !allocation_fact("frame_cycle_left") && !allocation_fact("frame_cycle_right"),
            "a nonallocating recursive component must remain nonallocating"
        );
    });

    // Removing allocation from FN-4 does not withdraw the compilation-unit
    // STOR-8 restriction on the allocating operation itself.
    assert_behavior_rule(
        r#"program no_heap;

fn main() -> status: std::process::ExitStatus pure {
  let made = box_new::<u64>(value: 7_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Stor8,
    );
}

#[test]
fn bound_calls_retain_the_formal_requirement_boundary() {
    let source = br#"interface Limited {
  fn accept(value: u64) -> result: u64 pure contract {
    requires value <= 10_u64;
  };
}

fn permissive(value: u64) -> result: u64 pure contract {
  requires value <= 11_u64;
} {
  return value;
}

binding PermissiveLimited : Limited {
  accept = permissive;
}

fn apply<interface Limited>(value: u64) -> result: u64 pure contract {
  requires value <= 10_u64;
} {
  return Limited::accept(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let result = apply::<PermissiveLimited>(value: 10_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("formal requirement boundary must check: {outcome:?}");
        };
        let actual = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "permissive")
            .expect("selected actual");
        let apply = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "apply")
            .expect("concrete wrapper");
        let [
            CheckedStatement::Return {
                value: CheckedExpression::UserCall { requirements, .. },
                ..
            },
        ] = apply.body.as_deref().expect("wrapper body")
        else {
            panic!("wrapper return must retain its bound call");
        };
        let [requirement] = requirements.as_slice() else {
            panic!("bound call must retain exactly the formal requirement");
        };
        assert_ne!(requirement.requires_clause, actual.requirements[0].clause);
        let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation: CheckedIntegerOperation::LessEqual,
                    ..
                },
            arguments,
            ..
        } = &requirement.goal.root
        else {
            panic!("formal requirement must remain its <= goal");
        };
        assert!(matches!(
            arguments.as_slice(),
            [
                _,
                GoalExpression::Datum(super::super::goal::GoalDatum::Literal(
                    CheckedValue::Integer { bits: 10, .. }
                ))
            ]
        ));
    });
}

#[test]
fn bound_calls_publish_only_the_formal_postcondition_with_fn4_fn9_authority() {
    let source = br#"interface LimitedResult {
  fn get() -> result: u64 pure contract {
    ensures result <= 10_u64;
  };
}

fn nine() -> result: u64 pure contract {
  ensures result <= 9_u64;
} {
  return 9_u64;
}

binding Nine : LimitedResult {
  get = nine;
}

fn require_ten(value: u64) -> result: unit pure contract {
  requires value <= 10_u64;
} {
  return unit;
}

fn apply<interface LimitedResult>() -> result: u64 pure {
  let value = LimitedResult::get();
  require_ten(value: value);
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = apply::<Nine>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("the formal postcondition must publish: {outcome:?}");
        };
        let main = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("entry function");
        let Some(CheckedExpression::UserCall {
            function: apply_id, ..
        }) = main.body.as_deref().and_then(|body| {
            body.iter().find_map(|statement| match statement {
                CheckedStatement::Let { value, .. } => Some(value),
                _ => None,
            })
        })
        else {
            panic!("main must call the concrete apply instance");
        };
        let apply = checked
            .data
            .functions
            .get(apply_id.0 as usize)
            .filter(|function| function.id == *apply_id && function.name == "apply")
            .expect("main's concrete apply instance");
        let authority = apply
            .entailment
            .derivations
            .nodes
            .iter()
            .find_map(|node| match node {
                super::super::entailment::DerivationNode::PostconditionCall { detail } => {
                    match &detail.summary.summary {
                        super::super::entailment::RelationProvenance::FormalBoundary {
                            query,
                            actual,
                            premises,
                        } => Some((*query, *actual, premises)),
                        super::super::entailment::RelationProvenance::Verified(_) => None,
                    }
                }
                _ => None,
            })
            .expect("the requirement proof must use the formal S12 relation");
        let query = checked
            .data
            .contract_queries
            .get(authority.0.0 as usize)
            .expect("formal boundary query");
        assert_eq!(query.instance, Some(apply.id));
        assert_eq!(query.premises.len(), 1);
        assert_eq!(authority.2.len(), 1);
        assert_eq!(authority.1, authority.2[0].function);
    });

    // The actual's tighter `<= 9` relation proves the formal `<= 10`
    // promise, but FN-5 does not expose that stronger implementation detail
    // to the generic caller.
    assert_behavior_rule(
        &String::from_utf8(source.to_vec())
            .expect("source text")
            .replace("requires value <= 10_u64;", "requires value <= 9_u64;"),
        SemanticRule::Fn8,
    );
}

#[test]
fn a_bound_routed_relation_follows_a_named_outcome_to_its_selected_arm() {
    let source = r#"interface RoutedIdentity {
  fn choose(value: i32) -> outcome: Result<i32, u8> pure contract {
    ensures when Ok(value: selected): selected == value;
  };
}

fn choose(value: i32) -> outcome: Result<i32, u8> pure contract {
  ensures when Ok(value: selected): selected == value;
} {
  return Ok<i32, u8>(value: value);
}

binding RoutedChoice : RoutedIdentity {
  choose = choose;
}

fn require_same(left: i32, right: i32) -> result: unit pure contract {
  requires left == right;
} {
  return unit;
}

fn apply<interface RoutedIdentity>(value: i32) -> result: unit pure {
  match RoutedIdentity::choose(value: value) {
    Ok(value: selected) => {
      require_same(left: selected, right: value);
    }
    Err(error: problem) => {
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  apply::<RoutedChoice>(value: 7_i32);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "the formal routed relation must reach its direct selected arm: {outcome:?}"
        );
    });

    // The formal boundary supplies conditional evidence at the call; naming
    // the outcome preserves it without strengthening the supplied contract.
    let indirect = source.replace(
        "  match RoutedIdentity::choose(value: value) {",
        "  let outcome = RoutedIdentity::choose(value: value);\n  match outcome {",
    );
    with_semantics(indirect.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

/// A bound call cannot form a recursive postcondition component with its
/// selected concrete actual: returning from that actual to the generic
/// wrapper replaces the wrapper's function argument instead of forwarding
/// its complete template vector. FN-6 rejects that source cycle before FN-9,
/// so this fixture cannot use formal-boundary publication to bootstrap the
/// selected actual's same-component summary.
#[test]
fn a_recursive_bound_call_stops_at_fn6_before_summary_publication() {
    let source = r#"interface Identity {
  fn get(value: i32) -> result: i32 pure contract {
    ensures result == value;
  };
}

fn apply<interface Identity>(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let selected = Identity::get(value: value);
  return selected;
}

fn actual(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  cycle(value: value);
  return value;
}

binding Selected : Identity {
  get = actual;
}

fn cycle(value: i32) -> result: unit pure {
  let ignored = apply::<Selected>(value: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let ignored = apply::<Selected>(value: 1_i32);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_behavior_rule(source, SemanticRule::Fn6);
}

#[test]
fn raw_function_binders_are_visible_and_emit_no_symbolic_hypotheses() {
    let source = br#"fn zero() -> result: u64 pure {
  return 0_u64;
}

fn apply<fn get() -> result: u64 pure>() -> result: u64 pure {
  return get();
}

fn main() -> status: std::process::ExitStatus pure {
  let value = apply::<fn zero>();
  let other = apply::<fn zero>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("{outcome:?}");
        };
        assert!(
            checked
                .data
                .functions
                .iter()
                .all(|function| !function.formal_hypothesis)
        );
        let lowered = lower_checked(*checked, OverlapLowering::Off)
            .expect("raw function substitution lowers directly");
        assert_eq!(
            lowered
                .functions()
                .iter()
                .filter(|function| !function.blocks().is_empty())
                .count(),
            3
        );
    });
}

#[test]
fn a_nominal_only_function_argument_must_match_its_signature() {
    let source = r#"struct Holder<fn pick(value: u64) -> result: u64 pure> {
  marker: u64;
}

fn bad(value: Bool) -> result: Bool pure {
  return value;
}

fn inspect(value: &Holder<fn bad>) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_behavior_site(source, SemanticRule::Fn4, "fn bad");
    with_semantics(source.replace(": Bool", ": u64").as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

#[test]
fn called_raw_function_mismatches_point_at_the_written_argument() {
    let source = r#"fn bad(value: Bool) -> result: Bool pure {
  return value;
}

fn apply<fn pick(value: u64) -> result: u64 pure>(value: u64) -> result: u64 pure {
  return pick(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let result = apply::<fn bad>(value: 7_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_behavior_site(source, SemanticRule::Fn4, "fn bad");
}

// Retired with the region parameter of [FN-2, FORM-8]: v0.59's
// `member_regions_are_instantiated_per_call_and_never_on_the_formal_header`
// instantiated a formal member's `['r]` at each call and refused a region
// list on the formal header. v0.60 has no region parameter and no `Slice`
// type, so a formal member has no per-call region left to instantiate; the
// surviving FN-4 signature check over reference parameters is
// `formal_row_comparison_uses_parameter_ordinals_not_binder_spellings` and
// `formal_range_reference_parameters_compare_by_ordinal` below.

#[test]
fn a_bound_call_cannot_drop_part_of_a_vector_on_a_written_cycle() {
    let source = r#"fn stop() -> result: unit pure {
  return unit;
}

fn first<fn work() -> result: unit pure>() -> result: unit pure {
  return work();
}

fn second<fn work() -> result: unit pure>() -> result: unit pure {
  return first::<fn work>();
}

fn main() -> status: std::process::ExitStatus pure {
  first::<fn second::<fn stop>>();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("expected FN-6 rejection");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn6);
        assert!(format!("{:?}", issue.kind()).contains("first -> second -> first"));
    });
    let acyclic = source.replace("return first::<fn work>();", "return work();");
    with_semantics(acyclic.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "acyclic composed calls must remain admitted"
        );
    });
}

#[test]
fn actual_expansion_cycles_include_member_function_arguments() {
    let source = r#"interface Work {
  fn run() -> result: unit pure;
}

binding Recursive : Work {
  run = drive::<Recursive>;
}

fn drive<interface Work>() -> result: unit pure {
  Work::run();
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Fn3);
        assert!(format!("{:?}", issue.kind()).contains("Recursive -> Recursive"));
    });
}

#[test]
fn qualified_actual_forwarding_remains_an_acyclic_abbreviation() {
    let source = r#"interface Factory {
  fn make() -> result: u64 pure;
}

fn zero() -> result: u64 pure {
  return 0_u64;
}

binding First : Factory {
  make = zero;
}

binding Second : Factory {
  make = First::make;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(
            checked
                .data
                .functions
                .iter()
                .filter(|function| function.body.is_some())
                .count(),
            2
        );
        assert!(
            checked
                .data
                .functions
                .iter()
                .all(|function| !function.formal_hypothesis)
        );
    });
    // A self-reference is already visible at the declaration point, so this
    // reaches abbreviation-cycle checking rather than a forward-use error.
    assert_behavior_rule(
        &source.replace("make = zero;", "make = First::make;"),
        SemanticRule::Fn3,
    );
}

#[test]
fn actual_member_aliases_preserve_the_complete_instantiation_cycle() {
    let source = r#"interface Work {
  fn run() -> result: u64 pure;
}

binding First : Work {
  run = trampoline;
}

binding Alias : Work {
  run = First::run;
}

fn poly<T: drop>() -> result: u64 pure {
  return invoke::<Alias>();
}

fn trampoline() -> result: u64 pure {
  return poly::<u64>();
}

fn invoke<interface Work>() -> result: u64 pure {
  return Work::run();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for program in [
        source.to_owned(),
        source.replace("invoke::<Alias>()", "invoke::<First>()"),
    ] {
        with_semantics(program.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("{outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Fn6);
            let SemanticIssueKind::PolymorphicRecursion { cycle, .. } = issue.kind() else {
                panic!("{issue:?}");
            };
            assert!(
                cycle.contains("poly") && cycle.contains("trampoline"),
                "{cycle}"
            );
        });
    }
}

#[test]
fn instantiation_forwards_all_kinds_and_refuses_constructed_cycles_on_both_paths() {
    let forward = br#"fn task() -> result: unit pure {
  return unit;
}

fn repeat<T: copy, const n: u64, fn work() -> result: unit pure>(value: T) -> result: T pure {
  work();
  return repeat::<T, n, fn work>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let result = repeat::<u64, 1, fn task>(value: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(forward, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("{outcome:?}");
        };
        lower_checked(*checked, OverlapLowering::Off)
            .expect("the unchanged vector has a finite direct-call instance graph");
    });
    let wrapped = r#"fn nested<fn work() -> result: unit pure>() -> result: unit pure {
  return nested::<fn nested::<fn work>>();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_behavior_rule(wrapped, SemanticRule::Fn6);
    let growing = r#"struct Grow<T: drop> {
  next: Box<Grow<Box<T>>>;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let selector = r#"fn relation(value: u64) -> result: u64 pure contract {
  ensures result == value;
} {
  return value;
}

"#;
    // Resolution's FN-9 selector preflight and ordinary complete-unit
    // checking must both refuse before materializing Grow<Box<...>>.
    for source in [growing.to_owned(), format!("{selector}{growing}")] {
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("{outcome:?}");
            };
            assert_eq!(issue.rule(), SemanticRule::Fn6);
            let SemanticIssueKind::PolymorphicRecursion { cycle, .. } = issue.kind() else {
                panic!("{issue:?}");
            };
            assert_eq!(cycle, "Grow -> Grow");
        });
    }
}

fn assert_issue_slice(source: &[u8], rule: SemanticRule, kind: SemanticIssueKind, expected: &[u8]) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("expected {rule:?}/{kind:?}, got {outcome:?}");
        };
        assert_eq!(issue.rule(), rule);
        assert_eq!(issue.kind(), &kind);
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("test offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("test offset fits");
        assert_eq!(&source[start..end], expected);
    });
}

#[test]
fn missing_entry_diagnostic_salvage_checks_instantiation_before_discovery() {
    let source = "struct Grow<T: drop> {\n  next: Box<Grow<Box<T>>>;\n}\n";
    assert_behavior_rule(source, SemanticRule::Fn6);
    let source = "fn repeat<T: drop>(value: T) -> result: unit pure {\n  let boxed = box_new::<T>(value: move value);\n  repeat::<Box<T>>(value: move boxed);\n  return unit;\n}\n";
    assert_behavior_rule(source, SemanticRule::Fn6);
}

#[test]
fn static_group_bindings_have_no_executable_metadata() {
    let source = br#"interface Zeroed {
  fn zero() -> result: i32 pure;
}

fn make_zero() -> result: i32 pure {
  return 0_i32;
}

binding Zero : Zeroed {
  zero = make_zero;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("valid static conformance must check: {outcome:?}");
        };
        // D7 replaces standalone conformance metadata with checked group
        // expansion. No symbolic function hypothesis reaches executable IR.
        assert!(
            checked
                .data
                .functions
                .iter()
                .all(|function| !function.formal_hypothesis)
        );
        assert_eq!(
            checked
                .data
                .functions
                .iter()
                .filter(|function| function.name == "make_zero")
                .count(),
            1
        );

        // The bound function is still present exactly once in the ordinary
        // function table. Contract metadata contributes no executable function.
        assert_eq!(
            checked
                .data
                .functions
                .iter()
                .filter(|function| function.body.is_some())
                .count(),
            2
        );
        let lowered = lower_checked(*checked, OverlapLowering::Off)
            .expect("static contract metadata must not alter ordinary lowering");
        assert_eq!(
            lowered
                .functions()
                .iter()
                .filter(|function| !function.blocks().is_empty())
                .count(),
            2
        );
    });
}

#[test]
fn empty_formal_and_actual_groups_are_valid() {
    let source = br#"interface Marker {
}

binding Empty : Marker {
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("empty marker conformance must check: {outcome:?}");
        };
        assert_eq!(
            checked
                .data
                .functions
                .iter()
                .filter(|function| function.body.is_some())
                .count(),
            1
        );
        assert!(!checked.data.functions[0].formal_hypothesis);
    });
}

#[test]
fn actual_header_materializes_its_only_generic_nominal_instance() {
    let source = br#"struct Wrapper<T: drop> {
  value: T;
}

interface Marker<T: drop> {
}

binding Wrapped : Marker<Wrapper<i32>> {
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("contract-only generic subject must check: {outcome:?}");
        };
        assert!(
            checked
                .data
                .nominals
                .iter()
                .any(|nominal| nominal.name.starts_with("Wrapper<"))
        );
        let semantic_nominal_count = checked.data.nominals.len();
        let lowered = lower_checked(*checked, OverlapLowering::Off)
            .expect("conformance-only nominal metadata must not affect ordinary lowering");
        // FN-3 expansion leaves only concrete nominal instances here; the
        // retired conformance-subject placeholder is no longer in this table.
        assert_eq!(lowered.nominals().len(), semantic_nominal_count);
    });
}

#[test]
fn formal_member_materializes_its_only_generic_nominal_instance() {
    let source = br#"struct Wrapper<T: drop> {
  value: T;
}

interface Factory {
  fn make() -> result: Wrapper<i32> pure;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("contract-only generic member type must check: {outcome:?}");
        };
        assert!(
            checked
                .data
                .nominals
                .iter()
                .any(|nominal| nominal.name.starts_with("Wrapper<"))
        );
        let semantic_nominal_count = checked.data.nominals.len();
        let lowered = lower_checked(*checked, OverlapLowering::Off)
            .expect("contract-only nominal metadata must not affect ordinary lowering");
        // A formal signature contributes its concrete type, not a separate
        // contract-subject placeholder that lowering would have to discard.
        assert_eq!(lowered.nominals().len(), semantic_nominal_count);
    });
}

#[test]
fn formal_nominal_inventory_keeps_only_concrete_types_and_result_lists() {
    let source = r#"struct Envelope<T> {
  payload: T;
}

interface Reader<T> {
  fn read(value: &Envelope<T>) -> result: u64 reads(value);
}

interface Factory {
  fn make() -> (wrapped: Envelope<i32>, optional: Option<u8>) pure;
}

fn unused<T, fn read(value: &Envelope<T>) -> (wrapped: T, optional: u64) reads(value)>() -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for mode in [OverlapLowering::Off, OverlapLowering::On] {
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("unused symbolic formals must check: {outcome:?}");
            };
            let executable = &checked.data.nominals[..checked.data.executable_nominal_count];
            let wrappers = executable
                .iter()
                .filter(|nominal| nominal.name.starts_with("Envelope<"))
                .collect::<Vec<_>>();
            let [wrapper] = wrappers.as_slice() else {
                panic!("only the concrete formal-only Envelope<i32> must remain");
            };
            let CheckedNominalKind::Struct { fields } = &wrapper.kind else {
                panic!("Envelope is a struct");
            };
            assert_eq!(fields[0].ty, CheckedType::Integer(IntegerType::I32));
            let lists = executable
                .iter()
                .filter(|nominal| nominal.name.starts_with("(wrapped:"))
                .collect::<Vec<_>>();
            let [list] = lists.as_slice() else {
                panic!("only the concrete formal-only result list must remain");
            };
            let CheckedNominalKind::Struct { fields } = &list.kind else {
                panic!("a result list is a struct");
            };
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "wrapped");
            assert_eq!(fields[0].ty, CheckedType::Nominal(wrapper.id));
            assert_eq!(fields[1].name, "optional");
            let CheckedType::Nominal(optional) = fields[1].ty else {
                panic!("the second result retains its Option type");
            };
            assert!(
                checked.data.nominals[optional.0 as usize]
                    .name
                    .starts_with("Option<")
            );
            lower_checked(*checked, mode)
                .expect("symbolic formal types must not reach executable lowering");
        });
    }

    // Scratch rollback cannot bypass formation of an unused formal's contract.
    assert_behavior_rule(
        &source.replace(
            "-> result: u64 reads(value);",
            "-> result: u64 reads(value) contract {\n    requires deref(entry(value)).payload == deref(entry(value)).payload;\n  };",
        ),
        SemanticRule::Msr3,
    );
}

#[test]
fn nominal_formal_contract_queries_survive_scratch_rollback() {
    let source = br#"struct Envelope<T: copy> {
  tag: u64;
  payload: T;
}

interface Reader<T: copy> {
  fn read(value: &Envelope<T>) -> result: u64 reads(value) contract {
    requires deref(value).tag <= 99_u64;
    ensures result == deref(value).tag;
  };
}

interface Factory {
  fn make() -> result: Box<Slots<Envelope<i32>>> pure;
}

fn read_tag(input: &Envelope<u64>) -> output: u64 reads(input.tag) contract {
  requires deref(input).tag <= 99_u64;
  ensures output == deref(input).tag;
} {
  return deref(input).tag;
}

binding ReadU64 : Reader<u64> {
  read = read_tag;
}

fn invoke<interface Reader<T>>(value: &Envelope<T>) -> result: u64 reads(value) contract {
  requires deref(value).tag <= 99_u64;
  ensures result == deref(value).tag;
} {
  let answer = Reader::read(value: value);
  return answer;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = Envelope<u64>(tag: 7_u64, payload: 8_u64);
  if value.tag <= 99_u64 {
    let tag = invoke::<ReadU64>(value: &value);
    if tag == 7_u64 {
      return std::process::exit_status(code: 0_u8);
    }
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    for mode in [OverlapLowering::Off, OverlapLowering::On] {
        with_semantics(source, |outcome| {
            let SemanticOutcome::Complete(checked) = outcome else {
                panic!("concrete nominal contracts must survive reification: {outcome:?}");
            };
            assert!(!checked.data.contract_queries.is_empty());
            assert!(
                checked
                    .data
                    .contract_queries
                    .iter()
                    .any(|query| query.instance.is_some())
            );
            for query in &checked.data.contract_queries {
                super::entailment::validate_derivations(&query.proof);
            }
            lower_checked(*checked, mode)
                .expect("concrete signatures and contract identities must remain valid");
        });
    }
}

#[test]
fn retired_owned_law_identity_syntax_is_not_admitted() {
    let source = br#"contract InvalidIdentity {
  fn combine() -> result: Slots<u8, 1> pure;
  law identity(combine, zero);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    // D7 removes law syntax, including the formerly invalid owned identity.
    assert_parse_rule(source, crate::SyntaxRule::Gram2);
}

#[test]
fn migrated_group_rejections_keep_their_selected_rules() {
    assert_rule(
        include_bytes!("../../../../tests/conformance/cases/fn3-neg-missing-binding.wf"),
        SemanticRule::Fn3,
        SemanticIssueKind::type_mismatch(
            "a binding group binds every interface member exactly once in declared order",
            "a nonmatching behavior argument",
        ),
    );
    // D7 assigns interface compatibility to FN-4. The prior implicit
    // type/conformance uniqueness test is now the two-explicit-actuals
    // positive; it cannot remain a negative under the selected design.
    for source in [
        include_str!("../../../../tests/conformance/cases/fn4-neg-requires-mismatch.wf"),
        include_str!("../../../../tests/conformance/cases/fn4-neg-formal-row-coverage.wf"),
    ] {
        assert_behavior_rule(source, SemanticRule::Fn4);
    }
}

#[test]
fn retired_closed_law_table_has_no_remaining_acceptance_path() {
    // The owner's D7 removes law/law_arg entirely. Retain every old source:
    // formerly discharged and undischarged rows must both stop at grammar,
    // rather than leave a hidden theorem/metadata acceptance path behind.
    for source in [
        br#"contract Semigroup {
  doc "A law uses a semantic name from the closed {associative, commutative, identity} table [FN-4].";
  fn combine(x: i32, y: i32) -> result: i32 pure;
  law associative(combine);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#.as_slice(),
        br#"contract BadLaw {
  fn combine(x: i32, y: i32) -> result: i32 pure;
  law distributive(combine, combine);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#.as_slice(),
        br#"contract BadMonoid {
  doc "FN-4: signed saturating add is NOT associative ((MAX sat+ 1) sat+ -1 != MAX sat+ (1 sat+ -1)); the stated law must be refuted, not trusted.";
  fn combine(x: i64, y: i64) -> result: i64 pure;
  law associative(combine);
}

fn satadd_signed(x: i64, y: i64) -> result: i64 pure {
  return x +sat y;
}

conform i64: BadMonoid {
  combine = satadd_signed;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#.as_slice(),
        br#"contract OpaqueMonoid {
  doc "FN-4: a stated law on a fn whose body is not a single table op has no static proof; stated-but-unchecked is a hard reject, never a trusted fact.";
  fn combine(x: u64, y: u64) -> result: u64 pure;
  law associative(combine);
}

fn twostep(x: u64, y: u64) -> result: u64 pure {
  let t = x +wrap y;
  return t +wrap 1_u64;
}

conform u64: OpaqueMonoid {
  combine = twostep;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#.as_slice(),
        br#"contract SatMonoid {
  doc "FN-4 stated-and-checked: the exact direct +sat body and closed unsigned table cells discharge these laws.";
  fn combine(x: u64, y: u64) -> result: u64 pure;
  law associative(combine);
  law commutative(combine);
  law identity(combine, 0_u64);
}

fn satadd(x: u64, y: u64) -> result: u64 pure {
  return x +sat y;
}

conform u64: SatMonoid {
  combine = satadd;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#.as_slice(),
    ] {
        assert_parse_rule(source, crate::SyntaxRule::Gram2);
    }
}

#[test]
fn retired_law_identity_with_wrong_literal_type_is_a_grammar_error() {
    let source = br#"contract BadIdentity {
  fn combine(x: u64, y: u64) -> result: u64 pure;
  law identity(combine, unit);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_parse_rule(source, crate::SyntaxRule::Gram2);
}

#[test]
fn retired_law_identity_with_an_earlier_constant_is_a_grammar_error() {
    let source = br#"const zero: u64 = 0_u64;

contract AddIdentity {
  fn combine(x: u64, y: u64) -> result: u64 pure;
  law identity(combine, zero);
}

fn saturating_add(x: u64, y: u64) -> result: u64 pure {
  return x +sat y;
}

conform u64: AddIdentity {
  combine = saturating_add;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_parse_rule(source, crate::SyntaxRule::Gram2);
}

#[test]
fn repeated_member_points_at_the_later_signature() {
    let source = br#"interface Repeated {
  fn value() -> result: i32 pure;
  fn value() -> result: i32 pure;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_issue_slice(
        source,
        SemanticRule::Fn3,
        SemanticIssueKind::type_mismatch(
            "each interface member name occurs once",
            "a nonmatching behavior argument",
        ),
        b"fn value() -> result: i32 pure",
    );
}

#[test]
fn retired_numeric_conformance_spelling_is_a_grammar_error() {
    let source = br#"conform i32: Int {
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    // Int is a built-in numeric bound; D7 has no conformance declaration.
    // The corresponding formal-domain mismatch is covered in resolution.
    assert_parse_rule(source, crate::SyntaxRule::Form1);
}

#[test]
fn actual_header_arguments_match_the_formal_header_arity() {
    let source = br#"interface Plain {
}

binding Invalid : Plain<i32> {
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_issue_slice(
        source,
        SemanticRule::Fn3,
        SemanticIssueKind::type_mismatch(
            "0 written expanded generic arguments",
            "1 written expanded generic argument",
        ),
        b"Plain<i32>",
    );
}

#[test]
fn incompatible_and_out_of_order_bindings_point_at_the_fn_bind() {
    let source = br#"interface Pair {
  fn first() -> result: i32 pure;
  fn second() -> result: i32 pure;
}

fn make_first() -> result: i32 pure {
  return 1_i32;
}

fn make_second() -> result: i32 pure {
  return 2_i32;
}

binding Reversed : Pair {
  second = make_second;
  first = make_first;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_issue_slice(
        source,
        SemanticRule::Fn3,
        SemanticIssueKind::type_mismatch(
            "binding member names follow the interface's declared order",
            "a nonmatching behavior argument",
        ),
        b"second = make_second;",
    );
}

#[test]
fn missing_binding_points_at_the_complete_actual_declaration() {
    let source = br#"interface Pair {
  fn first() -> result: i32 pure;
  fn second() -> result: i32 pure;
}

fn make_first() -> result: i32 pure {
  return 1_i32;
}

binding Incomplete : Pair {
  first = make_first;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_issue_slice(
        source,
        SemanticRule::Fn3,
        SemanticIssueKind::type_mismatch(
            "a binding group binds every interface member exactly once in declared order",
            "a nonmatching behavior argument",
        ),
        b"binding Incomplete : Pair {\n  first = make_first;\n}",
    );
}

#[test]
fn retired_source_contract_bound_is_a_grammar_error() {
    let source = br#"contract Marker {
}

fn generic<T: Marker>() -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    // D7 removes this type-attached contract mechanism. A formal group in
    // the numeric-bound position is separately refused by name resolution.
    assert_parse_rule(source, crate::SyntaxRule::Gram2);
}

#[test]
fn formal_row_comparison_uses_parameter_ordinals_not_binder_spellings() {
    // v0.59 compared positional regions here. [FN-4] still normalizes the two
    // rows by parameter ordinal before comparing them, and [EFF-1] now writes
    // one path per entry, so the formal's `x, y` and the actual's
    // `first, second` are the same row.
    let source = br#"interface LengthSum {
  fn sum(x: &Slots<u8, 4>, y: &Slots<u8, 4>) -> result: u64 reads(x), reads(y);
}

fn add_lengths(first: &Slots<u8, 4>, second: &Slots<u8, 4>) -> result: u64 reads(first), reads(second) {
  let first_length = deref(first).len;
  let second_length = deref(second).len;
  return first_length +wrap second_length;
}

binding Sum : LengthSum {
  sum = add_lengths;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("ordinal-normalized rows must check: {outcome:?}");
        };
        assert!(
            checked
                .data
                .functions
                .iter()
                .all(|function| !function.formal_hypothesis)
        );
    });
}

#[test]
fn formal_range_reference_parameters_compare_by_ordinal() {
    // v0.59 wrote this operand as `own Slice<u8>`. `&[T]` is a reference kind
    // admitted only in parameter position [TYPE-8, REF-4]; the FN-4 ordinal
    // comparison over it is unchanged.
    let source = br#"interface ByteReader {
  fn first(values: &[u8]) -> result: u8 reads(values);
}

fn read_first(bytes: &[u8]) -> result: u8 reads(bytes) {
  let spare = deref(bytes).len;
  let ok = 0_u64 < spare;
  if ok {
    return deref(bytes)[0_u64];
  } else {
    return 0_u8;
  }
}

binding Bytes : ByteReader {
  first = read_first;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("range-reference parameters must compare by ordinal: {outcome:?}");
        };
        assert!(
            checked
                .data
                .functions
                .iter()
                .all(|function| !function.formal_hypothesis)
        );
    });
}

// Retired with the slice return ceiling of [VIEW-6] and the borrow-mode
// result of [FN-1]: v0.59's `formal_slice_results_share_function_signature_
// formation` read `CheckedFunction::slice_return_ceiling` for an `own
// Slice<'r, u8>` result and refused a `&uniq 'descriptor Slice<..>` result
// with `BorrowedSliceResult`. v0.60 has no slice type and no borrow-mode
// result: `rtype := "own" type` leaves nothing for a signature to return but
// an owned value, and [REF-3]'s `EscapingReference` is the successor refusal
// for a body that tries to return a reference, kept as the conformance case
// `ref3-neg-returned-reference`.
