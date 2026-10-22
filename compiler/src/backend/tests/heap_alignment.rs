//! Heap storage for an element type of the largest language alignment, 16
//! bytes (`std::process::MemoryMeter`): placement, growth and release, with the
//! production allocator's returned pointers checked.

use super::{compile, compile_link_and_run};

const ALIAS: &str = "alias MemoryMeter = std::process::MemoryMeter;\n";

const PREFIX: &str = r#"fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure waits {
  let std::process::Inputs(cwd: directory, handles: factory, memory_meter: meter, ..) = move inputs;
  let std::fs::Directory(read: directory_read, write: directory_write) = move directory;
  std::fs::close_directory_write(factory: &factory, directory: move directory_write);
  std::fs::close_directory(factory: &factory, directory: move directory_read);
  let before = std::process::heap_in_use(meter: &meter);
"#;

const SUFFIX: &str = r#"  let after = std::process::heap_in_use(meter: &meter);
  if after != before {
    return std::process::exit_status(code: 90_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// Observe the production allocator's returned pointers, including after
/// growth, rather than infer alignment from a successful unaligned host load.
/// The 16 bytes are the alignment `target.rs` records for every heap block.
fn run(body: &str, declarations: &str, minimum_allocations: usize, minimum_retakes: usize) {
    // The canonical form keeps aliases contiguous, then one blank line before
    // the first function; `declarations` begin with their own aliases.
    let separator = if declarations.is_empty() { "\n" } else { "" };
    let source = format!("{ALIAS}{separator}{declarations}{PREFIX}{body}{SUFFIX}");
    let llvm = compile(source.as_bytes());
    let observed = llvm
        .replace("@wf__heap_take(", "@wf_observe_take(")
        .replace("@wf__heap_retake(", "@wf_observe_retake(")
        .replace("@main(", "@wf_fixture_main(");
    let host = format!(
        r#"#include "completion/bridge.h"
#include <stdint.h>
#include <stdlib.h>
extern int wf_fixture_main(int, char **);
static unsigned allocations, retakes;
static void require_aligned(void *block) {{
    if (block == NULL || (uintptr_t)block % 16 != 0) exit(98);
}}
void *wf_observe_take(uint64_t bytes) {{
    void *block = wf__heap_take(bytes);
    require_aligned(block);
    ++allocations;
    return block;
}}
void *wf_observe_retake(void *block, uint64_t old_bytes, uint64_t new_bytes) {{
    require_aligned(block);
    void *fresh = wf__heap_retake(block, old_bytes, new_bytes);
    require_aligned(fresh);
    ++retakes;
    return fresh;
}}
int main(int argc, char **argv) {{
    int status = wf_fixture_main(argc, argv);
    if (status != 0) return status;
    return allocations >= {minimum_allocations} && retakes >= {minimum_retakes} ? 0 : 99;
}}
"#
    );
    let output = compile_link_and_run(&observed, Some(&host), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn a_box_holds_a_meter_and_releases_its_exact_bytes() {
    run(
        r#"  let value = std::process::meter_share(meter: &meter);
  let cell = box_new::<MemoryMeter>(value: move value);
  let allocated = std::process::heap_in_use(meter: &cell.inner);
  let expected_allocated = before +wrap 32_u64;
  if allocated != expected_allocated {
    return std::process::exit_status(code: 1_u8);
  }
  release::<Box<MemoryMeter>>(value: move cell);
  let second = std::process::meter_share(meter: &meter);
  let consumed = box_new::<MemoryMeter>(value: move second);
  let taken = move consumed.inner;
  let released = std::process::heap_in_use(meter: &taken);
  if released != before {
    return std::process::exit_status(code: 2_u8);
  }
"#,
        "",
        2,
        0,
    );
}

#[test]
fn empty_meter_slots_use_the_shared_header_without_accounting_bytes() {
    run(
        r#"  let cells = box_slots_new::<MemoryMeter>(capacity: 0_u64);
  let allocated = std::process::heap_in_use(meter: &meter);
  if allocated != before {
    return std::process::exit_status(code: 1_u8);
  }
  free_empty(window: move cells);
  let reserved = box_slots_new::<MemoryMeter>(capacity: 1_u64);
  let reserved_bytes = std::process::heap_in_use(meter: &meter);
  let expected_reserved_bytes = before +wrap 48_u64;
  if reserved_bytes != expected_reserved_bytes {
    return std::process::exit_status(code: 2_u8);
  }
  free_empty(window: move reserved);
"#,
        "",
        1,
        0,
    );
}

#[test]
fn occupied_meter_slots_keep_values_and_accounting_across_both_growth_paths() {
    run(
        r#"  let cells = box_slots_new::<MemoryMeter>(capacity: 0_u64);
  grow(cell: &cells, capacity: 1_u64);
  let first = std::process::meter_share(meter: &meter);
  place_back(window: &cells.inner, value: move first);
  grow(cell: &cells, capacity: 33_u64);
  let large = std::process::heap_in_use(meter: &cells.inner[0_u64]);
  let expected_large = before +wrap 1072_u64;
  if large != expected_large {
    return std::process::exit_status(code: 1_u8);
  }
  grow(cell: &cells, capacity: 65_u64);
  let larger = std::process::heap_in_use(meter: &cells.inner[0_u64]);
  let expected_larger = before +wrap 2096_u64;
  if larger != expected_larger {
    return std::process::exit_status(code: 2_u8);
  }
  let taken = take_back(window: &cells.inner);
  let kept = std::process::heap_in_use(meter: &taken);
  if kept != larger {
    return std::process::exit_status(code: 3_u8);
  }
  free_empty(window: move cells);
"#,
        "",
        0,
        3,
    );
}

#[test]
fn a_meter_ring_pads_its_header_and_keeps_wrapped_elements() {
    run(
        r#"  let ring = box_ring_new::<MemoryMeter>(capacity: 2_u64);
  let first = std::process::meter_share(meter: &meter);
  place_front(window: &ring.inner, value: move first);
  let second = std::process::meter_share(meter: &meter);
  place_back(window: &ring.inner, value: move second);
  let allocated = std::process::heap_in_use(meter: &ring.inner[0_u64]);
  let expected_allocated = before +wrap 96_u64;
  if allocated != expected_allocated {
    return std::process::exit_status(code: 1_u8);
  }
  let front = take_front(window: &ring.inner);
  let back = take_back(window: &ring.inner);
  let front_bytes = std::process::heap_in_use(meter: &front);
  let back_bytes = std::process::heap_in_use(meter: &back);
  if front_bytes != allocated {
    return std::process::exit_status(code: 2_u8);
  }
  if back_bytes != allocated {
    return std::process::exit_status(code: 3_u8);
  }
  free_empty(window: move ring);
"#,
        "",
        1,
        0,
    );
}

#[test]
fn meter_paged_elements_survive_page_zero_growth_and_a_new_page() {
    run(
        r#"  let pages = box_paged_new::<MemoryMeter>(capacity: 0_u64);
  grow_paged(cell: &pages, capacity: 1_u64);
  let first = std::process::meter_share(meter: &meter);
  place_back(window: &pages.inner, value: move first);
  let one = std::process::heap_in_use(meter: &pages.inner[0_u64]);
  let expected_one = before +wrap 64_u64;
  if one != expected_one {
    return std::process::exit_status(code: 1_u8);
  }
  grow_paged(cell: &pages, capacity: 2_u64);
  let second = std::process::meter_share(meter: &meter);
  place_back(window: &pages.inner, value: move second);
  let two = std::process::heap_in_use(meter: &pages.inner[1_u64]);
  let expected_two = before +wrap 96_u64;
  if two != expected_two {
    return std::process::exit_status(code: 2_u8);
  }
  grow_paged(cell: &pages, capacity: 129_u64);
  let crossed = std::process::heap_in_use(meter: &pages.inner[0_u64]);
  let expected_crossed = before +wrap 8232_u64;
  if crossed != expected_crossed {
    return std::process::exit_status(code: 3_u8);
  }
  let last = take_back(window: &pages.inner);
  let surviving = std::process::heap_in_use(meter: &last);
  if surviving != crossed {
    return std::process::exit_status(code: 4_u8);
  }
  release::<Box<Paged<MemoryMeter>>>(value: move pages);
"#,
        "",
        4,
        0,
    );
}

const MAP_DECLARATIONS: &str = r#"alias HashMap = std::collections::hash_map::HashMap;
alias HashMapPut = std::collections::hash_map::HashMapPut;
alias HashMapKey = std::collections::hash_map::HashMapKey;
alias hash_map_new = std::collections::hash_map::hash_map_new;
alias hash_map_put = std::collections::hash_map::hash_map_put;
alias hash_map_remove = std::collections::hash_map::hash_map_remove;

fn key_hash(env: &unit, key: &u64) -> result: u64 reads(key) {
  return key^;
}

fn key_equal(env: &unit, left: &u64, right: &u64) -> result: Bool reads(left), reads(right) {
  return left^ == right^;
}

binding MeterKey : HashMapKey<u64, unit> {
  hash = key_hash;
  equal = key_equal;
}

"#;

#[test]
fn a_hash_map_stores_meters_and_preserves_them_when_rebuilt() {
    run(
        r#"  let map = hash_map_new::<u64, MemoryMeter, 8>(capacity: 0_u64);
  let env = unit;
  let first = std::process::meter_share(meter: &meter);
  let inserted = hash_map_put::<MeterKey, MemoryMeter, 8>(map: &map, key: 7_u64, value: move first, env: &env);
  release::<HashMapPut<u64, MemoryMeter>>(value: move inserted);
  let second = std::process::meter_share(meter: &meter);
  let grew = hash_map_put::<MeterKey, MemoryMeter, 8>(map: &map, key: 8_u64, value: move second, env: &env);
  release::<HashMapPut<u64, MemoryMeter>>(value: move grew);
  let key = 7_u64;
  let removed = hash_map_remove::<MeterKey, MemoryMeter, 8>(map: &map, key: &key, env: &env);
  match move removed {
    Some(value: pair) => {
      let from_map = std::process::heap_in_use(meter: &pair.value);
      let direct = std::process::heap_in_use(meter: &meter);
      if from_map != direct {
        return std::process::exit_status(code: 1_u8);
      }
    }
    None() => {
      return std::process::exit_status(code: 2_u8);
    }
  }
  release::<HashMap<u64, MemoryMeter, 8>>(value: move map);
"#,
        MAP_DECLARATIONS,
        2,
        0,
    );
}

#[test]
fn shared_objects_and_maps_accept_meter_state_and_entries() {
    run(
        r#"  let value = std::process::meter_share(meter: &meter);
  let object = shared_new::<MemoryMeter>(value: move value);
  let maps = shared_map_new::<MemoryMeter>(capacity: 1_u64);
  let key_bytes = array_filled::<u8, 1>(value: 97_u8);
  let key = &key_bytes[0_u64..1_u64];
  let payload = std::process::meter_share(meter: &meter);
  atomic map = &maps {
    set map^[key] = Some<MemoryMeter>(value: move payload);
    set map^[key] = None<MemoryMeter>();
  }
  release::<Shared<MemoryMeter>>(value: move object);
  release::<Shared<ConcurrentHashMap<MemoryMeter>>>(value: move maps);
"#,
        "",
        0,
        0,
    );
}
