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
        // The reference's index is the field place `p.k`, which is rewritten before the write the reference must stay apart from, so the path the rejection names holds a superseded place and the description names that place.
        name: "reference-preservation-after-index-place-write.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct BystanderRecord {
  value: u8;
}

struct Cursor {
  k: u64;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}

fn inspect(values: &Array<BystanderRecord, 4>, p: Cursor, j: u64) -> result: u8 reads(values), writes(values[j]) contract {
  requires p.k < 4_u64;
  requires j < 4_u64;
} {
  let selected = &values^[p.k].value;
  set p.k = 1_u64;
  set values^[j] = BystanderRecord(value: 9_u8);
  return selected^;
}
"#,
        rule: "REF-2",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct BystanderRecord {
  value: u8;
}

struct Cursor {
  k: u64;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}

fn inspect(values: &Array<BystanderRecord, 4>, p: Cursor, j: u64) -> result: u8 reads(values), writes(values[j]) contract {
  requires p.k < j;
  requires j < 4_u64;
} {
  let k0 = p.k;
  let selected = &values^[k0].value;
  set p.k = 1_u64;
  set values^[j] = BystanderRecord(value: 9_u8);
  return selected^;
}
"#],
    },
];
