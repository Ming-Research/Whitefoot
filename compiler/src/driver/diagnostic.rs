//! How one compiler stop is presented to the writer who has to act on it.
//!
//! A stop is one [`Record`]: an optional kind name, an optional location
//! resolved to the file the caller named, and the stage payload as labeled
//! fields. The driver adds the envelope it already owns -- category, stage,
//! and the numbered rule of a source rejection [DIAG-1] -- and renders the
//! record one of two ways.
//!
//! [`DiagnosticFormat::Text`] is lean: a summary line in the
//! `file:line:column: error[RULE]: Kind` shape, the source line with a marker
//! under the span, then one `label: value` line per payload field. Every
//! envelope fact a writer can act on is in the summary line; the category and
//! stage of a source rejection, and the byte interval, are not repeated there.
//! [`DiagnosticFormat::Json`] is complete: one object on one line carrying the
//! envelope, the location with its byte interval, and the same payload fields.
//!
//! Every payload type lists its fields once, here, by exhaustive
//! destructuring, and every closed classification it names lists its
//! variants. A field or variant added to a payload fails to compile until it
//! is listed, so no payload value can silently drop out of what a writer
//! reads, and every label is the field's own name. A node path reaches this
//! module only beside its coordinate and is printed as that coordinate's
//! position: child ordinals, source ordinals and byte offsets are identities a
//! writer cannot act on, and a writer trial recorded a writer running `head
//! -c` on their own program to decode them.
//!
//! Allocation target-layout stops retain a source position and a `reason`
//! naming the payload layout and violated target limit, with no language rule
//! or source repair. Other non-source stops -- resource ceilings, invocation
//! envelopes, internal invariants, and remaining target/backend failures --
//! keep their stage value's `Debug` text as one compiler-facing `payload` field.

use core::fmt::{self, Write as _};

use super::CompositionIssue;
use crate::source::SourceBundle;
use crate::syntax::terminal::TerminalPredicate;
use crate::{
    CallRequirementDisposition, CanonicalIssue, ContractShapeIssue, DeclarationClass,
    DeclarationConflict, DeclarationDomain, DeclarationOrigin, ExpectedTerminals, GraphIssue,
    GraphIssueKind, LexicalUseRole, LookaheadPredicate, LoopInvariantProofObligation,
    NonExhaustiveMatchSite, PostconditionProofDisposition, ReservedDeclarationRole,
    ReservedNameClass, ResolutionIssue, ResolutionIssueKind, SemanticIssue, SemanticIssueKind,
    SemanticLocation, SemanticUnsupported, SourceIssue, SourceIssueKind, SourceOrigin,
    SourceProofObligation, StaticObligationDisposition, SyntaxCoordinate, SyntaxIssue, SyntaxRepair,
    TerminalIssue, UndischargedCallRequirementDetail, UndischargedPostconditionDetail,
    UnsupportedSemanticFeature,
};

/// How a caller asks for compiler stops to be printed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DiagnosticFormat {
    /// A summary line in the `file:line:column: error[RULE]: Kind` shape, the
    /// marked source line, then one `label: value` line per payload field.
    #[default]
    Text,
    /// One JSON object on one line: the complete envelope, the location with
    /// its byte interval, and the same payload fields.
    Json,
}

/// Renders a stop the command-line driver reached outside compilation: an
/// invalid option, an unreadable source, an unwritable output, or a host
/// toolchain that failed.
///
/// These stops carry one sentence and no payload, so the text form stays
/// that sentence on one line; the JSON form gives it the same envelope every
/// compilation stop has, with stage `Driver`.
#[must_use]
pub fn render_driver_failure(category: &str, message: &str, format: DiagnosticFormat) -> String {
    match format {
        DiagnosticFormat::Text => format!("whitefootc: {message}"),
        DiagnosticFormat::Json => Record {
            kind: None,
            at: None,
            detail: vec![("message", Value::Text(message.to_owned()))],
            expected: None,
        }
        .json(&Head {
            verdict: String::new(),
            category,
            stage: "Driver",
            rule: None,
        }),
    }
}

/// Renders a note the command-line driver gives beside a result that is not a
/// stop: something a writer may want to know that did not change the verdict.
///
/// The text form is one `note:` line; the JSON form is the envelope of a driver
/// stop with the kind `Note`, so a reader that parses stops also reads it.
#[must_use]
pub fn render_driver_note(category: &str, message: &str, format: DiagnosticFormat) -> String {
    match format {
        DiagnosticFormat::Text => format!("whitefootc: note: {message}"),
        DiagnosticFormat::Json => Record {
            kind: Some("Note".to_owned()),
            at: None,
            detail: vec![("message", Value::Text(message.to_owned()))],
            expected: None,
        }
        .json(&Head {
            verdict: String::new(),
            category,
            stage: "Driver",
            rule: None,
        }),
    }
}

/// The envelope the driver owns for one record.
pub(super) struct Head<'envelope> {
    /// The summary line's verdict: `error[RULE]` for a source rejection, and
    /// the category and stage in words for every other stop.
    pub(super) verdict: String,
    pub(super) category: &'envelope str,
    pub(super) stage: &'envelope str,
    pub(super) rule: Option<&'envelope str>,
}

/// One compiler stop as labeled data, independent of its rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Record {
    kind: Option<String>,
    at: Option<Place>,
    detail: Vec<(&'static str, Value)>,
    expected: Option<Vec<String>>,
}

impl Record {
    /// A rejection or capability stop whose payload lists its own fields,
    /// located at the coordinate its rule selected.
    pub(super) fn located<Issue: Report + ?Sized>(
        issue: &Issue,
        bundle: &SourceBundle,
        coordinate: SyntaxCoordinate,
        anchor: Anchor,
    ) -> Self {
        Self::placed(issue, bundle, Place::resolve(bundle, coordinate, anchor))
    }

    /// A rejection whose payload lists its own fields, at a place already
    /// resolved from another record, such as a graph entry's line, or at
    /// none when nothing written names it [MOD-9, STOR-8].
    pub(super) fn placed<Issue: Report + ?Sized>(
        issue: &Issue,
        bundle: &SourceBundle,
        at: Option<Place>,
    ) -> Self {
        let mut fields = Fields {
            bundle,
            detail: Vec::new(),
        };
        let kind = issue.report(&mut fields);
        Self {
            kind: Some(kind.to_owned()),
            at,
            detail: fields.detail,
            expected: issue.expected_terminals().map(|expected| {
                expected
                    .iter()
                    .map(|predicate| match predicate {
                        LookaheadPredicate::Terminal(terminal) => terminal.spelling().to_owned(),
                        LookaheadPredicate::SourceEnd => "end of source".to_owned(),
                    })
                    .collect()
            }),
        }
    }

    /// The file, line and column the record is located at, as the summary
    /// line prints them.
    pub(super) fn location(&self) -> Option<super::SourceLocation> {
        self.at.as_ref().map(|place| super::SourceLocation {
            path: place.file.clone(),
            line: place.line,
            column: place.column,
        })
    }

    /// A stop whose payload is a compiler-facing stage value.
    ///
    /// Its kind is the value's leading identifier, which for a derived
    /// `Debug` is the variant or type name.
    pub(super) fn opaque(payload: &dyn fmt::Debug) -> Self {
        let payload = format!("{payload:?}");
        let name_end = payload
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(payload.len());
        let kind = payload[..name_end].to_owned();
        Self {
            kind: (!kind.is_empty()).then_some(kind),
            at: None,
            detail: vec![("payload", Value::Text(payload))],
            expected: None,
        }
    }

    /// The grammar-selected terminal labels retained independently of rendering.
    pub(super) fn expected_terminals(&self) -> Option<&[String]> {
        self.expected.as_deref()
    }

    /// Rebinds a segment-local record to its original source coordinate.
    pub(super) fn relocate(&mut self, bundle: &SourceBundle, coordinate: SyntaxCoordinate) {
        self.at = Place::resolve(bundle, coordinate, Anchor::Start);
    }

    /// A target-stage stop may still name the checked source allocation that
    /// requires the failed representation, without claiming a language rule.
    pub(super) fn target_layout(
        message: String,
        bundle: &SourceBundle,
        coordinate: Option<SyntaxCoordinate>,
    ) -> Self {
        Self {
            kind: Some("TargetLayout".to_owned()),
            at: coordinate.and_then(|at| Place::resolve(bundle, at, Anchor::Start)),
            detail: vec![("reason", Value::Text(message))],
            expected: None,
        }
    }

    /// The payload fields in the text rendering, one `label: value` per line.
    pub(super) fn detail_text(&self) -> String {
        let mut text = String::new();
        for (index, (label, value)) in self.detail.iter().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            push_field(&mut text, label, value);
        }
        text
    }

    /// The lean text block: the summary line, the marked source line, then
    /// one indented line per payload field.
    pub(super) fn text(&self, head: &Head<'_>) -> String {
        let mut text = String::new();
        match &self.at {
            Some(place) => push_position(&mut text, place),
            None => text.push_str("whitefootc"),
        }
        text.push_str(": ");
        text.push_str(&head.verdict);
        if let Some(kind) = &self.kind {
            text.push_str(": ");
            text.push_str(kind);
        }
        if let Some(place) = &self.at {
            // `source:` and `marker:` are the same width, so the marker's
            // carets sit under the characters the coordinate names.
            text.push_str("\n  source: ");
            text.push_str(&place.line_text());
            text.push_str("\n  marker: ");
            let (offset, width) = place.marker();
            text.extend(core::iter::repeat_n(' ', offset));
            text.extend(core::iter::repeat_n('^', width));
        }
        for (label, value) in &self.detail {
            text.push_str("\n  ");
            push_field(&mut text, label, value);
        }
        text
    }

    /// One JSON object on one line: the complete envelope in the order
    /// `rule`, `kind`, `category`, `stage`, then `at`, `bytes`, `source` and
    /// the payload fields under `detail`.
    ///
    /// Strings carry the text rendering's spelling, escapes included, so an
    /// exact value reads the same in both forms. The marker is omitted: it is
    /// the text rendering of `at` and `bytes`, which the object carries.
    pub(super) fn json(&self, head: &Head<'_>) -> String {
        let mut envelope = Vec::with_capacity(4);
        if let Some(rule) = head.rule {
            envelope.push(("rule", rule));
        }
        if let Some(kind) = &self.kind {
            envelope.push(("kind", kind.as_str()));
        }
        envelope.push(("category", head.category));
        envelope.push(("stage", head.stage));
        let mut json = String::from("{");
        for (index, (key, value)) in envelope.into_iter().enumerate() {
            push_json_key(&mut json, key, index == 0);
            push_json_string(&mut json, value);
        }
        if let Some(place) = &self.at {
            json.push(',');
            push_json_position(&mut json, place);
            push_json_key(&mut json, "source", false);
            push_json_string(&mut json, &place.line_text());
        }
        push_json_key(&mut json, "detail", false);
        push_json_fields(&mut json, &self.detail);
        json.push('}');
        json
    }
}

impl Record {
    /// The kind name the summary line prints, when the stop has one.
    pub(super) fn kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }

    /// What is wrong with the words of this record: a `?` standing for a
    /// value the renderer could not write, a stretch the text marks as
    /// source with backticks that is not source, and a field that holds a
    /// source relation, place or callee that is not written in source terms.
    /// A stretch is source when the lexer reads it, bare or, where it begins
    /// with a backslash, as the body of a character literal (the spelling of
    /// an escape), or when the writer wrote it on the line the record
    /// quotes; a description in a stretch or a field is read as a name.
    /// Exact values are the writer's own source bytes and are not read; the
    /// marker line is not part of the payload.
    pub(super) fn text_defects(&self) -> Vec<String> {
        let written = self.at.as_ref().map(Place::line_text).unwrap_or_default();
        let mut defects = Vec::new();
        for (label, value) in &self.detail {
            value_defects(label, value, &written, &mut defects);
        }
        defects
    }
}

fn value_defects(label: &str, value: &Value, written: &str, defects: &mut Vec<String>) {
    match value {
        Value::Text(text) => text_defects(label, text, written, defects),
        Value::Source(text) => source_defects(label, text, defects),
        Value::Separation(text) => separation_defects(label, text, defects),
        Value::List(items) => {
            for item in items {
                value_defects(label, item, written, defects);
            }
        }
        Value::MatchSite {
            enum_name,
            missing_variants,
            ..
        } => {
            text_defects(label, enum_name, written, defects);
            for variant in missing_variants {
                value_defects(label, variant, written, defects);
            }
        }
        Value::Fields(fields) | Value::Variant(_, fields) => {
            for (inner, value) in fields {
                value_defects(inner, value, written, defects);
            }
        }
        Value::Exact(_) | Value::Number(_) | Value::Place(..) => {}
    }
}

fn text_defects(label: &str, text: &str, written: &str, defects: &mut Vec<String>) {
    if text.contains('?') {
        defects.push(format!("{label}: `?` stands for a value the text does not write: {text}"));
    }
    // The odd-numbered pieces between backticks are the stretches the text
    // marks as source; a last piece with no closing backtick is not one.
    let mut pieces = text.split('`').peekable();
    let mut inside = false;
    while let Some(piece) = pieces.next() {
        if inside && pieces.peek().is_some() && !piece.is_empty() {
            // A described value is not source, so a stretch that holds one is
            // read with each description replaced by a name.
            match named_parts(piece) {
                None => defects.push(format!(
                    "{label}: `{piece}` holds a description that is not closed"
                )),
                Some(plain) => {
                    if !written.contains(piece)
                        && !lexes_as_source(&plain)
                        && !(plain.starts_with('\\') && lexes_as_source(&format!("'{plain}'")))
                    {
                        defects.push(format!(
                            "{label}: `{piece}` is marked as source but is not source text"
                        ));
                    }
                }
            }
        }
        inside = !inside;
    }
}

/// What a description stands for while the source around it is read.
const DESCRIBED_NAME: &str = "described";

/// What is wrong with a field that holds a source relation, place or callee:
/// a `?`, and a value that is not source once each description in it is a
/// name. It is source when the lexer reads all of it and no two words, names
/// or literals stand side by side where the grammar writes none [GRAM-5], so
/// a place written as a phrase in words is a defect.
fn source_defects(label: &str, text: &str, defects: &mut Vec<String>) {
    if text.contains('?') {
        defects.push(format!("{label}: `?` stands for a value the text does not write: {text}"));
        return;
    }
    let Some(plain) = named_parts(text) else {
        defects.push(format!("{label}: a description is not closed: {text}"));
        return;
    };
    if !is_source_relation(&plain) {
        defects.push(format!("{label}: is not written in source terms: {text}"));
    }
}

/// What is wrong with the residual of a call separation: a `?`, a sentence
/// that is none of the forms the renderer writes, and a path that is not
/// written in source terms once each description is a name.
fn separation_defects(label: &str, text: &str, defects: &mut Vec<String>) {
    if text.contains('?') {
        defects.push(format!("{label}: `?` stands for a value the text does not write: {text}"));
        return;
    }
    let Some(plain) = named_parts(text) else {
        defects.push(format!("{label}: a description is not closed: {text}"));
        return;
    };
    let Some((left, right)) = crate::diagnostic_repairs::separation_paths(&plain) else {
        defects.push(format!("{label}: is none of the forms of a call separation: {text}"));
        return;
    };
    for path in [left, right] {
        if !is_source_relation(path) {
            defects.push(format!("{label}: the path `{path}` is not written in source terms: {text}"));
        }
    }
}

/// The text with each description and each specified call-argument datum
/// replaced by a name, so the source around them can be read as source; `None`
/// where a description is not closed. A description is one value, so its name
/// stands apart from what is written beside it: two descriptions, or a
/// description and a name, side by side stay two names and are not read as one.
fn named_parts(text: &str) -> Option<String> {
    let described = crate::diagnostic_repairs::replace_described(text, " described ")?;
    Some(crate::diagnostic_repairs::replace_call_argument_datums(&described, DESCRIBED_NAME))
}

/// Whether `text` is one run of source tokens in which a word, name or
/// literal never follows another with nothing between them, apart from the
/// words [GRAM-5] puts before or after one.
fn is_source_relation(text: &str) -> bool {
    let Some(lexed) = lex_fragment(text) else {
        return false;
    };
    let mut previous: Option<&[u8]> = None;
    for lexeme in lexed.lexemes() {
        let crate::Lexeme::Token(token) = lexeme else {
            // Trivia between two words does not make them one phrase.
            continue;
        };
        let Some(spelling) = lexed.source_bundle().span_bytes(token.span()) else {
            return false;
        };
        let word = matches!(
            token.kind(),
            crate::TokenKind::LowerWordForm
                | crate::TokenKind::UpperWordForm
                | crate::TokenKind::LabelForm
                | crate::TokenKind::OperationNameForm
                | crate::TokenKind::NumberForm
                | crate::TokenKind::CharacterForm
                | crate::TokenKind::StringForm
        );
        if word {
            if previous.is_some_and(|before| !joins_words(before, spelling)) {
                return false;
            }
            previous = Some(spelling);
        } else {
            previous = None;
        }
    }
    true
}

/// Whether the grammar writes `second` right after the word `first`: `move`
/// before a place, `fn` before a function argument of a type-argument list,
/// `unchanged` after a relation's left side, and the range clause words.
fn joins_words(first: &[u8], second: &[u8]) -> bool {
    const BEFORE: &[&[u8]] = &[
        b"move", b"not", b"is", b"in", b"when", b"forall", b"spawn", b"musttail", b"fn",
    ];
    BEFORE.contains(&first) || second == b"unchanged" || second == b"is" || second == b"in"
}

/// The lexical partition of `fragment`, when the lexer reads all of it.
fn lex_fragment(fragment: &str) -> Option<crate::LexedBundle> {
    let inputs = [crate::SourceInput::new("fragment.wf", fragment.as_bytes())];
    let bundle = SourceBundle::with_limits(&inputs, crate::SourceLimits::REPRESENTABLE).ok()?;
    let limits = crate::LexLimits {
        max_sources: u32::MAX,
        max_source_bytes: u64::MAX,
        max_total_source_bytes: u64::MAX,
        max_token_bytes: u64::MAX,
        max_tokens: u64::MAX,
        max_lexemes: u64::MAX,
    };
    match crate::lex(&bundle, limits) {
        crate::LexOutcome::Complete(lexed) => Some(lexed),
        _ => None,
    }
}

/// Whether the lexer reads `fragment` as one complete run of source tokens.
fn lexes_as_source(fragment: &str) -> bool {
    lex_fragment(fragment).is_some()
}

/// Which byte of a coordinate the reader is sent to.
#[derive(Clone, Copy)]
pub(super) enum Anchor {
    /// The coordinate's first byte: a written construct starts where it starts.
    Start,
    /// The first byte of a trivia gap that lies on the line the gap ends in.
    ///
    /// A gap that carries a line break begins at the end of the line before
    /// the one the writer must edit, so anchoring at its start quoted the
    /// enclosing item's header while the offending bytes sat lines below.
    LastLineOfGap,
}

/// One source coordinate in the terms the caller typed.
///
/// The file is the display path, so a record names the file the caller
/// named; a compiler-supplied prelude declaration, which exists in no file a
/// writer can open, is named `<prelude>/...`. The line is one-based. The
/// column is one-based and counts characters -- a byte that begins no valid
/// scalar counts as one -- and the marker is drawn over the printed line, so
/// both agree with what a terminal shows. The byte interval stays exact in
/// `start` and `end`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Place {
    file: String,
    line: u64,
    column: u64,
    start: u64,
    end: u64,
    /// The complete source line holding the anchor, as raw bytes.
    bytes: Vec<u8>,
    /// Bytes of `bytes` before the anchor.
    anchor: usize,
    /// Byte of `bytes` where the coordinate's part of this line ends.
    covered_end: usize,
}

impl Place {
    /// Resolves one coordinate, or `None` when the bundle does not hold it.
    ///
    /// This is presentation: a stage that already has a verdict must still
    /// deliver it, so an unresolvable coordinate leaves the record unlocated
    /// rather than failing the compilation.
    pub(super) fn resolve(
        bundle: &SourceBundle,
        coordinate: SyntaxCoordinate,
        anchor: Anchor,
    ) -> Option<Self> {
        let file = bundle.file(coordinate.source())?;
        let bytes = file.bytes();
        let start = usize::try_from(coordinate.start().value())
            .ok()?
            .min(bytes.len());
        let end = usize::try_from(coordinate.end().value())
            .ok()?
            .min(bytes.len())
            .max(start);
        let line_start_before = |offset: usize| {
            bytes[..offset]
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map_or(0, |index| index.saturating_add(1))
        };
        let anchored = match anchor {
            Anchor::Start => start,
            Anchor::LastLineOfGap => start.max(line_start_before(end)),
        };
        let line_start = line_start_before(anchored);
        let line_end = bytes[line_start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| line_start.saturating_add(offset));
        let line = bytes[..line_start]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            .saturating_add(1);
        let before = source_characters(&bytes[line_start..anchored]);
        let display_path = file.display_path();
        let file_name = match file.prelude() {
            Some(_) => format!(
                "<prelude>/{}",
                display_path
                    .strip_prefix("prelude/")
                    .unwrap_or(display_path)
            ),
            None => display_path.to_owned(),
        };
        Some(Self {
            file: file_name,
            line: u64::try_from(line).ok()?,
            column: u64::try_from(before.saturating_add(1)).ok()?,
            start: coordinate.start().value(),
            end: coordinate.end().value(),
            bytes: bytes[line_start..line_end].to_vec(),
            anchor: anchored.saturating_sub(line_start),
            covered_end: end.min(line_end).max(anchored).saturating_sub(line_start),
        })
    }

    /// The position, byte interval and line a rendering of this place
    /// prints, as the bytes a recorded rendering is read against.
    pub(crate) fn reading(&self) -> Vec<u8> {
        let mut reading = format!(
            "{}:{}:{} {}..{}\n",
            self.file, self.line, self.column, self.start, self.end
        )
        .into_bytes();
        reading.extend_from_slice(&self.bytes);
        reading
    }

    /// The whole line, printed as written with invisible and reordering
    /// characters escaped.
    fn line_text(&self) -> String {
        let mut text = String::new();
        push_source(&mut text, &self.bytes);
        text
    }

    /// The marker's offset and width under [`Place::line_text`]; a zero-width
    /// coordinate is one caret at its position.
    fn marker(&self) -> (usize, usize) {
        let width = |bytes: &[u8]| {
            let mut text = String::new();
            push_source(&mut text, bytes);
            text.chars().count()
        };
        (
            width(&self.bytes[..self.anchor]),
            width(&self.bytes[self.anchor..self.covered_end]).max(1),
        )
    }
}

/// A position a payload names, quoted by the node it belongs to.
///
/// One rule serves every related position: `at` is where the payload's
/// coordinate points, and the quote is the first line of `node`, the
/// production that coordinate belongs to, without surrounding blanks. For a
/// clause or selector the two are the same node; for a declaration origin,
/// `at` is the declared name and `node` the declaration, so the quote shows
/// what the other declaration is, not the name the record already spells.
struct Related {
    at: SyntaxCoordinate,
    node: SyntaxCoordinate,
}

impl FieldValue for Related {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        let Some(place) = Place::resolve(bundle, self.at, Anchor::Start) else {
            return Some(Value::Text("an unresolved coordinate".to_owned()));
        };
        let quote = bundle
            .file(self.node.source())
            .and_then(|file| {
                let start = usize::try_from(self.node.start().value()).ok()?;
                let end = usize::try_from(self.node.end().value()).ok()?;
                let extent = file.bytes().get(start..end)?;
                let first_line = extent
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(extent, |line_end| &extent[..line_end]);
                Some(first_line.trim_ascii().to_vec())
            })
            .unwrap_or_default();
        Some(Value::Place(Box::new(place), quote))
    }
}

/// Characters as the column counts them: each scalar, and each byte that
/// begins no valid scalar.
fn source_characters(bytes: &[u8]) -> usize {
    bytes
        .utf8_chunks()
        .map(|chunk| {
            chunk
                .valid()
                .chars()
                .count()
                .saturating_add(chunk.invalid().len())
        })
        .sum()
}

/// One field value, typed by how a reader must read it.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Value {
    /// Prose, a name, or a rendered source relation: quoted in a list and
    /// otherwise printed as written.
    Text(String),
    /// A relation, place or callee written in source terms, where a part with
    /// no source spelling is a description between the marks of
    /// `described`: printed as `Text` is, and read by the text check as
    /// source with each description replaced by a name.
    Source(String),
    /// The residual of a call separation: two paths of source terms joined by
    /// one of the fixed sentence forms of `SeparationForm`; printed as `Text`
    /// is, and read by the text check path by path.
    Separation(String),
    /// Exact source bytes whose spacing or escapes matter: printed quoted.
    Exact(Vec<u8>),
    Number(u64),
    List(Vec<Value>),
    /// Another source position the payload names, with the first line of
    /// the node it belongs to.
    Place(Box<Place>, Vec<u8>),
    /// One ERR-2 edit site: compact in text, complete in JSON.
    MatchSite {
        location: Option<Box<Place>>,
        enum_name: String,
        missing_variants: Vec<Value>,
    },
    /// A structured payload element, such as one declaration conflict.
    Fields(Vec<(&'static str, Value)>),
    /// A classification variant that carries fields of its own.
    Variant(&'static str, Vec<(&'static str, Value)>),
}

/// The fields one payload contributes, in declaration order.
pub(super) struct Fields<'bundle> {
    bundle: &'bundle SourceBundle,
    detail: Vec<(&'static str, Value)>,
}

impl Fields<'_> {
    /// Adds one field; an absent optional value adds nothing.
    fn field<Field: FieldValue + ?Sized>(&mut self, label: &'static str, value: &Field) {
        if let Some(value) = value.value(self.bundle) {
            self.detail.push((label, value));
        }
    }
}

/// A payload that lists its own fields and names its own kind.
pub(super) trait Report {
    /// Adds every payload field to `fields` and returns the kind name.
    fn report(&self, fields: &mut Fields<'_>) -> &'static str;

    /// Grammar diagnostics retain their typed set for recorded verdicts.
    fn expected_terminals(&self) -> Option<ExpectedTerminals> {
        None
    }
}

/// A payload structure whose fields flatten into the record that holds it.
trait FieldList {
    fn fields(&self, fields: &mut Fields<'_>);
}

/// Lists every variant of one payload enum with its fields.
///
/// `Variant { a, b }` adds fields `a` and `b`; `Variant(name)` adds its one
/// value as field `name`; `Variant[detail]` flattens a detail structure's
/// fields into the record. The match has no wildcard and the patterns have no
/// rest, so the listing cannot fall behind the enum. It evaluates to the
/// variant name.
macro_rules! report_variants {
    ($enum:ident, $value:expr, $fields:ident;
     $( $variant:ident
        $( { $($field:ident $(: $how:ident)?),* $(,)? } )?
        $( ( $single:ident ) )?
        $( [ $flat:ident ] )?
     ; )*
    ) => {
        match $value {
            $(
                $enum::$variant $( { $($field),* } )? $( ( $single ) )? $( ( $flat ) )? => {
                    $( $( report_field!($fields, $field $(, $how)?); )* )?
                    $( $fields.field(stringify!($single), $single); )?
                    $( FieldList::fields(&**$flat, $fields); )?
                    stringify!($variant)
                }
            )*
        }
    };
}

/// Defines one classification's report function and the list of its variant
/// names from one listing, so the list of rejection kinds cannot fall behind
/// the variants a record can print. The listing has the form of
/// [`report_variants!`].
macro_rules! report_kind_fn {
    ($report:ident, $names:ident, $enum:ident;
     $( $variant:ident
        $( { $($field:ident $(: $how:ident)?),* $(,)? } )?
        $( ( $single:ident ) )?
        $( [ $flat:ident ] )?
     ; )*
    ) => {
        /// The variant names this classification's records print.
        pub(crate) const $names: &[&str] = &[ $( stringify!($variant) ),* ];

        fn $report(value: &$enum, fields: &mut Fields<'_>) -> &'static str {
            report_variants!($enum, value, fields;
                $( $variant
                    $( { $($field $(: $how)?),* } )?
                    $( ( $single ) )?
                    $( [ $flat ] )?
                ; )*
            )
        }
    };
}

/// Lists every field of one payload structure, in declaration order.
macro_rules! list_fields {
    ($value:expr, $fields:ident; $name:ident { $($field:ident $(: $how:ident)?),* $(,)? }) => {{
        let $name { $($field),* } = $value;
        $( report_field!($fields, $field $(, $how)?); )*
    }};
}

/// Adds one field to the record; `source` marks a field that holds a source
/// relation, place or callee, which the text check reads as source.
macro_rules! report_field {
    ($fields:ident, $field:ident) => {
        $fields.field(stringify!($field), $field)
    };
    ($fields:ident, $field:ident, source) => {
        $fields.field(stringify!($field), &Relation($field))
    };
    ($fields:ident, $field:ident, separation) => {
        $fields.field(stringify!($field), &Separation($field))
    };
}

/// One payload value as a [`Value`].
trait FieldValue {
    fn value(&self, bundle: &SourceBundle) -> Option<Value>;
}

impl<Field: FieldValue + ?Sized> FieldValue for &Field {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        (**self).value(bundle)
    }
}

impl<Field: FieldValue + ?Sized> FieldValue for Box<Field> {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        (**self).value(bundle)
    }
}

impl<Field: FieldValue> FieldValue for Option<Field> {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        self.as_ref().and_then(|value| value.value(bundle))
    }
}

impl<Field: FieldValue> FieldValue for [Field] {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        Some(Value::List(
            self.iter().filter_map(|item| item.value(bundle)).collect(),
        ))
    }
}

impl<Field: FieldValue> FieldValue for Vec<Field> {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        self.as_slice().value(bundle)
    }
}

impl FieldValue for str {
    fn value(&self, _: &SourceBundle) -> Option<Value> {
        Some(Value::Text(self.to_owned()))
    }
}

/// A payload field that holds a source relation, place or callee: its value
/// is read as source with each description replaced by a name.
struct Relation<'field, Field: ?Sized>(&'field Field);

impl<Field: FieldValue + ?Sized> FieldValue for Relation<'_, Field> {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        self.0.value(bundle).map(into_source)
    }
}

/// A payload field that holds the residual of a call separation.
struct Separation<'field, Field: ?Sized>(&'field Field);

impl<Field: FieldValue + ?Sized> FieldValue for Separation<'_, Field> {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        self.0.value(bundle).map(|value| match value {
            Value::Text(text) => Value::Separation(text),
            other => other,
        })
    }
}

/// `value` with each text in it read as a source relation.
fn into_source(value: Value) -> Value {
    match value {
        Value::Text(text) => Value::Source(text),
        Value::List(items) => Value::List(items.into_iter().map(into_source).collect()),
        other => other,
    }
}

impl FieldValue for String {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        self.as_str().value(bundle)
    }
}

impl FieldValue for u16 {
    fn value(&self, _: &SourceBundle) -> Option<Value> {
        Some(Value::Number(u64::from(*self)))
    }
}

impl FieldValue for u32 {
    fn value(&self, _: &SourceBundle) -> Option<Value> {
        Some(Value::Number(u64::from(*self)))
    }
}

impl FieldValue for u64 {
    fn value(&self, _: &SourceBundle) -> Option<Value> {
        Some(Value::Number(*self))
    }
}

impl FieldValue for i128 {
    fn value(&self, _: &SourceBundle) -> Option<Value> {
        // The mathematical integer domain includes signed values and u64's
        // maximum; textual decimal preserves both in the diagnostic format.
        Some(Value::Text(self.to_string()))
    }
}

/// Exact bytes that must be read quoted, such as trivia a canonical-form
/// rejection wanted and found.
struct Exact<'text>(&'text str);

impl FieldValue for Exact<'_> {
    fn value(&self, _: &SourceBundle) -> Option<Value> {
        Some(Value::Exact(self.0.as_bytes().to_vec()))
    }
}

/// The source bytes one coordinate covers, which a lexical, terminal or
/// grammar rejection found where it expected something else.
struct Spelled(SyntaxCoordinate);

impl FieldValue for Spelled {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        let bytes = bundle.file(self.0.source())?.bytes();
        let start = usize::try_from(self.0.start().value()).ok()?;
        let end = usize::try_from(self.0.end().value()).ok()?;
        if start == end && end == bytes.len() {
            return Some(Value::Text("end of source".to_owned()));
        }
        Some(Value::Exact(bytes.get(start..end)?.to_vec()))
    }
}

/// A grammar rejection's repair [DIAG-1]; one that names the source's own
/// spelling reads it from the source the rejection cites.
impl FieldValue for SyntaxRepair {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        let bytes = match self.source() {
            Some(source) => bundle.file(source)?.bytes(),
            None => &[],
        };
        self.sentence(bytes).map(Value::Text)
    }
}

/// A node location's coordinate is the node's complete extent.
impl FieldValue for SemanticLocation {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        Related {
            at: self.coordinate(),
            node: self.coordinate(),
        }
        .value(bundle)
    }
}

impl FieldValue for NonExhaustiveMatchSite {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        let Self {
            location,
            enum_name,
            missing_variants,
        } = self;
        Some(Value::MatchSite {
            location: Place::resolve(bundle, location.coordinate(), Anchor::Start).map(Box::new),
            enum_name: enum_name.clone(),
            missing_variants: missing_variants.iter().cloned().map(Value::Text).collect(),
        })
    }
}

/// An origin points at its role's spelling and belongs to its owning
/// production.
impl FieldValue for SourceOrigin {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        Related {
            at: self.coordinate(),
            node: self.extent(),
        }
        .value(bundle)
    }
}

impl FieldValue for DeclarationOrigin {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        match self {
            Self::Source(origin) => origin.value(bundle),
            // The colliding spelling is already in the record; the prelude
            // ordinal adds nothing a writer can act on.
            Self::Prelude(_) => Some(Value::Text("prelude".to_owned())),
        }
    }
}

impl FieldValue for DeclarationConflict {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        let mut fields = Fields {
            bundle,
            detail: Vec::new(),
        };
        list_fields!(self, fields; DeclarationConflict { domain, class, origin });
        Some(Value::Fields(fields.detail))
    }
}

/// The expected set as a writer would write it: a fixed terminal quoted as
/// its exact spelling, a token class by its specification name, and the end
/// of the source in words.
impl FieldValue for ExpectedTerminals {
    fn value(&self, _: &SourceBundle) -> Option<Value> {
        Some(Value::List(
            self.iter()
                .map(|predicate| match predicate {
                    LookaheadPredicate::Terminal(TerminalPredicate::Fixed(terminal)) => {
                        Value::Exact(terminal.spelling_bytes().to_vec())
                    }
                    LookaheadPredicate::Terminal(class) => Value::Text(class.spelling().to_owned()),
                    LookaheadPredicate::SourceEnd => Value::Text("end of source".to_owned()),
                })
                .collect(),
        ))
    }
}

/// The kind a formed token that satisfies no terminal predicate prints.
const UNCLASSIFIED_TOKEN: &str = "UnclassifiedToken";
/// The kind a grammar failure prints.
const UNEXPECTED_TOKEN: &str = "UnexpectedToken";
/// The kind a trivia gap that is not the canonical one prints.
const NON_CANONICAL_TRIVIA: &str = "NonCanonicalTrivia";

/// Every kind name a source rejection can print, once each and sorted: the
/// variants of each rejection classification and the kinds the lexical,
/// grammar and canonical-form rejections print. The lists come from the
/// listings that define what a record prints, so a kind added to a payload is
/// in this list when it compiles. A capability stop is not a rejection and is
/// not listed.
#[must_use]
pub fn rejection_kinds() -> Vec<&'static str> {
    let fixed: &[&'static str] = &[UNCLASSIFIED_TOKEN, UNEXPECTED_TOKEN, NON_CANONICAL_TRIVIA];
    let lists: [&[&'static str]; 6] = [
        GRAPH_KINDS,
        COMPOSITION_KINDS,
        SEMANTIC_KINDS,
        RESOLUTION_KINDS,
        <SourceIssueKind as VariantName>::NAMES,
        fixed,
    ];
    let mut kinds: Vec<&'static str> = lists.concat();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

/// A closed classification printed by its variant name.
trait VariantName {
    /// Every variant's name, in declaration order.
    const NAMES: &'static [&'static str];

    fn name(&self) -> &'static str;
}

/// Lists every variant of each fieldless classification. The match has no
/// wildcard and unit patterns, so a variant added to one, or given data,
/// fails to compile until it is listed here.
macro_rules! variant_names {
    ($($enum:ident { $($variant:ident),* $(,)? })*) => {
        $(
            impl VariantName for $enum {
                const NAMES: &'static [&'static str] = &[ $( stringify!($variant) ),* ];

                fn name(&self) -> &'static str {
                    match self {
                        $( $enum::$variant => stringify!($variant), )*
                    }
                }
            }

            impl FieldValue for $enum {
                fn value(&self, _: &SourceBundle) -> Option<Value> {
                    Some(Value::Text(self.name().to_owned()))
                }
            }
        )*
    };
}

variant_names! {
    CallRequirementDisposition { Refuted, Unproved }
    ContractShapeIssue { MissingClause }
    DeclarationClass {
        Function, FunctionParameter, NamedConst, ConstGeneric, Value, GenericType, NominalType,
        StructConstructor, EnumVariant, BuiltinBound, Interface, Binding, Label, Invariant,
        OperationFamily, Module,
    }
    DeclarationDomain { LexicalIdentifier, NominalType, Constructor, BuiltinBound, Label, Invariant }
    LexicalUseRole {
        Type, GenericBound, FormalGroup, TypeArgument, Construct, VariantOwner, EnsuresVariant,
        EffectRoot, EffectIndex, BreakLabel, Const, ConstValue, IntegerMatchValue, PlaceBase, IdentifierCallee,
        OperationCallee, FunctionBinding, GenericNumericSuffix, InvariantValue, ProofValue,
        InvariantFact,
    }
    LoopInvariantProofObligation { Base, Backedge }
    PostconditionProofDisposition { Refuted, Unproved }
    ReservedDeclarationRole {
        Function, NamedConst, Parameter, Let, ContractDefinition, ForBinder, AtomicBinder,
        MatchBinder, PlainResultSelector, VariantResultSelector, Field, VariantField,
    }
    ReservedNameClass { DotlessOperation, ModeWord }
    SourceIssueKind {
        InvalidUtf8, UnexpectedByte, MissingLabelName, UnterminatedText, InvalidTextByte,
        InvalidTextEscape, InvalidSourceByte, CommentPrefix,
    }
    StaticObligationDisposition { Refuted, Unproved }
    UnsupportedSemanticFeature {
        Generics, PreludeNominalValues, ReferenceFormation, CompositeValues,
        RecursiveNominalLayout, OwnershipJoin, OperationFamily,
        RangeArithmetic, RangeOrdinaryGoal, RangeOperationIntervals,
        RangeLoopNesting, RangeCallFootprint, PageLengthPostcondition,
    }
}

/// A failed certificate part: its variant, with the use ordinals or
/// capacities it carries as fields.
impl FieldValue for SourceProofObligation {
    fn value(&self, bundle: &SourceBundle) -> Option<Value> {
        let mut fields = Fields {
            bundle,
            detail: Vec::new(),
        };
        let name = report_variants!(SourceProofObligation, self, fields;
            Premise(use_index);
            Combination;
            RedundantUseBlock;
            RepeatedUse { first, repeated };
            UseCapacity { maximum, actual };
            CertificateArithmeticOverflow;
            CertificateFormationCapacity;
            InvalidUseFactor { use_index };
            NonlinearCertificateSum;
        );
        Some(Value::Variant(name, fields.detail))
    }
}

impl FieldList for UndischargedCallRequirementDetail {
    fn fields(&self, fields: &mut Fields<'_>) {
        list_fields!(self, fields; UndischargedCallRequirementDetail {
            concrete_callee: source,
            requires_clause,
            instantiated_goal: source,
            disposition,
            mechanical_fix,
        });
    }
}

impl FieldList for UndischargedPostconditionDetail {
    fn fields(&self, fields: &mut Fields<'_>) {
        list_fields!(self, fields; UndischargedPostconditionDetail {
            concrete_function: source,
            postcondition,
            conjunct,
            selector,
            relation: source,
            clause: source,
            disposition,
            mechanical_fix,
        });
    }
}

/// The rule and location are the envelope's; the kind carries the payload,
/// and a rejection in a concrete generic instance names the call that
/// requested it, while it stays located at the template [FN-2, MOD-8].
impl Report for SemanticIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let Self {
            rule: _,
            location: _,
            kind,
            request,
        } = self;
        let name = kind.report(fields);
        fields.field("requested_at", &request.map(|at| Related { at, node: at }));
        name
    }
}

report_kind_fn!(report_graph_kind, GRAPH_KINDS, GraphIssueKind;
    DuplicateModule { path };
    SelfDependency { path };
    DuplicateDependency { path };
    UnregisteredDependency { path };
    LaterDependency { path };
    DuplicateEntry { name };
    UnregisteredEntryModule { target };
    StandardPath { path };
    UnknownStandardModule { path };
    StandardPrefixInLibrary { path };
    BoundPath { path };
    InvalidLocation { location };
    DuplicateBindingName { name };
    DuplicatePackage { name };
    BindingCycle { name };
    UnboundPackage { path };
    UnknownBoundModule { path };
);

/// A refused graph binding, row or entry is located at its written binding
/// or path; the kind carries the payload [MOD-1, MOD-11].
impl Report for GraphIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let kind = self.kind();
        report_graph_kind(kind, fields)
    }
}

report_kind_fn!(report_composition_kind, COMPOSITION_KINDS, CompositionIssue;
    PendingDeclaration { declaration };
    EntryFunctionMissing { function };
    EntryFunctionPrivate { function };
    HeapInClosure { path };
);

/// A composition rejection names what the entry's composition judgment
/// found [MOD-8, MOD-9, STOR-8].
impl Report for CompositionIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        report_composition_kind(self, fields)
    }
}

report_kind_fn!(report_semantic_kind, SEMANTIC_KINDS, SemanticIssueKind;
    InvalidIntegerLiteral;
    InvalidFloatLiteral { canonical_spelling, mechanical_fix };
    InvalidTextItem { reason, mechanical_fix };
    NonAsciiByteCharacter { mechanical_fix };
    TextLengthMismatch { declared_length, byte_length, mechanical_fix };
    InvalidConstValue;
    InaccessibleField { field, reason };
    InaccessibleVariant { variant, reason };
    ConstantCycle { cycle };
    ConstEvalOverflow { operation };
    ConstArgumentOutOfRange { parameter_type, value };
    ConstRuntimeArithmeticMode { mechanical_fix };
    TypeMismatch { expected, found, mechanical_fix };
    BorrowOfDerivedMeasure { mechanical_fix };
    UnadmittedOperandShape { expected, mechanical_fix };
    BehaviorArgumentMismatch { expected, mechanical_fix };
    UnforwardedGroupApplication { mechanical_fix };
    ImmutableSetTarget;
    InvalidSetTarget { root_class, required_classes, mechanical_fix };
    ImmutableWrittenArgument { binding, mechanical_fix };
    LinearValueNotConsumed { binding, obligation, mechanical_fix };
    LinearValuePartiallyConsumed { obligation, residual, mechanical_fix };
    LinearityBoundMismatch { parameter, bound, argument, actual };
    LivenessJoinDisagreement { binding, live_predecessor, dead_predecessor, mechanical_fix };
    LinearAssignmentTarget { target_type, mechanical_fix };
    InvalidRestrictedTypePlacement { spelling, mechanical_fix };
    SwapOverCopyPlace { place_type, mechanical_fix };
    HeapTypeUnderNoHeap { spelling, mechanical_fix };
    InvalidElementMove { mechanical_fix };
    MoveThroughReference { mechanical_fix };
    ReservedPseudoField { spelling, mechanical_fix };
    ReadonlyWriteTarget { spelling, mechanical_fix };
    FrozenForbiddenPart { part, mechanical_fix };
    SharedStateHoldsOwnHandle { part, handle, mechanical_fix };
    FrozenContentConsume { mechanical_fix };
    MoveOfCopy { binding, ty, reason, mechanical_fix };
    BareAffineUse { binding, ty, reason, mechanical_fix };
    ContainerConstruction { nominal, mechanical_fix };
    OpaqueDestructuring { nominal, mechanical_fix };
    UseAfterMove { mechanical_fix };
    InvalidReferenceUse { binder, event, mechanical_fix };
    UnestablishedPayloadVariant { place: source, variant, mechanical_fix };
    ReferenceToReferenceVariable { mechanical_fix };
    EscapingReference { mechanical_fix };
    RangeOverRing { mechanical_fix };
    RangeOverEntries { mechanical_fix };
    MoveOutOfSlot { mechanical_fix };
    MissingDereference { mechanical_fix };
    RangeReferentUse { mechanical_fix };
    MoveOuterBindingInLoop { binding, mechanical_fix };
    InvalidOperation { operation, operand_types, mechanical_fix };
    InvalidEqualityType { mechanical_fix };
    InvalidPredicateCondition;
    RedundantCondition { residual, decided, facts, mechanical_fix };
    InvalidConditionalForm { mechanical_fix };
    UndischargedBoundsObligation { residual: source, disposition, mechanical_fix };
    UndischargedEmptyRunRelease { residual: source, disposition, mechanical_fix };
    UndischargedIntegerDomainObligation { residual: source, disposition, mechanical_fix };
    UndischargedConversionDomainObligation { residual: source, disposition, mechanical_fix };
    UndischargedRangeFormationObligation { residual: source, disposition, mechanical_fix };
    UndischargedCallSeparation { residual: separation, mechanical_fix };
    OverlappingCallEffects { first, second, mechanical_fix };
    AtomicUpdateReachesTargetPrefix { target, effect, mechanical_fix };
    UndischargedCallRequirement[detail];
    InvalidCountedEndpoint { mechanical_fix };
    BreakOutsideLoop { mechanical_fix };
    ContinueOutsideLoop { mechanical_fix };
    InvalidRangeClause { reason, mechanical_fix };
    UndischargedRangeFact { fact, site, missing, mechanical_fix };
    UndischargedApart { pair, mechanical_fix };
    InvalidInvariant { reason, mechanical_fix };
    InvalidTypeInvariant { reason, mechanical_fix };
    TypeInvariantWritableField { field, mechanical_fix };
    UndischargedTypeInvariant { type_invariant, instantiated_goal: source, disposition, mechanical_fix };
    UndischargedLoopInvariant { name, obligation, incoming_edge, required_relation: source, disposition, mechanical_fix };
    UndischargedLocalInvariant { name, disposition, mechanical_fix };
    InvalidSourceProof { reason, mechanical_fix };
    UndischargedSourceProof { name, obligation, mechanical_fix };
    ReturnMismatch;
    WaitingCallOutsideWaitingFunction { callee, context, mechanical_fix };
    AtomicTargetNotShared { found, mechanical_fix };
    WaitInsideAtomic { construct, mechanical_fix };
    TableOffsetNotKey { mechanical_fix };
    TableEntriesNotBorrowed { mechanical_fix };
    AtomicKeyNotBytes { found, mechanical_fix };
    AtomicKeyReadsTheState { mechanical_fix };
    AtomicBindingUnused { binding, mechanical_fix };
    AtomicGuardWrites { mechanical_fix };
    AtomicGuardDecisionCell { mechanical_fix };
    AtomicGuardReadsSelfChanging { handle, mechanical_fix };
    InvalidSpawn { condition };
    InvalidMusttail { condition, subject };
    PolymorphicRecursion { cycle, changed_argument, mechanical_fix };
    UnreachableStatement;
    FunctionFallthrough;
    InvalidRequires;
    InvalidHeldContract { reason, mechanical_fix };
    SnapshotResource { owned_path, mechanical_fix };
    InvalidPostconditionSelector { mechanical_fix };
    AmbiguousResultRoute { mechanical_fix };
    InvalidPostconditionFields { required_fields };
    PostconditionCandidateNotFresh { spelling, conflicts };
    PostconditionLocalShadowsResult { spelling, selector };
    InvalidPostconditionClause;
    InvalidPostconditionRelation;
    InvalidEntryFormer { mechanical_fix };
    ContradictoryPublishedRelations { relations, mechanical_fix };
    InvalidPostconditionReturn;
    NoSelectedNormalExit { residual, mechanical_fix };
    UndischargedPostcondition[detail];
    InvalidNamedArguments { callee, declared_parameters };
    DuplicateFieldLabel { label };
    InvalidConstructionFields { constructor, declared_fields };
    InvalidMatchFields { variant, declared_fields };
    ForeignMatchVariant;
    NonExhaustiveMatch { missing_variants, other_sites };
    DuplicateMatchVariant { variant, mechanical_fix };
    InvalidIntegerMatchArm { reason, mechanical_fix };
    DuplicateIntegerMatchValue { value, earlier, mechanical_fix };
    InvalidIntegerMatchCoverage { integer_type, domain, mechanical_fix };
    InvalidPropagation { enclosing_return_type, operand_type, mechanical_fix };
    InvalidGive;
    EmptyDeliverySet { binding, mechanical_fix };
    InvalidEffectRow { reason, mechanical_fix };
    SubsumedEffectEntry { entry, covering };
    EffectMismatch { expected_row, found_row, missing, extra, mechanical_fix };
);

impl Report for SemanticIssueKind {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        report_semantic_kind(self, fields)
    }
}

report_kind_fn!(report_resolution_kind, RESOLUTION_KINDS, ResolutionIssueKind;
    ContractShape(shape);
    MisplacedHeapDeclaration { admitted };
    ReservedName { spelling, declaration_role, class, inventory_ordinal, mechanical_fix };
    MatchBinderFreshness { spelling, paired_field, earlier_binder, arm_entry_conflicts };
    DeclarationCollision { spelling, conflicts, mechanical_fix };
    InvisibleUse { spelling, role, admissible, origins, mechanical_fix };
    NonEnclosingLabel { spelling, role, origins };
    ModuleProgramHeapDeclaration;
    MisplacedAlias;
    InvalidAliasTarget { spelling, target, reason };
    AliasNamesPackage { spelling };
    LibraryNamesItself { path };
    UnknownModule { path };
    MissingModuleEdge { from, to };
    QualifiedNameNotFound { spelling, module, role, mechanical_fix };
    PrivateDeclaration { spelling, module };
    UnknownOwnedVariant { spelling, reason };
    MisplacedPublication { reason };
    PrivateInPublicSignature { spelling };
    Correspondence { spelling, reason };
    UnresolvedUse { spelling, role, admissible, available };
    UndeclaredSetTarget { spelling, mechanical_fix };
);

/// The rule and origin are the envelope's; the kind carries the payload.
impl Report for ResolutionIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let Self {
            rule: _,
            origin: _,
            kind,
        } = self;
        if matches!(
            kind,
            ResolutionIssueKind::UnresolvedUse {
                role: LexicalUseRole::IntegerMatchValue,
                ..
            }
        ) {
            fields.field("mechanical_fix", "write one in-scope named const of the scrutinee's exact integer type, or an integer literal of that type");
        }
        report_resolution_kind(kind, fields)
    }
}

/// [DIAG-1]'s raw lexical defects: the kind is the defect shape, and the
/// offending bytes are printed escaped because they are often invisible.
impl Report for SourceIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let Self {
            span,
            kind,
            mechanical_fix,
        } = self;
        fields.field(
            "found",
            &Spelled(SyntaxCoordinate::new(
                span.source(),
                span.start(),
                span.end(),
            )),
        );
        fields.field("mechanical_fix", mechanical_fix);
        kind.name()
    }
}

/// A formed token that satisfies no terminal predicate [GRAM-1]; its owner
/// is the envelope's rule.
impl Report for TerminalIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let Self {
            token,
            owner: _,
            mechanical_fix,
        } = self;
        fields.field(
            "found",
            &Spelled(SyntaxCoordinate::new(
                token.source(),
                token.start(),
                token.end(),
            )),
        );
        fields.field("mechanical_fix", mechanical_fix);
        UNCLASSIFIED_TOKEN
    }
}

/// The grammar's expected-terminal set at the failure boundary and what the
/// source has there instead [DIAG-1].
impl Report for SyntaxIssue {
    fn expected_terminals(&self) -> Option<ExpectedTerminals> {
        Some(self.expected())
    }

    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let Self {
            rule: _,
            coordinate,
            expected,
            mechanical_fix,
        } = self;
        fields.field("expected", expected);
        fields.field("found", &Spelled(*coordinate));
        fields.field("mechanical_fix", mechanical_fix);
        UNEXPECTED_TOKEN
    }
}

/// The trivia bytes [FORM-2] requires between two terminals and the bytes the
/// source carries there.
impl Report for CanonicalIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let Self {
            location: _,
            expected,
            found,
            differences: _,
            difference_count: _,
        } = self;
        fields.field("expected", &Exact(expected));
        fields.field("found", &Exact(found));
        fields.field("mechanical_fix", &self.mechanical_fix());
        NON_CANONICAL_TRIVIA
    }
}

/// A valid source that needs a semantic family the compiler has not built;
/// the node is the record's location.
impl Report for SemanticUnsupported {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let Self { feature, node: _ } = self;
        if *feature == UnsupportedSemanticFeature::PageLengthPostcondition {
            fields.field("reason", &"not yet supported: a page's length in a postcondition");
        }
        feature.name()
    }
}

fn push_field(text: &mut String, label: &str, value: &Value) {
    text.push_str(label);
    text.push_str(": ");
    write_text(text, value);
}

fn push_position(text: &mut String, place: &Place) {
    let _ = write!(text, "{}:{}:{}", place.file, place.line, place.column);
}

fn push_fields(text: &mut String, fields: &[(&'static str, Value)]) {
    text.push('{');
    for (index, (label, value)) in fields.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        push_field(text, label, value);
    }
    text.push('}');
}

/// Text rendering of one value; a value never spans two lines.
fn write_text(text: &mut String, value: &Value) {
    match value {
        Value::Text(value) | Value::Source(value) | Value::Separation(value) => {
            push_printable(text, &crate::diagnostic_repairs::unmark_described(value));
        }
        Value::Exact(bytes) => {
            text.push('"');
            push_escaped(text, bytes);
            text.push('"');
        }
        Value::Number(number) => {
            let _ = write!(text, "{number}");
        }
        Value::List(items) => {
            text.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    text.push_str(", ");
                }
                if let Value::Text(value) | Value::Source(value) | Value::Separation(value) = item {
                    text.push('"');
                    push_escaped(
                        text,
                        crate::diagnostic_repairs::unmark_described(value).as_bytes(),
                    );
                    text.push('"');
                } else {
                    write_text(text, item);
                }
            }
            text.push(']');
        }
        // Another position a payload names: where it is, and the node it
        // belongs to -- the clause, selector or declaration itself.
        Value::Place(place, quote) => {
            push_position(text, place);
            text.push_str(" \"");
            push_escaped(text, quote);
            text.push('"');
        }
        Value::MatchSite {
            location,
            enum_name: _,
            missing_variants,
        } => {
            match location {
                Some(place) => push_position(text, place),
                None => text.push_str("an unresolved coordinate"),
            }
            text.push_str(" missing [");
            for (index, variant) in missing_variants.iter().enumerate() {
                if index > 0 {
                    text.push_str(", ");
                }
                write_text(text, variant);
            }
            text.push(']');
        }
        Value::Fields(fields) => push_fields(text, fields),
        Value::Variant(name, fields) => {
            text.push_str(name);
            if !fields.is_empty() {
                text.push(' ');
                push_fields(text, fields);
            }
        }
    }
}

/// Whether a character is invisible, breaks a line, or reorders the text
/// around it: a control character, a zero-width character or byte-order
/// mark, a line or paragraph separator, or a bidirectional formatting
/// character [Unicode UAX #9]. Any of them can make a quoted line read
/// differently from its bytes.
fn is_invisible(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{061c}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}'
        )
}

fn push_invisible(text: &mut String, character: char) {
    match character {
        '\n' => text.push_str("\\n"),
        '\r' => text.push_str("\\r"),
        '\t' => text.push_str("\\t"),
        ascii if ascii.is_ascii() => {
            let _ = write!(text, "\\x{:02x}", u32::from(ascii));
        }
        other => {
            let _ = write!(text, "\\u{{{:x}}}", u32::from(other));
        }
    }
}

/// Rendered prose, printed as written with invisible characters escaped so
/// one field stays one line.
fn push_printable(text: &mut String, value: &str) {
    for character in value.chars() {
        if is_invisible(character) {
            push_invisible(text, character);
        } else {
            text.push(character);
        }
    }
}

/// A source line as written: visible scalars unchanged, invisible or
/// reordering ones escaped, and a byte that begins no valid scalar as `\x..`.
fn push_source(text: &mut String, bytes: &[u8]) {
    for chunk in bytes.utf8_chunks() {
        push_printable(text, chunk.valid());
        for byte in chunk.invalid() {
            let _ = write!(text, "\\x{byte:02x}");
        }
    }
}

/// Exact bytes with every byte that is not printable ASCII escaped: a
/// non-ASCII scalar as `\u{...}` and a byte that begins no valid scalar as
/// `\x..`, so an invisible or confusable defect is visible. The text form
/// quotes this spelling, and JSON carries it unchanged.
fn push_escaped(text: &mut String, bytes: &[u8]) {
    for chunk in bytes.utf8_chunks() {
        for character in chunk.valid().chars() {
            match character {
                '"' => text.push_str("\\\""),
                '\\' => text.push_str("\\\\"),
                ' '..='~' => text.push(character),
                other => push_invisible(text, other),
            }
        }
        for byte in chunk.invalid() {
            let _ = write!(text, "\\x{byte:02x}");
        }
    }
}

fn push_json_key(json: &mut String, key: &str, first: bool) {
    if !first {
        json.push(',');
    }
    push_json_string(json, key);
    json.push(':');
}

/// A JSON string [RFC 8259]: quote, backslash and control characters are
/// escaped; every other scalar is written as UTF-8.
fn push_json_string(json: &mut String, value: &str) {
    json.push('"');
    for character in value.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            control if u32::from(control) < 0x20 || control == '\u{7f}' => {
                let _ = write!(json, "\\u{:04x}", u32::from(control));
            }
            other => json.push(other),
        }
    }
    json.push('"');
}

/// A position's `at` and `bytes` members.
fn push_json_position(json: &mut String, place: &Place) {
    push_json_key(json, "at", true);
    json.push('{');
    push_json_key(json, "file", true);
    push_json_string(json, &place.file);
    let _ = write!(
        json,
        ",\"line\":{},\"column\":{}}},\"bytes\":{{\"start\":{},\"end\":{}}}",
        place.line, place.column, place.start, place.end
    );
}

fn push_json_fields(json: &mut String, fields: &[(&'static str, Value)]) {
    json.push('{');
    for (index, (label, value)) in fields.iter().enumerate() {
        push_json_key(json, label, index == 0);
        push_json_value(json, value);
    }
    json.push('}');
}

fn push_json_value(json: &mut String, value: &Value) {
    match value {
        Value::Text(value) | Value::Source(value) | Value::Separation(value) => {
            let mut printable = String::new();
            push_printable(&mut printable, &crate::diagnostic_repairs::unmark_described(value));
            push_json_string(json, &printable);
        }
        Value::Exact(bytes) => {
            let mut escaped = String::new();
            push_escaped(&mut escaped, bytes);
            push_json_string(json, &escaped);
        }
        Value::Number(number) => {
            let _ = write!(json, "{number}");
        }
        Value::List(items) => {
            json.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                push_json_value(json, item);
            }
            json.push(']');
        }
        // A related position: `at`, `bytes`, and its node's first line as
        // `text`.
        Value::Place(place, quote) => {
            json.push('{');
            push_json_position(json, place);
            push_json_key(json, "text", false);
            let mut escaped = String::new();
            push_escaped(&mut escaped, quote);
            push_json_string(json, &escaped);
            json.push('}');
        }
        Value::MatchSite {
            location,
            enum_name,
            missing_variants,
        } => {
            json.push('{');
            push_json_key(json, "location", true);
            match location {
                Some(place) => {
                    json.push('{');
                    push_json_position(json, place);
                    json.push('}');
                }
                None => push_json_string(json, "an unresolved coordinate"),
            }
            push_json_key(json, "enum_name", false);
            push_json_string(json, enum_name);
            push_json_key(json, "missing_variants", false);
            json.push('[');
            for (index, variant) in missing_variants.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                push_json_value(json, variant);
            }
            json.push_str("]}");
        }
        Value::Fields(fields) => push_json_fields(json, fields),
        // A variant with fields is an object naming it as `kind`; one without
        // fields is its name.
        Value::Variant(name, fields) if fields.is_empty() => push_json_string(json, name),
        Value::Variant(name, fields) => {
            json.push('{');
            push_json_key(json, "kind", true);
            push_json_string(json, name);
            for (label, value) in fields {
                push_json_key(json, label, false);
                push_json_value(json, value);
            }
            json.push('}');
        }
    }
}

#[cfg(test)]
mod tests;
