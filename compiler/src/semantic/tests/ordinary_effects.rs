//! Ordinary storage-place effects, ownership transfer and window proofs.
//!
//! Every effect path is rooted at a reference parameter [EFF-1]; a by-value
//! parameter has no effect entry at all, and allocation and release carry
//! none either [STOR-8]. These tests retain the ordinary source assertions
//! with the v0.60 spellings.

use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::{assert_rule_kind, with_semantics};

fn assert_complete(source: &[u8]) {
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "ordinary source must check: {outcome:?}"
        );
    });
}

#[test]
fn memory_reclamation_contributes_no_release_row() {
    assert_complete(
        b"fn consume(data: Box<u64>) -> result: unit pure {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
    );
    assert_complete(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let boxed = box_new::<u64>(value: 0_u64);\n  let stored = box_array_filled::<u8>(count: 4_u64, value: 0_u8);\n  return std::process::exit_status(code: 0_u8);\n}\n",
    );
}

#[test]
fn an_effect_repair_keeps_box_contents_and_nested_field_names() {
    assert_rule_kind(
        br#"struct Inner {
  len: u64;
}

struct Outer {
  next: Inner;
}

fn read_nested(cell: &Box<Outer>) -> result: u64 pure {
  return deref(cell).inner.next.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Eff2,
        |kind| {
            matches!(kind, SemanticIssueKind::EffectMismatch { missing, .. }
                if missing == &["reads(cell.inner.next.len)"])
        },
    );
}

/// [WIN-3, OP-14] a linear window has no compiler-derived release at all,
/// proved empty or not.
///
/// v0.59 gave a run proved empty an element-free derived release, so a
/// `requires run.len == 0_u64` made the scope exit legal. [WIN-3] retires it:
/// "No operation releases a linear element: a storage whose element type is
/// linear is itself linear [PROV-6] and the program must take every element
/// out and consume it, and then, with the storage proved empty, call
/// `free_empty` [OP-14]." The zero-length fact is what `free_empty`'s own
/// requirement reads; it is not a licence for the edge. The corpus states the
/// same verdict in
/// `prov6-neg-a-proved-empty-run-still-needs-its-backing-provider`, whose
/// source is this first program with a symbolic element type.
#[test]
fn a_linear_window_reaches_no_scope_exit_however_short_it_is_proved() {
    assert_rule_kind(
        br#"nodrop struct Token {
  value: u64;
}

fn release(run: Slots<Token, 4>) -> result: unit pure contract {
  requires run.len == 0_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Prov6,
        |kind| matches!(kind, SemanticIssueKind::LinearValueNotConsumed { .. }),
    );
    // An affine element type keeps its ordinary derived release on the same
    // edge, symbolic capacity included [STOR-3].
    assert_complete(
        br#"fn release<const n: u64>(run: Slots<u64, n>) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert_rule_kind(
        br#"nodrop struct Token {
  value: u64;
}

fn release(run: Slots<Token, 4>) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Prov6,
        |kind| matches!(kind, SemanticIssueKind::LinearValueNotConsumed { .. }),
    );
}

#[test]
fn a_scope_exit_cannot_discard_a_symbolically_linear_member() {
    // v0.59 wrote this as `dispose slot;` and refused it with
    // `DisposeOfLinearNode`. v0.60 has no `dispose`; the surviving refusal is
    // [PROV-6]'s own: a value linear in this scope that is live on an edge
    // leaving it has no compiler-derived release to carry it.
    assert_rule_kind(
        br#"nodrop struct Token {
  value: u64;
}

enum Slot {
  Vacant();
  Occupied(value: Token, owner: Box<u64>);
}

fn discard(slot: Slot) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Prov6,
        |kind| matches!(kind, SemanticIssueKind::LinearValueNotConsumed { .. }),
    );
}

#[test]
fn a_partial_consume_cannot_abandon_a_symbolically_linear_member() {
    assert_rule_kind(
        br#"struct Carrier<T> {
  must_consume: T;
  returned: Box<u64>;
}

fn take_returned<T>(carrier: Carrier<T>) -> result: Box<u64> pure {
  return move carrier.returned;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Prov6,
        |kind| matches!(kind, SemanticIssueKind::LinearValuePartiallyConsumed { .. }),
    );
}

// Retired with the store provider of [PROV-1]: v0.59's
// `a_partial_consume_cannot_abandon_storage_without_its_provider` held a
// `Vector<'s, u8>` whose release spent a store no scope binding provided, and
// v0.60 has one heap whose allocation and release carry no capability and no
// effect entry [STOR-8]; the residual-abandonment half survives above as
// [PROV-6]'s linear partial consume.

#[test]
fn live_effect_categories_keep_eff1_canonical_order_and_multiplicity() {
    super::assert_parse_rule(
        b"fn probe(file: &std::fs::ReadFile) -> result: unit pure, writes(file) {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        crate::SyntaxRule::Gram2,
    );
    assert_rule_kind(
        b"fn probe(file: &std::fs::ReadFile) -> result: unit writes(file), writes(file) {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Eff1,
        |kind| matches!(kind, SemanticIssueKind::InvalidEffectRow { .. }),
    );
    assert_rule_kind(
        b"fn probe(file: &std::fs::ReadFile) -> result: unit writes(file), reads(file) {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Eff1,
        |kind| matches!(kind, SemanticIssueKind::InvalidEffectRow { .. }),
    );
}

#[test]
fn a_by_value_parameter_is_no_effect_root_and_a_reference_one_must_be_exhibited() {
    // v0.59 admitted the path and asked EFF-2 whether the body exhibited it.
    // [EFF-1] now roots every path at a reference parameter of the same
    // callable, so a by-value root is refused at the row itself.
    assert_rule_kind(
        b"fn probe(value: u64) -> result: unit reads(value) {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Eff1,
        |kind| matches!(kind, SemanticIssueKind::InvalidEffectRow { .. }),
    );
    assert_rule_kind(
        b"fn probe(value: &u64) -> result: unit reads(value) {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Eff2,
        |kind| matches!(kind, SemanticIssueKind::EffectMismatch { .. }),
    );
}

#[test]
fn external_and_blocks_are_ordinary_function_and_parameter_names() {
    assert_complete(
        b"fn external(blocks: &std::text::Args) -> result: u64 reads(blocks) {\n  let total = std::text::args_count(args: blocks);\n  return total;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
    );
}

#[test]
fn referencing_one_struct_field_projects_only_that_field_effect() {
    assert_complete(
        br#"struct Pair {
  first: Slots<u8, 4>;
  second: Slots<u8, 4>;
}

fn length(value: &Slots<u8, 4>) -> result: u64 reads(value) {
  return deref(value).len;
}

fn read_second(pair: &Pair) -> result: unit reads(pair.second) {
  let count = length(value: &deref(pair).second);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
}

#[test]
fn an_affine_bounded_own_exchange_supports_a_copy_instantiation() {
    // v0.59 wrote the exchange as `replace target = move incoming;` over an
    // `own` parameter with a `reads(target), writes(target)` row. An `own`
    // parameter has no effect entry [EFF-1] and there is no `replace`
    // statement, so the exchange is two ordinary moves and the row is `pure`.
    let source =
        br#"fn exchange_owned<T: drop>(target: T, incoming: T) -> (current: T, previous: T) pure {
  let previous = move target;
  let current = move incoming;
  return move current, move previous;
}

fn main() -> status: std::process::ExitStatus pure {
  let (current, previous) = exchange_owned::<u64>(target: 1_u64, incoming: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn an_uncalled_recursive_owner_transfer_needs_no_routing_summary() {
    let source = br#"fn unclosed(value: Box<u64>) -> result: Box<u64> pure {
  let next = unclosed(value: move value);
  return move next;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn a_nonreturning_helper_keeps_its_structural_read_effect() {
    let source = r#"fn unclosed(value: &u64) -> result: u64 reads(value) {
  let observed = deref(value);
  let next = unclosed(value: value);
  return next;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source.as_bytes());
    let omitted = source.replace("reads(value)", "pure");
    assert_rule_kind(omitted.as_bytes(), SemanticRule::Eff2, |kind| {
        matches!(kind, SemanticIssueKind::EffectMismatch { missing, .. }
            if missing.iter().any(|effect| effect == "reads(value)"))
    });
}

#[test]
fn an_assignment_through_a_cell_uses_only_target_storage_effects() {
    let source = r#"fn exchange(target: &Box<u64>, incoming: Box<u64>) -> result: unit writes(target) {
  set deref(target) = move incoming;
  return unit;
}

fn observe(owner: &Box<u64>, spare: &Box<u64>, incoming: Box<u64>) -> result: unit writes(owner) {
  exchange(target: owner, incoming: move incoming);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source.as_bytes());
    let spurious = source.replace("writes(owner)", "reads(spare), writes(owner)");
    assert_rule_kind(spurious.as_bytes(), SemanticRule::Eff2, |kind| {
        matches!(kind, SemanticIssueKind::EffectMismatch { extra, .. }
            if extra.iter().any(|effect| effect == "reads(spare)"))
    });
}

#[test]
fn a_full_window_conversion_keeps_its_inherent_capacity() {
    // v0.59 froze a always-full `FixedVector` into an `array` through the
    // kernel row `array_from_fixed`, whose fullness demand was a [BLK-0]
    // requirement. [OP-13]'s `slots_into_array` is an ordinary [PRE-1] record
    // carrying `requires values.len == n`, discharged under [FN-8].
    assert_complete(
        br#"fn convert(values: Slots<u64, 0>) -> result: Array<u64, 0> pure {
  return slots_into_array::<u64, 0>(values: move values);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert_rule_kind(
        br#"fn convert(values: Slots<u64, 1>) -> result: Array<u64, 1> pure {
  return slots_into_array::<u64, 1>(values: move values);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Fn8,
        |kind| matches!(kind, SemanticIssueKind::UndischargedCallRequirement(_)),
    );
    assert_complete(
        br#"fn convert(values: Slots<u64, 1>) -> result: Array<u64, 1> pure contract {
  requires values.len == 1_u64;
} {
  return slots_into_array::<u64, 1>(values: move values);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
}

#[test]
fn owning_sparse_rehash_keeps_ordinary_loop_proofs() {
    let source = r#"struct Resource {
  id: u64;
  data0: u64;
  data1: u64;
}

enum Slot {
  Vacant();
  Deleted();
  Occupied(fingerprint: u8, key: u64, payload: Box<Resource>);
}

struct Progress {
  source_next: u64;
  next_probe: u64;
  phase: u8;
}

fn occupied(slot: &Slot) -> full: Bool reads(slot) {
  match deref(slot) {
    Occupied(fingerprint: tag, key: present, payload: owner) => {
      return True();
    }
    Vacant() => {
      return False();
    }
    Deleted() => {
      return False();
    }
  }
}

fn probe_index(home: u64, step: u64, count: u64) -> index: u64 pure contract {
  requires home < count;
  requires step < count;
  ensures index < count;
} {
  let until_end = count - home;
  if step >= until_end {
    let wrapped = step - until_end;
    return wrapped;
  }
  let straight = home + step;
  return straight;
}

fn advance(source: &Array<Slot, 4>, progress: &Progress, budget: u64) -> examined: u64 reads(source), writes(progress) {
  let inspected = 0_u64;
  if deref(progress).phase < 2_u8 {
    for (
      tick in 0_u64..budget,
      invariant work_min: inspected >= tick,
      invariant work_max: inspected <= tick
    ) {
      let count = deref(source).len;
      if deref(progress).source_next >= count {
        set deref(progress).phase = 3_u8;
        break;
      }
      let index = deref(progress).source_next;
      set deref(progress).source_next = deref(progress).source_next + 1_u64;
      let key = probe_index(home: index, step: index, count: count);
      let full = occupied(slot: &deref(source)[key]);
      if full {
        set deref(progress).next_probe = 0_u64;
        set deref(progress).phase = 1_u8;
      }
      set inspected = inspected + 1_u64;
    }
    if deref(progress).phase == 0_u8 {
      let source_count = deref(source).len;
      if deref(progress).source_next == source_count {
        set deref(progress).phase = 3_u8;
      }
    }
  }
  return inspected;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source.as_bytes());
}

#[test]
fn window_length_joins_and_counted_rotation_need_no_owner_summary() {
    let source = br#"fn choose(first: Box<u64>, second: Box<u64>, extra: Bool) -> result: Slots<Box<u64>, 4> pure {
  let values = slots_new::<Box<u64>, 4>();
  place_back(window: &values, value: move first);
  if extra {
    place_back(window: &values, value: move second);
    return move values;
  }
  return move values;
}

fn rotate(value: Box<u64>) -> result: Slots<Box<u64>, 4> pure {
  let values = slots_new::<Box<u64>, 4>();
  place_back(window: &values, value: move value);
  for (
    round in 0_u64..8_u64,
    invariant at_most_one: values.len <= 1_u64,
    invariant at_least_one: values.len >= 1_u64
  ) {
    let first = take_back(window: &values);
    place_back(window: &values, value: move first);
  }
  return move values;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn a_recursive_exchange_of_two_linear_owners_uses_swap() {
    // v0.59 wrote `let previous = replace deref(target) = move incoming;` and
    // returned the displaced owner. [WIN-3] refuses an assignment over a
    // linear owned place because a linear value has no release, and [OP-11]
    // `swap` is the exchange that exists precisely because no source body can
    // write it without a hole.
    let source = br#"fn recursive_exchange(target: &std::fs::ReadFile, incoming: &std::fs::ReadFile, stop: Bool) -> result: unit writes(target), writes(incoming) {
  if stop {
    swap(first: target, second: incoming);
    return unit;
  } else {
    return recursive_exchange(target: target, incoming: incoming, stop: stop);
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
}

#[test]
fn affine_opaque_drop_and_linear_transfer_use_the_ordinary_rules() {
    assert_complete(include_bytes!(
        "../../../../tests/conformance/cases/pre1-pos-affine-opaque-empty-drop.wf"
    ));
    assert_complete(include_bytes!(
        "../../../../tests/conformance/cases/pre1-pos-linear-opaque-ordinary-transfer.wf"
    ));
    assert_complete(include_bytes!(
        "../../../../tests/conformance/cases/accept-sysrelease-return-unit-declared.wf"
    ));
    assert_rule_kind(
        include_bytes!("../../../../tests/conformance/cases/reject-syseff-return-unit-pure.wf"),
        SemanticRule::Prov6,
        |_| true,
    );
}

#[test]
fn an_explicit_close_on_one_arm_contributes_ordinary_factory_effects() {
    assert_complete(include_bytes!(
        "../../../../tests/conformance/cases/accept-syseff-conditional-release-union.wf"
    ));
    assert_rule_kind(
        include_bytes!(
            "../../../../tests/conformance/cases/reject-syseff-conditional-release-narrow.wf"
        ),
        SemanticRule::Eff2,
        |kind| matches!(kind, SemanticIssueKind::EffectMismatch { .. }),
    );
}

#[test]
fn a_pure_formal_does_not_cover_an_actuals_explicit_close_effects() {
    assert_rule_kind(
        include_bytes!("../../../../tests/conformance/cases/fn4-neg-pure-member-binds-release.wf"),
        SemanticRule::Fn4,
        |kind| matches!(kind, SemanticIssueKind::BehaviorArgumentMismatch { .. }),
    );
}

#[test]
fn a_local_owner_has_no_history_effect_on_its_former_parameter() {
    let source = r#"fn relay(value: Box<u64>) -> result: Box<u64> pure {
  return move value;
}

fn observe(value: Box<u64>, witness: &Box<u64>) -> result: u64 pure {
  let local = relay(value: move value);
  return local.inner;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source.as_bytes());
    let extra = source.replace("-> result: u64 pure", "-> result: u64 reads(witness)");
    assert_rule_kind(extra.as_bytes(), SemanticRule::Eff2, |kind| {
        matches!(kind, SemanticIssueKind::EffectMismatch { extra, .. }
            if extra == &["reads(witness)"])
    });
}

#[test]
fn dynamic_slot_reads_do_not_import_the_placement_inputs_history() {
    let source = r#"struct Slot {
  key: u64;
  payload: Box<u64>;
}

fn read_key(slot: &Slot) -> result: u64 reads(slot.key) {
  return deref(slot).key;
}

fn read_payload(slot: &Slot) -> result: u64 reads(slot.payload) {
  return deref(slot).payload.inner;
}

fn observe(values: &Slots<Slot, 2>, spare: &Box<u64>, first: Box<u64>, second: Box<u64>, index: u64) -> result: u64 writes(values) contract {
  requires deref(values).len == 0_u64;
  requires index < 2_u64;
} {
  let left = Slot(key: 7_u64, payload: move first);
  let right = Slot(key: 9_u64, payload: move second);
  place_back(window: values, value: move left);
  place_back(window: values, value: move right);
  return OBSERVE;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    for observation in [
        "deref(values)[index].key",
        "read_key(slot: &deref(values)[index])",
        "read_payload(slot: &deref(values)[index])",
    ] {
        let accepted = source.replace("OBSERVE", observation);
        assert_complete(accepted.as_bytes());
        let extra = accepted.replace("writes(values)", "reads(spare), writes(values)");
        assert_rule_kind(extra.as_bytes(), SemanticRule::Eff2, |kind| {
            matches!(kind, SemanticIssueKind::EffectMismatch { extra, .. }
                if extra == &["reads(spare)"])
        });
    }
}

const FIELD_ASSIGNMENT: &str = r#"struct Holder {
  before: u64;
  value: Box<u64>;
  after: u64;
}

struct Nested {
  before: u64;
  holder: Holder;
  after: u64;
}

fn exchange(target: &Box<u64>, incoming: Box<u64>) -> result: unit writes(target) {
  set deref(target) = move incoming;
  return unit;
}

fn inspect(holder: &Holder, incoming: Box<u64>) -> result: unit writes(holder.value) {
  exchange(target: &deref(holder).value, incoming: move incoming);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn an_assignment_projects_only_the_selected_ordinary_field() {
    assert_complete(FIELD_ASSIGNMENT.as_bytes());
    // The same projection two levels down. v0.59 spelled the nested case as a
    // reborrow of a `&uniq Holder`; a reference is a local name for a path
    // [REF-1], so the nested path is written directly.
    assert_complete(
        FIELD_ASSIGNMENT
            .replace("holder: &Holder", "holder: &Nested")
            .replace("holder.value", "holder.holder.value")
            .replace("deref(holder).value", "deref(holder).holder.value")
            .as_bytes(),
    );
}

// Retired with the reborrow of [OWN-5]: v0.59's
// `nested_reborrows_project_the_same_field_effects` formed a child loan of a
// `&uniq` parent, and v0.60 has no loan, no holder and no reborrow: a
// reference is a local name for a path [REF-1]. The projection half it
// checked is the nested case of `an_assignment_projects_only_the_selected_
// ordinary_field` above.

#[test]
fn field_effects_reject_missing_selected_access_and_extra_siblings() {
    for row in [
        "reads(holder.value)",
        "pure",
        "writes(holder.before), writes(holder.value)",
        "writes(holder.value), writes(holder.after)",
        "writes(holder.after)",
    ] {
        let source = FIELD_ASSIGNMENT.replace("writes(holder.value)", row);
        assert_rule_kind(source.as_bytes(), SemanticRule::Eff2, |kind| {
            matches!(kind, SemanticIssueKind::EffectMismatch { .. })
        });
    }
}

#[test]
fn a_linear_field_refuses_the_assignment_and_takes_the_exchange_instead() {
    // Assigning over an owned place releases the old value when it is affine;
    // a linear value has no release, so [WIN-3] refuses the write outright.
    let refused = FIELD_ASSIGNMENT.replace("Box<u64>", "std::fs::ReadFile");
    assert_rule_kind(refused.as_bytes(), SemanticRule::Win3, |kind| {
        matches!(kind, SemanticIssueKind::LinearAssignmentTarget { .. })
    });
    assert_complete(
        br#"struct Holder {
  before: u64;
  value: std::fs::ReadFile;
  after: u64;
}

fn exchange(target: &std::fs::ReadFile, incoming: &std::fs::ReadFile) -> result: unit writes(target), writes(incoming) {
  swap(first: target, second: incoming);
  return unit;
}

fn inspect(holder: &Holder, spare: &std::fs::ReadFile) -> result: unit writes(holder.value), writes(spare) {
  exchange(target: &deref(holder).value, incoming: spare);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
}

#[test]
fn a_generic_swap_needs_no_result_routing_summary() {
    let source = r#"fn exchange<T>(target: &T, incoming: &T) -> result: unit writes(target), writes(incoming) {
  swap(first: target, second: incoming);
  return unit;
}

fn transfer(target: &std::fs::ReadFile, incoming: &std::fs::ReadFile) -> result: unit writes(target), writes(incoming) {
  return exchange::<std::fs::ReadFile>(target: target, incoming: incoming);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source.as_bytes());
}

#[test]
fn ordinary_numeric_result_facts_survive_a_later_outcome_match() {
    assert_complete(
        br#"enum TestError {
  Failed();
}

fn provide(end: u64) -> (result: Result<unit, TestError>, next: u64, count: u64) pure contract {
  ensures next <= end;
} {
  return Ok<unit, TestError>(value: unit), end, 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let (outcome, next, count) = provide(end: 4096_u64);
  match outcome {
    Ok(value: done) => {
      invariant bounded: next <= 4096_u64;
    }
    Err(error: problem) => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
}

/// The EFF-2 rejection's `expected_row`, `missing` and `extra` for one source.
fn effect_mismatch(source: &str) -> (String, Vec<String>, Vec<String>) {
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected an EFF-2 rejection, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Eff2);
        let SemanticIssueKind::EffectMismatch {
            expected_row,
            missing,
            extra,
            ..
        } = issue.kind()
        else {
            panic!("unexpected kind {:?}", issue.kind());
        };
        (expected_row.clone(), missing.clone(), extra.clone())
    })
}

/// A two-field counter whose `record` declares `ROW`, runs `BODY`, and is
/// called once, so a row a call refuses against itself is visible [EFF-5].
/// `spare` is never accessed: declaring `reads(spare)` beside a write that
/// covers the body is an EFF-2 mismatch that [SET-1] does not preempt.
const RECORD_COUNTER: &str = r#"struct Stats {
  count: u64;
  total: u64;
}

fn record(stats: &Stats, spare: &u64) -> result: unit ROW {
  BODY
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let stats = Stats(count: 0_u64, total: 0_u64);
  let other = 0_u64;
  record(stats: &stats, spare: &other);
  return std::process::exit_status(code: 0_u8);
}
"#;

fn record_counter(row: &str, body: &str) -> String {
    RECORD_COUNTER.replace("ROW", row).replace("BODY", body)
}

/// [EFF-1] `writes(p)` states every access at or below `p`, so a row that
/// also carries `reads(p)` states the read twice and is refused at the
/// redundant entry, naming the entry that covers it.
#[test]
fn a_read_the_same_rows_write_subsumes_is_an_eff1_rejection() {
    let source = record_counter(
        "reads(stats.count), writes(stats.count)",
        "let old = deref(stats).count;\n  set deref(stats).count = old +wrap 1_u64;",
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Eff1, |kind| {
        matches!(kind, SemanticIssueKind::SubsumedEffectEntry { entry, covering }
            if entry == "reads(stats.count)" && covering == "writes(stats.count)")
    });
    super::assert_rule_at(source.as_bytes(), SemanticRule::Eff1, "reads(stats.count)");
    // A read and a write of different paths remain one row.
    assert_complete(
        record_counter(
            "reads(stats.total), writes(stats.count)",
            "let old = deref(stats).total;\n  set deref(stats).count = old;",
        )
        .as_bytes(),
    );
}

/// [EFF-2] the suggested row never pairs a read with the write that
/// subsumes it [EFF-1], and declaring it is accepted and callable [EFF-5].
/// An examples study met `reads(stats.count), writes(stats.count)` here and
/// then an EFF-5 self-overlap at every call.
#[test]
fn the_suggested_row_for_a_read_modify_write_is_the_write_alone() {
    let body = "let old = deref(stats).count;\n  set deref(stats).count = old +wrap 1_u64;";
    let (expected, missing, extra) = effect_mismatch(&record_counter(
        "writes(stats.count), writes(stats.total)",
        body,
    ));
    assert_eq!(expected, "writes(stats.count)");
    assert!(missing.is_empty(), "{missing:?}");
    assert_eq!(extra, ["writes(stats.total)"]);
    assert_complete(record_counter(&expected, body).as_bytes());
}

/// [EFF-2] `missing` names the entry of the suggested row that covers each
/// uncovered access: an exhibited path the row lacks, or the write that
/// states an uncovered read below it [EFF-1].
#[test]
fn a_missing_entry_is_named_by_the_suggested_entry_covering_it() {
    let (expected, missing, extra) = effect_mismatch(&record_counter(
        "writes(stats.count)",
        "let seen = deref(stats).total;\n  let old = deref(stats).count;\n  set deref(stats).count = seen +wrap old;",
    ));
    assert_eq!(expected, "reads(stats.total), writes(stats.count)");
    assert_eq!(missing, ["reads(stats.total)"]);
    assert!(extra.is_empty(), "{extra:?}");
    let (expected, missing, extra) = effect_mismatch(&record_counter(
        "writes(stats.count)",
        "let whole = deref(stats);\n  set deref(stats).count = whole.total;",
    ));
    assert_eq!(expected, "reads(stats), writes(stats.count)");
    assert_eq!(missing, ["reads(stats)"]);
    assert!(extra.is_empty(), "{extra:?}");
    // A callee's projected write reaches the whole parameter [EFF-2]; the
    // read below it is covered, so `missing` names that write.
    let (expected, missing, extra) = effect_mismatch(
        r#"struct Stats {
  count: u64;
  total: u64;
}

fn reset(stats: &Stats) -> result: unit writes(stats) {
  set deref(stats) = Stats(count: 0_u64, total: 0_u64);
  return unit;
}

fn record(stats: &Stats) -> result: u64 reads(stats.count) {
  let seen = deref(stats).count;
  reset(stats: stats);
  return seen;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert_eq!(expected, "writes(stats)");
    assert_eq!(missing, ["writes(stats)"]);
    assert!(extra.is_empty(), "{extra:?}");
}

/// [EFF-2] the suggested row is the exhibited row without the entries another
/// of its entries covers, one path per entry [EFF-1]. A whole read beside a
/// write below it stays exact: one argument supplies both, and [EFF-5] does
/// not compare a pair that overlaps at every position. Each suggestion is
/// accepted with the call in place.
#[test]
fn the_suggested_row_is_the_exhibited_row_without_covered_entries() {
    for (body, suggested) in [
        (
            "let whole = deref(stats);\n  set deref(stats).count = whole.total;",
            "reads(stats), writes(stats.count)",
        ),
        (
            "let seen = deref(stats).count;\n  set deref(stats) = Stats(count: seen, total: 0_u64);",
            "writes(stats)",
        ),
        (
            "let seen = deref(stats).total;\n  set deref(stats).count = seen;",
            "reads(stats.total), writes(stats.count)",
        ),
        (
            "let first = deref(stats).count;\n  let second = deref(stats).total;",
            "reads(stats.count), reads(stats.total)",
        ),
    ] {
        let (expected, _, extra) =
            effect_mismatch(&record_counter("reads(spare), writes(stats)", body));
        assert_eq!(expected, suggested, "{body}");
        assert!(
            extra.iter().any(|entry| entry == "reads(spare)"),
            "{extra:?}"
        );
        assert_complete(record_counter(&expected, body).as_bytes());
    }
}

/// Two indexed positions stay apart in the suggestion: whether they select
/// one element is the call site's question, which a call with distinct
/// indices answers [EFF-5].
#[test]
fn the_suggested_row_keeps_positions_a_call_can_separate() {
    let source = r#"fn copy_within(window: &Slots<u8, 2>, spare: &u64, from: u64, to: u64) -> result: unit ROW {
  let length = deref(window).len;
  if from < length {
    if to < length {
      let old = deref(window)[from];
      set deref(window)[to] = old;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u8, 2>();
  place_back(window: &window, value: 7_u8);
  let other = 0_u64;
  let i = 0_u64;
  let j = 1_u64;
  if i < j {
    copy_within(window: &window, spare: &other, from: i, to: j);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let (expected, _, _) = effect_mismatch(&source.replace("ROW", "reads(spare), writes(window)"));
    assert_eq!(
        expected,
        "reads(window[from]), reads(window.len), writes(window[to])"
    );
    assert_complete(source.replace("ROW", &expected).as_bytes());
}
