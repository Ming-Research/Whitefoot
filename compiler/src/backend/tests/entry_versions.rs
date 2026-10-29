//! One all-cheap choice per function entry between the parallel body and the
//! canonical ordinary closure (compiler/parallel-lowering/two-worlds): which
//! functions choose, what the choice reads, and what each choice computes.

use super::parallel::function_body;
use super::{BoundedOutput, build_executable, emit, emit_with_overlap, test_directory};
use std::process::Command;

/// A folded range whose span is a caller's entry value only through a
/// wrapping endpoint, called repeatedly from a loop with a loop-carried
/// offset and seed.
const WALK: &str = r#"fn fold(seed: u64, lo: u64, hi: u64) -> result: u64 pure {
  let total = seed;
  for (i in lo..hi) {
    let scaled = i *wrap 7_u64;
    set total = total +wrap scaled;
  }
  return total;
}

fn walk(repetitions: u64, extent: u64, seed: u64) -> result: u64 pure {
  let total = seed;
  let n = 0_u64;
  loop @walk {
    if n == repetitions {
      break @walk;
    }
    let lo = n *wrap 7_u64;
    let hi = lo +wrap extent;
    if lo <= hi {
      let part = fold(seed: total, lo: lo, hi: hi);
      set total = part;
    }
    set n = n +wrap 1_u64;
  }
  return total;
}
"#;

/// The text between a function's entry choice and the block it enters for
/// the ordinary closure.
fn entry_choice(body: &str) -> &str {
    let start = body
        .find("par.region.entry:")
        .unwrap_or_else(|| panic!("no entry choice:\n{body}"));
    let end = body[start..]
        .find("par.region.sequential:")
        .map(|offset| start + offset)
        .unwrap_or_else(|| panic!("no ordinary entrance:\n{body}"));
    &body[start..end]
}

/// `walk` chooses once at entry from `extent`, an upper bound of every span
/// its calls reach, and its ordinary choice is the complete non-parallel
/// lowering. The parallel body still calls the parallel `fold`, which keeps
/// its own choice for callers that enter it directly.
#[test]
fn an_entry_invariant_extent_selects_the_ordinary_closure_once() {
    let module = emit_with_overlap(WALK.as_bytes());
    let ordinary = emit(WALK.as_bytes());
    let walk = function_body(&module, "@wf_walk");
    assert_eq!(walk.matches("par.region.entry:").count(), 1, "{walk}");
    assert_eq!(walk.matches("par.region.sequential:").count(), 1, "{walk}");
    assert_eq!(
        walk.matches("call i64 @wf__par_seq_walk(").count(),
        1,
        "{walk}"
    );
    let choice = entry_choice(walk);
    assert_eq!(choice.matches("udiv i64 149999, ").count(), 1, "{choice}");
    assert!(choice.contains("br i1 %"), "{choice}");
    assert!(walk.contains("call i64 @wf_fold("), "{walk}");
    assert!(
        entry_choice(function_body(&module, "@wf_fold")).contains("icmp ult i64"),
        "fold chooses for its own direct callers"
    );
    for name in ["walk", "fold"] {
        let clone = function_body(&module, &format!("@wf__par_seq_{name}"));
        let renamed = clone.replace("@wf__par_seq_", "@wf_");
        assert_eq!(
            renamed,
            function_body(&ordinary, &format!("@wf_{name}")),
            "{name}'s ordinary choice must be the complete non-parallel lowering"
        );
        assert!(!renamed.contains("@wf__par_"), "{clone}");
        assert!(!renamed.contains("par.region"), "{clone}");
    }
}

/// A span that varies after entry keeps the local decision: `walk` makes no
/// choice, while `fold` still chooses for each call from its own formals.
#[test]
fn a_varying_extent_keeps_the_local_decision() {
    let source = WALK.replace(
        "let hi = lo +wrap extent;",
        "let width = extent +wrap n;\n    let hi = lo +wrap width;",
    );
    let module = emit_with_overlap(source.as_bytes());
    let walk = function_body(&module, "@wf_walk");
    assert!(!walk.contains("par.region"), "{walk}");
    assert!(walk.contains("call i64 @wf_fold("), "{walk}");
    assert!(function_body(&module, "@wf_fold").contains("par.region.entry:"));
}

/// The choice does not require the reached split to repeat: one call after
/// a loop that schedules nothing is still decided once at entry.
#[test]
fn a_single_reached_split_is_chosen_at_entry() {
    let source = format!(
        "{WALK}\nfn once(repetitions: u64, extent: u64) -> result: u64 pure {{\n  let n = 0_u64;\n  loop @count {{\n    if n == repetitions {{\n      break @count;\n    }}\n    set n = n +wrap 1_u64;\n  }}\n  let folded = fold(seed: n, lo: 0_u64, hi: extent);\n  return folded;\n}}\n"
    );
    let module = emit_with_overlap(source.as_bytes());
    let once = function_body(&module, "@wf_once");
    assert_eq!(once.matches("par.region.entry:").count(), 1, "{once}");
    assert!(once.contains("call i64 @wf__par_seq_once("), "{once}");
}

/// A range reference's length is an entry value: the choice reads it from
/// the incoming descriptor before any source operation.
#[test]
fn a_formal_range_length_is_an_entry_value() {
    let source = br#"fn sum(values: &[u64]) -> result: u64 reads(values) {
  let n = values^.len;
  let total = 0_u64;
  for (i in 0_u64..n) {
    set total = total +wrap values^[i];
  }
  return total;
}
"#;
    let module = emit_with_overlap(source);
    let sum = function_body(&module, "@wf_sum");
    let choice = entry_choice(sum);
    assert!(choice.contains("extractvalue"), "{choice}");
    assert!(sum.contains("call i64 @wf__par_seq_sum("), "{sum}");
}

/// Literal endpoints with a calling body are not compile-time small sites,
/// because the lowering's local proof does not price helpers, but their
/// priced split decision is a constant. A cheap constant enters the ordinary
/// closure unconditionally; an expensive one makes no choice at all.
#[test]
fn constant_split_decisions_are_folded() {
    let source = br#"fn step(v: u64) -> result: u64 pure {
  return v *wrap 3_u64;
}

fn few(seed: u64) -> result: u64 pure {
  let total = seed;
  for (i in 0_u64..3_u64) {
    let stepped = step(v: i);
    set total = total +wrap stepped;
  }
  return total;
}

fn many(seed: u64) -> result: u64 pure {
  let total = seed;
  for (i in 0_u64..4000000_u64) {
    let stepped = step(v: i);
    set total = total +wrap stepped;
  }
  return total;
}
"#;
    let module = emit_with_overlap(source);
    let few = function_body(&module, "@wf_few");
    assert!(few.contains("@wf__par_split_few."), "{few}");
    assert!(
        few.contains("br i1 true, label %par.region.sequential, label %"),
        "{few}"
    );
    assert!(!entry_choice(few).contains("udiv"), "{few}");
    let many = function_body(&module, "@wf_many");
    assert!(many.contains("@wf__par_split_many."), "{many}");
    assert!(!many.contains("par.region"), "{many}");
}

/// An indexed reduction no longer leaves its function to local decisions:
/// its span, the formal's length, joins the entry choice. The family's cell
/// count is the length of a Box the body allocates, not an entry value, so
/// the cells refinement is dropped and only the span test decides.
#[test]
fn an_indexed_site_takes_part_in_the_entry_choice() {
    let source: &[u8] = include_bytes!("../../../../tests/programs/parallel/indexed_reductions.wf");
    super::system::with_parallel_ir(source, |program| {
        let histogram = program
            .functions()
            .iter()
            .find(|function| function.name() == "histogram")
            .expect("histogram");
        assert!(
            histogram
                .blocks()
                .iter()
                .flat_map(|block| block.instructions())
                .any(|instruction| {
                    matches!(
                        instruction,
                        crate::IrInstruction::Define {
                            operation: crate::IrOperation::LoopSplit { indexed, .. },
                            ..
                        } if !indexed.is_empty()
                    )
                }),
            "the fixture must keep an indexed split"
        );
        let predicates = program
            .entry_versions
            .get("histogram")
            .expect("an indexed site's span is an entry value");
        assert_eq!(predicates.len(), 1, "{predicates:?}");
        assert!(
            matches!(predicates[0].span, crate::IrWorkEstimate::Length(_)),
            "{predicates:?}"
        );
        assert!(predicates[0].cells.is_none(), "{predicates:?}");
    });
    let module = emit_with_overlap(source);
    assert!(super::emitted_body(&module, "histogram").contains("par.region.entry:"));
}

/// A remaining call group hands out calls whose static work reached the
/// grain, so a function that has one makes no entry choice.
#[test]
fn a_call_group_keeps_its_function_parallel() {
    let source = format!(
        "{WALK}\nfn pair(extent: u64) -> result: u64 pure {{\n  let first = fold(seed: 1_u64, lo: 0_u64, hi: extent);\n  let second = fold(seed: 2_u64, lo: 0_u64, hi: extent);\n  return first +wrap second;\n}}\n"
    );
    let module = emit_with_overlap(source.as_bytes());
    let pair = function_body(&module, "@wf_pair");
    assert!(pair.contains("call void @wf__par_publish("), "{pair}");
    assert!(!pair.contains("par.region"), "{pair}");
}

/// Both choices compute the source result, for scalar and register-returned
/// aggregate results, empty, cheap and profitable extents and a zero-trip
/// entry with a huge extent, at one and four workers. The oracle is computed
/// here, independently of either lowering.
#[test]
fn both_entry_choices_preserve_results() {
    let mut source = format!(
        "{WALK}\nstruct Totals {{\n  low: u64;\n  high: u64;\n}}\n\nfn totals(n: u64) -> result: Totals pure {{\n  let low = 0_u64;\n  for (i in 0_u64..n) {{\n    set low = low +wrap i;\n  }}\n  let high = 0_u64;\n  for (j in 0_u64..n) {{\n    let tripled = j *wrap 3_u64;\n    set high = high +wrap tripled;\n  }}\n  return Totals(low: low, high: high);\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n"
    );
    let fold = |seed: u64, lo: u64, hi: u64| {
        (lo..hi).fold(seed, |total, i| total.wrapping_add(i.wrapping_mul(7)))
    };
    let mut code = 1_u8;
    for (repetitions, extent, seed) in [
        (0_u64, u64::MAX, 5_u64),
        (2, 0, 10),
        (3, 3, 10),
        (2, 300_000, 10),
    ] {
        let mut total = seed;
        for n in 0..repetitions {
            let lo = n.wrapping_mul(7);
            let hi = lo.wrapping_add(extent);
            if lo <= hi {
                total = fold(total, lo, hi);
            }
        }
        source.push_str(&format!(
            "  let walked{code} = walk(repetitions: {repetitions}_u64, extent: {extent}_u64, seed: {seed}_u64);\n  if walked{code} != {total}_u64 {{\n    return std::process::exit_status(code: {code}_u8);\n  }}\n"
        ));
        code += 1;
    }
    for n in [0_u64, 1, 5, 300_000] {
        let low = (0..n).fold(0_u64, u64::wrapping_add);
        let high = (0..n).fold(0_u64, |total, j| total.wrapping_add(j.wrapping_mul(3)));
        source.push_str(&format!(
            "  let counted{code} = totals(n: {n}_u64);\n  if counted{code}.low != {low}_u64 {{\n    return std::process::exit_status(code: {code}_u8);\n  }}\n  if counted{code}.high != {high}_u64 {{\n    return std::process::exit_status(code: {code}_u8);\n  }}\n"
        ));
        code += 1;
    }
    source.push_str("  return std::process::exit_status(code: 0_u8);\n}\n");
    let parallel = emit_with_overlap(source.as_bytes());
    assert!(
        super::emitted_body(&parallel, "totals").contains("par.region.entry:"),
        "both totals loops are chosen at one entry"
    );
    let directory = test_directory();
    for module in [emit(source.as_bytes()), parallel] {
        let image = build_executable(&module, &directory);
        for workers in ["1", "4"] {
            let output = Command::new(&image)
                .env("WF_WORKERS", workers)
                .bounded_output()
                .expect("run entry choice witness");
            assert_eq!(
                output.status.code(),
                Some(0),
                "workers={workers}: {output:?}"
            );
        }
    }
    std::fs::remove_dir_all(directory).expect("remove entry choice artifacts");
}
