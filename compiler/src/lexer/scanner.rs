use crate::{ByteOffset, SourceBundle, SourceId, SourceSpan};

use crate::{
    LexCompilerFailure, LexLimit, LexLimits, LexOutcome, LexResourceFailure, LexStorage,
    LexedBundle, SourceIssue, SourceIssueKind,
};
use crate::{Lexeme, Token, TokenKind, Trivia, TriviaKind};

#[derive(Clone, Copy)]
enum RawKind {
    Token(TokenKind),
    Trivia(TriviaKind),
}

impl RawKind {
    const fn is_token(self) -> bool {
        matches!(self, Self::Token(_))
    }
}

#[derive(Clone, Copy)]
struct RawLexeme {
    start: usize,
    end: usize,
    kind: RawKind,
}

#[derive(Clone, Copy)]
struct RawIssue {
    start: usize,
    end: usize,
    kind: SourceIssueKind,
}

struct Scanner<'bytes> {
    bytes: &'bytes [u8],
    cursor: usize,
}

impl<'bytes> Scanner<'bytes> {
    const fn new(bytes: &'bytes [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn next(&mut self) -> Result<Option<RawLexeme>, RawIssue> {
        let start = self.cursor;
        let Some(byte) = self.bytes.get(start).copied() else {
            return Ok(None);
        };

        let lexeme = match byte {
            b' ' => self.spaces(start),
            b'\n' => self.fixed(start, 1, RawKind::Trivia(TriviaKind::LineFeed)),
            b'a'..=b'z' => self.lower_word(start),
            b'A'..=b'Z' => self.upper_word(start),
            b'@' => self.label_form(start)?,
            b'0'..=b'9' => self.number(start),
            b'-' if self.bytes.get(start + 1) == Some(&b'>') => {
                self.fixed(start, 2, RawKind::Token(TokenKind::ThinArrow))
            }
            b'-' if self.bytes.get(start + 1).is_some_and(u8::is_ascii_digit) => self.number(start),
            b'=' if self.bytes.get(start + 1) == Some(&b'>') => {
                self.fixed(start, 2, RawKind::Token(TokenKind::FatArrow))
            }
            b'=' if self.bytes.get(start + 1) == Some(&b'=') => {
                self.fixed(start, 2, RawKind::Token(TokenKind::EqualEqual))
            }
            b'!' if self.bytes.get(start + 1) == Some(&b'=') => {
                self.fixed(start, 2, RawKind::Token(TokenKind::BangEqual))
            }
            b'<' if self.bytes.get(start + 1) == Some(&b'=') => {
                self.fixed(start, 2, RawKind::Token(TokenKind::LessEqual))
            }
            b'>' if self.bytes.get(start + 1) == Some(&b'=') => {
                self.fixed(start, 2, RawKind::Token(TokenKind::GreaterEqual))
            }
            b':' if self.bytes.get(start + 1) == Some(&b':') => {
                self.fixed(start, 2, RawKind::Token(TokenKind::ColonColon))
            }
            b'/' if matches!(self.bytes.get(start + 1), Some(b'/' | b'*')) => {
                return Err(RawIssue {
                    start,
                    end: start + 2,
                    kind: SourceIssueKind::CommentPrefix,
                });
            }
            b'+' | b'-' | b'*' | b'/' | b'%' => self.operator_form(start),
            b'"' => self.string(start)?,
            b'(' => self.fixed(start, 1, RawKind::Token(TokenKind::LeftParen)),
            b')' => self.fixed(start, 1, RawKind::Token(TokenKind::RightParen)),
            b'{' => self.fixed(start, 1, RawKind::Token(TokenKind::LeftBrace)),
            b'}' => self.fixed(start, 1, RawKind::Token(TokenKind::RightBrace)),
            b'[' => self.fixed(start, 1, RawKind::Token(TokenKind::LeftBracket)),
            b']' => self.fixed(start, 1, RawKind::Token(TokenKind::RightBracket)),
            b'<' => self.fixed(start, 1, RawKind::Token(TokenKind::LeftAngle)),
            b'>' => self.fixed(start, 1, RawKind::Token(TokenKind::RightAngle)),
            b',' => self.fixed(start, 1, RawKind::Token(TokenKind::Comma)),
            b':' => self.fixed(start, 1, RawKind::Token(TokenKind::Colon)),
            b';' => self.fixed(start, 1, RawKind::Token(TokenKind::Semicolon)),
            b'.' if self.bytes.get(start + 1) == Some(&b'.') => {
                self.fixed(start, 2, RawKind::Token(TokenKind::DotDot))
            }
            b'.' => self.fixed(start, 1, RawKind::Token(TokenKind::Dot)),
            b'=' => self.fixed(start, 1, RawKind::Token(TokenKind::Equal)),
            b'&' => self.fixed(start, 1, RawKind::Token(TokenKind::Ampersand)),
            _ if !byte.is_ascii() => {
                let (end, kind) = match utf8_scalar_len(&self.bytes[start..]) {
                    Some(length) => (start + length, SourceIssueKind::UnexpectedByte),
                    None => (start + 1, SourceIssueKind::InvalidUtf8),
                };
                return Err(RawIssue { start, end, kind });
            }
            0x00..=0x1f | 0x7f => {
                return Err(RawIssue {
                    start,
                    end: start + 1,
                    kind: SourceIssueKind::InvalidSourceByte,
                });
            }
            _ => {
                return Err(RawIssue {
                    start,
                    end: start + 1,
                    kind: SourceIssueKind::UnexpectedByte,
                });
            }
        };
        self.cursor = lexeme.end;
        Ok(Some(lexeme))
    }

    fn fixed(&self, start: usize, length: usize, kind: RawKind) -> RawLexeme {
        RawLexeme {
            start,
            end: start + length,
            kind,
        }
    }

    fn spaces(&self, start: usize) -> RawLexeme {
        let mut end = start + 1;
        while self.bytes.get(end) == Some(&b' ') {
            end += 1;
        }
        RawLexeme {
            start,
            end,
            kind: RawKind::Trivia(TriviaKind::Spaces),
        }
    }

    fn lower_word(&self, start: usize) -> RawLexeme {
        let base_end = take_while(self.bytes, start + 1, is_lower_continuation);
        let opname_end = operation_name_end(self.bytes, base_end);
        RawLexeme {
            start,
            end: opname_end.unwrap_or(base_end),
            kind: RawKind::Token(if opname_end.is_some() {
                TokenKind::OperationNameForm
            } else {
                TokenKind::LowerWordForm
            }),
        }
    }

    /// Forms one operator token: the operator byte and its maximal `[a-z]*`
    /// suffix.
    ///
    /// The suffix is taken whole even when it is not one of `wrap`, `checked`,
    /// or `sat`, so that `a +foo b` reports one badly suffixed operator rather
    /// than an operator followed by a place named `foo`. The `->` and negative
    /// numeric guards on `b'-'` run before this arm.
    fn operator_form(&self, start: usize) -> RawLexeme {
        RawLexeme {
            start,
            end: take_while(self.bytes, start + 1, u8::is_ascii_lowercase),
            kind: RawKind::Token(TokenKind::OperatorForm),
        }
    }

    fn upper_word(&self, start: usize) -> RawLexeme {
        RawLexeme {
            start,
            end: take_while(self.bytes, start + 1, u8::is_ascii_alphanumeric),
            kind: RawKind::Token(TokenKind::UpperWordForm),
        }
    }

    /// Forms one label token: the `@` sigil and its mandatory lowercase name.
    ///
    /// v0.60 leaves `@` as the only sigil-prefixed form; the apostrophe-
    /// prefixed REGIONID retired with regions [FORM-3], so a `'` reaches the
    /// residual unexpected-byte arm instead of a second call here.
    fn label_form(&self, start: usize) -> Result<RawLexeme, RawIssue> {
        let missing = RawIssue {
            start,
            end: start + 1,
            kind: SourceIssueKind::MissingLabelName,
        };
        let Some(first) = self.bytes.get(start + 1).copied() else {
            return Err(missing);
        };
        if !first.is_ascii_lowercase() {
            return Err(missing);
        }
        Ok(RawLexeme {
            start,
            end: take_while(self.bytes, start + 2, is_lower_continuation),
            kind: RawKind::Token(TokenKind::LabelForm),
        })
    }

    fn number(&self, start: usize) -> RawLexeme {
        let mut end = start + usize::from(self.bytes.get(start) == Some(&b'-'));
        while let Some(byte) = self.bytes.get(end).copied() {
            if byte == b'.' && self.bytes.get(end + 1) == Some(&b'.') {
                break;
            }
            let previous = end.checked_sub(1).and_then(|index| self.bytes.get(index));
            let exponent_sign = matches!(byte, b'+' | b'-')
                && previous.is_some_and(|value| matches!(value, b'e' | b'E'));
            if !is_number_candidate_byte(byte) && !exponent_sign {
                break;
            }
            end += 1;
        }

        RawLexeme {
            start,
            end,
            kind: RawKind::Token(TokenKind::NumberForm),
        }
    }

    fn string(&self, start: usize) -> Result<RawLexeme, RawIssue> {
        let mut cursor = start + 1;
        loop {
            let Some(byte) = self.bytes.get(cursor).copied() else {
                return Err(RawIssue {
                    start,
                    end: self.bytes.len(),
                    kind: SourceIssueKind::UnterminatedString,
                });
            };
            match byte {
                b'"' => {
                    return Ok(RawLexeme {
                        start,
                        end: cursor + 1,
                        kind: RawKind::Token(TokenKind::StringForm),
                    });
                }
                b'\\' => {
                    let Some(escaped) = self.bytes.get(cursor + 1).copied() else {
                        return Err(RawIssue {
                            start: cursor,
                            end: cursor + 1,
                            kind: SourceIssueKind::InvalidStringEscape,
                        });
                    };
                    if !escaped.is_ascii() {
                        let Some(length) = utf8_scalar_len(&self.bytes[cursor + 1..]) else {
                            return Err(RawIssue {
                                start: cursor + 1,
                                end: cursor + 2,
                                kind: SourceIssueKind::InvalidUtf8,
                            });
                        };
                        return Err(RawIssue {
                            start: cursor,
                            end: cursor + 1 + length,
                            kind: SourceIssueKind::InvalidStringEscape,
                        });
                    }
                    if !matches!(escaped, b'\\' | b'"' | b'n') {
                        return Err(RawIssue {
                            start: cursor,
                            end: cursor + 2,
                            kind: SourceIssueKind::InvalidStringEscape,
                        });
                    }
                    cursor += 2;
                }
                0x20..=0x7e => cursor += 1,
                _ if !byte.is_ascii() => {
                    let (end, kind) = match utf8_scalar_len(&self.bytes[cursor..]) {
                        Some(length) => (cursor + length, SourceIssueKind::InvalidStringByte),
                        None => (cursor + 1, SourceIssueKind::InvalidUtf8),
                    };
                    return Err(RawIssue {
                        start: cursor,
                        end,
                        kind,
                    });
                }
                _ => {
                    return Err(RawIssue {
                        start: cursor,
                        end: cursor + 1,
                        kind: SourceIssueKind::InvalidStringByte,
                    });
                }
            }
        }
    }
}

/// Produces a failure-atomic, lossless lexical partition for the active specification source.
///
/// This entry point does not decide terminal classification, canonical
/// spacing, parseability, semantic acceptance, or normative diagnostics.
#[must_use]
pub fn lex(source: &SourceBundle, limits: LexLimits) -> LexOutcome {
    lex_shapes(source, limits)
}

fn lex_shapes(source: &SourceBundle, limits: LexLimits) -> LexOutcome {
    let source_count = match u32::try_from(source.len()) {
        Ok(value) => value,
        Err(_) => {
            return LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
                limit: LexLimit::Sources,
                maximum: u64::from(limits.max_sources),
                actual: u64::MAX,
            });
        }
    };
    if source_count > limits.max_sources {
        return LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
            limit: LexLimit::Sources,
            maximum: u64::from(limits.max_sources),
            actual: u64::from(source_count),
        });
    }
    if source.total_bytes() > limits.max_total_source_bytes {
        return LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
            limit: LexLimit::TotalSourceBytes,
            maximum: limits.max_total_source_bytes,
            actual: source.total_bytes(),
        });
    }

    let mut lexeme_count = 0_u64;
    let mut token_count = 0_u64;

    for (source_id, file) in source.iter() {
        if file.byte_len() > limits.max_source_bytes {
            return LexOutcome::ResourceFailure(LexResourceFailure::LimitExceeded {
                limit: LexLimit::SourceBytes,
                maximum: limits.max_source_bytes,
                actual: file.byte_len(),
            });
        }
        let mut scanner = Scanner::new(file.bytes());
        loop {
            match scanner.next() {
                Ok(Some(lexeme)) => {
                    lexeme_count = match increment_with_limit(
                        lexeme_count,
                        limits.max_lexemes(),
                        LexLimit::Lexemes,
                    ) {
                        Ok(value) => value,
                        Err(failure) => return LexOutcome::ResourceFailure(failure),
                    };
                    if lexeme.kind.is_token() {
                        let token_bytes = match u64::try_from(lexeme.end - lexeme.start) {
                            Ok(value) => value,
                            Err(_) => {
                                return LexOutcome::ResourceFailure(
                                    LexResourceFailure::LimitExceeded {
                                        limit: LexLimit::TokenBytes,
                                        maximum: limits.max_token_bytes,
                                        actual: u64::MAX,
                                    },
                                );
                            }
                        };
                        if token_bytes > limits.max_token_bytes {
                            return LexOutcome::ResourceFailure(
                                LexResourceFailure::LimitExceeded {
                                    limit: LexLimit::TokenBytes,
                                    maximum: limits.max_token_bytes,
                                    actual: token_bytes,
                                },
                            );
                        }
                        token_count = match increment_with_limit(
                            token_count,
                            limits.max_tokens(),
                            LexLimit::Tokens,
                        ) {
                            Ok(value) => value,
                            Err(failure) => return LexOutcome::ResourceFailure(failure),
                        };
                    }
                }
                Ok(None) => break,
                Err(issue) => return raw_issue(source, source_id, issue),
            }
        }
    }

    let Some(boundary_capacity) = source.len().checked_add(1) else {
        return LexOutcome::ResourceFailure(LexResourceFailure::AddressSpaceExceeded {
            storage: LexStorage::SourceBoundaries,
            requested: u64::MAX,
        });
    };

    let mut lexemes = match reserve_exact(LexStorage::Lexemes, lexeme_count) {
        Ok(storage) => storage,
        Err(failure) => return LexOutcome::ResourceFailure(failure),
    };
    let boundary_count = match u64::try_from(boundary_capacity) {
        Ok(value) => value,
        Err(_) => {
            return LexOutcome::ResourceFailure(LexResourceFailure::AddressSpaceExceeded {
                storage: LexStorage::SourceBoundaries,
                requested: u64::MAX,
            });
        }
    };
    let mut source_offsets = match reserve_exact(LexStorage::SourceBoundaries, boundary_count) {
        Ok(storage) => storage,
        Err(failure) => return LexOutcome::ResourceFailure(failure),
    };
    source_offsets.push(0);
    let mut emitted_lexemes = 0_u64;
    let mut emitted_tokens = 0_u64;

    for (source_id, file) in source.iter() {
        let mut scanner = Scanner::new(file.bytes());
        loop {
            let raw = match scanner.next() {
                Ok(Some(raw)) => raw,
                Ok(None) => break,
                Err(_) => {
                    return LexOutcome::CompilerFailure(LexCompilerFailure::PassDisagreement {
                        source: source_id,
                    });
                }
            };
            let span = match produced_span(source, source_id, raw.start, raw.end) {
                Ok(span) => span,
                Err(failure) => return LexOutcome::CompilerFailure(failure),
            };
            let next_lexemes = match emitted_lexemes.checked_add(1) {
                Some(value) => value,
                None => return LexOutcome::CompilerFailure(LexCompilerFailure::CounterOverflow),
            };
            let next_tokens = if raw.kind.is_token() {
                match emitted_tokens.checked_add(1) {
                    Some(value) => value,
                    None => {
                        return LexOutcome::CompilerFailure(LexCompilerFailure::CounterOverflow);
                    }
                }
            } else {
                emitted_tokens
            };
            if next_lexemes > lexeme_count || next_tokens > token_count {
                return LexOutcome::CompilerFailure(LexCompilerFailure::PassCountDisagreement {
                    expected_lexemes: lexeme_count,
                    actual_lexemes: next_lexemes,
                    expected_tokens: token_count,
                    actual_tokens: next_tokens,
                });
            }
            lexemes.push(match raw.kind {
                RawKind::Token(kind) => Lexeme::Token(Token::new(span, kind)),
                RawKind::Trivia(kind) => Lexeme::Trivia(Trivia::new(span, kind)),
            });
            emitted_lexemes = next_lexemes;
            emitted_tokens = next_tokens;
        }
        source_offsets.push(lexemes.len());
    }

    if emitted_lexemes != lexeme_count || emitted_tokens != token_count {
        return LexOutcome::CompilerFailure(LexCompilerFailure::PassCountDisagreement {
            expected_lexemes: lexeme_count,
            actual_lexemes: emitted_lexemes,
            expected_tokens: token_count,
            actual_tokens: emitted_tokens,
        });
    }
    LexOutcome::Complete(LexedBundle {
        source: source.clone(),
        lexemes,
        source_offsets,
        token_count,
    })
}

pub(crate) fn reserve_exact<T>(
    storage: LexStorage,
    requested: u64,
) -> Result<Vec<T>, LexResourceFailure> {
    let capacity = usize::try_from(requested)
        .map_err(|_| LexResourceFailure::AddressSpaceExceeded { storage, requested })?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| LexResourceFailure::StorageUnavailable { storage, requested })?;
    Ok(values)
}

fn increment_with_limit(
    current: u64,
    maximum: u64,
    limit: LexLimit,
) -> Result<u64, LexResourceFailure> {
    let actual = current
        .checked_add(1)
        .ok_or(LexResourceFailure::LimitExceeded {
            limit,
            maximum,
            actual: u64::MAX,
        })?;
    if actual > maximum {
        return Err(LexResourceFailure::LimitExceeded {
            limit,
            maximum,
            actual,
        });
    }
    Ok(actual)
}

fn raw_issue(source: &SourceBundle, source_id: SourceId, issue: RawIssue) -> LexOutcome {
    match produced_span(source, source_id, issue.start, issue.end) {
        Ok(span) => LexOutcome::SourceIssue(SourceIssue::new(span, issue.kind)),
        Err(failure) => LexOutcome::CompilerFailure(failure),
    }
}

fn produced_span(
    source: &SourceBundle,
    source_id: SourceId,
    start: usize,
    end: usize,
) -> Result<SourceSpan, LexCompilerFailure> {
    let start = u64::try_from(start).map(ByteOffset::new).map_err(|_| {
        LexCompilerFailure::InvalidProducedSpan {
            source: source_id,
            start: ByteOffset::new(u64::MAX),
            end: ByteOffset::new(u64::MAX),
        }
    })?;
    let end = u64::try_from(end).map(ByteOffset::new).map_err(|_| {
        LexCompilerFailure::InvalidProducedSpan {
            source: source_id,
            start,
            end: ByteOffset::new(u64::MAX),
        }
    })?;
    source
        .span(source_id, start, end)
        .map_err(|_| LexCompilerFailure::InvalidProducedSpan {
            source: source_id,
            start,
            end,
        })
}

fn take_while(bytes: &[u8], mut cursor: usize, predicate: fn(&u8) -> bool) -> usize {
    while bytes.get(cursor).is_some_and(predicate) {
        cursor += 1;
    }
    cursor
}

fn is_lower_continuation(byte: &u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_'
}

fn is_suffix_continuation(byte: &u8) -> bool {
    byte.is_ascii_alphanumeric() || *byte == b'_'
}

fn is_number_candidate_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.')
}

fn operation_name_end(bytes: &[u8], base_end: usize) -> Option<usize> {
    if bytes.get(base_end) != Some(&b'.') {
        return None;
    }
    let suffix_start = base_end.checked_add(1)?;
    for suffix in [
        b"checked".as_slice(),
        b"defined".as_slice(),
        b"strict".as_slice(),
        b"wrap".as_slice(),
        b"sat".as_slice(),
    ] {
        let end = suffix_start.checked_add(suffix.len())?;
        if bytes.get(suffix_start..end) == Some(suffix)
            && !bytes.get(end).is_some_and(is_suffix_continuation)
        {
            return Some(end);
        }
    }
    None
}

fn utf8_scalar_len(bytes: &[u8]) -> Option<usize> {
    let maximum = bytes.len().min(4);
    for length in 2..=maximum {
        let Some(candidate) = bytes.get(..length) else {
            continue;
        };
        if let Ok(text) = core::str::from_utf8(candidate) {
            let mut characters = text.chars();
            if characters.next().is_some() && characters.next().is_none() {
                return Some(length);
            }
        }
    }
    None
}
