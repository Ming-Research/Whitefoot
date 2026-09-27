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

use std::process::Command;

use super::parallel::{CountedProgram, clone_symbols, function_body, identical};
use super::{
    build_executable, emit, emit_with_overlap, module_requires_parallel_runtime, test_directory,
};

/// A counted `for` the judgment permits: one accumulator under `+wrap`, a
/// pure body doing real arithmetic per iteration, no write to
/// anything the iteration did not introduce, and no edge leaving the loop.
///
/// The body is an iterated integer mix rather than two operations, because the
/// split allowance refuses a range that is not worth splitting and a
/// two-operation body over this span would be refused — correctly, and then
/// this module would be measuring nothing. Its result is written to standard
/// output as eight bytes, so a difference anywhere in the fold is a difference
/// in the bytes.
const PERMITTED_FOLD: &[u8] = include_bytes!("../../../../tests/programs/parallel/range_fold.wf");

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
    let spare = deref(destination).len;
    let writable = cursor < spare;
    if writable {
      let byte = low_byte(v: rest);
      set deref(destination)[cursor] = byte;
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

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  let std::process::Inputs(args: unused_args, cwd: unused_cwd, stdout: out, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin) = move inputs;
  std::fs::close_directory(factory: &entry_factory, directory: move unused_cwd);
  let value = folded(salt: 9876543210_u64, rounds: 24_u64, stride: 7_u64);
  let report = box_array_filled::<u8>(count: 8_u64, value: 0_u8);
  let window = &report.inner[0_u64..8_u64];
  let stored = spell(destination: window, at: 0_u64, value: value);
  match std::io::write_once(factory: &entry_factory, output: &out, source: window, start: 0_u64, end: 8_u64) {
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

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  let std::process::Inputs(args: unused_args, cwd: unused_cwd, stdout: out, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin) = move inputs;
  std::fs::close_directory(factory: &entry_factory, directory: move unused_cwd);
  let report = mapped();
  let size = report.inner.len;
  let source = &report.inner[0_u64..size];
  match std::io::write_once(factory: &entry_factory, output: &out, source: source, start: 0_u64, end: size) {
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
/// that transport is outside the rescue-only capture optimization.
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

fn main() -> status: std::process::ExitStatus pure {
  let empty = mapped(count: 0_u64);
  if empty.inner.len != 0_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let output = mapped(count: 400000_u64);
  if output.inner.len != 400000_u64 {
    return std::process::exit_status(code: 2_u8);
  }
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
  return std::process::exit_status(code: 0_u8);
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
            "fn mapped(out: &Box<Array<u8>>) -> result: unit writes(out.inner) contract {\n  define spare = deref(out).inner.len;\n  requires 400000_u64 <= spare;\n} {\n",
            1,
        )
        .replacen(
            "    set out.inner[slot] = byte;\n",
            "    let old = deref(out).inner[slot];\n    let blended = old +wrap byte;\n    set deref(out).inner[slot] = blended;\n",
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

/// A permitted loop is emitted as a chunk, a splitter, and one ordinary
/// hand-out of the splitter's left half.
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
    assert!(module.contains("define weak i64 @wf__par_split_budget("));
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

    // Asked once per loop entry, in the enclosing function, and never inside
    // the chunk or per iteration.
    assert_eq!(
        module.matches("call i64 @wf__par_split_budget(").count(),
        1,
        "the split allowance must be asked exactly once, at loop entry:\n{module}"
    );
    assert!(
        function_body(&module, "@wf_folded").contains("call i64 @wf__par_split_budget("),
        "the allowance must be asked where the loop is entered:\n{module}"
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

/// The sequential world of a split loop is the loop.
///
/// Two things carry that. The enclosing function's clone calls the chunk's
/// clone and reaches no runtime entry point — no lane acquisition, no allowance query, no
/// join — so nothing about a split is executed there. And the chunk's clone is
/// the chunk, byte for byte once its own symbols are restored, so the loop the
/// sequential world runs is not merely similar to the one the overlapped world
/// runs at its leaf: it is the same lowering.
///
/// The clone exists at all so that each copy has exactly one caller and the
/// loop is inlined back into it. Sharing one chunk between the worlds would
/// give it two callers and cost the sequential world that inlining, which is
/// the whole reason the second copy is cheap.
#[test]
fn the_sequential_world_of_a_split_loop_is_the_loop() {
    let module = fold_module(true);

    let chunk_symbol = synthesized(&module, "@wf__par_chunk_");
    let splitter_symbol = synthesized(&module, "@wf__par_split_");
    let chunk_clone_symbol = chunk_symbol.replace("@wf_", "@wf__par_seq_");

    let mut cloned = clone_symbols(&module);
    cloned.sort_unstable();
    assert_eq!(
        cloned,
        [
            chunk_clone_symbol.as_str(),
            "@wf__par_seq_folded",
            "@wf__par_seq_main"
        ],
        "the clone set must be the enclosing closure plus the chunk, and never the splitter:\n{module}"
    );

    let clone = function_body(&module, "@wf__par_seq_folded");
    assert!(
        clone.contains(&format!("call i64 {chunk_clone_symbol}(")),
        "the sequential world must call the chunk's own clone:\n{clone}"
    );
    for absent in [
        "@wf__par_acquire_lane",
        "@wf__par_publish",
        "@wf__par_join",
        "@wf__par_split_budget",
        splitter_symbol.as_str(),
    ] {
        assert!(
            !clone.contains(absent),
            "the sequential world must not reach {absent}:\n{clone}"
        );
    }

    // The clone renaming is the whole difference between the two copies. After
    // undoing it there must be nothing left, or the sequential world runs a
    // second lowering of the loop that nobody audited.
    let chunk = function_body(&module, &chunk_symbol);
    let chunk_clone = function_body(&module, &chunk_clone_symbol);
    assert_eq!(
        chunk_clone.replace("@wf__par_seq_", "@wf_"),
        chunk,
        "the two worlds' copies of the loop must be one lowering"
    );
}

/// The one synthesized symbol in this module beginning with `prefix`.
///
/// The synthesized halves are numbered after every source function, so their
/// ordinals move whenever a fixture gains one. Reading the symbol out of the
/// module keeps these cases about the shape rather than about the count. The
/// tail has to be a number, because the runtime's own `wf__par_split_budget`
/// shares a prefix with the splitter and is not one of these.
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

/// Observe the selected outer call without changing any inner query or worker
/// protocol. The same image exercises a zero answer and the real runtime's
/// answer. A zero allowance enters the splitter once and reaches one chunk;
/// that chunk retains its nested queries and publication opportunities.
fn observe_outer_loop_budget(module: &str, caller: &str, splitter: &str, chunk: &str) -> String {
    let body = function_body(module, caller);
    assert_eq!(body.matches("call i64 @wf__par_split_budget(").count(), 1);
    let changed = body.replace(
        "call i64 @wf__par_split_budget(",
        "call i64 @wf_test_outer_budget(",
    );
    let mut observed = module.replacen(body, &changed, 1);
    for (symbol, observer) in [
        (splitter, "wf_test_outer_splitter"),
        (chunk, "wf_test_outer_chunk"),
    ] {
        let body = function_body(&observed, symbol).to_owned();
        let entry = body.lines().find(|line| line.ends_with(':')).unwrap();
        let changed = body.replacen(entry, &format!("{entry}\n  call void @{observer}()"), 1);
        observed = observed.replacen(&body, &changed, 1);
    }
    observed.push_str(
        "\ndeclare i64 @wf_test_outer_budget(i64, i64)\ndeclare void @wf_test_outer_splitter()\ndeclare void @wf_test_outer_chunk()\n",
    );
    super::parallel::observe_worker_schedule(&observed)
}

// Appended to the existing worker observer, so both ordinary and zero-budget
// runs share one construction and retain the real publication/join protocol.
// WF_TEST_NESTED distinguishes a map with no inner offers from the nested
// reduction, whose zero-budget outer chunk must still publish inner work.
const OUTER_LOOP_BUDGET_OBSERVER: &str = r#"
extern uint64_t wf__par_split_budget(uint64_t, uint64_t);
extern unsigned long wf__par_grants(void);
static _Atomic unsigned outer_queries, outer_splitters, outer_chunks;
static _Atomic uint64_t outer_allowance;
uint64_t wf_test_outer_budget(uint64_t span, uint64_t weight) {
    atomic_fetch_add(&outer_queries, 1);
    uint64_t allowance = getenv("WF_TEST_ZERO_BUDGET") ? 0 : wf__par_split_budget(span, weight);
    atomic_store(&outer_allowance, allowance);
    return allowance;
}
void wf_test_outer_splitter(void) { atomic_fetch_add(&outer_splitters, 1); }
void wf_test_outer_chunk(void) { atomic_fetch_add(&outer_chunks, 1); }
static void report_outer_budget(void) {
    unsigned queries = atomic_load(&outer_queries);
    unsigned splitters = atomic_load(&outer_splitters);
    unsigned chunks = atomic_load(&outer_chunks);
    uint64_t allowance = atomic_load(&outer_allowance);
    unsigned long grants = wf__par_grants();
    int zero = getenv("WF_TEST_ZERO_BUDGET") != NULL;
    if (queries != 1 || !chunks ||
        (!allowance && (splitters != 1 || chunks != 1)) ||
        (allowance && !splitters) ||
        (!WF_TEST_NESTED && !zero && !allowance) ||
        ((WF_TEST_NESTED || !zero) &&
            (!grants || !atomic_load(&schedule_entered)))) {
        fprintf(stderr, "outer loop budget: zero=%d queries=%u splitters=%u chunks=%u grants=%lu\n",
                zero, queries, splitters, chunks, grants);
        _Exit(116);
    }
}
__attribute__((constructor)) static void register_outer_budget(void) {
    atexit(report_outer_budget);
}
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
            "seed, bounds, a padded 24-byte record, a 24-byte array, budget and result"
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
            &crate::driver::launcher::render(program, "main").expect("ordinary test launcher"),
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
        .output()
        .expect("run the module that splits nothing");
    assert_eq!(reference.status.code(), Some(0));
    assert_eq!(reference.stdout.len(), 8);

    let executable = build_executable(&split, &directory);
    let mut runs = vec![("no split lowering".to_owned(), reference.stdout)];
    for workers in ["1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
            .expect("run the split program");
        assert_eq!(output.status.code(), Some(0), "WF_WORKERS={workers}");
        runs.push((format!("WF_WORKERS={workers}"), output.stdout));
    }
    identical(&runs).expect("a captured, xor-folded split must not move one byte");

    // A selected worker schedule remains observable on a one-core host.
    {
        let (granted, output) = CountedProgram::link(&split, &directory).run(Some("4"));
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
    let denied = emit_with_overlap(ALIGNED_PAYLOAD_MAP);
    assert!(
        synthesized_symbols(&denied, "@wf__par_chunk_").is_empty(),
        "PAR-2 must still deny a whole-root measure read beside a mapped write"
    );
    // The specification forbids the whole-root read inside an element map;
    // the preheader already captures the same immutable length. Preserve the
    // entire write fixture, its exact output oracle, and the negative control.
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
    assert!(!chunk.contains("call void @free("), "{chunk}");

    let directory = test_directory();
    let reference = Command::new(build_executable(&unsplit, &directory))
        .output()
        .expect("run the unsplit nominal-element map");
    assert_eq!(reference.status.code(), Some(0), "{reference:?}");
    assert!(reference.stdout.is_empty());

    let executable = build_executable(&split, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
            .expect("run the split nominal-element map");
        assert_eq!(
            output.status.code(),
            Some(0),
            "WF_WORKERS={workers}: {output:?}"
        );
        assert_eq!(output.stdout, reference.stdout, "WF_WORKERS={workers}");
    }
    let (granted, output) = CountedProgram::link(&split, &directory).run(Some("4"));
    assert!(
        granted > 0,
        "the nominal-element map must execute a real worker callback"
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
        .output()
        .expect("run the unsplit nested reductions");
    assert_eq!(reference.status.code(), Some(0), "{reference:?}");
    assert!(reference.stdout.is_empty());

    let executable = build_executable(&split, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
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
            .env_remove("WF_SPLIT_WORK")
            .env_remove("WF_TEST_ZERO_BUDGET");
        if zero {
            command.env("WF_TEST_ZERO_BUDGET", "1");
        }
        let output = command.output().expect("run observed nested loop budget");
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
    // KEPT AS WRITTEN for the lowering port: the `@free` counts are the
    // emitted release shape of one `Box<Array<u8>>` per return path [STOR-3].
    // If the release lowering of a boxed run changes, re-derive the counts.
    assert!(!chunk.contains("call void @free("), "{chunk}");
    assert!(!splitter.contains("call void @free("), "{splitter}");
    for outer in ["@wf_main", "@wf__par_seq_main"] {
        let body = function_body(&split, outer);
        let mut releases_in_block = 0;
        let mut returning_blocks = 0;
        for line in body.lines() {
            if line.ends_with(':') {
                releases_in_block = 0;
            }
            releases_in_block += usize::from(line.contains("call void @free("));
            if line.trim_start().starts_with("ret ") {
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
        .output()
        .expect("run the map that splits nothing");
    assert_eq!(reference.status.code(), Some(0));
    assert_eq!(reference.stdout.len(), 400000);

    let executable = build_executable(&split, &directory);
    let mut runs = vec![("no split lowering".to_owned(), reference.stdout)];
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
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
            .env_remove("WF_SPLIT_WORK")
            .env_remove("WF_TEST_ZERO_BUDGET");
        if zero {
            command.env("WF_TEST_ZERO_BUDGET", "1");
        }
        let output = command.output().expect("run observed Unit-map loop budget");
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
        .output()
        .expect("run the sequential holder map");
    assert_eq!(reference.status.code(), Some(0));
    assert_eq!(reference.stdout.len(), 400000);

    let executable = build_executable(&split, &directory);
    let mut runs = vec![("no split lowering".to_owned(), reference.stdout)];
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
            .expect("run the split holder map");
        assert_eq!(output.status.code(), Some(0), "WF_WORKERS={workers}");
        assert_eq!(output.stdout.len(), 400000, "WF_WORKERS={workers}");
        runs.push((format!("WF_WORKERS={workers}"), output.stdout));
    }
    identical(&runs).expect("the borrowed read-modify map must preserve every output byte");

    let (granted, output) = CountedProgram::link(&split, &directory).run(Some("4"));
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
  let count = deref(output).len;
  for (i in 0_u64..count) {
    set deref(output)[i] = value;
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
        .output()
        .expect("run the row map that splits nothing");
    assert_eq!(reference.status.code(), Some(0), "{reference:?}");
    let executable = build_executable(&split, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
            .expect("run the split row map");
        assert_eq!(
            output.status.code(),
            Some(0),
            "WF_WORKERS={workers}: every row must hold its own index"
        );
    }
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
        .output()
        .expect("run the combined loop that splits nothing");
    assert_eq!(reference.status.code(), Some(0));
    assert_eq!(reference.stdout.len(), 400008);

    let executable = build_executable(&split, &directory);
    let mut runs = vec![("no split lowering".to_owned(), reference.stdout)];
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
            .expect("run the combined split loop");
        assert_eq!(output.status.code(), Some(0), "WF_WORKERS={workers}");
        runs.push((format!("WF_WORKERS={workers}"), output.stdout));
    }
    identical(&runs).expect("the combined map and reduction must preserve every byte");

    let (granted, output) = CountedProgram::link(&split, &directory).run(Some("4"));
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

/// Odd, non-power-of-two range for controlled per-row worker executions.
/// WF_SPLIT_WORK=1 permits small chunks; the ordinary program family separately
/// covers the shipped work threshold with its substantial representative fold.
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
    let spare = deref(destination).len;
    let writable = cursor < spare;
    if writable {
      let byte = low_byte(v: rest);
      set deref(destination)[cursor] = byte;
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
        "\nfn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {{\n  \
         let std::process::Inputs(args: unused_args, cwd: cwd, stdout: out, stderr: unused_stderr, handles: factory, stdin: unused_stdin) = move inputs;\n  \
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
        "  match std::io::write_once(factory: &factory, output: &out, source: window, start: 0_u64, \
         end: {width}_u64) {{\n    Ok(value: accepted) => {{\n      \
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
        .output()
        .expect("run the module that splits nothing");
    assert_eq!(reference.status.code(), Some(0));
    assert_eq!(
        reference.stdout.len(),
        8 * ADMITTED_COMBINES.len(),
        "every row publishes eight bytes"
    );

    // Each row's caller brackets a fully joined fold. Interposition delays
    // the owner's join until one actual nonowner callback has entered.
    let mut observed = super::parallel::observe_worker_schedule(&split);
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
                &format!("{entry}\n  call void @wf_test_worker_schedule_begin()"),
                1,
            )
            .replace(
                "  ret i64 ",
                &format!("  call void @wf_test_row_end(i32 {index})\n  ret i64 "),
            );
        assert!(replacement.contains(&format!("@wf_test_row_end(i32 {index})")));
        observed = observed.replacen(&body, &replacement, 1);
    }
    observed.push_str(
        "\ndeclare void @wf_test_worker_schedule_begin()\ndeclare void @wf_test_row_end(i32)\n",
    );
    let observer = format!(
        "#define WF_TEST_SCHEDULE_MANUAL\n{}\n{}",
        super::parallel::WORKER_SCHEDULE,
        r#"
static unsigned completed_rows;
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
        .env("WF_SPLIT_WORK", "1")
        .output()
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

/// Two split loops surround an ordinary call join and precede a loop-header
/// phi. One image checks both allowance answers and the sequential world;
/// native construction also verifies every emitted phi predecessor.
#[test]
fn multiple_split_loops_and_an_ordinary_join_keep_phi_predecessors() {
    let source = br#"fn choose(value: u64) -> result: u64 pure {
  return imax(value, value);
}

fn composed(limit: u64) -> result: u64 pure {
  let total = 5_u64;
  for (i in 0_u64..limit) {
    set total = total +wrap i;
  }
  let a = choose(value: total);
  let b = choose(value: 17_u64);
  let c = choose(value: 19_u64);
  let ab = a +wrap b;
  let combined = ab +wrap c;
  let marks = array_filled::<u64, 2>(value: 0_u64);
  for (j in 0_u64..2_u64) {
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
    assert_eq!(body.matches("call i64 @wf__par_split_budget(").count(), 2);
    assert!(body.contains("call void @wf__par_publish("));
    assert!(body.contains(" = phi i64 "));
    let sequential = function_body(&module, "@wf__par_seq_composed");
    assert!(!sequential.contains("@wf__par_split_budget("));
    assert!(!sequential.contains("@wf__par_publish("));

    let splitters = synthesized_symbols(&module, "@wf__par_split_");
    assert_eq!(splitters.len(), 2);
    let mut observed = module.replace(
        "call i64 @wf__par_split_budget(",
        "call i64 @wf_test_cfg_budget(",
    );
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
    observed.push_str(
        "\ndeclare i64 @wf_test_cfg_budget(i64, i64)\ndeclare void @wf_test_cfg_splitter()\n",
    );
    let observer = r#"#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static _Atomic unsigned queries, splitters;
uint64_t wf_test_cfg_budget(uint64_t span, uint64_t weight) {
    (void)weight;
    if (span != 4 && span != 2) { fputs("wrong composed span\n", stderr); exit(117); }
    atomic_fetch_add(&queries, 1);
    return getenv("WF_TEST_POSITIVE_BUDGET") ? 4 : 0;
}
void wf_test_cfg_splitter(void) { atomic_fetch_add(&splitters, 1); }
static void report(void) {
    unsigned queried = atomic_load(&queries), entered = atomic_load(&splitters);
    int sequential = strcmp(getenv("WF_WORKERS"), "1") == 0;
    int positive = getenv("WF_TEST_POSITIVE_BUDGET") != NULL;
    if (queried != (sequential ? 0 : 2) ||
        (sequential ? entered != 0 : (positive ? entered <= 2 : entered != 2))) {
        fprintf(stderr, "composed loops: queries=%u splitters=%u\n", queried, entered);
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
        let output = command.output().expect("run composed split loops");
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
    let observed = module.replace(
        "call i64 @wf__par_split_budget(",
        "call i64 @wf_test_edge_budget(",
    ) + "\ndeclare i64 @wf_test_edge_budget(i64, i64)\n";
    let observer = r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static unsigned queries;
uint64_t wf_test_edge_budget(uint64_t span, uint64_t weight) {
    (void)weight;
    /* Empty and inverted ranges must not become wrapped positive spans. */
    if (span > 1) { fputs("wrapped degenerate span\n", stderr); exit(114); }
    ++queries;
    return getenv("WF_TEST_POSITIVE_BUDGET") ? 4 : 0;
}
static void report(void) { if (!queries) { fputs("no edge budget query\n", stderr); _Exit(115); } }
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
        let output = command.output().expect("run controlled degenerate ranges");
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
            "    let partial = 0_u64;\n    for @inner (j in 0_u64..2_u64) {\n      set partial = partial +wrap j;\n    }\n    let initial = mixed +wrap a0;\n    let bias0 = initial +wrap partial;",
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
                parent
                    .source_calls()
                    .iter()
                    .any(|call| call.allocation().is_some())
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
                .output()
                .expect("run reused ordinary outer loop");
            assert_eq!(
                output.status.code(),
                Some(0),
                "WF_WORKERS={workers}: {output:?}"
            );
        }
        let (granted, output) = CountedProgram::link(&module, &directory).run(Some("4"));
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(
            granted > 0,
            "the retained sibling calls must execute a worker callback"
        );
        std::fs::remove_dir_all(&directory).expect("remove the test directory");
    }
}
