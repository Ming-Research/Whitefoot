//! Selector and expired header-name repairs, consumed by the shared repair
//! harness. Keep these programs while the corresponding repairs are printed.

use super::RepairPair;

const SELECTOR: &[&str] = &[
    "]: InvalidPostconditionSelector\n",
    "\n  mechanical_fix: remove this ensures clause, and remove its contract block if no requires or ensures clauses remain; an unrouted clause can name only result data admitted by [CALL-4]; a routed clause selects only `when Ok(value: r):` for an own Result<T, E> or `when Some(value: r):` for an own Option<T>, with a fresh r and a payload T that supplies admitted data [FN-9]; Err, None and user-enum variants are not postcondition routes\n",
];

pub(super) const SELECTOR_SCOPE: &[RepairPair] = &[
    RepairPair {
        name: "none-postcondition-remove-empty-contract.wf",
        rejected: include_bytes!("../../../../tests/conformance/cases/fn9-neg-none-route.wf"),
        rule: "FN-9",
        sentences: SELECTOR,
        repaired: &[br#"fn nothing(limit: u64) -> result: Option<u64> pure {
  return None<u64>();
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "err-postcondition-retain-requires.wf",
        rejected: br#"fn fail(code: u64) -> result: Result<u64, u64> pure contract {
  requires code <= 10_u64;
  ensures when Err(error: e): e == code;
} {
  return Err<u64, u64>(error: code);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: SELECTOR,
        repaired: &[br#"fn fail(code: u64) -> result: Result<u64, u64> pure contract {
  requires code <= 10_u64;
} {
  return Err<u64, u64>(error: code);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "user-enum-postcondition-retain-result-type.wf",
        rejected: br#"enum Foreign {
  ForeignCase(value: i32);
}

fn selected(value: i32) -> result: Foreign pure contract {
  ensures when ForeignCase(value: payload): payload == value;
} {
  return Foreign::ForeignCase(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: SELECTOR,
        repaired: &[br#"enum Foreign {
  ForeignCase(value: i32);
}

fn selected(value: i32) -> result: Foreign pure {
  return Foreign::ForeignCase(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "unit-postcondition-remove-define-only-contract.wf",
        rejected: br#"fn invalid() -> result: unit pure contract {
  define zero = 0_i32;
  ensures result == zero;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: SELECTOR,
        repaired: &[br#"fn invalid() -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "counted-header-out-of-scope-auto.wf",
        rejected: include_bytes!("../../../../tests/conformance/cases/inv1-neg-header-name-after-loop.wf"),
        rule: "INV-1",
        sentences: &[
            "]: InvisibleUse\n",
            "\n  mechanical_fix: header invariant `bounded` can be named only inside its loop body [INV-1]; its conclusion survives only under the ordinary fact rules [ENT-5]: if AUTO proves this target, remove its proof block; otherwise replace this use of `bounded` with an available relation-form premise whose terms are in scope, keeping its coefficient [PRF-1]\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let cursor = 0_u64;
  for (
    i in 0_u64..4_u64,
    invariant bounded: cursor <= i
  ) {
    set cursor = cursor + 1_u64;
  }
  invariant after_loop: cursor <= 4_u64;
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "ordinary-header-out-of-scope-relation-premise.wf",
        rejected: br#"fn combine(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64) -> result: unit pure contract {
  requires a <= b;
  requires c <= d;
  requires e <= f;
} {
  loop (
    invariant first: a <= b
  ) {
    break;
  }
  invariant total: 3_u64 * a + c + e <= 3_u64 * b + d + f {
    use 3 times first;
    use (c <= d);
    use (e <= f);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "INV-1",
        sentences: &[
            "]: InvisibleUse\n",
            "\n  mechanical_fix: header invariant `first` can be named only inside its loop body [INV-1]; its conclusion survives only under the ordinary fact rules [ENT-5]: if AUTO proves this target, remove its proof block; otherwise replace this use of `first` with an available relation-form premise whose terms are in scope, keeping its coefficient [PRF-1]\n",
        ],
        repaired: &[br#"fn combine(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64) -> result: unit pure contract {
  requires a <= b;
  requires c <= d;
  requires e <= f;
} {
  loop (
    invariant first: a <= b
  ) {
    break;
  }
  invariant total: 3_u64 * a + c + e <= 3_u64 * b + d + f {
    use 3 times (a <= b);
    use (c <= d);
    use (e <= f);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
