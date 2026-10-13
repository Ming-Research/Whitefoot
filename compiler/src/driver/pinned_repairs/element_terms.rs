//! Element-term repairs and the aggregate index projection that retains an
//! occurrence-local value, consumed by the shared repair harness.

use super::RepairPair;

pub(super) const ELEMENT_TERMS: &[RepairPair] = &[
    RepairPair {
        // The collection root is a parameter, but its selector is local.
        // A requirement cannot refer to that selector at function entry.
        name: "call-requirement-over-an-element-with-a-local-selector.wf",
        rejected: br#"fn small(x: u8) -> result: u8 pure contract {
  requires x < 10_u8;
} {
  return x;
}

fn caller(values: &Array<u8, 2>, choose: u64) -> result: u8 reads(values) contract {
  requires choose < 2_u64;
} {
  let index = choose;
  let called = small(x: values^[index]);
  return called;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  instantiated_goal: values^[index] < 10_u8\n",
            "\n  mechanical_fix: `values^[index] < 10_u8` is not proved before this call: guard the call with `if values^[index] < 10_u8` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare\n",
        ],
        repaired: &[br#"fn small(x: u8) -> result: u8 pure contract {
  requires x < 10_u8;
} {
  return x;
}

fn caller(values: &Array<u8, 2>, choose: u64) -> result: u8 reads(values) contract {
  requires choose < 2_u64;
} {
  let index = choose;
  if values^[index] < 10_u8 {
    let called = small(x: values^[index]);
    return called;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // A whole struct element forms a structural index tree. The field
        // projection the requirement appends is outside that tree, so it
        // retains the legitimate occurrence-only binding repair.
        name: "call-requirement-over-an-argument-evaluated-in-the-call.wf",
        rejected: br#"struct Stats {
  count: u64;
}

fn small(x: Stats) -> result: u64 pure contract {
  requires x.count < 10_u64;
} {
  return x.count;
}

fn first(view: &[Stats]) -> result: u64 reads(view) contract {
  requires 1_u64 <= view^.len;
} {
  let called = small(x: view^[0_u64]);
  return called;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  instantiated_goal: argument #0 pre-transfer value.count < 10_u64\n",
            "\n  mechanical_fix: argument #0 is evaluated inside the call, where no fact names its value: bind it with one preceding `let`, establish the requirement over that binding, and pass the binding, borrowing it when the parameter is a reference\n",
        ],
        repaired: &[br#"struct Stats {
  count: u64;
}

fn small(x: Stats) -> result: u64 pure contract {
  requires x.count < 10_u64;
} {
  return x.count;
}

fn first(view: &[Stats]) -> result: u64 reads(view) contract {
  requires 1_u64 <= view^.len;
} {
  let captured = view^[0_u64];
  if captured.count < 10_u64 {
    let called = small(x: captured);
    return called;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
