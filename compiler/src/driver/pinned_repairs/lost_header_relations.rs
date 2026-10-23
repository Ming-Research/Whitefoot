//! Header relations withheld at a join: the rejected proof names the path
//! that must preserve the relation, and the paired contract does so.

use super::{CompilerLimits, RepairPair, SourceInput, compile};

const REJECTED: &[u8] = br#"fn touch(stack: &Box<Array<u64>>) -> r: unit writes(stack.inner) {
  if 0_u64 < stack^.inner.len {
    set stack^.inner[0_u64] = 1_u64;
  }
  return unit;
}

fn run(stack: &Box<Array<u64>>, fp: u64, c: u64) -> r: u64 writes(stack.inner) contract {
  requires fp + 4_u64 <= stack^.inner.len;
} {
  loop (
    invariant fb: fp + 4_u64 <= stack^.inner.len
  ) {
    if c != 0_u64 {
      touch(stack: stack);
    }
    set stack^.inner[fp] = 2_u64;
    if c == 0_u64 {
      break;
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

const REPAIRED: &[u8] = br#"fn touch(stack: &Box<Array<u64>>) -> r: unit writes(stack.inner) contract {
  ensures stack^.inner.len == entry(stack)^.inner.len;
} {
  if 0_u64 < stack^.inner.len {
    set stack^.inner[0_u64] = 1_u64;
  }
  return unit;
}

fn run(stack: &Box<Array<u64>>, fp: u64, c: u64) -> r: u64 writes(stack.inner) contract {
  requires fp + 4_u64 <= stack^.inner.len;
} {
  loop (
    invariant fb: fp + 4_u64 <= stack^.inner.len
  ) {
    if c != 0_u64 {
      touch(stack: stack);
    }
    set stack^.inner[fp] = 2_u64;
    if c == 0_u64 {
      break;
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

macro_rules! proof_source {
    ($helper_contract:literal, $ensures:literal, $operation:literal, $tail:literal) => {
        concat!(
            "fn touch(stack: &Box<Array<u64>>) -> r: unit writes(stack.inner)",
            $helper_contract,
            " {\n  if 0_u64 < stack^.inner.len {\n    set stack^.inner[0_u64] = 1_u64;\n  }\n  return unit;\n}\n\n",
            "fn run(stack: &Box<Array<u64>>, fp: u64, c: u64) -> r: u64 writes(stack.inner) contract {\n  requires fp + 4_u64 <= stack^.inner.len;",
            $ensures,
            "\n} {\n  loop (\n    invariant fb: fp + 4_u64 <= stack^.inner.len\n  ) {\n    if c != 0_u64 {\n      touch(stack: stack);\n    }\n",
            $operation,
            $tail,
            "  }\n  return 0_u64;\n}\n\n",
            "fn read_at(stack: &Box<Array<u64>>, fp: u64) -> r: u64 reads(stack.inner) contract {\n  requires fp < stack^.inner.len;\n} {\n  return stack^.inner[fp];\n}\n\n",
            "fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        )
        .as_bytes()
    };
}

pub(super) const LOST_HEADER_RELATIONS: &[RepairPair] = &[
    RepairPair {
        name: "bounds-after-lost-header-relation.wf",
        rejected: REJECTED,
        rule: "OP-4",
        sentences: &[
            "\n  residual: fp < stack^.inner.len\n",
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: header invariant `fb` (`(fp + 4_u64) <= stack^.inner.len`) was not kept at the join: the path through `touch` at line 15 did not prove `(fp + 4_u64) <= stack^.inner.len`; make that path preserve the relation: where `touch` preserves that measure, state `ensures stack^.inner.len == entry(stack)^.inner.len;` in its contract\n",
        ],
        repaired: &[REPAIRED],
    },
    RepairPair {
        name: "requirement-after-lost-header-relation.wf",
        rejected: proof_source!(
            "",
            "",
            "    let value = read_at(stack: stack, fp: fp);\n",
            "    if c == 0_u64 {\n      break;\n    }\n"
        ),
        rule: "FN-8",
        sentences: &[
            "\n  instantiated_goal: fp < stack^.inner.len\n",
            "header invariant `fb` (`(fp + 4_u64) <= stack^.inner.len`) was not kept at the join: the path through `touch` at line 15 did not prove `(fp + 4_u64) <= stack^.inner.len`",
            "state `ensures stack^.inner.len == entry(stack)^.inner.len;` in its contract\n",
        ],
        repaired: &[proof_source!(
            " contract {\n  ensures stack^.inner.len == entry(stack)^.inner.len;\n}",
            "",
            "    let value = read_at(stack: stack, fp: fp);\n",
            "    if c == 0_u64 {\n      break;\n    }\n"
        )],
    },
    RepairPair {
        name: "invariant-after-lost-header-relation.wf",
        rejected: proof_source!(
            "",
            "",
            "    invariant bound: fp < stack^.inner.len;\n",
            "    if c == 0_u64 {\n      break;\n    }\n"
        ),
        rule: "INV-1",
        sentences: &[
            "\n  name: bound\n",
            "header invariant `fb` (`(fp + 4_u64) <= stack^.inner.len`) was not kept at the join: the path through `touch` at line 15 did not prove `(fp + 4_u64) <= stack^.inner.len`",
            "state `ensures stack^.inner.len == entry(stack)^.inner.len;` in its contract\n",
        ],
        repaired: &[proof_source!(
            " contract {\n  ensures stack^.inner.len == entry(stack)^.inner.len;\n}",
            "",
            "    invariant bound: fp < stack^.inner.len;\n",
            "    if c == 0_u64 {\n      break;\n    }\n"
        )],
    },
    RepairPair {
        name: "postcondition-after-lost-header-relation.wf",
        rejected: proof_source!(
            "",
            "\n  ensures fp < stack^.inner.len;",
            "    break;\n",
            ""
        ),
        rule: "FN-9",
        sentences: &[
            "header invariant `fb` (`(fp + 4_u64) <= stack^.inner.len`) was not kept at the join: the path through `touch` at line 16 did not prove `(fp + 4_u64) <= stack^.inner.len`",
            "state `ensures stack^.inner.len == entry(stack)^.inner.len;` in its contract\n",
        ],
        repaired: &[proof_source!(
            " contract {\n  ensures stack^.inner.len == entry(stack)^.inner.len;\n}",
            "\n  ensures fp < stack^.inner.len;",
            "    break;\n",
            ""
        )],
    },
];

#[test]
fn exit_header_explanation_survives_only_while_its_support_does() {
    let source = proof_source!(
        "",
        "\n  ensures fp < stack^.inner.len;",
        "    break;\n",
        ""
    );
    for (name, before_return, explains) in [
        ("exit-unrelated-write.wf", "  set c = 0_u64;\n", true),
        ("exit-related-write.wf", "  touch(stack: stack);\n", false),
        (
            "exit-branch-write.wf",
            "  if c == 0_u64 {\n    touch(stack: stack);\n  }\n",
            false,
        ),
    ] {
        let source = std::str::from_utf8(source)
            .expect("source is text")
            .replace(
                "  return 0_u64;",
                &format!("{before_return}  return 0_u64;"),
            );
        let failure = compile(
            &[SourceInput::new(name, source.as_bytes())],
            CompilerLimits::default(),
        )
        .expect_err("the postcondition still lacks the lost length bound");
        assert_eq!(failure.rule_id(), Some("FN-9"), "{name}: {failure}");
        assert_eq!(
            failure.to_string().contains("header invariant `fb`"),
            explains,
            "{name}: {failure}",
        );
    }
}

#[test]
fn exit_header_explanation_requires_a_consistent_proof_of_the_goal() {
    let source = proof_source!(
        "",
        "\n  ensures c < stack^.inner.len;",
        "    break;\n",
        ""
    );
    for (name, before_return) in [
        ("exit-unrelated-goal.wf", ""),
        (
            "exit-conflicting-header.wf",
            "  if stack^.inner.len <= fp {\n    return 0_u64;\n  }\n",
        ),
    ] {
        let source = std::str::from_utf8(source)
            .expect("source is text")
            .replace(
                "  return 0_u64;",
                &format!("{before_return}  return 0_u64;"),
            );
        let failure = compile(
            &[SourceInput::new(name, source.as_bytes())],
            CompilerLimits::default(),
        )
        .expect_err("the header relation gives no bound on c");
        assert_eq!(failure.rule_id(), Some("FN-9"), "{name}: {failure}");
        assert!(
            !failure.to_string().contains("was not kept at the join"),
            "{name}: {failure}",
        );
    }
}

#[test]
fn unrelated_bound_does_not_blame_the_lost_header_relation() {
    let source = std::str::from_utf8(REJECTED)
        .expect("source is text")
        .replace("stack^.inner[fp]", "stack^.inner[c]");
    let failure = compile(
        &[SourceInput::new("unrelated-bound.wf", source.as_bytes())],
        CompilerLimits::default(),
    )
    .expect_err("the header relation gives no bound on c");
    assert_eq!(failure.rule_id(), Some("OP-4"));
    assert!(!failure.to_string().contains("was not kept at the join"));
}

#[test]
fn a_later_write_does_not_reuse_the_old_join_explanation() {
    let source = std::str::from_utf8(REJECTED)
        .expect("source is text")
        .replace(
            "    set stack^.inner[fp]",
            "    set fp = 0_u64;\n    set stack^.inner[fp]",
        );
    let failure = compile(
        &[SourceInput::new("write-after-join.wf", source.as_bytes())],
        CompilerLimits::default(),
    )
    .expect_err("the bound remains unproved after the later write");
    assert_eq!(failure.rule_id(), Some("OP-4"));
    assert!(!failure.to_string().contains("was not kept at the join"));
}

#[test]
fn an_unrelated_write_keeps_the_header_explanation() {
    let source = std::str::from_utf8(REJECTED)
        .expect("source is text")
        .replace(
            "    set stack^.inner[fp]",
            "    set c = 0_u64;\n    set stack^.inner[fp]",
        );
    let failure = compile(
        &[SourceInput::new("unrelated-write.wf", source.as_bytes())],
        CompilerLimits::default(),
    )
    .expect_err("an unrelated scalar write does not restore the missing bound");
    assert_eq!(failure.rule_id(), Some("OP-4"));
    assert!(failure.to_string().contains("header invariant `fb`"));
}

#[test]
fn a_conflicting_header_relation_does_not_explain_an_unrelated_goal() {
    let source = br#"fn run(rows: &Array<u64>, i: u64, x: u64, c: u64) -> r: unit reads(rows) contract {
  requires x <= 1_u64;
  requires i <= rows^.len;
} {
  loop (
    invariant low: x <= 1_u64
  ) {
    if c != 0_u64 {
      set x = 2_u64;
    } else {
      set x = 2_u64;
    }
    let value = rows^[i];
    if c == 0_u64 {
      break;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile(
        &[SourceInput::new("conflicting-header.wf", source)],
        CompilerLimits::default(),
    )
    .expect_err("the unavailable header cannot supply a contradictory premise");
    assert_eq!(failure.rule_id(), Some("OP-4"));
    assert!(!failure.to_string().contains("was not kept at the join"));
}
