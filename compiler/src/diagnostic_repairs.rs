//! Wording of diagnostic repairs; owning stages select the repair and its instance.

mod reserved_names;

pub(crate) use reserved_names::{reserved_name_fix, source_word};

use crate::syntax::LayoutDifference;

pub(crate) const BOOLEAN_OPERATION_FIX: &str = "write `band(a, b)` for boolean conjunction and `bor(a, b)` for boolean disjunction; both operations are eager, so bind computed operands first with `let` in a body, or with preceding `define` bindings in a contract definition initializer when [FN-8] admits those operations [OP-1, GRAM-5, GRAM-9]";

/// RANGE-1's repair for `&&`, selected by the grammar position of a
/// `range_clause`, where `band` and `bor` are calls and so are not admitted.
pub(crate) const RANGE1_BOOLEAN_AND_FIX: &str = "a range clause admits no call, so `band` is not available there; join Bool tests with commas, each a variant relation testing one Bool datum, as in `when nodes^[k] is True, flags^[k] is True:`, and the same commas join conclusions [RANGE-1, GRAM-5]";

/// A raw `||` rejection has no grammar position; its repair states the
/// separate forms admitted in expressions and range clauses.
pub(crate) const BOOLEAN_DISJUNCTION_FIX: &str = "there is no `||`; in a body write eager `bor(a, b)` with computed operands bound first by `let`; for a contract definition or requires clause, when [FN-8] admits the operations and every operand name is visible in definitions before all clauses, bind computed operands and the `bor` result with fresh `define` bindings and use the result in the requires clause; an ordinary ensures clause admits no Boolean connective [FN-9]; a range clause admits no call or disjunction, so express a disjunctive guard as one range clause per disjunct with the same conclusion; a disjunctive conclusion has no direct range spelling [FORM-1, OP-1, GRAM-5, GRAM-9, RANGE-1]";

pub(crate) const CHARACTER_SUFFIX_FIX: &str = "write a character literal with its mandatory suffix: `u8` for ASCII or `u32` for any Unicode scalar, such as `'a'_u8` [FORM-5, FORM-7]";

pub(crate) const DECIMAL_INTEGER_FIX: &str = "write an integer literal in decimal with its mandatory type suffix, such as `255_u32` [FORM-5, FORM-7]";

pub(crate) const REMAINDER_MODE_FIX: &str = "write `%`, `%defined`, or `%checked` for remainder; `%wrap` is not an infix operator [GRAM-1, GRAM-5]";

/// A Rust array type `[T; N]` written where a type stands, or after `&`.
///
/// The expectation list at that position names the types the grammar admits
/// and never says that a constant-capacity array is `Array<T, N>`, so a
/// writer moved the Rust spelling around instead of replacing it.
pub(crate) const ARRAY_TYPE_FIX: &str = "there is no `[T; N]` type: a constant-capacity array is written `Array<T, N>`, so `[u8; 256]` is `Array<u8, 256>` and `Box<[u8; 256]>` is `Box<Array<u8, 256>>`; omit the capacity for a runtime-capacity array held in a `Box`, as in `Box<Array<u8>>`, and write a parameter's type `&[u8]` for a reference to a run of any length [GRAM-3, TYPE-9]";

/// `give if` and `give match`: a conditional is a statement or a `let`
/// initializer, never the expression `give` takes, so the lexical-looking
/// FORM-3 refusal of `if` in an expression slot is not what the writer needs.
pub(crate) const GIVE_CONDITIONAL_FIX: &str = "`give` takes an expression, and `if` and `match` are not expressions: a conditional value is only a `let` initializer [GRAM-4, GRAM-7, GIVE-1]; to deliver from branches, write the `if` or `match` here as a statement whose every branch ends in its own `give`, as in `if c { give a; } else { give b; }`, or bind the conditional first with `let x = if c { give a; } else { give b; }` (no `;` after its closing `}`) and write `give x;`";

/// An `if` or `match` where an operand stands (after `return`, in a call's
/// argument, in a `set`): the same refusal as after `give`, whose statement
/// form does not apply, so the only repair is the `let` initializer.
pub(crate) const CONDITIONAL_OPERAND_FIX: &str = "an `if` or `match` is not an expression: a conditional value is only a `let` initializer [GRAM-4, GRAM-7, GIVE-1]; bind it first, as in `let x = if c { give a; } else { give b; }` (no `;` after its closing `}`), and write `x` where the conditional stood";

pub(crate) const FLAT_OPERATION_FIX: &str = "bind each operation with its own preceding `let` and pass the binding as the operand; an expression has one operation over two atoms, with no chaining or parenthesized subexpressions [GRAM-6, GRAM-9]";

pub(crate) const EFFECT_ROW_FIX: &str = "write an effect row after the result binding: `pure` for no effects, or `reads(path)` and `writes(path)` entries describing the body's reference accesses; write `waits` after that row [GRAM-2, EFF-1]";

pub(crate) const ARRAY_FIX: &str = "replace the expression list with `array_filled` or `box_array_filled` for repeated copy elements, or `slots_new` followed by `place_back` for individual elements [OP-13]; for static elements, declare `const values: Array<u8, 2> =[1_u8, 2_u8];` and use `values` [CONST-2]";

pub(crate) const VALUE_MATCH_FIX: &str = "remove the `;` after this value match: a `let` initializer `match` ends at its closing `}` [GRAM-4, GRAM-7]";

pub(crate) const LOOP_FIX: &str = "remove the empty parentheses and write `loop {` when the loop has no header invariant [GRAM-4]";

pub(crate) const INVARIANT_FIX: &str = "name this loop-header relation as `invariant name: left <= right`, with a fresh lowercase name before the colon [GRAM-4]";

pub(crate) const MATCH_BINDER_FIX: &str = "write named field binders as `Variant(field: binder)` in declared field order, for example `Some(value: x)`; a final `..` covers omitted fields [GRAM-10]";

pub(crate) const LET_MUT_FIX: &str = "remove `mut` and write `let x = value;`; `let` derives the binding's mode and type, and assignment uses `set x = value;` [GRAM-4, TYPE-5, SET-1]";

pub(crate) const LET_TYPE_FIX: &str = "remove the type annotation and write `let x = value;`; the initializer determines the binding's mode and type [GRAM-4, TYPE-5]";

pub(crate) const BORROW_MUT_FIX: &str = "remove `mut` and write `&p`; references have no permission marker, and a callee's effect row declares writes through its reference parameters [GRAM-5, REF-1, EFF-1]";

pub(crate) const ALIAS_FIX: &str = "replace the import with an `alias` declaration, for example `alias process = std::process;`; in a module program, an alias names a module listed in this module's graph row [GRAM-2, MOD-4, MOD-5]";

pub(crate) const ENTRY_FIX: &str = "remove `entry` before `fn`; declare the entry in the module graph file, for example `entry app = pkg::main;` [GRAM-2, MOD-1]";

pub(crate) const GRAPH_FIX: &str = "write graph declarations as `package name = \"location\";`, then module rows `module_path: [dependency_path, ...];`, then entries `entry name = module_path;`; examples are `package util = \"../util\";`, `pkg::geometry: [];`, and `entry app = pkg::geometry::main;` [GRAM-2]";

/// GRAM-9's repairs for body expressions and contract definition initializers.
pub(crate) const GRAM9_BODY_FIX: &str = "a `call` or `construct` in an atom position does not derive [GRAM-9]: bind the inner call with its own preceding `let` in this body and write that binder in the atom position — `let inner = f(x: 0_u64); let outer = g(y: inner);`";
pub(crate) const GRAM9_CONTRACT_FIX: &str = "if the inner operation is a non-consuming, pure, total form admitted by [FN-8], bind it with a fresh preceding `define` in this contract block and use that binder in the atom position; for example, split `define wide = cvt::<u32, u64>(cvt::<u8, u32>(value));` into `define inner = cvt::<u8, u32>(value); define wide = cvt::<u32, u64>(inner);` [GRAM-2, GRAM-9]";

/// [FORM-3]'s lexical class for each name slot, with the repair a slot filled
/// from another class admits.
///
/// A name slot's class is a grammar position: `const_decl` writes IDENT,
/// `struct_decl` writes TYPEID, and no class is admitted in another's slot.
/// The expectation list names the class and never says what the class is, so
/// a writer who spelled a const `Limit` read only `expected: ["IDENT"]`.
pub(crate) const FORM3_IDENT_FIX: &str = "an IDENT slot admits only [FORM-3]'s IDENT `[a-z][a-z0-9_]*`, so a `const`, `fn`, parameter, `let`, field, or binder name is lowercase and is never a TYPEID `[A-Z][A-Za-z0-9]*`, a LABEL `@[a-z][a-z0-9_]*`, or an OPNAME; rename the name written here to the IDENT shape";
pub(crate) const FORM3_TYPEID_FIX: &str = "a TYPEID slot admits only [FORM-3]'s TYPEID `[A-Z][A-Za-z0-9]*`, so a struct, enum, contract, variant, or constructor name is capitalized and is never an IDENT `[a-z][a-z0-9_]*`, a LABEL `@[a-z][a-z0-9_]*`, or an OPNAME; rename the name written here to the TYPEID shape";
pub(crate) const FORM3_LABEL_FIX: &str = "a LABEL slot admits only [FORM-3]'s LABEL `@[a-z][a-z0-9_]*`, so write the leading `@`; an IDENT `[a-z][a-z0-9_]*`, a TYPEID `[A-Z][A-Za-z0-9]*`, and an OPNAME are other lexical classes and none is admitted here";

/// [GRAM-2] fixes the order of a `contract_block`: every `contract_define`,
/// then every `requires_clause`, then every `ensures_clause`. A clause written
/// out of that order leaves the frontier at a position whose expectation list
/// names only the sections still open, which states what is admitted next
/// without ever stating the order that was broken.
pub(crate) const GRAM2_CONTRACT_ORDER_FIX: &str = "a `contract_block` is written in one fixed order: all `define` definitions first, then all `requires` requirements, then all `ensures` postconditions. A clause of an earlier section written after a later one is not admitted, so move it above the first clause of the later section";

/// Formats the lowercase spelling selected by a rejected upper word.
pub(crate) fn lowercase_name(spelling: &str) -> String {
    format!(
        "write this name in lower case as `{}` [FORM-3]",
        spelling.to_ascii_lowercase()
    )
}

/// Formats the accessible names already selected, ordered and capped by resolution.
pub(crate) fn qualified_name(spelling: &str, module_name: &str, names: &[&str]) -> String {
    if names.is_empty() {
        return format!(
            "declare an accessible name in `{module_name}` for this grammar position [MOD-5]"
        );
    }
    let choices = names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "replace `{spelling}` with an accessible name declared in `{module_name}` for this grammar position: {choices} [MOD-5]"
    )
}

pub(crate) const FLAT_CONTRACT_OPERATION_FIX: &str = "if the operations are admitted by [FN-8], split this definition initializer into preceding definitions with fresh binders and pass each binder as the operand; each initializer has one operation over two atoms [GRAM-2, GRAM-6, GRAM-9]";

/// A bare Bool is tested by a variant relation in a range guard [RANGE-1].
pub(crate) const RANGE1_BARE_BOOL_FIX: &str = "a range guard is one comparison or one variant relation, and a range postcondition conclusion may also state `unchanged` [RANGE-1]; a Bool datum written alone does not derive: test it with `is True` or `is False`, as in `when nodes^[k].live is True:`";

/// The longest terminal spelling a layout change quotes on each side of its
/// gap; a longer one keeps the characters nearest the gap.
const LAYOUT_NEIGHBOR_CHARS: usize = 24;

/// The repair of a FORM-2 rejection: every terminal gap of the rejected
/// source whose bytes differ from the canonical ones, each as the exact bytes
/// to write between its two terminals.
///
/// [FORM-2] reports only the first difference, and a writer who learns of one
/// per compile spends a round on each. The rejection still names that first
/// difference; this sentence lists them all, so one round repairs the layout.
/// `total` counts every difference, `listed` the ones the sentence names.
pub(crate) fn canonical_layout_fix(total: u64, listed: &[LayoutDifference]) -> String {
    let shown = u64::try_from(listed.len()).unwrap_or(u64::MAX);
    let mut sentence = if total == 1 {
        "make the change below to reach the canonical layout; it gives the bytes FORM-2 requires between two terminals [FORM-2]: ".to_owned()
    } else if total > shown {
        format!(
            "{total} gaps differ from the canonical layout; make the {shown} changes below, then compile again; each gives the bytes FORM-2 requires between two terminals [FORM-2]: "
        )
    } else {
        format!(
            "make all {total} changes below to reach the canonical layout, then compile again; each gives the bytes FORM-2 requires between two terminals [FORM-2]: "
        )
    };
    for (index, difference) in listed.iter().enumerate() {
        if index > 0 {
            sentence.push_str("; ");
        }
        sentence.push_str(&layout_change(difference));
    }
    if total > shown {
        sentence.push_str(&format!("; and {} more after these", total - shown));
    }
    sentence
}

/// One change: where the gap is, the terminals around it, and the bytes
/// FORM-2 requires there instead of the bytes the source has.
fn layout_change(difference: &LayoutDifference) -> String {
    let line = difference.line;
    let place = match (&difference.before, &difference.after) {
        (Some(before), Some(after)) => format!(
            "line {line} between `{}` and `{}`",
            neighbor(before, true),
            neighbor(after, false)
        ),
        (Some(before), None) => {
            format!(
                "line {line} after `{}`, at the end of the source",
                neighbor(before, true)
            )
        }
        (None, Some(after)) => {
            format!(
                "line {line} before `{}`, at the start of the source",
                neighbor(after, false)
            )
        }
        (None, None) => format!("line {line}, in a source with no items"),
    };
    format!(
        "{place}: write \"{}\" instead of \"{}\"",
        escaped(&difference.expected),
        escaped(&difference.found)
    )
}

/// The characters of a neighboring terminal nearest the gap: its tail when it
/// precedes the gap, its head when it follows it.
fn neighbor(spelling: &str, tail: bool) -> String {
    let count = spelling.chars().count();
    if count <= LAYOUT_NEIGHBOR_CHARS {
        return spelling.to_owned();
    }
    let skip = count - LAYOUT_NEIGHBOR_CHARS;
    if tail {
        let kept: String = spelling.chars().skip(skip).collect();
        format!("...{kept}")
    } else {
        let kept: String = spelling.chars().take(LAYOUT_NEIGHBOR_CHARS).collect();
        format!("{kept}...")
    }
}

/// Trivia bytes inside a double-quoted string: a line break, tab, quote or
/// backslash as its escape, and any other control byte as `\x..`.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            control if control.is_control() => {
                out.push_str(&format!("\\x{:02x}", u32::from(control)));
            }
            other => out.push(other),
        }
    }
    out
}
