#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

mod edge_cases;
mod lexical;
mod resources;

use crate::{SourceBundle, SourceInput, SourceLimits};

use crate::{LexLimits, LexOutcome, LexedBundle};

fn generous_limits() -> LexLimits {
    LexLimits {
        max_sources: u32::MAX,
        max_source_bytes: u64::MAX,
        max_total_source_bytes: u64::MAX,
        max_token_bytes: u64::MAX,
        max_tokens: u64::MAX,
        max_lexemes: u64::MAX,
    }
}

fn bundle(inputs: &[(&str, &[u8])]) -> SourceBundle {
    let inputs: Vec<_> = inputs
        .iter()
        .map(|(path, bytes)| SourceInput::new(path, bytes))
        .collect();
    SourceBundle::with_limits(&inputs, SourceLimits::REPRESENTABLE).unwrap()
}

fn complete(source: &SourceBundle) -> LexedBundle {
    match crate::lex(source, generous_limits()) {
        LexOutcome::Complete(lexed) => lexed,
        outcome => panic!("expected complete lexical partition, got {outcome:?}"),
    }
}
