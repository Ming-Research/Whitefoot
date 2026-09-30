//! Compiler-independent owner/order observations for the closed terminal
//! permutation, plus accepted near-misses which must keep ordinary lowering.

use crate::{IrInstruction, IrOperation, OverlapLowering};

use super::{
    compile_link_and_run, emit_lowered,
    system::{with_ir, with_mutated_ir_lowering},
};

const RECORDS: &str = r#"nodrop struct Parcel {
  owner: Box<u64>;
}

nocopy struct EmptyOwner {
  words: Array<u64, 0>;
}

struct Ledger {
  count: u64;
  sequence: u64;
}

fn receive(log: &Ledger, item: Parcel) -> result: unit writes(log) {
  let Parcel(owner: owner) = move item;
  set log^.count = log^.count +wrap 1_u64;
  let shifted = log^.sequence *wrap 257_u64;
  set log^.sequence = shifted +wrap owner.inner;
  return unit;
}

fn receive_other(log: &Ledger, item: Parcel) -> result: unit writes(log) {
  receive(log: log, item: move item);
  return unit;
}

fn receive_zero(log: &Ledger, item: EmptyOwner) -> result: unit writes(log) {
  set log^.count = log^.count +wrap 1_u64;
  return unit;
}

fn record_length(log: &Ledger, length: u64) -> result: unit writes(log) {
  let shifted = log^.sequence *wrap 257_u64;
  set log^.sequence = shifted +wrap length;
  return unit;
}

"#;

fn retire(name: &str, ty: &str, window: &str, consumer: &str) -> String {
    format!(
        r#"fn {name}(items: &{ty}, keep: u64, log: &Ledger) -> result: unit writes(items), writes(log) contract {{
  requires keep <= {window}.len;
  ensures {window}.len == keep;
}} {{
  let original = {window}.len;
  let removed = original - keep;
  let middle = removed / 2_u64;
  for @exchange (
    offset in 0_u64..middle,
    invariant lower: {window}.len >= original - offset,
    invariant upper: {window}.len <= original - offset
  ) {{
    let front = keep + offset;
    invariant pair: {window}.len >= keep + offset + 2_u64 {{
      use lower;
      use (2_u64 * middle <= removed);
      use 2 times (offset < middle);
    }}
    let item = take_back(window: &{window});
    swap(first: &{window}[front], second: &item);
    {consumer}(log: log, item: move item);
  }}
  loop @remaining (
    invariant prefix: {window}.len >= keep
  ) {{
    if {window}.len <= keep {{
      invariant exhausted: {window}.len == keep;
      break @remaining;
    }}
    let item = take_back(window: &{window});
    {consumer}(log: log, item: move item);
  }}
  return unit;
}}

"#
    )
}

fn selected_count(source: &str) -> usize {
    with_ir(source.as_bytes(), |program| {
        program
            .functions()
            .iter()
            .flat_map(|function| function.blocks())
            .flat_map(|block| block.instructions())
            .filter(|instruction| {
                matches!(
                    instruction,
                    IrInstruction::Define {
                        operation: IrOperation::RunConsumeFinish { .. },
                        ..
                    }
                )
            })
            .count()
    })
}

fn program() -> String {
    let mut source = RECORDS.to_owned();
    source.push_str(&retire(
        "retire_slots",
        "Slots<Parcel, 8>",
        "items^",
        "receive",
    ));
    source.push_str(&retire(
        "retire_ring",
        "Ring<Parcel, 8>",
        "items^",
        "receive",
    ));
    source.push_str(&retire(
        "retire_runtime",
        "Box<Slots<Parcel>>",
        "items^.inner",
        "receive",
    ));
    source.push_str(&retire(
        "retire_zero",
        "Box<Ring<EmptyOwner>>",
        "items^.inner",
        "receive_zero",
    ));
    source.push_str(
        &retire("retire_observed", "Slots<Parcel, 8>", "items^", "receive").replacen(
            "    swap(first:",
            "    record_length(log: log, length: items^.len);\n    swap(first:",
            1,
        ),
    );
    source.push_str("fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n");
    source
}

// This observer supplies valid ABI values, derives the expected logical order
// independently, and checks every actual release against its original owner.
// It never reads a window from inside the callback/release hook.
const OBSERVER: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { uint64_t count, sequence; } Ledger;
typedef struct { uint64_t len; void *items[8]; } Slots;
typedef struct { uint64_t len, head; void *items[8]; } Ring;
typedef struct { uint64_t len, cap; void *items[8]; } Runtime;
typedef struct { uint64_t len, cap, head; } Zero;
extern uint8_t wf_retire_slots(Slots *, uint64_t, Ledger *);
extern uint8_t wf_retire_ring(Ring *, uint64_t, Ledger *);
extern uint8_t wf_retire_runtime(Runtime **, uint64_t, Ledger *);
extern uint8_t wf_retire_zero(Zero **, uint64_t, Ledger *);
extern uint8_t wf_retire_observed(Slots *, uint64_t, Ledger *);
static void *owners[8];
static uint64_t ids[8], expected[8], released, expected_count;
static unsigned live[8];
static int fault;
static Ledger *current_log;

static void fail(const char *reason) { fprintf(stderr, "%s\n", reason); exit(91); }
void wf_release_observed(void *owner) {
  if (fault == 2) return;
  if (fault == 3) ++current_log->count;
  if (current_log->count != released + 1) fail("callback release interleaving");
  unsigned i = 0;
  while (i != 8 && owners[i] != owner) ++i;
  if (i == 8 || !live[i]) fail("unknown or repeated owner release");
  if (released >= expected_count || ids[i] != expected[released]) fail("owner release order");
  live[i] = 0;
  ++released;
  free(owner);
}

int main(int argc, char **argv) {
  fault = argc > 1 ? atoi(argv[1]) : 0;
  uint64_t cases = 0;
  for (unsigned shape = 0; shape != 4; ++shape) {
    for (uint64_t n = 0; n <= 8; ++n) for (uint64_t keep = 0; keep <= n; ++keep) {
      for (uint64_t head = 0; head != (shape == 1 ? 8 : 1); ++head) {
        Slots slots = {0}; Ring ring = {0}; Runtime runtime = {0}; Runtime *boxed = &runtime;
        slots.len = ring.len = runtime.len = n; ring.head = head; runtime.cap = 8;
        memset(live, 0, sizeof(live)); released = 0; expected_count = n - keep;
        uint64_t hash = 0;
        for (uint64_t i = 0; i < n; ++i) {
          uint64_t *owner = malloc(sizeof(*owner)); if (!owner) abort();
          *owner = 101 + i; owners[i] = owner; ids[i] = *owner; live[i] = 1;
          slots.items[i] = runtime.items[i] = owner; ring.items[(head + i) % 8] = owner;
          if (i >= keep) {
            expected[i - keep] = *owner;
            if (shape == 3 && i - keep < (n - keep) / 2) hash = hash * 257 + n - 1 - (i - keep);
            hash = hash * 257 + *owner;
          }
        }
        if (fault == 1 && expected_count >= 2) {
          uint64_t temp = expected[0]; expected[0] = expected[1]; expected[1] = temp;
        }
        Ledger log = {0};
        current_log = &log;
        if (shape == 0) wf_retire_slots(&slots, keep, &log);
        else if (shape == 1) wf_retire_ring(&ring, keep, &log);
        else if (shape == 2) wf_retire_runtime(&boxed, keep, &log);
        else wf_retire_observed(&slots, keep, &log);
        if (released != expected_count) fail("owner release count");
        if (log.count != n - keep || log.sequence != hash) fail("callback count or order");
        if (slots.len != (shape == 0 || shape == 3 ? keep : n) || ring.len != (shape == 1 ? keep : n)
            || runtime.len != (shape == 2 ? keep : n) || ring.head != head || runtime.cap != 8)
          fail("window boundary or head");
        for (uint64_t i = 0; i < keep; ++i) {
          void *actual = shape == 0 || shape == 3 ? slots.items[i] : shape == 1 ? ring.items[(head + i) % 8] : runtime.items[i];
          if (actual != owners[i] || !live[i] || *(uint64_t *)actual != 101 + i) fail("retained prefix identity");
          free(actual); live[i] = 0;
        }
        for (unsigned i = 0; i < 8; ++i) if (live[i]) fail("unreleased owner");
        ++cases;
      }
    }
  }
  for (unsigned large = 0; large != 2; ++large) for (uint64_t removed = 0; removed <= 5; ++removed) {
    uint64_t length = large ? UINT64_C(9223372036854775811) : 5;
    Zero zero = {length, UINT64_MAX, UINT64_MAX - 2}; Zero *boxed = &zero;
    Ledger log = {0}; wf_retire_zero(&boxed, length - removed, &log);
    if (zero.len != length - removed || zero.cap != UINT64_MAX || zero.head != UINT64_MAX - 2
        || log.count != removed) fail("zero-stride logical count or wrapped head");
    ++cases;
  }
  printf("terminal consumption: %llu cases; owner order, prefix, head and zero-stride counts checked\n", (unsigned long long)cases);
  return 0;
}
"#;

#[test]
fn terminal_consumption_preserves_owners_prefix_and_wrapped_zero_stride_windows() {
    let source = program();
    assert_eq!(
        selected_count(&source),
        4,
        "renamed unrelated functions must select"
    );
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let module = emit_lowered(source.as_bytes(), overlap)
            .replace("@free(", "@wf_release_observed(")
            .replace("@main(", "@wf_fixture_main(");
        let output = compile_link_and_run(&module, Some(OBSERVER), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "terminal consumption: 507 cases; owner order, prefix, head and zero-stride counts checked\n"
        );
        for (fault, reason) in [
            (b"1".as_slice(), "owner release order\n"),
            (b"2".as_slice(), "owner release count\n"),
            (b"3".as_slice(), "callback release interleaving\n"),
        ] {
            let output = compile_link_and_run(&module, Some(OBSERVER), &[fault]);
            assert_eq!(output.status.code(), Some(91), "{output:?}");
            assert_eq!(String::from_utf8(output.stderr).unwrap(), reason);
        }
    }
}

#[test]
fn terminal_consumption_keeps_observers_different_consumers_and_partial_exits() {
    let base = retire("unrelated_name", "Slots<Parcel, 8>", "items^", "receive");
    for changed in [
        base.replace(
            "    swap(first:",
            "    record_length(log: log, length: items^.len);\n    swap(first:",
        ),
        base.replacen(
            "    receive(log: log, item: move item);",
            "    receive_other(log: log, item: move item);",
            1,
        ),
        base.replace("  ensures items^.len == keep;\n", "").replace(
            "  let original =",
            "  if keep == 0_u64 {\n    return unit;\n  }\n  let original =",
        ),
        base.replace(
            "  let original =",
            "  let borrowed = items;\n  let original =",
        ),
    ] {
        assert_eq!(
            selected_count(&format!("{RECORDS}{}\n", changed.trim_end())),
            0,
            "near-match must retain ordinary lowering"
        );
    }
}

fn waiting_consumption_source() -> String {
    let mut source = r#"nodrop struct Parcel {
  owner: Box<u64>;
}

fn parcel(value: u64) -> result: Parcel pure {
  let owner = box_new::<u64>(value: value);
  return Parcel(owner: move owner);
}

fn receive_waiting(log: u64, item: Parcel) -> result: unit pure waits {
  let Parcel(owner: owner) = move item;
  set owner.inner = owner.inner +wrap log;
  return unit;
}

"#
    .to_owned();
    source.push_str(
        &retire(
            "retire_waiting",
            "Slots<Parcel, 8>",
            "items^",
            "receive_waiting",
        )
        .replace("log: &Ledger", "log: u64")
        .replace(
            "writes(items), writes(log) contract",
            "writes(items) waits contract",
        ),
    );
    source.push_str(
        r#"fn main() -> status: std::process::ExitStatus pure waits {
  let items = slots_new::<Parcel, 8>();
  let first = parcel(value: 101_u64);
  place_back(window: &items, value: move first);
  let second = parcel(value: 102_u64);
  place_back(window: &items, value: move second);
  let third = parcel(value: 103_u64);
  place_back(window: &items, value: move third);
  retire_waiting(items: &items, keep: 0_u64, log: 1000_u64);
  free_empty(window: move items);
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    source
}

#[test]
fn terminal_consumption_keeps_waiting_context_starts_and_exit_join() {
    let ordinary = waiting_consumption_source();
    let started = ordinary.replace("    receive_waiting(", "    spawn receive_waiting(");
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        for (source, expected) in [(&ordinary, [1, 0, 0]), (&started, [0, 2, 1])] {
            with_mutated_ir_lowering(source.as_bytes(), overlap, |program| {
                let function = program
                    .functions()
                    .iter()
                    .find(|function| function.name() == "retire_waiting")
                    .expect("the waiting consumer");
                assert!(function.waits());
                let mut counts = [0; 3];
                for instruction in function
                    .blocks()
                    .iter()
                    .flat_map(|block| block.instructions())
                {
                    if let IrInstruction::Define { operation, .. } = instruction {
                        match operation {
                            IrOperation::RunConsumeFinish { .. } => counts[0] += 1,
                            IrOperation::ContextStart { .. } => counts[1] += 1,
                            IrOperation::ContextJoin => counts[2] += 1,
                            _ => {}
                        }
                    }
                }
                assert_eq!(
                    counts, expected,
                    "marked callbacks retain starts and exit join"
                );
            });
        }
    }
}

// Deliberately defer every context launch until the join, retaining only its
// argument block. This is the observer's schedule, not a source requirement
// against eager completion. Observe the captured values and cleanup owners,
// never the source window; each owner must then be updated and released once.
const WAITING_OBSERVER: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern void *wf__context_prepare(uint64_t);
extern void wf__context_launch(uint64_t *, void *, void *(*)(void *));
extern int wf__context_join_wait(uint64_t *, void *);
static unsigned starts, joins, released;
static struct {
  uint64_t bytes;
  uint64_t *group;
  void *arguments;
  void *(*start)(void *);
} deferred[3];

static void fail(const char *reason) {
  fprintf(stderr, "%s\n", reason);
  exit(91);
}

void *wf_prepare_observed(uint64_t bytes) {
  if (starts >= 3 || deferred[starts].arguments != NULL) fail("context prepare order");
  deferred[starts].bytes = bytes;
  deferred[starts].arguments = malloc((size_t)bytes);
  if (deferred[starts].arguments == NULL) fail("observer allocation");
  return deferred[starts].arguments;
}

void wf_launch_observed(uint64_t *group, void *arguments, void *(*start)(void *)) {
  if (starts >= 3 || arguments != deferred[starts].arguments) fail("context capture order");
  deferred[starts].group = group;
  deferred[starts].start = start;
  ++starts;
}

int wf_join_observed(uint64_t *group, void *frame) {
  if (starts != 3 || joins != 0 || released != 0) fail("context capture or join order");
  ++joins;
  for (unsigned index = 0; index < starts; ++index) {
    if (deferred[index].group != group) fail("context group changed");
    void *arguments = wf__context_prepare(deferred[index].bytes);
    memcpy(arguments, deferred[index].arguments, (size_t)deferred[index].bytes);
    free(deferred[index].arguments);
    wf__context_launch(group, arguments, deferred[index].start);
  }
  return wf__context_join_wait(group, frame);
}

void wf_release_observed(void *owner) {
  if (joins != 1) fail("owner released before deferred context launch");
  uint64_t value = *(uint64_t *)owner;
  if (value < 1101 || value > 1103) fail("captured owner or value changed");
  unsigned bit = 1u << (unsigned)(value - 1101);
  if ((released & bit) != 0) fail("owner released twice");
  released |= bit;
  free(owner);
  if (released == 7) puts("waiting owners: three starts, one join, three releases");
}
"#;

#[test]
fn terminal_consumption_waiting_owners_survive_until_context_join() {
    let source =
        waiting_consumption_source().replace("    receive_waiting(", "    spawn receive_waiting(");
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let module = emit_lowered(source.as_bytes(), overlap)
            .replace("@free(", "@wf_release_observed(")
            .replace("@wf__context_prepare(", "@wf_prepare_observed(")
            .replace("@wf__context_launch(", "@wf_launch_observed(")
            .replace("@wf__context_join_wait(", "@wf_join_observed(");
        let output = compile_link_and_run(&module, Some(WAITING_OBSERVER), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "waiting owners: three starts, one join, three releases\n"
        );
    }
}
