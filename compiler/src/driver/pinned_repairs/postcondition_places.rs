//! [ENT-2, FN-9] the repairs of a subscript in a place a postcondition
//! forms at its selected returns, pinned with the programs they produce. The
//! pair form and the harness that compiles every pair are
//! `driver::pinned_repairs`'s; the family lives apart only to keep each file
//! readable.

use super::RepairPair;

pub(super) const POSTCONDITION_PLACES: &[RepairPair] = &[
    RepairPair {
        // A postcondition forms its places at every return that selects it,
        // in the state that return leaves with: a requirement establishes
        // the bound at all of them, and a guard establishes it at the returns
        // it encloses, which here are all the routed clause selects. The
        // repair names the selecting return the subscript failed at.
        name: "bounds-of-a-place-a-postcondition-forms.wf",
        rejected: br#"fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64) -> r: Option<u64> reads(rows) contract {
  ensures when Some(value: v): v <= rows^[i].len;
} {
  if rows^[0_u64].len > 9_u64 {
    return None<u64>();
  }
  return Some<u64>(value: 0_u64);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  residual: i < rows^.len\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires i < rows^.len;` to the `contract` of `width`, which each caller then establishes, or establish it before the `return` on line 7 and every other `return` that selects this postcondition\n",
        ],
        repaired: &[
            br#"fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64) -> r: Option<u64> reads(rows) contract {
  requires i < rows^.len;
  ensures when Some(value: v): v <= rows^[i].len;
} {
  if rows^[0_u64].len > 9_u64 {
    return None<u64>();
  }
  return Some<u64>(value: 0_u64);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64) -> r: Option<u64> reads(rows) contract {
  ensures when Some(value: v): v <= rows^[i].len;
} {
  if rows^[0_u64].len > 9_u64 {
    return None<u64>();
  }
  if i < rows^.len {
    return Some<u64>(value: 0_u64);
  }
  return None<u64>();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // A definition that only a postcondition reaches is formed at that
        // postcondition's selected returns, not at body entry, so its repair
        // is the postcondition's and names the selecting return; before, it
        // was the requirement repair, which offered no return to guard. The
        // clause is unrouted and so selects every return, so only the
        // requirement is carried out here; the guard alternative is the
        // first pair's.
        name: "bounds-of-a-definition-only-a-postcondition-reaches.wf",
        rejected: br#"fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64) -> r: u64 reads(rows) contract {
  define w = rows^[i].len;
  ensures r <= w;
} {
  let z = rows^[0_u64].len;
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  residual: i < rows^.len\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires i < rows^.len;` to the `contract` of `width`, which each caller then establishes, or establish it before the `return` on line 6 and every other `return` that selects this postcondition\n",
        ],
        repaired: &[br#"fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64) -> r: u64 reads(rows) contract {
  define w = rows^[i].len;
  requires i < rows^.len;
  ensures r <= w;
} {
  let z = rows^[0_u64].len;
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
