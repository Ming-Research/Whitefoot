//! Eligibility diagnostics name the first forbidden part in declaration order.
use super::with_semantics;
use crate::{SemanticIssueKind, SemanticOutcome};

#[test]
fn frozen_eligibility_walks_nested_payloads_storage_and_cells_in_order() {
    for (declarations, content, first) in [
        (
            "struct Nested {\n  state: SharedRead<u8>;\n}\n\nstruct Holder {\n  earlier: Nested;\n  later: Shared<u8>;\n}\n\n",
            "Holder",
            ".earlier.state",
        ),
        (
            "enum Choice {\n  Empty();\n  Full(state: Shared<u8>);\n}\n\n",
            "Choice",
            ".Full.state",
        ),
        ("", "Option<Shared<u8>>", ".Some.value"),
        ("", "Box<SharedRead<u8>>", ".inner"),
        ("", "Array<Shared<u8>, 0>", "[element]"),
        (
            "struct HostPart {\n  meter: std::process::MemoryMeter;\n}\n\n",
            "HostPart",
            ".meter",
        ),
        ("", "std::time::CancelSource", ".state"),
    ] {
        let source = format!(
            "{declarations}fn denied(value: {content}) -> result: Frozen<{content}> pure {{\n  return frozen_new::<{content}>(value: move value);\n}}\n"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("frozen content {content} must reject: {outcome:?}");
            };
            assert_eq!(issue.rule_id(), "SHARE-1");
            let SemanticIssueKind::FrozenForbiddenPart {
                part,
                mechanical_fix,
            } = issue.kind()
            else {
                panic!("eligibility diagnostic: {issue:?}");
            };
            assert!(part.ends_with(first), "first forbidden part: {part}");
            assert!(mechanical_fix.contains(part));
            let coordinate = issue.location().coordinate();
            let start = usize::try_from(coordinate.start().value()).unwrap();
            let end = usize::try_from(coordinate.end().value()).unwrap();
            assert_eq!(
                &source[start..end],
                content,
                "the complete content targ owns the error"
            );
        });
    }
}

#[test]
fn nested_frozen_handles_admit_copy_reads_and_reading_references() {
    let source = b"fn peek(value: &Frozen<Frozen<Box<u8>>>) -> result: u8 reads(value) {\n  let part = &value^.inner.inner.inner;\n  return part^;\n}\n";
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

#[test]
fn frozen_content_consumes_follow_diag1_rule_definition_order() {
    // DIAG-1 selects OWN-1 (defined before SHARE-1) for copy moves and
    // bare affine uses. Only explicit non-copy consumes select SHARE-1.
    for (source, expected_kind) in [
        (
            "fn denied(value: Frozen<u8>) -> result: u8 pure {\n  return move value.inner;\n}\n",
            "MoveOfCopy",
        ),
        (
            "fn denied(value: Frozen<Box<Array<u8>>>) -> result: u64 pure {\n  return move value.inner.inner.len;\n}\n",
            "MoveOfCopy",
        ),
        (
            "fn denied(value: Frozen<Box<u8>>) -> result: Box<u8> pure {\n  return value.inner;\n}\n",
            "BareAffineUse",
        ),
        (
            "fn denied(value: Frozen<Option<Box<u8>>>) -> result: u8 pure {\n  match value.inner {\n    Some(value: part) => {\n      return part.inner;\n    }\n    None() => {\n      return 0_u8;\n    }\n  }\n}\n",
            "BareAffineUse",
        ),
        (
            "fn denied(value: Frozen<Box<u8>>) -> result: Box<u8> pure {\n  return move value.inner;\n}\n",
            "FrozenContentConsume",
        ),
        (
            "fn denied(value: Frozen<Option<Box<u8>>>) -> result: u8 pure {\n  match move value.inner {\n    Some(value: part) => {\n      return part.inner;\n    }\n    None() => {\n      return 0_u8;\n    }\n  }\n}\n",
            "FrozenContentConsume",
        ),
    ] {
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("{outcome:?}");
            };
            let expected_rule = if expected_kind == "FrozenContentConsume" {
                "SHARE-1"
            } else {
                "OWN-1"
            };
            assert_eq!(issue.rule_id(), expected_rule);
            match expected_kind {
                "MoveOfCopy" => {
                    assert!(matches!(issue.kind(), SemanticIssueKind::MoveOfCopy { .. }))
                }
                "BareAffineUse" => assert!(matches!(
                    issue.kind(),
                    SemanticIssueKind::BareAffineUse { .. }
                )),
                "FrozenContentConsume" => assert!(matches!(
                    issue.kind(),
                    SemanticIssueKind::FrozenContentConsume { .. }
                )),
                _ => unreachable!(),
            }
        });
    }
}

#[test]
fn frozen_storage_measure_reads_are_copy_observations() {
    let source = b"fn length(value: &Frozen<Box<Array<u8>>>) -> result: u64 reads(value) {\n  return value^.inner.inner.len;\n}\n";
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "{outcome:?}"
        );
    });
}

#[test]
fn frozen_and_owned_box_are_distinct_types() {
    let source =
        b"fn denied(value: Frozen<u8>) -> result: Box<u8> pure {\n  return move value;\n}\n";
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("{outcome:?}");
        };
        // A returned Frozen<u8> where the result is Box<u8> fails result
        // agreement, which FN-1 owns.
        assert_eq!(issue.rule_id(), "FN-1");
    });
}

#[test]
fn frozen_loop_alias_writes_and_moves_follow_diag1() {
    let write =
        include_str!("../../../../tests/conformance/cases/share1-neg-frozen-loop-alias-write.wf");
    // A write through the joined alias reaches a readonly path through
    // Frozen.inner [TYPE-2]; a move through `^` is OWN-1's move through a
    // reference, defined before SHARE-1 [DIAG-1].
    let moved = write
        .replace(
            "-> result: unit writes(wrapper)",
            "-> result: Box<u8> reads(wrapper)",
        )
        .replace(
            "  set cursor^.inner = 9_u8;\n  return unit;\n",
            "  return move cursor^;\n",
        );
    assert_ne!(moved, write);
    for (source, rule) in [(write, "TYPE-2"), (moved.as_str(), "OWN-1")] {
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("{outcome:?}");
            };
            assert_eq!(issue.rule_id(), rule);
        });
    }
}

#[test]
fn frozen_range_reads_use_the_same_version_and_refuse_an_unrelated_value() {
    for (source, expected) in [
        (
            include_bytes!("../../../../tests/conformance/cases/share1-pos-frozen-range-read.wf")
                .as_slice(),
            None,
        ),
        (
            include_bytes!(
                "../../../../tests/conformance/cases/share1-neg-frozen-range-wrong-value.wf"
            )
            .as_slice(),
            Some("RANGE-3"),
        ),
    ] {
        with_semantics(source, |outcome| match (outcome, expected) {
            (SemanticOutcome::Complete(_), None) => {}
            (SemanticOutcome::SourceIssue { issue }, Some(rule)) => {
                assert_eq!(issue.rule_id(), rule)
            }
            (outcome, _) => panic!("expected {expected:?}: {outcome:?}"),
        });
    }
}

#[test]
fn frozen_new_owes_its_argument_type_invariants() {
    let source = b"struct Positive {\n  byte: u8;\n  invariant positive(p): p.byte >= 1_u8;\n}\n\nfn freeze() -> result: Frozen<Positive> pure {\n  let value = Positive(byte: 1_u8);\n  set value.byte = 0_u8;\n  return frozen_new::<Positive>(value: value);\n}\n";
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("{outcome:?}");
        };
        // [TYPE-11]: a frozen_new argument owes its struct's invariants.
        assert_eq!(issue.rule_id(), "FN-8");
    });
}

#[test]
fn frozen_new_checks_its_written_content_argument_at_the_call() {
    let source = "fn denied(value: Shared<u8>) -> result: unit pure {\n  let held = frozen_new::<Shared<u8>>(value: move value);\n  return unit;\n}\n";
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(issue.rule_id(), "SHARE-1");
        assert!(matches!(
            issue.kind(),
            SemanticIssueKind::FrozenForbiddenPart { .. }
        ));
        let coordinate = issue.location().coordinate();
        let start = usize::try_from(coordinate.start().value()).unwrap();
        let end = usize::try_from(coordinate.end().value()).unwrap();
        assert_eq!(start, source.rfind("Shared<u8>").unwrap());
        assert_eq!(&source[start..end], "Shared<u8>");
    });
}

#[test]
fn frozen_type_mismatches_use_source_names_including_nested_boxes() {
    for (actual, setup, expected_name) in [
        ("Frozen<u8>", "", "Frozen<u8>"),
        (
            "Frozen<u8>",
            "  let nested = box_new::<Frozen<u8>>(value: move value);\n",
            "Box<Frozen<u8>>",
        ),
    ] {
        let operand = if setup.is_empty() { "value" } else { "nested" };
        let source = format!(
            "struct Holder {{\n  held: Box<u8>;\n}}\n\nfn denied(value: {actual}) -> result: Holder pure {{\n{setup}  let object = Holder(held: move {operand});\n  return move object;\n}}\n"
        );
        with_semantics(source.as_bytes(), |outcome| {
            let SemanticOutcome::SourceIssue { issue } = outcome else {
                panic!("{outcome:?}");
            };
            assert_eq!(issue.rule_id(), "TYPE-5");
            let SemanticIssueKind::TypeMismatch { expected, found } = issue.kind() else {
                panic!("{issue:?}");
            };
            assert_eq!(expected, "Box<u8>");
            assert_eq!(found, expected_name);
        });
    }
}

#[test]
fn prelude_opaque_handle_repairs_name_their_forming_functions() {
    for (ty, forming, fields) in [
        ("Frozen<u8>", "frozen_new", "inner: 7_u8"),
        ("Shared<u8>", "shared_new", ""),
        ("SharedRead<u8>", "shared_read", ""),
    ] {
        let name = ty.split('<').next().unwrap();
        for statement in [
            format!("  let made = {ty}({fields});\n  return move made;\n"),
            if fields.is_empty() {
                format!("  let {name}() = move value;\n  return unit;\n")
            } else {
                format!("  let {name}(inner: part) = move value;\n  return part;\n")
            },
        ] {
            let result = if statement.contains("let made") {
                ty
            } else if fields.is_empty() {
                "unit"
            } else {
                "u8"
            };
            let source =
                format!("fn denied(value: {ty}) -> result: {result} pure {{\n{statement}}}\n");
            with_semantics(source.as_bytes(), |outcome| {
                let SemanticOutcome::SourceIssue { issue } = outcome else {
                    panic!("{outcome:?}");
                };
                assert_eq!(issue.rule_id(), "TYPE-2");
                let SemanticIssueKind::ContainerConstruction { mechanical_fix, .. } = issue.kind()
                else {
                    panic!("{issue:?}");
                };
                assert!(mechanical_fix.contains(forming), "{mechanical_fix}");
                assert!(
                    !mechanical_fix.contains("remove `opaque`"),
                    "{mechanical_fix}"
                );
            });
        }
    }
}
