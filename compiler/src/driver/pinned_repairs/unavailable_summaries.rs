//! Callee-proof failures explained at a dependent proof.

use super::RepairPair;

macro_rules! scanner_source {
    ($header:literal) => {
        concat!(
            "struct Budget {\n  end: u64;\n  popped: u64;\n}\n\n",
            "fn batch(budget: &Budget) -> result: unit reads(budget.popped), writes(budget.end) contract {\n",
            "  ensures budget^.end <= budget^.popped;\n",
            "} {\n",
            "  let end = 0_u64;\n",
            $header,
            "    if budget^.popped <= end {\n      break;\n    }\n",
            "    set end = end + 1_u64;\n",
            "  }\n",
            "  loop (\n    invariant upper: end <= budget^.popped\n  ) {\n    break;\n  }\n",
            "  set budget^.end = budget^.popped;\n",
            "  return unit;\n",
            "}\n",
        )
        .as_bytes()
    };
}

macro_rules! replay_source {
    ($contract:literal, $body:literal) => {
        concat!(
            "fn caller(budget: &Budget) -> result: unit reads(budget.popped), writes(budget.end)",
            $contract,
            " {\n",
            $body,
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

/// The ordinary pair supplies the scanner source; a second, earlier source
/// supplies its dependent proof, retaining the original first-error order.
struct MultiSourceRepairPair {
    caller_name: &'static str,
    caller: &'static [u8],
    callee: RepairPair,
}

const SCANNER_REPAIRS: &[MultiSourceRepairPair] = &[
    MultiSourceRepairPair {
        caller_name: "a-loop-replay.wf",
        caller: replay_source!(
            " contract {\n  requires budget^.end <= budget^.popped;\n}",
            "  loop (\n    invariant window: budget^.end <= budget^.popped\n  ) {\n    batch(budget: budget);\n    if budget^.popped == 0_u64 {\n      break;\n    }\n  }\n"
        ),
        callee: RepairPair {
            name: "z-scanner.wf",
            rejected: scanner_source!("  loop {\n"),
            rule: "INV-1",
            sentences: &[
                "\n  name: window\n",
                "\n  obligation: Backedge\n",
                "\n  mechanical_fix: the call to `batch` at a-loop-replay.wf:7 cannot supply its summary `ensures budget^.end <= budget^.popped;` because that callee's [INV-1] obligation `end <= budget^.popped` fails at z-scanner.wf:17; repair that obligation in `batch` so it proves the summary needed by this proof; for a loop-computed bound, carry the needed bound in a loop-header `invariant`\n",
            ],
            repaired: &[scanner_source!(
                "  loop (\n    invariant bounded: end <= budget^.popped\n  ) {\n"
            )],
        },
    },
    MultiSourceRepairPair {
        caller_name: "a-local-replay.wf",
        caller: replay_source!(
            "",
            "  batch(budget: budget);\n  invariant available: budget^.end <= budget^.popped;\n"
        ),
        callee: RepairPair {
            name: "z-scanner.wf",
            rejected: scanner_source!("  loop {\n"),
            rule: "INV-1",
            sentences: &[
                "\n  name: available\n",
                "the call to `batch` at a-local-replay.wf:2 cannot supply its summary `ensures budget^.end <= budget^.popped;`",
                "that callee's [INV-1] obligation `end <= budget^.popped` fails at z-scanner.wf:17",
            ],
            repaired: &[scanner_source!(
                "  loop (\n    invariant bounded: end <= budget^.popped\n  ) {\n"
            )],
        },
    },
    MultiSourceRepairPair {
        caller_name: "a-bounds-replay.wf",
        caller: replay_source!(
            " contract {\n  requires budget^.popped < 8_u64;\n}",
            "  batch(budget: budget);\n  let window = array_filled::<u64, 8>(value: 0_u64);\n  set window[budget^.end] = 1_u64;\n"
        ),
        callee: RepairPair {
            name: "z-scanner.wf",
            rejected: scanner_source!("  loop {\n"),
            rule: "OP-4",
            sentences: &[
                "the call to `batch` at a-bounds-replay.wf:4 cannot supply its summary `ensures budget^.end <= budget^.popped;`",
                "that callee's [INV-1] obligation `end <= budget^.popped` fails at z-scanner.wf:17",
            ],
            repaired: &[scanner_source!(
                "  loop (\n    invariant bounded: end <= budget^.popped\n  ) {\n"
            )],
        },
    },
    MultiSourceRepairPair {
        caller_name: "a-postcondition-replay.wf",
        caller: replay_source!(
            " contract {\n  ensures budget^.end <= budget^.popped;\n}",
            "  batch(budget: budget);\n  let marker = 0_u64;\n  set marker = 1_u64;\n"
        ),
        callee: RepairPair {
            name: "z-scanner.wf",
            rejected: scanner_source!("  loop {\n"),
            rule: "FN-9",
            sentences: &[
                "the call to `batch` at a-postcondition-replay.wf:4 cannot supply its summary `ensures budget^.end <= budget^.popped;`",
                "that callee's [INV-1] obligation `end <= budget^.popped` fails at z-scanner.wf:17",
            ],
            repaired: &[scanner_source!(
                "  loop (\n    invariant bounded: end <= budget^.popped\n  ) {\n"
            )],
        },
    },
];

#[test]
fn every_recorded_consumer_names_the_failed_callee_in_the_other_file() {
    for pair in SCANNER_REPAIRS {
        let failure = super::compile(
            &[
                super::SourceInput::new(pair.caller_name, pair.caller),
                super::SourceInput::new(pair.callee.name, pair.callee.rejected),
            ],
            super::CompilerLimits::default(),
        )
        .expect_err(pair.caller_name);
        assert_eq!(failure.rule_id(), Some(pair.callee.rule), "{failure}");
        assert_eq!(
            failure.location().expect("caller location").path(),
            pair.caller_name
        );
        let rendered = format!("{failure}\n");
        for expected in pair.callee.sentences {
            assert!(
                rendered.contains(expected),
                "wanted: {expected}\ngot: {rendered}"
            );
        }
        for repaired in pair.callee.repaired {
            let mut functions = super::declared_functions(pair.caller);
            functions.extend(super::declared_functions(repaired));
            let contradictions = super::super::with_checked_program(
                &[
                    super::SourceInput::new(pair.caller_name, pair.caller),
                    super::SourceInput::new(pair.callee.name, repaired),
                ],
                None,
                super::CompilerLimits::default(),
                |program, _| Ok(program.contradictory_successes(&functions)),
            )
            .expect("adding the callee's header invariant repairs the whole two-source unit");
            assert!(contradictions.is_empty(), "{contradictions:?}");
        }
    }
}

#[test]
fn recorded_consumers_do_not_blame_an_insufficient_summary() {
    let cases: &[(&str, &[u8], &str, &str)] = &[
        (
            "a-insufficient-loop-summary.wf",
            replay_source!(
                " contract {\n  requires budget^.end < budget^.popped;\n}",
                "  loop (\n    invariant strict: budget^.end < budget^.popped\n  ) {\n    batch(budget: budget);\n  }\n"
            ),
            "INV-1",
            "\n  name: strict\n",
        ),
        (
            "a-insufficient-bounds-summary.wf",
            replay_source!(
                " contract {\n  requires budget^.popped <= 8_u64;\n}",
                "  batch(budget: budget);\n  let window = array_filled::<u64, 8>(value: 0_u64);\n  set window[budget^.end] = 1_u64;\n"
            ),
            "OP-4",
            "\n  residual: budget^.end < window.len\n",
        ),
        (
            "a-insufficient-postcondition-summary.wf",
            replay_source!(
                " contract {\n  ensures budget^.end < budget^.popped;\n}",
                "  batch(budget: budget);\n"
            ),
            "FN-9",
            "\n  concrete_function: caller\n",
        ),
    ];
    for (name, caller, rule, target) in cases {
        for callee in [
            scanner_source!("  loop {\n"),
            scanner_source!("  loop (\n    invariant bounded: end <= budget^.popped\n  ) {\n"),
        ] {
            let failure = super::compile(
                &[
                    super::SourceInput::new(name, caller),
                    super::SourceInput::new("z-scanner.wf", callee),
                ],
                super::CompilerLimits::default(),
            )
            .expect_err("even the proved non-strict summary cannot establish this target");
            assert_eq!(failure.rule_id(), Some(*rule), "{name}: {failure}");
            assert_eq!(failure.location().expect("caller location").path(), *name);
            let rendered = failure.to_string();
            assert!(rendered.contains(target), "{name}: {rendered}");
            assert!(!rendered.contains("cannot supply its summary"), "{name}: {rendered}");
        }
    }
}

macro_rules! recursive_budget_source {
    ($goal:literal, $progress:literal) => {
        concat!(
            "fn caller(budget: &Budget) -> result: unit reads(budget.popped), writes(budget.end) {\n",
            "  batch(budget: budget);\n",
            "  invariant available: budget^.end ",
            $goal,
            " budget^.popped;\n",
            "  return unit;\n",
            "}\n\n",
            "struct Budget {\n  end: u64;\n  popped: u64;\n}\n\n",
            "fn batch(budget: &Budget) -> result: unit reads(budget.popped), writes(budget.end) contract {\n",
            "  ensures budget^.end <= budget^.popped;\n",
            "} {\n",
            "  let progress = ",
            $progress,
            ";\n",
            "  loop (\n    invariant bounded: 1_u64 <= progress\n  ) {\n    break;\n  }\n",
            "  caller(budget: budget);\n",
            "  set budget^.end = budget^.popped;\n",
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

#[test]
fn recursive_component_does_not_blame_its_induction_hypothesis() {
    let name = "same-component-summary.wf";
    for source in [
        recursive_budget_source!("<", "0_u64"),
        recursive_budget_source!("<", "1_u64"),
    ] {
        let failure = super::compile(
            &[super::SourceInput::new(name, source)],
            super::CompilerLimits::default(),
        )
        .expect_err("the recursive hypothesis is already present and does not prove a strict bound");
        assert_eq!(failure.rule_id(), Some("INV-1"), "{failure}");
        let rendered = failure.to_string();
        assert!(rendered.contains("\n  name: available\n"), "{rendered}");
        assert!(!rendered.contains("cannot supply its summary"), "{rendered}");
    }
    let accepted = recursive_budget_source!("<=", "1_u64");
    super::super::with_checked_program(
        &[super::SourceInput::new(name, accepted)],
        None,
        super::CompilerLimits::default(),
        |program, _| {
            let schedule = &program.data.postcondition_schedule;
            let components = program
                .data
                .functions
                .iter()
                .filter(|function| matches!(function.name.as_str(), "caller" | "batch"))
                .map(|function| schedule.function_components[function.id.0 as usize])
                .collect::<Vec<_>>();
            assert_eq!(components.len(), 2, "both recursive functions are present");
            assert_eq!(components[0], components[1], "the calls form one component");
            let contradictions =
                program.contradictory_successes(&super::declared_functions(accepted));
            assert!(contradictions.is_empty(), "{contradictions:?}");
            Ok(())
        },
    )
    .expect("the component hypothesis establishes the non-strict control target");
}

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
        name: "callee-withholds-two-needed-clauses.wf",
        rejected: budget_source!(
            "  ensures budget^.end <= budget^.popped;\n  ensures budget^.end >= budget^.popped;\n",
            "  loop {\n",
            "",
            "  requires budget^.end == budget^.popped;\n"
        ),
        rule: "FN-8",
        sentences: &[
            "\n  instantiated_goal: budget^.end == budget^.popped\n",
            "cannot supply its summary `ensures budget^.end <= budget^.popped; ensures budget^.end >= budget^.popped;`",
            "that callee's [FN-9] obligation `budget^.end - budget^.popped <= 0` declared at callee-withholds-two-needed-clauses.wf:13 fails at callee-withholds-two-needed-clauses.wf:24",
        ],
        repaired: &[budget_source!(
            "  ensures budget^.end <= budget^.popped;\n  ensures budget^.end >= budget^.popped;\n",
            "  loop (\n    invariant bounded: end <= budget^.popped\n  ) {\n",
            "",
            "  requires budget^.end == budget^.popped;\n"
        )],
    },
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
            "\n  mechanical_fix: the call to `batch` at callee-loop-withholds-reference-summary.wf:2 cannot supply its summary `ensures budget^.end <= budget^.popped;` because that callee's [FN-9] obligation `budget^.end - budget^.popped <= 0` declared at callee-loop-withholds-reference-summary.wf:13 fails at callee-loop-withholds-reference-summary.wf:23; repair that obligation in `batch` so it proves the summary needed by this proof; for a loop-computed bound, carry the needed bound in a loop-header `invariant`\n",
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
            "\n  mechanical_fix: the call to `callee::<T, 1>` at partly-concrete-callee-summary-unavailable.wf:4 cannot supply its summary `ensures result >= floor;` because that callee's [FN-9] obligation `0 - 0 <= -1` declared at partly-concrete-callee-summary-unavailable.wf:9 fails at partly-concrete-callee-summary-unavailable.wf:11; repair that obligation in `callee::<T, 1>` so it proves the summary needed by this proof; for a loop-computed bound, carry the needed bound in a loop-header `invariant`\n",
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
            "\n  mechanical_fix: the call to `callee::<T, 1>` at callee-header-failure-withholds-proved-summary.wf:4 cannot supply its summary `ensures result >= floor;` because that callee's [INV-1] obligation `1_u64 <= progress` fails at callee-header-failure-withholds-proved-summary.wf:13; repair that obligation in `callee::<T, 1>` so it proves the summary needed by this proof; for a loop-computed bound, carry the needed bound in a loop-header `invariant`\n",
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
            "the call to `batch` at reference-summary-survives-unrelated-write.wf:2 cannot supply its summary",
        ),
        (
            "only-second-call-can-supply-reference-summary.wf",
            budget_source!(
                "  ensures budget^.end <= budget^.popped;\n",
                "  loop {\n",
                "  set budget^.end = 1_u64;\n  batch(budget: budget);\n",
                "  requires budget^.end <= budget^.popped;\n"
            ),
            "the call to `batch` at only-second-call-can-supply-reference-summary.wf:4 cannot supply its summary",
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
    assert!(!rendered.contains("cannot supply its summary"), "{rendered}");
}
