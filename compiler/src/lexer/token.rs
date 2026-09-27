use core::fmt;

use crate::{ByteOffset, SourceId, SourceSpan};

/// A validated in-memory identity for one token occurrence.
///
/// It is the source and byte range the lexer validated against its bundle,
/// whose bytes are read through that bundle. Its source and offsets are
/// portable coordinates only when they are accompanied by a separately
/// verified source binding; they are not a global identifier, digest, or
/// authentication token.
#[derive(Clone, Copy)]
pub struct TokenId {
    span: SourceSpan,
}

impl fmt::Debug for TokenId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenId")
            .field("source", &self.source())
            .field("start", &self.start())
            .field("end", &self.end())
            .finish()
    }
}

impl TokenId {
    pub(crate) const fn new(span: SourceSpan) -> Self {
        Self { span }
    }

    /// Returns the bundle-order source coordinate.
    #[must_use]
    pub const fn source(self) -> SourceId {
        self.span.source()
    }

    /// Returns the inclusive starting byte coordinate.
    #[must_use]
    pub const fn start(self) -> ByteOffset {
        self.span.start()
    }

    /// Returns the exclusive ending byte coordinate.
    #[must_use]
    pub const fn end(self) -> ByteOffset {
        self.span.end()
    }

    /// Returns the exact source-bound span that validates this handle.
    #[must_use]
    pub const fn span(self) -> SourceSpan {
        self.span
    }
}

/// Shape-only lexical category for one Whitefoot token.
///
/// Lowercase words include keywords and dotless operation names. Numeric forms
/// retain their spelling without deciding type, range, or canonical float
/// representation. Those distinctions belong to later stages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    /// A FORM-3 lowercase word shape.
    LowerWordForm,
    /// A FORM-3 uppercase word shape.
    UpperWordForm,
    /// An at-sign-prefixed label shape.
    LabelForm,
    /// A dotted operation-name shape with one closed mode suffix.
    OperationNameForm,
    /// An arithmetic operator byte carrying a maximal lowercase mode suffix.
    ///
    /// The suffix is retained exactly as written, including one the closed
    /// `infix_op` list does not admit; rejecting it is terminal membership's
    /// decision, not formation's.
    OperatorForm,
    /// A decimal numeric candidate retained for later literal checking.
    NumberForm,
    /// A string with a lexically valid raw body and escape structure.
    StringForm,
    /// `(`.
    LeftParen,
    /// `)`.
    RightParen,
    /// `{`.
    LeftBrace,
    /// `}`.
    RightBrace,
    /// `[`.
    LeftBracket,
    /// `]`.
    RightBracket,
    /// `<` when it is not part of `<=`.
    LeftAngle,
    /// `>` when it is not part of `>=`.
    RightAngle,
    /// `,`.
    Comma,
    /// `:` when it is not part of `::`.
    Colon,
    /// `;`.
    Semicolon,
    /// `.` when it is not part of an operation name.
    Dot,
    /// `..`.
    DotDot,
    /// `=` when it is not part of a fat arrow or of `==`.
    Equal,
    /// `->`.
    ThinArrow,
    /// `=>`.
    FatArrow,
    /// `==`.
    EqualEqual,
    /// `!=`.
    BangEqual,
    /// `<=`.
    LessEqual,
    /// `>=`.
    GreaterEqual,
    /// `::`.
    ColonColon,
    /// `&`.
    Ampersand,
}

/// One validated token: the source range it covers and its shape.
#[derive(Clone, Copy, Debug)]
pub struct Token {
    span: SourceSpan,
    kind: TokenKind,
}

impl Token {
    pub(crate) const fn new(span: SourceSpan, kind: TokenKind) -> Self {
        Self { span, kind }
    }

    /// Returns the source-bound token identity.
    #[must_use]
    pub const fn id(self) -> TokenId {
        TokenId::new(self.span)
    }

    /// Returns the shape-only lexical category.
    #[must_use]
    pub const fn kind(self) -> TokenKind {
        self.kind
    }

    /// Returns the exact source span and bytes for this token.
    #[must_use]
    pub const fn span(self) -> SourceSpan {
        self.span
    }
}

/// Kind of byte-preserving trivia between tokens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriviaKind {
    /// One maximal nonempty run of ASCII space bytes.
    Spaces,
    /// One LF byte.
    LineFeed,
}

/// One trivia piece bound to the exact source bytes it covers.
#[derive(Clone, Copy, Debug)]
pub struct Trivia {
    span: SourceSpan,
    kind: TriviaKind,
}

impl Trivia {
    pub(crate) const fn new(span: SourceSpan, kind: TriviaKind) -> Self {
        Self { span, kind }
    }

    /// Returns the trivia category.
    #[must_use]
    pub const fn kind(self) -> TriviaKind {
        self.kind
    }

    /// Returns the exact source span and bytes for this trivia.
    #[must_use]
    pub const fn span(self) -> SourceSpan {
        self.span
    }
}

/// One member of the exact lexical partition of a source file.
#[derive(Clone, Copy, Debug)]
pub enum Lexeme {
    /// A token-shaped byte range.
    Token(Token),
    /// A retained space or line-feed byte range.
    Trivia(Trivia),
}

impl Lexeme {
    /// Returns the exact source span covered by this partition member.
    #[must_use]
    pub const fn span(self) -> SourceSpan {
        match self {
            Self::Token(token) => token.span(),
            Self::Trivia(trivia) => trivia.span(),
        }
    }
}
