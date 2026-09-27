//! Measure placement follows owned storage [MSR-3], while mutable facts die
//! with the storage a write may replace [ENT-5, MSR-2].

use super::{assert_accepts, assert_rule_kind};
use crate::{SemanticIssueKind, SemanticRule};

#[test]
fn destructuring_a_boxed_window_preserves_its_empty_measure() {
    let source = br#"struct GrowVector<T> {
  storage: Box<Slots<T>>;
}

fn release<T>(values: GrowVector<T>) -> result: unit pure contract {
  requires values.storage.inner.len <= 0_u64;
} {
  let GrowVector(storage: storage) = move values;
  free_empty(window: move storage);
  return unit;
}
"#;
    assert_accepts(source);
}

#[test]
fn boxed_window_measures_cross_rebinding_construction_and_payload_selection() {
    let source = br#"struct Wrapper {
  storage: Box<Ring<u8>>;
  stamp: u64;
}

fn carry(window: Box<Ring<u8>>) -> result: unit pure contract {
  requires window.inner.len == 0_u64;
  requires window.inner.cap == 4_u64;
  requires window.inner.head == 2_u64;
} {
  let renamed = move window;
  invariant empty: renamed.inner.len == 0_u64;
  invariant capacity: renamed.inner.cap == 4_u64;
  invariant head: renamed.inner.head == 2_u64;
  let target = box_ring_new::<u8>(capacity: 1_u64);
  set target = move renamed;
  let wrapper = Wrapper(storage: move target, stamp: 0_u64);
  let transferred = move wrapper;
  set transferred.stamp = 1_u64;
  let selected = Some<Box<Ring<u8>>>(value: move transferred.storage);
  match selected {
    None() => {
    }
    Some(value: returned) => {
      invariant retained_capacity: returned.inner.cap == 4_u64;
      invariant retained_head: returned.inner.head == 2_u64;
      free_empty(window: move returned);
    }
  }
  return unit;
}
"#;
    assert_accepts(source);
}

#[test]
fn a_projected_box_move_preserves_the_selected_fields_measures() {
    let source = br#"struct Wrapper {
  storage: Box<Slots<u8>>;
}

fn carry(wrapper: Wrapper) -> result: unit pure contract {
  requires wrapper.storage.inner.len <= 0_u64;
} {
  let storage = move wrapper.storage;
  free_empty(window: move storage);
  return unit;
}

fn replace(target: Wrapper, source: Wrapper) -> result: unit pure contract {
  requires source.storage.inner.len <= 0_u64;
} {
  set target.storage = move source.storage;
  free_empty(window: move target.storage);
  return unit;
}
"#;
    assert_accepts(source);
}

#[test]
fn taking_nested_box_content_preserves_its_measures() {
    let source = br#"struct Wrapper {
  storage: Box<Slots<u8>>;
}

fn unbox(outer: Box<Box<Slots<u8>>>) -> result: unit pure contract {
  requires outer.inner.inner.len <= 0_u64;
} {
  let storage = move outer.inner;
  free_empty(window: move storage);
  return unit;
}

fn destructure(outer: Box<Wrapper>) -> result: unit pure contract {
  requires outer.inner.storage.inner.len <= 0_u64;
} {
  let Wrapper(storage: storage) = move outer.inner;
  free_empty(window: move storage);
  return unit;
}

fn take_inline(outer: Box<Slots<u8, 4>>) -> result: unit pure contract {
  requires outer.inner.len <= 0_u64;
} {
  let storage = move outer.inner;
  free_empty(window: move storage);
  return unit;
}

fn construct_inside(outer: Box<Wrapper>, empty: Box<Slots<u8>>) -> result: unit pure contract {
  requires empty.inner.len <= 0_u64;
} {
  set outer.inner = Wrapper(storage: move empty);
  invariant filled_content: outer.inner.storage.inner.len == 0_u64;
  let Wrapper(storage: storage) = move outer.inner;
  free_empty(window: move storage);
  return unit;
}
"#;
    assert_accepts(source);
}

#[test]
fn a_written_element_placement_preserves_box_content_measures() {
    let source = br#"struct Wrapper {
  storage: Box<Slots<u8>>;
}

fn carry() -> result: unit pure {
  let initial = box_slots_new::<u8>(capacity: 1_u64);
  let table = slots_new::<Box<Slots<u8>>, 1>();
  place_back(window: &table, value: move initial);
  let empty = box_slots_new::<u8>(capacity: 3_u64);
  set table[0_u64] = move empty;
  invariant empty_element: table[0_u64].inner.len == 0_u64;
  invariant element_capacity: table[0_u64].inner.cap == 3_u64;
  return unit;
}

fn construct_element() -> result: unit pure {
  let initial = box_slots_new::<u8>(capacity: 1_u64);
  let wrapper = Wrapper(storage: move initial);
  let table = slots_new::<Wrapper, 1>();
  place_back(window: &table, value: move wrapper);
  let empty = box_slots_new::<u8>(capacity: 3_u64);
  set table[0_u64] = Wrapper(storage: move empty);
  invariant empty_element: table[0_u64].storage.inner.len == 0_u64;
  invariant element_capacity: table[0_u64].storage.inner.cap == 3_u64;
  return unit;
}
"#;
    assert_accepts(source);
}

#[test]
fn an_overwrite_before_or_after_box_placement_kills_the_old_measure() {
    for statements in [
        "set original = move replacement;\n  let renamed = move original;",
        "let renamed = move original;\n  set renamed = move replacement;",
    ] {
        let source = format!(
            r#"fn release(original: Box<Slots<u8>>, replacement: Box<Slots<u8>>) -> result: unit pure contract {{
  requires original.inner.len <= 0_u64;
}} {{
  {statements}
  free_empty(window: move renamed);
  return unit;
}}
"#
        );
        assert_rule_kind(source.as_bytes(), SemanticRule::Op14, |_| true);
    }
}

#[test]
fn a_declared_write_before_or_after_box_placement_kills_the_old_measure() {
    for statements in [
        "fill(window: &original);\n  let renamed = move original;",
        "let renamed = move original;\n  fill(window: &renamed);",
    ] {
        let source = format!(
            r#"fn fill(window: &Box<Slots<u8>>) -> result: unit writes(window) {{
  let filled = box_slots_new::<u8>(capacity: 1_u64);
  place_back(window: &filled.inner, value: 7_u8);
  set deref(window) = move filled;
  return unit;
}}

fn release(original: Box<Slots<u8>>) -> result: unit pure contract {{
  requires original.inner.len <= 0_u64;
}} {{
  {statements}
  free_empty(window: move renamed);
  return unit;
}}
"#
        );
        assert_rule_kind(source.as_bytes(), SemanticRule::Op14, |_| true);
    }
}

// The second and third selected windows lie beyond repeated Chain and Box
// nominal identities. Merely stopping the type walk at its first cycle loses
// their distinct facts. Ordinary constructors and set targets establish the
// complete paths without relying on a call to publish its Box content.
const RECURSIVE_BOX_MEASURES: &str = r#"enum Chain {
  End();
  Next(window: Box<Ring<u8>>, tail: Box<Chain>);
}

fn carry(third_window: Box<Ring<u8>>) -> result: unit pure contract {
  requires third_window.inner.len == 2_u64;
  requires third_window.inner.cap == 8_u64;
  requires third_window.inner.head == 3_u64;
} {
  let end = Chain::End();
  let end_box = box_new::<Chain>(value: move end);
  let deepest_value = Chain::End();
  let deepest = box_new::<Chain>(value: move deepest_value);
  set deepest.inner = Chain::Next(window: move third_window, tail: move end_box);
  let second_window = box_ring_new::<u8>(capacity: 4_u64);
  place_back(window: &second_window.inner, value: 7_u8);
  let middle_value = Chain::End();
  let middle_box = box_new::<Chain>(value: move middle_value);
  set middle_box.inner = Chain::Next(window: move second_window, tail: move deepest);
  let first_window = box_ring_new::<u8>(capacity: 4_u64);
  let root = Chain::Next(window: move first_window, tail: move middle_box);
  let moved = move root;
  match moved {
    End() => {
    }
    Next(window: first, tail: middle) => {
      free_empty(window: move first);
      match move middle.inner {
        End() => {
        }
        Next(window: second, tail: last) => {
          invariant second_length: second.inner.len == 1_u64;
          match move last.inner {
            End() => {
            }
            Next(window: third, tail: remaining) => {
              invariant third_length: third.inner.len == 2_u64;
              invariant third_capacity: third.inner.cap == 8_u64;
              invariant third_head: third.inner.head == 3_u64;
            }
          }
        }
      }
    }
  }
  return unit;
}
"#;

#[test]
fn recursive_box_placements_preserve_distinct_measures_beyond_two_cycles() {
    assert_accepts(RECURSIVE_BOX_MEASURES.as_bytes());
}

#[test]
fn recursive_box_placement_preserves_capacity_and_head_without_length_evidence() {
    let source = RECURSIVE_BOX_MEASURES
        .replace("  requires third_window.inner.len == 2_u64;\n", "")
        .replace(
            "              invariant third_length: third.inner.len == 2_u64;\n",
            "",
        );
    assert_accepts(source.as_bytes());
}

#[test]
fn recursive_box_placement_does_not_revive_an_overwritten_descendants_measure() {
    let source = RECURSIVE_BOX_MEASURES.replace(
        "  let second_window =",
        r#"  let replacement_window = box_ring_new::<u8>(capacity: 4_u64);
  let replacement_end = Chain::End();
  let replacement_tail = box_new::<Chain>(value: move replacement_end);
  set deepest.inner = Chain::Next(window: move replacement_window, tail: move replacement_tail);
  let second_window ="#,
    );
    assert_rule_kind(source.as_bytes(), SemanticRule::Inv1, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedLocalInvariant { name, .. }
            if name == "third_length")
    });
}

#[test]
fn a_recursive_descendant_cursors_fact_is_not_the_owners_fact() {
    let source = br#"struct Node {
  window: Box<Slots<u8>>;
  next: Option<Box<Node>>;
}

fn examine(root: Node) -> result: unit pure {
  let cursor = &root;
  for (i in 0_u64..2_u64) {
    match deref(cursor).next {
      Some(value: child) => {
        set cursor = &deref(child).inner;
      }
      None() => {
        break;
      }
    }
  }
  if deref(cursor).window.inner.len == 0_u64 {
    let moved = move root;
    free_empty(window: move moved.window);
    return unit;
  }
  return unit;
}
"#;
    assert_rule_kind(source, SemanticRule::Op14, |_| true);
}

#[test]
fn a_call_through_a_joined_reference_kills_every_possible_targets_fact() {
    let source = br#"fn write(target: &u64) -> result: unit writes(target) {
  set deref(target) = 1_u64;
  return unit;
}

fn examine(flag: u64) -> result: unit pure {
  let a = 0_u64;
  let b = 0_u64;
  let p = if flag == 1_u64 {
    give &a;
  } else {
    give &b;
  }
  if b == 0_u64 {
    write(target: p);
    invariant stale: b == 0_u64;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  examine(flag: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Inv1, |_| true);
}

#[test]
fn a_joined_reference_write_cannot_preserve_a_nonzero_divisor_proof() {
    let source = br#"fn zero(target: &u64) -> result: unit writes(target) {
  set deref(target) = 0_u64;
  return unit;
}

fn examine(flag: u64) -> result: u64 pure {
  let a = 1_u64;
  let b = 1_u64;
  let p = if flag == 1_u64 {
    give &a;
  } else {
    give &b;
  }
  if b != 0_u64 {
    zero(target: p);
    return 1_u64 / b;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let quotient = examine(flag: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Op2, |_| true);
}

#[test]
fn replacing_an_indexed_window_kills_its_old_length() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let row = slots_new::<u8, 4>();
  place_back(window: &row, value: 7_u8);
  let table = slots_new::<Slots<u8, 4>, 2>();
  place_back(window: &table, value: move row);
  let index = 0_u64;
  if table[index].len == 1_u64 {
    let blank = slots_new::<u8, 4>();
    set table[index] = move blank;
    let observed = table[index][0_u64];
    return std::process::exit_status(code: observed);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Op4, |_| true);
}

#[test]
fn a_written_proof_cannot_reuse_a_replaced_elements_length() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let row = slots_new::<u8, 4>();
  place_back(window: &row, value: 7_u8);
  let table = slots_new::<Slots<u8, 4>, 2>();
  place_back(window: &table, value: move row);
  let index = 0_u64;
  if table[index].len == 1_u64 {
    let blank = slots_new::<u8, 4>();
    set table[index] = move blank;
    invariant stale: table[index].len == 1_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Inv1, |_| true);
}

#[test]
fn replacing_a_distinct_element_preserves_the_measured_element() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let row = slots_new::<u8, 4>();
  place_back(window: &row, value: 7_u8);
  let other = slots_new::<u8, 4>();
  let table = slots_new::<Slots<u8, 4>, 2>();
  place_back(window: &table, value: move row);
  place_back(window: &table, value: move other);
  if table[0_u64].len == 1_u64 {
    let initial = table[0_u64][0_u64];
    let blank = slots_new::<u8, 4>();
    set table[1_u64] = move blank;
    let observed = table[0_u64][0_u64];
    return std::process::exit_status(code: observed);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

#[test]
fn changing_length_preserves_a_runtime_capacity_fact() {
    let source = br#"fn check_capacity(value: u64) -> result: unit pure contract {
  requires value == 4_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = box_slots_new::<u8>(capacity: 4_u64);
  place_back(window: &window.inner, value: 7_u8);
  check_capacity(value: window.inner.cap);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}
