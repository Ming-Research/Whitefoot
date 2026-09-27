use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

use super::super::model::{
    CheckedExpression, CheckedStatement, CheckedType, CheckedValue, IntegerType,
};
use super::{assert_rule, assert_rule_kind, with_semantics};

const ENDPOINT_TERM_FIX: &str =
    "bind the computed u64 value with one preceding ordinary let and use that term as the endpoint";

/// [SET-1]'s closed writability relation, as the diagnostic names it. There is
/// no permission marker on a reference any more, so the required classes are a
/// live own-mode binding and a path below `deref` of a reference whose row
/// declares that write [REF-1, EFF-1].
const SET1_WRITABLE_ROOTS: &str = "a live own-mode value binding, or a path below deref of a \
                                   reference whose row declares that write";

fn assert_checks(source: &[u8]) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(_) = outcome else {
            panic!("counted range must check: {outcome:?}");
        };
    });
}

/// Asserts the rule one source establishes without pinning a payload the call
/// site does not state, for the judgments whose v0.59 issue kind was deleted
/// with its subject and whose v0.60 successor carries no payload of its own.
fn assert_only_rule(source: &[u8], rule: SemanticRule) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("expected {rule:?}, got {outcome:?}");
        };
        assert_eq!(issue.rule(), rule, "{issue:?}");
    });
}

#[test]
fn counted_range_retains_checked_inputs_binder_and_real_exhaustion() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  for @items (i in 2_u64..1_u64) {
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("break-free zero-trip counted range must check: {outcome:?}");
        };
        let CheckedStatement::CountedRange {
            node_path,
            binder,
            lower,
            upper,
            body,
            backedge_drops,
            ..
        } = &checked.data.functions[0].body.as_deref().expect("WF body")[0]
        else {
            panic!("counted source must retain its dedicated checked node");
        };
        assert!(!node_path.components().is_empty());
        assert_eq!(binder.0, 0);
        assert!(matches!(
            lower,
            CheckedExpression::Constant(CheckedValue::Integer {
                ty: IntegerType::U64,
                bits: 2
            })
        ));
        assert!(matches!(
            upper.as_ref(),
            CheckedExpression::Constant(CheckedValue::Integer {
                ty: IntegerType::U64,
                bits: 1
            })
        ));
        assert!(body.is_empty());
        assert!(backedge_drops.is_empty());
    });

    assert_checks(
        br#"fn main() -> status: std::process::ExitStatus pure {
  for @items (i in 18446744073709551614_u64..18446744073709551615_u64) {
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
}

#[test]
fn counted_endpoints_require_exact_own_u64_with_type7_exclusive() {
    assert_rule_kind(
        br#"fn main() -> status: std::process::ExitStatus pure {
  for @items (i in 0_u32..1_u64) {
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );

    assert_rule(
        br#"fn walk(start: &u64) -> result: unit pure {
  for @items (i in start..1_u64) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Type7,
        SemanticIssueKind::MissingDereference {
            mechanical_fix: "write `deref(holder)`",
        },
    );

    // The holder is a reference formed in the body rather than the retired
    // store cell. [TYPE-7] owns the same exclusive judgment there: the
    // endpoint names a reference where its referent `own u64` is required, so
    // the missing `deref` is cited at the same operand. A `Box` is no longer
    // a candidate here — its content is the field `inner` [TYPE-9] and
    // `deref` of a cell is itself a TYPE-7 rejection.
    assert_rule(
        br#"fn main() -> status: std::process::ExitStatus pure {
  let origin = 0_u64;
  let start = &origin;
  for @items (i in start..1_u64) {
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Type7,
        SemanticIssueKind::MissingDereference {
            mechanical_fix: "write `deref(holder)`",
        },
    );

    assert_rule(
        br#"fn main() -> status: std::process::ExitStatus pure {
  let origin = 0_u64;
  let start = &origin;
  loop @outer {
    for @items (i in start..1_u64) {
    }
    break @outer;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Type7,
        SemanticIssueKind::MissingDereference {
            mechanical_fix: "write `deref(holder)`",
        },
    );

    assert_checks(
        br#"fn walk(lower: &u64, upper: &u64) -> result: unit reads(lower), reads(upper) {
  for @items (i in deref(lower)..deref(upper)) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
}

#[test]
fn counted_endpoints_require_a_preceding_term_or_constant() {
    let subscript = br#"const bounds: Array<u64, 2> =[0_u64, 0_u64];

fn probe() -> result: unit pure {
  for @items (i in bounds[0_u64]..bounds[1_u64]) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        subscript,
        SemanticRule::Ent2,
        SemanticIssueKind::InvalidCountedEndpoint {
            mechanical_fix: ENDPOINT_TERM_FIX,
        },
    );
    super::assert_rule_at(subscript, SemanticRule::Ent2, "bounds[0_u64]");

    // [EFF-1] a by-value parameter has no effect entry at all, so the row
    // names only the reference endpoint. The v0.59 spelling declared
    // `reads(bounds.lower, upper)`: one entry names exactly one path, and an
    // own root is an EFF-1 rejection.
    assert_checks(
        br#"struct Bounds {
  lower: u64;
}

fn probe(bounds: Bounds, upper: &u64) -> result: unit reads(upper) {
  for @items (i in bounds.lower..deref(upper)) {
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
}

/// A client module of `pkg::records`, whose interface declares the readonly
/// fields these offset tests read: a source readonly field is written only on
/// a public field of an interface record, and it withholds writes only from
/// the modules that do not declare it [TYPE-2, MOD-6].
fn check_records_client(main: &[u8]) -> Result<(), crate::CompilationFailure> {
    super::check_module_sources(
        b"pkg::records: [];\npkg: [pkg::records, std::process];\n",
        &[
            (
                "records/module.wfm",
                b"public struct Entry {\n  public readonly width: u64;\n}\n\npublic struct Node {\n  public readonly count: u64;\n}\n"
                    .as_slice(),
            ),
            ("module.wfm", b"\n".as_slice()),
            ("main.wf", main),
        ],
    )
}

fn assert_unsupported_composite(result: Result<(), crate::CompilationFailure>) {
    let failure = result.expect_err("an unrepresented offset stops the check");
    assert_eq!(
        failure.kind(),
        crate::CompilationFailureKind::Unsupported,
        "{failure}"
    );
    assert!(failure.to_string().contains("CompositeValues"), "{failure}");
}

/// [ENT-2] clause (b): a readonly field below a subscript is an endpoint term
/// exactly when every offset in its place is itself a clause (a) or clause
/// (c) term.
///
/// A bare binding offset is represented and admitted. An array-element offset
/// is no term, so the place is no term and the endpoint is ENT-2's rejection.
/// A tracked field place is an admitted offset that this compiler captures no
/// value for, so the place is the compiler capability it is and never a
/// source verdict [DIAG-1]; the conformance corpus cannot pin that, because a
/// case declares the specification's verdict, which is acceptance.
#[test]
fn a_readonly_field_endpoint_follows_its_offset_forms() {
    let program = |offset: &str| {
        format!(
            r#"alias Entry = pkg::records::Entry;

struct Cursor {{
  at: u64;
}}

fn probe(entries: Array<Entry, 4>, slots: Array<u64, 2>, i: u64) -> result: u64 pure contract {{
  requires i < 4_u64;
}} {{
  let cursor = Cursor(at: 0_u64);
  let seen = 0_u64;
  for @items (c in 0_u64..entries[{offset}].width) {{
    set seen = seen +wrap 1_u64;
  }}
  return seen;
}}

fn main() -> status: std::process::ExitStatus pure {{
  return std::process::exit_status(code: 0_u8);
}}
"#
        )
    };
    check_records_client(program("i").as_bytes()).expect("a bare binding offset is admitted");
    let failure = check_records_client(program("slots[0_u64]").as_bytes())
        .expect_err("an element offset is no term");
    assert_eq!(failure.rule_id(), Some("ENT-2"), "{failure}");
    assert!(
        failure.to_string().contains("InvalidCountedEndpoint"),
        "{failure}"
    );
    assert_unsupported_composite(check_records_client(program("cursor.at").as_bytes()));
}

/// [ENT-2, DIAG-1] an admitted offset the compiler cannot capture stops the
/// read itself, in a body and in a contract clause alike, so no missing fact
/// can later surface as a source rejection. A measure over such a place is
/// the same capability.
#[test]
fn an_unrepresented_offset_is_unsupported_wherever_the_place_is_read() {
    let body = br#"alias Entry = pkg::records::Entry;

struct Cursor {
  at: u64;
}

fn probe(entries: Array<Entry, 4>, cursor: Cursor) -> result: u64 pure contract {
  requires cursor.at < 4_u64;
} {
  let width = entries[cursor.at].width;
  return width;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_unsupported_composite(check_records_client(body));
    let clause = br#"alias Node = pkg::records::Node;

struct Cursor {
  at: u64;
}

fn probe(nodes: &[Node], cursor: Cursor) -> result: u64 reads(nodes) contract {
  requires cursor.at < deref(nodes).len;
  requires deref(nodes)[cursor.at].count <= 8_u64;
} {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_unsupported_composite(check_records_client(clause));
    let measure = br#"struct Cursor {
  at: u64;
}

fn probe(grid: Array<Slots<u8, 4>, 4>, cursor: Cursor) -> result: u64 pure contract {
  requires cursor.at < 4_u64;
} {
  let width = grid[cursor.at].len;
  return width;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    super::assert_unsupported(measure, crate::UnsupportedSemanticFeature::CompositeValues);
}

/// [OWN-11] a counted binder may be copied and may have a reference formed to
/// it, but it is compiler-updated state: source may not write it [SET-1] and
/// may not pass it to a callee whose row declares a write of it [EFF-1].
///
/// v0.59's middle case refused `&uniq i` outright. The permission marker is
/// retired [REF-1], so forming a reference to the binder is admitted and the
/// rule's remaining restriction is the callee's declared write.
#[test]
fn counted_binder_is_not_source_writable_and_is_not_written_through() {
    assert_rule(
        br#"fn main() -> status: std::process::ExitStatus pure {
  for @items (i in 0_u64..1_u64) {
    set i = 1_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Set1,
        SemanticIssueKind::InvalidSetTarget {
            root_class: "compiler-updated counted binder".to_owned(),
            required_classes: SET1_WRITABLE_ROOTS,
        },
    );

    assert_checks(
        br#"fn observe(value: &u64) -> result: unit reads(value) {
  let seen = deref(value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  for @items (i in 0_u64..1_u64) {
    let copied = i;
    let shared = &i;
    observe(value: shared);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );

    assert_only_rule(
        br#"fn overwrite(target: &u64) -> result: unit writes(target) {
  set deref(target) = 9_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  for @items (i in 0_u64..1_u64) {
    overwrite(target: &i);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Own11,
    );
}

#[test]
fn a_counted_binders_reference_does_not_make_it_writable() {
    assert_only_rule(
        br#"fn main() -> status: std::process::ExitStatus pure {
  for (i in 0_u64..2_u64) {
    let held = &i;
    let aliased = held;
    set deref(aliased) = 9_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Set1,
    );

    // The first possible target is writable. The second is the counted
    // binder; checking only the first member must not authorize this call.
    assert_only_rule(
        br#"fn overwrite(target: &u64) -> result: unit writes(target) {
  set deref(target) = 9_u64;
  return unit;
}

fn examine(flag: Bool) -> result: unit pure {
  for (i in 0_u64..2_u64) {
    let spare = 0_u64;
    let held = if flag {
      give &spare;
    } else {
      give &i;
    }
    overwrite(target: held);
  }
  return unit;
}
"#,
        SemanticRule::Own11,
    );
}

#[test]
fn counted_body_inherits_own11_and_accepts_body_local_ownership() {
    assert_rule(
        br#"nocopy struct Token {
  value: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let token = Token(value: 1_u64);
  for @items (i in 0_u64..1_u64) {
    let consumed = move token;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Own11,
        SemanticIssueKind::MoveOuterBindingInLoop {
            binding: "token".to_owned(),
            mechanical_fix: "one iteration must leave every outer binding in the status the next \
                             one starts from: commit a value back into it before the backedge, or \
                             declare and consume it inside the body",
        },
    );

    // Retired with its subject: the second case asserted [OWN-11]'s
    // `BorrowRegionOutsideLoop`, a borrow created inside a loop naming a
    // region introduced outside it. Regions retire with no successor, and a
    // reference now dies with the scope of the local its path starts at
    // [REF-2], which is not a rule of its own about loops.

    assert_checks(
        br#"nocopy struct Token {
  value: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  for @items (i in 0_u64..1_u64) {
    let shared = &i;
    let token = Token(value: i);
    let consumed = move token;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
}

#[test]
fn counted_cleanup_is_attached_only_to_taken_body_exits() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  for @items (i in 0_u64..1_u64) {
    let values = box_new::<u64>(value: 1_u64);
    break @items;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("counted break cleanup must check: {outcome:?}");
        };
        let CheckedStatement::CountedRange {
            body,
            backedge_drops,
            ..
        } = &checked.data.functions[0].body.as_deref().expect("WF body")[0]
        else {
            panic!("expected counted range");
        };
        assert!(backedge_drops.is_empty());
        let CheckedStatement::Break { drops, .. } = &body[1] else {
            panic!("expected local counted break");
        };
        assert_eq!(drops.len(), 1);
        assert!(matches!(drops[0].ty, CheckedType::Nominal(_)));
    });
}

#[test]
fn counted_return_and_propagate_edges_reuse_exact_cleanup() {
    let source = br#"enum Fail {
  Bad();
}

fn source() -> result: Result<u64, Fail> pure {
  return Ok<u64, Fail>(value: 1_u64);
}

fn leave() -> result: unit pure {
  for @items (i in 0_u64..1_u64) {
    let values = box_new::<u64>(value: 1_u64);
    return unit;
  }
  return unit;
}

fn forward() -> result: Result<unit, Fail> pure {
  for @items (i in 0_u64..1_u64) {
    let values = box_new::<u64>(value: 1_u64);
    let value = propagate source();
  }
  return Ok<unit, Fail>(value: unit);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("counted return and propagation cleanup must check: {outcome:?}");
        };
        let leave = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "leave")
            .expect("leave must be checked");
        let CheckedStatement::CountedRange {
            body,
            backedge_drops,
            ..
        } = &leave.body.as_deref().expect("WF body")[0]
        else {
            panic!("leave must retain its counted range");
        };
        assert!(backedge_drops.is_empty());
        let CheckedStatement::Return { drops, .. } = &body[1] else {
            panic!("leave must retain its return edge");
        };
        assert_eq!(drops.len(), 1);
        assert!(matches!(drops[0].ty, CheckedType::Nominal(_)));

        let forward = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "forward")
            .expect("forward must be checked");
        let CheckedStatement::CountedRange {
            body,
            backedge_drops,
            ..
        } = &forward.body.as_deref().expect("WF body")[0]
        else {
            panic!("forward must retain its counted range");
        };
        assert_eq!(backedge_drops.len(), 1);
        assert!(matches!(backedge_drops[0].ty, CheckedType::Nominal(_)));
        let CheckedStatement::PropagateLet { error_drops, .. } = &body[1] else {
            panic!("forward must retain its propagation edge");
        };
        assert_eq!(error_drops.len(), 1);
        assert!(matches!(error_drops[0].ty, CheckedType::Nominal(_)));
        assert_eq!(error_drops[0].binding, backedge_drops[0].binding);
    });
}

#[test]
fn optional_labels_preserve_structural_break_targets_and_invariant_parentage() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  loop @outer {
    loop {
      break;
    }
    for (
      index in 0_u64..1_u64,
      invariant within_range: index <= 1_u64
    ) {
      break;
    }
    break @outer;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("optional loop labels must check through structural targets: {outcome:?}");
        };
        let CheckedStatement::Loop {
            id: outer_id,
            body: outer_body,
            ..
        } = &checked.data.functions[0].body.as_deref().expect("WF body")[0]
        else {
            panic!("expected the labeled outer loop");
        };
        let CheckedStatement::Loop {
            id: inner_id,
            body: inner_body,
            ..
        } = &outer_body[0]
        else {
            panic!("expected the unlabeled inner loop");
        };
        let CheckedStatement::Break {
            target: inner_target,
            ..
        } = &inner_body[0]
        else {
            panic!("expected the unlabeled inner break");
        };
        assert_eq!(inner_target, inner_id);

        let CheckedStatement::CountedRange {
            id: counted_id,
            invariants,
            body: counted_body,
            ..
        } = &outer_body[1]
        else {
            panic!("expected the unlabeled counted loop");
        };
        assert_eq!(invariants.len(), 1);
        assert_eq!(invariants[0].loop_id, *counted_id);
        let CheckedStatement::Break {
            target: counted_target,
            ..
        } = &counted_body[0]
        else {
            panic!("expected the unlabeled counted break");
        };
        assert_eq!(counted_target, counted_id);

        let CheckedStatement::Break {
            target: outer_target,
            ..
        } = &outer_body[2]
        else {
            panic!("expected the labeled cross-level break");
        };
        assert_eq!(outer_target, outer_id);
        assert_ne!(outer_target, inner_id);
        assert_ne!(outer_target, counted_id);
    });
}

#[test]
fn an_unlabeled_break_requires_an_enclosing_loop() {
    assert_rule(
        br#"fn main() -> status: std::process::ExitStatus pure {
  break;
}
"#,
        SemanticRule::Fn1,
        SemanticIssueKind::BreakOutsideLoop {
            mechanical_fix: "move `break;` inside a loop or remove it",
        },
    );
}
