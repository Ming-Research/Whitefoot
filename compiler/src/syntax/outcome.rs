use crate::lexer::{Token, TokenId};
use crate::syntax::terminal::TerminalSet;
use crate::{ByteOffset, SourceBundle, SourceId, SpecHash};

/// Which explicit terminal-classification ceiling was exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalLimit {
    /// Total formed tokens across the ordered source bundle.
    Tokens,
}

/// Caller-selected implementation ceilings for terminal classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalLimits {
    /// Maximum total formed-token count.
    pub max_tokens: u64,
}

/// Which exact output allocation could not be represented or reserved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalStorage {
    /// Classified token records.
    Tokens,
    /// Per-source token-boundary indices.
    SourceBoundaries,
}

/// A toolchain resource failure, never a Whitefoot source verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalResourceFailure {
    /// A configured inclusive ceiling was exceeded.
    LimitExceeded {
        /// Exceeded ceiling category.
        limit: TerminalLimit,
        /// Configured inclusive maximum.
        maximum: u64,
        /// Exact requested value beyond that maximum.
        actual: u64,
    },
    /// An exact output count does not fit the host address space.
    AddressSpaceExceeded {
        /// Affected output allocation.
        storage: TerminalStorage,
        /// Requested element count.
        requested: u64,
    },
    /// The allocator could not reserve the already-counted output.
    StorageUnavailable {
        /// Affected output allocation.
        storage: TerminalStorage,
        /// Requested element count.
        requested: u64,
    },
}

/// An invocation/input failure, never a Whitefoot source verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalInvocationFailure {
    /// The caller selected a different numbered specification.
    SpecificationMismatch {
        /// Exact specification required by this entry point.
        expected: SpecHash,
        /// Exact specification supplied by the caller.
        actual: SpecHash,
    },
}

/// An impossible condition in the trusted terminal-classification boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCompilerFailure {
    /// A shape token did not satisfy the corresponding approved predicate.
    InvalidFormedToken {
        /// Source containing the impossible token.
        source: SourceId,
        /// Inclusive token start.
        start: ByteOffset,
        /// Exclusive token end.
        end: ByteOffset,
    },
    /// A complete lexical result omitted one source partition.
    MissingSourcePartition {
        /// Source whose partition was absent.
        source: SourceId,
    },
    /// The lexer-reported count disagreed with the token stream.
    TokenCountDisagreement {
        /// Count reported by the complete lexical result.
        expected: u64,
        /// Count observed while classifying.
        actual: u64,
    },
    /// A checked counter overflowed despite the lexical result's bounds.
    CounterOverflow,
}

/// The numbered rule owning a terminal-membership rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalIssueOwner {
    /// FORM-3 lexical-class membership and reserved lower-word spellings.
    Form3,
    /// FORM-5 literal spelling membership.
    Form5,
    /// GRAM-1 operator-form suffix membership.
    Gram1,
}

impl TerminalIssueOwner {
    /// Returns the exact numbered rule spelling from the active kernel specification.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Form3 => "FORM-3",
            Self::Form5 => "FORM-5",
            Self::Gram1 => "GRAM-1",
        }
    }
}

/// The first formed token that matched no approved terminal predicate.
#[derive(Clone, Copy, Debug)]
pub struct TerminalIssue {
    pub(crate) token: TokenId,
    pub(crate) owner: TerminalIssueOwner,
}

impl TerminalIssue {
    /// Returns the exact source-bound offending token.
    #[must_use]
    pub const fn token(self) -> TokenId {
        self.token
    }

    /// Returns the numbered rule owning the rejected spelling.
    #[must_use]
    pub const fn owner(self) -> TerminalIssueOwner {
        self.owner
    }
}

/// One formed token and every approved active-specification predicate it satisfies.
#[derive(Clone, Copy, Debug)]
pub struct ClassifiedToken {
    pub(crate) token: Token,
    pub(crate) terminals: TerminalSet,
}

impl ClassifiedToken {
    /// Returns the original source-bound shape token.
    #[must_use]
    pub const fn token(self) -> Token {
        self.token
    }

    /// Returns every matching terminal predicate without priority selection.
    #[must_use]
    pub const fn terminals(self) -> TerminalSet {
        self.terminals
    }
}

/// A complete terminal-membership projection over one lossless lexical result.
///
/// It keeps a handle on the source bundle its tokens were formed from, which
/// every later stage reads token text through; the lexemes and trivia are
/// not kept, since no stage after classification reads them. This type is
/// not a parse tree, a portable token identity, or an acceptance capability.
#[derive(Clone, Debug)]
pub struct ClassifiedBundle {
    pub(crate) spec: SpecHash,
    pub(crate) source: SourceBundle,
    pub(crate) tokens: Vec<ClassifiedToken>,
    pub(crate) source_offsets: Vec<usize>,
}

impl ClassifiedBundle {
    /// Returns the exact numbered specification owning every predicate.
    #[must_use]
    pub const fn spec_hash(&self) -> SpecHash {
        self.spec
    }

    /// Returns all classified tokens in source order, then byte order.
    #[must_use]
    pub fn tokens(&self) -> &[ClassifiedToken] {
        &self.tokens
    }

    /// Returns one source's classified token sequence, including an empty one.
    #[must_use]
    pub fn source_tokens(&self, source: SourceId) -> Option<&[ClassifiedToken]> {
        let index = usize::try_from(source.ordinal()).ok()?;
        let start = *self.source_offsets.get(index)?;
        let end = *self.source_offsets.get(index.checked_add(1)?)?;
        self.tokens.get(start..end)
    }

    /// Returns the exact source bundle underlying every token handle.
    #[must_use]
    pub const fn source_bundle(&self) -> &SourceBundle {
        &self.source
    }

    /// Returns the source bytes one token of this bundle covers.
    #[must_use]
    pub fn token_bytes(&self, token: Token) -> Option<&[u8]> {
        self.source.span_bytes(token.span())
    }
}

/// Failure-atomic result of complete active-specification terminal classification.
#[derive(Debug)]
pub enum TerminalOutcome {
    /// Every formed token retained at least one approved predicate.
    Complete(ClassifiedBundle),
    /// One token matched no approved terminal predicate.
    SourceIssue(TerminalIssue),
    /// Explicit ceilings or host storage prevented completion.
    ResourceFailure(TerminalResourceFailure),
    /// The invocation does not select the exact contract required here.
    InvocationFailure(TerminalInvocationFailure),
    /// A trusted compiler invariant failed.
    CompilerFailure(TerminalCompilerFailure),
}
