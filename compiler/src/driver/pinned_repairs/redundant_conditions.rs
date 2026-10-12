//! [OP-5] repairs of a condition the fixed derivation decides: the test and
//! its dead branch are deleted and the selected branch is kept.

use super::RepairPair;

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
