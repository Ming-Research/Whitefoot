//! [FN-8, REF-1] a measure of an element whose captured index no longer
//! has a source spelling, so the rejection describes it in words. Keep these
//! programs while their repairs are printed.

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
            "\n  instantiated_goal: 0_u64 < rows^[‹the value of k before it was rewritten›].len\n",
            "\n  mechanical_fix: 0_u64 < rows^[‹the value of k before it was rewritten›].len reads an element selected by the old value of `k`, which was rewritten after the reference was formed; before forming that reference, save the index in a fresh binding (for example `let k0 = k;`), form the reference through the saved binding, establish the requirement through that reference before the write, and keep the saved binding unchanged so those facts survive the write\n",
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
        // The index is a place, `p.k`, so the captured term is a superseded place rather than a superseded binding, and the description names the place.
        name: "requirement-after-index-place-write.wf",
        rejected: br#"struct Cursor {
  k: u64;
}

fn need(row: &Slots<u64, 8>) -> result: unit reads(row) contract {
  requires 0_u64 < row^.len;
} {
  let value = row^[0_u64];
  return unit;
}

fn inspect(rows: &Array<Slots<u64, 8>, 2>, p: Cursor) -> result: unit reads(rows) contract {
  requires p.k < 2_u64;
} {
  let wr = &rows^[p.k];
  if 0_u64 < wr^.len {
    set p.k = 1_u64;
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
            "\n  instantiated_goal: 0_u64 < rows^[\u{2039}the value of p.k before it was rewritten\u{203a}].len\n",
        ],
        repaired: &[br#"struct Cursor {
  k: u64;
}

fn need(row: &Slots<u64, 8>) -> result: unit reads(row) contract {
  requires 0_u64 < row^.len;
} {
  let value = row^[0_u64];
  return unit;
}

fn inspect(rows: &Array<Slots<u64, 8>, 2>, p: Cursor) -> result: unit reads(rows) contract {
  requires p.k < 2_u64;
} {
  let k0 = p.k;
  let wr = &rows^[k0];
  if 0_u64 < wr^.len {
    set p.k = 1_u64;
    need(row: wr);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
