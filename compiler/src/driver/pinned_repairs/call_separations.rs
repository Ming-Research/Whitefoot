//! [EFF-5] the repairs of a call whose substituted entries no admitted
//! family separates, pinned with the programs they produce. The pair form
//! and the harness that compiles every pair are `driver::pinned_repairs`'s;
//! the family lives apart only to keep each file readable.

use super::RepairPair;

pub(super) const CALL_SEPARATIONS: &[RepairPair] = &[
    RepairPair {
        name: "call-passes-one-place-to-two-arguments.wf",
        rejected: br#"struct Cell {
  value: u64;
}

fn copy_across(source: &Cell, destination: &Cell) -> result: unit reads(source.value), writes(destination.value) {
  let v = deref(source).value;
  set deref(destination).value = v;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = Cell(value: 4_u64);
  copy_across(source: &a, destination: &a);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: pass places that do not overlap, or pass the shared place through one argument only\n",
        ],
        repaired: &[br#"struct Cell {
  value: u64;
}

fn copy_across(source: &Cell, destination: &Cell) -> result: unit reads(source.value), writes(destination.value) {
  let v = deref(source).value;
  set deref(destination).value = v;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = Cell(value: 4_u64);
  let b = Cell(value: 0_u64);
  copy_across(source: &a, destination: &b);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // [EFF-5] compares two entries one argument supplies when their
        // declared positions can tell them apart, and this call gives those
        // positions one value, so the repair is at the call's positions or at
        // the callee's row.
        name: "row-positions-one-call-makes-equal.wf",
        rejected: br#"fn copy_within(values: &Array<u8, 4>, i: u64, j: u64) -> result: unit reads(values[i]), writes(values[j]) contract {
  requires i < 4_u64;
  requires j < 4_u64;
} {
  let observed = deref(values)[i];
  set deref(values)[j] = observed;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  copy_within(values: &values, i: 1_u64, j: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: this call gives these two entries of the callee's row the same positions: pass positions this call proves do not overlap, or replace the callee's row entries at or below their common path with one `writes` entry of that path\n",
        ],
        repaired: &[
            br#"fn copy_within(values: &Array<u8, 4>, i: u64, j: u64) -> result: unit reads(values[i]), writes(values[j]) contract {
  requires i < 4_u64;
  requires j < 4_u64;
} {
  let observed = deref(values)[i];
  set deref(values)[j] = observed;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  copy_within(values: &values, i: 1_u64, j: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn copy_within(values: &Array<u8, 4>, i: u64, j: u64) -> result: unit writes(values) contract {
  requires i < 4_u64;
  requires j < 4_u64;
} {
  let observed = deref(values)[i];
  set deref(values)[j] = observed;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  copy_within(values: &values, i: 1_u64, j: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [EFF-5] one argument supplies both entries and the two positions
        // are left to the entailment fragment, which cannot prove `a` and `b`
        // distinct here; passing one of the places removes neither entry.
        name: "call-separation-one-argument.wf",
        rejected: br#"fn copy_within(values: &Array<u8, 4>, i: u64, j: u64) -> result: unit reads(values[i]), writes(values[j]) contract {
  requires i < 4_u64;
  requires j < 4_u64;
} {
  let observed = deref(values)[i];
  set deref(values)[j] = observed;
  return unit;
}

fn shift(values: &Array<u8, 4>, a: u64, b: u64) -> result: unit writes(values) contract {
  requires a < 4_u64;
  requires b < 4_u64;
} {
  copy_within(values: values, i: a, j: b);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  shift(values: &values, a: 1_u64, b: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: when the two positions can differ here, prove them distinct before this call; otherwise pass positions this call proves distinct, or replace the callee's row entries at or below their common path with one `writes` entry of that path\n",
        ],
        repaired: &[
            br#"fn copy_within(values: &Array<u8, 4>, i: u64, j: u64) -> result: unit reads(values[i]), writes(values[j]) contract {
  requires i < 4_u64;
  requires j < 4_u64;
} {
  let observed = deref(values)[i];
  set deref(values)[j] = observed;
  return unit;
}

fn shift(values: &Array<u8, 4>, a: u64, b: u64) -> result: unit writes(values) contract {
  requires a < 4_u64;
  requires b < 4_u64;
  requires a < b;
} {
  copy_within(values: values, i: a, j: b);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  shift(values: &values, a: 1_u64, b: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn copy_within(values: &Array<u8, 4>, i: u64, j: u64) -> result: unit reads(values[i]), writes(values[j]) contract {
  requires i < 4_u64;
  requires j < 4_u64;
} {
  let observed = deref(values)[i];
  set deref(values)[j] = observed;
  return unit;
}

fn shift(values: &Array<u8, 4>, a: u64, b: u64) -> result: unit writes(values) contract {
  requires a < 4_u64;
  requires b < 4_u64;
} {
  copy_within(values: values, i: 0_u64, j: 1_u64);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  shift(values: &values, a: 1_u64, b: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn copy_within(values: &Array<u8, 4>, i: u64, j: u64) -> result: unit writes(values) contract {
  requires i < 4_u64;
  requires j < 4_u64;
} {
  let observed = deref(values)[i];
  set deref(values)[j] = observed;
  return unit;
}

fn shift(values: &Array<u8, 4>, a: u64, b: u64) -> result: unit writes(values) contract {
  requires a < 4_u64;
  requires b < 4_u64;
} {
  copy_within(values: values, i: a, j: b);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  shift(values: &values, a: 1_u64, b: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [OWN-7] two ranges one argument supplies are separated when one is
        // proved to end at or before the other starts, or one empty, which the
        // call does not prove of `0..x` and `2..4` [EFF-5].
        name: "call-separation-two-ranges.wf",
        rejected: br#"fn move_run(values: &Array<u64, 4>, lo: u64, hi: u64, a: u64, b: u64) -> result: unit reads(values[lo..hi]), writes(values[a..b]) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires a <= b;
  requires b <= 4_u64;
} {
  let run = &deref(values)[lo..hi];
  let count = deref(run).len;
  let out = &deref(values)[a..b];
  if 0_u64 < deref(out).len {
    set deref(out)[0_u64] = count;
  }
  return unit;
}

fn shift(values: &Array<u64, 4>, x: u64) -> result: unit writes(values) contract {
  requires x <= 4_u64;
} {
  move_run(values: values, lo: 0_u64, hi: x, a: 2_u64, b: 4_u64);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 1_u64);
  shift(values: &values, x: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: when the two ranges can lie apart here, prove before this call that one ends at or before the other starts, or that one is empty; otherwise pass ranges this call proves apart, or replace the callee's row entries at or below their common path with one `writes` entry of that path\n",
        ],
        repaired: &[
            br#"fn move_run(values: &Array<u64, 4>, lo: u64, hi: u64, a: u64, b: u64) -> result: unit reads(values[lo..hi]), writes(values[a..b]) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires a <= b;
  requires b <= 4_u64;
} {
  let run = &deref(values)[lo..hi];
  let count = deref(run).len;
  let out = &deref(values)[a..b];
  if 0_u64 < deref(out).len {
    set deref(out)[0_u64] = count;
  }
  return unit;
}

fn shift(values: &Array<u64, 4>, x: u64) -> result: unit writes(values) contract {
  requires x <= 2_u64;
} {
  move_run(values: values, lo: 0_u64, hi: x, a: 2_u64, b: 4_u64);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 1_u64);
  shift(values: &values, x: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn move_run(values: &Array<u64, 4>, lo: u64, hi: u64, a: u64, b: u64) -> result: unit reads(values[lo..hi]), writes(values[a..b]) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires a <= b;
  requires b <= 4_u64;
} {
  let run = &deref(values)[lo..hi];
  let count = deref(run).len;
  let out = &deref(values)[a..b];
  if 0_u64 < deref(out).len {
    set deref(out)[0_u64] = count;
  }
  return unit;
}

fn shift(values: &Array<u64, 4>, x: u64) -> result: unit writes(values) contract {
  requires x <= 4_u64;
} {
  move_run(values: values, lo: 0_u64, hi: 2_u64, a: 2_u64, b: 4_u64);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 1_u64);
  shift(values: &values, x: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn move_run(values: &Array<u64, 4>, lo: u64, hi: u64, a: u64, b: u64) -> result: unit writes(values) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires a <= b;
  requires b <= 4_u64;
} {
  let run = &deref(values)[lo..hi];
  let count = deref(run).len;
  let out = &deref(values)[a..b];
  if 0_u64 < deref(out).len {
    set deref(out)[0_u64] = count;
  }
  return unit;
}

fn shift(values: &Array<u64, 4>, x: u64) -> result: unit writes(values) contract {
  requires x <= 4_u64;
} {
  move_run(values: values, lo: 0_u64, hi: x, a: 2_u64, b: 4_u64);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 1_u64);
  shift(values: &values, x: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [OWN-7] the same two ranges, supplied by two arguments over one
        // place, so merging the callee's row is no alternative [EFF-5].
        name: "call-separation-two-ranges-two-arguments.wf",
        rejected: br#"fn move_run(source: &Array<u64, 4>, target: &Array<u64, 4>, lo: u64, hi: u64, a: u64, b: u64) -> result: unit reads(source[lo..hi]), writes(target[a..b]) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires a <= b;
  requires b <= 4_u64;
} {
  let run = &deref(source)[lo..hi];
  let count = deref(run).len;
  let out = &deref(target)[a..b];
  if 0_u64 < deref(out).len {
    set deref(out)[0_u64] = count;
  }
  return unit;
}

fn shift(values: &Array<u64, 4>, x: u64) -> result: unit writes(values) contract {
  requires x <= 4_u64;
} {
  move_run(source: values, target: values, lo: 0_u64, hi: x, a: 2_u64, b: 4_u64);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 1_u64);
  shift(values: &values, x: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: when the two ranges can lie apart here, prove before this call that one ends at or before the other starts, or that one is empty; otherwise pass ranges this call proves apart\n",
        ],
        repaired: &[
            br#"fn move_run(source: &Array<u64, 4>, target: &Array<u64, 4>, lo: u64, hi: u64, a: u64, b: u64) -> result: unit reads(source[lo..hi]), writes(target[a..b]) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires a <= b;
  requires b <= 4_u64;
} {
  let run = &deref(source)[lo..hi];
  let count = deref(run).len;
  let out = &deref(target)[a..b];
  if 0_u64 < deref(out).len {
    set deref(out)[0_u64] = count;
  }
  return unit;
}

fn shift(values: &Array<u64, 4>, x: u64) -> result: unit writes(values) contract {
  requires x <= 2_u64;
} {
  move_run(source: values, target: values, lo: 0_u64, hi: x, a: 2_u64, b: 4_u64);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 1_u64);
  shift(values: &values, x: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn move_run(source: &Array<u64, 4>, target: &Array<u64, 4>, lo: u64, hi: u64, a: u64, b: u64) -> result: unit reads(source[lo..hi]), writes(target[a..b]) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires a <= b;
  requires b <= 4_u64;
} {
  let run = &deref(source)[lo..hi];
  let count = deref(run).len;
  let out = &deref(target)[a..b];
  if 0_u64 < deref(out).len {
    set deref(out)[0_u64] = count;
  }
  return unit;
}

fn shift(values: &Array<u64, 4>, x: u64) -> result: unit writes(values) contract {
  requires x <= 4_u64;
} {
  move_run(source: values, target: values, lo: 0_u64, hi: 2_u64, a: 2_u64, b: 4_u64);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 1_u64);
  shift(values: &values, x: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [WIN-2] an index beside a window's `next` is separated from it by
        // being below the window's length, which the call does not prove;
        // one argument supplies both entries [EFF-5].
        name: "call-separation-index-beside-next.wf",
        rejected: br#"fn read_then_append(r: &Slots<u64, 4>, i: u64) -> result: u64 reads(r[i]), writes(r.next), writes(r.len) contract {
  requires deref(r).len < 4_u64;
} {
  let seen = if i < deref(r).len {
    give deref(r)[i];
  } else {
    give 0_u64;
  }
  place_back(window: r, value: seen);
  return seen;
}

fn caller(r: &Slots<u64, 4>, k: u64) -> result: u64 writes(r) contract {
  requires deref(r).len < 4_u64;
} {
  let x = read_then_append(r: r, i: k);
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u64, 4>();
  place_back(window: &window, value: 5_u64);
  let x = caller(r: &window, k: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: when the index can be below the window's length here, prove that before this call; otherwise pass an index this call proves below it, or replace the callee's row entries at or below their common path with one `writes` entry of that path\n",
        ],
        repaired: &[
            br#"fn read_then_append(r: &Slots<u64, 4>, i: u64) -> result: u64 reads(r[i]), writes(r.next), writes(r.len) contract {
  requires deref(r).len < 4_u64;
} {
  let seen = if i < deref(r).len {
    give deref(r)[i];
  } else {
    give 0_u64;
  }
  place_back(window: r, value: seen);
  return seen;
}

fn caller(r: &Slots<u64, 4>, k: u64) -> result: u64 writes(r) contract {
  requires deref(r).len < 4_u64;
} {
  if k < deref(r).len {
    let x = read_then_append(r: r, i: k);
    return x;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u64, 4>();
  place_back(window: &window, value: 5_u64);
  let x = caller(r: &window, k: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn read_then_append(r: &Slots<u64, 4>, i: u64) -> result: u64 reads(r[i]), writes(r.next), writes(r.len) contract {
  requires deref(r).len < 4_u64;
} {
  let seen = if i < deref(r).len {
    give deref(r)[i];
  } else {
    give 0_u64;
  }
  place_back(window: r, value: seen);
  return seen;
}

fn caller(r: &Slots<u64, 4>, k: u64) -> result: u64 writes(r) contract {
  requires deref(r).len < 4_u64;
  requires 0_u64 < deref(r).len;
} {
  let x = read_then_append(r: r, i: 0_u64);
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u64, 4>();
  place_back(window: &window, value: 5_u64);
  let x = caller(r: &window, k: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn read_then_append(r: &Slots<u64, 4>, i: u64) -> result: u64 writes(r) contract {
  requires deref(r).len < 4_u64;
} {
  let seen = if i < deref(r).len {
    give deref(r)[i];
  } else {
    give 0_u64;
  }
  place_back(window: r, value: seen);
  return seen;
}

fn caller(r: &Slots<u64, 4>, k: u64) -> result: u64 writes(r) contract {
  requires deref(r).len < 4_u64;
} {
  let x = read_then_append(r: r, i: k);
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u64, 4>();
  place_back(window: &window, value: 5_u64);
  let x = caller(r: &window, k: 0_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // The same [WIN-2] question from two arguments: an interface member's
        // row is declared without a body, so nothing before the call bounds
        // `i` and neither argument writes the window's length.
        name: "call-separation-index-beside-next-two-arguments.wf",
        rejected: br#"interface Stage {
  fn step(x: &Slots<u64, 4>, w: &Slots<u64, 4>, i: u64) -> result: u64 reads(x[i]), writes(w.next);
}

fn outer<interface Stage>(r: &Slots<u64, 4>, k: u64) -> result: u64 writes(r) {
  let y = Stage::step(x: r, w: r, i: k);
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: when the index can be below the window's length here, prove that before this call; otherwise pass an index this call proves below it\n",
        ],
        repaired: &[
            br#"interface Stage {
  fn step(x: &Slots<u64, 4>, w: &Slots<u64, 4>, i: u64) -> result: u64 reads(x[i]), writes(w.next);
}

fn outer<interface Stage>(r: &Slots<u64, 4>, k: u64) -> result: u64 writes(r) contract {
  requires k < deref(r).len;
} {
  let y = Stage::step(x: r, w: r, i: k);
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"interface Stage {
  fn step(x: &Slots<u64, 4>, w: &Slots<u64, 4>, i: u64) -> result: u64 reads(x[i]), writes(w.next);
}

fn outer<interface Stage>(r: &Slots<u64, 4>, k: u64) -> result: u64 writes(r) contract {
  requires 0_u64 < deref(r).len;
} {
  let y = Stage::step(x: r, w: r, i: 0_u64);
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [OWN-7] an index beside a range one argument supplies is separated
        // from it once proved outside it, which this call does not prove
        // [EFF-5].
        name: "call-separation-index-beside-range.wf",
        rejected: br#"fn record_run(values: &Array<u64, 4>, start: u64, end: u64, slot: u64) -> result: unit reads(values[start..end]), writes(values[slot]) contract {
  requires start <= end;
  requires end <= 4_u64;
  requires slot < 4_u64;
} {
  let run = &deref(values)[start..end];
  let length = deref(run).len;
  set deref(values)[slot] = length;
  return unit;
}

fn outer(values: &Array<u64, 4>, lo: u64, hi: u64, at: u64) -> result: unit writes(values) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires at < 4_u64;
} {
  record_run(values: values, start: lo, end: hi, slot: at);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  outer(values: &values, lo: 0_u64, hi: 2_u64, at: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: when the index can lie outside the range here, prove before this call that it is below the range's start or at or after its end; otherwise pass positions this call proves apart, or replace the callee's row entries at or below their common path with one `writes` entry of that path\n",
        ],
        repaired: &[
            br#"fn record_run(values: &Array<u64, 4>, start: u64, end: u64, slot: u64) -> result: unit reads(values[start..end]), writes(values[slot]) contract {
  requires start <= end;
  requires end <= 4_u64;
  requires slot < 4_u64;
} {
  let run = &deref(values)[start..end];
  let length = deref(run).len;
  set deref(values)[slot] = length;
  return unit;
}

fn outer(values: &Array<u64, 4>, lo: u64, hi: u64, at: u64) -> result: unit writes(values) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires at < 4_u64;
} {
  if hi <= at {
    record_run(values: values, start: lo, end: hi, slot: at);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  outer(values: &values, lo: 0_u64, hi: 2_u64, at: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn record_run(values: &Array<u64, 4>, start: u64, end: u64, slot: u64) -> result: unit reads(values[start..end]), writes(values[slot]) contract {
  requires start <= end;
  requires end <= 4_u64;
  requires slot < 4_u64;
} {
  let run = &deref(values)[start..end];
  let length = deref(run).len;
  set deref(values)[slot] = length;
  return unit;
}

fn outer(values: &Array<u64, 4>, lo: u64, hi: u64, at: u64) -> result: unit writes(values) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires at < 4_u64;
} {
  record_run(values: values, start: 0_u64, end: 2_u64, slot: 3_u64);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  outer(values: &values, lo: 0_u64, hi: 2_u64, at: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn record_run(values: &Array<u64, 4>, start: u64, end: u64, slot: u64) -> result: unit writes(values) contract {
  requires start <= end;
  requires end <= 4_u64;
  requires slot < 4_u64;
} {
  let run = &deref(values)[start..end];
  let length = deref(run).len;
  set deref(values)[slot] = length;
  return unit;
}

fn outer(values: &Array<u64, 4>, lo: u64, hi: u64, at: u64) -> result: unit writes(values) contract {
  requires lo <= hi;
  requires hi <= 4_u64;
  requires at < 4_u64;
} {
  record_run(values: values, start: lo, end: hi, slot: at);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  outer(values: &values, lo: 0_u64, hi: 2_u64, at: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [WIN-2] a range beside a window's `next` is separated from it once
        // proved to end at or below the window's length.
        name: "call-separation-range-beside-next.wf",
        rejected: br#"fn count_then_push(r: &Slots<u64, 4>, lo: u64, hi: u64) -> result: u64 reads(r[lo..hi]), writes(r.next), writes(r.len) contract {
  requires lo <= hi;
  requires deref(r).len < 4_u64;
} {
  let seen = if hi <= deref(r).len {
    let run = &deref(r)[lo..hi];
    give deref(run).len;
  } else {
    give 0_u64;
  }
  place_back(window: r, value: seen);
  return seen;
}

fn caller(r: &Slots<u64, 4>, a: u64, b: u64) -> result: u64 writes(r) contract {
  requires a <= b;
  requires deref(r).len < 4_u64;
} {
  let x = count_then_push(r: r, lo: a, hi: b);
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u64, 4>();
  place_back(window: &window, value: 5_u64);
  place_back(window: &window, value: 6_u64);
  let x = caller(r: &window, a: 0_u64, b: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: when the range can end at or below the window's length here, prove that before this call; otherwise pass a range this call proves ends there, or replace the callee's row entries at or below their common path with one `writes` entry of that path\n",
        ],
        repaired: &[
            br#"fn count_then_push(r: &Slots<u64, 4>, lo: u64, hi: u64) -> result: u64 reads(r[lo..hi]), writes(r.next), writes(r.len) contract {
  requires lo <= hi;
  requires deref(r).len < 4_u64;
} {
  let seen = if hi <= deref(r).len {
    let run = &deref(r)[lo..hi];
    give deref(run).len;
  } else {
    give 0_u64;
  }
  place_back(window: r, value: seen);
  return seen;
}

fn caller(r: &Slots<u64, 4>, a: u64, b: u64) -> result: u64 writes(r) contract {
  requires a <= b;
  requires deref(r).len < 4_u64;
} {
  if b <= deref(r).len {
    let x = count_then_push(r: r, lo: a, hi: b);
    return x;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u64, 4>();
  place_back(window: &window, value: 5_u64);
  place_back(window: &window, value: 6_u64);
  let x = caller(r: &window, a: 0_u64, b: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn count_then_push(r: &Slots<u64, 4>, lo: u64, hi: u64) -> result: u64 reads(r[lo..hi]), writes(r.next), writes(r.len) contract {
  requires lo <= hi;
  requires deref(r).len < 4_u64;
} {
  let seen = if hi <= deref(r).len {
    let run = &deref(r)[lo..hi];
    give deref(run).len;
  } else {
    give 0_u64;
  }
  place_back(window: r, value: seen);
  return seen;
}

fn caller(r: &Slots<u64, 4>, a: u64, b: u64) -> result: u64 writes(r) contract {
  requires a <= b;
  requires deref(r).len < 4_u64;
} {
  let x = count_then_push(r: r, lo: 0_u64, hi: 0_u64);
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u64, 4>();
  place_back(window: &window, value: 5_u64);
  place_back(window: &window, value: 6_u64);
  let x = caller(r: &window, a: 0_u64, b: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn count_then_push(r: &Slots<u64, 4>, lo: u64, hi: u64) -> result: u64 writes(r) contract {
  requires lo <= hi;
  requires deref(r).len < 4_u64;
} {
  let seen = if hi <= deref(r).len {
    let run = &deref(r)[lo..hi];
    give deref(run).len;
  } else {
    give 0_u64;
  }
  place_back(window: r, value: seen);
  return seen;
}

fn caller(r: &Slots<u64, 4>, a: u64, b: u64) -> result: u64 writes(r) contract {
  requires a <= b;
  requires deref(r).len < 4_u64;
} {
  let x = count_then_push(r: r, lo: a, hi: b);
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = slots_new::<u64, 4>();
  place_back(window: &window, value: 5_u64);
  place_back(window: &window, value: 6_u64);
  let x = caller(r: &window, a: 0_u64, b: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // The index-beside-range question from two arguments of an interface
        // member's row, whose declaration bounds none of its positions.
        name: "call-separation-index-beside-range-two-arguments.wf",
        rejected: br#"interface Stage {
  fn step(x: &Array<u64, 4>, y: &Array<u64, 4>, lo: u64, hi: u64, i: u64) -> result: u64 reads(x[lo..hi]), writes(y[i]);
}

fn outer<interface Stage>(r: &Array<u64, 4>, a: u64, b: u64, k: u64) -> result: u64 writes(r) {
  let y = Stage::step(x: r, y: r, lo: a, hi: b, i: k);
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: when the index can lie outside the range here, prove before this call that it is below the range's start or at or after its end; otherwise pass positions this call proves apart\n",
        ],
        repaired: &[
            br#"interface Stage {
  fn step(x: &Array<u64, 4>, y: &Array<u64, 4>, lo: u64, hi: u64, i: u64) -> result: u64 reads(x[lo..hi]), writes(y[i]);
}

fn outer<interface Stage>(r: &Array<u64, 4>, a: u64, b: u64, k: u64) -> result: u64 writes(r) {
  if b <= k {
    let y = Stage::step(x: r, y: r, lo: a, hi: b, i: k);
    return y;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"interface Stage {
  fn step(x: &Array<u64, 4>, y: &Array<u64, 4>, lo: u64, hi: u64, i: u64) -> result: u64 reads(x[lo..hi]), writes(y[i]);
}

fn outer<interface Stage>(r: &Array<u64, 4>, a: u64, b: u64, k: u64) -> result: u64 writes(r) {
  let y = Stage::step(x: r, y: r, lo: 0_u64, hi: 2_u64, i: 3_u64);
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // The range-beside-`next` question from two arguments.
        name: "call-separation-range-beside-next-two-arguments.wf",
        rejected: br#"interface Stage {
  fn step(x: &Slots<u64, 4>, w: &Slots<u64, 4>, lo: u64, hi: u64) -> result: u64 reads(x[lo..hi]), writes(w.next);
}

fn outer<interface Stage>(r: &Slots<u64, 4>, a: u64, b: u64) -> result: u64 writes(r) {
  let y = Stage::step(x: r, w: r, lo: a, hi: b);
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: when the range can end at or below the window's length here, prove that before this call; otherwise pass a range this call proves ends there\n",
        ],
        repaired: &[
            br#"interface Stage {
  fn step(x: &Slots<u64, 4>, w: &Slots<u64, 4>, lo: u64, hi: u64) -> result: u64 reads(x[lo..hi]), writes(w.next);
}

fn outer<interface Stage>(r: &Slots<u64, 4>, a: u64, b: u64) -> result: u64 writes(r) {
  if b <= deref(r).len {
    let y = Stage::step(x: r, w: r, lo: a, hi: b);
    return y;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"interface Stage {
  fn step(x: &Slots<u64, 4>, w: &Slots<u64, 4>, lo: u64, hi: u64) -> result: u64 reads(x[lo..hi]), writes(w.next);
}

fn outer<interface Stage>(r: &Slots<u64, 4>, a: u64, b: u64) -> result: u64 writes(r) {
  let y = Stage::step(x: r, w: r, lo: 0_u64, hi: 0_u64);
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [EFF-5] two arguments name elements of one array at positions the
        // entailment fragment cannot prove distinct.
        name: "call-separation-two-arguments.wf",
        rejected: br#"fn copy_across(source: &u8, destination: &u8) -> result: unit reads(source), writes(destination) {
  let observed = deref(source);
  set deref(destination) = observed;
  return unit;
}

fn shift(values: &Array<u8, 4>, a: u64, b: u64) -> result: unit writes(values) contract {
  requires a < 4_u64;
  requires b < 4_u64;
} {
  copy_across(source: &deref(values)[a], destination: &deref(values)[b]);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  shift(values: &values, a: 1_u64, b: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: when the two positions can differ here, prove them distinct before this call; otherwise pass places this call proves do not overlap\n",
        ],
        repaired: &[
            br#"fn copy_across(source: &u8, destination: &u8) -> result: unit reads(source), writes(destination) {
  let observed = deref(source);
  set deref(destination) = observed;
  return unit;
}

fn shift(values: &Array<u8, 4>, a: u64, b: u64) -> result: unit writes(values) contract {
  requires a < 4_u64;
  requires b < 4_u64;
  requires a < b;
} {
  copy_across(source: &deref(values)[a], destination: &deref(values)[b]);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  shift(values: &values, a: 1_u64, b: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn copy_across(source: &u8, destination: &u8) -> result: unit reads(source), writes(destination) {
  let observed = deref(source);
  set deref(destination) = observed;
  return unit;
}

fn shift(values: &Array<u8, 4>, a: u64, b: u64) -> result: unit writes(values) contract {
  requires a < 4_u64;
  requires b < 4_u64;
} {
  copy_across(source: &deref(values)[0_u64], destination: &deref(values)[1_u64]);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 1_u8);
  shift(values: &values, a: 1_u64, b: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
];
