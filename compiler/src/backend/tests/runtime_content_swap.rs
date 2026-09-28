//! Runtime-capacity content exchange and the references which survive it.
//! The native oracles supply padded valid owners, so the old header-only
//! exchange produces a wrong observation without reading beyond an allocation.

use super::owned_places::{allocation_observer, retain_calls};
use super::{compile_link_and_run, compile_rejection, emit_lowered};
use crate::OverlapLowering;

const EMPTY_SWAP: &[u8] = br#"fn main() -> status: std::process::ExitStatus pure {
  let empty = box_slots_new::<u64>(capacity: 0_u64);
  let full = box_slots_new::<u64>(capacity: 1_u64);
  place_back(window: &full.inner, value: 7_u64);
  let content = &empty.inner;
  let earlier = content;
  swap(first: content, second: &full.inner);
  swap(first: earlier, second: content);
  if earlier^.cap != 1_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if earlier^.len != 1_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if earlier^[0_u64] != 7_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if full.inner.cap != 0_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if full.inner.len != 0_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn runtime_content_swap_moves_the_complete_zero_capacity_value() {
    let host = allocation_observer(2, 0).replace(
        "void *allocation = malloc(size);",
        "void *allocation = malloc(size + 64);\n    if (allocation != NULL) memset(allocation, 0xa5, size + 64);",
    );
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let module = retain_calls(&emit_lowered(EMPTY_SWAP, overlap))
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let output = compile_link_and_run(&module, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let text = std::str::from_utf8(&output.stdout).unwrap();
        let mut records = text.split_terminator(';').collect::<Vec<_>>();
        records.sort_unstable();
        assert_eq!(records, ["A1", "A2", "F1", "F2"], "{output:?}");
    }
}

fn scalar_source() -> String {
    let mut source = String::from(
        r#"struct Shelf {
  cells: Array<Box<Slots<u64>>, 3>;
}

fn interchange_array<T>(first: &Box<Array<T>>, second: &Box<Array<T>>) -> result: unit writes(first.inner), writes(second.inner) {
  swap(first: &first^.inner, second: &second^.inner);
  return unit;
}

fn generic_array(first: &Box<Array<u64>>, second: &Box<Array<u64>>) -> result: u64 writes(first.inner), writes(second.inner) {
  let earlier = &first^.inner;
  interchange_array::<u64>(first: first, second: second);
  let hash = earlier^.len;
  for (index in 0_u64..earlier^.len) {
    let scaled = hash *wrap 257_u64;
    set hash = scaled +wrap earlier^[index];
  }
  return hash;
}

"#,
    );
    for (name, shape) in [("slots", "Slots"), ("ring", "Ring")] {
        let head = if shape == "Ring" {
            "  let head_prefix = hash *wrap 257_u64;\n  set hash = head_prefix +wrap earlier^.head;\n"
        } else {
            ""
        };
        source.push_str(&format!(
            r#"fn interchange_{name}<T>(first: &Box<{shape}<T>>, second: &Box<{shape}<T>>) -> result: unit writes(first.inner), writes(second.inner) {{
  swap(first: &first^.inner, second: &second^.inner);
  return unit;
}}

fn {name}_exchange(first: &Box<{shape}<u64>>, second: &Box<{shape}<u64>>, choice: Bool) -> result: u64 writes(first.inner), writes(second.inner) {{
  let content = &first^.inner;
  let earlier = content;
  let selected = if choice {{
    give earlier;
  }} else {{
    give &first^.inner;
  }}
  let rebound = &second^.inner;
  set rebound = &first^.inner;
  interchange_{name}::<u64>(first: first, second: second);
  swap(first: selected, second: rebound);
  let hash = earlier^.cap *wrap 257_u64;
  set hash = hash +wrap earlier^.len;
{head}  for (index in 0_u64..earlier^.len) {{
    let scaled = hash *wrap 257_u64;
    set hash = scaled +wrap earlier^[index];
  }}
  return hash;
}}

"#
        ));
    }
    source.push_str(
        r#"fn nested_exchange(shelf: &Shelf, first: u64, second: u64, choice: Bool) -> result: u64 writes(shelf) contract {
  requires first < second;
  requires second < 3_u64;
} {
  return slots_exchange(first: &shelf^.cells[first], second: &shelf^.cells[second], choice: choice);
}

fn captured_pair(first: &Box<Slots<u64>>, second: &Box<Slots<u64>>, third: &Box<Slots<u64>>, fourth: &Box<Slots<u64>>) -> result: u64 writes(first.inner), writes(second.inner), writes(third.inner), writes(fourth.inner) {
  let yes = 0_u64 == 0_u64;
  let no = 0_u64 != 0_u64;
  let left = slots_exchange(first: first, second: second, choice: yes);
  let right = slots_exchange(first: third, second: fourth, choice: no);
  let scaled = left *wrap 257_u64;
  return scaled +wrap right;
}

fn whole_read(values: &Box<Slots<u64>>) -> result: u64 reads(values) {
  let scaled = values^.inner.cap *wrap 257_u64;
  return scaled +wrap values^.inner.len;
}

fn blocked_pair(first: &Box<Slots<u64>>, second: &Box<Slots<u64>>) -> result: u64 reads(first), writes(first.inner), writes(second.inner) {
  let yes = 0_u64 == 0_u64;
  let left = slots_exchange(first: first, second: second, choice: yes);
  let right = whole_read(values: first);
  let scaled = left *wrap 257_u64;
  return scaled +wrap right;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    source
}

const SCALAR_OBSERVER: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
typedef struct { uint64_t len, cap, words[8]; } Slots;
typedef struct { uint64_t len, cap, head, words[8]; } Ring;
typedef struct { uint64_t len, words[8]; } Array;
typedef struct { Slots *cells[3]; } Shelf;
extern uint64_t wf_generic_array(Array **, Array **);
extern uint64_t wf_slots_exchange(Slots **, Slots **, uint8_t);
extern uint64_t wf_ring_exchange(Ring **, Ring **, uint8_t);
extern uint64_t wf_nested_exchange(Shelf *, uint64_t, uint64_t, uint8_t);
extern uint64_t wf_captured_pair(Slots **, Slots **, Slots **, Slots **);
extern uint64_t wf_blocked_pair(Slots **, Slots **);
static void fail(const char *reason) { fprintf(stderr, "%s\n", reason); exit(93); }
static uint64_t expected(uint64_t cap, uint64_t len, uint64_t head, uint64_t base, int ring) {
  uint64_t hash = cap * 257 + len;
  if (ring) hash = hash * 257 + head;
  for (uint64_t i = 0; i < len; ++i) hash = hash * 257 + base + i;
  return hash;
}
static void initialize_slots(Slots *p, uint64_t cap, uint64_t len, uint64_t base) {
  memset(p, 0xa5, sizeof(*p)); p->cap = cap; p->len = len;
  for (uint64_t i = 0; i < len; ++i) p->words[i] = base + i;
}
static void initialize_ring(Ring *p, uint64_t cap, uint64_t len, uint64_t head, uint64_t base) {
  memset(p, 0xa5, sizeof(*p)); p->cap = cap; p->len = len; p->head = head;
  for (uint64_t i = 0; i < len; ++i) p->words[(head + i) % cap] = base + i;
}
int main(int argc, char **argv) {
  int fault = argc > 1 ? atoi(argv[1]) : 0;
  const uint64_t cases[][6] = {{0,0,0,1,1,0},{1,1,0,4,3,3},{3,2,2,2,2,1}};
  unsigned count = 0;
  for (unsigned row = 0; row < 3; ++row) for (uint8_t choice = 0; choice < 2; ++choice) {
    const uint64_t *c = cases[row];
    Slots a, b, untouched; initialize_slots(&a,c[0],c[1],11); initialize_slots(&b,c[3],c[4],71);
    Slots *left = &a, *right = &b;
    uint64_t wanted = expected(c[3],c[4],0,71,0);
    if (fault == 1) ++wanted;
    if (wf_slots_exchange(&left,&right,choice) != wanted) fail("complete content or whole alias");
    if (wf_slots_exchange(&left,&right,choice) != expected(c[0],c[1],0,11,0)) fail("exchange round trip");
    initialize_slots(&untouched,2,1,151);
    Shelf shelf = {{left,&untouched,right}};
    if (wf_nested_exchange(&shelf,0,2,choice) != expected(c[3],c[4],0,71,0)
        || shelf.cells[1]->cap != 2 || shelf.cells[1]->len != 1 || shelf.cells[1]->words[0] != 151)
      fail("selected nested owner or sibling");
    Ring x,y; initialize_ring(&x,c[0],c[1],c[2],31); initialize_ring(&y,c[3],c[4],c[5],101);
    Ring *front = &x, *back = &y;
    if (wf_ring_exchange(&front,&back,choice) != expected(c[3],c[4],c[5],101,1)) fail("wrapped content or head");
    if (wf_ring_exchange(&front,&back,choice) != expected(c[0],c[1],c[2],31,1)) fail("wrapped exchange round trip");
    Array u={0},v={0}; u.len=c[1]; v.len=c[4];
    for (uint64_t i=0;i<u.len;++i) u.words[i]=11+i;
    for (uint64_t i=0;i<v.len;++i) v.words[i]=71+i;
    Array *au=&u,*av=&v;
    if (wf_generic_array(&au,&av) != expected(0,c[4],0,71,0)) fail("generic copy instance contents");
    ++count;
  }
  Slots a,b,c,d; initialize_slots(&a,1,1,11); initialize_slots(&b,3,2,71);
  initialize_slots(&c,0,0,31); initialize_slots(&d,4,3,101);
  Slots *p=&a,*q=&b,*r=&c,*s=&d;
  if (wf_captured_pair(&p,&q,&r,&s) != expected(3,2,0,71,0)*257+expected(4,3,0,101,0))
    fail("captured owner references");
  if (wf_blocked_pair(&p,&q) != expected(1,1,0,11,0)*257+258)
    fail("whole-root sequencing");
  printf("runtime content: %u alias, nested and wrapped cases\n",count);
  return 0;
}
"#;

#[test]
fn runtime_content_swap_preserves_aliases_joins_and_nested_wrapped_values() {
    let source = scalar_source();
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let module =
            emit_lowered(source.as_bytes(), overlap).replace("@main(", "@wf_fixture_main(");
        for module in [module.clone(), retain_calls(&module)] {
            let output = compile_link_and_run(&module, Some(SCALAR_OBSERVER), &[]);
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(
                output.stdout,
                b"runtime content: 6 alias, nested and wrapped cases\n"
            );
            assert!(output.stderr.is_empty(), "{output:?}");
        }
        let output = compile_link_and_run(&module, Some(SCALAR_OBSERVER), &[b"1"]);
        assert_eq!(output.status.code(), Some(93), "{output:?}");
        assert_eq!(output.stderr, b"complete content or whole alias\n");
    }
}

#[test]
fn runtime_content_swap_captures_only_disjoint_owner_references() {
    let source = scalar_source();
    super::system::with_parallel_ir(source.as_bytes(), |program| {
        for (name, expected) in [("captured_pair", 1), ("blocked_pair", 0)] {
            let function = program
                .functions()
                .iter()
                .find(|f| f.name() == name)
                .unwrap();
            assert_eq!(function.overlaps().len(), expected, "{name}");
        }
    });
    let module =
        emit_lowered(source.as_bytes(), OverlapLowering::On).replace("@main(", "@wf_fixture_main(");
    let observed = super::parallel::observe_worker_schedule(&module);
    let host = format!(
        "{}\n{}\n{}",
        SCALAR_OBSERVER,
        super::parallel::WORKER_SCHEDULE,
        super::parallel::GRANT_OBSERVER
    );
    let directory = super::test_directory();
    let executable = super::build_linked_executable(&observed, Some(&host), &[], &directory);
    let output = std::process::Command::new(executable)
        .env("WF_WORKERS", "2")
        .env_remove("WF_SCHED_REPORT")
        .output()
        .expect("run the real captured worker");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        output.stdout,
        b"runtime content: 6 alias, nested and wrapped cases\n"
    );
    let stderr = std::str::from_utf8(&output.stderr).unwrap();
    let grants = stderr
        .lines()
        .find_map(|line| line.strip_prefix("grants="))
        .unwrap();
    assert!(grants.parse::<u64>().unwrap() > 0, "{stderr}");
    std::fs::remove_dir_all(directory).expect("remove captured-owner test artifacts");
}

fn owning_source() -> String {
    let mut source = String::from(
        r#"nodrop struct Ticket {
  owner: Box<u64>;
}

fn consume_ticket(item: Ticket) -> result: u64 pure {
  let Ticket(owner: owner) = move item;
  return owner.inner;
}

fn release_array(values: Box<Array<Box<u64>>>) -> result: unit pure {
  return unit;
}

fn read_range(values: &[Box<u64>]) -> result: u64 reads(values) {
  let hash = values^.len;
  for (index in 0_u64..values^.len) {
    let scaled = hash *wrap 257_u64;
    set hash = scaled +wrap values^[index].inner;
  }
  return hash;
}

fn array_own(first: Box<Array<Box<u64>>>, second: Box<Array<Box<u64>>>) -> result: u64 pure {
  let earlier = &first.inner;
  swap(first: earlier, second: &second.inner);
  swap(first: earlier, second: &first.inner);
  let hash = earlier^.len;
  for (index in 0_u64..earlier^.len) {
    let scaled = hash *wrap 257_u64;
    set hash = scaled +wrap earlier^[index].inner;
  }
  let ranged = read_range(values: &earlier^[0_u64..earlier^.len]);
  release_array(values: move first);
  release_array(values: move second);
  let scaled = hash *wrap 257_u64;
  return scaled +wrap ranged;
}

"#,
    );
    for (name, shape) in [("slots", "Slots"), ("ring", "Ring")] {
        source.push_str(&format!(
            r#"fn drain_{name}(values: Box<{shape}<Ticket>>) -> result: u64 pure {{
  let hash = values.inner.len;
  loop @drain {{
    if values.inner.len == 0_u64 {{
      break @drain;
    }}
    let item = take_back(window: &values.inner);
    let value = consume_ticket(item: move item);
    let scaled = hash *wrap 257_u64;
    set hash = scaled +wrap value;
  }}
  free_empty(window: move values);
  return hash;
}}

fn {name}_own(first: Box<{shape}<Ticket>>, second: Box<{shape}<Ticket>>) -> result: u64 pure {{
  let earlier = &first.inner;
  swap(first: earlier, second: &second.inner);
  swap(first: earlier, second: &first.inner);
  let left = drain_{name}(values: move first);
  let right = drain_{name}(values: move second);
  let scaled = left *wrap 257_u64;
  return scaled +wrap right;
}}

"#
        ));
    }
    source.push_str("fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n");
    source
}

const OWNER_OBSERVER: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdatomic.h>
extern uint64_t wf_array_own(void *, void *);
extern uint64_t wf_slots_own(void *, void *);
extern uint64_t wf_ring_own(void *, void *);
static void *held[16];
static uint64_t ids[16], expected[16];
static unsigned live[16], allocated, released, expected_count;
static atomic_flag observer_lock = ATOMIC_FLAG_INIT;
static int fault;
static void fail(const char *reason) { fprintf(stderr, "%s\n", reason); exit(94); }
static void *allocate(size_t bytes, uint64_t id) {
  void *p = calloc(1, bytes); if (!p || allocated == 16) abort();
  held[allocated] = p; ids[allocated] = id; live[allocated++] = 1; return p;
}
void wf_release_observed(void *p) {
  if (fault == 2) return;
  while (atomic_flag_test_and_set_explicit(&observer_lock, memory_order_acquire)) {}
  unsigned index = 0; while (index != allocated && held[index] != p) ++index;
  if (index == allocated || !live[index]) fail("unknown or repeated release");
  unsigned expected_index = 0;
  while (expected_index != expected_count && expected[expected_index] != ids[index])
    ++expected_index;
  if (expected_index == expected_count) fail("owner identity or release ledger");
  expected[expected_index] = 0;
  live[index] = 0; ++released;
  atomic_flag_clear_explicit(&observer_lock, memory_order_release);
  free(p);
}
static void *make(unsigned shape, uint64_t cap, uint64_t len, uint64_t head, uint64_t base, uint64_t id) {
  uint64_t *block = allocate(128, id); block[0] = len;
  unsigned header = shape == 0 ? 1 : shape == 1 ? 2 : 3;
  if (shape != 0) block[1] = cap;
  if (shape == 2) block[2] = head;
  void **items = (void **)(block + header);
  for (uint64_t i = 0; i < len; ++i) {
    uint64_t *value = allocate(sizeof(*value), base + i); *value = base + i;
    items[shape == 2 ? (head + i) % cap : i] = value;
  }
  return block;
}
static uint64_t sequence(uint64_t len, uint64_t base, int reverse) {
  uint64_t hash = len;
  for (uint64_t i = 0; i < len; ++i) hash = hash * 257 + base + (reverse ? len - 1 - i : i);
  return hash;
}
static void expect(uint64_t len, uint64_t base, uint64_t id, int reverse) {
  for (uint64_t i = 0; i < len; ++i) expected[expected_count++] = base + (reverse ? len - 1 - i : i);
  expected[expected_count++] = id;
}
int main(int argc, char **argv) {
  fault = argc > 1 ? atoi(argv[1]) : 0;
  unsigned count = 0;
  for (unsigned shape = 0; shape != 3; ++shape) for (unsigned empty = 0; empty != 2; ++empty) {
    allocated = released = expected_count = 0; memset(live, 0, sizeof(live));
    uint64_t left_len = empty ? 0 : 2, right_len = 3;
    void *left = make(shape,empty ? 0 : 2,left_len,empty ? 0 : 1,11,101);
    void *right = make(shape,4,right_len,3,71,201);
    expect(right_len,71,201,shape != 0); expect(left_len,11,101,shape != 0);
    if (fault == 3) ++expected[0];
    uint64_t want = shape == 0 ? sequence(right_len,71,0) * 258
      : sequence(right_len,71,1) * 257 + sequence(left_len,11,1);
    if (fault == 1) ++want;
    uint64_t actual = shape == 0 ? wf_array_own(left,right)
      : shape == 1 ? wf_slots_own(left,right) : wf_ring_own(left,right);
    if (actual != want) fail("owned content value or order");
    if (released != expected_count) fail("owner release count");
    for (unsigned i = 0; i < allocated; ++i) if (live[i]) fail("unreleased owner");
    ++count;
  }
  printf("runtime content: %u owning Array and linear window cases\n",count);
  return 0;
}
"#;

#[test]
fn runtime_content_swap_preserves_owning_arrays_and_linear_cleanup() {
    let source = owning_source();
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let module = emit_lowered(source.as_bytes(), overlap)
            .replace("@main(", "@wf_fixture_main(")
            .replace("@free(", "@wf_release_observed(");
        for (_retained, module) in [(false, module.clone()), (true, retain_calls(&module))] {
            let output = compile_link_and_run(&module, Some(OWNER_OBSERVER), &[]);
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(
                output.stdout,
                b"runtime content: 6 owning Array and linear window cases\n"
            );
            assert!(output.stderr.is_empty(), "{output:?}");
        }
        for (fault, reason) in [
            (
                b"1".as_slice(),
                b"owned content value or order\n".as_slice(),
            ),
            (b"2".as_slice(), b"owner release count\n".as_slice()),
            (
                b"3".as_slice(),
                b"owner identity or release ledger\n".as_slice(),
            ),
        ] {
            let output = compile_link_and_run(&module, Some(OWNER_OBSERVER), &[fault]);
            assert_eq!(output.status.code(), Some(94), "{output:?}");
            assert_eq!(output.stderr, reason, "{output:?}");
        }
    }
}

#[test]
fn runtime_content_swap_owning_cleanup_reaches_a_real_worker() {
    let source = owning_source();
    let module = emit_lowered(source.as_bytes(), OverlapLowering::On)
        .replace("@main(", "@wf_fixture_main(")
        .replace("@free(", "@wf_release_observed(");
    let observed = super::parallel::observe_worker_schedule(&module);
    let host = format!(
        "{}\n{}\n{}",
        OWNER_OBSERVER,
        super::parallel::WORKER_SCHEDULE,
        super::parallel::GRANT_OBSERVER
    );
    let directory = super::test_directory();
    let executable = super::build_linked_executable(&observed, Some(&host), &[], &directory);
    let output = std::process::Command::new(executable)
        .env("WF_WORKERS", "2")
        .output()
        .expect("run owning cleanup on a real worker");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        output.stdout,
        b"runtime content: 6 owning Array and linear window cases\n"
    );
    let stderr = std::str::from_utf8(&output.stderr).unwrap();
    let grants = stderr
        .lines()
        .find_map(|line| line.strip_prefix("grants="))
        .unwrap();
    assert!(grants.parse::<u64>().unwrap() > 0, "{stderr}");
    std::fs::remove_dir_all(directory).expect("remove owning-worker test artifacts");
}

#[test]
fn runtime_content_swap_invalidates_element_and_range_descendants() {
    for (shape, reference, observation) in [
        ("Slots<u64>", "&first^.inner[0_u64]", "stale^"),
        ("Slots<u64>", "&first^.inner[0_u64..1_u64]", "stale^.len"),
        ("Ring<u64>", "&first^.inner[0_u64]", "stale^"),
        ("Array<Box<u64>>", "&first^.inner[0_u64]", "stale^.inner"),
        (
            "Array<Box<u64>>",
            "&first^.inner[0_u64..1_u64]",
            "stale^.len",
        ),
    ] {
        let source = format!(
            r#"fn inspect(first: &Box<{shape}>, second: &Box<{shape}>) -> result: u64 writes(first.inner), writes(second.inner) contract {{
  requires first^.inner.len > 0_u64;
}} {{
  let stale = {reference};
  swap(first: &first^.inner, second: &second^.inner);
  return {observation};
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
        let failure = compile_rejection(source.as_bytes());
        assert_eq!(failure.rule_id(), Some("REF-2"), "{source}\n{failure}");
    }
}

#[test]
fn runtime_content_swap_preserves_copy_spelling_and_runtime_value_rules() {
    for (parameter, place, row) in [
        ("Box<Array<u64>>", "values.inner", "pure"),
        ("&Box<Array<u64>>", "values^.inner", "reads(values)"),
    ] {
        let source = format!(
            "fn inspect(values: {parameter}) -> result: unit {row} {{\n  let content = {place};\n  return unit;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        let failure = compile_rejection(source.as_bytes());
        assert_eq!(failure.rule_id(), Some("TYPE-9"), "{source}\n{failure}");
    }
    for (parameter, place) in [
        ("Box<Array<u64>>", "move values.inner"),
        ("&Box<Array<u64>>", "move values^.inner"),
    ] {
        let source = format!(
            "fn inspect(values: {parameter}) -> result: unit pure {{\n  let moved = {place};\n  return unit;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        let failure = compile_rejection(source.as_bytes());
        assert_eq!(failure.rule_id(), Some("TYPE-9"), "{source}\n{failure}");
    }
    for parameter in ["u64", "T"] {
        let generic = if parameter == "T" { "<T: copy>" } else { "" };
        let source = format!(
            "fn invalid{generic}(first: &Box<Array<{parameter}>>, second: &Box<Array<{parameter}>>) -> result: unit writes(first.inner), writes(second.inner) {{\n  swap(first: &first^.inner, second: &second^.inner);\n  return unit;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        let failure = compile_rejection(source.as_bytes());
        assert_eq!(failure.rule_id(), Some("OP-11"), "{source}\n{failure}");
    }
}
