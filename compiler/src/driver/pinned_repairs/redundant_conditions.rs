//! [OP-5] repairs of a condition the fixed derivation decides: the test and
//! its dead branch are deleted and the selected branch is kept.

use super::harness::contradictory_successes;
use super::{CompilationFailureKind, CompilerLimits, RepairPair, compile};
use crate::SourceInput;

pub(super) const REDUNDANT_CONDITIONS: &[RepairPair] = &[
    // Decided True: the then branch stays and the continuation it leaves
    // unreachable goes with the test.
    RepairPair {
        name: "op5-neg-redundant-proved.wf",
        rejected: include_bytes!("../../../../tests/conformance/cases/op5-neg-redundant-proved.wf"),
        rule: "OP-5",
        sentences: &[
            "]: RedundantCondition\n",
            "\n  decided: True\n",
            "\n  mechanical_fix: the condition always holds here: delete the test and its else branch, keep the then branch's statements and delivery in place, and delete any statement that leaves unreachable",
        ],
        repaired: &[br#"fn choose(x: u64) -> r: u64 pure contract {
  requires x < 8_u64;
} {
  doc "The requirement proves the condition.";
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "The checker judges every body.";
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // Decided False without an else: the whole statement goes.
    RepairPair {
        name: "op5-neg-redundant-refuted.wf",
        rejected: include_bytes!("../../../../tests/conformance/cases/op5-neg-redundant-refuted.wf"),
        rule: "OP-5",
        sentences: &[
            "]: RedundantCondition\n",
            "\n  decided: False\n",
            "\n  mechanical_fix: the condition never holds here: delete the test and its then branch and keep the else branch's statements and delivery in place, or delete the whole statement when it has no else",
        ],
        repaired: &[br#"fn choose(x: u64) -> r: u64 pure contract {
  requires 8_u64 <= x;
} {
  doc "The requirement refutes the condition.";
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "The checker judges every body.";
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];

/// Guarding a refuted goal is no repair [DIAG-1]. Where the guard's entering
/// state is noncontradictory, the guard is a condition the derivation decides
/// False, which [OP-5] rejects as a redundant source form. Where that state
/// is contradictory, [OP-5] admits the guard, and the goal inside it succeeds
/// only because the state is contradictory: the pair test's second condition
/// is live and reports it.
#[test]
fn a_guard_around_a_refuted_goal_succeeds_only_in_a_contradictory_state() {
    let guarded = br#"fn main() -> status: std::process::ExitStatus pure {
  let x = 255_u8;
  if x +defined 1_u8 {
    let y = x + 1_u8;
    return std::process::exit_status(code: y);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile(
        &[SourceInput::new("guarded-refuted-goal.wf", guarded)],
        CompilerLimits::default(),
    )
    .expect_err("a guard decided False in a noncontradictory state is redundant");
    assert_eq!(failure.kind(), CompilationFailureKind::Source, "{failure}");
    assert_eq!(failure.rule_id(), Some("OP-5"), "{failure}");
    let rendered = format!("{failure}\n");
    for sentence in ["]: RedundantCondition\n", "\n  decided: False\n"] {
        assert!(rendered.contains(sentence), "{rendered}");
    }
    let uninhabited = br#"fn bump(x: u8) -> r: u8 pure contract {
  requires x < 0_u8;
} {
  if x +defined 1_u8 {
    let y = x + 1_u8;
    return y;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let contradictions = contradictory_successes("guarded-refuted-goal.wf", uninhabited)
        .expect("a guard in a contradictory entering state is accepted");
    assert!(
        !contradictions.is_empty(),
        "the operation inside the contradictory state must be discharged by contradiction"
    );
}
