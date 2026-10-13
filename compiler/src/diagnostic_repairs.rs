//! Wording of diagnostic repairs; owning stages select the repair and its instance.

pub(crate) const BOOLEAN_OPERATION_FIX: &str = "write `band(a, b)` for boolean conjunction and `bor(a, b)` for boolean disjunction; both operations are eager, so bind computed operands first with `let` in a body or `define` in a contract block [OP-1, GRAM-5, GRAM-9]";

/// RANGE-1's repair for `&&`, selected by the grammar position of a
/// `range_clause`, where `band` and `bor` are calls and so are not admitted.
pub(crate) const RANGE1_BOOLEAN_AND_FIX: &str = "a range clause admits no call, so `band` is not available there; join Bool tests with commas, each a variant relation testing one Bool datum, as in `when nodes^[k] is True, flags^[k] is True:`, and the same commas join conclusions [RANGE-1, GRAM-5]";

/// A raw `||` rejection has no grammar position; its repair states the
/// separate forms admitted in expressions and range clauses.
pub(crate) const BOOLEAN_DISJUNCTION_FIX: &str = "there is no `||`; in a body write eager `bor(a, b)` with computed operands bound first by `let`; in a contract block bind computed operands and the `bor` result with `define`, then use the result binding in an ordinary clause; a range clause admits no call or disjunction, so express a disjunctive guard as one range clause per disjunct with the same conclusion; a disjunctive conclusion has no direct range spelling [FORM-1, OP-1, GRAM-5, GRAM-9, RANGE-1]";

pub(crate) const CHARACTER_SUFFIX_FIX: &str = "write a character literal with its mandatory suffix: `u8` for ASCII or `u32` for any Unicode scalar, such as `'a'_u8` [FORM-5, FORM-7]";

pub(crate) const DECIMAL_INTEGER_FIX: &str = "write an integer literal in decimal with its mandatory type suffix, such as `255_u32` [FORM-5, FORM-7]";

pub(crate) const REMAINDER_MODE_FIX: &str = "write `%`, `%defined`, or `%checked` for remainder; `%wrap` is not an infix operator [GRAM-1, GRAM-5]";

pub(crate) const BLANK_LINE_FIX: &str = "remove the blank line inside this item [FORM-2]";

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

/// GRAM-9's repair in the two grammar positions that admit a binding.
pub(crate) const GRAM9_BODY_FIX: &str = "a `call` or `construct` in an atom position does not derive [GRAM-9]: bind the inner call with its own preceding `let` in this body and write that binder in the atom position — `let inner = f(x: 0_u64); let outer = g(y: inner);`";
pub(crate) const GRAM9_CONTRACT_FIX: &str = "a `call` or `construct` in an atom position does not derive [GRAM-9]: a `contract_block` has no `let`, so bind the inner call with a preceding `define` in this same block and write that binder in the atom position — `define inner = f(x: 0_u64); requires g(y: inner);`";

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

pub(crate) const FLAT_CONTRACT_OPERATION_FIX: &str = "bind each operation with its own preceding `define` in this contract block and pass the binding as the operand; a `contract_block` has no `let`, and each expression has one operation over two atoms [GRAM-2, GRAM-6, GRAM-9]";

/// A bare Bool is tested by a variant relation in a range guard [RANGE-1].
pub(crate) const RANGE1_BARE_BOOL_FIX: &str = "a range guard is one comparison or one variant relation, and a range postcondition conclusion may also state `unchanged` [RANGE-1]; a Bool datum written alone does not derive: test it with `is True` or `is False`, as in `when nodes^[k].live is True:`";
