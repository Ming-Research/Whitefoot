use crate::SourceId;

use super::{bundle, complete, generous_limits};
use crate::{LexLimit, LexOutcome, LexResourceFailure};

#[test]
fn explicit_limits_accept_edges_and_reject_the_first_excess_piece() {
    let source = bundle(&[("limits.wf", b"a b\n")]);
    let mut limits = generous_limits();
    limits.max_tokens = 2;
    limits.max_lexemes = 4;
    assert!(matches!(
        crate::lex(&source, limits),
        LexOutcome::Complete(_)
    ));
    limits.max_tokens = 1;
    assert!(matches!(
        crate::lex(&source, limits),
        LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
            limit: LexLimit::Tokens,
            maximum: 1,
            actual: 2,
        })
    ));
    limits.max_tokens = 2;
    limits.max_lexemes = 3;
    assert!(matches!(
        crate::lex(&source, limits),
        LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
            limit: LexLimit::Lexemes,
            maximum: 3,
            actual: 4,
        })
    ));
}

#[test]
fn source_and_token_byte_ceilings_are_explicit_and_independent() {
    let source = bundle(&[("long.wf", b"abcdefgh")]);
    let mut limits = generous_limits();
    limits.max_sources = 0;
    assert!(matches!(
        crate::lex(&source, limits),
        LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
            limit: LexLimit::Sources,
            actual: 1,
            ..
        })
    ));

    limits = generous_limits();
    limits.max_source_bytes = 7;
    assert!(matches!(
        crate::lex(&source, limits),
        LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
            limit: LexLimit::SourceBytes,
            actual: 8,
            ..
        })
    ));

    limits = generous_limits();
    limits.max_total_source_bytes = 7;
    assert!(matches!(
        crate::lex(&source, limits),
        LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
            limit: LexLimit::TotalSourceBytes,
            actual: 8,
            ..
        })
    ));

    limits = generous_limits();
    limits.max_token_bytes = 7;
    assert!(matches!(
        crate::lex(&source, limits),
        LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
            limit: LexLimit::TokenBytes,
            actual: 8,
            ..
        })
    ));
}

#[test]
fn empty_files_have_distinct_empty_partitions() {
    let source = bundle(&[("first.wf", b""), ("second.wf", b"")]);
    let lexed = complete(&source);
    assert_eq!(lexed.lexemes().len(), 0);
    assert_eq!(
        lexed
            .source_lexemes(SourceId::from_ordinal(0))
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        lexed
            .source_lexemes(SourceId::from_ordinal(1))
            .unwrap()
            .len(),
        0
    );
    assert!(lexed.source_lexemes(SourceId::from_ordinal(2)).is_none());
}

#[test]
fn token_count_excludes_retained_trivia() {
    let source = bundle(&[("count.wf", b"  a  \n")]);
    let lexed = complete(&source);
    assert_eq!(lexed.token_count(), 1);
    assert_eq!(lexed.lexemes().len(), 4);
}

#[test]
fn exact_reservation_failure_is_reported_without_panicking() {
    let result = crate::lexer::scanner::reserve_exact::<u8>(crate::LexStorage::Lexemes, u64::MAX);
    assert!(matches!(
        result,
        Err(LexResourceFailure::AddressSpaceExceeded { .. })
            | Err(LexResourceFailure::StorageUnavailable { .. })
    ));
}

#[test]
fn different_sufficient_limits_produce_identical_partitions() {
    let source = bundle(&[("same.wf", b"fn  name() -> result: unit\n")]);
    let render = |limits| match crate::lex(&source, limits) {
        LexOutcome::Complete(lexed) => lexed
            .lexemes()
            .iter()
            .map(|lexeme| {
                (
                    lexeme.span().start().value(),
                    lexeme.span().end().value(),
                    source
                        .span_bytes(lexeme.span())
                        .expect("a lexeme of this bundle")
                        .to_vec(),
                )
            })
            .collect::<Vec<_>>(),
        outcome => panic!("expected complete result, got {outcome:?}"),
    };
    let generous = generous_limits();
    let exact = crate::LexLimits {
        max_sources: 1,
        max_source_bytes: source.total_bytes(),
        max_total_source_bytes: source.total_bytes(),
        max_token_bytes: 6,
        max_tokens: 9,
        max_lexemes: 15,
    };
    assert_eq!(render(generous), render(exact));
}

#[test]
fn giant_token_and_alternating_partition_limits_fail_at_exact_edges() {
    let huge = vec![b'a'; 8_192];
    let source = bundle(&[("huge.wf", &huge)]);
    let mut limits = generous_limits();
    limits.max_token_bytes = 8_191;
    assert!(matches!(
        crate::lex(&source, limits),
        LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
            limit: LexLimit::TokenBytes,
            actual: 8_192,
            ..
        })
    ));

    let source = bundle(&[("alternating.wf", b"a a a a")]);
    limits = generous_limits();
    limits.max_tokens = 4;
    limits.max_lexemes = 7;
    assert!(matches!(
        crate::lex(&source, limits),
        LexOutcome::Complete(_)
    ));
    limits.max_lexemes = 6;
    assert!(matches!(
        crate::lex(&source, limits),
        LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
            limit: LexLimit::Lexemes,
            actual: 7,
            ..
        })
    ));
}
