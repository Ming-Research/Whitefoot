//! A result is a value at the call's return, even when its source subsequently
//! changes or dies. Raw entry-selection checks also catch dangling reads that
//! an allocator or native optimizer could otherwise hide in a value-only test.

use super::owned_places::retain_calls;
use super::{compile_and_run, emit, emitted_function};

const ITEMS: &str = r#"struct Item {
  a: u64;
  b: u64;
  c: u64;
  d: u64;
}

fn item_of(item: &Item) -> result: Item reads(item) {
  return item^;
}

fn item_at(items: &Box<Slots<Item>>, index: u64) -> result: Item reads(items) contract {
  requires index < items^.inner.len;
} {
  return items^.inner[index];
}
"#;

fn assert_public(module: &str, caller: &str, callee: &str) {
    let body = emitted_function(module, caller);
    assert!(
        body.contains(&format!("call void @wf_{callee}("))
            && !body.contains(&format!("call ptr @wf_{callee}.source(")),
        "{body}"
    );
}

fn assert_runs(module: &str) {
    // Keep the boundary in a second run: inlining must not conceal a source
    // address that points into a returned activation or released allocation.
    for native in [module.to_owned(), retain_calls(module)] {
        let output = compile_and_run(&native);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
    }
}

#[test]
fn a_result_outlives_its_dropped_or_moved_owner() {
    let source = format!(
        "{ITEMS}{}",
        r#"
struct Wrapper {
  cell: Box<Item>;
}

fn after_drop(owner: Box<Item>) -> result: u64 pure {
  let old = item_of(item: &owner.inner);
  release::<Box<Item>>(value: move owner);
  return old.a;
}

fn after_move(owner: Box<Item>) -> result: u64 pure {
  let old = item_of(item: &owner.inner);
  let wrapped = Wrapper(cell: move owner);
  set wrapped.cell.inner.a = 99_u64;
  let before = old.a;
  let after = wrapped.cell.inner.a;
  return before +wrap after;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = Item(a: 7_u64, b: 8_u64, c: 9_u64, d: 10_u64);
  let first = box_new::<Item>(value: value);
  let dropped = after_drop(owner: move first);
  if dropped != 7_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let second = box_new::<Item>(value: value);
  let moved = after_move(owner: move second);
  if moved != 106_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#
    );
    let module = emit(source.as_bytes());
    // The first read would dangle after release; the second would observe
    // 99 through the moved owner's new path instead of the saved 7.
    assert_public(&module, "after_drop", "item_of");
    assert_public(&module, "after_move", "item_of");
    assert_runs(&module);
}

#[test]
fn a_result_keeps_its_value_across_window_growth() {
    let source = format!(
        "{ITEMS}{}",
        r#"
fn after_grow(items: &Box<Slots<Item>>) -> result: u64 writes(items) contract {
  requires 0_u64 < items^.inner.len;
  requires items^.inner.cap <= 8_u64;
} {
  let old = item_at(items: items, index: 0_u64);
  grow(cell: items, capacity: 8_u64);
  return old.a;
}

fn main() -> status: std::process::ExitStatus pure {
  let items = box_slots_new::<Item>(capacity: 1_u64);
  if items.inner.len < items.inner.cap {
    let value = Item(a: 7_u64, b: 8_u64, c: 9_u64, d: 10_u64);
    place_back(window: &items.inner, value: value);
  }
  if 0_u64 < items.inner.len {
    if items.inner.cap <= 8_u64 {
      let old = after_grow(items: &items);
      if old != 7_u64 {
        return std::process::exit_status(code: 1_u8);
      }
      if items.inner.cap != 8_u64 {
        return std::process::exit_status(code: 2_u8);
      }
      return std::process::exit_status(code: 0_u8);
    }
  }
  return std::process::exit_status(code: 3_u8);
}
"#
    );
    let module = emit(source.as_bytes());
    // Growth itself is the only event between the call and field read, so
    // a later explicit store cannot accidentally supply the tested barrier.
    assert_public(&module, "after_grow", "item_at");
    assert_runs(&module);
}

#[test]
fn a_result_keeps_its_value_across_a_writing_call() {
    let source = format!(
        "{ITEMS}{}",
        r#"
fn replace(item: &Item) -> result: unit writes(item) {
  set item^.a = 99_u64;
  return unit;
}

fn after_call(item: &Item) -> result: u64 writes(item) {
  let old = item_of(item: item);
  replace(item: item);
  return old.a;
}

fn main() -> status: std::process::ExitStatus pure {
  let item = Item(a: 7_u64, b: 8_u64, c: 9_u64, d: 10_u64);
  let old = after_call(item: &item);
  if old != 7_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if item.a != 99_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#
    );
    let module = emit(source.as_bytes());
    // Dropping the write-effect barrier chooses .source and reads 99.
    assert_public(&module, "after_call", "item_of");
    assert_runs(&module);
}

#[test]
fn a_result_survives_the_end_of_an_atomic_hold() {
    let source = format!(
        "{ITEMS}{}",
        r#"
fn after_hold(held: Shared<Item>) -> result: u64 pure waits {
  let saved = Item(a: 0_u64, b: 0_u64, c: 0_u64, d: 0_u64);
  atomic state = &held {
    set saved = item_of(item: state);
  }
  atomic state = &held {
    set state^.a = 99_u64;
  }
  return saved.a;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let item = Item(a: 7_u64, b: 8_u64, c: 9_u64, d: 10_u64);
  let held = shared_new::<Item>(value: item);
  let old = after_hold(held: move held);
  if old != 7_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#
    );
    let module = emit(source.as_bytes());
    // SHARE-2 makes every atomic-containing function waiting. This source
    // regression covers that actual exclusion, not an unreachable ordinary
    // caller, and does not independently isolate the SharedUnlock barrier.
    assert_public(&module, "after_hold", "item_of");
    assert!(!module.contains("@wf_after_hold.source("), "{module}");
    assert_runs(&module);
}

#[test]
fn loop_carried_results_keep_previous_values_while_fresh_results_read_through() {
    let source = format!(
        "{ITEMS}{}",
        r#"
fn carried(item: &Item) -> result: u64 writes(item) {
  let last = Item(a: 3_u64, b: 0_u64, c: 0_u64, d: 0_u64);
  let total = 0_u64;
  for (i in 0_u64..3_u64) {
    set item^.a = i +wrap 7_u64;
    let current = item_of(item: item);
    let previous = last.a;
    set total = total +wrap previous;
    set last = current;
  }
  return total;
}

fn fresh(item: &Item) -> result: u64 writes(item) {
  let total = 0_u64;
  for (i in 0_u64..3_u64) {
    set item^.a = i +wrap 7_u64;
    let current = item_of(item: item);
    let observed = current.a;
    set total = total +wrap observed;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  let item = Item(a: 0_u64, b: 0_u64, c: 0_u64, d: 0_u64);
  let previous = carried(item: &item);
  if previous != 18_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let current = fresh(item: &item);
  if current != 24_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#
    );
    let module = emit(source.as_bytes());
    // The carried result observes 3 + 7 + 8, not 3 + 8 + 9. Its mixed-origin
    // loop parameter also prevents selection; this is a source-level carry
    // regression rather than an isolated test of the family's liveness guard.
    assert_public(&module, "carried", "item_of");
    // A fresh call begins a new interval on each iteration. Treating the
    // next iteration's store as part of that interval loses this source call.
    let fresh = emitted_function(&module, "fresh");
    assert!(fresh.contains("call ptr @wf_item_of.source("), "{fresh}");
    assert!(!fresh.contains("call void @wf_item_of("), "{fresh}");
    assert_runs(&module);
}

#[test]
fn a_forwarded_result_never_returns_a_local_destination_slot() {
    let source = format!(
        "{ITEMS}{}",
        r#"
fn sourced_or_built(item: &Item, build: Bool) -> result: Item reads(item) {
  if build {
    return Item(a: 31_u64, b: 32_u64, c: 33_u64, d: 34_u64);
  }
  return item^;
}

fn pick(item: &Item, first: Bool) -> result: Item reads(item) {
  let build = True();
  let left = sourced_or_built(item: item, build: build);
  let borrow = False();
  let right = sourced_or_built(item: item, build: borrow);
  if first {
    return left;
  }
  return right;
}

fn main() -> status: std::process::ExitStatus pure {
  let item = Item(a: 7_u64, b: 8_u64, c: 9_u64, d: 10_u64);
  let first = True();
  let built = pick(item: &item, first: first);
  if built.a != 31_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if built.d != 34_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  let second = False();
  let sourced = pick(item: &item, first: second);
  if sourced.a != 7_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if sourced.d != 10_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#
    );
    let module = emit(source.as_bytes());
    let pick = emitted_function(&module, "pick");
    // Both results are live at the branch, so their distinct slots cannot
    // both be this function's result destination. No return source may be
    // selected: the built branch's address would point into pick's frame.
    assert_eq!(
        pick.matches("call void @wf_sourced_or_built(").count(),
        2,
        "{pick}"
    );
    assert!(
        !pick.contains("call ptr @wf_sourced_or_built.source("),
        "{pick}"
    );
    assert!(
        !pick.contains("call void @wf_sourced_or_built(ptr %wf.result,"),
        "{pick}"
    );
    let entry = emitted_function(&module, "pick.source");
    assert!(
        entry.contains("call void @wf_pick(ptr %wf.result,"),
        "{entry}"
    );
    assert!(entry.contains("ret ptr %wf.result"), "{entry}");
    assert_runs(&module);
}
