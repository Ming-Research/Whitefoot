//! What a reserved lowercase spelling is, and the rename that frees a name
//! written with one [FORM-3, OP-1].
//!
//! Three closed sets take a lowercase spelling away from names: the fixed
//! words of the grammar, which IDENT excludes [FORM-3], and the mode words
//! and dotless operation names, which no declaration may use [OP-1]. A
//! rejection that only says the slot expected an IDENT, or names the set by
//! an index into it, leaves the writer to guess why `copy` or `checked` is not
//! a name. Each sentence here is derived from the set's own data, the
//! generated grammar tables and the operation table, so an amendment that adds
//! a word or moves one to another production changes the sentence without an
//! edit here. The one written datum is the phrase that names each production a
//! fixed lowercase word occurs in; a test fails when such a production has
//! none.

use crate::resolution::{OPERATION_FAMILIES, ReservedNameClass, reserved_name};
use crate::syntax::SyntaxRule;
use crate::syntax::grammar::{
    GrammarNodeId, GrammarNodeKind, LookaheadPredicate, Production, grammar_node, productions,
};
use crate::syntax::terminal::{FixedTerminal, TerminalPredicate, is_identifier};

/// The closed set that takes one lowercase spelling away from names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Reservation {
    /// A fixed word of the grammar, which IDENT excludes [FORM-3].
    GrammarWord(FixedTerminal),
    /// A mode word or a dotless operation name [OP-1].
    LowerName(ReservedNameClass),
}

/// The repair for a name written with a reserved spelling: what reserves it,
/// and a fresh spelling to write at the declaration and every use instead.
///
/// `taken` says whether a candidate spelling is already used where the
/// rejection can see, so the suggested name never collides with one the
/// program writes. `None` when `spelling` is not reserved.
pub(crate) fn reserved_name_fix(spelling: &str, taken: impl Fn(&str) -> bool) -> Option<String> {
    let reason = description(spelling, reservation(spelling)?)?;
    let fresh = fresh_name(spelling, &taken);
    Some(format!(
        "`{spelling}` is reserved: {reason}; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `{fresh}`"
    ))
}

/// Whether `word` occurs in `source` as a complete name: a maximal run of
/// ASCII letters, digits and `_`.
pub(crate) fn source_word(source: &[u8], word: &str) -> bool {
    source
        .split(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'))
        .any(|candidate| candidate == word.as_bytes())
}

fn reservation(spelling: &str) -> Option<Reservation> {
    if is_lower_word(spelling)
        && let Some(terminal) = FixedTerminal::from_spelling(spelling.as_bytes())
    {
        return Some(Reservation::GrammarWord(terminal));
    }
    reserved_name(spelling).map(|(class, _)| Reservation::LowerName(class))
}

/// FORM-3's raw lowercase-word shape `[a-z][a-z0-9_]*`.
fn is_lower_word(spelling: &str) -> bool {
    spelling
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase())
        && spelling
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn description(spelling: &str, reservation: Reservation) -> Option<String> {
    match reservation {
        Reservation::GrammarWord(terminal) => {
            let places = occupants(terminal)
                .into_iter()
                .map(|production| {
                    let rule = SyntaxRule::from(production.owner()).id();
                    phrase(production).map(|phrase| format!("{phrase} [{rule}]"))
                })
                .collect::<Option<Vec<_>>>()?;
            (!places.is_empty()).then(|| {
                format!(
                    "it is a fixed word of the grammar of {}, and IDENT excludes every fixed grammar word [FORM-3]",
                    series(&places)
                )
            })
        }
        Reservation::LowerName(ReservedNameClass::ModeWord) => {
            let spellings = mode_spellings(spelling)
                .into_iter()
                .map(|family| format!("`{family}`"))
                .collect::<Vec<_>>();
            (!spellings.is_empty()).then(|| {
                format!(
                    "it is a mode word, the mode suffix of the operation spellings {}, and no declaration may use a mode word [OP-1, FORM-3]",
                    series(&spellings)
                )
            })
        }
        Reservation::LowerName(ReservedNameClass::DotlessOperation) => Some(format!(
            "it is the name of the operation family `{spelling}`, which an IDENT callee spelled `{spelling}` resolves to, and no declaration may use a dotless operation name [OP-1, FORM-3]"
        )),
    }
}

/// Every production, in specification order, whose own right-hand side
/// writes `terminal`; a referenced production is its own occupant.
fn occupants(terminal: FixedTerminal) -> Vec<Production> {
    productions()
        .iter()
        .copied()
        .filter(|production| writes(production.root(), terminal))
        .collect()
}

fn writes(node: GrammarNodeId, terminal: FixedTerminal) -> bool {
    let Some(record) = grammar_node(node) else {
        return false;
    };
    match record.kind() {
        GrammarNodeKind::Production(_) => false,
        GrammarNodeKind::TerminalSequence => {
            record
                .terminals()
                .contains(&LookaheadPredicate::Terminal(TerminalPredicate::Fixed(
                    terminal,
                )))
        }
        _ => record
            .children()
            .iter()
            .any(|child| writes(*child, terminal)),
    }
}

/// The operation spellings a mode word suffixes, in operation-table order:
/// the operator forms such as `+checked` and the dotted names such as
/// `ineg.checked` [GRAM-1, OP-1].
fn mode_spellings(mode: &str) -> Vec<&'static str> {
    OPERATION_FAMILIES
        .iter()
        .copied()
        .filter(|family| {
            family.strip_suffix(mode).is_some_and(|prefix| {
                prefix.ends_with('.')
                    || (!prefix.is_empty()
                        && !prefix.bytes().any(|byte| byte.is_ascii_alphanumeric()))
            })
        })
        .collect()
}

/// `a`, `a and b`, `a, b and c`.
fn series(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// `spelling` with `_value`, or with `_value2`, `_value3` and on when that
/// is taken: an IDENT that no set reserves and the program does not write.
fn fresh_name(spelling: &str, taken: &impl Fn(&str) -> bool) -> String {
    let base = format!("{spelling}_value");
    let free = |candidate: &str| {
        is_identifier(candidate.as_bytes()) && reservation(candidate).is_none() && !taken(candidate)
    };
    if free(&base) {
        return base;
    }
    (2..=u32::MAX)
        .map(|ordinal| format!("{base}{ordinal}"))
        .find(|candidate| free(candidate))
        .unwrap_or(base)
}

/// The construct a production derives, as a writer names it.
///
/// Only a production whose own right-hand side writes a fixed lowercase word
/// needs one; the test below fails for one that lacks it and for one listed
/// here that writes none.
const fn phrase(production: Production) -> Option<&'static str> {
    Some(match production {
        Production::Item => "a top-level item",
        Production::AliasDecl => "an alias declaration",
        Production::HeapDecl => "the no-heap program declaration",
        Production::StructDecl => "a struct declaration",
        Production::Field => "a struct field",
        Production::EnumDecl => "an enum declaration",
        Production::Vfield => "an enum variant's payload field",
        Production::FnDecl => "a function declaration",
        Production::ContractBlock => "a contract block",
        Production::ContractDefine => "a contract definition",
        Production::RequiresClause => "a requires clause",
        Production::HeldClause => "a held clause",
        Production::EnsuresClause => "an ensures clause",
        Production::ResultRoute => "a routed postcondition's result route",
        Production::InterfaceDecl => "an interface declaration",
        Production::BindingDecl => "a binding declaration",
        Production::FnSig => "an interface member's function signature",
        Production::FunctionArg => "a function type argument",
        Production::ConstDecl => "a const declaration",
        Production::Doc => "a documentation entry",
        Production::Gparam => "a generic parameter",
        Production::CapabilityBound => "a type parameter's capability bound",
        Production::PackageDecl => "a module graph's package binding",
        Production::ModulePath => "a module path",
        Production::EntryDecl => "a module graph's entry declaration",
        Production::Type => "a primitive type name",
        Production::TypePath => "a qualified type path",
        Production::LetStmt => "a let statement",
        Production::IfStmt => "an if statement",
        Production::ValueIf => "a value-if initializer",
        Production::PropagateLetRhs => "a propagating let initializer",
        Production::SetStmt => "a set statement",
        Production::ReturnStmt => "a return statement",
        Production::LoopStmt => "a loop statement",
        Production::ForStmt => "a counted for statement",
        Production::ForBinding => "a counted for statement's binding",
        Production::HeaderInvariant => "a loop-header invariant",
        Production::InvariantStmt => "a local invariant statement",
        Production::TypeInvariant => "a type invariant",
        Production::ProofUse => "a proof use step",
        Production::BreakStmt => "a break statement",
        Production::ContinueStmt => "a continue statement",
        Production::GiveStmt => "a give statement",
        Production::AtomicStmt => "an atomic statement",
        Production::MatchStmt => "a match statement",
        Production::ValueMatch => "a value-match initializer",
        Production::Arm => "a match arm",
        Production::ApartClause => "an apart certificate",
        Production::Atom => "an expression atom",
        Production::Call => "a call",
        Production::Callee => "a callee path",
        Production::ClauseExpr => "a contract clause expression",
        Production::Pbase => "a place's base",
        Production::RangeClause => "a range clause",
        Production::RangeBinder => "a range clause's binder",
        Production::RangeRelation => "a range relation",
        Production::RangeVariant => "a range variant relation",
        Production::Effects => "an effect row",
        Production::Effect => "an effect row entry",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{description, occupants, phrase, reservation, reserved_name_fix, writes};
    use crate::resolution::{MODE_WORDS, OPERATION_FAMILIES};
    use crate::syntax::grammar::productions;
    use crate::syntax::terminal::ALL_FIXED_TERMINALS;

    /// Every reserved lowercase spelling, from the three sets' own data.
    fn reserved_spellings() -> Vec<&'static str> {
        ALL_FIXED_TERMINALS
            .iter()
            .map(|terminal| terminal.spelling())
            .filter(|spelling| super::is_lower_word(spelling))
            .chain(MODE_WORDS)
            .chain(
                OPERATION_FAMILIES
                    .iter()
                    .copied()
                    .filter(|family| super::is_lower_word(family)),
            )
            .collect()
    }

    /// [FORM-3, OP-1] every reserved spelling has a class and a description
    /// of what occupies it, so no rejection of a name falls back to the bare
    /// expectation list or an index into a set.
    #[test]
    fn every_reserved_spelling_says_what_takes_it() {
        let spellings = reserved_spellings();
        // `copy`, a mode word and a dotless operation name: one of each set.
        for sample in ["copy", "checked", "band"] {
            assert!(spellings.contains(&sample), "{sample} is reserved");
        }
        for spelling in spellings {
            let class = reservation(spelling)
                .unwrap_or_else(|| panic!("`{spelling}` has no reservation class"));
            let sentence = description(spelling, class).unwrap_or_else(|| {
                panic!("`{spelling}` has no description of the grammar or operations that take it")
            });
            assert!(!sentence.is_empty(), "{spelling}");
        }
    }

    /// The phrase table holds exactly the productions that write a fixed
    /// lowercase word, so an amendment that adds such a production, or retires
    /// one, fails here until its phrase is written or removed.
    #[test]
    fn phrases_name_exactly_the_productions_that_hold_a_lowercase_word() {
        for production in productions() {
            let holds = ALL_FIXED_TERMINALS
                .iter()
                .filter(|terminal| super::is_lower_word(terminal.spelling()))
                .any(|terminal| writes(production.root(), *terminal));
            assert_eq!(
                phrase(*production).is_some(),
                holds,
                "{production:?}: a phrase exactly when its right-hand side writes a fixed lowercase word"
            );
        }
        for terminal in ALL_FIXED_TERMINALS {
            if super::is_lower_word(terminal.spelling()) {
                assert!(!occupants(terminal).is_empty(), "{terminal:?}");
            }
        }
    }

    #[test]
    fn the_sentence_names_the_occupant_and_a_fresh_name() {
        assert_eq!(
            reserved_name_fix("is", |_| false).as_deref(),
            Some(
                "`is` is reserved: it is a fixed word of the grammar of a routed postcondition's result route [GRAM-2] and a range variant relation [GRAM-5], and IDENT excludes every fixed grammar word [FORM-3]; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `is_value`"
            )
        );
        assert_eq!(
            reserved_name_fix("strict", |_| false).as_deref(),
            Some(
                "`strict` is reserved: it is a mode word, the mode suffix of the operation spellings `fadd.strict`, `fsub.strict`, `fmul.strict`, `fdiv.strict`, `fsqrt.strict` and `ffma.strict`, and no declaration may use a mode word [OP-1, FORM-3]; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `strict_value`"
            )
        );
        assert_eq!(
            reserved_name_fix("band", |_| false).as_deref(),
            Some(
                "`band` is reserved: it is the name of the operation family `band`, which an IDENT callee spelled `band` resolves to, and no declaration may use a dotless operation name [OP-1, FORM-3]; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `band_value`"
            )
        );
    }

    /// The suggested name never collides with one the program writes.
    #[test]
    fn a_taken_fresh_name_moves_to_the_next_ordinal() {
        let sentence = reserved_name_fix("copy", |candidate| {
            matches!(candidate, "copy_value" | "copy_value2")
        })
        .expect("`copy` is reserved");
        assert!(sentence.ends_with("such as `copy_value3`"), "{sentence}");
        assert!(super::source_word(b"let copy_value = 1_u8;", "copy_value"));
        assert!(!super::source_word(
            b"let copy_values = 1_u8;",
            "copy_value"
        ));
    }

    /// An ordinary IDENT and a retired spelling carry no reservation.
    #[test]
    fn a_free_spelling_has_no_repair() {
        for free in ["value", "len", "box_new", "Copy"] {
            assert_eq!(reserved_name_fix(free, |_| false), None, "{free}");
        }
    }
}
