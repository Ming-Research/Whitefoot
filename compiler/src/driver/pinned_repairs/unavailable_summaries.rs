//! Callee-proof failures explained at a caller's dependent return.

use super::RepairPair;

pub(super) const UNAVAILABLE_SUMMARIES: &[RepairPair] = &[
    RepairPair {
        name: "partly-concrete-callee-summary-unavailable.wf",
        rejected: br#"fn caller<T: drop>(token: T) -> result: u64 pure contract {
  ensures result >= 1_u64;
} {
  let obtained = callee::<T, 1>(token: move token);
  return obtained;
}

fn callee<T: drop, const floor: u64>(token: T) -> result: u64 pure contract {
  ensures result >= floor;
} {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let obtained = caller::<unit>(token: unit);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: &[
            "\n  concrete_function: caller::<T>\n",
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: the returned value reads the result of `callee::<T, 1>`, whose summary `ensures result >= floor;` is unavailable because its [FN-9] obligation `0 - 0 <= -1` fails at line 11; if this summary supplies the missing return relation, repair that obligation in `callee::<T, 1>` so it proves the summary before relying on it at this return\n",
        ],
        repaired: &[br#"fn caller<T: drop>(token: T) -> result: u64 pure contract {
  ensures result >= 1_u64;
} {
  let obtained = callee::<T, 1>(token: move token);
  return obtained;
}

fn callee<T: drop, const floor: u64>(token: T) -> result: u64 pure contract {
  ensures result >= floor;
} {
  return floor;
}

fn main() -> status: std::process::ExitStatus pure {
  let obtained = caller::<unit>(token: unit);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "callee-header-failure-withholds-proved-summary.wf",
        rejected: br#"fn caller<T: drop>(token: T) -> result: u64 pure contract {
  ensures result >= 1_u64;
} {
  let obtained = callee::<T, 1>(token: move token);
  return obtained;
}

fn callee<T: drop, const floor: u64>(token: T) -> result: u64 pure contract {
  ensures result >= floor;
} {
  let progress = 0_u64;
  loop (
    invariant bounded: 1_u64 <= progress
  ) {
    break;
  }
  return floor;
}

fn main() -> status: std::process::ExitStatus pure {
  let obtained = caller::<unit>(token: unit);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: &[
            "\n  concrete_function: caller::<T>\n",
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: the returned value reads the result of `callee::<T, 1>`, whose summary `ensures result >= floor;` is unavailable because its [INV-1] obligation `1_u64 <= progress` fails at line 13; if this summary supplies the missing return relation, repair that obligation in `callee::<T, 1>` so it proves the summary before relying on it at this return\n",
        ],
        repaired: &[br#"fn caller<T: drop>(token: T) -> result: u64 pure contract {
  ensures result >= 1_u64;
} {
  let obtained = callee::<T, 1>(token: move token);
  return obtained;
}

fn callee<T: drop, const floor: u64>(token: T) -> result: u64 pure contract {
  ensures result >= floor;
} {
  let progress = 1_u64;
  loop (
    invariant bounded: 1_u64 <= progress
  ) {
    break;
  }
  return floor;
}

fn main() -> status: std::process::ExitStatus pure {
  let obtained = caller::<unit>(token: unit);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];

#[test]
fn overwriting_returned_call_result_does_not_blame_unavailable_summary() {
    let source = br#"fn caller<T: drop>(token: T, replacement: u64) -> result: u64 pure contract {
  ensures result >= 1_u64;
} {
  let obtained = callee::<T, 1>(token: move token);
  set obtained = replacement;
  return obtained;
}

fn callee<T: drop, const floor: u64>(token: T) -> result: u64 pure contract {
  ensures result >= floor;
} {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let obtained = caller::<unit>(token: unit, replacement: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = super::compile(
        &[super::SourceInput::new("overwritten-call-result.wf", source)],
        super::CompilerLimits::default(),
    )
    .expect_err("the caller cannot prove its postcondition over replacement");
    assert_eq!(failure.rule_id(), Some("FN-9"));
    let rendered = failure.to_string();
    assert!(rendered.contains("concrete_function: caller::<T>"), "{rendered}");
    assert!(rendered.contains("disposition: Unproved"), "{rendered}");
    assert!(!rendered.contains("whose summary"), "{rendered}");
}
