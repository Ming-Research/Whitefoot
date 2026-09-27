use crate::lexer::{LexLimits, LexOutcome, lex};
use crate::{ACTIVE_KERNEL_SPEC_HASH, SourceBundle, SourceInput, SourceLimits};

use crate::{ClassifiedBundle, TerminalLimits, TerminalOutcome, classify_terminals};

use super::super::{CanonicalLimits, FinalizeLimits};
use crate::syntax::parser::{ParseLimits, ParseOutcome, ParsedBundle, parse};

const SOURCE_LIMITS: SourceLimits = SourceLimits {
    max_sources: 16,
    max_logical_path_bytes: 128,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_binding_bytes: 1_048_576,
};

const LEX_LIMITS: LexLimits = LexLimits {
    max_sources: 16,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_token_bytes: 16_384,
    max_tokens: 131_072,
    max_lexemes: 262_144,
};

const PARSE_LIMITS: ParseLimits = ParseLimits {
    max_work: 8_000_000,
    max_tasks: 131_072,
    max_frames: 8_192,
    max_elements: 262_144,
};

pub(super) const FINALIZE_LIMITS: FinalizeLimits = FinalizeLimits {
    max_work: 8_000_000,
    max_roots: 131_072,
    max_shape_tasks: 131_072,
    max_nodes: 131_072,
    max_child_edges: 131_072,
    max_terminals: 131_072,
    max_sources: 16,
};

pub(super) const CANONICAL_LIMITS: CanonicalLimits = CanonicalLimits {
    max_work: 8_000_000,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_gaps: 131_072,
    max_path_components: 8_192,
};

pub(super) fn with_parsed<ResultValue>(
    inputs: &[SourceInput<'_>],
    run: impl FnOnce(ParsedBundle) -> ResultValue,
) -> ResultValue {
    let Ok(bundle) = SourceBundle::with_limits(inputs, SOURCE_LIMITS) else {
        panic!("test source bundle must be valid");
    };
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("test source must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) else {
        panic!("test source must classify");
    };
    let ParseOutcome::Complete(parsed) = parse(classified, PARSE_LIMITS) else {
        panic!("test source must derive");
    };
    run(parsed)
}

pub(super) fn source_offsets(classified: &ClassifiedBundle) -> Vec<usize> {
    classified.source_offsets.clone()
}

/// Renders one source's canonical bytes, or `None` when it reaches no tree.
///
/// Rendering is defined on any source that parses, canonical or not, which is
/// what makes it usable for migration: the input need only be well-formed.
pub(super) fn rendered_bytes(source: &[u8]) -> Option<Vec<u8>> {
    let inputs = [SourceInput::new("generated.wf", source)];
    let Ok(bundle) = SourceBundle::with_limits(&inputs, SOURCE_LIMITS) else {
        panic!("generated source envelope must remain valid");
    };
    let lexed = match lex(&bundle, LEX_LIMITS) {
        LexOutcome::Complete(lexed) => lexed,
        LexOutcome::SourceIssue(_) => return None,
        other => panic!("generated source must not hit a non-source lex outcome: {other:?}"),
    };
    let classified = match classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) {
        TerminalOutcome::Complete(classified) => classified,
        TerminalOutcome::SourceIssue(_) => return None,
        other => panic!("generated source must not hit a non-source terminal outcome: {other:?}"),
    };
    let parsed = match parse(classified, PARSE_LIMITS) {
        ParseOutcome::Complete(parsed) => parsed,
        ParseOutcome::SourceIssue(_) => return None,
        other => panic!("generated source must not hit a non-source parse outcome: {other:?}"),
    };
    let finalized = match super::super::finalize(parsed, FINALIZE_LIMITS) {
        super::super::FinalizeOutcome::Complete(finalized) => finalized,
        other => panic!("trusted generated derivation must finalize: {other:?}"),
    };
    match super::super::render_canonical(&finalized, CANONICAL_LIMITS) {
        super::super::RenderOutcome::Complete(sources) => {
            let [rendered] = <[_; 1]>::try_from(sources).expect("one input renders one source");
            Some(rendered.bytes)
        }
        other => panic!("a finalized tree must render: {other:?}"),
    }
}

pub(super) fn reaches_canonical_syntax(source: &[u8]) -> bool {
    let inputs = [SourceInput::new("generated.wf", source)];
    let Ok(bundle) = SourceBundle::with_limits(&inputs, SOURCE_LIMITS) else {
        panic!("generated source envelope must remain valid");
    };
    let lexed = match lex(&bundle, LEX_LIMITS) {
        LexOutcome::Complete(lexed) => lexed,
        LexOutcome::SourceIssue(_) => return false,
        other => panic!("generated source must not hit a non-source lex outcome: {other:?}"),
    };
    let classified = match classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) {
        TerminalOutcome::Complete(classified) => classified,
        TerminalOutcome::SourceIssue(_) => return false,
        other => panic!("generated source must not hit a non-source terminal outcome: {other:?}"),
    };
    let parsed = match parse(classified, PARSE_LIMITS) {
        ParseOutcome::Complete(parsed) => parsed,
        ParseOutcome::SourceIssue(_) => return false,
        other => panic!("generated source must not hit a non-source parse outcome: {other:?}"),
    };
    let finalized = match super::super::finalize(parsed, FINALIZE_LIMITS) {
        super::super::FinalizeOutcome::Complete(finalized) => finalized,
        other => panic!("trusted generated derivation must finalize: {other:?}"),
    };
    match super::super::audit_canonical(*finalized, CANONICAL_LIMITS) {
        super::super::CanonicalOutcome::Complete(_) => true,
        super::super::CanonicalOutcome::SourceIssue(_) => false,
        other => panic!("generated source must not hit an internal canonical outcome: {other:?}"),
    }
}
