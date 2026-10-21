//! Lossless, resource-bounded lexical shapes for the active Whitefoot specification.
//!
//! The lexer entry point performs byte-preserving lexical partitioning. A
//! complete lexical result is not a language-acceptance verdict: parsing,
//! terminal classification, canonical formatting, semantic checking, and
//! normative diagnostics remain separate stages.

mod outcome;
mod repairs;
mod scanner;
mod token;

pub use outcome::{
    LexCompilerFailure, LexLimit, LexLimits, LexOutcome, LexResourceFailure, LexStorage,
    LexedBundle, SourceIssue, SourceIssueKind,
};
pub use scanner::lex;
pub use token::{Lexeme, Token, TokenId, TokenKind, Trivia, TriviaKind};

#[cfg(test)]
mod tests;
