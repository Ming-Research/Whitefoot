//! Writer-trial ownership and structural-edge repairs, applied as written.

use super::RepairPair;

const JOIN_REJECTED: &[u8] = br#"fn finish(pool: Box<u8>, flag: Bool) -> result: unit pure {
  if flag {
    let released_pool = move pool;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

const JOIN_REPAIRED: &[&[u8]] = &[
    br#"fn finish(pool: Box<u8>, flag: Bool) -> result: unit pure {
  if flag {
    let released_pool = move pool;
  } else {
    release::<Box<u8>>(value: move pool);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    br#"fn finish(pool: Box<u8>, flag: Bool) -> result: unit pure {
  if flag {
    let observed = pool.inner;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    br#"fn finish(pool: Box<u8>, flag: Bool) -> result: unit pure {
  if flag {
    let released_pool = move pool;
    set pool = move released_pool;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    br#"fn finish(pool: Box<u8>, flag: Bool) -> result: unit pure {
  release::<Box<u8>>(value: move pool);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
];

pub(super) const WRITER_OWNERSHIP: &[RepairPair] = &[
    RepairPair {
        name: "writer-reference-copy-needs-no-move.wf",
        rejected: br#"fn read(p: &u8) -> result: u8 reads(p) {
  let q = move p;
  return q^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OWN-1",
        sentences: &[
            "\n  ty: &u8\n",
            "\n  reason: `p` has type &u8; every part is copy\n",
            "\n  mechanical_fix: write `p` without `move`\n",
        ],
        repaired: &[br#"fn read(p: &u8) -> result: u8 reads(p) {
  let q = p;
  return q^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-noncopy-const-cannot-move.wf",
        rejected: br#"nocopy struct Token {
  id: u32;
}

const token: Token = Token(id: 1_u32);

fn read_whole() -> result: u32 pure {
  let t = move token;
  return t.id;
}
"#,
        rule: "OWN-1",
        sentences: &[
            "]: MoveOfCopy\n",
            "\n  ty: Token\n",
            "this constant is not owned storage and cannot be consumed",
            "\n  mechanical_fix: read the const through a field, subscript or measure supported by its type, or take a `&` reference\n",
        ],
        repaired: &[
            br#"nocopy struct Token {
  id: u32;
}

const token: Token = Token(id: 1_u32);

fn read_whole() -> result: u32 pure {
  let t = token.id;
  return t;
}
"#,
            br#"nocopy struct Token {
  id: u32;
}

const token: Token = Token(id: 1_u32);

fn read_whole() -> result: u32 pure {
  let t = &token;
  return t^.id;
}
"#,
        ],
    },
    RepairPair {
        name: "writer-affine-result-needs-move.wf",
        rejected: br#"fn keep(p: Result<Box<u8>, unit>) -> result: Result<Box<u8>, unit> pure {
  return p;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OWN-1",
        sentences: &[
            "\n  binding: p\n",
            "\n  ty: Result<Box<u8>, unit>\n",
            "\n  reason: `p` has type Result<Box<u8>, unit>; its Ok payload `value` has non-copy type Box<u8>\n",
            "\n  mechanical_fix: write `move p`\n",
        ],
        repaired: &[br#"fn keep(p: Result<Box<u8>, unit>) -> result: Result<Box<u8>, unit> pure {
  return move p;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-copy-result-needs-no-move.wf",
        rejected: br#"fn keep(p: Result<u8, unit>) -> result: Result<u8, unit> pure {
  return move p;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OWN-1",
        sentences: &[
            "\n  binding: p\n",
            "\n  ty: Result<u8, unit>\n",
            "\n  reason: `p` has type Result<u8, unit>; every part is copy\n",
            "\n  mechanical_fix: write `p` without `move`\n",
        ],
        repaired: &[br#"fn keep(p: Result<u8, unit>) -> result: Result<u8, unit> pure {
  return p;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-first-noncopy-field-declaration-order.wf",
        rejected: br#"struct Parcel<T> {
  stamp: u8;
  zebra: T;
  alpha: Box<u16>;
}

fn keep(p: Parcel<Box<u8>>) -> result: Parcel<Box<u8>> pure {
  return p;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OWN-1",
        sentences: &[
            "\n  ty: Parcel<Box<u8>>\n",
            "its `zebra` field has non-copy type Box<u8>",
            "\n  mechanical_fix: write `move p`\n",
        ],
        repaired: &[br#"struct Parcel<T> {
  stamp: u8;
  zebra: T;
  alpha: Box<u16>;
}

fn keep(p: Parcel<Box<u8>>) -> result: Parcel<Box<u8>> pure {
  return move p;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "writer-implicit-false-edge-retained-for-undecided-condition.wf",
        rejected: JOIN_REJECTED,
        rule: "LIV-1",
        sentences: &[
            "\n  live_predecessor: the implicit false edge of this `if` (no `else` is written)\n",
            "\n  dead_predecessor: the `if` branch\n",
            "consume it on every predecessor, on none, or commit a value back into it",
            "write `release::<Box<u8>>(value: move pool);` before the code that needs it released and remove the conditional consume",
        ],
        repaired: JOIN_REPAIRED,
    },
];

#[test]
fn an_implicit_false_edge_is_structural_and_only_droppable_owners_get_release() {
    use super::{CompilerLimits, SourceInput, compile};

    let failure = compile(
        &[SourceInput::from_host_path(
            "variable-edge.wf",
            "variable-edge.wf",
            JOIN_REJECTED,
        )],
        CompilerLimits::default(),
    )
    .expect_err("the structural false edge retains the owner");
    assert_eq!(failure.rule_id(), Some("LIV-1"));
    let rendered = failure.to_string();
    assert!(rendered.contains("the implicit false edge of this `if` (no `else` is written)"));
    assert!(!rendered.contains("constant conditions do not remove"));

    let source = br#"nodrop struct Ticket {
  code: u8;
}

fn finish(ticket: Ticket, flag: Bool) -> result: unit pure {
  if flag {
    let Ticket(code: code) = move ticket;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile(
        &[SourceInput::from_host_path(
            "linear-edge.wf",
            "linear-edge.wf",
            source,
        )],
        CompilerLimits::default(),
    )
    .expect_err("the structural false edge retains the linear value");
    assert_eq!(failure.rule_id(), Some("LIV-1"));
    assert!(!failure.to_string().contains("release::<"));
}

/// The structural join is checked before source-condition redundancy. Both
/// literals still carry LIV-1's constant-edge sentence on an unbalanced join;
/// restoring the owner makes that join valid and exposes OP-5 instead.
#[test]
fn a_constant_unbalanced_join_precedes_condition_redundancy() {
    use super::{CompilerLimits, SourceInput, compile};

    for (condition, decided) in [("True()", "True"), ("False()", "False")] {
        let source = std::str::from_utf8(JOIN_REJECTED)
            .expect("the fixture is UTF-8")
            .replace("if flag", &format!("if {condition}"));
        let failure = compile(
            &[SourceInput::new("constant-edge.wf", source.as_bytes())],
            CompilerLimits::default(),
        )
        .expect_err("the structural join retains the owner on only one edge");
        assert_eq!(failure.rule_id(), Some("LIV-1"), "{failure}");
        assert!(failure.to_string().contains(
            "\n  live_predecessor: the implicit false edge of this `if` (no `else` is written); constant conditions do not remove this structural edge [FN-1]\n"
        ));

        let source = std::str::from_utf8(JOIN_REPAIRED[2])
            .expect("the fixture is UTF-8")
            .replace("if flag", &format!("if {condition}"));
        let failure = compile(
            &[SourceInput::new("constant-edge.wf", source.as_bytes())],
            CompilerLimits::default(),
        )
        .expect_err("a balanced join exposes the redundant constant condition");
        assert_eq!(failure.rule_id(), Some("OP-5"), "{failure}");
        let rendered = failure.to_string();
        assert!(rendered.contains("]: RedundantCondition\n"), "{rendered}");
        assert!(
            rendered.contains(&format!("\n  decided: {decided}\n")),
            "{rendered}"
        );
    }
}

/// OP-5 admits constants in a contradictory entering state. Structural
/// ownership still requires agreement at the join: contradiction does not
/// remove the implicit false edge [FN-1]. The restored-owner control proves
/// the constant condition itself is admitted in this state.
#[test]
fn contradictory_conditions_do_not_remove_structural_ownership_edges() {
    use super::{CompilerLimits, SourceInput, compile};

    for condition in ["True()", "False()"] {
        for (source, restored) in [(JOIN_REJECTED, false), (JOIN_REPAIRED[2], true)] {
            let source = std::str::from_utf8(source)
                .expect("the fixture is UTF-8")
                .replace("flag: Bool)", "flag: Bool, n: u8)")
                .replace(
                    "-> result: unit pure {",
                    "-> result: unit pure contract {\n  requires n < 0_u8;\n} {",
                )
                .replace("if flag", &format!("if {condition}"));
            let outcome = compile(
                &[SourceInput::new("contradictory-edge.wf", source.as_bytes())],
                CompilerLimits::default(),
            );
            if restored {
                outcome.expect("OP-5 admits the constant in a contradictory entering state");
            } else {
                let failure = outcome.expect_err("structural ownership still judges both edges");
                assert_eq!(failure.rule_id(), Some("LIV-1"), "{failure}");
                assert!(failure.to_string().contains(
                    "\n  live_predecessor: the implicit false edge of this `if` (no `else` is written); constant conditions do not remove this structural edge [FN-1]\n"
                ));
            }
        }
    }
}
