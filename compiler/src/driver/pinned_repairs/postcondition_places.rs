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
    RepairPair {
        // The place `entry(rows)^[i]` reads the base at function entry, so its
        // subscript is refuted against the entry length although the append
        // makes the element exist at the return; the residual spells the entry
        // image and the repair names which value is meant.
        name: "bounds-of-an-entry-image-place-refuted.wf",
        rejected: br#"fn grow_claim(rows: &Slots<Slots<u8, 8>, 4>, i: u64) -> r: u64 writes(rows.next), writes(rows.len) contract {
  requires i == rows^.len;
  requires rows^.len < rows^.cap;
  ensures r <= entry(rows)^[i].len;
} {
  let fresh = slots_new::<u8, 8>();
  place_back(window: rows, value: move fresh);
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  residual: i < entry(rows)^.len\n  disposition: Refuted\n",
            "\n  mechanical_fix: `i < entry(rows)^.len` is false at the `return` on line 8, which selects this postcondition, with `rows^.len` at its function-entry value: index within the storage, or change the statements or requirements that fix the index\n",
        ],
        repaired: &[br#"fn grow_claim(rows: &Slots<Slots<u8, 8>, 4>, i: u64) -> r: u64 writes(rows.next), writes(rows.len) contract {
  requires i < rows^.len;
  requires rows^.len < rows^.cap;
  ensures r <= entry(rows)^[i].len;
} {
  let fresh = slots_new::<u8, 8>();
  place_back(window: rows, value: move fresh);
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // A test of the length after the append is no evidence for the entry
        // image's length: the repair asks for the bound at entry, which a
        // requirement gives, or a test made before the body changes it.
        name: "bounds-of-an-entry-image-place-unproved.wf",
        rejected: br#"fn grow_claim(rows: &Slots<Slots<u8, 8>, 4>, i: u64) -> r: u64 writes(rows.next), writes(rows.len) contract {
  requires rows^.len < rows^.cap;
  ensures r <= entry(rows)^[i].len;
} {
  let fresh = slots_new::<u8, 8>();
  place_back(window: rows, value: move fresh);
  if i < rows^.len {
    return 0_u64;
  }
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  residual: i < entry(rows)^.len\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires i < rows^.len;` to the `contract` of `grow_claim`, which each caller then establishes, or establish it with `rows^.len` at its function-entry value before the `return` on line 8 and every other `return` that selects this postcondition, which a test made after the body changes it does not\n",
        ],
        repaired: &[
            br#"fn grow_claim(rows: &Slots<Slots<u8, 8>, 4>, i: u64) -> r: u64 writes(rows.next), writes(rows.len) contract {
  requires i < rows^.len;
  requires rows^.len < rows^.cap;
  ensures r <= entry(rows)^[i].len;
} {
  let fresh = slots_new::<u8, 8>();
  place_back(window: rows, value: move fresh);
  if i < rows^.len {
    return 0_u64;
  }
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn grow_claim(rows: &Slots<Slots<u8, 8>, 4>, i: u64) -> r: u64 writes(rows.next), writes(rows.len) contract {
  requires rows^.len < rows^.cap;
  ensures r <= entry(rows)^[i].len;
} {
  if i < rows^.len {
    let fresh = slots_new::<u8, 8>();
    place_back(window: rows, value: move fresh);
    return 0_u64;
  }
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // The place reads `i` at function entry; once the body rebinds `i` the
        // clause is unavailable at the return whatever bounds it, so the repair
        // gives the new value its own name besides bounding the entry value.
        name: "bounds-of-a-place-below-a-rebound-offset.wf",
        rejected: br#"fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64, j: u64) -> r: u64 pure contract {
  ensures r <= rows^[i].len;
} {
  set i = j;
  if i < 4_u64 {
    return 0_u64;
  }
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  residual: i < rows^.len\n  disposition: Unproved\n",
            "\n  mechanical_fix: `i < rows^.len` reads `i` at its function-entry value, and a clause reading it is unavailable at the `return` on line 6, where the body has rebound it: give the new value its own name instead of rebinding `i`, and add `requires i < rows^.len;` to the `contract` of `width`, which each caller then establishes\n",
        ],
        repaired: &[br#"fn width(rows: &Array<Slots<u8, 8>, 4>, i: u64, j: u64) -> r: u64 pure contract {
  requires i < rows^.len;
  ensures r <= rows^[i].len;
} {
  let k = j;
  if k < 4_u64 {
    return 0_u64;
  }
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
