//! Callee-proof failures explained at a dependent return or requirement.

use super::RepairPair;

macro_rules! budget_source {
    ($summary:literal, $header:literal, $after:literal, $requirement:literal) => {
        concat!(
            "fn caller(budget: &Budget) -> result: unit reads(budget.popped), writes(budget.end) {\n",
            "  batch(budget: budget);\n",
            $after,
            "  commit(budget: budget);\n",
            "  return unit;\n",
            "}\n\n",
            "struct Budget {\n",
            "  end: u64;\n",
            "  popped: u64;\n",
            "}\n\n",
            "fn batch(budget: &Budget) -> result: unit reads(budget.popped), writes(budget.end) contract {\n",
            $summary,
            "} {\n",
            "  let end = 0_u64;\n",
            $header,
            "    if budget^.popped <= end {\n",
            "      break;\n",
            "    }\n",
            "    set end = end + 1_u64;\n",
            "  }\n",
            "  set budget^.end = end;\n",
            "  return unit;\n",
            "}\n\n",
            "fn commit(budget: &Budget) -> result: unit pure contract {\n",
            $requirement,
            "} {\n",
            "  return unit;\n",
            "}\n\n",
            "fn main() -> status: std::process::ExitStatus pure {\n",
            "  let budget = Budget(end: 0_u64, popped: 4_u64);\n",
            "  caller(budget: &budget);\n",
            "  return std::process::exit_status(code: 0_u8);\n",
            "}\n",
        )
        .as_bytes()
    };
}

pub(super) const UNAVAILABLE_SUMMARIES: &[RepairPair] = &[
    RepairPair {
        name: "callee-loop-withholds-reference-summary.wf",
        rejected: budget_source!(
            "  ensures budget^.end <= budget^.popped;\n",
            "  loop {\n",
            "",
            "  requires budget^.end <= budget^.popped;\n"
        ),
        rule: "FN-8",
        sentences: &[
            "\n  concrete_callee: commit\n",
            "\n  instantiated_goal: budget^.end <= budget^.popped\n",
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: the call to `batch` at line 2 cannot supply its summary `ensures budget^.end <= budget^.popped;` because that callee's [FN-9] obligation `budget^.end - budget^.popped <= 0` fails at line 23; repair that obligation in `batch` so it proves the summary needed by this requirement; for a loop-computed bound, carry the needed bound in a loop-header `invariant`\n",
        ],
        repaired: &[budget_source!(
            "  ensures budget^.end <= budget^.popped;\n",
            "  loop (\n    invariant bounded: end <= budget^.popped\n  ) {\n",
            "",
            "  requires budget^.end <= budget^.popped;\n"
        )],
    },
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
fn reference_summary_must_be_unavailable_and_sufficient() {
    let weak = budget_source!(
        "  ensures budget^.end >= 0_u64;\n",
        "  loop {\n",
        "",
        "  requires budget^.end <= budget^.popped;\n"
    );
    let without_requirement = std::str::from_utf8(weak)
        .expect("the witness is ASCII")
        .replacen("  commit(budget: budget);\n", "", 1);
    super::compile(
        &[super::SourceInput::new(
            "weak-reference-summary-is-available.wf",
            without_requirement.as_bytes(),
        )],
        super::CompilerLimits::default(),
    )
    .expect("the callee proves its weaker summary");
    let cases: &[(&str, &[u8])] = &[
        ("available-but-weak-reference-summary.wf", weak),
        (
            "unavailable-reference-summary-does-not-prove-target.wf",
            budget_source!(
                "  ensures budget^.end <= budget^.popped;\n",
                "  loop {\n",
                "",
                "  requires budget^.end < budget^.popped;\n"
            ),
        ),
        (
            "reference-summary-killed-before-requirement.wf",
            budget_source!(
                "  ensures budget^.end <= budget^.popped;\n",
                "  loop {\n",
                "  set budget^.end = 1_u64;\n",
                "  requires budget^.end <= budget^.popped;\n"
            ),
        ),
        (
            "reference-summary-contradicts-a-branch.wf",
            budget_source!(
                "  ensures budget^.end <= budget^.popped;\n",
                "  loop {\n",
                "  if budget^.popped < budget^.end {\n    set budget^.end = 1_u64;\n  }\n",
                "  requires budget^.end <= budget^.popped;\n"
            ),
        ),
    ];
    for (name, source) in cases {
        let failure = super::compile(
            &[super::SourceInput::new(name, source)],
            super::CompilerLimits::default(),
        )
        .expect_err("this requirement does not follow from an unavailable summary");
        assert_eq!(failure.rule_id(), Some("FN-8"), "{name}: {failure}");
        let rendered = failure.to_string();
        assert!(rendered.contains("concrete_callee: commit"), "{name}: {rendered}");
        assert!(rendered.contains("disposition: Unproved"), "{name}: {rendered}");
        assert!(!rendered.contains("cannot supply its summary"), "{name}: {rendered}");
    }
}

#[test]
fn reference_summary_tracks_the_call_occurrence_and_surviving_support() {
    let cases: &[(&str, &[u8], &str)] = &[
        (
            "reference-summary-survives-unrelated-write.wf",
            budget_source!(
                "  ensures budget^.end <= budget^.popped;\n",
                "  loop {\n",
                "  let marker = 0_u64;\n  set marker = 1_u64;\n",
                "  requires budget^.end <= budget^.popped;\n"
            ),
            "the call to `batch` at line 2 cannot supply its summary",
        ),
        (
            "only-second-call-can-supply-reference-summary.wf",
            budget_source!(
                "  ensures budget^.end <= budget^.popped;\n",
                "  loop {\n",
                "  set budget^.end = 1_u64;\n  batch(budget: budget);\n",
                "  requires budget^.end <= budget^.popped;\n"
            ),
            "the call to `batch` at line 4 cannot supply its summary",
        ),
    ];
    for (name, source, expected) in cases {
        let failure = super::compile(
            &[super::SourceInput::new(name, source)],
            super::CompilerLimits::default(),
        )
        .expect_err("the callee has not proved its reference summary");
        assert_eq!(failure.rule_id(), Some("FN-8"), "{name}: {failure}");
        let rendered = failure.to_string();
        assert!(rendered.contains(expected), "{name}: {rendered}");
        assert!(!rendered.contains("or guard"), "{name}: {rendered}");
    }
}

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
