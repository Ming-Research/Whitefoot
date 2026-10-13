//! [FN-8, REF-1] a measure of an element whose captured index no longer
//! has a source spelling. Keep these programs while their repairs are printed.

use super::RepairPair;

pub(super) const CAPTURED_INDICES: &[RepairPair] = &[
    RepairPair {
        name: "requirement-after-index-write.wf",
        rejected: br#"fn need(row: &Slots<u64, 8>) -> result: unit reads(row) contract {
  requires 0_u64 < row^.len;
} {
  let value = row^[0_u64];
  return unit;
}

fn inspect(rows: &Array<Slots<u64, 8>, 2>, k: u64) -> result: unit reads(rows) contract {
  requires k < 2_u64;
} {
  let wr = &rows^[k];
  if 0_u64 < wr^.len {
    set k = 1_u64;
    need(row: wr);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  instantiated_goal: 0_u64 < rows^[?].len\n",
            "\n  mechanical_fix: `0_u64 < rows^[?].len` reads an element selected by the old value of `k`, which was rewritten after the reference was formed; before forming that reference, save the index in a fresh binding (for example `let k0 = k;`), form the reference through the saved binding, establish the requirement through that reference before the write, and keep the saved binding unchanged so those facts survive the write\n",
        ],
        repaired: &[br#"fn need(row: &Slots<u64, 8>) -> result: unit reads(row) contract {
  requires 0_u64 < row^.len;
} {
  let value = row^[0_u64];
  return unit;
}

fn inspect(rows: &Array<Slots<u64, 8>, 2>, k: u64) -> result: unit reads(rows) contract {
  requires k < 2_u64;
} {
  let k0 = k;
  let wr = &rows^[k0];
  if 0_u64 < wr^.len {
    set k = 1_u64;
    need(row: wr);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "requirement-through-loop-carried-reference.wf",
        rejected: br#"fn need(row: &Slots<u64, 8>) -> result: unit reads(row) contract {
  requires 0_u64 < row^.len;
} {
  let value = row^[0_u64];
  return unit;
}

fn inspect(rows: &Array<Slots<u64, 8>, 2>) -> result: unit reads(rows) {
  let wr = &rows^[0_u64];
  for (k in 0_u64..2_u64) {
    if 0_u64 < rows^[k].len {
      need(row: wr);
    }
    set wr = &rows^[k];
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  instantiated_goal: 0_u64 < rows^[?].len\n",
            "\n  mechanical_fix: `0_u64 < rows^[?].len` reads an element selected by a loop-carried reference whose old index has no current spelling; when this call is intended to use the current iteration's element, re-form the reference inside the loop after selecting that element's index, then establish the requirement through that reference before the call\n",
        ],
        repaired: &[br#"fn need(row: &Slots<u64, 8>) -> result: unit reads(row) contract {
  requires 0_u64 < row^.len;
} {
  let value = row^[0_u64];
  return unit;
}

fn inspect(rows: &Array<Slots<u64, 8>, 2>) -> result: unit reads(rows) {
  let wr = &rows^[0_u64];
  for (k in 0_u64..2_u64) {
    set wr = &rows^[k];
    if 0_u64 < wr^.len {
      need(row: wr);
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
