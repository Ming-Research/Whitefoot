//! Waiting-kind [WAIT-1] and loop-progress [TERM-1] repairs, consumed by the
//! shared repair harness. Keep these programs while the corresponding
//! repairs are printed.

use super::RepairPair;

pub(super) const WAITING_AND_PROGRESS: &[RepairPair] = &[
    // -------------------------------------------------------------------
    // [WAIT-1] a declared waiting kind the body does not match. Each repair
    // either writes the kind the body has or changes the body's waits.
    // -------------------------------------------------------------------
    RepairPair {
        name: "waiting-kind-without-a-wait.wf",
        rejected: br#"fn pause(skip: Bool, until: std::time::Instant) -> result: unit pure may_wait {
  if skip {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "WAIT-1",
        sentences: &[
            "]: WaitKindMismatch\n",
            "\n  mechanical_fix: the body executes no wait: remove the waiting kind after the effect row, or execute a wait in the body, on every path to an exit for `must_wait` and on some path but not every one for `may_wait`\n",
        ],
        repaired: &[
            br#"fn pause(skip: Bool, until: std::time::Instant) -> result: unit pure {
  if skip {
    return unit;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn pause(skip: Bool, until: std::time::Instant) -> result: unit pure may_wait {
  if skip {
    return unit;
  }
  std::time::sleep_until(deadline: until);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "must-wait-with-a-path-that-does-not-wait.wf",
        rejected: br#"fn pause(skip: Bool, until: std::time::Instant) -> result: unit pure must_wait {
  if skip {
    return unit;
  }
  std::time::sleep_until(deadline: until);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "WAIT-1",
        sentences: &[
            "]: WaitKindMismatch\n",
            "\n  mechanical_fix: a path from the body's start to an exit executes no wait: write `may_wait`, or execute a wait on that path before its exit\n",
        ],
        repaired: &[
            br#"fn pause(skip: Bool, until: std::time::Instant) -> result: unit pure may_wait {
  if skip {
    return unit;
  }
  std::time::sleep_until(deadline: until);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn pause(skip: Bool, until: std::time::Instant) -> result: unit pure must_wait {
  if skip {
    std::time::sleep_until(deadline: until);
    return unit;
  }
  std::time::sleep_until(deadline: until);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "may-wait-over-a-body-that-always-waits.wf",
        rejected: br#"fn pause(until: std::time::Instant) -> result: unit pure may_wait {
  std::time::sleep_until(deadline: until);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "WAIT-1",
        sentences: &[
            "]: WaitKindMismatch\n",
            "\n  mechanical_fix: every path from the body's start to an exit executes a wait: write `must_wait`, so that every call of the function is a wait\n",
        ],
        repaired: &[br#"fn pause(until: std::time::Instant) -> result: unit pure must_wait {
  std::time::sleep_until(deadline: until);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // -------------------------------------------------------------------
    // [TERM-1] a loop whose only exit test leaves on an equality. The
    // repair orders the test, which derives a rank; a written rank is no
    // alternative, since its descent needs the order the equality lacks.
    // -------------------------------------------------------------------
    RepairPair {
        name: "loop-leaving-on-an-equality.wf",
        rejected: br#"fn count_to(limit: u64) -> result: u64 pure {
  let cursor = 0_u64;
  loop {
    if cursor == limit {
      break;
    }
    set cursor = cursor + 1_u64;
  }
  return cursor;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TERM-1",
        sentences: &[
            "]: LoopWithoutProgress\n",
            "\n  source:     if cursor == limit {\n  marker:        ^^^^^^^^^^^^^^^\n",
            "\n  mechanical_fix: this exit test leaves the loop on `==`, from which no rank derives, since a cursor that steps past the value it is compared with never meets it: compare with an order instead, such as `if i >= count { break; }` for a cursor `i` that rises to `count`\n",
        ],
        repaired: &[
            br#"fn count_to(limit: u64) -> result: u64 pure {
  let cursor = 0_u64;
  loop {
    if cursor >= limit {
      break;
    }
    set cursor = cursor + 1_u64;
  }
  return cursor;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
];
