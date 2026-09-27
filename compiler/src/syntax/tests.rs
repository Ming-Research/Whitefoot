#![allow(clippy::panic)]

use crate::lexer::{LexLimits, LexOutcome, LexedBundle, lex};
use crate::syntax::terminal::{ALL_FIXED_TERMINALS, FixedTerminal, TerminalPredicate};
use crate::{ACTIVE_KERNEL_SPEC_HASH, SourceBundle, SourceId, SourceInput, SourceLimits, SpecHash};

use crate::{
    TerminalInvocationFailure, TerminalIssueOwner, TerminalLimit, TerminalLimits, TerminalOutcome,
    TerminalResourceFailure, classify_terminals,
};

const SOURCE_LIMITS: SourceLimits = SourceLimits {
    max_sources: 8,
    max_logical_path_bytes: 64,
    max_source_bytes: 16_384,
    max_total_source_bytes: 32_768,
    max_binding_bytes: 65_536,
};

const LEX_LIMITS: LexLimits = LexLimits {
    max_sources: 8,
    max_source_bytes: 16_384,
    max_total_source_bytes: 32_768,
    max_token_bytes: 1_024,
    max_tokens: 4_096,
    max_lexemes: 8_192,
};

fn source_bundle(inputs: &[SourceInput<'_>]) -> Result<SourceBundle, String> {
    SourceBundle::with_limits(inputs, SOURCE_LIMITS).map_err(|error| format!("{error:?}"))
}

fn lexed(bundle: &SourceBundle) -> Result<LexedBundle, String> {
    match lex(bundle, LEX_LIMITS) {
        LexOutcome::Complete(lexed) => Ok(lexed),
        other => Err(format!("{other:?}")),
    }
}

#[test]
fn every_fixed_predicate_is_retained_without_identifier_priority() {
    let mut source = Vec::new();
    for (index, terminal) in ALL_FIXED_TERMINALS.iter().enumerate() {
        if index != 0 {
            source.push(b' ');
        }
        source.extend_from_slice(terminal.spelling_bytes());
    }
    let inputs = [SourceInput::new("fixed.wf", &source)];
    let Ok(bundle) = source_bundle(&inputs) else {
        panic!("test source bundle must be constructible");
    };
    let Ok(lexed) = lexed(&bundle) else {
        panic!("fixed terminal inventory must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 128 },
    ) else {
        panic!("fixed terminal inventory must classify");
    };

    assert_eq!(classified.tokens().len(), ALL_FIXED_TERMINALS.len());
    for (actual, expected) in classified.tokens().iter().zip(ALL_FIXED_TERMINALS) {
        assert!(
            actual
                .terminals()
                .contains(TerminalPredicate::Fixed(expected))
        );
        assert!(!actual.terminals().contains(TerminalPredicate::Identifier));
        let expected_count = if expected == FixedTerminal::Unit {
            2
        } else {
            1
        };
        assert_eq!(actual.terminals().len(), expected_count);
    }
}

#[test]
fn every_external_shape_is_classified_context_free() {
    let source = b"name Type @label iadd.checked 42 1_i32 1.00_f64 0_T \"text\"";
    let inputs = [SourceInput::new("external.wf", source)];
    let Ok(bundle) = source_bundle(&inputs) else {
        panic!("test source bundle must be constructible");
    };
    let Ok(lexed) = lexed(&bundle) else {
        panic!("external predicates must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 9 },
    ) else {
        panic!("external predicates must classify");
    };
    let expected = [
        TerminalPredicate::Identifier,
        TerminalPredicate::TypeIdentifier,
        TerminalPredicate::Label,
        TerminalPredicate::OperationName,
        TerminalPredicate::Digits,
        TerminalPredicate::Literal,
        TerminalPredicate::Literal,
        TerminalPredicate::Literal,
        TerminalPredicate::String,
    ];
    assert_eq!(classified.tokens().len(), expected.len());
    for (token, predicate) in classified.tokens().iter().zip(expected) {
        assert_eq!(token.terminals().len(), 1);
        assert!(token.terminals().contains(predicate));
    }
}

#[test]
fn retired_statement_spellings_are_ordinary_identifiers() {
    // v0.60 releases eleven more spellings back to IDENT: the region and
    // permission markers, the two retired statements, the retired effect
    // category, and the five lowercase shape atoms [GRAM-1, GRAM-3, TYPE-9].
    for spelling in [
        b"check".as_slice(),
        b"trap",
        b"prove",
        b"region",
        b"uniq",
        b"dispose",
        b"replace",
        b"allocates",
        b"array",
        b"box",
        b"arena",
        b"buffer",
        b"formal",
        b"actual",
        b"linear",
        b"affine",
    ] {
        let inputs = [SourceInput::new("identifier.wf", spelling)];
        let bundle = source_bundle(&inputs).expect("identifier source must be constructible");
        let lexed = lexed(&bundle).expect("identifier must remain one lower-word token");
        let TerminalOutcome::Complete(classified) = classify_terminals(
            &lexed,
            ACTIVE_KERNEL_SPEC_HASH,
            TerminalLimits { max_tokens: 1 },
        ) else {
            panic!("retired statement spelling must classify as an ordinary identifier");
        };
        assert_eq!(classified.tokens().len(), 1);
        assert!(
            classified.tokens()[0]
                .terminals()
                .contains(TerminalPredicate::Identifier)
        );
        assert_eq!(classified.tokens()[0].terminals().len(), 1);
    }
}

/// `program` and `no_heap` leave IDENT, and `Slice` and `MutSlice` join TYPEID.
///
/// This is the other half of the retirement sweep above and the one that can
/// silently break a source: v0.60's [GRAM-2] makes `program` and `no_heap`
/// fixed atoms, so a binding or parameter that used either spelling no longer
/// lexes as a name. In the other direction the capitalized view atoms retired
/// with the view nominals, so `Slice` is an ordinary TYPEID again [GRAM-3].
#[test]
fn the_heap_declaration_atoms_leave_ident_and_the_view_atoms_rejoin_typeid() {
    for (spelling, expected) in [
        (
            b"program".as_slice(),
            TerminalPredicate::Fixed(FixedTerminal::Program),
        ),
        (b"no_heap", TerminalPredicate::Fixed(FixedTerminal::NoHeap)),
        (b"Slice", TerminalPredicate::TypeIdentifier),
        (b"MutSlice", TerminalPredicate::TypeIdentifier),
    ] {
        let inputs = [SourceInput::new("atoms.wf", spelling)];
        let bundle = source_bundle(&inputs).expect("atom source must be constructible");
        let lexed = lexed(&bundle).expect("atom must remain one word token");
        let TerminalOutcome::Complete(classified) = classify_terminals(
            &lexed,
            ACTIVE_KERNEL_SPEC_HASH,
            TerminalLimits { max_tokens: 1 },
        ) else {
            panic!("atom must classify: {spelling:?}");
        };
        let set = classified.tokens()[0].terminals();
        assert_eq!(set.len(), 1, "spelling: {spelling:?}");
        assert!(set.contains(expected), "spelling: {spelling:?}");
    }
}

#[test]
fn unit_retains_fixed_and_literal_memberships() {
    let inputs = [SourceInput::new("unit.wf", b"unit")];
    let Ok(bundle) = source_bundle(&inputs) else {
        panic!("test source bundle must be constructible");
    };
    let Ok(lexed) = lexed(&bundle) else {
        panic!("unit must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 1 },
    ) else {
        panic!("unit must classify");
    };
    let set = classified.tokens()[0].terminals();
    assert_eq!(set.len(), 2);
    assert!(set.contains(TerminalPredicate::Fixed(FixedTerminal::Unit)));
    assert!(set.contains(TerminalPredicate::Literal));
    assert!(!set.contains(TerminalPredicate::Identifier));
}

#[test]
fn operator_suffix_membership_admits_exactly_the_infix_op_list() {
    for spelling in [
        b"+".as_slice(),
        b"+wrap",
        b"+checked",
        b"+sat",
        b"-",
        b"-wrap",
        b"-checked",
        b"-sat",
        b"*",
        b"*wrap",
        b"*checked",
        b"*sat",
        b"/",
        b"/checked",
        b"%",
        b"%checked",
    ] {
        let inputs = [SourceInput::new("operator.wf", spelling)];
        let Ok(bundle) = source_bundle(&inputs) else {
            panic!("test source bundle must be constructible");
        };
        let Ok(lexed) = lexed(&bundle) else {
            panic!("operator form must lex");
        };
        let TerminalOutcome::Complete(_) = classify_terminals(
            &lexed,
            ACTIVE_KERNEL_SPEC_HASH,
            TerminalLimits { max_tokens: 1 },
        ) else {
            panic!("{spelling:?} is an infix_op row and must pass membership");
        };
    }

    // A suffix outside the closed list forms one token and is rejected by
    // membership, not by formation, and the whole operator is the span.
    // `/sat` and `%sat` are the near misses: the mode word is admissible on
    // other carriers but no division or remainder row carries it.
    for spelling in [
        b"+strict".as_slice(),
        b"+trap",
        b"-foo",
        b"*x",
        b"/sat",
        b"%sat",
        b"/wrap",
    ] {
        let inputs = [SourceInput::new("operator.wf", spelling)];
        let Ok(bundle) = source_bundle(&inputs) else {
            panic!("test source bundle must be constructible");
        };
        let Ok(lexed) = lexed(&bundle) else {
            panic!("a bad operator suffix must still form one token");
        };
        let TerminalOutcome::SourceIssue(issue) = classify_terminals(
            &lexed,
            ACTIVE_KERNEL_SPEC_HASH,
            TerminalLimits { max_tokens: 1 },
        ) else {
            panic!("{spelling:?} must fail terminal membership");
        };
        assert_eq!(issue.owner(), TerminalIssueOwner::Gram1);
        assert_eq!(issue.token().start().value(), 0);
        assert_eq!(
            issue.token().end().value(),
            u64::try_from(spelling.len()).unwrap_or(u64::MAX)
        );
    }
}

#[test]
fn malformed_numeric_membership_stops_at_first_source_then_byte() {
    let inputs = [
        SourceInput::new("first.wf", b"x 1e+ 2_T"),
        SourceInput::new("second.wf", b"01.0_f32"),
    ];
    let Ok(bundle) = source_bundle(&inputs) else {
        panic!("test source bundle must be constructible");
    };
    let Ok(lexed) = lexed(&bundle) else {
        panic!("broad numeric candidates must lex");
    };
    let TerminalOutcome::SourceIssue(issue) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 4 },
    ) else {
        panic!("the first invalid numeric candidate must be rejected");
    };
    assert_eq!(issue.owner(), TerminalIssueOwner::Form5);
    assert_eq!(issue.token().source(), SourceId::from_ordinal(0));
    assert_eq!(issue.token().start().value(), 2);
    assert_eq!(issue.token().end().value(), 5);
}

#[test]
fn malformed_numeric_language_is_rejected_without_rescanning() {
    for spelling in [
        b"2_T".as_slice(),
        b"1e+",
        b"01.0_f32",
        b"1.0e01_f32",
        b"1.0E2_f32",
        b"1._f32",
        b"1_f32",
        b"1_i128",
    ] {
        let inputs = [SourceInput::new("number.wf", spelling)];
        let Ok(bundle) = source_bundle(&inputs) else {
            panic!("test source bundle must be constructible");
        };
        let Ok(lexed) = lexed(&bundle) else {
            panic!("broad numeric candidate must remain one formed token");
        };
        let TerminalOutcome::SourceIssue(issue) = classify_terminals(
            &lexed,
            ACTIVE_KERNEL_SPEC_HASH,
            TerminalLimits { max_tokens: 1 },
        ) else {
            panic!("malformed numeric spelling must fail terminal membership");
        };
        assert_eq!(issue.owner(), TerminalIssueOwner::Form5);
        assert_eq!(issue.token().start().value(), 0);
        assert_eq!(
            issue.token().end().value(),
            u64::try_from(spelling.len()).unwrap_or(u64::MAX)
        );
    }
}

#[test]
fn form7_only_numeric_defects_remain_literal_members() {
    let source = b"00_i8 -0_i64 999999999999999999999_u8 1.00_f64 0.000_f32 1.5e-0_f64";
    let inputs = [SourceInput::new("number.wf", source)];
    let Ok(bundle) = source_bundle(&inputs) else {
        panic!("test source bundle must be constructible");
    };
    let Ok(lexed) = lexed(&bundle) else {
        panic!("FORM-7-only numeric cases must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 6 },
    ) else {
        panic!("FORM-7-only defects must survive terminal membership");
    };
    assert_eq!(classified.tokens().len(), 6);
    for token in classified.tokens() {
        assert_eq!(token.terminals().len(), 1);
        assert!(token.terminals().contains(TerminalPredicate::Literal));
    }
}

#[test]
fn source_boundaries_and_empty_partitions_survive_classification() {
    let inputs = [
        SourceInput::new("empty.wf", b""),
        SourceInput::new("one.wf", b"x"),
        SourceInput::new("two.wf", b"Type unit"),
    ];
    let Ok(bundle) = source_bundle(&inputs) else {
        panic!("test source bundle must be constructible");
    };
    let Ok(lexed) = lexed(&bundle) else {
        panic!("source partitions must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 3 },
    ) else {
        panic!("source partitions must classify");
    };
    assert!(
        classified
            .source_tokens(SourceId::from_ordinal(0))
            .is_some_and(<[_]>::is_empty)
    );
    assert_eq!(
        classified
            .source_tokens(SourceId::from_ordinal(1))
            .map(<[_]>::len),
        Some(1)
    );
    assert_eq!(
        classified
            .source_tokens(SourceId::from_ordinal(2))
            .map(<[_]>::len),
        Some(2)
    );
    assert!(
        classified
            .source_tokens(SourceId::from_ordinal(3))
            .is_none()
    );
    assert_eq!(classified.source_bundle(), &bundle);
}

#[test]
fn specification_identity_fails_before_classification() {
    let inputs = [SourceInput::new("main.wf", b"1e+")];
    let Ok(bundle) = source_bundle(&inputs) else {
        panic!("test source bundle must be constructible");
    };
    let Ok(lexed) = lexed(&bundle) else {
        panic!("broad numeric candidate must lex");
    };
    let wrong = SpecHash::from_sha256([0x5a; 32]);
    let TerminalOutcome::InvocationFailure(TerminalInvocationFailure::SpecificationMismatch {
        expected,
        actual,
    }) = classify_terminals(&lexed, wrong, TerminalLimits { max_tokens: 0 })
    else {
        panic!("specification mismatch must win");
    };
    assert_eq!(expected, ACTIVE_KERNEL_SPEC_HASH);
    assert_eq!(actual, wrong);
}

#[test]
fn token_limit_is_inclusive_and_precedes_membership_work() {
    let inputs = [SourceInput::new("main.wf", b"x 1e+")];
    let Ok(bundle) = source_bundle(&inputs) else {
        panic!("test source bundle must be constructible");
    };
    let Ok(lexed) = lexed(&bundle) else {
        panic!("broad numeric candidate must lex");
    };
    let TerminalOutcome::ResourceFailure(TerminalResourceFailure::LimitExceeded {
        limit,
        maximum,
        actual,
    }) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 1 },
    )
    else {
        panic!("resource limit must win before membership");
    };
    assert_eq!(limit, TerminalLimit::Tokens);
    assert_eq!(maximum, 1);
    assert_eq!(actual, 2);
}

#[test]
fn repeated_classification_is_deterministic() {
    let inputs = [SourceInput::new("main.wf", b"deref(x) unit 42")];
    let Ok(bundle) = source_bundle(&inputs) else {
        panic!("test source bundle must be constructible");
    };
    let Ok(lexed) = lexed(&bundle) else {
        panic!("test source must lex");
    };
    let first = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 7 },
    );
    let second = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 7 },
    );
    let (TerminalOutcome::Complete(first), TerminalOutcome::Complete(second)) = (first, second)
    else {
        panic!("both classifications must complete");
    };
    let first_sets: Vec<_> = first
        .tokens()
        .iter()
        .map(|token| token.terminals())
        .collect();
    let second_sets: Vec<_> = second
        .tokens()
        .iter()
        .map(|token| token.terminals())
        .collect();
    assert_eq!(first_sets, second_sets);
}

#[test]
fn active_contract_reserves_fixed_spellings_and_frees_retired_effect_words() {
    let inputs = [SourceInput::new(
        "reserved.wf",
        b"as external blocks for in ..",
    )];
    let Ok(bundle) = source_bundle(&inputs) else {
        panic!("test source bundle must be constructible");
    };
    let Ok(lexed) = lexed(&bundle) else {
        panic!("reserved spellings must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 6 },
    ) else {
        panic!("reserved spellings must classify under the active contract");
    };
    let expected = [
        b"as".as_slice(),
        b"external",
        b"blocks",
        b"for",
        b"in",
        b"..",
    ];
    assert_eq!(classified.tokens().len(), expected.len());
    for (token, spelling) in classified.tokens().iter().zip(expected) {
        if matches!(spelling, b"as" | b"external" | b"blocks") {
            assert!(token.terminals().contains(TerminalPredicate::Identifier));
            assert!(FixedTerminal::from_spelling(spelling).is_none());
        } else {
            let terminal = FixedTerminal::from_spelling(spelling)
                .unwrap_or_else(|| panic!("missing fixed terminal for {spelling:?}"));
            assert!(
                token
                    .terminals()
                    .contains(TerminalPredicate::Fixed(terminal))
            );
            assert!(!token.terminals().contains(TerminalPredicate::Identifier));
        }
        assert_eq!(token.terminals().len(), 1);
    }
}
