use crate::{ACTIVE_KERNEL_SPEC_HASH, SpecHash};

/// Exact numbered specification owning this terminal contract.
pub const TERMINAL_CONTRACT_SPEC_HASH: SpecHash = ACTIVE_KERNEL_SPEC_HASH;

/// One exact raw-token spelling produced by a fixed grammar atom in the active specification.
///
/// Every v0.60 fixed atom is exactly one raw formed token [GRAM-1]: the last
/// compound atom, `&uniq`, retired with the permission marker, so no variant
/// stands for a two-token sequence any more. Every atom is also lowercase or
/// punctuation, the capitalized `Slice` and `MutSlice` having retired with the
/// view nominals [TYPE-2]; nothing in the inventory competes with TYPEID.
///
/// The declaration order is the stable dense predicate index and is kept equal
/// to first grammar-occurrence order, which [`ALL_FIXED_TERMINALS`] carries.
/// v0.70 opens [GRAM-2]'s `item` with the `public` item marker and the file
/// alias header, so `public`, `alias`, `=`, `pkg`, `::` and `;` take the first
/// six slots and every other index moves; v0.72's alias header also admits the
/// standard library qualifier `std` [MOD-10], which takes the slot after `pkg`; retired source atoms leave this
/// current-grammar inventory outright. The indices are
/// compiler-local and are never serialized.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum FixedTerminal {
    /// `public`.
    Public,
    /// `alias`.
    Alias,
    /// `=`.
    Equal,
    /// `pkg`.
    Pkg,
    /// `std`.
    Std,
    /// `::`.
    ColonColon,
    /// `;`.
    Semicolon,
    /// `program`.
    Program,
    /// `no_heap`.
    NoHeap,
    /// `opaque`.
    Opaque,
    /// `nocopy`.
    Nocopy,
    /// `nodrop`.
    Nodrop,
    /// `struct`.
    Struct,
    /// `{`.
    LeftBrace,
    /// `}`.
    RightBrace,
    /// `readonly`.
    Readonly,
    /// `:`.
    Colon,
    /// `enum`.
    Enum,
    /// `(`.
    LeftParen,
    /// `)`.
    RightParen,
    /// `,`.
    Comma,
    /// `fn`.
    Fn,
    /// `->`.
    ThinArrow,
    /// `contract`.
    Contract,
    /// `define`.
    Define,
    /// `requires`.
    Requires,
    /// `ensures`.
    Ensures,
    /// `when`.
    When,
    /// `is`.
    Is,
    /// `interface`.
    Interface,
    /// `binding`.
    Binding,
    /// `const`.
    Const,
    /// `doc`.
    Doc,
    /// `<`.
    LeftAngle,
    /// `>`.
    RightAngle,
    /// `copy`.
    Copy,
    /// `drop`.
    Drop,
    /// `&`.
    Ampersand,
    /// `[`.
    LeftBracket,
    /// `]`.
    RightBracket,
    /// `entry`.
    Entry,
    /// `i8`.
    I8,
    /// `i16`.
    I16,
    /// `i32`.
    I32,
    /// `i64`.
    I64,
    /// `u8`.
    U8,
    /// `u16`.
    U16,
    /// `u32`.
    U32,
    /// `u64`.
    U64,
    /// `f32`.
    F32,
    /// `f64`.
    F64,
    /// `unit`.
    Unit,
    /// `let`.
    Let,
    /// `..`.
    DotDot,
    /// `move`.
    Move,
    /// `if`.
    If,
    /// `else`.
    Else,
    /// `propagate`.
    Propagate,
    /// `set`.
    Set,
    /// `return`.
    Return,
    /// `loop`.
    Loop,
    /// `for`.
    For,
    /// `in`.
    In,
    /// `invariant`.
    Invariant,
    /// `use`.
    Use,
    /// `times`.
    Times,
    /// `*`.
    Star,
    /// `+`.
    Plus,
    /// `-`.
    Minus,
    /// `break`.
    Break,
    /// `give`.
    Give,
    /// `match`.
    Match,
    /// `=>`.
    FatArrow,
    /// `+wrap`.
    PlusWrap,
    /// `+defined`.
    PlusDefined,
    /// `+checked`.
    PlusChecked,
    /// `+sat`.
    PlusSat,
    /// `-wrap`.
    MinusWrap,
    /// `-defined`.
    MinusDefined,
    /// `-checked`.
    MinusChecked,
    /// `-sat`.
    MinusSat,
    /// `*wrap`.
    StarWrap,
    /// `*defined`.
    StarDefined,
    /// `*checked`.
    StarChecked,
    /// `*sat`.
    StarSat,
    /// `/`.
    Slash,
    /// `/defined`.
    SlashDefined,
    /// `/checked`.
    SlashChecked,
    /// `%`.
    Percent,
    /// `%defined`.
    PercentDefined,
    /// `%checked`.
    PercentChecked,
    /// `==`.
    EqualEqual,
    /// `!=`.
    BangEqual,
    /// `<=`.
    LessEqual,
    /// `>=`.
    GreaterEqual,
    /// `musttail`.
    Musttail,
    /// `deref`.
    Deref,
    /// `.`.
    Dot,
    /// `pure`.
    Pure,
    /// `reads`.
    Reads,
    /// `writes`.
    Writes,
}

/// Every fixed raw-token predicate in the active specification, in first occurrence order.
pub const ALL_FIXED_TERMINALS: [FixedTerminal; 101] = [
    FixedTerminal::Public,
    FixedTerminal::Alias,
    FixedTerminal::Equal,
    FixedTerminal::Pkg,
    FixedTerminal::Std,
    FixedTerminal::ColonColon,
    FixedTerminal::Semicolon,
    FixedTerminal::Program,
    FixedTerminal::NoHeap,
    FixedTerminal::Opaque,
    FixedTerminal::Nocopy,
    FixedTerminal::Nodrop,
    FixedTerminal::Struct,
    FixedTerminal::LeftBrace,
    FixedTerminal::RightBrace,
    FixedTerminal::Readonly,
    FixedTerminal::Colon,
    FixedTerminal::Enum,
    FixedTerminal::LeftParen,
    FixedTerminal::RightParen,
    FixedTerminal::Comma,
    FixedTerminal::Fn,
    FixedTerminal::ThinArrow,
    FixedTerminal::Contract,
    FixedTerminal::Define,
    FixedTerminal::Requires,
    FixedTerminal::Ensures,
    FixedTerminal::When,
    FixedTerminal::Is,
    FixedTerminal::Interface,
    FixedTerminal::Binding,
    FixedTerminal::Const,
    FixedTerminal::Doc,
    FixedTerminal::LeftAngle,
    FixedTerminal::RightAngle,
    FixedTerminal::Copy,
    FixedTerminal::Drop,
    FixedTerminal::Ampersand,
    FixedTerminal::LeftBracket,
    FixedTerminal::RightBracket,
    FixedTerminal::Entry,
    FixedTerminal::I8,
    FixedTerminal::I16,
    FixedTerminal::I32,
    FixedTerminal::I64,
    FixedTerminal::U8,
    FixedTerminal::U16,
    FixedTerminal::U32,
    FixedTerminal::U64,
    FixedTerminal::F32,
    FixedTerminal::F64,
    FixedTerminal::Unit,
    FixedTerminal::Let,
    FixedTerminal::DotDot,
    FixedTerminal::Move,
    FixedTerminal::If,
    FixedTerminal::Else,
    FixedTerminal::Propagate,
    FixedTerminal::Set,
    FixedTerminal::Return,
    FixedTerminal::Loop,
    FixedTerminal::For,
    FixedTerminal::In,
    FixedTerminal::Invariant,
    FixedTerminal::Use,
    FixedTerminal::Times,
    FixedTerminal::Star,
    FixedTerminal::Plus,
    FixedTerminal::Minus,
    FixedTerminal::Break,
    FixedTerminal::Give,
    FixedTerminal::Match,
    FixedTerminal::FatArrow,
    FixedTerminal::PlusWrap,
    FixedTerminal::PlusDefined,
    FixedTerminal::PlusChecked,
    FixedTerminal::PlusSat,
    FixedTerminal::MinusWrap,
    FixedTerminal::MinusDefined,
    FixedTerminal::MinusChecked,
    FixedTerminal::MinusSat,
    FixedTerminal::StarWrap,
    FixedTerminal::StarDefined,
    FixedTerminal::StarChecked,
    FixedTerminal::StarSat,
    FixedTerminal::Slash,
    FixedTerminal::SlashDefined,
    FixedTerminal::SlashChecked,
    FixedTerminal::Percent,
    FixedTerminal::PercentDefined,
    FixedTerminal::PercentChecked,
    FixedTerminal::EqualEqual,
    FixedTerminal::BangEqual,
    FixedTerminal::LessEqual,
    FixedTerminal::GreaterEqual,
    FixedTerminal::Musttail,
    FixedTerminal::Deref,
    FixedTerminal::Dot,
    FixedTerminal::Pure,
    FixedTerminal::Reads,
    FixedTerminal::Writes,
];

impl FixedTerminal {
    /// Returns the exact one-token spelling of this predicate.
    ///
    /// Text rather than bytes because this is the spelling a writer types, and
    /// a diagnostic that lists what was expected here prints it directly;
    /// [`FixedTerminal::spelling_bytes`] serves the comparisons against raw
    /// source.
    #[must_use]
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::Program => "program",
            Self::NoHeap => "no_heap",
            Self::Semicolon => ";",
            Self::Opaque => "opaque",
            Self::Nocopy => "nocopy",
            Self::Nodrop => "nodrop",
            Self::Struct => "struct",
            Self::LeftBrace => "{",
            Self::RightBrace => "}",
            Self::Readonly => "readonly",
            Self::Colon => ":",
            Self::Enum => "enum",
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::Comma => ",",
            Self::Fn => "fn",
            Self::ThinArrow => "->",
            Self::Contract => "contract",
            Self::Define => "define",
            Self::Equal => "=",
            Self::Requires => "requires",
            Self::Ensures => "ensures",
            Self::When => "when",
            Self::Is => "is",
            Self::Interface => "interface",
            Self::Binding => "binding",
            Self::ColonColon => "::",
            Self::Const => "const",
            Self::Doc => "doc",
            Self::LeftAngle => "<",
            Self::RightAngle => ">",
            Self::Copy => "copy",
            Self::Drop => "drop",
            Self::Ampersand => "&",
            Self::LeftBracket => "[",
            Self::RightBracket => "]",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Unit => "unit",
            Self::Let => "let",
            Self::DotDot => "..",
            Self::Move => "move",
            Self::If => "if",
            Self::Else => "else",
            Self::Propagate => "propagate",
            Self::Set => "set",
            Self::Return => "return",
            Self::Musttail => "musttail",
            Self::Loop => "loop",
            Self::For => "for",
            Self::In => "in",
            Self::Invariant => "invariant",
            Self::Use => "use",
            Self::Times => "times",
            Self::Star => "*",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Break => "break",
            Self::Give => "give",
            Self::Match => "match",
            Self::FatArrow => "=>",
            Self::PlusWrap => "+wrap",
            Self::PlusDefined => "+defined",
            Self::PlusChecked => "+checked",
            Self::PlusSat => "+sat",
            Self::MinusWrap => "-wrap",
            Self::MinusDefined => "-defined",
            Self::MinusChecked => "-checked",
            Self::MinusSat => "-sat",
            Self::StarWrap => "*wrap",
            Self::StarDefined => "*defined",
            Self::StarChecked => "*checked",
            Self::StarSat => "*sat",
            Self::Slash => "/",
            Self::SlashDefined => "/defined",
            Self::SlashChecked => "/checked",
            Self::Percent => "%",
            Self::PercentDefined => "%defined",
            Self::PercentChecked => "%checked",
            Self::EqualEqual => "==",
            Self::BangEqual => "!=",
            Self::LessEqual => "<=",
            Self::GreaterEqual => ">=",
            Self::Deref => "deref",
            Self::Entry => "entry",
            Self::Dot => ".",
            Self::Pure => "pure",
            Self::Reads => "reads",
            Self::Writes => "writes",
            Self::Public => "public",
            Self::Alias => "alias",
            Self::Pkg => "pkg",
            Self::Std => "std",
        }
    }

    /// Returns the exact one-token spelling as raw source bytes.
    #[must_use]
    pub const fn spelling_bytes(self) -> &'static [u8] {
        self.spelling().as_bytes()
    }

    /// Finds the fixed predicate with exactly these raw-token bytes.
    #[must_use]
    pub fn from_spelling(spelling: &[u8]) -> Option<Self> {
        ALL_FIXED_TERMINALS
            .iter()
            .copied()
            .find(|terminal| terminal.spelling_bytes() == spelling)
    }

    /// Reports whether this is one of the sixteen GRAM-1 operator forms.
    ///
    /// The four compound comparisons are excluded: they are compound
    /// punctuation, not operator forms, and carry no mode suffix.
    #[must_use]
    pub const fn is_operator_form(self) -> bool {
        matches!(
            self,
            Self::Plus
                | Self::PlusWrap
                | Self::PlusDefined
                | Self::PlusChecked
                | Self::PlusSat
                | Self::Minus
                | Self::MinusWrap
                | Self::MinusDefined
                | Self::MinusChecked
                | Self::MinusSat
                | Self::Star
                | Self::StarWrap
                | Self::StarDefined
                | Self::StarChecked
                | Self::StarSat
                | Self::Slash
                | Self::SlashDefined
                | Self::SlashChecked
                | Self::Percent
                | Self::PercentDefined
                | Self::PercentChecked
        )
    }

    const fn index(self) -> u8 {
        self as u8
    }
}

/// One terminal predicate in the complete approved active token-membership set.
///
/// A formed token may satisfy more than one predicate. In particular, `unit`
/// satisfies both its fixed predicate and `Literal`; callers must retain both
/// rather than choosing one by priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalPredicate {
    /// One exact fixed raw-token spelling.
    Fixed(FixedTerminal),
    /// FORM-3 `IDENT`.
    Identifier,
    /// FORM-3 `TYPEID`.
    TypeIdentifier,
    /// FORM-3 `LABEL`.
    Label,
    /// FORM-3 `OPNAME`.
    OperationName,
    /// FORM-5 `literal` union membership, before FORM-7 checking.
    Literal,
    /// FORM-5 `STRING`.
    String,
    /// The sole `[0-9]+` grammar-pattern predicate.
    Digits,
}

/// The predicates that are not one fixed spelling, in inventory order.
///
/// [FORM-3] and [FORM-5] give the classes; `SOURCE_END` is intentionally
/// absent from the inventory this list completes.
const EXTERNAL_TERMINAL_PREDICATES: [TerminalPredicate; 7] = [
    TerminalPredicate::Identifier,
    TerminalPredicate::TypeIdentifier,
    TerminalPredicate::Label,
    TerminalPredicate::OperationName,
    TerminalPredicate::Literal,
    TerminalPredicate::String,
    TerminalPredicate::Digits,
];

/// Where [`EXTERNAL_TERMINAL_PREDICATES`] starts in the inventory: immediately
/// after the last fixed terminal.
///
/// Deriving this rather than writing it is what keeps a new keyword from
/// silently overwriting an external predicate. The inventory below copies the
/// fixed terminals in first, so a hardcoded external position that no longer
/// clears them is a collision the array initializer cannot report.
const EXTERNAL_TERMINAL_BASE: usize = ALL_FIXED_TERMINALS.len();

/// Every approved active-specification token predicate: the fixed inventory in
/// first occurrence order followed by the external predicates. `SOURCE_END` is
/// intentionally absent.
pub const ALL_TERMINAL_PREDICATES: [TerminalPredicate;
    ALL_FIXED_TERMINALS.len() + EXTERNAL_TERMINAL_PREDICATES.len()] = {
    let mut predicates = [TerminalPredicate::Identifier;
        ALL_FIXED_TERMINALS.len() + EXTERNAL_TERMINAL_PREDICATES.len()];
    let mut index = 0;
    while index < ALL_FIXED_TERMINALS.len() {
        predicates[index] = TerminalPredicate::Fixed(ALL_FIXED_TERMINALS[index]);
        index += 1;
    }
    let mut offset = 0;
    while offset < EXTERNAL_TERMINAL_PREDICATES.len() {
        predicates[EXTERNAL_TERMINAL_BASE + offset] = EXTERNAL_TERMINAL_PREDICATES[offset];
        offset += 1;
    }
    predicates
};

impl TerminalPredicate {
    const fn index(self) -> u8 {
        let base = EXTERNAL_TERMINAL_BASE as u8;
        match self {
            Self::Fixed(terminal) => terminal.index(),
            Self::Identifier => base,
            Self::TypeIdentifier => base + 1,
            Self::Label => base + 2,
            Self::OperationName => base + 3,
            Self::Literal => base + 4,
            Self::String => base + 5,
            Self::Digits => base + 6,
        }
    }

    /// The source spelling a writer would have to write to satisfy this
    /// predicate.
    ///
    /// A fixed terminal is its own bytes. A pattern predicate has no single
    /// spelling, so it answers with the name [FORM-3] and [FORM-5] give the
    /// class — the same name the grammar productions use, so a reader can look
    /// it up.
    #[must_use]
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::Fixed(terminal) => terminal.spelling(),
            Self::Identifier => "IDENT",
            Self::TypeIdentifier => "TYPEID",
            Self::Label => "LABEL",
            Self::OperationName => "OPNAME",
            Self::Literal => "literal",
            Self::String => "STRING",
            Self::Digits => "digits",
        }
    }
}

/// The complete set of the active specification terminal predicates retained for one formed token.
///
/// This is a membership set, not a selected token kind. Its compact layout is
/// runtime-local and is not an artifact encoding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerminalSet(u128);

impl TerminalSet {
    /// Creates an empty membership set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Adds one matching predicate.
    pub fn insert(&mut self, predicate: TerminalPredicate) {
        self.0 |= 1_u128 << predicate.index();
    }

    /// Reports whether this token matched the given predicate.
    #[must_use]
    pub const fn contains(self, predicate: TerminalPredicate) -> bool {
        self.0 & (1_u128 << predicate.index()) != 0
    }

    /// Reports whether no approved predicate matched.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns the number of matching predicates.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// Visits matching predicates in stable storage order.
    ///
    /// This order is not the grammar-occurrence order used by syntax
    /// diagnostics. Parser tables must retain their specification-defined
    /// source-grammar ranks.
    pub fn iter(self) -> impl Iterator<Item = TerminalPredicate> {
        ALL_TERMINAL_PREDICATES
            .iter()
            .copied()
            .filter(move |predicate| self.contains(*predicate))
    }
}

fn lower_word(spelling: &[u8]) -> bool {
    spelling.first().is_some_and(u8::is_ascii_lowercase)
        && spelling
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

/// Tests `IDENT` membership, excluding only fixed lowercase spellings. Retired
/// source atoms are ordinary identifiers again.
#[must_use]
pub fn is_identifier(spelling: &[u8]) -> bool {
    lower_word(spelling) && FixedTerminal::from_spelling(spelling).is_none()
}

/// Tests active specification `TYPEID` membership.
///
/// v0.60's [GRAM-3] derives every storage shape as `TYPEID targs?`, so the
/// capitalized fixed atoms `Slice` and `MutSlice` retired with the view
/// nominals and no fixed spelling starts with an uppercase byte. The
/// exclusion `is_identifier` still makes for a fixed lowercase word therefore
/// has no subject here, and an upper word is a TYPEID on its shape alone.
#[must_use]
pub fn is_type_identifier(spelling: &[u8]) -> bool {
    spelling.first().is_some_and(u8::is_ascii_uppercase)
        && spelling[1..].iter().all(u8::is_ascii_alphanumeric)
}

/// Tests active specification `LABEL` membership.
#[must_use]
pub fn is_label(spelling: &[u8]) -> bool {
    spelling.strip_prefix(b"@").is_some_and(lower_word)
}

/// Tests active specification `OPNAME` membership.
#[must_use]
pub fn is_operation_name(spelling: &[u8]) -> bool {
    [
        b".wrap".as_slice(),
        b".defined",
        b".checked",
        b".sat",
        b".strict",
    ]
    .iter()
    .any(|suffix| spelling.strip_suffix(*suffix).is_some_and(lower_word))
}

/// Tests the active specification's sole `[0-9]+` pattern predicate.
#[must_use]
pub fn is_digits(spelling: &[u8]) -> bool {
    !spelling.is_empty() && spelling.iter().all(u8::is_ascii_digit)
}

fn integer_literal(spelling: &[u8]) -> bool {
    let Some(split) = spelling.iter().rposition(|byte| *byte == b'_') else {
        return false;
    };
    let suffix = &spelling[split + 1..];
    if !matches!(
        suffix,
        b"i8" | b"i16" | b"i32" | b"i64" | b"u8" | b"u16" | b"u32" | b"u64"
    ) {
        return false;
    }
    let negative = spelling.first() == Some(&b'-');
    let digits = &spelling[usize::from(negative)..split];
    !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
}

fn decimal_component_end(spelling: &[u8], start: usize) -> Option<usize> {
    let first = *spelling.get(start)?;
    if !first.is_ascii_digit() {
        return None;
    }
    if first == b'0' {
        return Some(start + 1);
    }
    let mut cursor = start + 1;
    while spelling.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    Some(cursor)
}

fn float_literal(spelling: &[u8]) -> bool {
    let mut cursor = usize::from(spelling.first() == Some(&b'-'));
    let Some(integer_end) = decimal_component_end(spelling, cursor) else {
        return false;
    };
    cursor = integer_end;
    if spelling.get(cursor) != Some(&b'.') {
        return false;
    }
    cursor += 1;
    let fraction_start = cursor;
    while spelling.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    if cursor == fraction_start {
        return false;
    }
    if spelling.get(cursor) == Some(&b'e') {
        cursor += 1;
        if spelling.get(cursor) == Some(&b'-') {
            cursor += 1;
        }
        let Some(exponent_end) = decimal_component_end(spelling, cursor) else {
            return false;
        };
        cursor = exponent_end;
    }
    matches!(&spelling[cursor..], b"_f32" | b"_f64")
}

/// Tests the active specification `literal` grammar membership before FORM-7 value checking.
///
/// Range, integer leading-zero, finite-value, and shortest-float checks are
/// deliberately outside this predicate, as required by FORM-7.
#[must_use]
pub fn is_literal(spelling: &[u8]) -> bool {
    matches!(spelling, b"unit" | b"0_T" | b"1_T")
        || integer_literal(spelling)
        || float_literal(spelling)
}

/// Tests active specification `STRING` membership.
#[must_use]
pub fn is_string(spelling: &[u8]) -> bool {
    if spelling.len() < 2 || spelling.first() != Some(&b'"') || spelling.last() != Some(&b'"') {
        return false;
    }
    let mut cursor = 1;
    while cursor + 1 < spelling.len() {
        let byte = spelling[cursor];
        if byte == b'\\' {
            if !matches!(spelling.get(cursor + 1), Some(b'\\' | b'"' | b'n')) {
                return false;
            }
            cursor += 2;
        } else if !(0x20..=0x7e).contains(&byte) || matches!(byte, b'"' | b'\\') {
            return false;
        } else {
            cursor += 1;
        }
    }
    cursor + 1 == spelling.len()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        ALL_FIXED_TERMINALS, ALL_TERMINAL_PREDICATES, EXTERNAL_TERMINAL_PREDICATES, FixedTerminal,
        TerminalPredicate, TerminalSet, is_identifier, is_literal, is_operation_name, is_string,
    };

    #[test]
    fn fixed_inventory_is_unique_and_round_trips() {
        let spellings: BTreeSet<&[u8]> = ALL_FIXED_TERMINALS
            .iter()
            .map(|terminal| terminal.spelling_bytes())
            .collect();
        assert_eq!(spellings.len(), ALL_FIXED_TERMINALS.len());
        for terminal in ALL_FIXED_TERMINALS {
            assert_eq!(
                FixedTerminal::from_spelling(terminal.spelling_bytes()),
                Some(terminal)
            );
        }
        // v0.70's [GRAM-2] `item` opens with its `public` item marker and its
        // file alias header arm, so `public`, `alias`, `=`, `pkg`, `::` and `;`
        // take the first six slots and `heap_decl`'s two atoms follow; v0.72's
        // `std` qualifier [MOD-10] first occurs beside `pkg` in that header. The graph
        // productions close [GRAM-2], so `entry` now first occurs there, before
        // the primitive type atoms, and a call's `musttail` first occurs in
        // [GRAM-5] after the comparison atoms.
        assert_eq!(FixedTerminal::Alias as u8, 1);
        assert_eq!(FixedTerminal::Equal as u8, 2);
        assert_eq!(FixedTerminal::Pkg as u8, 3);
        assert_eq!(FixedTerminal::Std as u8, 4);
        assert_eq!(FixedTerminal::ColonColon as u8, 5);
        assert_eq!(FixedTerminal::Semicolon as u8, 6);
        assert_eq!(FixedTerminal::Public as u8, 0);
        assert_eq!(FixedTerminal::Program as u8, 7);
        assert_eq!(FixedTerminal::NoHeap as u8, 8);
        assert_eq!(FixedTerminal::Opaque as u8, 9);
        assert_eq!(FixedTerminal::Nocopy as u8, 10);
        assert_eq!(FixedTerminal::Nodrop as u8, 11);
        assert_eq!(FixedTerminal::Readonly as u8, 15);
        assert_eq!(FixedTerminal::Colon as u8, 16);
        assert_eq!(FixedTerminal::Ensures as u8, 26);
        assert_eq!(FixedTerminal::Is as u8, 28);
        assert_eq!(FixedTerminal::Copy as u8, 35);
        assert_eq!(FixedTerminal::Drop as u8, 36);
        assert_eq!(FixedTerminal::Ampersand as u8, 37);
        assert_eq!(FixedTerminal::Entry as u8, 40);
        assert_eq!(FixedTerminal::DotDot as u8, 53);
        assert_eq!(FixedTerminal::For as u8, 61);
        assert_eq!(FixedTerminal::In as u8, 62);
        assert_eq!(FixedTerminal::Invariant as u8, 63);
        assert_eq!(FixedTerminal::Use as u8, 64);
        assert_eq!(FixedTerminal::Times as u8, 65);
        assert_eq!(FixedTerminal::Musttail as u8, 95);
        assert_eq!(FixedTerminal::PercentChecked as u8, 90);
        assert_eq!(FixedTerminal::Writes as u8, 100);
        assert_eq!(TerminalPredicate::Identifier.index(), 101);
        assert_eq!(TerminalPredicate::Digits.index(), 107);
    }

    /// The inventory holds every predicate, once.
    ///
    /// This is the invariant a hardcoded external position broke: the array
    /// copies the fixed terminals in first and then writes the external
    /// predicates at fixed slots, so adding a ninety-ninth fixed terminal
    /// while those slots still started at 98 overwrote `Fixed(Times)` with
    /// `Identifier`. The inventory then held 106 predicates instead of 107,
    /// and the missing one was a keyword the parser had just gained. Counting
    /// what is actually there catches both the loss and any duplicate.
    #[test]
    fn the_inventory_holds_every_predicate_once() {
        assert_eq!(
            ALL_TERMINAL_PREDICATES.len(),
            ALL_FIXED_TERMINALS.len() + EXTERNAL_TERMINAL_PREDICATES.len()
        );
        let holds = |wanted: TerminalPredicate| {
            ALL_TERMINAL_PREDICATES
                .into_iter()
                .filter(|predicate| *predicate == wanted)
                .count()
        };
        for terminal in ALL_FIXED_TERMINALS {
            assert_eq!(holds(TerminalPredicate::Fixed(terminal)), 1);
        }
        for predicate in EXTERNAL_TERMINAL_PREDICATES {
            assert_eq!(holds(predicate), 1);
        }
    }

    /// `index()` is a `TerminalSet` bit position, so it must be distinct per
    /// predicate and inside the `u128` the set is stored in. It is not an
    /// index into the inventory above: the fixed terminals answer with their
    /// declaration order and the inventory is in first-occurrence order.
    #[test]
    fn every_predicate_has_its_own_bit() {
        let bits: BTreeSet<u8> = ALL_TERMINAL_PREDICATES
            .into_iter()
            .map(TerminalPredicate::index)
            .collect();
        assert_eq!(bits.len(), ALL_TERMINAL_PREDICATES.len());
        assert!(
            bits.iter()
                .all(|bit| usize::from(*bit) < u128::BITS as usize)
        );
    }

    #[test]
    fn fixed_lower_words_are_excluded_from_identifiers() {
        for terminal in ALL_FIXED_TERMINALS {
            if terminal
                .spelling_bytes()
                .first()
                .is_some_and(u8::is_ascii_lowercase)
            {
                assert!(!is_identifier(terminal.spelling_bytes()));
            }
        }
        for spelling in [
            b"x".as_slice(),
            b"deref_value",
            b"wrap",
            b"ieq",
            b"claim",
            b"because",
            b"deny_claims",
            b"traps",
            b"trap",
        ] {
            assert!(is_identifier(spelling));
        }
        assert!(is_identifier(b"check"));
    }

    #[test]
    fn operation_suffix_language_is_closed() {
        for spelling in [
            b"iadd.wrap".as_slice(),
            b"iadd.defined",
            b"iadd.checked",
            b"iadd.sat",
            b"iadd.strict",
        ] {
            assert!(is_operation_name(spelling));
        }
        for spelling in [
            b"iadd.trap".as_slice(),
            b".wrap",
            b"x.other",
            b"x.wrap_more",
            b"X.wrap",
        ] {
            assert!(!is_operation_name(spelling));
        }
    }

    #[test]
    fn literal_membership_stops_before_form7_value_checks() {
        for spelling in [
            b"unit".as_slice(),
            b"0_T",
            b"1_T",
            b"00_i8",
            b"-0_i64",
            b"999999999999999999999_u8",
            b"0.0_f32",
            b"1.00_f64",
            b"1.5e-0_f64",
        ] {
            assert!(is_literal(spelling));
        }
        for spelling in [
            b"2_T".as_slice(),
            b"1_i128",
            b"01.0_f32",
            b"1.0e01_f32",
            b"1.0e+1_f32",
            b"1.0_f16",
        ] {
            assert!(!is_literal(spelling));
        }
    }

    #[test]
    fn string_membership_checks_exact_raw_bytes() {
        for spelling in [b"\"\"".as_slice(), b"\"text\"", b"\"\\n\\\"\\\\\""] {
            assert!(is_string(spelling));
        }
        for spelling in [b"text".as_slice(), b"\"\\t\"", b"\"line\nfeed\""] {
            assert!(!is_string(spelling));
        }
    }

    #[test]
    fn membership_set_retains_noncompeting_overlap() {
        let mut set = TerminalSet::empty();
        set.insert(TerminalPredicate::Fixed(FixedTerminal::Unit));
        set.insert(TerminalPredicate::Literal);
        assert_eq!(set.len(), 2);
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            vec![
                TerminalPredicate::Fixed(FixedTerminal::Unit),
                TerminalPredicate::Literal,
            ]
        );
    }
}
