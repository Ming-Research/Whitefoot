//! Existing expanded requirements are moved before the places they admit.

use super::RepairPair;

pub(super) const CLAUSE_REQUIREMENTS: &[RepairPair] = &[
    RepairPair {
        name: "clause-bound-defined-later-requirement.wf",
        rejected: br#"fn inspect(rows: &[Slots<u8, 4>], i: u64) -> result: unit reads(rows) contract {
  define bound = i < rows^.len;
  define inside = bound;
  requires rows^[i].len == 0_u64;
  requires inside;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "]: UndischargedBoundsObligation\n",
            "\n  mechanical_fix: move the existing `requires inside;` and any `define` declarations it needs ahead of the requirement that forms this place, keeping those definitions before their uses; it already states `i < rows^.len`\n",
        ],
        repaired: &[br#"fn inspect(rows: &[Slots<u8, 4>], i: u64) -> result: unit reads(rows) contract {
  define bound = i < rows^.len;
  define inside = bound;
  requires inside;
  requires rows^[i].len == 0_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "clause-bound-constant-later-requirement.wf",
        rejected: br#"const first_index: u64 = 0_u64;

fn inspect(rows: &[Slots<u8, 4>]) -> result: unit reads(rows) contract {
  requires rows^[0_u64].len == 0_u64;
  requires first_index < rows^.len;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "]: UndischargedBoundsObligation\n",
            "\n  mechanical_fix: move the existing `requires first_index < rows^.len;` and any `define` declarations it needs ahead of the requirement that forms this place, keeping those definitions before their uses; it already states `0_u64 < rows^.len`\n",
        ],
        repaired: &[br#"const first_index: u64 = 0_u64;

fn inspect(rows: &[Slots<u8, 4>]) -> result: unit reads(rows) contract {
  requires first_index < rows^.len;
  requires rows^[0_u64].len == 0_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
