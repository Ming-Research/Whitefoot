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
//! Stops that are not source rejections -- resource ceilings, invocation
//! envelopes, internal invariants, target layout, backend -- carry
//! compiler-facing payloads with no writer repair. They keep their stage
//! value's `Debug` text as one `payload` field.

use core::fmt::{self, Write as _};

use crate::source::SourceBundle;
use crate::syntax::terminal::TerminalPredicate;
use crate::{
    CallRequirementDisposition, CanonicalIssue, ContractShapeIssue, DeclarationClass,
    DeclarationConflict, DeclarationDomain, DeclarationOrigin, ExpectedTerminals, GraphIssue,
    GraphIssueKind, LexicalUseRole, LookaheadPredicate, LoopInvariantProofObligation,
    PostconditionProofDisposition, ReservedDeclarationRole, ReservedNameClass, ResolutionIssue,
    ResolutionIssueKind, SemanticIssue, SemanticIssueKind, SemanticLocation, SemanticUnsupported,
    SourceIssue, SourceIssueKind, SourceOrigin, SourceProofObligation, StaticObligationDisposition,
    SyntaxCoordinate, SyntaxIssue, TerminalIssue, UndischargedCallRequirementDetail,
    UndischargedPostconditionDetail, UnsupportedSemanticFeature,
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
    /// Prose, a name, or a rendered source relation: printed as written.
    Text(String),
    /// Exact source bytes whose spacing or escapes matter: printed quoted.
    Exact(Vec<u8>),
    Number(u64),
    List(Vec<Value>),
    /// Another source position the payload names, with the first line of
    /// the node it belongs to.
    Place(Box<Place>, Vec<u8>),
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
        $( { $($field:ident),* $(,)? } )?
        $( ( $single:ident ) )?
        $( [ $flat:ident ] )?
     ; )*
    ) => {
        match $value {
            $(
                $enum::$variant $( { $($field),* } )? $( ( $single ) )? $( ( $flat ) )? => {
                    $( $( $fields.field(stringify!($field), $field); )* )?
                    $( $fields.field(stringify!($single), $single); )?
                    $( FieldList::fields(&**$flat, $fields); )?
                    stringify!($variant)
                }
            )*
        }
    };
}

/// Lists every field of one payload structure, in declaration order.
macro_rules! list_fields {
    ($value:expr, $fields:ident; $name:ident { $($field:ident),* $(,)? }) => {{
        let $name { $($field),* } = $value;
        $( $fields.field(stringify!($field), $field); )*
    }};
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

/// A closed classification printed by its variant name.
trait VariantName {
    fn name(&self) -> &'static str;
}

/// Lists every variant of each fieldless classification. The match has no
/// wildcard and unit patterns, so a variant added to one, or given data,
/// fails to compile until it is listed here.
macro_rules! variant_names {
    ($($enum:ident { $($variant:ident),* $(,)? })*) => {
        $(
            impl VariantName for $enum {
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
        StructConstructor, EnumVariant, NumericBound, Interface, Binding, Label, Invariant,
        OperationFamily, Module,
    }
    DeclarationDomain { LexicalIdentifier, NominalType, Constructor, NumericBound, Label, Invariant }
    LexicalUseRole {
        Type, GenericBound, FormalGroup, TypeArgument, Construct, VariantOwner, EnsuresVariant,
        EffectRoot, EffectIndex, BreakLabel, Const, ConstValue, PlaceBase, IdentifierCallee,
        OperationCallee, FunctionBinding, GenericNumericSuffix, InvariantValue, ProofValue,
        InvariantFact,
    }
    LoopInvariantProofObligation { Base, Backedge }
    PostconditionProofDisposition { Refuted, Unproved }
    ReservedDeclarationRole {
        Function, NamedConst, Parameter, Let, ContractDefinition, ForBinder, MatchBinder,
        PlainResultSelector, VariantResultSelector, Field, VariantField,
    }
    ReservedNameClass { DotlessOperation, ModeWord }
    SourceIssueKind {
        InvalidUtf8, UnexpectedByte, MissingLabelName, UnterminatedString, InvalidStringByte,
        InvalidStringEscape, InvalidSourceByte, CommentPrefix,
    }
    StaticObligationDisposition { Refuted, Unproved }
    UnsupportedSemanticFeature {
        Generics, PreludeNominalValues, ReferenceFormation, CompositeValues,
        RecursiveNominalLayout, OwnershipJoin, DuplicateMatchArm, OperationFamily,
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
            concrete_callee,
            requires_clause,
            instantiated_goal,
            disposition,
            mechanical_fix,
        });
    }
}

impl FieldList for UndischargedPostconditionDetail {
    fn fields(&self, fields: &mut Fields<'_>) {
        list_fields!(self, fields; UndischargedPostconditionDetail {
            concrete_function,
            postcondition,
            conjunct,
            selector,
            relation,
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

/// A refused graph row or entry is located at its written path; the kind
/// carries the payload [MOD-1].
impl Report for GraphIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let kind = self.kind();
        report_variants!(GraphIssueKind, kind, fields;
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
        )
    }
}

/// A composition rejection names what the entry's composition judgment
/// found [MOD-8, MOD-9, STOR-8].
impl Report for super::CompositionIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        use super::CompositionIssue;
        report_variants!(CompositionIssue, self, fields;
            PendingDeclaration { declaration };
            EntryFunctionMissing { function };
            EntryFunctionPrivate { function };
            HeapInClosure { path };
        )
    }
}

impl Report for SemanticIssueKind {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        report_variants!(SemanticIssueKind, self, fields;
            InvalidIntegerLiteral;
            InvalidFloatLiteral;
            InvalidConstValue;
            InaccessibleField { field, reason };
            InaccessibleVariant { variant, reason };
            ConstantCycle { cycle };
            ConstEvalOverflow { operation };
            ConstRuntimeArithmeticMode { mechanical_fix };
            TypeMismatch { expected, found };
            UnadmittedOperandShape { expected, mechanical_fix };
            BehaviorArgumentMismatch { expected, mechanical_fix };
            ImmutableSetTarget;
            InvalidSetTarget { root_class, required_classes };
            ImmutableWrittenArgument { binding, mechanical_fix };
            LinearValueNotConsumed { binding, obligation, mechanical_fix };
            LinearValuePartiallyConsumed { obligation, residual, mechanical_fix };
            LinearityBoundMismatch { parameter, bound, argument, actual };
            LivenessJoinDisagreement { binding, live_predecessor, dead_predecessor, mechanical_fix };
            LinearAssignmentTarget { target_type, mechanical_fix };
            InlineRuntimeCapacityShape { spelling, mechanical_fix };
            SwapOverCopyPlace { place_type, mechanical_fix };
            HeapTypeUnderNoHeap { spelling, mechanical_fix };
            InvalidElementMove { mechanical_fix };
            MoveThroughReference { mechanical_fix };
            ReservedPseudoField { spelling, mechanical_fix };
            ReadonlyWriteTarget { spelling, mechanical_fix };
            MoveOfCopy { mechanical_fix };
            BareAffineUse { mechanical_fix };
            ContainerConstruction { nominal, mechanical_fix };
            UseAfterMove { mechanical_fix };
            InvalidReferenceUse { binder, event, mechanical_fix };
            ReferenceToReferenceVariable { mechanical_fix };
            EscapingReference { mechanical_fix };
            RangeOverRing { mechanical_fix };
            MoveOutOfSlot { mechanical_fix };
            MissingDereference { mechanical_fix };
            MoveOuterBindingInLoop { binding, mechanical_fix };
            InvalidOperation;
            InvalidPredicateCondition;
            InvalidConditionalForm { mechanical_fix };
            UndischargedBoundsObligation { residual, disposition, mechanical_fix };
            UndischargedEmptyRunRelease { residual, disposition, mechanical_fix };
            UndischargedIntegerDomainObligation { residual, disposition, mechanical_fix };
            UndischargedConversionDomainObligation { residual, disposition, mechanical_fix };
            UndischargedAllocationFitObligation { residual, disposition, mechanical_fix };
            UndischargedRangeFormationObligation { residual, disposition, mechanical_fix };
            UndischargedCallSeparation { residual, mechanical_fix };
            OverlappingCallEffects { first, second, mechanical_fix };
            AtomicUpdateReachesTargetPrefix { target, effect, mechanical_fix };
            UndischargedCallRequirement[detail];
            InvalidCountedEndpoint { mechanical_fix };
            BreakOutsideLoop { mechanical_fix };
            InvalidInvariant { reason, mechanical_fix };
            UndischargedLoopInvariant { name, obligation, required_relation, disposition, mechanical_fix };
            UndischargedLocalInvariant { name, disposition, mechanical_fix };
            InvalidSourceProof { reason, mechanical_fix };
            UndischargedSourceProof { name, obligation, mechanical_fix };
            ReturnMismatch;
            InvalidMusttail { condition, subject };
            PolymorphicRecursion { cycle, mechanical_fix };
            UnreachableStatement;
            FunctionFallthrough;
            InvalidRequires;
            InvalidPostconditionSelector;
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
            NonExhaustiveMatch { missing_variants };
            InvalidPropagation;
            InvalidGive;
            EmptyDeliverySet { binding, mechanical_fix };
            InvalidEffectRow { reason, mechanical_fix };
            SubsumedEffectEntry { entry, covering };
            EffectMismatch { expected_row, found_row, missing, extra, mechanical_fix };
            SourceContractGenericBound;
        )
    }
}

/// The rule and origin are the envelope's; the kind carries the payload.
impl Report for ResolutionIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let Self {
            rule: _,
            origin: _,
            kind,
        } = self;
        report_variants!(ResolutionIssueKind, kind, fields;
            ContractShape(shape);
            MisplacedHeapDeclaration { admitted };
            ReservedName { spelling, declaration_role, class, inventory_ordinal };
            MatchBinderFreshness { spelling, paired_field, earlier_binder, arm_entry_conflicts };
            DeclarationCollision { spelling, conflicts, mechanical_fix };
            InvisibleUse { spelling, role, admissible, origins };
            NonEnclosingLabel { spelling, role, origins };
            ModuleProgramHeapDeclaration;
            MisplacedAlias;
            InvalidAliasTarget { spelling, target, reason };
            LibraryNamesItself { path };
            UnknownModule { path };
            MissingModuleEdge { from, to };
            QualifiedNameNotFound { spelling, module, role };
            PrivateDeclaration { spelling, module };
            UnknownOwnedVariant { spelling, reason };
            MisplacedPublication { reason };
            PrivateInPublicSignature { spelling };
            Correspondence { spelling, reason };
            UnresolvedUse { spelling, role, admissible, available };
            UndeclaredSetTarget { spelling, mechanical_fix };
        )
    }
}

/// [DIAG-1]'s raw lexical defects: the kind is the defect shape, and the
/// offending bytes are printed escaped because they are often invisible.
impl Report for SourceIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let Self { span, kind } = self;
        fields.field(
            "found",
            &Spelled(SyntaxCoordinate::new(
                span.source(),
                span.start(),
                span.end(),
            )),
        );
        kind.name()
    }
}

/// A formed token that satisfies no terminal predicate [GRAM-1]; its owner
/// is the envelope's rule.
impl Report for TerminalIssue {
    fn report(&self, fields: &mut Fields<'_>) -> &'static str {
        let Self { token, owner: _ } = self;
        fields.field(
            "found",
            &Spelled(SyntaxCoordinate::new(
                token.source(),
                token.start(),
                token.end(),
            )),
        );
        "UnclassifiedToken"
    }
}

/// The grammar's expected-terminal set at the failure boundary and what the
/// source has there instead [DIAG-1].
impl Report for SyntaxIssue {
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
        "UnexpectedToken"
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
        } = self;
        fields.field("expected", &Exact(expected));
        fields.field("found", &Exact(found));
        "NonCanonicalTrivia"
    }
}

/// A valid source that needs a semantic family the compiler has not built;
/// the node is the record's location.
impl Report for SemanticUnsupported {
    fn report(&self, _: &mut Fields<'_>) -> &'static str {
        let Self { feature, node: _ } = self;
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
        Value::Text(value) => push_printable(text, value),
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
                write_text(text, item);
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
        Value::Text(value) => {
            let mut printable = String::new();
            push_printable(&mut printable, value);
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
