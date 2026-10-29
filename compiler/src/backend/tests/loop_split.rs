//! Actualization of a permitted counted loop [PAR-2 candidate]: what a split
//! loop emits, what each world runs, and what the program observes.
//!
//! The property this module exists to check is the same one the pair path makes —
//! overlapping changes nothing observable — with one thing added that the pair
//! path never does: the *shape* of the combination tree is chosen by the
//! runtime rather than written in the source. So the byte comparisons here span
//! worker counts that produce genuinely different trees, and the differential
//! against the unsplit lowering is what says the tree does not matter.
//!
//! A run at `WF_WORKERS=1` proves nothing about overlapping — it takes the
//! sequential world — so the cases that need real overlap read the runtime's
//! own grant count and refuse a repeat that never actually handed anything out.
//!
//! # Kernel spec v0.60
//!
//! No test in this module retired. Every subject here is [PAR-2]'s counted
//! permission and the lowering that actualizes it, and [PAR-2] survives the
//! amendment: what changed under it is how a statement's footprint is spelled,
//! not which loops may split. The `.wf` fixtures were retargeted:
//!
//! - `region { .. }` wrappers are gone with regions themselves [OWN-3, OWN-4,
//!   OWN-10, FORM-8]; their statements stay in the enclosing block. Nothing is
//!   lost, because loans and arenas contribute no footprint any more and
//!   [STOR-8] gives allocation and release no effect entry at all, so neither
//!   can stop two statements from overlapping [PAR-1].
//! - `buffer_new(count, value)` became [OP-13]'s `box_array_filled` over the
//!   one heap [STOR-8], and `array_new` became `array_filled`.
//! - `slice_of` / `mut_slice_of` and the types `Slice<T>` / `MutSlice<T>`
//!   retired with [VIEW-1] and [VIEW-4]. The successor is [REF-4]'s range
//!   reference `&x[lo..hi]` carried by the parameter kind `&[T]`, whose
//!   disjointness is [OWN-7]'s four non-strict range orderings - the same
//!   judgment [PAR-1] and [PAR-2] now name directly.
//! - `len_of(p)` became [OP-15]'s measure member read `p.len`, a place form
//!   and not a call.
//! - `&uniq p` became the one reference spelling `&p`; whether a callee writes
//!   through it is stated by its effect row [EFF-1], and `writes` subsumes
//!   `reads` for one path, so `reads(d), writes(d)` collapses to `writes(d)`.
//!
//! [`map_and_reduction_source`] now takes its eight-byte speller from
//! [`COMBINE_PRELUDE`] instead of splicing it out of the `range_fold.wf`
//! program fixture. The two were byte-identical; taking the one this module
//! owns keeps the generated fixture on the spec version the module is written
//! against.

use super::BoundedOutput;
use std::process::Command;

use super::parallel::{CountedProgram, function_body, identical};
use super::{
    build_executable, emit, emit_with_overlap, module_requires_parallel_runtime, test_directory,
};

/// A counted `for` the judgment permits: one accumulator under `+wrap`, a
/// pure body doing real arithmetic per iteration, no write to
/// anything the iteration did not introduce, and no edge leaving the loop.
///
/// The body is an iterated integer mix rather than two operations, because the
/// range driver refuses a range that is not worth splitting and a
/// two-operation body over this span would be refused — correctly, and then
/// this module would be measuring nothing. Its result is written to standard
/// output as eight bytes, so a difference anywhere in the fold is a difference
/// in the bytes.
const PERMITTED_FOLD: &[u8] = include_bytes!("../../../../tests/programs/parallel/range_fold.wf");

const TABLE_READ_FOLD: &[u8] =
    include_bytes!("../../../../tests/programs/parallel/map_read_fold.wf");

#[test]
fn reference_sibling_field_map_splits_without_copying_its_referent() {
    let source =
        include_bytes!("../../../../tests/conformance/cases/par2-pos-affine-sibling-field-read.wf");
    let module = emit_with_overlap(source);
    let chunks = synthesized_symbols(&module, "@wf__par_chunk_");
    let chunk = chunks
        .iter()
        .find(|name| name.starts_with("@wf__par_chunk_fill."))
        .expect("the sibling scalar read must permit fill's split");
    let body = function_body(&module, chunk);
    assert!(
        body.contains("load i64, ptr"),
        "the chunk must load the scalar: {body}"
    );
    assert!(
        body.contains("store i64 "),
        "the chunk must write the element: {body}"
    );
    assert!(
        !body.contains("llvm.memcpy"),
        "the field read must not copy the whole referent: {body}"
    );
}

fn table_read_fold_with_sibling_writes() -> String {
    std::str::from_utf8(TABLE_READ_FOLD)
        .expect("UTF-8 map reader")
        .replace(
            "  let sum = 0_u64;",
            "  let sum = 0_u64;\n  let marks = array_filled::<u64, 256>(value: 0_u64);",
        )
        .replace(
            "fn main() -> status: std::process::ExitStatus pure waits {",
            "fn sibling_marks(limit: u64) -> status: std::process::ExitStatus pure waits contract {\n  requires limit <= 256_u64;\n} {",
        )
        .replace("for (i in 0_u64..4000000_u64)", "for @entries (i in 0_u64..limit)")
        .replace(
            "      set sum = sum +wrap v;",
            "      set marks[i] = v;\n      set sum = sum +wrap v;",
        )
        + "\nfn main() -> status: std::process::ExitStatus pure waits {\n  let result = sibling_marks(limit: 256_u64);\n  return move result;\n}\n"
}

#[test]
fn split_table_read_fold_uses_read_selections() {
    assert_chunk_table_selections_read(TABLE_READ_FOLD);
    assert_chunk_table_selections_read(table_read_fold_with_sibling_writes().as_bytes());
}

fn assert_chunk_table_selections_read(source: &[u8]) {
    let module = emit_with_overlap(source);
    let chunks = synthesized_symbols(&module, "@wf__par_chunk_");
    assert!(!chunks.is_empty(), "the table reader must split");
    let mut selections = 0;
    for chunk in chunks {
        for line in function_body(&module, &chunk).lines() {
            if line.contains("call ptr @wf__table_held_entry(") {
                selections += 1;
                assert!(
                    line.ends_with("i32 0)"),
                    "chunk selection must read: {line}"
                );
            }
        }
    }
    assert!(selections > 0, "the chunk must select table entries");
}

#[test]
fn split_table_read_fold_runs_repeatedly() {
    let module = emit_with_overlap(TABLE_READ_FOLD);
    let directory = test_directory();
    let executable = CountedProgram::link(&control_range_transport(&module), &directory);
    for run in 0..20 {
        let (grants, output) = executable.run(Some("4"));
        assert_eq!(output.status.code(), Some(0), "run {run}: {output:?}");
        assert!(grants > 0, "run {run} must execute a worker: {output:?}");
    }
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

#[test]
fn split_table_read_fold_with_overlapping_writes_stays_sequential() {
    let source = std::str::from_utf8(TABLE_READ_FOLD)
        .expect("UTF-8 table reader")
        .replace(
            "      let slot = &t^[key];",
            "      set t^[key] = None<u8>();\n      let slot = &t^[key];",
        );
    let ledger = super::compile_permission_ledger(source.as_bytes());
    assert!(
        ledger
            .iter()
            .any(|line| line.starts_with("PAR loop") && line.contains("denied")),
        "{ledger:#?}"
    );
    let module = emit_with_overlap(source.as_bytes());
    assert!(
        !module.contains("@wf__par_split_"),
        "overlapping table writes cannot split"
    );
    assert!(
        module.lines().any(
            |line| line.contains("call ptr @wf__table_held_entry(") && line.ends_with("i32 1)")
        ),
        "the sequential write must still materialize its cell"
    );
}

/// Two affine maps through potentially identical atomic roots may conflict,
/// while writes at the same iteration index still own one element each.
#[test]
fn split_loop_keeps_atomic_aliases_in_element_prefixes() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure waits {
  let values = array_filled::<u8, 65>(value: 0_u8);
  let first = shared_new::<Array<u8, 65>>(value: values);
  let second = shared_share::<Array<u8, 65>>(shared: &first);
  atomic a = &first, b = &second {
    for @cells (i in 0_u64..64_u64) {
      let next = i + 1_u64;
      set a^[i] = 1_u8;
      set b^[next] = 2_u8;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let ledger = super::compile_permission_ledger(source);
    assert!(
        ledger
            .iter()
            .any(|line| line.starts_with("PAR loop") && line.contains("denied")),
        "{ledger:#?}"
    );
    assert!(!emit_with_overlap(source).contains("@wf__par_split_"));
    let same_index = std::str::from_utf8(source)
        .expect("source")
        .replace("b^[next]", "b^[i]");
    let ledger = super::compile_permission_ledger(same_index.as_bytes());
    assert!(
        ledger
            .iter()
            .any(|line| line.starts_with("PAR loop") && line.contains("permitted")),
        "{ledger:#?}"
    );
}

fn fold_module(parallel: bool) -> String {
    use std::sync::OnceLock;
    static PLAIN: OnceLock<String> = OnceLock::new();
    static PARALLEL: OnceLock<String> = OnceLock::new();
    let cell = if parallel { &PARALLEL } else { &PLAIN };
    let module = cell
        .get_or_init(|| {
            if parallel {
                emit_with_overlap(PERMITTED_FOLD)
            } else {
                emit(PERMITTED_FOLD)
            }
        })
        .clone();
    super::exhaustion::assert_stack_probes(&module);
    module
}

/// The permitted loop of [`PERMITTED_FOLD`] over ranges the split has to answer
/// for without folding anything: empty, inverted, and one wide.
///
/// The exit status carries the comparison, so a splitter that computed a width
/// before testing the endpoints — which wraps an inverted range to something
/// near 2^64 — fails here rather than hanging somewhere later.
const EDGE_RANGES: &[u8] = br#"fn mix(seed: u64) -> result: u64 pure {
  let state = seed;
  let round = 0_u64;
  loop @rounds {
    let done = round == 24_u64;
    if done {
      break @rounds;
    }
    let shifted = irotl(state, 27_u32);
    let scaled = state *wrap 6364136223846793005_u64;
    set state = ixor(shifted, scaled);
    set state = state +wrap 1442695040888963407_u64;
    set round = round +wrap 1_u64;
  }
  return state;
}

fn folded(lo: u64, hi: u64) -> result: u64 pure {
  let total = 7_u64;
  for @points (i in lo..hi) {
    let mixed = mix(seed: i);
    set total = total +wrap mixed;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Every degenerate range folds to the accumulator it arrived with, and one wide range folds to the same value split or not.";
  let empty = folded(lo: 5_u64, hi: 5_u64);
  if empty == 7_u64 {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  let inverted = folded(lo: 400000_u64, hi: 5_u64);
  if inverted == 7_u64 {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  let single = folded(lo: 5_u64, hi: 6_u64);
  let one = mix(seed: 5_u64);
  let expected = one +wrap 7_u64;
  if single == expected {
  } else {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// The same permitted loop inside a function carrying enough live scalars that
/// the splitter's lane frame cannot hold them.
///
/// The frame is bounded by the runtime, and a split whose frame exceeds it
/// would have every lane acquisition refused forever: the program would pay for the
/// splitter and never overlap. So the bound is applied at compile time and the
/// loop declines with a line naming the width.
const WIDE_FRAME: &[u8] = br#"fn mix(seed: u64) -> result: u64 pure {
  let state = seed;
  let round = 0_u64;
  loop @rounds {
    let done = round == 24_u64;
    if done {
      break @rounds;
    }
    let shifted = irotl(state, 27_u32);
    let scaled = state *wrap 6364136223846793005_u64;
    set state = ixor(shifted, scaled);
    set state = state +wrap 1442695040888963407_u64;
    set round = round +wrap 1_u64;
  }
  return state;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "Thirty-two live scalars stand between the loop and a frame that fits.";
  let a0 = 0_u64;
  let a1 = 1_u64;
  let a2 = 2_u64;
  let a3 = 3_u64;
  let a4 = 4_u64;
  let a5 = 5_u64;
  let a6 = 6_u64;
  let a7 = 7_u64;
  let a8 = 8_u64;
  let a9 = 9_u64;
  let b0 = 10_u64;
  let b1 = 11_u64;
  let b2 = 12_u64;
  let b3 = 13_u64;
  let b4 = 14_u64;
  let b5 = 15_u64;
  let b6 = 16_u64;
  let b7 = 17_u64;
  let b8 = 18_u64;
  let b9 = 19_u64;
  let c0 = 20_u64;
  let c1 = 21_u64;
  let c2 = 22_u64;
  let c3 = 23_u64;
  let c4 = 24_u64;
  let c5 = 25_u64;
  let c6 = 26_u64;
  let c7 = 27_u64;
  let c8 = 28_u64;
  let c9 = 29_u64;
  let d0 = 30_u64;
  let d1 = 31_u64;
  let total = 0_u64;
  for @points (i in 0_u64..400000_u64) {
    let mixed = mix(seed: i);
    let bias0 = mixed +wrap a0;
    let bias1 = bias0 +wrap a1;
    let bias2 = bias1 +wrap a2;
    let bias3 = bias2 +wrap a3;
    let bias4 = bias3 +wrap a4;
    let bias5 = bias4 +wrap a5;
    let bias6 = bias5 +wrap a6;
    let bias7 = bias6 +wrap a7;
    let bias8 = bias7 +wrap a8;
    let bias9 = bias8 +wrap a9;
    let bias10 = bias9 +wrap b0;
    let bias11 = bias10 +wrap b1;
    let bias12 = bias11 +wrap b2;
    let bias13 = bias12 +wrap b3;
    let bias14 = bias13 +wrap b4;
    let bias15 = bias14 +wrap b5;
    let bias16 = bias15 +wrap b6;
    let bias17 = bias16 +wrap b7;
    let bias18 = bias17 +wrap b8;
    let bias19 = bias18 +wrap b9;
    let bias20 = bias19 +wrap c0;
    let bias21 = bias20 +wrap c1;
    let bias22 = bias21 +wrap c2;
    let bias23 = bias22 +wrap c3;
    let bias24 = bias23 +wrap c4;
    let bias25 = bias24 +wrap c5;
    let bias26 = bias25 +wrap c6;
    let bias27 = bias26 +wrap c7;
    let bias28 = bias27 +wrap c8;
    let bias29 = bias28 +wrap c9;
    let bias30 = bias29 +wrap d0;
    let bias31 = bias30 +wrap d1;
    let biased = bias31;
    set total = total +wrap biased;
  }
  if total == 0_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// A permitted loop that captures a nonzero record and inline array and folds
/// under an operation that is not `+wrap`.
///
/// Two things nothing else here reaches. **Captures**: the chunk and the
/// splitter declare them, the site passes them, and a lane frame carries them
/// through a thunk — four lists that have to agree on order and type, and a
/// fixture with no capture at all cannot tell whether they do. Their fields
/// differ in value *and* are folded asymmetrically, so a swapped pair moves the
/// published bytes. **A second combine**: both `ixor` and `+wrap` have identity zero, but the incoming nonzero seed
/// must reach the left half rather than seeding that half
/// where the right should be, changes the answer here and not there.
const CAPTURED_XOR_FOLD: &[u8] = br#"struct FoldControls {
  tag: u8;
  salt: u64;
  rounds: u64;
}

fn mix(seed: u64, salt: u64, rounds: u64) -> result: u64 pure {
  let state = ixor(seed, salt);
  let round = 0_u64;
  loop @rounds {
    let done = round == rounds;
    if done {
      break @rounds;
    }
    let shifted = irotl(state, 27_u32);
    let scaled = state *wrap 6364136223846793005_u64;
    set state = ixor(shifted, scaled);
    set state = state +wrap 1442695040888963407_u64;
    set round = round +wrap 1_u64;
  }
  return state;
}

fn low_byte(v: u64) -> result: u8 pure {
  let low = iand(v, 255_u64);
  match cvt.checked::<u64, u8>(low) {
    Ok(value: byte) => {
      return byte;
    }
    Err(error: problem) => {
      return 0_u8;
    }
  }
}

fn spell(destination: &[u8], at: u64, value: u64) -> result: u64 writes(destination) {
  let cursor = at;
  let rest = value;
  loop @octets {
    let limit = at +wrap 8_u64;
    let done = cursor >= limit;
    if done {
      break @octets;
    }
    let spare = destination^.len;
    let writable = cursor < spare;
    if writable {
      let byte = low_byte(v: rest);
      set destination^[cursor] = byte;
    }
    set rest = irotr(rest, 8_u32);
    set cursor = cursor +wrap 1_u64;
  }
  return at +wrap 8_u64;
}

fn folded(salt: u64, rounds: u64, stride: u64) -> result: u64 pure {
  doc "Copied nonzero record and array captures, folded under ixor.";
  let controls = FoldControls(tag: 13_u8, salt: salt, rounds: rounds);
  let strides = array_filled::<u64, 3>(value: stride);
  let total = 12345678901234567890_u64;
  for @points (i in 0_u64..400000_u64) {
    let copied_controls = controls;
    let copied_strides = strides;
    let tag = cvt::<u8, u64>(copied_controls.tag);
    let step = copied_strides[1_u64] +wrap tag;
    let stepped = i *wrap step;
    let mixed = mix(seed: stepped, salt: copied_controls.salt, rounds: copied_controls.rounds);
    set total = ixor(total, mixed);
  }
  return total;
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure waits {
  let std::process::Inputs(args: unused_args, drivers: unused_driver_count, cwd: unused_cwd_directory, stdout: out, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin, clock: unused_clock, wall_clock: unused_wall_clock, stops: unused_stops, memory_meter: unused_memory_meter, host_paths: unused_host_paths, process_id: unused_process_id) = move inputs;
  let std::fs::Directory(read: unused_cwd, write: unused_cwd_write) = move unused_cwd_directory;
  std::fs::close_directory_write(factory: &entry_factory, directory: move unused_cwd_write);
  std::fs::close_directory(factory: &entry_factory, directory: move unused_cwd);
  let value = folded(salt: 9876543210_u64, rounds: 24_u64, stride: 7_u64);
  let report = box_array_filled::<u8>(count: 8_u64, value: 0_u8);
  let window = &report.inner[0_u64..8_u64];
  let stored = spell(destination: window, at: 0_u64, value: value);
  let no_deadline = None<std::time::Instant>();
  let wait_cancel_1 = std::time::cancel_never();
  let wait_outcome_1 = std::io::write_once(factory: &entry_factory, output: &out, source: window, start: 0_u64, end: 8_u64, deadline: no_deadline, cancel: &wait_cancel_1);
  std::time::close_cancel_watch(watch: move wait_cancel_1);
  match wait_outcome_1 {
    Ok(value: accepted) => {
      return std::process::exit_status(code: 0_u8);
    }
    Err(error: problem) => {
      return std::process::exit_status(code: 1_u8);
    }
  }
}
"#;

/// A map whose only carried effect is one already-proved write through a
/// copied and affinely transformed counted binder. The mapped buffer is returned, borrowed by
/// `write_once`, and then dropped by its one outer owner, so the observable
/// bytes cover capture, store, join, post-loop use, and cleanup together.
const INDEPENDENT_MAP: &[u8] = br#"fn mix(seed: u64) -> result: u64 pure {
  let state = seed;
  let round = 0_u64;
  loop @rounds {
    let done = round == 24_u64;
    if done {
      break @rounds;
    }
    let shifted = irotl(state, 27_u32);
    let scaled = state *wrap 6364136223846793005_u64;
    set state = ixor(shifted, scaled);
    set state = state +wrap 1442695040888963407_u64;
    set round = round +wrap 1_u64;
  }
  return state;
}

fn low_byte(v: u64) -> result: u8 pure {
  let low = iand(v, 255_u64);
  match cvt.checked::<u64, u8>(low) {
    Ok(value: byte) => {
      return byte;
    }
    Err(error: problem) => {
      return 0_u8;
    }
  }
}

fn mapped() -> result: Box<Array<u8>> pure {
  let out = box_array_filled::<u8>(count: 400000_u64, value: 0_u8);
  for @fill (i in 0_u64..400000_u64) {
    let copied = i;
    let slot = copied * 1_u64;
    let mixed = mix(seed: i);
    let byte = low_byte(v: mixed);
    set out.inner[slot] = byte;
  }
  return move out;
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure waits {
  let std::process::Inputs(args: unused_args, drivers: unused_driver_count, cwd: unused_cwd_directory, stdout: out, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin, clock: unused_clock, wall_clock: unused_wall_clock, stops: unused_stops, memory_meter: unused_memory_meter, host_paths: unused_host_paths, process_id: unused_process_id) = move inputs;
  let std::fs::Directory(read: unused_cwd, write: unused_cwd_write) = move unused_cwd_directory;
  std::fs::close_directory_write(factory: &entry_factory, directory: move unused_cwd_write);
  std::fs::close_directory(factory: &entry_factory, directory: move unused_cwd);
  let report = mapped();
  let size = report.inner.len;
  let source = &report.inner[0_u64..size];
  let no_deadline = None<std::time::Instant>();
  let wait_cancel_1 = std::time::cancel_never();
  let wait_outcome_1 = std::io::write_once(factory: &entry_factory, output: &out, source: source, start: 0_u64, end: size, deadline: no_deadline, cancel: &wait_cancel_1);
  std::time::close_cancel_watch(watch: move wait_cancel_1);
  match wait_outcome_1 {
    Ok(value: accepted) => {
      return std::process::exit_status(code: 0_u8);
    }
    Err(error: problem) => {
      return std::process::exit_status(code: 1_u8);
    }
  }
}
"#;

/// One supported padded nominal in a runtime-capacity Array. Whole-element
/// assignment avoids the currently unsupported projection through a nominal
/// buffer element while retaining the `{ u8, u64 }` element's alignment and
/// stride. Reading the written Array's whole measure inside the loop denies
/// PAR-2; the test retains that negative control and moves only this measure
/// read to the preheader for its positive split. The empty Box's measure is
/// still read by each iteration. A zero-length call takes the empty edge.
/// `discarded` is deliberately consumed before the loop. This already-fitting
/// frame retains its original unused transport, which must neither read the
/// consumed allocation nor acquire cleanup authority in the chunk. Removing
/// that transport is outside the rescue-only capture optimization. `main`
/// publishes both returned lengths, each as eight little-endian bytes, for the
/// harness to compare.
const ALIGNED_PAYLOAD_MAP: &[u8] = br#"struct Aligned {
  tag: u8;
  word: u64;
}

fn discard(value: Box<Array<Aligned>>) -> result: unit pure {
  return unit;
}

fn mix(seed: u64) -> result: u64 pure {
  let state = seed;
  let round = 0_u64;
  loop @rounds {
    let done = round == 24_u64;
    if done {
      break @rounds;
    }
    let shifted = irotl(state, 27_u32);
    let scaled = state *wrap 6364136223846793005_u64;
    set state = ixor(shifted, scaled);
    set state = state +wrap 1442695040888963407_u64;
    set round = round +wrap 1_u64;
  }
  return state;
}

fn marked(seed: u64) -> result: Aligned pure {
  let mixed = mix(seed: seed);
  let result = Aligned(tag: 7_u8, word: mixed);
  return result;
}

fn low_byte(v: u64) -> result: u8 pure {
  let low = iand(v, 255_u64);
  match cvt.checked::<u64, u8>(low) {
    Ok(value: byte) => {
      return byte;
    }
    Err(error: problem) => {
      return 0_u8;
    }
  }
}

fn spell(destination: &[u8], at: u64, value: u64) -> result: u64 writes(destination) {
  let cursor = at;
  let rest = value;
  loop @octets {
    let limit = at +wrap 8_u64;
    let done = cursor >= limit;
    if done {
      break @octets;
    }
    let spare = destination^.len;
    let writable = cursor < spare;
    if writable {
      let byte = low_byte(v: rest);
      set destination^[cursor] = byte;
    }
    set rest = irotr(rest, 8_u32);
    set cursor = cursor +wrap 1_u64;
  }
  return at +wrap 8_u64;
}

fn aligned_array(count: u64, tag: u8, word: u64) -> result: Box<Array<Aligned>> pure contract {
  requires count <= 400000_u64;
  ensures result.inner.len == count;
} {
  let value = Aligned(tag: tag, word: word);
  let result = box_array_filled::<Aligned>(count: count, value: value);
  return move result;
}

fn mapped(count: u64) -> result: Box<Array<Aligned>> pure contract {
  requires count <= 400000_u64;
  ensures result.inner.len == count;
} {
  let discarded = aligned_array(count: 1_u64, tag: 99_u8, word: 99_u64);
  set discarded.inner[0_u64] = Aligned(tag: 98_u8, word: 98_u64);
  let gone = discard(value: move discarded);
  let empty = aligned_array(count: 0_u64, tag: 0_u8, word: 0_u64);
  let output = aligned_array(count: count, tag: 0_u8, word: 0_u64);
  let extent = output.inner.len;
  for @fill (i in 0_u64..extent) {
    let empty_extent = empty.inner.len;
    let current_extent = output.inner.len;
    let all_extents = empty_extent + current_extent;
    let shifted = i +wrap all_extents;
    let selected = marked(seed: shifted);
    set output.inner[i] = selected;
  }
  return move output;
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure waits {
  let std::process::Inputs(args: unused_args, drivers: unused_driver_count, cwd: unused_cwd_directory, stdout: out, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin, clock: unused_clock, wall_clock: unused_wall_clock, stops: unused_stops, memory_meter: unused_memory_meter, host_paths: unused_host_paths, process_id: unused_process_id) = move inputs;
  let std::fs::Directory(read: unused_cwd, write: unused_cwd_write) = move unused_cwd_directory;
  std::fs::close_directory_write(factory: &entry_factory, directory: move unused_cwd_write);
  std::fs::close_directory(factory: &entry_factory, directory: move unused_cwd);
  let empty = mapped(count: 0_u64);
  let output = mapped(count: 400000_u64);
  let first = output.inner[0_u64];
  let first_expected = mix(seed: 400000_u64);
  if first.tag != 7_u8 {
    return std::process::exit_status(code: 3_u8);
  }
  if first.word != first_expected {
    return std::process::exit_status(code: 4_u8);
  }
  let last = output.inner[399999_u64];
  let last_expected = mix(seed: 799999_u64);
  if last.tag != 7_u8 {
    return std::process::exit_status(code: 5_u8);
  }
  if last.word != last_expected {
    return std::process::exit_status(code: 6_u8);
  }
  let empty_length = empty.inner.len;
  let output_length = output.inner.len;
  let report = box_array_filled::<u8>(count: 16_u64, value: 0_u8);
  let window = &report.inner[0_u64..16_u64];
  let empty_end = spell(destination: window, at: 0_u64, value: empty_length);
  let output_end = spell(destination: window, at: 8_u64, value: output_length);
  let no_deadline = None<std::time::Instant>();
  let wait_cancel_1 = std::time::cancel_never();
  let wait_outcome_1 = std::io::write_once(factory: &entry_factory, output: &out, source: window, start: 0_u64, end: 16_u64, deadline: no_deadline, cancel: &wait_cancel_1);
  std::time::close_cancel_watch(watch: move wait_cancel_1);
  match wait_outcome_1 {
    Ok(value: accepted) => {
      return std::process::exit_status(code: 0_u8);
    }
    Err(error: problem) => {
      return std::process::exit_status(code: 1_u8);
    }
  }
}
"#;

/// Two lexically nested split reductions share one locally owned boxed Array.
/// The owner is projected into the outer split, reconstructed in its chunk,
/// and projected again into the inner split reached from that chunk.
const NESTED_PAYLOAD_REDUCTIONS: &[u8] = br#"fn nested() -> result: u64 pure {
  let source = box_array_filled::<u64>(count: 65536_u64, value: 3_u64);
  let total = 0_u64;
  for @batches (i in 0_u64..8_u64) {
    let partial = 0_u64;
    let extent = source.inner.len;
    for @items (j in 0_u64..extent) {
      let value = source.inner[j];
      let salted = value +wrap i;
      set partial = partial +wrap salted;
    }
    set total = total +wrap partial;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  let observed = nested();
  if observed != 3407872_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// Preserve the entire map and append all eight checksum bytes, so a defect in
/// any reduction bit is visible without overwriting the map's first element.
///
/// The speller is taken from [`COMBINE_PRELUDE`], which this module owns, so
/// the generated fixture is on the same spec version as the rest of the file.
fn map_and_reduction_source() -> Vec<u8> {
    let source = std::str::from_utf8(INDEPENDENT_MAP).expect("UTF-8 fixture");
    let (_, spell) = COMBINE_PRELUDE
        .split_once("fn spell(")
        .expect("the shared prelude defines the eight-byte speller");
    format!("fn spell({spell}\n{source}")
        .replacen("count: 400000_u64", "count: 400008_u64", 1)
        .replacen("  for @fill", "  let checksum = 0_u64;\n  for @fill", 1)
        .replacen("    set out.inner[slot] = byte;", "    set out.inner[slot] = byte;\n    set checksum = checksum +wrap mixed;", 1)
        .replacen("  return move out;", "  let tail = &out.inner[0_u64..400008_u64];\n  let end = spell(destination: tail, at: 400000_u64, value: checksum);\n  return move out;", 1)
        .into_bytes()
}

/// The same work as [`INDEPENDENT_MAP`], expressed through an ordinary
/// reference to its boxed Array and a same-index read-modify-write. PAR-2's
/// single-element family requires an Array or Slots subscript; it does not
/// admit indexing an unpartitioned range reference. This exercises that
/// family's reference-parameter route without changing its access pattern.
fn borrowed_read_modify_map_source() -> Vec<u8> {
    let source = std::str::from_utf8(INDEPENDENT_MAP).expect("the fixture is UTF-8");
    source
        .replacen(
            "fn mapped() -> result: Box<Array<u8>> pure {\n  let out = box_array_filled::<u8>(count: 400000_u64, value: 0_u8);\n",
            "fn mapped(out: &Box<Array<u8>>) -> result: unit writes(out.inner) contract {\n  define spare = out^.inner.len;\n  requires 400000_u64 <= spare;\n} {\n",
            1,
        )
        .replacen(
            "    set out.inner[slot] = byte;\n",
            "    let old = out^.inner[slot];\n    let blended = old +wrap byte;\n    set out^.inner[slot] = blended;\n",
            1,
        )
        .replacen("  return move out;\n", "  return unit;\n", 1)
        .replacen(
            "  let report = mapped();\n",
            "  let report = box_array_filled::<u8>(count: 400000_u64, value: 173_u8);\n  let done = mapped(out: &report);\n",
            1,
        )
        .into_bytes()
}

/// A permitted loop is emitted as ordinary subrange chunks and a demand
/// splitter that hands out its far half and retains its near half.
///
/// The whole design is in this shape: the leaf is the *loop*, never one
/// iteration, so the body keeps the vectorization and unrolling the loop gave
/// it; and the splitter's two halves are an ordinary overlap group, so the
/// lane acquisition, the frame, the thunk, and the join are the machinery a permitted pair
/// already uses rather than a second one.
#[test]
fn a_permitted_loop_is_outlined_split_and_joined() {
    let module = fold_module(true);

    assert!(
        module.contains("define i64 @wf__par_chunk_"),
        "a split loop must outline its own body as a chunk:\n{module}"
    );
    assert!(
        module.contains("define i64 @wf__par_split_"),
        "a split loop must synthesize a range splitter:\n{module}"
    );
    let splitter_symbol = synthesized(&module, "@wf__par_split_");
    let chunk_symbol = synthesized(&module, "@wf__par_chunk_");
    let splitter = function_body(&module, &splitter_symbol);
    for step in [
        "call ptr @wf__par_acquire_lane(",
        "call void @wf__par_publish(",
        "call void @wf__par_join(",
        "call void @wf__par_release(",
    ] {
        assert!(
            splitter.contains(step),
            "the splitter must take the ordinary lane protocol, missing {step}:\n{splitter}"
        );
    }
    assert!(
        splitter.contains(&format!("call i64 {chunk_symbol}(")),
        "the splitter's leaf must be the chunk, not one iteration:\n{splitter}"
    );

    // The leaf is a loop: the chunk carries a back edge and the counted range's
    // own guard, and names no runtime symbol at all.
    let chunk = function_body(&module, &chunk_symbol);
    assert!(
        chunk.contains("icmp ult i64") && chunk.matches("phi ").count() > 0,
        "the chunk must be the counted loop itself:\n{chunk}"
    );
    let (_, chunk_body) = chunk.split_once('\n').expect("a definition has a body");
    assert!(
        !chunk_body.contains("@wf__par_"),
        "the chunk is the loop and must reach no runtime entry point:\n{chunk}"
    );

    // Range scheduling happens inside the splitter.
    assert!(splitter.contains("@wf__par_demand_word"), "{splitter}");
    assert!(splitter.contains("load atomic i64"), "{splitter}");
    assert!(splitter.contains("udiv i64"), "{splitter}");
    assert!(
        splitter.contains("150000") && splitter.contains("149999"),
        "{splitter}"
    );
}

/// The default compilation splits nothing.
///
/// A permission is never an obligation. A build that did not ask for
/// actualization emits exactly the module it emitted before this path existed:
/// the loop inline, no synthesized function, and no runtime symbol anywhere.
#[test]
fn the_default_compilation_of_a_permitted_loop_splits_nothing() {
    let module = fold_module(false);
    assert!(
        !module.contains("wf__par_"),
        "the default build of a permitted loop must name no part of the split:\n{module}"
    );
    assert!(!module_requires_parallel_runtime(&module));
}

/// Ordinary lowering supersedes the old clone-of-chunk construction: retaining
/// its in-place loop is necessary for single-thread computation identity.
#[test]
fn the_sequential_world_of_a_split_loop_is_the_ordinary_loop() {
    let module = fold_module(true);
    let ordinary = fold_module(false);
    let clone = function_body(&module, "@wf__par_seq_folded");
    assert_eq!(
        clone.replace("@wf__par_seq_", "@wf_"),
        function_body(&ordinary, "@wf_folded"),
        "the sequential computation must be the complete non-parallel lowering"
    );
    for absent in [
        "@wf__par_acquire_lane",
        "@wf__par_publish",
        "@wf__par_join",
        "@wf__par_seq__par_chunk_",
        "@wf__par_chunk_",
    ] {
        assert!(!clone.contains(absent), "unexpected {absent}: {clone}");
    }
}

/// The one synthesized symbol in this module beginning with `prefix`.
///
/// The synthesized halves are numbered after every source function, so their
/// ordinals move whenever a fixture gains one. Reading the symbol out of the
/// module keeps these cases about the shape rather than about the count. The
/// tail has to be a number, so a runtime symbol sharing the driver's prefix
/// is never one of these.
fn synthesized(module: &str, prefix: &str) -> String {
    let found = synthesized_symbols(module, prefix);
    let [only] = found.as_slice() else {
        panic!("the module must define exactly one {prefix}: {found:?}\n{module}");
    };
    only.clone()
}

/// [MOD-8] a split's helpers and the thunks their halves hand out are named by
/// the function they came from and their number among its own, so another
/// function gaining a permitted loop leaves every one of them, and the
/// function that calls them, byte for byte as it was.
#[test]
fn split_helpers_keep_their_symbols_when_another_function_gains_a_split() {
    let base = fold_module(true);
    let source = std::str::from_utf8(PERMITTED_FOLD).expect("the fixture is text");
    let refolded = source
        .split_once("fn folded(")
        .and_then(|(_, rest)| rest.split_once("\n}\n"))
        .map(|(body, _)| format!("fn refolded({body}\n}}\n\n"))
        .expect("the fixture defines folded");
    let extended = emit_with_overlap(
        source
            .replacen("fn folded(", &format!("{refolded}fn folded("), 1)
            .as_bytes(),
    );
    let splitters = synthesized_symbols(&base, "@wf__par_split_");
    let chunks = synthesized_symbols(&base, "@wf__par_chunk_");
    assert_eq!(splitters, ["@wf__par_split_folded.0"], "{base}");
    assert_eq!(chunks, ["@wf__par_chunk_folded.1"], "{base}");
    assert_eq!(
        synthesized_symbols(&extended, "@wf__par_split_").len(),
        2,
        "the added function splits its own loop:\n{extended}"
    );
    let thunks = base
        .lines()
        .filter_map(|line| line.strip_prefix("define internal void @wf__par_thunk_"))
        .filter_map(|rest| rest.split_once('('))
        .map(|(name, _)| format!("@wf__par_thunk_{name}"))
        .collect::<Vec<_>>();
    assert!(!thunks.is_empty(), "{base}");
    for symbol in splitters
        .iter()
        .chain(&chunks)
        .chain(&thunks)
        .map(String::as_str)
        .chain(["@wf_folded"])
    {
        assert_eq!(
            function_body(&base, symbol),
            function_body(&extended, symbol),
            "{symbol} must keep its text"
        );
    }
}

/// Every synthesized definition bearing `prefix`, without runtime helpers or
/// sequential-clone spellings that merely contain a similar suffix. A
/// synthesized symbol names its source function and its number among that
/// function's helpers.
fn synthesized_symbols(module: &str, prefix: &str) -> Vec<String> {
    let mut found: Vec<String> = module
        .lines()
        .filter_map(|line| line.split_once(prefix))
        .filter_map(|(head, tail)| head.starts_with("define ").then_some(tail))
        .filter_map(|tail| tail.split_once('('))
        .filter(|(name, _)| {
            name.rsplit_once('.')
                .is_some_and(|(_, number)| number.parse::<u32>().is_ok())
        })
        .map(|(name, _)| format!("{prefix}{name}"))
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

/// Force only the scheduling inputs of transport probes: enter every parallel
/// body, request at each source/chunk entry to a driver and raise a cheap
/// price just to the grain. Production-priced runs above remain the
/// source-result reference. Keeping the original large price avoids turning
/// large ranges into single iterations.
fn control_range_transport(module: &str) -> String {
    let module = super::without_entry_selection(module);
    let module = module.as_str();
    let mut controlled = String::new();
    let mut source_or_chunk = false;
    let mut protected = Vec::new();
    let mut ordinal = 0;
    for line in module.lines() {
        if line.starts_with("define ") {
            let symbol = line.split_once('@').unwrap().1.split_once('(').unwrap().0;
            source_or_chunk = (!symbol.starts_with("wf__par_")
                || symbol.starts_with("wf__par_chunk_"))
                && !symbol.starts_with("wf__ctx_thunk_");
            if !source_or_chunk {
                protected.push(format!("@{symbol}"));
            }
        }
        if source_or_chunk && line.contains("call ") && line.contains("@wf__par_split_") {
            let (_, arguments) = line.split_once("@wf__par_split_").unwrap();
            let (_, arguments) = arguments.split_once('(').unwrap();
            let (arguments, _) = arguments.rsplit_once(')').unwrap();
            let values: Vec<_> = arguments.split(", ").collect();
            let lower = values[1].strip_prefix("i64 ").unwrap();
            let upper = values[2].strip_prefix("i64 ").unwrap();
            let price = values.last().unwrap().strip_prefix("i64 ").unwrap();
            controlled.push_str(&format!(
                "  %transport.{ordinal}.ascending = icmp ult i64 {lower}, {upper}\n  %transport.{ordinal}.difference = sub i64 {upper}, {lower}\n  %transport.{ordinal}.span = select i1 %transport.{ordinal}.ascending, i64 %transport.{ordinal}.difference, i64 0\n  %transport.{ordinal}.divisor = call i64 @llvm.umax.i64(i64 %transport.{ordinal}.span, i64 1)\n  %transport.{ordinal}.quotient = udiv i64 149999, %transport.{ordinal}.divisor\n  %transport.{ordinal}.minimum = add i64 %transport.{ordinal}.quotient, 1\n  %transport.{ordinal}.price = call i64 @llvm.umax.i64(i64 {price}, i64 %transport.{ordinal}.minimum)\n  store atomic i64 1, ptr @wf__par_demand_word release, align 8\n"
            ));
            let (prefix, _) = line.rsplit_once(", i64 ").unwrap();
            controlled.push_str(&format!("{prefix}, i64 %transport.{ordinal}.price)\n"));
            ordinal += 1;
        } else {
            controlled.push_str(line);
            controlled.push('\n');
        }
    }
    assert!(
        ordinal > 0,
        "transport probe must retain an actual range driver"
    );
    // Hand-out callbacks must forward the original price and request state;
    // refreshing either at every far half would force a different fork tree.
    for symbol in protected {
        assert_eq!(
            function_body(module, &symbol),
            function_body(&controlled, &symbol),
            "transport control must preserve callback and recursive-driver body {symbol}"
        );
    }
    controlled
}

/// Observe the selected outer demand word without changing an inner loop's
/// requests. With no outer request, ordinary slices must retain inner offers;
/// the real request arm still proves a nonowner publication and completion.
fn observe_outer_loop_budget(module: &str, caller: &str, splitter: &str, chunk: &str) -> String {
    let mut observed = control_range_transport(module);
    for (symbol, observer) in [
        (splitter, "wf_test_outer_splitter"),
        (chunk, "wf_test_outer_chunk"),
    ] {
        let body = function_body(&observed, symbol).to_owned();
        let entry = body.lines().find(|line| line.ends_with(':')).unwrap();
        let mut changed = body.replacen(entry, &format!("{entry}\n  call void @{observer}()"), 1);
        if symbol == splitter {
            let mut polls = 0;
            changed = changed
                .lines()
                .map(|line| {
                    if line.contains("load atomic i64") && line.contains("@wf__par_demand_word") {
                        polls += 1;
                        let (result, _) = line.split_once(" = ").unwrap();
                        format!("{result} = call i64 @wf_test_outer_requested()")
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(polls, 1, "observe exactly the outer request instruction");
        }
        observed = observed.replacen(&body, &changed, 1);
    }
    observed.push_str("\ndeclare i64 @wf_test_outer_requested()\ndeclare void @wf_test_outer_splitter()\ndeclare void @wf_test_outer_chunk()\n");
    super::parallel::observe_worker_schedule(&observed)
}

const OUTER_LOOP_BUDGET_OBSERVER: &str = r#"
extern _Thread_local uint64_t wf__par_demand_word;
extern unsigned long wf__par_grants(void);
static _Atomic unsigned outer_requests, outer_splitters, outer_chunks;
uint64_t wf_test_outer_requested(void) {
    atomic_fetch_add(&outer_requests, 1);
    return getenv("WF_TEST_ZERO_BUDGET") ? 0 :
        atomic_load_explicit((_Atomic uint64_t *)&wf__par_demand_word, memory_order_acquire);
}
void wf_test_outer_splitter(void) { atomic_fetch_add(&outer_splitters, 1); }
void wf_test_outer_chunk(void) { atomic_fetch_add(&outer_chunks, 1); }
static void report_outer_budget(void) {
    unsigned requests = atomic_load(&outer_requests);
    unsigned splitters = atomic_load(&outer_splitters);
    unsigned chunks = atomic_load(&outer_chunks);
    unsigned long grants = wf__par_grants();
    int zero = getenv("WF_TEST_ZERO_BUDGET") != NULL;
    if (!chunks || !requests || (zero && splitters != 1) ||
        ((!zero || WF_TEST_NESTED) && (!grants || !atomic_load(&schedule_entered)))) {
        fprintf(stderr, "outer loop demand: zero=%d requests=%u splitters=%u chunks=%u grants=%lu\n",
                zero, requests, splitters, chunks, grants);
        _Exit(116);
    }
}
__attribute__((constructor)) static void register_outer_budget(void) { atexit(report_outer_budget); }
"#;

/// A split that carries captures and folds under a second admitted operation
/// publishes what the unsplit lowering publishes, at every worker count.
///
/// Both combines have identity zero; the nonzero incoming seed and aggregate
/// captures have to reach the chunk in the order and the types its parameters
/// declare — through a lane frame and a thunk on the granted edge. Both are
/// checked against the default compilation of the same source rather than
/// against another run of the same module, so a defect the split introduces is
/// not present in the reference.
#[test]
fn a_split_loop_carries_its_captures_and_a_second_combine() {
    // The tail uses every extra scalar and an inline array, but none belongs
    // to the loop's task.
    // Capturing lexical scope would put this otherwise unchanged fold beyond
    // the lane limit. Keep the existing native builds and worker observations.
    let tail_bindings = (0..32)
        .map(|index| format!("  let tail{index} = {index}_u64;\n"))
        .collect::<String>();
    let tail_sum = (0..32)
        .map(|index| format!("  set tail_sum = tail_sum +wrap tail{index};\n"))
        .collect::<String>();
    let source = std::str::from_utf8(CAPTURED_XOR_FOLD)
        .expect("UTF-8 fixture")
        .replace(
            "  let total = 12345678901234567890_u64;",
            &format!("{tail_bindings}  let tail_array = array_filled::<u64, 512>(value: 99_u64);\n  let total = 12345678901234567890_u64;"),
        )
        .replace(
            "  return total;",
            &format!(
                "  let tail_sum = 0_u64;\n{tail_sum}  let tail_delta = tail_sum -wrap 496_u64;\n  let saved_array = tail_array;\n  let array_value = saved_array[0_u64];\n  let array_delta = array_value -wrap 99_u64;\n  let adjusted = total +wrap tail_delta;\n  return adjusted +wrap array_delta;"
            ),
        );
    let unsplit = emit(source.as_bytes());
    let split = super::system::with_parallel_ir(source.as_bytes(), |program| {
        use crate::target::{TargetLayout, parallel_lane_frame_layout};
        let host = TargetLayout::host().expect("supported test host");
        let splitter = program
            .functions()
            .iter()
            .find(|function| function.synthesis() == Some(crate::IrSynthesis::Splitter))
            .expect("the fold must split despite its wide surrounding scope");
        let frame = parallel_lane_frame_layout(
            host,
            program.nominals(),
            program.elements(),
            splitter.parameters().iter().map(|(_, ty)| *ty),
            splitter.result(),
            false,
        )
        .expect("target frame layout")
        .expect("the needed capture frame fits");
        assert_eq!(
            frame.size(),
            88,
            "seed, bounds, a padded 24-byte record, a 24-byte array, runtime price and result"
        );
        let chunk = program
            .functions()
            .iter()
            .find(|function| function.synthesis() == Some(crate::IrSynthesis::Chunk))
            .expect("the fold has one chunk");
        assert!(
            chunk
                .value_types()
                .iter()
                .any(|ty| matches!(ty, crate::IrType::Array { length: 512, .. })),
            "the removed capture must leave aggregate type metadata for this regression"
        );
        let storage = crate::backend::storage::FunctionStoragePlan::build(program, chunk)
            .expect("the pruned chunk has valid storage");
        assert!(
            storage.slots().iter().all(|ty| {
                matches!(
                    ty,
                    crate::IrType::Nominal(_) | crate::IrType::Array { length: 3, .. }
                )
            }),
            "only the retained record and array may allocate chunk storage; the removed 512-element capture must leave no phantom slot"
        );
        let mut module = crate::backend::emitter::emit_llvm_with_layout(program, host)
            .expect("the reduced capture ABI must emit")
            .into_string();
        module.push_str(
            &crate::driver::launcher::render(program, "main")
                .expect("ordinary test launcher")
                .render(),
        );
        module
    });
    assert!(
        split.contains("@wf__par_split_"),
        "the fixture's loop must actually split, or this checks nothing:\n{split}"
    );
    // Aggregate formals use the ordinary indirect function ABI. The seed and
    // endpoints are the only scalar parameters; both retained payloads travel
    // by value inside the lane frame and are copied into the chunk's storage.
    let chunk = function_body(&split, &synthesized(&split, "@wf__par_chunk_"));
    let signature = chunk.lines().next().expect("a definition has a signature");
    assert_eq!(
        signature.matches("i64 %").count(),
        3,
        "the chunk must declare the seed and both endpoints: {signature}"
    );
    assert_eq!(signature.matches("ptr ").count(), 2, "{signature}");

    let directory = test_directory();
    let reference = Command::new(build_executable(&unsplit, &directory))
        .bounded_output()
        .expect("run the module that splits nothing");
    assert_eq!(reference.status.code(), Some(0));
    assert_eq!(reference.stdout.len(), 8);

    let executable = build_executable(&split, &directory);
    let mut runs = vec![("no split lowering".to_owned(), reference.stdout)];
    for workers in ["1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .bounded_output()
            .expect("run the split program");
        assert_eq!(output.status.code(), Some(0), "WF_WORKERS={workers}");
        runs.push((format!("WF_WORKERS={workers}"), output.stdout));
    }
    identical(&runs).expect("a captured, xor-folded split must not move one byte");

    // A selected worker schedule remains observable on a one-core host.
    {
        let (granted, output) =
            CountedProgram::link(&control_range_transport(&split), &directory).run(Some("4"));
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, runs[0].1);
        assert!(
            granted > 0,
            "the comparison above overlapped nothing in the controlled worker execution"
        );
    }

    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// A proved single-binder affine map uses the same split machinery without inventing
/// a source accumulator: Unit carries the worker join, while the captured
/// buffer carries the only observable result.
fn assert_map_payload_capture(module: &str, chunk: &str, seed_type: &str, element_type: &str) {
    let signature = chunk.lines().next().expect("chunk definition");
    let (_, arguments) = signature.split_once('(').expect("chunk parameters");
    let (arguments, _) = arguments.split_once(')').expect("closed parameters");
    let arguments = arguments.split(',').collect::<Vec<_>>();
    let types = arguments
        .iter()
        .map(|argument| argument.split_whitespace().next().expect("parameter type"))
        .collect::<Vec<_>>();
    assert_eq!(
        types,
        [seed_type, "i64", "i64", "ptr"],
        "the chunk must capture exactly the stable thin Box pointer after its seed and bounds:\n{chunk}"
    );
    let captured_payload = arguments
        .last()
        .and_then(|argument| argument.split_whitespace().next_back())
        .expect("Box<Array<T>> payload capture parameter");
    let parent_slot_load = format!("load ptr, ptr {captured_payload}");
    assert!(
        !chunk.contains(&parent_slot_load),
        "the chunk must not reload a Box pointer through the captured parent owner slot:\n{chunk}"
    );

    let block_type = format!("{{ i64, [0 x {element_type}] }}");
    let forward = format!("getelementptr inbounds {block_type}, ptr ");
    let outer = function_body(module, "@wf_mapped");
    let projection = outer
        .find(&forward)
        .expect("the owner must be projected to its element base before the split");
    let split_call = outer
        .find("call i8 @wf__par_split_")
        .or_else(|| outer.find("call i64 @wf__par_split_"))
        .expect("the projected capture must reach the splitter");
    assert!(
        projection < split_call,
        "payload projection must precede the split:\n{outer}"
    );
    assert!(
        outer[projection..].contains("i64 0, i32 1, i64 0"),
        "the forward operation must select the first array element:\n{outer}"
    );

    let offset =
        format!("ptrtoint (ptr getelementptr ({block_type}, ptr null, i64 0, i32 1) to i64)");
    assert!(
        chunk.contains(&offset),
        "the chunk must derive the exact inverse of the typed payload projection:\n{chunk}"
    );
    assert!(
        chunk.contains(&format!(
            "getelementptr inbounds i8, ptr {captured_payload}, i64 %"
        )),
        "the chunk must reconstruct a local owner from its payload parameter:\n{chunk}"
    );
}

#[test]
fn an_aligned_nominal_payload_capture_handles_empty_and_mixed_measure_element_paths() {
    let unsplit = emit(ALIGNED_PAYLOAD_MAP);
    assert!(!module_requires_parallel_runtime(&unsplit));
    let measured = emit_with_overlap(ALIGNED_PAYLOAD_MAP);
    assert!(
        module_requires_parallel_runtime(&measured)
            && !synthesized_symbols(&measured, "@wf__par_chunk_").is_empty(),
        "PAR-2 admits a measure read of the mapped root beside its mapped writes, so the in-body length read still splits"
    );
    // The same map with the length read replaced by the preheader's captured
    // extent must split identically; the assertions below inspect that form.
    let source = std::str::from_utf8(ALIGNED_PAYLOAD_MAP)
        .expect("UTF-8 fixture")
        .replacen(
            "let current_extent = output.inner.len;",
            "let current_extent = extent;",
            1,
        );
    let split = emit_with_overlap(source.as_bytes());
    assert!(
        module_requires_parallel_runtime(&split),
        "the nominal-element map must actualize its permitted split:\n{split}"
    );

    let aligned_type = split
        .lines()
        .find_map(|line| line.strip_suffix(" = type { i8, i64 }"))
        .expect("Aligned must retain its padding-sensitive nominal LLVM type");
    let block_type = format!("{{ i64, [0 x {aligned_type}] }}");
    let outer = function_body(&split, "@wf_mapped");
    let forward = format!("getelementptr inbounds {block_type}, ptr ");
    assert_eq!(
        outer
            .lines()
            .filter(|line| line.contains(&forward) && line.contains("i64 0, i32 1, i64 0"))
            .count(),
        3,
        "the fitting ABI retains both used payloads and its original unused transport:\n{outer}"
    );

    let chunk = function_body(&split, &synthesized(&split, "@wf__par_chunk_"));
    let inverse =
        format!("ptrtoint (ptr getelementptr ({block_type}, ptr null, i64 0, i32 1) to i64)");
    assert_eq!(
        chunk.matches(&inverse).count(),
        3,
        "the fitting chunk retains its original reconstruction without acquiring cleanup:\n{chunk}"
    );
    assert!(
        chunk.lines().any(|line| {
            line.contains(&format!("getelementptr inbounds {block_type}, ptr "))
                && line.contains("i32 1, i64 %")
        }),
        "the live payload must still reach ordinary indexed element lowering:\n{chunk}"
    );
    assert!(
        chunk.lines().any(|line| line.contains("load i64, ptr ")),
        "the reconstructed empty owner must still support its len read:\n{chunk}"
    );
    assert!(!chunk.contains("call void @wf__heap_give("), "{chunk}");

    let directory = test_directory();
    let reference = Command::new(build_executable(&unsplit, &directory))
        .bounded_output()
        .expect("run the unsplit nominal-element map");
    assert_eq!(reference.status.code(), Some(0), "{reference:?}");
    // The two returned lengths: the empty call's zero, then 400000.
    let lengths = [0_u64.to_le_bytes(), 400_000_u64.to_le_bytes()].concat();
    assert_eq!(reference.stdout, lengths, "{reference:?}");

    let executable = build_executable(&split, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .bounded_output()
            .expect("run the split nominal-element map");
        assert_eq!(
            output.status.code(),
            Some(0),
            "WF_WORKERS={workers}: {output:?}"
        );
        assert_eq!(output.stdout, reference.stdout, "WF_WORKERS={workers}");
    }
    let (granted, output) =
        CountedProgram::link(&control_range_transport(&split), &directory).run(Some("4"));
    assert!(
        granted > 0,
        "the nominal-element map must execute a real worker callback"
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, reference.stdout);

    // [PAR-2] the original map reads `output.inner.len` inside each chunk;
    // its split must compute the same result as the unsplit program and the
    // preheader form, with real workers.
    let measured_executable = build_executable(&measured, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&measured_executable)
            .env("WF_WORKERS", workers)
            .bounded_output()
            .expect("run the split map that reads its length in the body");
        assert_eq!(
            output.status.code(),
            Some(0),
            "WF_WORKERS={workers}: {output:?}"
        );
        assert_eq!(output.stdout, reference.stdout, "WF_WORKERS={workers}");
    }
    let (granted, output) =
        CountedProgram::link(&control_range_transport(&measured), &directory).run(Some("4"));
    assert!(
        granted > 0,
        "the map reading its length in the body must execute a real worker callback"
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, reference.stdout);
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

#[test]
fn nested_boxed_array_payload_reductions_preserve_the_unsplit_result() {
    let unsplit = emit(NESTED_PAYLOAD_REDUCTIONS);
    assert!(!module_requires_parallel_runtime(&unsplit));
    let split = emit_with_overlap(NESTED_PAYLOAD_REDUCTIONS);
    assert!(module_requires_parallel_runtime(&split));

    let splitters = synthesized_symbols(&split, "@wf__par_split_");
    let chunks = synthesized_symbols(&split, "@wf__par_chunk_");
    assert_eq!(
        splitters.len(),
        2,
        "both reductions must split: {splitters:?}\n{split}"
    );
    assert_eq!(
        chunks.len(),
        2,
        "both reductions must have chunks: {chunks:?}\n{split}"
    );
    let chunk_bodies = chunks
        .iter()
        .map(|symbol| function_body(&split, symbol))
        .collect::<Vec<_>>();
    let block_type = "{ i64, [0 x i64] }";
    let inverse =
        format!("ptrtoint (ptr getelementptr ({block_type}, ptr null, i64 0, i32 1) to i64)");
    for chunk in &chunk_bodies {
        assert!(
            chunk.contains(&inverse),
            "each nested chunk must reconstruct its payload capture:\n{chunk}"
        );
    }
    let outer_chunk = chunk_bodies
        .iter()
        .find(|chunk| chunk.contains("call i64 @wf__par_split_"))
        .expect("the outer chunk must enter the inner splitter");
    assert!(
        outer_chunk.lines().any(|line| {
            line.contains("getelementptr inbounds { i64, [0 x i64] }, ptr ")
                && line.contains("i64 0, i32 1, i64 0")
        }),
        "the outer chunk must project the reconstructed owner into the inner split:\n{outer_chunk}"
    );
    let nested = function_body(&split, "@wf_nested");
    assert!(
        nested.lines().any(|line| {
            line.contains("getelementptr inbounds { i64, [0 x i64] }, ptr ")
                && line.contains("i64 0, i32 1, i64 0")
        }),
        "the source owner must be projected into the outer split:\n{nested}"
    );

    let directory = test_directory();
    let reference = Command::new(build_executable(&unsplit, &directory))
        .bounded_output()
        .expect("run the unsplit nested reductions");
    assert_eq!(reference.status.code(), Some(0), "{reference:?}");
    assert!(reference.stdout.is_empty());

    let executable = build_executable(&split, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .bounded_output()
            .expect("run the nested split reductions");
        assert_eq!(
            output.status.code(),
            Some(0),
            "WF_WORKERS={workers}: {output:?}"
        );
        assert_eq!(output.stdout, reference.stdout, "WF_WORKERS={workers}");
    }
    let outer_splitter = splitters
        .iter()
        .find(|symbol| nested.contains(&format!("call i64 {symbol}(")))
        .expect("the enclosing function enters the outer splitter");
    let outer_chunk_symbol = chunks
        .iter()
        .find(|symbol| {
            function_body(&split, outer_splitter).contains(&format!("call i64 {symbol}("))
        })
        .expect("the outer splitter has one chunk");
    let observed =
        observe_outer_loop_budget(&split, "@wf_nested", outer_splitter, outer_chunk_symbol);
    let observer = format!(
        "#define WF_TEST_NESTED 1\n{}\n{OUTER_LOOP_BUDGET_OBSERVER}",
        super::parallel::WORKER_SCHEDULE,
    );
    let executable = super::build_linked_executable(&observed, Some(&observer), &[], &directory);
    for zero in [false, true] {
        let mut command = Command::new(&executable);
        command
            .env("WF_WORKERS", "4")
            .env_remove("WF_TEST_ZERO_BUDGET");
        if zero {
            command.env("WF_TEST_ZERO_BUDGET", "1");
        }
        let output = command
            .bounded_output()
            .expect("run observed nested loop budget");
        assert_eq!(output.status.code(), Some(0), "zero={zero}: {output:?}");
        assert_eq!(output.stdout, reference.stdout);
    }
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

#[test]
fn an_independent_map_joins_and_preserves_its_outer_buffer() {
    let unsplit = emit(INDEPENDENT_MAP);
    assert!(
        !module_requires_parallel_runtime(&unsplit),
        "the default map lowering must remain the ordinary loop"
    );
    let split = emit_with_overlap(INDEPENDENT_MAP);
    assert!(
        module_requires_parallel_runtime(&split),
        "the proved map must actualize when overlap lowering is requested:\n{split}"
    );

    let splitter_symbol = synthesized(&split, "@wf__par_split_");
    let chunk_symbol = synthesized(&split, "@wf__par_chunk_");
    let splitter = function_body(&split, &splitter_symbol);
    let chunk = function_body(&split, &chunk_symbol);
    assert!(
        chunk.starts_with("define i8 "),
        "an independent map chunk must return the Unit token:\n{chunk}"
    );
    assert!(
        splitter.starts_with("define i8 "),
        "an independent map splitter must return the Unit token:\n{splitter}"
    );
    // STOR-1 puts the descriptor inside the allocation. The complete chunk
    // signature is Unit seed, two bounds, and the stable thin Box pointer
    // snapshotted before the split. Native bytes and the source owner's
    // per-exit releases are checked below.
    assert_map_payload_capture(&split, chunk, "i8", "i8");
    for step in [
        "call ptr @wf__par_acquire_lane(",
        "call void @wf__par_publish(",
        "call void @wf__par_join(",
        "call void @wf__par_release(",
    ] {
        assert!(
            splitter.contains(step),
            "the independent map must retain the worker join, missing {step}:\n{splitter}"
        );
    }
    assert!(
        splitter.contains(&format!("call i8 {chunk_symbol}(")),
        "the map splitter's leaf must be the Unit-returning loop chunk:\n{splitter}"
    );

    // Captures are proof-scoped aliases, not source owners. Neither helper may
    // release the captured descriptor, while each selectable outer main owns
    // and frees exactly the buffer returned after the joined loop.
    //
    // KEPT AS WRITTEN for the lowering port: the `@wf__heap_give` counts are the
    // emitted release shape of one `Box<Array<u8>>` per return path [STOR-3].
    // If the release lowering of a boxed run changes, re-derive the counts.
    assert!(!chunk.contains("call void @wf__heap_give("), "{chunk}");
    assert!(
        !splitter.contains("call void @wf__heap_give("),
        "{splitter}"
    );
    for outer in ["@wf_main", "@wf__par_seq_main"] {
        let body = function_body(&split, outer);
        // The entry waits, so each source return leaves by a branch to the
        // frame's final block, and the one `ret` is the frame's return to
        // its resumer from the block every suspension shares.
        let mut releases_in_block = 0;
        let mut returning_blocks = 0;
        let mut block = "";
        for line in body.lines() {
            if let Some(label) = line.strip_suffix(':') {
                releases_in_block = 0;
                block = label;
            }
            releases_in_block += usize::from(line.contains("call void @wf__heap_give("));
            let instruction = line.trim_start();
            if instruction == "br label %wf.coro.final"
                || (instruction.starts_with("ret ") && block != "wf.coro.suspended")
            {
                returning_blocks += 1;
                assert_eq!(
                    releases_in_block, 1,
                    "each {outer} return path must release its one source-owned buffer once:\n{body}"
                );
            }
        }
        assert_eq!(returning_blocks, 2, "the fixture has two result arms");
    }

    let directory = test_directory();
    let reference = Command::new(build_executable(&unsplit, &directory))
        .bounded_output()
        .expect("run the map that splits nothing");
    assert_eq!(reference.status.code(), Some(0));
    assert_eq!(reference.stdout.len(), 400000);

    let executable = build_executable(&split, &directory);
    let mut runs = vec![("no split lowering".to_owned(), reference.stdout)];
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .bounded_output()
            .expect("run the split map");
        assert_eq!(output.status.code(), Some(0), "WF_WORKERS={workers}");
        assert_eq!(output.stdout.len(), 400000, "WF_WORKERS={workers}");
        runs.push((format!("WF_WORKERS={workers}"), output.stdout));
    }
    identical(&runs).expect("splitting an independent map must not move one output byte");

    let observed = observe_outer_loop_budget(&split, "@wf_mapped", &splitter_symbol, &chunk_symbol);
    let observer = format!(
        "#define WF_TEST_NESTED 0\n{}\n{OUTER_LOOP_BUDGET_OBSERVER}",
        super::parallel::WORKER_SCHEDULE,
    );
    let executable = super::build_linked_executable(&observed, Some(&observer), &[], &directory);
    for zero in [false, true] {
        let mut command = Command::new(&executable);
        command
            .env("WF_WORKERS", "4")
            .env_remove("WF_TEST_ZERO_BUDGET");
        if zero {
            command.env("WF_TEST_ZERO_BUDGET", "1");
        }
        let output = command
            .bounded_output()
            .expect("run observed Unit-map loop budget");
        assert_eq!(output.status.code(), Some(0), "zero={zero}: {output:?}");
        assert_eq!(output.stdout, runs[0].1);
    }

    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// PAR-2's read-side map and unique-holder routes use the ordinary counted
/// splitter. Running the same program with and without that lowering proves
/// that the new permission changes scheduling only, not the published bytes.
#[test]
fn a_borrowed_read_modify_map_preserves_the_sequential_bytes() {
    let source = borrowed_read_modify_map_source();
    let unsplit = emit(&source);
    let split = emit_with_overlap(&source);
    assert!(
        module_requires_parallel_runtime(&split),
        "the proved holder map must actualize when overlap lowering is requested"
    );

    let directory = test_directory();
    let reference = Command::new(build_executable(&unsplit, &directory))
        .bounded_output()
        .expect("run the sequential holder map");
    assert_eq!(reference.status.code(), Some(0));
    assert_eq!(reference.stdout.len(), 400000);

    let executable = build_executable(&split, &directory);
    let mut runs = vec![("no split lowering".to_owned(), reference.stdout)];
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .bounded_output()
            .expect("run the split holder map");
        assert_eq!(output.status.code(), Some(0), "WF_WORKERS={workers}");
        assert_eq!(output.stdout.len(), 400000, "WF_WORKERS={workers}");
        runs.push((format!("WF_WORKERS={workers}"), output.stdout));
    }
    identical(&runs).expect("the borrowed read-modify map must preserve every output byte");

    let (granted, output) =
        CountedProgram::link(&control_range_transport(&split), &directory).run(Some("4"));
    assert!(granted > 0, "the map must execute a real worker callback");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, runs[0].1);
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// Each iteration fills one proved-disjoint row through a unit helper written
/// as a [GRAM-4] expression statement, the natural spelling of a call whose
/// result is `unit`. PAR-2 judges that call by its row exactly as it judges a
/// let-bound one, so the row loop is split as an independent map.
const EXPRESSION_STATEMENT_ROWS: &[u8] =
    br#"fn fill_row(output: &[u64], value: u64) -> result: unit writes(output) {
  let count = output^.len;
  for (i in 0_u64..count) {
    set output^[i] = value;
  }
  return unit;
}

fn rows(width: u64) -> result: Box<Array<u64>> pure contract {
  requires width <= 4096_u64;
} {
  let cells = 64_u64 * width;
  let values = box_array_filled::<u64>(count: cells, value: 0_u64);
  for (r in 0_u64..64_u64) {
    let start = r * width;
    let end = start + width;
    invariant bounded: end <= cells {
      use width times (r + 1_u64 <= 64_u64);
    }
    let row = &values.inner[start..end];
    fill_row(output: row, value: r);
  }
  return move values;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = rows(width: 1024_u64);
  let count = values.inner.len;
  if count != 65536_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  for (i in 0_u64..count) {
    let seen = values.inner[i];
    let expected = i / 1024_u64;
    if seen != expected {
      return std::process::exit_status(code: 2_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn an_expression_statement_row_map_is_split_and_keeps_its_rows() {
    let ledger = super::compile_permission_ledger(EXPRESSION_STATEMENT_ROWS);
    assert!(
        ledger
            .iter()
            .any(|line| line.starts_with("PAR split") && line.contains(" rows ")),
        "the row loop must be split as an independent map: {ledger:?}"
    );
    let unsplit = emit(EXPRESSION_STATEMENT_ROWS);
    let split = emit_with_overlap(EXPRESSION_STATEMENT_ROWS);
    let directory = test_directory();
    let reference = Command::new(build_executable(&unsplit, &directory))
        .bounded_output()
        .expect("run the row map that splits nothing");
    assert_eq!(reference.status.code(), Some(0), "{reference:?}");
    let executable = build_executable(&split, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .bounded_output()
            .expect("run the split row map");
        assert_eq!(
            output.status.code(),
            Some(0),
            "WF_WORKERS={workers}: every row must hold its own index"
        );
    }
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// [CALL-4] a map whose body binds a two-result call: each iteration's
/// binders are its own, so the loop splits, and `main` checks every element
/// against the sequential computation.
const ORDERED_RESULT_MAP: &[u8] = br#"fn halves(seed: u64) -> (high: u64, low: u64) pure {
  let state = seed;
  for (round in 0_u64..64_u64) {
    let scaled = state *wrap 6364136223846793005_u64;
    set state = scaled +wrap 1442695040888963407_u64;
  }
  let high = state / 4294967296_u64;
  let low = iand(state, 4294967295_u64);
  return high, low;
}

fn mapped(count: u64) -> result: Box<Array<u64>> pure contract {
  requires count <= 1000000_u64;
} {
  let out = box_array_filled::<u64>(count: count, value: 0_u64);
  for (i in 0_u64..count) {
    let (high, low) = halves(seed: i);
    set out.inner[i] = high +wrap low;
  }
  return move out;
}

fn main() -> status: std::process::ExitStatus pure {
  let count = 200000_u64;
  let out = mapped(count: count);
  if out.inner.len != count {
    return std::process::exit_status(code: 1_u8);
  }
  for (i in 0_u64..count) {
    let (high, low) = halves(seed: i);
    let expected = high +wrap low;
    let seen = out.inner[i];
    if seen != expected {
      return std::process::exit_status(code: 2_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn a_map_binding_an_ordered_result_list_is_split_and_keeps_its_elements() {
    let ledger = super::compile_permission_ledger(ORDERED_RESULT_MAP);
    assert!(
        ledger
            .iter()
            .any(|line| line.starts_with("PAR split") && line.contains(" mapped ")),
        "the binder-list loop must be split as an independent map: {ledger:?}"
    );
    let unsplit = emit(ORDERED_RESULT_MAP);
    let split = emit_with_overlap(ORDERED_RESULT_MAP);
    assert!(module_requires_parallel_runtime(&split));
    let directory = test_directory();
    let reference = Command::new(build_executable(&unsplit, &directory))
        .bounded_output()
        .expect("run the binder-list map that splits nothing");
    assert_eq!(reference.status.code(), Some(0), "{reference:?}");
    let executable = build_executable(&split, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .bounded_output()
            .expect("run the split binder-list map");
        assert_eq!(
            output.status.code(),
            Some(0),
            "WF_WORKERS={workers}: every element must hold its iteration's two results"
        );
    }
    let (granted, output) =
        CountedProgram::link(&control_range_transport(&split), &directory).run(Some("4"));
    assert!(
        granted > 0,
        "ordered result-list capture must execute a real worker"
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, reference.stdout);
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// A loop that maps and reduces still selects the Reduction result path. The
/// full map and all eight reduction bytes are independently observable.
#[test]
fn a_map_and_reduction_preserves_both_results() {
    let source = map_and_reduction_source();
    let unsplit = emit(&source);
    let split = emit_with_overlap(&source);
    assert!(module_requires_parallel_runtime(&split));

    let chunk = function_body(&split, &synthesized(&split, "@wf__par_chunk_"));
    let splitter = function_body(&split, &synthesized(&split, "@wf__par_split_"));
    assert!(
        chunk.starts_with("define i64 "),
        "the combined loop must retain its real reduction result:\n{chunk}"
    );
    assert!(
        splitter.starts_with("define i64 "),
        "the combined loop must retain its real reduction result:\n{splitter}"
    );
    // The reduction seed replaces Unit; bounds and the one stable thin Box
    // pointer retain exactly the independent map's capture ABI.
    assert_map_payload_capture(&split, chunk, "i64", "i8");

    let directory = test_directory();
    let reference = Command::new(build_executable(&unsplit, &directory))
        .bounded_output()
        .expect("run the combined loop that splits nothing");
    assert_eq!(reference.status.code(), Some(0));
    assert_eq!(reference.stdout.len(), 400008);

    let executable = build_executable(&split, &directory);
    let mut runs = vec![("no split lowering".to_owned(), reference.stdout)];
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .bounded_output()
            .expect("run the combined split loop");
        assert_eq!(output.status.code(), Some(0), "WF_WORKERS={workers}");
        runs.push((format!("WF_WORKERS={workers}"), output.stdout));
    }
    identical(&runs).expect("the combined map and reduction must preserve every byte");

    let (granted, output) =
        CountedProgram::link(&control_range_transport(&split), &directory).run(Some("4"));
    assert!(granted > 0, "the map must execute a real worker callback");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, runs[0].1);
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// One admitted combine, at one accumulator type, as a fold a program actually
/// runs.
///
/// The fixtures above reach two of the ten combines at one width. The rest of
/// the set was covered only by `the_identity_of_every_admitted_combine_is_two_sided`
/// in `lowering::builder::split`, which re-implements the operations and so
/// cannot see a lowering that is wrong for one particular combine. These rows
/// close that: every combine, at `u64` and at `u8` where the combine has a
/// width, folded by a real program whose bytes are compared against the
/// lowering that splits nothing.
struct Combine {
    /// Names the fold's function and the row a failure reports.
    name: &'static str,
    /// The [OP-1] spelling the ledger prints, which is what says this row
    /// reached the combine it meant to.
    spelling: &'static str,
    /// The accumulator's written type.
    ty: &'static str,
    /// The accumulator's initial value.
    seed: &'static str,
    /// Statements binding `element` from `mixed`, in order. They are separate
    /// `let`s because a call is not an operand of another operation here.
    element: &'static [&'static str],
    /// The right-hand side of the body's `set total = ...`.
    fold: &'static str,
    /// How the accumulator becomes the `u64` the row publishes.
    publish: &'static str,
}

/// The `u64` accumulator publishes itself.
const PUBLISH_WIDE: &str = "  return total;\n";
/// The `u8` accumulator widens, which is a total pair and so binds directly.
const PUBLISH_NARROW: &str = "  let wide = cvt::<u8, u64>(total);\n  return wide;\n";
/// `Bool` has no numeric conversion, so the branch is written out.
const PUBLISH_BOOL: &str =
    "  let wide = 0_u64;\n  if total {\n    set wide = 1_u64;\n  }\n  return wide;\n";

/// Every admitted combine, at every accumulator type it carries.
///
/// Each element expression is chosen so the fold's own value is not the value a
/// wrong identity element would produce: `iand` keeps a mask alive rather than
/// collapsing to zero, `imin` folds a range whose minimum is not zero, `imax`
/// one whose maximum is not the type's, and `band` fold to `True` where `bor`
/// folds to `False`. Each bisection carries the incoming seed on the left and
/// introduces one identity on the right. The odd 257-element range exercises
/// uneven leaves; the independent identity table also covers signed widths.
const ADMITTED_COMBINES: &[Combine] = &[
    Combine {
        name: "add_wide",
        spelling: "+wrap",
        ty: "u64",
        seed: "0_u64",
        element: &["let element = mixed;"],
        fold: "total +wrap element",
        publish: PUBLISH_WIDE,
    },
    Combine {
        name: "mul_wide",
        spelling: "*wrap",
        ty: "u64",
        seed: "1_u64",
        element: &["let element = ior(mixed, 1_u64);"],
        fold: "total *wrap element",
        publish: PUBLISH_WIDE,
    },
    Combine {
        name: "and_wide",
        spelling: "iand",
        ty: "u64",
        seed: "18446744073709551615_u64",
        element: &["let element = ior(mixed, 12297829382473034410_u64);"],
        fold: "iand(total, element)",
        publish: PUBLISH_WIDE,
    },
    Combine {
        name: "or_wide",
        spelling: "ior",
        ty: "u64",
        seed: "0_u64",
        element: &["let element = iand(mixed, 4886718345_u64);"],
        fold: "ior(total, element)",
        publish: PUBLISH_WIDE,
    },
    Combine {
        name: "xor_wide",
        spelling: "ixor",
        ty: "u64",
        seed: "0_u64",
        element: &["let element = mixed;"],
        fold: "ixor(total, element)",
        publish: PUBLISH_WIDE,
    },
    Combine {
        name: "min_wide",
        spelling: "imin",
        ty: "u64",
        seed: "18446744073709551615_u64",
        element: &["let element = mixed;"],
        fold: "imin(total, element)",
        publish: PUBLISH_WIDE,
    },
    Combine {
        name: "max_wide",
        spelling: "imax",
        ty: "u64",
        seed: "0_u64",
        element: &["let element = mixed;"],
        fold: "imax(total, element)",
        publish: PUBLISH_WIDE,
    },
    Combine {
        name: "add_narrow",
        spelling: "+wrap",
        ty: "u8",
        seed: "0_u8",
        element: &["let element = low_byte(v: mixed);"],
        fold: "total +wrap element",
        publish: PUBLISH_NARROW,
    },
    Combine {
        name: "mul_narrow",
        spelling: "*wrap",
        ty: "u8",
        seed: "1_u8",
        element: &[
            "let low = low_byte(v: mixed);",
            "let element = ior(low, 1_u8);",
        ],
        fold: "total *wrap element",
        publish: PUBLISH_NARROW,
    },
    Combine {
        name: "and_narrow",
        spelling: "iand",
        ty: "u8",
        seed: "255_u8",
        element: &[
            "let low = low_byte(v: mixed);",
            "let element = ior(low, 170_u8);",
        ],
        fold: "iand(total, element)",
        publish: PUBLISH_NARROW,
    },
    Combine {
        name: "or_narrow",
        spelling: "ior",
        ty: "u8",
        seed: "0_u8",
        element: &[
            "let low = low_byte(v: mixed);",
            "let element = iand(low, 5_u8);",
        ],
        fold: "ior(total, element)",
        publish: PUBLISH_NARROW,
    },
    Combine {
        name: "xor_narrow",
        spelling: "ixor",
        ty: "u8",
        seed: "0_u8",
        element: &["let element = low_byte(v: mixed);"],
        fold: "ixor(total, element)",
        publish: PUBLISH_NARROW,
    },
    Combine {
        name: "min_narrow",
        spelling: "imin",
        ty: "u8",
        seed: "255_u8",
        element: &[
            "let low = low_byte(v: mixed);",
            "let element = ior(low, 3_u8);",
        ],
        fold: "imin(total, element)",
        publish: PUBLISH_NARROW,
    },
    Combine {
        name: "max_narrow",
        spelling: "imax",
        ty: "u8",
        seed: "0_u8",
        element: &[
            "let low = low_byte(v: mixed);",
            "let element = iand(low, 63_u8);",
        ],
        fold: "imax(total, element)",
        publish: PUBLISH_NARROW,
    },
    Combine {
        name: "and_bool",
        spelling: "band",
        ty: "Bool",
        seed: "True()",
        element: &["let element = mixed >= 0_u64;"],
        fold: "band(total, element)",
        publish: PUBLISH_BOOL,
    },
    Combine {
        name: "or_bool",
        spelling: "bor",
        ty: "Bool",
        seed: "False()",
        element: &["let element = mixed < 0_u64;"],
        fold: "bor(total, element)",
        publish: PUBLISH_BOOL,
    },
    Combine {
        name: "xor_bool",
        spelling: "bxor",
        ty: "Bool",
        seed: "False()",
        element: &[
            "let bit = iand(mixed, 1_u64);",
            "let element = bit == 1_u64;",
        ],
        fold: "bxor(total, element)",
        publish: PUBLISH_BOOL,
    },
];

/// Odd range for controlled per-row worker executions. The observer raises
/// only the emitted price operand; separate tests lock the production grain.
const COMBINE_SPAN: u64 = 257;

/// The helpers every row's fold shares: the per-iteration mix that gives the
/// body enough weight to be worth splitting, the narrowing to a byte, and the
/// eight-byte spelling each row publishes through.
const COMBINE_PRELUDE: &str = r#"fn mix(seed: u64) -> result: u64 pure {
  doc "A pure mix with enough arithmetic that splitting the range around it pays.";
  let state = seed;
  let round = 0_u64;
  loop @rounds {
    let done = round == 24_u64;
    if done {
      break @rounds;
    }
    let shifted = irotl(state, 27_u32);
    let scaled = state *wrap 6364136223846793005_u64;
    set state = ixor(shifted, scaled);
    set state = state +wrap 1442695040888963407_u64;
    set round = round +wrap 1_u64;
  }
  return state;
}

fn low_byte(v: u64) -> result: u8 pure {
  let low = iand(v, 255_u64);
  match cvt.checked::<u64, u8>(low) {
    Ok(value: byte) => {
      return byte;
    }
    Err(error: problem) => {
      return 0_u8;
    }
  }
}

fn spell(destination: &[u8], at: u64, value: u64) -> result: u64 writes(destination) {
  let cursor = at;
  let rest = value;
  loop @octets {
    let limit = at +wrap 8_u64;
    let done = cursor >= limit;
    if done {
      break @octets;
    }
    let spare = destination^.len;
    let writable = cursor < spare;
    if writable {
      let byte = low_byte(v: rest);
      set destination^[cursor] = byte;
    }
    set rest = irotr(rest, 8_u32);
    set cursor = cursor +wrap 1_u64;
  }
  return at +wrap 8_u64;
}
"#;

/// One program whose every row of [`ADMITTED_COMBINES`] is a permitted counted
/// loop, publishing each row's fold as eight bytes in table order.
fn admitted_combine_source() -> Vec<u8> {
    let mut source = String::from(COMBINE_PRELUDE);
    for combine in ADMITTED_COMBINES {
        let Combine {
            name,
            ty,
            seed,
            element,
            fold,
            publish,
            ..
        } = combine;
        source.push_str(&format!(
            "\nfn fold_{name}(lo: u64, hi: u64) -> result: {ty} pure {{\n  \
             let total = {seed};\n  for @points (i in lo..hi) {{\n    \
             let mixed = mix(seed: i);\n"
        ));
        for statement in *element {
            source.push_str(&format!("    {statement}\n"));
        }
        // `after` is the previous row's write cursor, and `imin(after, 0)` is
        // zero for every unsigned value it can hold — so the fold's range is
        // the same for every row, and no two of `main`'s calls are independent.
        // That is what keeps every lane this program is granted a range split
        // rather than an overlapped window pair, which is what the grant
        // assertion below has to be about.
        source.push_str(&format!(
            "    set total = {fold};\n  }}\n  return total;\n}}\n\n\
             fn value_{name}(after: u64) -> result: u64 pure {{\n  \
             let lo = imin(after, 0_u64);\n  \
             let total = fold_{name}(lo: lo, hi: {COMBINE_SPAN}_u64);\n{publish}}}\n"
        ));
    }
    let width = 8 * ADMITTED_COMBINES.len();
    source.push_str(&format!(
        "\nfn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure waits {{\n  \
         let std::process::Inputs(args: unused_args, drivers: unused_driver_count, cwd: cwd_directory, stdout: out, stderr: unused_stderr, handles: factory, stdin: unused_stdin, clock: unused_clock, wall_clock: unused_wall_clock, stops: unused_stops, memory_meter: unused_memory_meter, host_paths: unused_host_paths, process_id: unused_process_id) = move inputs;\n  \
         let std::fs::Directory(read: cwd, write: cwd_write) = move cwd_directory;\n  \
         std::fs::close_directory_write(factory: &factory, directory: move cwd_write);\n  \
         std::fs::close_directory(factory: &factory, directory: move cwd);\n  \
         let report = box_array_filled::<u8>(count: {width}_u64, value: 0_u8);\n  \
         let window = &report.inner[0_u64..{width}_u64];\n"
    ));
    let mut at = "0_u64".to_owned();
    for (index, combine) in ADMITTED_COMBINES.iter().enumerate() {
        let name = combine.name;
        source.push_str(&format!(
            "  let v{index} = value_{name}(after: {at});\n  \
             let a{index} = spell(destination: window, at: {at}, value: v{index});\n"
        ));
        at = format!("a{index}");
    }
    source.push_str(&format!(
        "  let no_deadline = None<std::time::Instant>();\n  \
         let wait_cancel_1 = std::time::cancel_never();\n  \
         let wait_outcome_1 = std::io::write_once(factory: &factory, output: &out, source: window, start: 0_u64, \
         end: {width}_u64, deadline: no_deadline, cancel: &wait_cancel_1);\n  \
         std::time::close_cancel_watch(watch: move wait_cancel_1);\n  \
         match wait_outcome_1 {{\n    Ok(value: accepted) => {{\n      \
         return std::process::exit_status(code: 0_u8);\n    }}\n    Err(error: problem) => {{\n      \
         return std::process::exit_status(code: 1_u8);\n    }}\n  }}\n}}\n"
    ));
    source.into_bytes()
}

/// Every admitted combine folds a real split range and publishes what the
/// lowering that splits nothing publishes.
///
/// Until this case the only combines a running program ever folded through the
/// split were `+wrap` and `ixor`, both at `u64`. Everything else rested on
/// `the_identity_of_every_admitted_combine_is_two_sided`, which takes the
/// identity and the operation from production but evaluates them through its
/// own re-implementation — so it catches a wrong identity constant and cannot
/// catch a wrong lowering for one particular combine.
///
/// The rows are one program rather than seventeen because the expensive half is
/// the link. One ordinary reference and one observed parallel image cover all
/// rows. Each row needs its permission entry, a real worker callback, and the
/// correct result; none can borrow a positive counter from another row.
#[test]
fn every_admitted_combine_splits_and_publishes_the_unsplit_bytes() {
    let source = admitted_combine_source();
    // KEPT AS WRITTEN for the lowering port: the ledger row spellings
    // (`PAR split`, `fold_NAME  loop at `, `split under OP over`) are the
    // permission reporter's own text. The subject - every admitted combine
    // reaches a real split - is unchanged; re-derive the strings if the
    // reporter's wording moves.
    let ledger = super::compile_permission_ledger(&source);
    for combine in ADMITTED_COMBINES {
        let expected = format!("fold_{}  loop at ", combine.name);
        let line = ledger
            .iter()
            .find(|line| line.starts_with("PAR split") && line.contains(&expected))
            .unwrap_or_else(|| {
                panic!(
                    "the {} row's loop must split, or the row checks nothing:\n{}",
                    combine.name,
                    ledger.join("\n")
                )
            });
        assert!(
            line.contains(&format!("split under {} over", combine.spelling)),
            "the {} row must split under {}: {line}",
            combine.name,
            combine.spelling
        );
    }

    let unsplit = emit(&source);
    assert!(
        !module_requires_parallel_runtime(&unsplit),
        "the reference module must contain no split at all"
    );
    let split = emit_with_overlap(&source);

    let directory = test_directory();
    let reference = Command::new(build_executable(&unsplit, &directory))
        .bounded_output()
        .expect("run the module that splits nothing");
    assert_eq!(reference.status.code(), Some(0));
    assert_eq!(
        reference.stdout.len(),
        8 * ADMITTED_COMBINES.len(),
        "every row publishes eight bytes"
    );

    // Each row's caller brackets a fully joined fold. Interposition delays
    // the owner's join until one actual nonowner callback has entered.
    let mut observed =
        super::parallel::observe_worker_schedule(&super::without_entry_selection(&split));
    let mut controlled = 0;
    for combine in ADMITTED_COMBINES {
        let symbol = format!("@wf_fold_{}", combine.name);
        let body = function_body(&observed, &symbol).to_owned();
        let mut calls = 0;
        let changed = body
            .lines()
            .map(|line| {
                if line.contains("call ") && line.contains("@wf__par_split_") {
                    calls += 1;
                    let (prefix, _) = line.rsplit_once(", i64 ").expect("driver price operand");
                    format!("{prefix}, i64 150000)")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            calls, 1,
            "{} must retain one controlled demand driver",
            combine.name
        );
        controlled += calls;
        observed = observed.replacen(&body, &changed, 1);
    }
    assert_eq!(
        controlled, 17,
        "every admitted combine must exercise a granted range"
    );
    for (index, combine) in ADMITTED_COMBINES.iter().enumerate() {
        let symbol = format!("@wf_value_{}", combine.name);
        let body = function_body(&observed, &symbol).to_owned();
        let entry = body
            .lines()
            .find(|line| line.ends_with(':'))
            .expect("entry block");
        let replacement = body
            .replacen(
                entry,
                &format!("{entry}\n  call void @wf_test_row_begin()"),
                1,
            )
            .replace(
                "  ret i64 ",
                &format!("  call void @wf_test_row_end(i32 {index})\n  ret i64 "),
            );
        assert!(replacement.contains(&format!("@wf_test_row_end(i32 {index})")));
        observed = observed.replacen(&body, &replacement, 1);
    }
    observed.push_str("\ndeclare void @wf_test_row_begin()\ndeclare void @wf_test_row_end(i32)\n");
    let observer = format!(
        "#define WF_TEST_SCHEDULE_MANUAL\n{}\n{}",
        super::parallel::WORKER_SCHEDULE,
        r#"
extern _Thread_local uint64_t wf__par_demand_word;
static unsigned completed_rows;
void wf_test_row_begin(void) {
    atomic_store_explicit((_Atomic uint64_t *)&wf__par_demand_word, 1, memory_order_release);
    wf_test_worker_schedule_begin();
}
void wf_test_row_end(unsigned row) {
    if (row != completed_rows || !atomic_load(&schedule_entered)) {
        fprintf(stderr, "combine row %u did not execute a real worker\n", row);
        exit(112);
    }
    wf_test_worker_schedule_end();
    ++completed_rows;
}
static void report_rows(void) {
    if (completed_rows != 17) { fputs("missing combine rows\n", stderr); _Exit(113); }
}
__attribute__((constructor)) static void register_rows(void) { atexit(report_rows); }
"#
    );
    let executable = super::build_linked_executable(&observed, Some(&observer), &[], &directory);
    let output = Command::new(executable)
        .env("WF_WORKERS", "4")
        .bounded_output()
        .expect("run all controlled combine rows");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_combine_rows(
        &reference.stdout,
        &output.stdout,
        "every row executed a worker",
    );

    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// Compares two runs of the combine program row by row, so a failure names the
/// combine that moved rather than an offset into a hundred and thirty-six
/// bytes.
fn assert_combine_rows(reference: &[u8], published: &[u8], setting: &str) {
    assert_eq!(
        published.len(),
        reference.len(),
        "{setting} published {} bytes, not {}",
        published.len(),
        reference.len()
    );
    for (index, combine) in ADMITTED_COMBINES.iter().enumerate() {
        let row = index * 8..index * 8 + 8;
        assert_eq!(
            published[row.clone()],
            reference[row],
            "{setting} moved the {} row, folded under {}",
            combine.name,
            combine.spelling
        );
    }
}

/// Two dynamic split loops surround an ordinary call join and precede a
/// loop-header phi. One image checks forced-request execution, execution where only idle
/// lanes may request, and the sequential world;
/// native construction also verifies every emitted phi predecessor.
#[test]
fn multiple_split_loops_and_an_ordinary_join_keep_phi_predecessors() {
    let source = br#"fn choose(value: u64) -> result: u64 pure {
  return imax(value, value);
}

fn composed(limit: u64) -> result: u64 pure contract {
  requires limit <= 4_u64;
} {
  let total = 5_u64;
  for @sum (i in 0_u64..limit) {
    set total = total +wrap i;
  }
  let a = choose(value: total);
  let b = choose(value: 17_u64);
  let c = choose(value: 19_u64);
  let ab = a +wrap b;
  let combined = ab +wrap c;
  let marks = array_filled::<u64, 4>(value: 0_u64);
  for @marks (j in 0_u64..limit) {
    set marks[j] = j +wrap combined;
  }
  let acc = marks[0_u64] +wrap marks[1_u64];
  let round = 0_u64;
  loop @carry {
    if round == 2_u64 {
      break @carry;
    }
    set acc = acc +wrap 1_u64;
    set round = round +wrap 1_u64;
  }
  return acc;
}

fn main() -> status: std::process::ExitStatus pure {
  let result = composed(limit: 4_u64);
  if result != 97_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = emit_with_overlap(source);
    let body = function_body(&module, "@wf_composed");
    assert_eq!(body.matches("call i64 @wf__par_split_composed.").count(), 1);
    assert_eq!(body.matches("call i8 @wf__par_split_composed.").count(), 1);
    assert!(body.contains("call void @wf__par_publish("));
    assert!(body.contains(" = phi i64 "));
    let sequential = function_body(&module, "@wf__par_seq_composed");
    assert!(!sequential.contains("@wf__par_publish("));

    let splitters = synthesized_symbols(&module, "@wf__par_split_");
    assert_eq!(splitters.len(), 2);
    let mut observed = module.to_owned();
    // Force a worthwhile price in the observer, leaving source lowering and
    // the source-result oracle intact while executing both fork branches.
    let changed = body
        .lines()
        .map(|line| {
            if line.contains("call i64 @wf__par_split_composed.")
                || line.contains("call i8 @wf__par_split_composed.")
            {
                let (prefix, _) = line.rsplit_once(", i64 ").expect("price operand");
                format!("{prefix}, i64 150000)")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    observed = observed.replacen(body, &changed, 1);
    for symbol in splitters {
        let body = function_body(&observed, &symbol).to_owned();
        let entry = body.lines().find(|line| line.ends_with(':')).unwrap();
        let changed = body.replacen(
            entry,
            &format!("{entry}\n  call void @wf_test_cfg_splitter()"),
            1,
        );
        observed = observed.replacen(&body, &changed, 1);
    }
    observed.push_str("\ndeclare void @wf_test_cfg_splitter()\n");
    let observer = r#"#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
extern _Thread_local uint64_t wf__par_demand_word;
static _Atomic unsigned splitters;
void wf_test_cfg_splitter(void) {
    atomic_fetch_add(&splitters, 1);
    atomic_store_explicit((_Atomic uint64_t *)&wf__par_demand_word,
        getenv("WF_TEST_POSITIVE_BUDGET") ? 1 : 0, memory_order_release);
}
static void report(void) {
    unsigned entered = atomic_load(&splitters);
    int sequential = strcmp(getenv("WF_WORKERS"), "1") == 0;
    int positive = getenv("WF_TEST_POSITIVE_BUDGET") != NULL;
    /* Without the observer's request an idle lane may still send an advisory
     * request (a failed steal scan does), so that run enters each splitter at
     * least once and may split; the forced request always splits. */
    if (sequential ? entered != 0 : (positive ? entered <= 2 : entered < 2)) {
        fprintf(stderr, "composed loops: splitters=%u\n", entered);
        _Exit(118);
    }
}
__attribute__((constructor)) static void register_report(void) { atexit(report); }
"#;
    let directory = test_directory();
    let executable = super::build_linked_executable(&observed, Some(observer), &[], &directory);
    for (workers, positive) in [("1", false), ("4", false), ("4", true)] {
        let mut command = Command::new(&executable);
        command
            .env("WF_WORKERS", workers)
            .env_remove("WF_TEST_POSITIVE_BUDGET");
        if positive {
            command.env("WF_TEST_POSITIVE_BUDGET", "1");
        }
        let output = command.bounded_output().expect("run composed split loops");
        assert_eq!(
            output.status.code(),
            Some(0),
            "WF_WORKERS={workers}, positive={positive}: {output:?}"
        );
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
    }
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// An empty range folds nothing, an inverted one folds nothing, and a one-wide
/// one folds exactly once.
///
/// The inverted case is the one with teeth. A splitter that computed `hi - lo`
/// before testing the endpoints wraps an inverted range to something near 2^64
/// and then descends into a range the loop never had; the loop it stands for
/// runs zero iterations. The command returns a nonzero status when the folded
/// value differs from the sequential result, so the disagreement is reported
/// here rather than somewhere downstream.
#[test]
fn a_degenerate_range_folds_to_the_accumulator_it_arrived_with() {
    let module = emit_with_overlap(EDGE_RANGES);
    assert!(
        module.contains("@wf__par_split_"),
        "the fixture's loop must actually split, or this checks nothing:\n{module}"
    );
    let directory = test_directory();
    let splitter = synthesized(&module, "@wf__par_split_");
    let body = function_body(&module, &splitter);
    let entry = body.lines().find(|line| line.ends_with(':')).unwrap();
    let changed = body.replacen(
        entry,
        &format!("{entry}\n  call void @wf_test_edge_entry()"),
        1,
    );
    // `folded` selects its ordinary closure for these spans at entry; the
    // observer enters the parallel body so every degenerate range reaches
    // the driver.
    let observed = super::without_entry_selection(&module).replacen(body, &changed, 1)
        + "\ndeclare void @wf_test_edge_entry()\n";
    let observer = r#"#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
extern _Thread_local uint64_t wf__par_demand_word;
static unsigned entries;
void wf_test_edge_entry(void) {
    ++entries;
    atomic_store_explicit((_Atomic uint64_t *)&wf__par_demand_word,
        getenv("WF_TEST_POSITIVE_BUDGET") ? 1 : 0, memory_order_release);
}
static void report(void) { if (entries != 3) { fputs("degenerate range entered a recursive split\n", stderr); _Exit(115); } }
__attribute__((constructor)) static void register_report(void) { atexit(report); }
"#;
    let executable = super::build_linked_executable(&observed, Some(observer), &[], &directory);
    for positive in [false, true] {
        let mut command = Command::new(&executable);
        command
            .env("WF_WORKERS", "4")
            .env_remove("WF_TEST_POSITIVE_BUDGET");
        if positive {
            command.env("WF_TEST_POSITIVE_BUDGET", "1");
        }
        let output = command
            .bounded_output()
            .expect("run controlled degenerate ranges");
        assert_eq!(
            output.status.code(),
            Some(0),
            "positive={positive}: {output:?}"
        );
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
    }
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// A loop whose lane frame would not fit declines at compile time, and says so.
///
/// The runtime refuses an acquisition whose frame is over the bound, so a split
/// emitted anyway would descend, be refused every lane, and run the whole range
/// on one thread having paid for the splitter — a sequentialization with no
/// report. Declining here converts that into a line naming the width.
#[test]
fn a_loop_whose_frame_is_too_wide_declines_and_says_so() {
    // KEPT AS WRITTEN for the lowering port: `declined:` and `lane frame` are
    // the permission reporter's own words for the capacity refusal.
    let ledger = super::compile_permission_ledger(WIDE_FRAME);
    let declined = ledger
        .iter()
        .find(|line| line.starts_with("PAR split"))
        .unwrap_or_else(|| panic!("the lowering must report what it did:\n{ledger:#?}"));
    assert!(
        declined.contains("declined:") && declined.contains("lane frame"),
        "the decline must name the frame it could not fit: {declined}"
    );
    assert!(
        ledger
            .iter()
            .any(|line| line.starts_with("PAR loop") && line.contains("permitted")),
        "the judgment must still permit the loop the lowering declined:\n{ledger:#?}"
    );

    let module = emit_with_overlap(WIDE_FRAME);
    assert!(
        !module.contains("@wf__par_split_"),
        "a declined loop must emit no splitter:\n{module}"
    );

    // The outer candidate first builds a rescuable inner reduction. Reuse
    // must retain it once, together with an ordinary sibling-call group, the
    // original parent Box slot and iteration-local allocation/cleanup.
    let expected = (0_u64..4).fold(7_u64, |total, seed| {
        let mut state = seed;
        for _ in 0..24 {
            state = state.rotate_left(27) ^ state.wrapping_mul(6364136223846793005);
            state = state.wrapping_add(1442695040888963407);
        }
        total.wrapping_add(state.wrapping_mul(2).wrapping_add(502))
    });
    let nested = std::str::from_utf8(WIDE_FRAME).expect("UTF-8 fixture")
        .replace("400000_u64", "4_u64")
        .replace("  let total = 0_u64;", "  let retained = box_array_filled::<u64>(count: 1_u64, value: 5_u64);\n  let total = 7_u64;")
        .replace("    let mixed = mix(seed: i);", "    let first = mix(seed: i);\n    let second = mix(seed: i);\n    let doubled = first +wrap second;\n    let local = box_new::<u64>(value: i);\n    let saved = retained.inner[0_u64];\n    let mixed = doubled +wrap saved;")
        .replace(
            "    let bias0 = mixed +wrap a0;",
            "    let partial = 0_u64;\n    let inner_zero = imin(i, 0_u64);\n    let inner_count = inner_zero +wrap 2_u64;\n    for @inner (j in 0_u64..inner_count) {\n      set partial = partial +wrap j;\n    }\n    let initial = mixed +wrap a0;\n    let bias0 = initial +wrap partial;",
        )
        .replace("  if total == 0_u64 {", &format!("  if total != {expected}_u64 {{"));
    let mapped = nested
        .replace("  let total = 7_u64;", "  let mapped = box_array_filled::<u64>(count: 4_u64, value: 0_u64);")
        .replace("    set total = total +wrap biased;", "    set mapped.inner[i] = biased;")
        .replace(
            &format!("  if total != {expected}_u64 {{"),
            &format!("  let first = mapped.inner[0_u64];\n  let second = mapped.inner[1_u64];\n  let third = mapped.inner[2_u64];\n  let fourth = mapped.inner[3_u64];\n  let left = first +wrap second;\n  let right = third +wrap fourth;\n  let total = left +wrap right;\n  if total != {}_u64 {{", expected.wrapping_sub(7)),
        );
    for source in [&nested, &mapped] {
        super::system::with_parallel_ir(source.as_bytes(), |program| {
            let rows = program.actualization_ledger();
            assert_eq!(
                rows.iter()
                    .filter(|line| line.contains("declined:"))
                    .count(),
                1
            );
            let parent = program
                .functions()
                .iter()
                .find(|function| function.name() == "main")
                .expect("ordinary parent function");
            assert!(
                !parent.overlaps().is_empty(),
                "the imported sibling-call group must survive"
            );
            assert!(
                parent.blocks().iter().any(|block| matches!(
                    block.terminator(), crate::IrTerminator::Jump { drops, .. } if !drops.is_empty()
                )),
                "iteration-local cleanup must remain on the backedge"
            );
            assert!(
                parent
                    .blocks()
                    .iter()
                    .flat_map(|block| block.instructions())
                    .all(|instruction| {
                        !matches!(
                            instruction,
                            crate::IrInstruction::Define {
                                operation: crate::IrOperation::RuntimeBoxPayload { .. }
                                    | crate::IrOperation::RuntimeBoxOwner { .. },
                                ..
                            }
                        )
                    }),
                "ordinary fallback must reuse the parent slot without payload reconstruction"
            );
            for source_call in parent.source_calls() {
                let present = parent.blocks().iter().flat_map(|block| block.instructions()).any(|instruction| {
                    matches!(instruction, crate::IrInstruction::Define {
                        result, operation: crate::IrOperation::Call { arguments, .. }, ..
                    } if *result == source_call.result() && arguments.len() == source_call.arguments().len())
                });
                assert!(
                    present,
                    "source-call metadata must still name its ordinary call"
                );
            }
            assert!(
                parent.source_calls().iter().any(|call| parent
                    .blocks()
                    .iter()
                    .flat_map(|block| block.instructions())
                    .any(
                        |instruction| matches!(instruction, crate::IrInstruction::Define {
                            result, operation: crate::IrOperation::Call { function, .. }, ..
                        } if *result == call.result()
                            && program.functions()[*function as usize]
                                .name()
                                .starts_with("box_array_filled"))
                    )),
                "the allocating call keeps its source-call metadata"
            );
            assert_eq!(
                rows.iter()
                    .filter(|line| line.contains("split under"))
                    .count(),
                1
            );
            assert_eq!(
                program
                    .functions()
                    .iter()
                    .filter(|function| function.synthesis() == Some(crate::IrSynthesis::Chunk))
                    .count(),
                1
            );
            let host = crate::target::TargetLayout::host().expect("supported test host");
            let module = crate::backend::emitter::emit_llvm_with_layout(program, host)
                .expect("ordinary lowering must retain valid nested synthesis ordinals")
                .into_string();
            assert_eq!(synthesized_symbols(&module, "@wf__par_split_").len(), 1);
            assert!(function_body(&module, "@wf_main").contains("call i64 @wf__par_split_"));
        });
        let module = emit_with_overlap(source.as_bytes());
        let directory = test_directory();
        let executable = build_executable(&module, &directory);
        for workers in ["1", "4"] {
            let output = Command::new(&executable)
                .env("WF_WORKERS", workers)
                .bounded_output()
                .expect("run reused ordinary outer loop");
            assert_eq!(
                output.status.code(),
                Some(0),
                "WF_WORKERS={workers}: {output:?}"
            );
        }
        let (granted, output) =
            CountedProgram::link(&control_range_transport(&module), &directory).run(Some("4"));
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(
            granted > 0,
            "the retained sibling calls must execute a worker callback"
        );
        std::fs::remove_dir_all(&directory).expect("remove the test directory");
    }
}

/// The production splitter executes against a deterministic scheduler oracle.
/// Its ordinary chunk remains available for the no-request value comparison;
/// recording chunks make enormous-boundary cases finite without traversing them.
#[test]
fn segmented_ranges_poll_at_the_grain_and_publish_only_a_requested_far_half() {
    let source = br#"fn folded(lo: u64, hi: u64) -> result: u64 pure {
  let total = 19_u64;
  for @points (i in lo..hi) {
    set total = total +wrap i;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = emit_with_overlap(source);
    let splitter = synthesized(&module, "@wf__par_split_");
    let chunk = synthesized(&module, "@wf__par_chunk_");
    let parameter_types = |symbol: &str| {
        let header = function_body(&module, symbol).lines().next().unwrap();
        let (_, arguments) = header.split_once(&format!("{symbol}(")).unwrap();
        let (arguments, _) = arguments.split_once(')').unwrap();
        arguments
            .split(", ")
            .map(|argument| argument.split_whitespace().next().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let chunk_types = parameter_types(&chunk);
    let driver_types = parameter_types(&splitter);
    assert!(
        chunk_types.len() >= 3 && chunk_types.iter().all(|ty| ty == "i64"),
        "the observer's scalar fold must retain a scalar seed, bounds and captures: {chunk_types:?}"
    );
    assert_eq!(
        driver_types.len(),
        chunk_types.len() + 1,
        "driver appends exactly its runtime price"
    );
    assert_eq!(&driver_types[..chunk_types.len()], &chunk_types);
    assert_eq!(driver_types.last().map(String::as_str), Some("i64"));
    let captures = chunk_types.len() - 3;
    assert!(captures <= 2, "only the two source bounds can be captured");
    let c_types = |length: usize| vec!["uint64_t"; length].join(", ");
    let capture_parameters = (0..captures)
        .map(|index| format!(", uint64_t capture{index}"))
        .collect::<String>();
    let capture_arguments = (0..captures)
        .map(|index| format!(", capture{index}"))
        .collect::<String>();
    let source_captures = ["lower", "upper"][..captures]
        .iter()
        .map(|value| format!(", {value}"))
        .collect::<String>();
    let mut observed = module
        .replace("@main(", "@wf_segmented_original_main(")
        .replace("@wf__main_body(", "@wf_segmented_original_body(")
        .replace(
            &format!("call i64 {chunk}("),
            "call i64 @wf_test_segmented_chunk(",
        )
        .replace(
            "call ptr @wf__par_acquire_lane(",
            "call ptr @wf_test_segmented_acquire(",
        )
        .replace(
            "call void @wf__par_publish(",
            "call void @wf_test_segmented_publish(",
        )
        .replace(
            "call void @wf__par_join(",
            "call void @wf_test_segmented_join(",
        )
        .replace(
            "call void @wf__par_release(",
            "call void @wf_test_segmented_release(",
        );
    observed.push_str(&format!("\ndeclare i64 @wf_test_segmented_chunk({})\ndeclare ptr @wf_test_segmented_acquire(i64)\ndeclare void @wf_test_segmented_publish(ptr, ptr)\ndeclare void @wf_test_segmented_join(ptr)\ndeclare void @wf_test_segmented_release(ptr)\n", chunk_types.join(", ")));
    let observer = SEGMENTED_RANGE_OBSERVER
        .replace("SPLITTER_SYMBOL", &splitter[1..])
        .replace("CHUNK_SYMBOL", &chunk[1..])
        .replace("DRIVER_TYPES", &c_types(driver_types.len()))
        .replace("CHUNK_TYPES", &c_types(chunk_types.len()))
        .replace("CAPTURE_PARAMETERS", &capture_parameters)
        .replace("CAPTURE_ARGUMENTS", &capture_arguments)
        .replace("SOURCE_CAPTURES", &source_captures);
    let directory = test_directory();
    let executable = super::build_linked_executable(&observed, Some(&observer), &[], &directory);
    for mode in [
        "slices",
        "late",
        "nondiv",
        "ceil",
        "ceil_below",
        "real",
        "odd",
        "edge",
        "single",
        "empty",
        "inverted",
        "threshold",
        "below",
        "full",
    ] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", "1")
            .env("WF_TEST_SEGMENTED", mode)
            .bounded_output()
            .expect("run segmented-range oracle");
        assert_eq!(output.status.code(), Some(0), "{mode}: {output:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{mode}: {output:?}"
        );
    }
    std::fs::remove_dir_all(directory).expect("remove segmented-range oracle");
}

const SEGMENTED_RANGE_OBSERVER: &str = r#"
#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
extern int wf__floor_run(int, char **);
extern _Thread_local uint64_t wf__par_demand_word;
extern uint64_t test_split(DRIVER_TYPES) __asm__("SPLITTER_SYMBOL");
extern uint64_t actual_chunk(CHUNK_TYPES) __asm__("CHUNK_SYMBOL");
static uint64_t lower, upper, midpoint, chunk_lo[16], chunk_hi[16];
static unsigned chunks, acquires, publishes, joins, releases;
static int real, full, late;
static void fail(const char *message) { fprintf(stderr, "segmented range: %s\n", message); exit(119); }
uint64_t wf_test_segmented_chunk(uint64_t seed, uint64_t lo, uint64_t hi CAPTURE_PARAMETERS) {
    if (lo >= hi || lo < lower || hi > upper) fail("out-of-range or empty chunk");
    if (full) {
        if (lo != UINT64_MAX / 2 || hi != lo + 150000 || acquires != 1 || publishes != 1)
            fail("full-width far-half arithmetic");
        _Exit(0);
    }
    if (chunks == 16) fail("unexpected chunk count");
    chunk_lo[chunks] = lo; chunk_hi[chunks++] = hi;
    if (late && chunks == 1) atomic_store_explicit((_Atomic uint64_t *)&wf__par_demand_word, 1, memory_order_release);
    return real ? actual_chunk(seed, lo, hi CAPTURE_ARGUMENTS) : seed + (hi - lo);
}
void *wf_test_segmented_acquire(uint64_t bytes) {
    ++acquires;
    if (bytes > 256) fail("unexpected frame width");
    void *frame = malloc(256);
    if (!frame) abort();
    return frame;
}
void wf_test_segmented_publish(void *frame, void (*run)(void *)) {
    ++publishes;
    atomic_store_explicit((_Atomic uint64_t *)&wf__par_demand_word, 0, memory_order_release);
    run(frame);
}
void wf_test_segmented_join(void *frame) { (void)frame; ++joins; }
void wf_test_segmented_release(void *frame) { ++releases; free(frame); }
int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    const char *mode = getenv("WF_TEST_SEGMENTED");
    uint64_t price = 1, asked = 0;
    lower = 0; upper = 150005;
    if (!strcmp(mode, "late")) { upper = 300001; late = 1; }
    else if (!strcmp(mode, "nondiv")) { upper = 21431; price = 7; }
    else if (!strcmp(mode, "ceil")) { upper = 21429; price = 7; asked = 1; }
    else if (!strcmp(mode, "ceil_below")) { upper = 21428; price = 7; asked = 1; }
    else if (!strcmp(mode, "real")) real = 1;
    else if (!strcmp(mode, "odd")) { upper = 5; price = 150000; asked = 1; }
    else if (!strcmp(mode, "edge")) { lower = UINT64_MAX - 5; upper = UINT64_MAX; price = 150000; asked = 1; }
    else if (!strcmp(mode, "single")) { lower = UINT64_MAX - 1; upper = UINT64_MAX; price = UINT64_MAX; asked = 1; }
    else if (!strcmp(mode, "empty")) { lower = upper; asked = 1; }
    else if (!strcmp(mode, "inverted")) { lower = upper + 1; asked = 1; }
    else if (!strcmp(mode, "threshold")) { upper = 2; price = 75000; asked = 1; }
    else if (!strcmp(mode, "below")) { upper = 2; price = 74999; asked = 1; }
    else if (!strcmp(mode, "full")) { upper = UINT64_MAX; asked = 1; full = 1; }
    else if (strcmp(mode, "slices")) fail("unknown mode");
    atomic_store_explicit((_Atomic uint64_t *)&wf__par_demand_word, asked, memory_order_release);
    midpoint = late ? 225000 : lower <= upper ? lower + (upper - lower) / 2 : lower;
    uint64_t actual = test_split(19, lower, upper SOURCE_CAPTURES, price);
    if (full) fail("full-width case did not observe a far chunk");
    uint64_t expected = 19;
    if (lower < upper) {
        if (real) for (uint64_t i = lower; i < upper; ++i) expected += i;
        else expected += upper - lower;
    }
    if (actual != expected) fail("lost seed or range contribution");
    int split = late || (asked && upper > lower && upper - lower > 1 && strcmp(mode, "below") && strcmp(mode, "ceil_below"));
    if (acquires != (unsigned)split || publishes != acquires || joins != acquires || releases != acquires)
        fail("request/grain gate or frame accounting");
    if (lower >= upper) { if (chunks) fail("degenerate range executed"); }
    else if (split) {
        uint64_t cursor = midpoint;
        unsigned near = 0;
        if (late && (chunks != 3 || chunk_lo[0] != 0 || chunk_hi[0] != 150000)) fail("late request missed its slice boundary");
        for (unsigned i = late ? 1 : 0; i < chunks; ++i) {
            if (!near && cursor == upper) { near = 1; cursor = late ? 150000 : lower; }
            uint64_t end = near ? midpoint : upper;
            if (chunk_lo[i] != cursor || chunk_hi[i] > end) fail("far-before-near partition");
            cursor = chunk_hi[i];
        }
        if (!near || cursor != midpoint) fail("half split lost a boundary");
    } else {
        uint64_t cursor = lower;
        for (unsigned i = 0; i < chunks; ++i) {
            if (chunk_lo[i] != cursor) fail("slice overlap or gap");
            cursor = chunk_hi[i];
        }
        if (cursor != upper) fail("slice tail lost");
        if ((!strcmp(mode, "slices") || real) &&
            (chunks != 2 || chunk_hi[0] != 150000 || chunk_lo[1] != 150000))
            fail("150000-unit request interval");
        if (!strcmp(mode, "nondiv") && (chunks != 2 || chunk_hi[0] != 21428 || chunk_lo[1] != 21428))
            fail("request interval must round down nondivisible prices");
    }
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
"#;

#[test]
fn compile_time_small_ranges_keep_ordinary_code_without_any_runtime_query() {
    let source = br#"fn small() -> result: u64 pure {
  let cells = array_filled::<u64, 3>(value: 0_u64);
  for @fill (i in 0_u64..3_u64) {
    set cells[i] = i +wrap 7_u64;
  }
  let sum = 19_u64;
  for @sum (i in 0_u64..3_u64) {
    set sum = sum +wrap i;
  }
  return sum +wrap cells[2_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  let result = small();
  if result != 31_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let parallel = emit_with_overlap(source);
    for absent in [
        "@wf__par_split_",
        "@wf__par_chunk_",
        "@wf__par_demand_word",
        "@wf__par_acquire_lane",
    ] {
        assert!(
            !parallel.contains(absent),
            "compile-time-small site retained {absent}:\n{parallel}"
        );
    }
    assert_eq!(
        function_body(&parallel, "@wf_small"),
        function_body(&emit(source), "@wf_small")
    );
    // With no offer left, the whole program, bootstrap included, selects no
    // pool (compiler/parallel-lowering): the entry body calls `main` directly.
    let program = crate::compile_with_overlap(
        &[crate::SourceInput::new("test.wf", source)],
        crate::CompilerLimits::default(),
        crate::OverlapLowering::On,
    )
    .expect("compile-time-small program emits");
    assert!(
        !program.contains("call i32 @wf__par_pool_active("),
        "a program with no remaining offer must not query the pool:\n{program}"
    );
    let bootstrap = function_body(&program, "@wf__main_body");
    assert!(
        bootstrap.contains("@\"wf_main\"(") && !bootstrap.contains("@\"wf__par_seq_main\"("),
        "the bootstrap must enter the one remaining world:\n{bootstrap}"
    );
}

#[test]
fn request_word_tls_kind_survives_target_emission_and_cached_fragments() {
    let source = br#"fn folded(lo: u64, hi: u64) -> result: u64 pure {
  let total = 19_u64;
  for @points (i in lo..hi) {
    set total = total +wrap i;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  let result = folded(lo: 0_u64, hi: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    for (triple, kind, definition) in [
        (
            "x86_64-unknown-linux-gnu",
            "thread_local(initialexec) global i64",
            "weak",
        ),
        (
            "x86_64-apple-darwin",
            "thread_local(initialexec) global i64",
            "weak",
        ),
        (
            crate::target::WINDOWS_X86_64_TRIPLE,
            "thread_local global i64",
            "external",
        ),
    ] {
        let target = crate::target::TargetLayout::for_triple(triple).expect("qualified target");
        let whole =
            super::system::with_ir_layout(source, crate::OverlapLowering::On, target, |program| {
                let prepared =
                    crate::backend::emitter::prepare_dispatch_layout(program, target, true)
                        .expect("partitionable ordinary and split layouts");
                let mut module = crate::backend::emitter::emit_prepared_llvm(&prepared, target)
                    .expect("target demand driver emission");
                module
                    .append(
                        crate::driver::launcher::render(program, "main")
                            .expect("ordinary launcher"),
                    )
                    .expect("target demand driver launcher dependencies");
                module
            });
        let expected = format!("@wf__par_demand_word = {definition} {kind}");
        assert!(whole.as_str().contains(&expected), "{triple}: {whole}");
        let restored = crate::LlvmModule::decode(&whole.encode()).expect("TLS cache roundtrip");
        assert_eq!(whole.as_str(), restored.as_str());
        for granularity in [
            crate::FragmentGranularity::Function,
            crate::FragmentGranularity::Module,
        ] {
            let original =
                crate::split_module(&whole, granularity).expect("target demand fragments");
            let cached = crate::split_module(&restored, granularity)
                .expect("cached target demand fragments");
            assert_eq!(original, cached, "{triple}");
            let mut polling = 0;
            for fragment in original {
                if fragment.lines().any(|line| {
                    line.contains("load atomic i64") && line.contains("@wf__par_demand_word")
                }) {
                    polling += 1;
                    let declaration = fragment
                        .lines()
                        .find(|line| line.starts_with("@wf__par_demand_word = "))
                        .expect("polling fragment must declare its TLS dependency");
                    assert!(declaration.contains(kind), "{triple}: {declaration}");
                    assert!(
                        declaration.ends_with(", align 8"),
                        "{triple}: {declaration}"
                    );
                }
            }
            assert!(polling > 0, "{triple}: retained demand driver must poll");
        }
    }
}
