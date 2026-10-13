//! Indexed postcondition formation repairs, consumed by the shared repair
//! harness. Keep these programs while the corresponding repairs are printed.

use super::RepairPair;

pub(super) const POSTCONDITION_BOUNDS: &[RepairPair] = &[
    RepairPair {
        name: "postcondition-entry-element-needs-a-bound.wf",
        rejected:
            br#"fn first(values: Array<Slots<u8, 2>, 2>, index: u64) -> result: u64 pure contract {
  ensures result == values[index].len;
} {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: add a `requires` over the parameters of `first` establishing each index below the length of the storage it selects at entry, which each caller then establishes\n",
        ],
        repaired: &[
            br#"fn first(values: Array<Slots<u8, 2>, 2>, index: u64) -> result: u64 pure contract {
  requires index < 2_u64;
  ensures result == values[index].len;
} {
  return values[index].len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "postcondition-entry-element-is-out-of-bounds.wf",
        rejected: br#"fn first(values: Array<Slots<u8, 2>, 2>) -> result: u64 pure contract {
  ensures result == values[2_u64].len;
} {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: select an element that exists at function entry, or change the parameter storage type and requirements so that the selected entry element exists\n",
        ],
        repaired: &[
            br#"fn first(values: Array<Slots<u8, 2>, 2>) -> result: u64 pure contract {
  ensures result == values[1_u64].len;
} {
  return values[1_u64].len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn first(values: Array<Slots<u8, 2>, 3>) -> result: u64 pure contract {
  ensures result == values[2_u64].len;
} {
  return values[2_u64].len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "postcondition-exit-element-needs-a-bound.wf",
        rejected: br#"fn establish(values: &Array<u8, 2>, index: u64) -> result: unit writes(values) contract {
  ensures values^[index] == 1_u8;
} {
  set values^[0_u64] = 1_u8;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: retain or restore storage whose length contains every selected index before each selected return, and establish those bounds independently of the postcondition; or state a postcondition over elements that exist there\n",
        ],
        repaired: &[
            br#"fn establish(values: &Array<u8, 2>, index: u64) -> result: unit writes(values) contract {
  requires index < 2_u64;
  ensures values^[index] == 1_u8;
} {
  set values^[index] = 1_u8;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn establish(values: &Array<u8, 2>, index: u64) -> result: unit writes(values) contract {
  ensures values^[0_u64] == 1_u8;
} {
  set values^[0_u64] = 1_u8;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
];
