use crate::{ByteOffset, SourceBundle, SourceId, SourceSpan};

use crate::Lexeme;

/// Which explicit lexical output ceiling was exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LexLimit {
    /// Number of ordered source files.
    Sources,
    /// Bytes in one source file.
    SourceBytes,
    /// Sum of bytes across the ordered source bundle.
    TotalSourceBytes,
    /// Bytes in one token-shaped partition member.
    TokenBytes,
    /// Total tokens across the ordered source bundle.
    Tokens,
    /// Total token and trivia pieces across the ordered source bundle.
    Lexemes,
}

/// Caller-selected implementation ceilings for lexical output.
///
/// There is deliberately no default or unbounded production profile. The
/// caller must select every ceiling explicitly for its deployment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LexLimits {
    /// Maximum ordered source count.
    pub max_sources: u32,
    /// Maximum bytes in one source file.
    pub max_source_bytes: u64,
    /// Maximum total bytes across all source files.
    pub max_total_source_bytes: u64,
    /// Maximum bytes in one token candidate.
    pub max_token_bytes: u64,
    /// Maximum token count.
    pub max_tokens: u64,
    /// Maximum token-plus-trivia count.
    pub max_lexemes: u64,
}

impl LexLimits {
    /// Returns the inclusive token ceiling.
    #[must_use]
    pub const fn max_tokens(self) -> u64 {
        self.max_tokens
    }

    /// Returns the inclusive token-plus-trivia ceiling.
    #[must_use]
    pub const fn max_lexemes(self) -> u64 {
        self.max_lexemes
    }
}

/// Which output allocation could not be represented or reserved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LexStorage {
    /// Flat token-and-trivia partition storage.
    Lexemes,
    /// Per-source boundary index storage.
    SourceBoundaries,
}

/// A toolchain resource failure, never a Whitefoot semantic verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LexResourceFailure {
    /// A configured inclusive ceiling was exceeded.
    LimitExceeded {
        /// Exceeded ceiling category.
        limit: LexLimit,
        /// Configured inclusive maximum.
        maximum: u64,
        /// First attempted value beyond that maximum.
        actual: u64,
    },
    /// A requested output count does not fit the host address space.
    AddressSpaceExceeded {
        /// Affected output allocation.
        storage: LexStorage,
        /// Requested element count.
        requested: u64,
    },
    /// The allocator could not reserve the already-counted output.
    StorageUnavailable {
        /// Affected output allocation.
        storage: LexStorage,
        /// Requested element count.
        requested: u64,
    },
}

/// An impossible condition in the immutable two-pass lexical implementation.
///
/// This is reported separately from malformed source and resource exhaustion;
/// callers must not translate it into a language rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LexCompilerFailure {
    /// A scanner-produced range failed validation against its own source.
    InvalidProducedSpan {
        /// Source being scanned.
        source: SourceId,
        /// Inclusive produced start.
        start: ByteOffset,
        /// Exclusive produced end.
        end: ByteOffset,
    },
    /// The immutable second pass did not reproduce the first pass.
    PassDisagreement {
        /// Source on which the passes disagreed.
        source: SourceId,
    },
    /// The emission pass produced different token or lexeme counts.
    PassCountDisagreement {
        /// Count established by the allocation-free pass.
        expected_lexemes: u64,
        /// Count observed or attempted by the emission pass.
        actual_lexemes: u64,
        /// Token count established by the allocation-free pass.
        expected_tokens: u64,
        /// Token count observed or attempted by the emission pass.
        actual_tokens: u64,
    },
    /// A checked counter overflowed despite the source-bundle bounds.
    CounterOverflow,
}

/// Classification of malformed bytes found before parsing.
///
/// Each category is one clause of DIAG-1's raw-lexical attribution, so the
/// numbered rule it cites is fixed by the category alone and is returned by
/// [`SourceIssueKind::rule_id`]. These categories still carry no tree path,
/// acceptance verdict, or conformance authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceIssueKind {
    /// The first byte of an invalid UTF-8 sequence.
    InvalidUtf8,
    /// A byte is not part of the active raw token or retained trivia shape.
    UnexpectedByte,
    /// An at sign is not followed by a lowercase label-name start.
    MissingLabelName,
    /// A string reached the source boundary without a closing quote.
    UnterminatedString,
    /// A string contains a byte outside its closed raw-byte set.
    InvalidStringByte,
    /// A string uses an escape outside the closed escape set.
    InvalidStringEscape,
    /// A control or DEL byte violates the source UTF-8/byte contract.
    InvalidSourceByte,
    /// An exact `//` or `/*` prefix violates the no-comments rule.
    CommentPrefix,
}

impl SourceIssueKind {
    /// Returns the numbered rule DIAG-1 attributes to this raw lexical defect.
    ///
    /// DIAG-1's raw lexical scanning paragraph fixes one rule per defect
    /// shape, and the scanner already branches on exactly those shapes: an
    /// invalid UTF-8 encoding or a forbidden control byte cites FORM-2
    /// everywhere it occurs, an `@` with no lowercase follower cites FORM-3,
    /// a `//` or `/*` prefix cites FORM-4, every STRING-interior defect cites
    /// FORM-5, and any other byte or scalar that cannot begin a specified
    /// token cites FORM-1. v0.60 removed the apostrophe clause with REGIONID,
    /// so `'` is now an ordinary byte that begins no specified token and
    /// falls to the residual FORM-1 clause.
    #[must_use]
    pub const fn rule_id(self) -> &'static str {
        match self {
            Self::InvalidUtf8 | Self::InvalidSourceByte => "FORM-2",
            Self::UnexpectedByte => "FORM-1",
            Self::MissingLabelName => "FORM-3",
            Self::UnterminatedString | Self::InvalidStringByte | Self::InvalidStringEscape => {
                "FORM-5"
            }
            Self::CommentPrefix => "FORM-4",
        }
    }
}

/// One source-local issue found before a canonical tree exists.
#[derive(Clone, Copy, Debug)]
pub struct SourceIssue {
    pub(crate) span: SourceSpan,
    pub(crate) kind: SourceIssueKind,
}

impl SourceIssue {
    pub(crate) const fn new(span: SourceSpan, kind: SourceIssueKind) -> Self {
        Self { span, kind }
    }

    /// Returns the internal issue classification.
    #[must_use]
    pub const fn kind(self) -> SourceIssueKind {
        self.kind
    }

    /// Returns the exact source-bound location discovered by the scanner.
    #[must_use]
    pub const fn span(self) -> SourceSpan {
        self.span
    }
}

/// A complete lossless lexical partition of one ordered source bundle.
///
/// Completion proves only that every byte belongs to a recognized token or
/// retained trivia shape. It does not prove parsing, canonical formatting, or
/// semantic acceptance.
#[derive(Debug)]
pub struct LexedBundle {
    pub(crate) source: SourceBundle,
    pub(crate) lexemes: Vec<Lexeme>,
    pub(crate) source_offsets: Vec<usize>,
    pub(crate) token_count: u64,
}

impl LexedBundle {
    /// Returns the exact source bundle from which all handles were derived.
    #[must_use]
    pub const fn source_bundle(&self) -> &SourceBundle {
        &self.source
    }

    /// Returns all partition members in source order, then byte order.
    #[must_use]
    pub fn lexemes(&self) -> &[Lexeme] {
        &self.lexemes
    }

    /// Returns the partition of one bundle source, including an empty one.
    #[must_use]
    pub fn source_lexemes(&self, source: SourceId) -> Option<&[Lexeme]> {
        let index = usize::try_from(source.ordinal()).ok()?;
        let start = *self.source_offsets.get(index)?;
        let end = *self.source_offsets.get(index.checked_add(1)?)?;
        self.lexemes.get(start..end)
    }

    /// Returns the total number of token pieces.
    #[must_use]
    pub const fn token_count(&self) -> u64 {
        self.token_count
    }
}

/// Failure-atomic result of lexing an ordered source bundle.
#[derive(Debug)]
pub enum LexOutcome {
    /// Every source byte has exactly one token-or-trivia owner.
    Complete(LexedBundle),
    /// Source bytes do not form a complete lexical partition.
    SourceIssue(SourceIssue),
    /// Explicit ceilings or host storage prevented completion.
    ResourceFailure(LexResourceFailure),
    /// An internal invariant failed; this is not a source verdict.
    CompilerFailure(LexCompilerFailure),
}
