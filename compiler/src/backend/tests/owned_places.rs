//! Execute ownership transfers over inline aggregates, cells and windows.
//! These cases observe values and effects rather than the emitter's choice of
//! instructions.
//!
//! Six of this module's cases retired with the rules their subject was:
//!
//! - `replaced_array_snapshots_remain_independent_through_multi_result_calls`
//!   and `replaced_aggregate_snapshots_survive_writes_and_helper_returns`
//!   retired with [SET-2]: `let x = replace p = e;` has no v0.60 production
//!   and nothing hands the old value back, so an independent snapshot of the
//!   displaced value has no subject. [SET-1] writes exactly one place and
//!   [WIN-3] releases the old affine value there; the exchange that keeps both
//!   owners is [OP-11] `swap`, and the read-and-commit is [OP-12]'s atomic
//!   in-place update `set p = f(move p, args...);`. The capture, adjacency and
//!   release properties those two also carried are kept by
//!   `an_element_target_is_captured_before_the_rhs_changes_its_index`,
//!   `zero_sized_aggregate_assignment_preserves_adjacent_fields` and
//!   `box_assignment_updates_the_owner_and_releases_each_cell_once`.
//! - `general_and_extent_boxes_keep_distinct_cleanup_actions` and
//!   `region_polymorphic_box_calls_preserve_each_independent_release_class`
//!   retired with [OWN-3], [OWN-4], [OWN-10], [FORM-8] and [STOR-4]: v0.60 has
//!   no regions, no region parameters, no arenas and no second release class.
//!   There is one heap [STOR-8] and one cell release [STOR-3], which every
//!   remaining allocation ledger below observes.
//! - `box_field_borrows_keep_the_owner_slot_across_reborrow_and_return`
//!   retired with [OWN-6] and [OWN-14]: [REF-1] makes a reference a name for a
//!   path, so there is no reborrow, and [REF-3] refuses a returned reference
//!   outright. `box_assignment_updates_the_owner_and_releases_each_cell_once`
//!   and `referencing_owned_box_content_addresses_the_allocation` keep the
//!   owner-slot coverage.
//! - `partial_construction_refusal_preserves_effects_values_and_release_order`
//!   retired with [BLK-2]: its whole subject was the refusal edge of a
//!   fallible allocation. [STOR-8] makes allocation total in the source - it
//!   never returns a failure and no allocating operation carries a `Result` -
//!   so no refusal arm exists to preserve effects across.
//!   `nested_and_residual_cleanup_preserve_release_graph_order` keeps the
//!   release-order coverage.
//! - `indexed_targets_are_captured_before_disjoint_rhs_effects`,
//!   `aggregate_loop_carries_and_element_swaps_preserve_both_owners` and
//!   `loop_owner_sources_cover_later_iterations_and_counted_exhaustion` each
//!   lost their multi-target commit `set (a, b) = e, f;` with [LIV-2] and kept
//!   the rest. Where the commit exchanged two owners it is now [OP-11] `swap`;
//!   where it wrote two copy places it spells the temporary [OP-11] refuses to
//!   hide; and where it supplied a second right-hand side it is gone with the
//!   helper that produced it.
//! - `dispose x;` retired with it. [PROV-6] states there is no release
//!   operation: an affine value is released early by moving it into a function
//!   that consumes it and ends, which is what
//!   `owner_cleanup_releases_cell_fields_in_checked_order` now does, and a
//!   proved-empty window is consumed by [OP-14] `free_empty`.

use super::{compile, compile_and_run, compile_link_and_run};

/// Retain the ordinary emitted call boundaries for a second execution. This
/// changes only optimization permission, so a passing inlined body cannot hide
/// a broken aggregate argument or result ABI.
pub(super) fn retain_calls(module: &str) -> String {
    let mut retained = 0;
    let result = module
        .lines()
        .map(|line| {
            if line.starts_with("define ")
                && line.contains(" @wf_")
                && !line.contains(" @wf__")
                && let Some(header) = line.strip_suffix(" {")
            {
                retained += 1;
                format!("{header} noinline {{\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    assert!(retained > 0, "the fixture must retain an emitted function");
    result
}

fn assert_success(module: &str) {
    let output = compile_and_run(module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn consumed_result_fields_preserve_padding_siblings_and_smaller_returns() {
    let source = br#"nocopy struct Row {
  left: u64;
  right: u64;
}

fn split(value: Row, bias: u64) -> (before: u8, updated: Row, after: u16) pure {
  set value.left = value.left +wrap bias;
  set value.right = value.right +wrap 3_u64;
  return 7_u8, move value, 513_u16;
}

fn relay(value: Row, bias: u64) -> result: Row pure {
  let (before, updated, after) = split(value: move value, bias: bias);
  let stamp = 0_u64;
  if before == 7_u8 {
    set stamp = stamp +wrap 1_u64;
  }
  if after == 513_u16 {
    set stamp = stamp +wrap 2_u64;
  }
  set updated.left = updated.left +wrap stamp;
  return move updated;
}

fn repeat(value: Row, count: u64) -> result: Row pure {
  for (iteration in 0_u64..count) {
    let (before, updated, after) = split(value: move value, bias: 5_u64);
    set value = move updated;
    if before != 7_u8 {
      return Row(left: 0_u64, right: 0_u64);
    }
    if after != 513_u16 {
      return Row(left: 0_u64, right: 0_u64);
    }
  }
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  let input = Row(left: 11_u64, right: 29_u64);
  let result = relay(value: move input, bias: 5_u64);
  if result.left != 19_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if result.right != 32_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  let empty_input = Row(left: 41_u64, right: 53_u64);
  let empty = repeat(value: move empty_input, count: 0_u64);
  if empty.left != 41_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if empty.right != 53_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  let loop_input = Row(left: 41_u64, right: 53_u64);
  let loop_result = repeat(value: move loop_input, count: 3_u64);
  if loop_result.left != 56_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  if loop_result.right != 62_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        // Retaining calls makes the callee write the complete padded result
        // before its caller extracts the middle field and the later sibling.
        // relay returns only Row, whose output is smaller than split's tuple.
        assert_success(&module);
        assert_success(&retain_calls(&module));
    }
}

/// A reference to an indexed child field names exactly that path [REF-1], so
/// two calls through it reach the selected element's field and nothing else
/// [EFF-5].
#[test]
fn indexed_child_references_update_only_the_selected_field() {
    let source = br#"struct Point {
  x: u64;
  y: u64;
}

fn write(value: &u64) -> result: unit writes(value) {
  set deref(value) = 7_u64;
  return unit;
}

fn update(points: &Array<Point, 2>, index: u64) -> result: unit writes(points) contract {
  requires index < 2_u64;
} {
  write(value: &deref(points)[index].x);
  write(value: &deref(points)[index].x);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = Point(x: 17_u64, y: 29_u64);
  let second = Point(x: 41_u64, y: 53_u64);
  let loaded = slots_new::<Point, 2>();
  place_back(window: &loaded, value: first);
  place_back(window: &loaded, value: second);
  let points = slots_into_array::<Point, 2>(values: move loaded);
  update(points: &points, index: 1_u64);
  if points[0_u64].x != 17_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if points[0_u64].y != 29_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if points[1_u64].x != 7_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if points[1_u64].y != 53_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        assert_success(&module);
        assert_success(&retain_calls(&module));
    }
}

/// Runtime-capacity Array elements use the same complete typed address path
/// for copy reads, stores, and references. Nested fields retain their exact
/// selected address and do not disturb adjacent fields or elements.
#[test]
fn runtime_array_element_suffixes_reach_only_the_selected_field() {
    let source = br#"struct Pair {
  left: u64;
  right: u64;
}

struct Row {
  pair: Pair;
  guard: u64;
}

fn add_to(value: &u64, amount: u64) -> result: unit writes(value) {
  let old = deref(value);
  set deref(value) = old +wrap amount;
  return unit;
}

fn adjust(rows: &Box<Array<Row>>, index: u64, amount: u64) -> result: unit writes(rows) contract {
  requires index < deref(rows).inner.len;
} {
  let old = deref(rows).inner[index].pair.right;
  set deref(rows).inner[index].pair.right = old +wrap 1_u64;
  let selected = &deref(rows).inner[index].pair.right;
  add_to(value: selected, amount: amount);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = Pair(left: 3_u64, right: 5_u64);
  let seed = Row(pair: pair, guard: 7_u64);
  let rows = box_array_filled::<Row>(count: 2_u64, value: seed);
  let before = rows.inner[0_u64].pair.left;
  set rows.inner[1_u64].pair.right = 11_u64;
  let selected = &rows.inner[1_u64].pair.right;
  add_to(value: selected, amount: before);
  adjust(rows: &rows, index: 1_u64, amount: before);
  if rows.inner.len != 2_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if rows.inner[0_u64].pair.left != 3_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if rows.inner[0_u64].pair.right != 5_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if rows.inner[1_u64].pair.left != 3_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if rows.inner[1_u64].pair.right != 18_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  if rows.inner[1_u64].guard != 7_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  let owners = slots_new::<Box<Array<Row>>, 1>();
  place_back(window: &owners, value: move rows);
  let nested = slots_into_array::<Box<Array<Row>>, 1>(values: move owners);
  if nested[0_u64].inner.len != 2_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  let nested_value = &nested[0_u64].inner[1_u64].pair.right;
  add_to(value: nested_value, amount: 2_u64);
  if nested[0_u64].inner[1_u64].pair.right != 20_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  if nested[0_u64].inner[0_u64].pair.right != 5_u64 {
    return std::process::exit_status(code: 9_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        assert_success(&module);
        assert_success(&retain_calls(&module));
    }
}

#[test]
fn loop_owner_sources_cover_later_iterations_and_counted_exhaustion() {
    // The two-place exchange retired with [LIV-2]'s multi-target commit; the
    // v0.60 spelling is [OP-11] `swap`, which consumes neither root and leaves
    // each the sole owner of the value the other held.
    let source = br#"fn rotate(first: Box<u64>, second: Box<u64>, count: u64) -> result: u64 pure {
  for (iteration in 0_u64..count) {
    swap(first: &first, second: &second);
  }
  let held = first.inner;
  return held;
}

fn once(first: Box<u64>, second: Box<u64>) -> result: u64 pure {
  let repeat = True();
  loop {
    let observed = first.inner;
    if repeat {
      set repeat = False();
      swap(first: &first, second: &second);
    } else {
      break;
    }
  }
  let held = first.inner;
  return held;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = box_new::<u64>(value: 17_u64);
  let b = box_new::<u64>(value: 29_u64);
  let zero = rotate(first: move a, second: move b, count: 0_u64);
  let c = box_new::<u64>(value: 17_u64);
  let d = box_new::<u64>(value: 29_u64);
  let one = rotate(first: move c, second: move d, count: 1_u64);
  let e = box_new::<u64>(value: 17_u64);
  let f = box_new::<u64>(value: 29_u64);
  let two = rotate(first: move e, second: move f, count: 2_u64);
  let g = box_new::<u64>(value: 17_u64);
  let h = box_new::<u64>(value: 29_u64);
  let ordinary = once(first: move g, second: move h);
  if zero != 17_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if one != 29_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if two != 17_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if ordinary != 29_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        assert_success(&module);
        assert_success(&retain_calls(&module));
    }
}

#[test]
fn wide_result_returns_preserve_success_refusal_and_owned_children() {
    let source = br#"struct Record {
  words: Array<u64, 512>;
  first: Box<u64>;
  second: Box<u64>;
}

fn make_record(seed: u64) -> result: Result<Record, u8> pure {
  let first = box_new::<u64>(value: seed);
  if seed == 0_u64 {
    return Err<Record, u8>(error: 1_u8);
  }
  let words = array_filled::<u64, 512>(value: seed);
  let second = box_new::<u64>(value: 29_u64);
  let record = Record(words: words, first: move first, second: move second);
  return Ok<Record, u8>(value: move record);
}

fn relay(seed: u64) -> result: Result<Record, u8> pure {
  return make_record(seed: seed);
}

fn main() -> status: std::process::ExitStatus pure {
  let reserved = box_slots_new::<Record>(capacity: 1_u64);
  match relay(seed: 0_u64) {
    Err(error: code) => {
      if code != 1_u8 {
        return std::process::exit_status(code: 2_u8);
      }
    }
    Ok(value: unexpected) => {
      return std::process::exit_status(code: 3_u8);
    }
  }
  match relay(seed: 17_u64) {
    Err(error: code) => {
      return std::process::exit_status(code: 4_u8);
    }
    Ok(value: made) => {
      place_back(window: &reserved.inner, value: move made);
      let observed = take_back(window: &reserved.inner);
      if observed.words[0_u64] != 17_u64 {
        return std::process::exit_status(code: 5_u8);
      }
      if observed.words[511_u64] != 17_u64 {
        return std::process::exit_status(code: 6_u8);
      }
      if observed.first.inner != 17_u64 {
        return std::process::exit_status(code: 7_u8);
      }
      if observed.second.inner != 29_u64 {
        return std::process::exit_status(code: 8_u8);
      }
      return std::process::exit_status(code: 0_u8);
    }
  }
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        // The refused-reservation arm retired with [BLK-2]: [STOR-8] makes
        // allocation total in the source, so no writer-reachable refusal edge
        // exists for the observer to script.
        let host = allocation_observer(4, 0);
        let output = compile_link_and_run(&observed, Some(&host), &[]);
        // Failure cleans the first child. Success moves both children through
        // Result and the reserved window, releasing each once.
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, b"A1;A2;F2;A3;A4;F3;F4;F1;", "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
fn competing_wide_returns_preserve_live_and_referenced_contents() {
    let source = br#"fn choose_live(seed: u64) -> result: Array<u64, 512> pure {
  let original = array_filled::<u64, 512>(value: seed);
  if seed == 0_u64 {
    return original;
  }
  let candidate = array_filled::<u64, 512>(value: 37_u64);
  if original[511_u64] != seed {
    return original;
  }
  return candidate;
}

fn choose_referenced(seed: u64) -> result: Array<u64, 512> pure {
  let original = array_filled::<u64, 512>(value: seed);
  if seed == 0_u64 {
    return original;
  }
  let reader = &original[0_u64..512_u64];
  let candidate = array_filled::<u64, 512>(value: 43_u64);
  let trailing = deref(reader)[511_u64];
  if trailing != seed {
    return array_filled::<u64, 512>(value: 99_u64);
  }
  return candidate;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = choose_live(seed: 0_u64);
  let second = choose_live(seed: 17_u64);
  let third = choose_referenced(seed: 0_u64);
  let fourth = choose_referenced(seed: 19_u64);
  if first[0_u64] != 0_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if first[511_u64] != 0_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if second[0_u64] != 37_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if second[511_u64] != 37_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if third[0_u64] != 0_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  if third[511_u64] != 0_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  if fourth[0_u64] != 43_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  if fourth[511_u64] != 43_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        assert_success(&retain_calls(&module));
    }
}

#[test]
fn an_owned_parameter_uses_same_or_distinct_result_storage_after_entry_transfer() {
    // An ordinary value-field update keeps the independent owned input/result
    // ABI under test. The wide array still forces indirect storage; the watch
    // read and both result aliases still expose a misplaced or premature entry
    // transfer. `set same = extend(items: move same, ...)` is [OP-12]'s atomic
    // in-place update: the target place is the call's first argument, `extend`
    // returns the place's type and has no failure exit, and its row writes no
    // prefix of the target. A by-value parameter carries no effect entry
    // [EFF-1], so the row states the watch read alone.
    let source = br#"nocopy struct Row {
  payload: Array<u64, 16>;
  value: u64;
}

fn extend(items: Row, value: u64, watch: &u64) -> updated: Row reads(watch) {
  let bias = deref(watch);
  let adjusted = value +wrap bias;
  set items.value = adjusted;
  return move items;
}

fn main() -> status: std::process::ExitStatus pure {
  let watch = 5_u64;
  let payload = array_filled::<u64, 16>(value: 41_u64);
  let same = Row(payload: payload, value: 0_u64);
  set same = extend(items: move same, value: 12_u64, watch: &watch);
  let other_payload = array_filled::<u64, 16>(value: 43_u64);
  let vacant = Row(payload: other_payload, value: 0_u64);
  let distinct = extend(items: move vacant, value: 24_u64, watch: &watch);
  if same.value != 17_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if distinct.value != 29_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if same.payload[15_u64] != 41_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if distinct.payload[15_u64] != 43_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if watch != 5_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = super::emit_lowered(source, super::OverlapLowering::Off);
    let append = super::emitted_function(&module, "extend");
    assert!(!append.contains("alloca"), "{append}");
    assert_eq!(append.matches("call void @llvm.memmove.").count(), 1);
    let main = super::emitted_function(&module, "main");
    let calls = main
        .lines()
        .filter_map(|line| line.split_once("@wf_extend(").map(|(_, tail)| tail))
        .map(|arguments| {
            let mut arguments = arguments.split(',').map(str::trim);
            let result = arguments.next().expect("result pointer");
            let input = arguments.next().expect("input pointer");
            result == input
        })
        .collect::<Vec<_>>();
    assert!(
        calls.contains(&true),
        "exercise a consumed input/result alias"
    );
    assert!(
        calls.contains(&false),
        "exercise an independent result destination"
    );
    assert_success(&retain_calls(&module));
}

#[test]
fn returned_parameter_snapshots_precede_result_alias_writes() {
    let source = br#"nocopy struct Row {
  value: u64;
}

struct Holder {
  row: Row;
}

fn discard(value: Row) -> result: unit pure {
  return unit;
}

fn choose(left: Row, right: Row, watch: &u64) -> result: Row reads(watch) {
  let expected = deref(watch);
  if right.value != expected {
    let ignored = discard(value: move left);
    return Row(value: 99_u64);
  }
  return move left;
}

fn relay(held: Row, watch: &u64, offered: u64) -> result: Row reads(watch) {
  let row = Row(value: offered);
  let fresh = Holder(row: move row);
  return choose(left: move fresh.row, right: move held, watch: watch);
}

fn main() -> status: std::process::ExitStatus pure {
  let watch = 29_u64;
  let first_input = Row(value: 29_u64);
  let second_input = Row(value: 31_u64);
  let first = relay(held: move first_input, watch: &watch, offered: 11_u64);
  let second = relay(held: move second_input, watch: &watch, offered: 17_u64);
  if first.value != 11_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if second.value != 99_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if watch != 29_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        // The call inside relay can place its result in its consumed second
        // input's backing while choose returns its first input. Retained calls
        // keep snapshot ordering observable: save the second input privately
        // before the first input initializes result storage, or the branch
        // changes and returns 99 instead of 11.
        assert_success(&retain_calls(&module));
    }
}

/// A [SET-1] commit over a zero-extent aggregate field leaves its adjacent
/// fields alone. The displaced value is released there [WIN-3]; the read-out
/// that handed it back retired with [SET-2].
#[test]
fn zero_sized_aggregate_assignment_preserves_adjacent_fields() {
    let source = br#"struct Envelope {
  before: u64;
  empty: Array<u64, 0>;
  after: u64;
}

fn install_empty(target: &Envelope, value: Array<u64, 0>) -> result: unit writes(target.empty) {
  set deref(target).empty = value;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let empty = array_filled::<u64, 0>(value: 0_u64);
  let envelope = Envelope(before: 17_u64, empty: empty, after: 29_u64);
  let replacement = array_filled::<u64, 0>(value: 43_u64);
  install_empty(target: &envelope, value: replacement);
  if envelope.before != 17_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if envelope.after != 29_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  let size = envelope.empty.len;
  if size != 0_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        assert_success(&retain_calls(&module));
    }
}

#[test]
fn ordinary_box_owner_transfer_keeps_values_and_release_across_two_helpers() {
    // The exchange that hands both owners back is [OP-11] `swap`: it consumes
    // neither root, and after it each binding is the sole owner of the cell
    // the other held. `replace`'s read-out retired with [SET-2].
    let source = br#"struct Observed {
  previous: u64;
  current: u64;
}

fn exchange(slot: &Box<u64>, incoming: &Box<u64>) -> result: unit writes(slot), writes(incoming) {
  swap(first: slot, second: incoming);
  return unit;
}

fn observe(owner: Box<u64>, incoming: Box<u64>) -> result: Observed pure {
  exchange(slot: &owner, incoming: &incoming);
  let previous_value = incoming.inner;
  let current_value = owner.inner;
  return Observed(previous: previous_value, current: current_value);
}

fn main() -> status: std::process::ExitStatus pure {
  let owner = box_new::<u64>(value: 11_u64);
  let incoming_value = owner.inner +wrap 11_u64;
  let incoming = box_new::<u64>(value: incoming_value);
  let seen = observe(owner: move owner, incoming: move incoming);
  if seen.previous != 11_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if seen.current != 22_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let host = allocation_observer(2, 0);
        let output = compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, b"A1;A2;F1;F2;");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// A [SET-1] commit through a reference parameter updates the caller's owner
/// and releases the displaced cell there [WIN-3], so each of the two cells is
/// freed exactly once.
///
/// The read-out of the displaced cell retired with [SET-2]: nothing hands it
/// back, because [WIN-3] releases it at the commit. The exchange that keeps
/// both owners alive is [OP-11] `swap`, exercised by
/// `ordinary_box_owner_transfer_keeps_values_and_release_across_two_helpers`
/// above.
#[test]
fn box_assignment_updates_the_owner_and_releases_each_cell_once() {
    let source = br#"fn install(slot: &Box<u64>, incoming: Box<u64>) -> result: unit writes(slot) {
  set deref(slot) = move incoming;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let cell = box_new::<u64>(value: 11_u64);
  let incoming = box_new::<u64>(value: 22_u64);
  install(slot: &cell, incoming: move incoming);
  let seen = cell.inner;
  if seen != 22_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let host = allocation_observer(2, 0);
        let output = compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, b"A1;A2;F1;F2;");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// A [SET-1] commit through a payload-step reference updates the child owner
/// inside its cell, and [WIN-3] releases the displaced child there; the
/// caller's tree then owns the replacement and not a copied enum's slot.
///
/// The read-out of the displaced child retired with [SET-2]; the sibling read
/// beside it keeps the payload-step reference under test [REF-1, OWN-13].
#[test]
fn enum_payload_assignment_updates_the_child_owner_in_its_box() {
    let source = br#"enum Node {
  Marker(prefix: u64, suffix: u64);
  HasChildren(left: Box<u64>, right: Box<u64>);
}

fn exchange_child(tree: &Box<Node>, incoming: Box<u64>) -> kept: u64 writes(tree) {
  match deref(tree).inner {
    Marker(prefix: marker_prefix, suffix: marker_suffix) => {
      return 0_u64;
    }
    HasChildren(left: left_slot, right: child_slot) => {
      set deref(child_slot) = move incoming;
      let kept = deref(left_slot).inner;
      return kept;
    }
  }
}

fn read_child(tree: &Box<Node>) -> result: u64 reads(tree) {
  match deref(tree).inner {
    Marker(prefix: marker_prefix, suffix: marker_suffix) => {
      return 0_u64;
    }
    HasChildren(left: left_slot, right: child_slot) => {
      let sibling = deref(left_slot).inner;
      if sibling != 33_u64 {
        return 0_u64;
      }
      let child = deref(child_slot).inner;
      return child;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let first = box_new::<u64>(value: 11_u64);
  let incoming_value = first.inner +wrap 11_u64;
  let incoming = box_new::<u64>(value: incoming_value);
  let sibling_value = incoming.inner +wrap 11_u64;
  let sibling = box_new::<u64>(value: sibling_value);
  let node = Node::HasChildren(left: move sibling, right: move first);
  let tree = box_new::<Node>(value: move node);
  let kept = exchange_child(tree: &tree, incoming: move incoming);
  if kept != 33_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  let observed = read_child(tree: &tree);
  if observed != 22_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let host = allocation_observer(4, 0);
        let output = compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        // Each cell construction reads the preceding cell, so [PAR-1] cannot
        // overlap the allocations whose observer IDs name these owners.
        // The displaced child is released at the commit, before the caller
        // rereads its tree. That tree must own the replacement child, not a
        // copied enum's slot. PROV-6 then visits the sibling and replacement
        // in field order and frees the cell after its content.
        assert_eq!(output.stdout, b"A1;A2;A3;A4;F1;F3;F2;F4;");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// [SET-1] resolves and evaluates its target before the right-hand side, so a
/// call that writes the index binding still commits to the slot the statement
/// named. Slots and both fixed- and runtime-capacity Arrays share this rule.
///
/// The two-target half of this case retired with [LIV-2]: `set (a, b) = e, f;`
/// has no v0.60 production, [SET-1] writes exactly one place, and the
/// `finish` helper that supplied the second right-hand side went with it.
#[test]
fn indexed_targets_are_captured_before_disjoint_rhs_effects() {
    let module = compile(
        br#"fn advance(offset: &u64, trace: &u64) -> result: u64 writes(offset), writes(trace) {
  set deref(offset) = 1_u64;
  let shifted = deref(trace) *wrap 10_u64;
  set deref(trace) = shifted +wrap 1_u64;
  return 41_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let left = slots_new::<u64, 2>();
  place_back(window: &left, value: 3_u64);
  place_back(window: &left, value: 5_u64);
  let offset = 0_u64;
  let trace = 0_u64;
  invariant single_target_bound: offset < left.len;
  set left[offset] = advance(offset: &offset, trace: &trace);
  if left[0_u64] != 41_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if left[1_u64] != 5_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if offset != 1_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if trace != 1_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  let fixed = array_filled::<u64, 2>(value: 7_u64);
  set offset = 0_u64;
  invariant fixed_target_bound: offset < fixed.len;
  set fixed[offset] = advance(offset: &offset, trace: &trace);
  if fixed[0_u64] != 41_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  if fixed[1_u64] != 7_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  if offset != 1_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  let runtime = box_array_filled::<u64>(count: 2_u64, value: 11_u64);
  set offset = 0_u64;
  invariant runtime_target_bound: offset < runtime.inner.len;
  set runtime.inner[offset] = advance(offset: &offset, trace: &trace);
  if runtime.inner[0_u64] != 41_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  if runtime.inner[1_u64] != 11_u64 {
    return std::process::exit_status(code: 9_u8);
  }
  if offset != 1_u64 {
    return std::process::exit_status(code: 10_u8);
  }
  if trace != 111_u64 {
    return std::process::exit_status(code: 11_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert_success(&module);
    assert_success(&retain_calls(&module));
}

/// The same capture over an aggregate element: the commit lands in the slot
/// the statement named, not in the slot the right-hand side's write of the
/// index binding would name afterwards.
///
/// The displaced row's read-out retired with [SET-2]; [WIN-3] releases the old
/// element at the commit instead.
#[test]
fn an_element_target_is_captured_before_the_rhs_changes_its_index() {
    let module = compile(
        br#"struct Row {
  left: u64;
  right: u64;
}

fn replacement(offset: &u64) -> result: Row writes(offset) {
  set deref(offset) = 1_u64;
  return Row(left: 19_u64, right: 23_u64);
}

fn main() -> status: std::process::ExitStatus pure {
  let rows = slots_new::<Row, 2>();
  let first = Row(left: 3_u64, right: 5_u64);
  place_back(window: &rows, value: first);
  let second = Row(left: 11_u64, right: 13_u64);
  place_back(window: &rows, value: second);
  let offset = 0_u64;
  invariant target_bound: offset < rows.len;
  set rows[offset] = replacement(offset: &offset);
  if rows[0_u64].left != 19_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if rows[0_u64].right != 23_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if rows[1_u64].left != 11_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  if rows[1_u64].right != 13_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  if offset != 1_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert_success(&module);
    assert_success(&retain_calls(&module));
}

/// A loop-carried exchange and an element exchange each keep both owners
/// alive [OP-11], and the copy-place exchange beside them spells the temporary
/// [OP-11] refuses to hide.
///
/// [LIV-2]'s `set (a, b) = e, f;` retired; each of its three uses here has its
/// own successor. Two exchange owners, which is `swap`; the third exchanged
/// two copy places, which `swap` rejects at the first `borrow_expr` with the
/// restructuring `read the two values and assign them back`.
#[test]
fn aggregate_loop_carries_and_element_swaps_preserve_both_owners() {
    let module = compile(
        br#"nocopy struct Row {
  left: u64;
  right: u64;
}

struct Table {
  rows: Array<Row, 2>;
  tag: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = Row(left: 1_u64, right: 10_u64);
  let second = Row(left: 2_u64, right: 20_u64);
  for (round in 0_u64..5_u64) {
    swap(first: &first, second: &second);
    set first.left = first.left +wrap 100_u64;
    set second.right = second.right +wrap 1_u64;
  }
  if first.left != 302_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if first.right != 22_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if second.left != 201_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if second.right != 13_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  let loaded = slots_new::<Row, 2>();
  place_back(window: &loaded, value: move first);
  place_back(window: &loaded, value: move second);
  let rows = slots_into_array::<Row, 2>(values: move loaded);
  let table = Table(rows: move rows, tag: 41_u64);
  swap(first: &table.rows[0_u64], second: &table.rows[1_u64]);
  let saved_left = table.rows[0_u64].left;
  set table.rows[0_u64].left = table.rows[1_u64].right;
  set table.rows[1_u64].right = saved_left;
  if table.rows[0_u64].left != 22_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  if table.rows[0_u64].right != 13_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  if table.rows[1_u64].left != 302_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  if table.rows[1_u64].right != 201_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  if table.tag != 41_u64 {
    return std::process::exit_status(code: 9_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert_success(&module);
}

/// OP-11 excludes partial overlap while admitting an exactly equal target.
/// The owning payload makes a corrupted snapshot observable at cleanup; the
/// wrapped Ring and retained helper keep address formation and dynamic indexes
/// in the executable path instead of folding the exchange into constants.
#[test]
fn aggregate_swap_uses_equal_or_disjoint_copies_for_wrapped_owning_records() {
    let module = compile(
        br#"nocopy struct Row {
  words: Array<u64, 32>;
  owner: Box<u64>;
}

fn make_row(value: u64) -> result: Row pure {
  let words = array_filled::<u64, 32>(value: value);
  let owner = box_new::<u64>(value: value);
  return Row(words: words, owner: move owner);
}

fn exchange(values: &Ring<Row, 3>, first: u64, second: u64) -> result: unit writes(values) contract {
  requires first < second;
  requires second < deref(values).len;
  ensures deref(values).len == deref(entry(values)).len;
} {
  swap(first: &deref(values)[first], second: &deref(values)[second]);
  return unit;
}

fn same(values: &Ring<Row, 3>, index: u64) -> result: unit writes(values) contract {
  requires index < deref(values).len;
  ensures deref(values).len == deref(entry(values)).len;
} {
  swap(first: &deref(values)[index], second: &deref(values)[index]);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = ring_new::<Row, 3>();
  let first = make_row(value: 11_u64);
  place_back(window: &values, value: move first);
  let second = make_row(value: 22_u64);
  place_back(window: &values, value: move second);
  let third = make_row(value: 33_u64);
  place_back(window: &values, value: move third);
  let rotated = take_front(window: &values);
  place_back(window: &values, value: move rotated);
  exchange(values: &values, first: 0_u64, second: 2_u64);
  same(values: &values, index: 1_u64);
  if values[0_u64].owner.inner != 11_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if values[1_u64].owner.inner != 33_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if values[2_u64].owner.inner != 22_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  for (index in 0_u64..32_u64) {
    if values[0_u64].words[index] != 11_u64 {
      return std::process::exit_status(code: 4_u8);
    }
    if values[1_u64].words[index] != 33_u64 {
      return std::process::exit_status(code: 5_u8);
    }
    if values[2_u64].words[index] != 22_u64 {
      return std::process::exit_status(code: 6_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    let swap = super::emitted_prelude_row(&module, "swap");
    assert!(swap.contains("call void @llvm.memcpy."), "{swap}");
    assert!(!swap.contains("@llvm.memmove."), "{swap}");
    assert!(!swap.contains("@llvm.memcpy.inline."), "{swap}");
    assert!(!swap.lines().next().unwrap().contains("noalias"), "{swap}");
    assert_success(&module);
    assert_success(&retain_calls(&module));
}

/// A reference to an element, a reference to an element's field, a range
/// reference over a byte run and a reference to a sibling scalar all name
/// paths into the one owner's storage [REF-1, REF-4], and a write through each
/// lands there.
///
/// The two helpers that returned their reference argument retired with
/// [OWN-6] and [OWN-14]: [REF-3] refuses a returned reference outright, with
/// the restructuring `return an index and let the caller form the reference`.
/// A reference is a name for a path, so the caller forms it directly.
#[test]
fn element_and_range_references_reach_the_owners_storage() {
    let module = compile(
        br#"struct Row {
  left: u64;
  right: u64;
}

struct Table {
  rows: Array<Row, 2>;
  bytes: Array<u8, 2>;
  tag: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let loaded = slots_new::<Row, 2>();
  let first = Row(left: 3_u64, right: 5_u64);
  place_back(window: &loaded, value: first);
  let second = Row(left: 7_u64, right: 11_u64);
  place_back(window: &loaded, value: second);
  let rows = slots_into_array::<Row, 2>(values: move loaded);
  let bytes = array_filled::<u8, 2>(value: 13_u8);
  let table = Table(rows: rows, bytes: bytes, tag: 17_u64);
  let saved = &table.rows[0_u64];
  let base = deref(saved).left;
  let changed = &table.rows[1_u64].right;
  set deref(changed) = base +wrap 19_u64;
  let view = &table.bytes[0_u64..2_u64];
  set deref(view)[0_u64] = 23_u8;
  let sibling = &table.tag;
  set deref(sibling) = 29_u64;
  if table.rows[0_u64].left != 3_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if table.rows[0_u64].right != 5_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if table.rows[1_u64].left != 7_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if table.rows[1_u64].right != 22_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if table.bytes[0_u64] != 23_u8 {
    return std::process::exit_status(code: 5_u8);
  }
  if table.tag != 29_u64 {
    return std::process::exit_status(code: 6_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert_success(&module);
    assert_success(&retain_calls(&module));
}

#[test]
fn value_if_cleans_unchosen_owners_before_reusing_delivery_storage() {
    let module = compile(
        br#"struct Cell {
  value: Box<u64>;
}

fn choose(left: Cell, right: Cell, flag: Bool) -> result: u64 pure {
  let selected = if flag {
    let first = move left;
    let second = move right;
    give move first;
  } else {
    let first = move left;
    let second = move right;
    give move second;
  }
  let held = selected.value.inner;
  return held;
}

fn main() -> status: std::process::ExitStatus pure {
  for (round in 0_u64..2_u64) {
    let first_cell = box_new::<u64>(value: 11_u64);
    let second_cell = box_new::<u64>(value: 22_u64);
    let left = Cell(value: move first_cell);
    let right = Cell(value: move second_cell);
    let flag = round == 0_u64;
    let value = choose(left: move left, right: move right, flag: flag);
    let expected = if flag {
      give 11_u64;
    } else {
      give 22_u64;
    }
    if value != expected {
      return std::process::exit_status(code: 3_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    let observed = retain_calls(&module)
        .replace("@malloc(", "@wf_test_allocate(")
        .replace("@free(", "@wf_test_release(");
    let host = allocation_observer(4, 0);
    let output = compile_link_and_run(&observed, Some(&host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"A1;A2;F2;F1;A3;A4;F3;F4;");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// A struct owning a cell and a boxed window releases both fields in
/// declaration order [PROV-6, STOR-3], whether the owner is consumed early by
/// a function that ends or released at the scope exit of the function holding
/// it.
///
/// `dispose value;` retired: [PROV-6] states there is no release operation,
/// and an affine value is released early by moving it into a function that
/// consumes it and ends. That is what `consume` is here.
#[test]
fn owner_cleanup_releases_cell_fields_in_checked_order() {
    let module = compile(
        br#"struct Holder {
  cell: Box<u64>;
  bytes: Box<Slots<u8>>;
  stamp: u64;
}

fn touch(value: &u64) -> result: unit writes(value) {
  set deref(value) = 41_u64;
  return unit;
}

fn consume(value: Holder) -> result: u8 pure {
  return 0_u8;
}

fn release(value: Holder, early: Bool) -> result: u8 pure {
  touch(value: &value.stamp);
  if value.stamp != 41_u64 {
    return 2_u8;
  }
  if early {
    let code = consume(value: move value);
    return code;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  for (round in 0_u64..2_u64) {
    let cell = box_new::<u64>(value: 17_u64);
    let bytes = box_slots_new::<u8>(capacity: 3_u64);
    let holder = Holder(cell: move cell, bytes: move bytes, stamp: 0_u64);
    let early = round == 0_u64;
    let status = release(value: move holder, early: early);
    if status != 0_u8 {
      return std::process::exit_status(code: status);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    let observed = retain_calls(&module)
        .replace("@malloc(", "@wf_test_allocate(")
        .replace("@free(", "@wf_test_release(");
    let host = allocation_observer(4, 0);
    let output = compile_link_and_run(&observed, Some(&host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    // PROV-6 visits fields in declaration order. Both the early consume and
    // the scope exit must preserve the two cells' identity after a real
    // reference-taking helper call, without releasing either field twice.
    assert_eq!(output.stdout, b"A1;A2;F1;F2;A3;A4;F3;F4;");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn nested_and_residual_cleanup_preserve_release_graph_order() {
    // v0.60 has no block statement, so each scope whose exit the ledger reads
    // is its own function. The `region { .. }` wrapper that used to supply
    // that scope retired with regions [OWN-3, FORM-8].
    let module = compile(
        br#"struct Pair {
  first: Box<u64>;
  second: Box<u64>;
}

struct Triple {
  first: Box<u64>;
  selected: Box<u64>;
  tail: Box<u64>;
}

enum Packet {
  Held(pair: Pair, extra: Box<u64>);
  Empty();
}

fn hold_packet() -> result: unit pure {
  let first = box_new::<u64>(value: 11_u64);
  let second = box_new::<u64>(value: 22_u64);
  let pair = Pair(first: move first, second: move second);
  let extra = box_new::<u64>(value: 33_u64);
  let packet = Packet::Held(pair: move pair, extra: move extra);
  return unit;
}

fn hold_triple() -> result: u8 pure {
  let first = box_new::<u64>(value: 44_u64);
  let selected = box_new::<u64>(value: 55_u64);
  let tail = box_new::<u64>(value: 66_u64);
  let triple = Triple(first: move first, selected: move selected, tail: move tail);
  let retained = move triple.selected;
  let held = retained.inner;
  if held != 55_u64 {
    return 1_u8;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  hold_packet();
  let code = hold_triple();
  return std::process::exit_status(code: code);
}
"#,
    );
    let observed = retain_calls(&module)
        .replace("@malloc(", "@wf_test_allocate(")
        .replace("@free(", "@wf_test_release(");
    let host = allocation_observer(6, 0);
    let output = compile_link_and_run(&observed, Some(&host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    // The active payload walks its nested struct before the next payload
    // field. A move out of one field consumes the whole owner [WIN-3] and
    // releases only the residual fields, in their declaration order; the
    // moved-out owner remains live to its own scope exit.
    assert_eq!(output.stdout, b"A1;A2;A3;F1;F2;F3;A4;A5;A6;F4;F6;F5;");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn nested_box_content_take_releases_selected_path_and_residuals_in_order() {
    let module = compile(
        br#"nocopy struct Payload {
  value: u8;
}

nocopy struct Inner {
  selected: Box<Payload>;
  tail: Box<u8>;
}

struct Outer {
  head: Box<Inner>;
  other: Box<u8>;
}

fn main() -> status: std::process::ExitStatus pure {
  let payload = Payload(value: 7_u8);
  let selected = box_new::<Payload>(value: move payload);
  let tail = box_new::<u8>(value: 2_u8);
  let inner = Inner(selected: move selected, tail: move tail);
  let head = box_new::<Inner>(value: move inner);
  let other = box_new::<u8>(value: 4_u8);
  let outer = Outer(head: move head, other: move other);
  let taken = move outer.head.inner.selected.inner;
  if taken.value != 7_u8 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    let observed = retain_calls(&module)
        .replace("@malloc(", "@wf_test_allocate(")
        .replace("@free(", "@wf_test_release(");
    let output = compile_link_and_run(&observed, Some(&allocation_observer(4, 0)), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"A1;A2;A3;A4;F1;F2;F3;F4;");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// Observe only source allocations, without changing the host floor allocator.
/// A duplicate or unknown release aborts instead of allowing a use-after-free
/// to appear successful because its bytes happened to remain unchanged.
pub(super) fn allocation_observer(limit: usize, refused: usize) -> String {
    allocation_observer_body(limit, &refused.to_string(), false)
}

/// For fixtures whose allocations all contain one u64, identify releases by
/// the stored value rather than the order in which worker allocations arrive.
pub(super) fn u64_allocation_observer(limit: usize) -> String {
    allocation_observer_body(limit, "0", true)
}

pub(super) fn allocation_observer_by_process(limit: usize) -> String {
    let body = allocation_observer_body(limit, "wf_test_refusal()", false);
    format!(
        r#"#include <stdlib.h>
static unsigned long wf_test_refusal(void) {{
    const char *text = getenv("WF_TEST_REFUSE_ALLOCATION");
    char *end = NULL;
    if (text == NULL || *text == '\0') abort();
    unsigned long value = strtoul(text, &end, 10);
    if (*end != '\0' || value > {limit}) abort();
    return value;
}}
{body}"#
    )
}

fn allocation_observer_body(limit: usize, refused: &str, observe_u64_payload: bool) -> String {
    let slots = limit + 1;
    let release_record = if observe_u64_payload {
        "uint64_t value; memcpy(&value, allocation, sizeof(value)); printf(\"V%\" PRIu64 \";\", value);"
    } else {
        "printf(\"F%u;\", id);"
    };
    format!(
        r#"#include <stddef.h>
#include <inttypes.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void *held[{slots}];
static unsigned attempts;
static atomic_flag observer_lock = ATOMIC_FLAG_INIT;

static void lock_observer(void) {{
    while (atomic_flag_test_and_set_explicit(&observer_lock, memory_order_acquire)) {{}}
}}

static void unlock_observer(void) {{
    atomic_flag_clear_explicit(&observer_lock, memory_order_release);
}}

void *wf_test_allocate(size_t size) {{
    lock_observer();
    unsigned id = ++attempts;
    if (id > {limit}) abort();
    if (id == {refused}) {{
        printf("X%u;", id);
        unlock_observer();
        return NULL;
    }}
    void *allocation = malloc(size);
    if (allocation == NULL) abort();
    held[id] = allocation;
    printf("A%u;", id);
    unlock_observer();
    return allocation;
}}

void wf_test_release(void *allocation) {{
    lock_observer();
    for (unsigned id = 1; id <= attempts && id <= {limit}; ++id) {{
        if (allocation != NULL && held[id] == allocation) {{
            held[id] = NULL;
            {release_record}
            free(allocation);
            unlock_observer();
            return;
        }}
    }}
    abort();
}}
"#
    )
}

/// A front removal and back placement wrap a Ring, whose logical
/// reads and drain must yield 20, 30, and 40 [TYPE-9, WIN-1, OP-10]. The helper
/// consumes each whole Box while returning its copy scalar, so scope cleanup
/// frees that cell before the next source step [WIN-3]. The process-tag trace
/// checks that the removed owner leaves before A5, the remaining owners leave
/// in logical order, and the outer cell leaves last. The runtime shape drains
/// explicitly; a boxed inline shape leaves a wrapped partial window to cleanup.
#[test]
fn boxed_runtime_ring_wraps_and_releases_each_owner_in_order() {
    let source = include_str!("../../../../tests/programs/runtime_ring_wrap.wf");
    let fixed = source.replace(
        "  let ring = box_ring_new::<Box<u64>>(capacity: 3_u64);",
        r#"  let empty = ring_new::<Box<u64>, 3>();
  let ring = box_new::<Ring<Box<u64>, 3>>(value: move empty);
  if ring.inner.len != 0_u64 {
    return std::process::exit_status(code: 12_u8);
  }
  if ring.inner.head != 0_u64 {
    return std::process::exit_status(code: 13_u8);
  }"#,
    );
    assert_ne!(fixed, source);
    let (partial, _) = fixed
        .split_once("  let second_owner = take_front(window: &ring.inner);")
        .expect("the original control drains a second element");
    let partial = format!("{partial}  return std::process::exit_status(code: 0_u8);\n}}\n");
    let host = allocation_observer(5, 0).replace(
        "held[id] = allocation;",
        "memset(allocation, 0xa5, size);\n    held[id] = allocation;",
    );
    for source in [source, partial.as_str()] {
        let module = compile(source.as_bytes());
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let output = compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(
            output.stdout, b"A1;A2;A3;A4;F2;A5;F3;F4;F5;F1;",
            "{output:?}"
        );
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// A full bounded append returns its input rather than overwriting an element.
///
/// The arena half of this case retired with [STOR-4]: there is one heap
/// [STOR-8] and one cell release [STOR-3], so the same helper over a second
/// store class has no successor to contrast against.
#[test]
fn boxed_window_bounded_append_preserves_storage_and_elements() {
    let source = br#"struct Checked {
  storage: Box<Slots<Box<u64>, 2>>;
  code: u8;
}

fn add_one(storage: Box<Slots<Box<u64>, 2>>, value: Box<u64>) -> result: Box<Slots<Box<u64>, 2>> pure contract {
  requires storage.inner.len < storage.inner.cap;
} {
  place_back(window: &storage.inner, value: move value);
  return move storage;
}

fn try_append(storage: Box<Slots<Box<u64>, 2>>, value: Box<u64>) -> (result: Box<Slots<Box<u64>, 2>>, returned: Option<Box<u64>>) pure {
  let filled = storage.inner.len;
  if filled < 2_u64 {
    let updated = add_one(storage: move storage, value: move value);
    return move updated, None<Box<u64>>();
  }
  return move storage, Some<Box<u64>>(value: move value);
}

fn inspect(values: Slots<Box<u64>, 2>) -> code: u8 pure {
  let length = values.len;
  if length != 2_u64 {
    return 4_u8;
  }
  let second = take_back(window: &values);
  let first = take_back(window: &values);
  let first_value = first.inner;
  let second_value = second.inner;
  if first_value != 17_u64 {
    return 5_u8;
  }
  if second_value != 29_u64 {
    return 6_u8;
  }
  return 0_u8;
}

fn exercise(storage: Box<Slots<Box<u64>, 2>>) -> result: Checked pure {
  let first = box_new::<u64>(value: 17_u64);
  let (one, first_returned) = try_append(storage: move storage, value: move first);
  match first_returned {
    None() => {
    }
    Some(value: unwanted) => {
      return Checked(storage: move one, code: 1_u8);
    }
  }
  let second = box_new::<u64>(value: 29_u64);
  let (two, second_returned) = try_append(storage: move one, value: move second);
  match second_returned {
    None() => {
    }
    Some(value: unwanted) => {
      return Checked(storage: move two, code: 2_u8);
    }
  }
  let third = box_new::<u64>(value: 41_u64);
  let (full, third_returned) = try_append(storage: move two, value: move third);
  match third_returned {
    None() => {
      return Checked(storage: move full, code: 3_u8);
    }
    Some(value: refused) => {
      let refused_value = refused.inner;
      if refused_value != 41_u64 {
        return Checked(storage: move full, code: 7_u8);
      }
      return Checked(storage: move full, code: 0_u8);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let empty = slots_new::<Box<u64>, 2>();
  let cell = box_new::<Slots<Box<u64>, 2>>(value: move empty);
  if cell.inner.len != 0_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  let result = exercise(storage: move cell);
  let Checked(storage: storage, code: code) = move result;
  if code != 0_u8 {
    return std::process::exit_status(code: code);
  }
  let values = move storage.inner;
  let inspected = inspect(values: move values);
  return std::process::exit_status(code: inspected);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let host = allocation_observer(4, 0).replace(
            "held[id] = allocation;",
            "memset(allocation, 0xa5, size);\n    held[id] = allocation;",
        );
        let output = compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        // The refused element leaves first. Moving the content out of the cell
        // frees the cell alone [TYPE-9]; inspecting then consumes both
        // remaining elements.
        assert_eq!(output.stdout, b"A1;A2;A3;A4;F4;F1;F2;F3;", "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// The owning enum-slot put over its whole ledger.
///
/// The four allocation-refusal rows retired with [BLK-2]: the ported corpus
/// case's own doc states that allocation is total [STOR-8], so the wrapper has
/// no refusal arm and there is no writer-reachable refusal edge to script.
#[test]
fn owning_map_put_releases_each_displaced_and_remaining_payload_once() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/run-exclusive-owning-map-put.wf");
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = retain_calls(&super::emit_lowered(source, overlap))
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        // Three cells, one per `allocate_put`: A1 is key 1's payload (id 11),
        // A2 is key 3's (id 22), and A3 is the replacement offered for key 1
        // (id 33). The observer's limit is that same three, so a fourth
        // allocation would abort rather than be absorbed.
        //
        // Keys 1 and 3 collide at home slot 1, so key 1 takes slot 1 and
        // key 3 probes on to slot 0. The third put matches key 1 and the old
        // occupant leaves by swap [OP-11]: `check_put` binds it as `old`,
        // reads its id, and the binder's own release frees it — F1 — before
        // `main` reaches its exit at all.
        //
        // `main`'s exit then releases the window's slots in ascending logical
        // index order [STOR-3]: slot 0 holds key 3's cell, which is F2, and
        // slot 1 holds the replacement, which is F3.
        let observer = allocation_observer(3, 0);
        let output = compile_link_and_run(&module, Some(&observer), &[]);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        assert_eq!(
            output.stdout, b"A1;A2;A3;F1;F2;F3;",
            "{overlap:?}: {output:?}"
        );
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// An argument reference lasts exactly through its call, sequential calls to
/// the same declared write path are permitted, and owned results stay alive
/// [REF-1, REF-2, EFF-2].
#[test]
fn argument_references_and_owned_results_stay_alive_across_sequential_calls() {
    let source = include_bytes!(
        "../../../../tests/conformance/cases/own6-pos-statement-children-use-a-longer-local-region.wf"
    );
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        assert_success(&module);
        assert_success(&retain_calls(&module));
    }
}

/// A reference into a `Box`'s content names a path through the cell's field
/// `inner` [TYPE-9, REF-1] and addresses the one heap object, so a callee's
/// write through it lands in the allocation.
#[test]
fn referencing_owned_box_content_addresses_the_allocation() {
    let source = br#"struct Pair {
  left: u64;
  right: u64;
}

fn write(value: &u64, fresh: u64) -> result: unit writes(value) {
  set deref(value) = fresh;
  return unit;
}

fn read(value: &u64) -> result: u64 reads(value) {
  return deref(value);
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = Pair(left: 11_u64, right: 29_u64);
  let owner = box_new::<Pair>(value: pair);
  write(value: &owner.inner.left, fresh: 37_u64);
  let observed = read(value: &owner.inner.right);
  if observed != 29_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if owner.inner.left != 37_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  let held = &owner.inner.right;
  write(value: held, fresh: 43_u64);
  if owner.inner.right != 43_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let output = compile_link_and_run(&observed, Some(&allocation_observer(1, 0)), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, b"A1;F1;", "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// A contract over a referenced window still reads the caller's own cell
/// content when the reference names the `Box` field `inner` [TYPE-9, REF-1].
#[test]
fn boxed_window_contracts_keep_the_content_projection_through_a_holder() {
    let source = br#"fn first(values: &Slots<u64, 2>) -> result: u64 reads(values) contract {
  requires deref(values).len > 0_u64;
} {
  return deref(values)[0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  let initial = array_filled::<u64, 2>(value: 29_u64);
  let values = slots_from_array::<u64, 2>(values: initial);
  let owner = box_new::<Slots<u64, 2>>(value: move values);
  let size = owner.inner.len;
  if size > 0_u64 {
    let held = &owner.inner;
    let observed = first(values: held);
    if observed == 29_u64 {
      return std::process::exit_status(code: 0_u8);
    }
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 2_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let output = compile_link_and_run(&observed, Some(&allocation_observer(1, 0)), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, b"A1;F1;", "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// A [SET-1] commit over a directly named binding releases exactly the owner
/// it displaces: the one the target held, once, at the commit, and nothing
/// when the target's type has no release action.
///
/// [WIN-3]: "Assigning over any owned place releases the old value when it is
/// affine". [STOR-3] sites it and bounds it: "A successful [SET-1] assignment
/// derives no finalizer or cleanup edge and no release beyond the old value's
/// own: a copy target's previous value needs none, and an affine target's
/// previous value takes the release [WIN-3] states." The same rule fixes what
/// happens with no commit at all -- "Release actions run on every source
/// control-flow edge that leaves their owner scope, in reverse declaration
/// order" -- which is the first body's control trace.
///
/// So the three bodies below differ in exactly one statement and their ledgers
/// differ in exactly the way the two rules say:
///
/// - no commit: both cells live to the scope exit and are released newest
///   first, carrying values 22 and 11;
/// - a commit over a live affine binding: the displaced first cell is released
///   at the commit and the installed second at the scope exit, values 11 and 22;
/// - a commit over a `u64` binding: the previous value "needs none", so the
///   program's one cell, value 11, is released once at the scope exit.
///
/// The first body's allocations stay independent under [PAR-1]. Allocation
/// order cannot identify its source owners: the observer instead records each
/// released Box<u64>'s payload, while still checking the allocation limit and
/// rejecting unknown or duplicate frees. Its own shared ledger is synchronized.
///
/// A lowering that derived the release from the target's type alone would
/// still produce the first two and would free nothing extra in the third; one
/// that emitted no release for a named binding leaks the first cell of the
/// second body and records only the payload 22.
#[test]
fn a_commit_over_a_named_binding_releases_exactly_the_owner_it_displaces() {
    let program = |body: &str, expected: &str| {
        format!(
            r#"fn probe() -> result: u64 pure {{
{body}
}}

fn main() -> status: std::process::ExitStatus pure {{
  let total = probe();
  if total != {expected} {{
    return std::process::exit_status(code: 1_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
        )
    };
    let cases = [
        (
            program(
                "  let cell = box_new::<u64>(value: 11_u64);\n  \
                 let fresh = box_new::<u64>(value: 22_u64);\n  \
                 let seen = cell.inner;\n  \
                 let other = fresh.inner;\n  \
                 return seen +wrap other;",
                "33_u64",
            ),
            2,
            vec!["V22", "V11"],
        ),
        (
            program(
                "  let cell = box_new::<u64>(value: 11_u64);\n  \
                 let fresh_value = cell.inner +wrap 11_u64;\n  \
                 let fresh = box_new::<u64>(value: fresh_value);\n  \
                 set cell = move fresh;\n  \
                 let seen = cell.inner;\n  \
                 return seen;",
                "22_u64",
            ),
            2,
            vec!["V11", "V22"],
        ),
        (
            program(
                "  let cell = box_new::<u64>(value: 11_u64);\n  \
                 let seen = cell.inner;\n  \
                 set seen = 22_u64;\n  \
                 return seen;",
                "22_u64",
            ),
            1,
            vec!["V11"],
        ),
    ];
    for (source, limit, trace) in cases {
        for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
            let module = super::emit_lowered(source.as_bytes(), overlap);
            let observed = retain_calls(&module)
                .replace("@malloc(", "@wf_test_allocate(")
                .replace("@free(", "@wf_test_release(");
            let observer = allocation_observer_body(limit, "0", true);
            let output = compile_link_and_run(&observed, Some(&observer), &[]);
            assert_eq!(output.status.code(), Some(0), "{source}\n{output:?}");
            let records = std::str::from_utf8(&output.stdout)
                .expect("observer emits ASCII")
                .split_terminator(';')
                .collect::<Vec<_>>();
            assert_eq!(
                records
                    .iter()
                    .filter(|record| record.starts_with('A'))
                    .count(),
                limit,
                "{source}\n{output:?}"
            );
            assert_eq!(
                records
                    .iter()
                    .copied()
                    .filter(|record| record.starts_with('V'))
                    .collect::<Vec<_>>(),
                trace,
                "{source}\n{output:?}"
            );
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}

#[test]
fn discarded_struct_results_release_nested_cells_once() {
    for statement in ["make();", "let held = make();"] {
        let source = format!(
            r#"struct Inner {{
  value: Box<u64>;
}}

struct Outer {{
  first: Box<u64>;
  inner: Inner;
}}

fn make() -> result: Outer pure {{
  let first = box_new::<u64>(value: 11_u64);
  let second = box_new::<u64>(value: 22_u64);
  let inner = Inner(value: move second);
  return Outer(first: move first, inner: move inner);
}}

fn main() -> status: std::process::ExitStatus pure {{
  {statement}
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
        for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
            let module = super::emit_lowered(source.as_bytes(), overlap);
            let observed = retain_calls(&module)
                .replace("@malloc(", "@wf_test_allocate(")
                .replace("@free(", "@wf_test_release(");
            let output = compile_link_and_run(&observed, Some(&u64_allocation_observer(2)), &[]);
            assert_eq!(output.status.code(), Some(0), "{statement}: {output:?}");
            let records = std::str::from_utf8(&output.stdout)
                .expect("observer emits ASCII")
                .split_terminator(';')
                .filter(|record| record.starts_with('V'))
                .collect::<Vec<_>>();
            assert_eq!(records, ["V11", "V22"], "{statement}: {output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}

#[test]
fn boxed_field_consumes_skip_empty_residuals_and_release_nested_cells() {
    let source = br#"nocopy enum Tag {
  Present();
}

struct Residual {
  tag: Tag;
  cell: Box<u64>;
}

struct Holder {
  stamp: u64;
  selected: Box<u64>;
  tag: Tag;
  residual: Residual;
}

fn main() -> status: std::process::ExitStatus pure {
  let selected = box_new::<u64>(value: 17_u64);
  let other = box_new::<u64>(value: 29_u64);
  let residual_tag = Tag::Present();
  let residual = Residual(tag: move residual_tag, cell: move other);
  let tag = Tag::Present();
  let holder = Holder(stamp: 43_u64, selected: move selected, tag: move tag, residual: move residual);
  let owner = box_new::<Holder>(value: move holder);
  let taken = move owner.inner.selected;
  if taken.inner != 17_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap);
        let observed = retain_calls(&module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let output = compile_link_and_run(&observed, Some(&u64_allocation_observer(3)), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let records = std::str::from_utf8(&output.stdout)
            .expect("observer emits ASCII")
            .split_terminator(';')
            .collect::<Vec<_>>();
        let released = records
            .iter()
            .copied()
            .filter(|record| record.starts_with('V'))
            .collect::<Vec<_>>();
        // Payload identity is independent of parallel allocation order:
        // release the residual, then the outer cell, then the moved result.
        assert_eq!(released, ["V29", "V43", "V17"], "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}
