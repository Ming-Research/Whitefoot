#[path = "repairs.rs"]
mod repairs;

use crate::diagnostic_repairs::{
    FLAT_CONTRACT_OPERATION_FIX, FLAT_OPERATION_FIX, FORM3_IDENT_FIX, FORM3_LABEL_FIX,
    FORM3_TYPEID_FIX, GRAM2_CONTRACT_ORDER_FIX, GRAM9_BODY_FIX, GRAM9_CONTRACT_FIX,
    RANGE1_BARE_BOOL_FIX,
};
use crate::lexer::TokenKind;
use crate::syntax::grammar::{
    Decision, DecisionContext, DecisionKind, GrammarNodeId, GrammarNodeKind, LookaheadPredicate,
    NamePredicate, Production, SelectAtom, SelectRow, grammar_node, grammar_prefixes,
};
use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};
use crate::{ByteOffset, SourceId};

use crate::ClassifiedToken;

use super::{
    ExpectedBuilder, ParseCompilerFailure, ParseLimit, ParseLimits, ParseResourceFailure,
    ParseStorage, QualifiedValuePath, SyntaxCoordinate, SyntaxIssue, SyntaxRepair, SyntaxRule,
    Work,
};

pub(crate) enum DiagnosticResult {
    Issue(SyntaxIssue),
    Resource(ParseResourceFailure),
    Compiler(ParseCompilerFailure),
}

pub(crate) enum DecisionSelection {
    Arm(u8),
    NoMatch,
    Conflict,
}

#[derive(Clone, Copy)]
pub(crate) struct DiagnosticSite<'tokens> {
    pub(crate) source: SourceId,
    pub(crate) source_len: u64,
    pub(crate) bundle: &'tokens crate::SourceBundle,
    pub(crate) tokens: &'tokens [ClassifiedToken],
    pub(crate) cursor: usize,
    pub(crate) limits: ParseLimits,
}

#[derive(Clone, Copy)]
pub(crate) struct ProbeContext {
    pub(crate) production: Production,
    pub(crate) atom_only: bool,
    /// Whether an open production frame is the `contract_block` of GRAM-2.
    pub(crate) in_contract: bool,
    /// Only a definition initializer can be split into preceding definitions
    /// without moving its operands out of their grammar scope. Result data,
    /// routed payloads and range binders are unavailable there [TYPE-6, FN-9].
    pub(crate) in_contract_define: bool,
    /// An open production frame is a `range_clause`, whose relations admit no call.
    pub(crate) in_range: bool,
    /// The nearest atom-bearing grammar role is an `infix_tail` operand.
    pub(crate) infix_operand: bool,
    /// The just-completed `infix_tail`'s final `atom` child, when no terminal
    /// has been consumed since. Strong-LL(2) recognition can finish this atom
    /// using another caller's continuation before the next frontier rejects;
    /// DIAG-1 row 2 still attributes that occurrence's two-token infix start.
    pub(crate) completed_infix_operand: Option<SyntaxCoordinate>,
    /// The preceding completed statement has a value-match initializer.
    pub(crate) after_value_match: bool,
    /// The grammar is inside an expression, including its callee/type arguments.
    pub(crate) in_expression: bool,
    /// The nearest field-binding role is a match arm, not a result route or let.
    pub(crate) match_binder: bool,
    /// The qualified path a `call` in a value position has consumed when the
    /// token after its final IDENT continues no call [DIAG-1, MOD-5].
    ///
    /// Only the open frames record that the path is a `callee` of a `call`
    /// an `expr` or `affine_factor` derives, so the parser reads it from them
    /// at the failing frontier; a production the diagnostic probe enters
    /// starts without it.
    pub(crate) qualified_value: Option<QualifiedValueSite>,
}

/// A qualified path with the index of the token after its final IDENT.
#[derive(Clone, Copy)]
pub(crate) struct QualifiedValueSite {
    pub(crate) path: QualifiedValuePath,
    pub(crate) after: usize,
}

impl QualifiedValueSite {
    /// Whether a frontier whose lookahead begins at `cursor` lies at the
    /// path's final IDENT or at the token after it [DIAG-1].
    ///
    /// Recognition stops at either. When an operator follows the path, as in
    /// `pkg::m::k == 1`, no `callee_path` row starts `IDENT ==`, so the
    /// `callee_path` choice fails with its lookahead at the final IDENT; when
    /// a `;` follows, the `IDENT ;` row of `fn_bind`'s continuation admits the
    /// IDENT and the next decision fails at the `;`.
    pub(crate) fn applies_at(self, cursor: usize) -> bool {
        self.after == cursor || cursor.checked_add(1) == Some(self.after)
    }
}

/// The repair for a qualified path written where a value stands [MOD-5]: the
/// alias that binds its final name [MOD-4], with that name written in the
/// path's place.
///
/// The path is a `callee` spelling, so a writer who meant another module's
/// const reads only that a call was expected. An alias path begins at `pkg`,
/// `std` or a bound package name and never at a module alias, which the
/// grammar cannot tell from a bound name, so an IDENT-rooted path also says
/// how to spell its root. A const of the record's own module would collide
/// with such an alias, so the sentence names the bare name for it too.
pub(crate) fn qualified_value_fix(path: &str, root: &str, name: &str) -> String {
    let root_spelling = if root == "pkg" || root == "std" {
        String::new()
    } else {
        format!(
            ", spelling `{root}` as the complete module path it stands for when it is a module alias, since an alias path begins at `pkg`, `std` or a bound package name"
        )
    };
    format!(
        "a qualified path names only a callee, type, construction or destructuring target, never a value [MOD-5]: to read the const `{name}` here, add `alias {name} = {path};` to this record's alias header and write `{name}` in place of `{path}` [MOD-4]{root_spelling}; a const of this record's own module is written `{name}` with no alias"
    )
}

/// The repair the failing production admits, when the production itself fixes
/// one. The decision is the grammar position, never the text of the line.
const fn production_fix(production: Production) -> Option<&'static str> {
    match production {
        Production::ContractBlock => Some(GRAM2_CONTRACT_ORDER_FIX),
        Production::RangeRelation => Some(RANGE1_BARE_BOOL_FIX),
        _ => None,
    }
}

/// The repair a name slot admits, selected by the lexical class the slot's
/// grammar position writes.
///
/// OPNAME is the one class no slot admits alone: the grammar's single OPNAME
/// atom is `callee`'s second row, whose frontier also carries `callee`'s IDENT
/// row, so the two transparent names disagree and this selection is never
/// reached with OPNAME. It carries no sentence rather than an unreachable one.
const fn name_class_fix(expected: NamePredicate) -> Option<&'static str> {
    match expected {
        NamePredicate::Identifier => Some(FORM3_IDENT_FIX),
        NamePredicate::TypeIdentifier => Some(FORM3_TYPEID_FIX),
        NamePredicate::Label => Some(FORM3_LABEL_FIX),
        NamePredicate::OperationName => None,
    }
}

#[derive(Clone, Copy)]
enum ProbeTask {
    Execute(GrammarNodeId, ProbeContext),
    Continue(GrammarNodeId, ProbeContext),
    Match(TerminalPredicate, ProbeContext),
    Boundary,
}

/// Unconsumed grammar work, in execution order, with finished frames omitted.
#[derive(Clone, Copy)]
pub(crate) enum Continuation {
    Node(GrammarNodeId),
    Repeat(GrammarNodeId),
    Terminal(TerminalPredicate),
}

impl ProbeTask {
    fn continuation(self) -> Option<Continuation> {
        match self {
            Self::Execute(node, _) => Some(Continuation::Node(node)),
            Self::Continue(node, _) => Some(Continuation::Repeat(node)),
            Self::Match(terminal, _) => Some(Continuation::Terminal(terminal)),
            Self::Boundary => None,
        }
    }
}

fn same_occurrence(left: SelectAtom, right: SelectAtom) -> bool {
    left.predicate() == right.predicate()
        && left.provenance() == right.provenance()
        && left.transparent_name() == right.transparent_name()
        && left.is_atom_only() == right.is_atom_only()
}

/// A global row belongs to this call instance exactly when its outside-arm
/// suffix is a prefix of the actual pending continuation. FIRST(2) words
/// carry their length, so nullable tasks advance to the next pending task;
/// SOURCE_END is introduced only after that complete continuation ends.
fn contextual_row(
    decision: Decision,
    row: SelectRow,
    continuation: impl Iterator<Item = Continuation>,
    work: &mut Work,
) -> Result<bool, DiagnosticResult> {
    let atoms = [row.position(0), row.position(1)];
    let [Some(first), Some(second)] = atoms else {
        return Err(DiagnosticResult::Compiler(
            ParseCompilerFailure::InvalidGrammarData,
        ));
    };
    let suffix = if !first.is_inside_arm() {
        [Some(first), Some(second)]
    } else if !second.is_inside_arm() {
        [Some(second), None]
    } else {
        return Ok(true);
    };
    let length = if suffix[1].is_some() { 2 } else { 1 };
    let repeated = (row.arm() == 0
        && matches!(decision.kind(), DecisionKind::Repeat0 | DecisionKind::Repeat1))
        .then_some(Continuation::Repeat(decision.node()));
    let mut positions = [true, false];
    for task in repeated.into_iter().chain(continuation) {
        let mut next = [false; 2];
        for consumed in 0..length {
            if !positions[consumed] {
                continue;
            }
            if let Continuation::Terminal(terminal) = task {
                work.spend(1).map_err(DiagnosticResult::Resource)?;
                if suffix[consumed].is_some_and(|atom| {
                    atom.predicate() == LookaheadPredicate::Terminal(terminal)
                }) {
                    if consumed + 1 == length {
                        return Ok(true);
                    }
                    next[consumed + 1] = true;
                }
                continue;
            }
            let (node, repeat_tail) = match task {
                Continuation::Node(node) => (node, false),
                Continuation::Repeat(node) => (node, true),
                Continuation::Terminal(_) => {
                    return Err(DiagnosticResult::Compiler(
                        ParseCompilerFailure::InvalidGrammarData,
                    ));
                }
            };
            for word in grammar_prefixes(node, repeat_tail) {
                work.spend(1).map_err(DiagnosticResult::Resource)?;
                let mut position = consumed;
                let mut matches = true;
                for atom in word.iter().flatten() {
                    if !suffix[position].is_some_and(|expected| same_occurrence(expected, *atom)) {
                        matches = false;
                        break;
                    }
                    position += 1;
                    if position == length {
                        return Ok(true);
                    }
                }
                if matches {
                    next[position] = true;
                }
            }
        }
        positions = next;
        if !positions.iter().any(|position| *position) {
            return Ok(false);
        }
    }
    Ok((0..length).any(|consumed| {
        positions[consumed]
            && suffix[consumed..length].iter().all(|atom| {
                atom.is_some_and(|atom| atom.predicate() == LookaheadPredicate::SourceEnd)
            })
    }))
}

fn accepts(
    predicate: LookaheadPredicate,
    tokens: &[ClassifiedToken],
    cursor: usize,
    position: usize,
) -> Result<bool, ParseCompilerFailure> {
    let index = cursor
        .checked_add(position)
        .ok_or(ParseCompilerFailure::CounterOverflow)?;
    Ok(match (tokens.get(index), predicate) {
        (Some(token), LookaheadPredicate::Terminal(expected)) => {
            token.terminals().contains(expected)
        }
        (None, LookaheadPredicate::SourceEnd) => true,
        _ => false,
    })
}

fn row_score(
    row: SelectRow,
    tokens: &[ClassifiedToken],
    cursor: usize,
) -> Result<u8, ParseCompilerFailure> {
    let first = row
        .position(0)
        .ok_or(ParseCompilerFailure::InvalidGrammarData)?;
    if !accepts(first.predicate(), tokens, cursor, 0)? {
        return Ok(0);
    }
    let second = row
        .position(1)
        .ok_or(ParseCompilerFailure::InvalidGrammarData)?;
    Ok(if accepts(second.predicate(), tokens, cursor, 1)? {
        2
    } else {
        1
    })
}

pub(crate) fn select_arm(
    decision: Decision,
    tokens: &[ClassifiedToken],
    cursor: usize,
    work: &mut Work,
) -> Result<DecisionSelection, DiagnosticResult> {
    let mut selected = None;
    let first = tokens.get(cursor).map(|token| token.terminals());
    let predicates = first
        .into_iter()
        .flat_map(|terminals| terminals.iter().map(LookaheadPredicate::Terminal))
        .chain(first.is_none().then_some(LookaheadPredicate::SourceEnd));
    for row in predicates.flat_map(|predicate| decision.rows_starting_with(predicate)) {
        work.spend(1).map_err(DiagnosticResult::Resource)?;
        let second = row.position(1).ok_or(DiagnosticResult::Compiler(
            ParseCompilerFailure::InvalidGrammarData,
        ))?;
        if !accepts(second.predicate(), tokens, cursor, 1).map_err(DiagnosticResult::Compiler)? {
            continue;
        }
        match selected {
            Some(arm) if arm != row.arm() => return Ok(DecisionSelection::Conflict),
            Some(_) => {}
            None => selected = Some(row.arm()),
        }
    }
    Ok(selected.map_or(DecisionSelection::NoMatch, DecisionSelection::Arm))
}

fn boundary_coordinate(
    source: SourceId,
    source_len: u64,
    tokens: &[ClassifiedToken],
    cursor: usize,
    offset: usize,
) -> Result<SyntaxCoordinate, ParseCompilerFailure> {
    let index = cursor
        .checked_add(offset)
        .ok_or(ParseCompilerFailure::CounterOverflow)?;
    if let Some(token) = tokens.get(index) {
        let id = token.token().id();
        Ok(SyntaxCoordinate::new(source, id.start(), id.end()))
    } else {
        let end = ByteOffset::new(source_len);
        Ok(SyntaxCoordinate::new(source, end, end))
    }
}

fn has(token: &ClassifiedToken, predicate: TerminalPredicate) -> bool {
    token.terminals().contains(predicate)
}

fn fixed(token: &ClassifiedToken, terminal: FixedTerminal) -> bool {
    has(token, TerminalPredicate::Fixed(terminal))
}

fn dotted_override(
    source: SourceId,
    tokens: &[ClassifiedToken],
    boundary: usize,
    expected: super::ExpectedTerminals,
    work: &mut Work,
) -> Result<Option<SyntaxIssue>, ParseResourceFailure> {
    if boundary >= tokens.len() {
        return Ok(None);
    }
    let first_start = boundary.saturating_sub(3);
    for start in first_start..=boundary {
        work.spend(1)?;
        let Some(end) = start.checked_add(4) else {
            continue;
        };
        if boundary >= end {
            continue;
        }
        let Some(window) = tokens.get(start..end) else {
            continue;
        };
        if has(&window[0], TerminalPredicate::Identifier)
            && fixed(&window[1], FixedTerminal::Dot)
            && has(&window[2], TerminalPredicate::Identifier)
            && (fixed(&window[3], FixedTerminal::LeftParen)
                || fixed(&window[3], FixedTerminal::ColonColon))
        {
            return Ok(Some(SyntaxIssue {
                rule: SyntaxRule::Form3,
                coordinate: SyntaxCoordinate::new(
                    source,
                    window[0].token().id().start(),
                    window[2].token().id().end(),
                ),
                expected,
                mechanical_fix: None,
            }));
        }
    }
    Ok(None)
}

/// GRAM-9 does not require a repair [DIAG-1]. A clause cannot introduce a
/// local definition, and lifting its expressions can lose result/payload/range
/// scope or facts from earlier requirements. Offer no unverified hoist there.
fn atom_repair(context: ProbeContext, nested_infix: bool) -> Option<&'static str> {
    if context.in_contract_define {
        Some(if nested_infix {
            FLAT_CONTRACT_OPERATION_FIX
        } else {
            GRAM9_CONTRACT_FIX
        })
    } else if context.in_contract {
        None
    } else {
        Some(if nested_infix {
            FLAT_OPERATION_FIX
        } else {
            GRAM9_BODY_FIX
        })
    }
}

/// The qualified path `("pkg" | "std" | IDENT) "::" IDENT ("::" IDENT)*`
/// that begins at `start`, with the index of the token after its final
/// IDENT, when that token continues no call: it is neither `(` nor `::`.
pub(crate) fn qualified_value_path(
    source: SourceId,
    tokens: &[ClassifiedToken],
    start: usize,
    work: &mut Work,
) -> Result<Option<QualifiedValueSite>, ParseResourceFailure> {
    let Some(root) = tokens.get(start) else {
        return Ok(None);
    };
    if !(fixed(root, FixedTerminal::Pkg)
        || fixed(root, FixedTerminal::Std)
        || has(root, TerminalPredicate::Identifier))
    {
        return Ok(None);
    }
    let mut last = start;
    loop {
        work.spend(1)?;
        let (Some(colons), Some(name)) = (
            last.checked_add(1).and_then(|index| tokens.get(index)),
            last.checked_add(2).and_then(|index| tokens.get(index)),
        ) else {
            break;
        };
        if !(fixed(colons, FixedTerminal::ColonColon) && has(name, TerminalPredicate::Identifier)) {
            break;
        }
        let Some(next) = last.checked_add(2) else {
            break;
        };
        last = next;
    }
    let Some(after) = last.checked_add(1) else {
        return Ok(None);
    };
    if last == start
        || tokens.get(after).is_some_and(|next| {
            fixed(next, FixedTerminal::LeftParen) || fixed(next, FixedTerminal::ColonColon)
        })
    {
        return Ok(None);
    }
    let Some(name) = tokens.get(last) else {
        return Ok(None);
    };
    let coordinate = |token: &ClassifiedToken| {
        SyntaxCoordinate::new(source, token.token().id().start(), token.token().id().end())
    };
    Ok(Some(QualifiedValueSite {
        path: QualifiedValuePath::new(coordinate(root), coordinate(name)),
        after,
    }))
}

fn forbidden_atom_override(
    source: SourceId,
    tokens: &[ClassifiedToken],
    cursor: usize,
    context: ProbeContext,
    expected: super::ExpectedTerminals,
    work: &mut Work,
) -> Result<Option<SyntaxIssue>, ParseResourceFailure> {
    if let Some(atom) = context.completed_infix_operand
        && let Some(first) = cursor.checked_sub(1).and_then(|index| tokens.get(index))
        && let Some(second) = tokens.get(cursor)
        && atom.source() == source
        && atom.start() == first.token().id().start()
        && atom.end() == first.token().id().end()
        && repairs::is_infix_operator(second)
    {
        return Ok(Some(SyntaxIssue {
            rule: SyntaxRule::Gram9,
            coordinate: SyntaxCoordinate::new(
                source,
                first.token().id().start(),
                second.token().id().end(),
            ),
            expected,
            mechanical_fix: atom_repair(context, true).map(SyntaxRepair::Fixed),
        }));
    }
    if !context.atom_only {
        return Ok(None);
    }
    let (Some(first), Some(second)) = (
        tokens.get(cursor),
        cursor.checked_add(1).and_then(|index| tokens.get(index)),
    ) else {
        return Ok(None);
    };
    // [DIAG-1] row 2: a `call` start is a name followed by `(` or by the
    // `::` type-application or qualified-path delimiter, or `pkg ::` / `std ::`; a
    // `construct` start is a TYPEID followed by `(`, `<` or the `::` of a
    // type-owned variant [TYPE-6]. A call marker is followed by FIRST(callee):
    // IDENT, OPNAME, TYPEID, pkg or std. `IDENT <` is a comparison, never a call.
    let named_head =
        has(first, TerminalPredicate::Identifier) || has(first, TerminalPredicate::OperationName);
    let construct_head = has(first, TerminalPredicate::TypeIdentifier);
    let call_start = (named_head
        && (fixed(second, FixedTerminal::LeftParen) || fixed(second, FixedTerminal::ColonColon)))
        || ((fixed(first, FixedTerminal::Pkg) || fixed(first, FixedTerminal::Std))
            && fixed(second, FixedTerminal::ColonColon));
    let construct_start = construct_head
        && (fixed(second, FixedTerminal::LeftParen)
            || fixed(second, FixedTerminal::LeftAngle)
            || fixed(second, FixedTerminal::ColonColon));
    let marked_call_start = (fixed(first, FixedTerminal::Musttail)
        || fixed(first, FixedTerminal::Spawn))
        && (has(second, TerminalPredicate::Identifier)
            || has(second, TerminalPredicate::OperationName)
            || has(second, TerminalPredicate::TypeIdentifier)
            || fixed(second, FixedTerminal::Pkg)
            || fixed(second, FixedTerminal::Std));
    let nested_infix = (context.infix_operand || context.production == Production::InfixTail)
        && repairs::is_infix_operator(second);
    if !(call_start || construct_start || marked_call_start || nested_infix) {
        return Ok(None);
    }
    // A qualified path no call continues names a const through the wrong
    // spelling [MOD-5]. Its alias repair applies in body and contract scope.
    let qualified = if call_start {
        qualified_value_path(source, tokens, cursor, work)?
    } else {
        None
    };
    let repair = if let Some(site) = qualified {
        Some(SyntaxRepair::QualifiedValue(site.path))
    } else if marked_call_start || (construct_start && context.in_contract) {
        // A musttail call cannot be lifted to a let [FN-10], and spawn
        // admission depends on the callee [WAIT-3]. Construction is always
        // inadmissible in definitions [FN-8], so that route also fails.
        None
    } else {
        atom_repair(context, nested_infix).map(SyntaxRepair::Fixed)
    };
    Ok(Some(SyntaxIssue {
        rule: SyntaxRule::Gram9,
        coordinate: SyntaxCoordinate::new(
            source,
            first.token().id().start(),
            second.token().id().end(),
        ),
        expected,
        mechanical_fix: repair,
    }))
}

/// [DIAG-1] row 2's second clause: a `call` an `expr` or `affine_factor`
/// derives has consumed a qualified path that no `(` or `::` continues, so
/// the path names no callee and the rejection cites [MOD-5] at the complete
/// path, expecting the call continuation `::` or `(`.
fn qualified_value_override(context: ProbeContext, cursor: usize) -> Option<SyntaxIssue> {
    let site = context.qualified_value?;
    if !site.applies_at(cursor) {
        return None;
    }
    let mut expected = ExpectedBuilder::empty();
    for terminal in [FixedTerminal::ColonColon, FixedTerminal::LeftParen] {
        expected.insert(LookaheadPredicate::Terminal(TerminalPredicate::Fixed(
            terminal,
        )));
    }
    Some(SyntaxIssue {
        rule: SyntaxRule::Mod5,
        coordinate: site.path.path(),
        expected: expected.finish(),
        mechanical_fix: Some(SyntaxRepair::QualifiedValue(site.path)),
    })
}

fn raw_restriction_owner(
    token: &ClassifiedToken,
    expected: super::ExpectedTerminals,
) -> Option<SyntaxRule> {
    for predicate in expected.iter() {
        match predicate {
            LookaheadPredicate::Terminal(TerminalPredicate::Identifier)
                if token.token().kind() == TokenKind::LowerWordForm
                    && !has(token, TerminalPredicate::Identifier) =>
            {
                return Some(SyntaxRule::Form3);
            }
            LookaheadPredicate::Terminal(TerminalPredicate::Literal)
                if token.token().kind() == TokenKind::NumberForm
                    && !has(token, TerminalPredicate::Literal) =>
            {
                return Some(SyntaxRule::Form5);
            }
            // Unlike the two arms above, this one names a production's own
            // rule rather than a lexical class, so it must recognize that
            // production. Two positions share the grammar's sole pattern
            // predicate — a `const` size and a `proof_use` multiplicity — and
            // only the first is CONST-1's; blaming the second on CONST-1 cites
            // a rule whose text is about `array<T, N>` sizes and `const`
            // targs. A `const` is `"[0-9]+" | IDENT` and nothing else, so its
            // expected set is exactly those two, while a multiplicity position
            // also admits the `(` of an unmultiplied relation premise. Where
            // the shape does not match, the ordinary parse rule for the
            // position is the honest citation. A third production sharing the
            // predicate would need the production threaded here rather than a
            // second shape test.
            LookaheadPredicate::Terminal(TerminalPredicate::Digits)
                if token.token().kind() == TokenKind::NumberForm
                    && !has(token, TerminalPredicate::Digits)
                    && is_const_position(expected) =>
            {
                return Some(SyntaxRule::Const1);
            }
            _ => {}
        }
    }
    None
}

/// The rename a fixed lowercase word in an IDENT slot admits [FORM-3].
///
/// [`raw_restriction_owner`] cites FORM-3 only for a lowercase word that
/// IDENT excludes, which is a fixed grammar word, where IDENT is expected.
/// The rename is offered only where an IDENT in that place lets the
/// frontier's row match the token after it: `outer(musttail 0_u32)` meant a
/// call marker, and no name written in place of `musttail` derives there.
fn reserved_name_repair(
    rule: SyntaxRule,
    identifier_continues: bool,
    coordinate: SyntaxCoordinate,
) -> Option<SyntaxRepair> {
    (rule == SyntaxRule::Form3 && identifier_continues)
        .then_some(SyntaxRepair::ReservedName(coordinate))
}

/// Whether the expected set is exactly a `const`'s own two alternatives.
fn is_const_position(expected: super::ExpectedTerminals) -> bool {
    expected.iter().eq([
        LookaheadPredicate::Terminal(TerminalPredicate::Identifier),
        LookaheadPredicate::Terminal(TerminalPredicate::Digits),
    ])
}

fn actual_name(token: &ClassifiedToken) -> Option<NamePredicate> {
    [
        NamePredicate::Identifier,
        NamePredicate::TypeIdentifier,
        NamePredicate::Label,
        NamePredicate::OperationName,
    ]
    .into_iter()
    .find(|predicate| has(token, predicate.terminal()))
}

/// The owning rule and the lexical class the slot admits, for a name slot
/// filled from another class.
fn name_slot_owner(
    token: &ClassifiedToken,
    transparent: Option<NamePredicate>,
    paths_agree: bool,
) -> Option<(SyntaxRule, NamePredicate)> {
    let actual = actual_name(token)?;
    let expected = transparent?;
    (paths_agree && actual != expected).then_some((SyntaxRule::Form3, expected))
}

fn construct_override(
    context: DecisionContext,
    source: SourceId,
    tokens: &[ClassifiedToken],
    cursor: usize,
    expected: super::ExpectedTerminals,
) -> Option<SyntaxIssue> {
    if !matches!(
        context,
        DecisionContext::ConstructEntry | DecisionContext::ProgramItems
    ) {
        return None;
    }
    let token = tokens.get(cursor)?;
    if !has(token, TerminalPredicate::Identifier) {
        return None;
    }
    let id = token.token().id();
    Some(SyntaxIssue {
        rule: SyntaxRule::Form1,
        coordinate: SyntaxCoordinate::new(source, id.start(), id.end()),
        expected,
        mechanical_fix: None,
    })
}

fn program_leftover(
    context: DecisionContext,
    source: SourceId,
    tokens: &[ClassifiedToken],
    cursor: usize,
    maximum: u8,
) -> Option<SyntaxIssue> {
    if context != DecisionContext::ProgramItems {
        return None;
    }
    // [DIAG-1] row 5 owns the frontier only when the first actual token
    // predicate matches no consuming `item` row. A token is present here, so
    // the exit arm's SOURCE_END row scores zero and a nonzero maximal prefix
    // means some consuming row did accept that first token; the frontier then
    // belongs to the earlier rows or to ordinary traversal, not to leftover.
    if maximum != 0 {
        return None;
    }
    let token = tokens.get(cursor)?;
    let id = token.token().id();
    Some(SyntaxIssue {
        rule: SyntaxRule::Gram2,
        coordinate: SyntaxCoordinate::new(source, id.start(), id.end()),
        expected: ExpectedBuilder::only_end().finish(),
        mechanical_fix: None,
    })
}

struct Frontier {
    maximum: u8,
    expected: super::ExpectedTerminals,
    best_arm: Option<u8>,
    best_arm_internal: bool,
    transparent_name: Option<NamePredicate>,
    transparent_disagreement: bool,
    atom_only: bool,
    /// A maximal-prefix row has already matched a terminal of an atom.
    matched_atom_prefix: bool,
    /// A maximal-prefix row expects IDENT at the boundary and the token
    /// after the boundary can follow that IDENT, so writing a fresh IDENT in
    /// place of the boundary token lets the derivation continue past it
    /// [FORM-3, DIAG-1].
    identifier_continues: bool,
}

/// Whether `token` can follow the terminal occurrence `node` within the
/// production whose right-hand side writes it.
///
/// A row whose last position is the IDENT ends there, so the row cannot say
/// what follows it. The answer comes from the source EBNF: each enclosing
/// sequence's later children, and each enclosing repetition's own body, up to
/// the production's root. What follows the production itself depends on the
/// caller instance, which the occurrence does not record, so reaching the
/// root answers no: `outer(& musttail inner())` writes its IDENT in `pbase`,
/// and no rename of `musttail` derives there.
fn follows_in_production(node: GrammarNodeId, token: Option<&ClassifiedToken>) -> bool {
    let Some(token) = token else {
        return false;
    };
    let Some(path) = crate::syntax::grammar::productions()
        .iter()
        .find_map(|production| enclosing_path(production.root(), node))
    else {
        return false;
    };
    for (parent, index) in path.iter().rev() {
        let Some(record) = grammar_node(*parent) else {
            return false;
        };
        let children = record.children();
        match record.kind() {
            GrammarNodeKind::Sequence => {
                for sibling in children.iter().skip(index + 1) {
                    let (starts, nullable) = first_accepts(*sibling, token);
                    if starts {
                        return true;
                    }
                    if !nullable {
                        return false;
                    }
                }
            }
            GrammarNodeKind::RepeatZero | GrammarNodeKind::RepeatOne
                if children
                    .get(*index)
                    .is_some_and(|body| first_accepts(*body, token).0) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// The structural ancestors of `target` below `node`, outermost first, each
/// with the index of the child on the way down; `None` when `target` is not
/// in `node`'s tree. A production reference is a leaf here.
fn enclosing_path(
    node: GrammarNodeId,
    target: GrammarNodeId,
) -> Option<Vec<(GrammarNodeId, usize)>> {
    if node == target {
        return Some(Vec::new());
    }
    let record = grammar_node(node)?;
    record
        .children()
        .iter()
        .enumerate()
        .find_map(|(index, child)| {
            let mut path = enclosing_path(*child, target)?;
            path.insert(0, (node, index));
            Some(path)
        })
}

/// Whether `token` can begin `node`, and whether `node` derives the empty
/// string.
fn first_accepts(node: GrammarNodeId, token: &ClassifiedToken) -> (bool, bool) {
    let mut starts = false;
    let mut nullable = false;
    for word in grammar_prefixes(node, false) {
        match word[0] {
            Some(atom) => {
                starts |= matches!(
                    atom.predicate(),
                    LookaheadPredicate::Terminal(predicate) if token.terminals().contains(predicate)
                );
            }
            None => nullable = true,
        }
    }
    (starts, nullable)
}

fn frontier(
    decision: Decision,
    tokens: &[ClassifiedToken],
    cursor: usize,
    continuation: impl Iterator<Item = Continuation> + Clone,
    work: &mut Work,
) -> Result<Frontier, DiagnosticResult> {
    let arm_count = usize::from(decision.arm_count());
    if arm_count == 0 || arm_count > 64 {
        return Err(DiagnosticResult::Compiler(
            ParseCompilerFailure::InvalidGrammarData,
        ));
    }
    let mut arm_scores = [None; 64];
    let mut maximum = 0_u8;
    for row in decision.rows() {
        work.spend(1).map_err(DiagnosticResult::Resource)?;
        if !contextual_row(decision, *row, continuation.clone(), work)? {
            continue;
        }
        let score = row_score(*row, tokens, cursor).map_err(DiagnosticResult::Compiler)?;
        if score == 2 {
            return Err(DiagnosticResult::Compiler(
                ParseCompilerFailure::PredictiveConflict,
            ));
        }
        let arm = usize::from(row.arm());
        if arm >= arm_count {
            return Err(DiagnosticResult::Compiler(
                ParseCompilerFailure::InvalidGrammarData,
            ));
        }
        arm_scores[arm] = Some(arm_scores[arm].map_or(score, |old: u8| old.max(score)));
        maximum = maximum.max(score);
    }
    if arm_scores[..arm_count].iter().all(Option::is_none) {
        return Err(DiagnosticResult::Compiler(
            ParseCompilerFailure::InvalidGrammarData,
        ));
    }
    let mut expected = ExpectedBuilder::empty();
    let mut transparent_name = None;
    let mut transparent_disagreement = false;
    let mut atom_only = false;
    let mut matched_atom_prefix = false;
    let mut identifier_continues = false;
    for row in decision.rows() {
        work.spend(1).map_err(DiagnosticResult::Resource)?;
        if !contextual_row(decision, *row, continuation.clone(), work)? {
            continue;
        }
        if row_score(*row, tokens, cursor).map_err(DiagnosticResult::Compiler)? != maximum {
            continue;
        }
        let atom = row
            .position(usize::from(maximum))
            .ok_or(DiagnosticResult::Compiler(
                ParseCompilerFailure::InvalidGrammarData,
            ))?;
        expected.insert(atom.predicate());
        if atom.predicate() == LookaheadPredicate::Terminal(TerminalPredicate::Identifier) {
            identifier_continues |= match row.position(usize::from(maximum) + 1) {
                Some(next) => accepts(next.predicate(), tokens, cursor, usize::from(maximum) + 1)
                    .map_err(DiagnosticResult::Compiler)?,
                // The row ends at the IDENT, so it says nothing about the
                // token after it; ask the production that writes the IDENT.
                None => atom.provenance().is_some_and(|occurrence| {
                    follows_in_production(occurrence, tokens.get(cursor + usize::from(maximum) + 1))
                }),
            };
        }
        atom_only |= atom.is_atom_only();
        matched_atom_prefix |=
            maximum > 0 && row.position(0).is_some_and(|first| first.is_atom_only());
        if let Some(name) = atom.transparent_name() {
            match transparent_name {
                Some(previous) if previous != name => transparent_disagreement = true,
                Some(_) => {}
                None => transparent_name = Some(name),
            }
        }
    }
    let best_arms = arm_scores[..arm_count]
        .iter()
        .filter(|score| **score == Some(maximum))
        .count();
    let best_arm = if best_arms == 1 {
        arm_scores[..arm_count]
            .iter()
            .position(|score| *score == Some(maximum))
            .and_then(|index| u8::try_from(index).ok())
    } else {
        None
    };
    let mut best_arm_internal = best_arm.is_some();
    if let Some(arm) = best_arm {
        for row in decision.rows() {
            work.spend(1).map_err(DiagnosticResult::Resource)?;
            if row.arm() != arm
                || row_score(*row, tokens, cursor).map_err(DiagnosticResult::Compiler)? != maximum
                || !contextual_row(decision, *row, continuation.clone(), work)?
            {
                continue;
            }
            let atom = row
                .position(usize::from(maximum))
                .ok_or(DiagnosticResult::Compiler(
                    ParseCompilerFailure::InvalidGrammarData,
                ))?;
            best_arm_internal &= atom.is_inside_arm();
        }
    }
    Ok(Frontier {
        maximum,
        expected: expected.finish(),
        best_arm,
        best_arm_internal,
        transparent_name,
        transparent_disagreement,
        atom_only,
        matched_atom_prefix,
        identifier_continues,
    })
}

fn override_issue(
    decision: Decision,
    frontier: &Frontier,
    site: DiagnosticSite<'_>,
    context: ProbeContext,
    work: &mut Work,
) -> Result<Option<SyntaxIssue>, DiagnosticResult> {
    let boundary = site
        .cursor
        .checked_add(usize::from(frontier.maximum))
        .ok_or(DiagnosticResult::Compiler(
            ParseCompilerFailure::CounterOverflow,
        ))?;
    if let Some(issue) =
        dotted_override(site.source, site.tokens, boundary, frontier.expected, work)
            .map_err(DiagnosticResult::Resource)?
    {
        return Ok(Some(issue));
    }
    // A matched delimiter can put the would-next-enter atom at position m,
    // for example `( std` or `, spawn` in an argument-list continuation.
    // This is a new atom only when no maximal-prefix row has matched an atom's
    // terminal: `value spawn`, `& spawn` and `move spawn` have already begun
    // the atom. Existing starts retain their original cursor in that case.
    let atom_cursor = if frontier.atom_only && !frontier.matched_atom_prefix {
        boundary
    } else {
        site.cursor
    };
    if let Some(issue) = forbidden_atom_override(
        site.source,
        site.tokens,
        atom_cursor,
        ProbeContext {
            atom_only: context.atom_only || frontier.atom_only,
            ..context
        },
        frontier.expected,
        work,
    )
    .map_err(DiagnosticResult::Resource)?
    {
        return Ok(Some(issue));
    }
    if let Some(issue) = qualified_value_override(context, site.cursor) {
        return Ok(Some(issue));
    }
    if let Some(token) = site.tokens.get(boundary) {
        if let Some(rule) = raw_restriction_owner(token, frontier.expected) {
            let coordinate = boundary_coordinate(
                site.source,
                site.source_len,
                site.tokens,
                site.cursor,
                usize::from(frontier.maximum),
            )
            .map_err(DiagnosticResult::Compiler)?;
            return Ok(Some(SyntaxIssue {
                rule,
                coordinate,
                expected: frontier.expected,
                mechanical_fix: reserved_name_repair(
                    rule,
                    frontier.identifier_continues,
                    coordinate,
                ),
            }));
        }
        if let Some((rule, admitted)) = name_slot_owner(
            token,
            frontier.transparent_name,
            !frontier.transparent_disagreement,
        ) {
            return Ok(Some(SyntaxIssue {
                rule,
                coordinate: boundary_coordinate(
                    site.source,
                    site.source_len,
                    site.tokens,
                    site.cursor,
                    usize::from(frontier.maximum),
                )
                .map_err(DiagnosticResult::Compiler)?,
                expected: frontier.expected,
                mechanical_fix: name_class_fix(admitted).map(SyntaxRepair::Fixed),
            }));
        }
    }
    if let Some(issue) = construct_override(
        decision.context(),
        site.source,
        site.tokens,
        site.cursor,
        frontier.expected,
    ) {
        return Ok(Some(issue));
    }
    Ok(program_leftover(
        decision.context(),
        site.source,
        site.tokens,
        site.cursor,
        frontier.maximum,
    ))
}

fn push_probe(
    tasks: &mut Vec<ProbeTask>,
    task: ProbeTask,
    limits: ParseLimits,
) -> Result<(), ParseResourceFailure> {
    let actual = u64::try_from(tasks.len())
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(ParseResourceFailure::AddressSpaceExceeded {
            storage: ParseStorage::Tasks,
            requested: u64::MAX,
        })?;
    if actual > limits.max_tasks {
        return Err(ParseResourceFailure::LimitExceeded {
            limit: ParseLimit::Tasks,
            maximum: limits.max_tasks,
            actual,
        });
    }
    tasks
        .try_reserve(1)
        .map_err(|_| ParseResourceFailure::StorageUnavailable {
            storage: ParseStorage::Tasks,
            requested: actual,
        })?;
    tasks.push(task);
    Ok(())
}

fn arm_node(decision: Decision, arm: u8) -> Result<Option<GrammarNodeId>, ParseCompilerFailure> {
    let node = grammar_node(decision.node()).ok_or(ParseCompilerFailure::MissingGrammarNode)?;
    match decision.kind() {
        DecisionKind::Choice => node
            .children()
            .get(usize::from(arm))
            .copied()
            .map(Some)
            .ok_or(ParseCompilerFailure::InvalidGrammarData),
        DecisionKind::Optional | DecisionKind::Repeat0 | DecisionKind::Repeat1 => match arm {
            0 => node
                .children()
                .first()
                .copied()
                .map(Some)
                .ok_or(ParseCompilerFailure::InvalidGrammarData),
            1 => Ok(None),
            _ => Err(ParseCompilerFailure::InvalidGrammarData),
        },
    }
}

fn descend_or_issue(
    decision: Decision,
    context: ProbeContext,
    site: DiagnosticSite<'_>,
    work: &mut Work,
    tasks: &mut Vec<ProbeTask>,
    continuation: impl Iterator<Item = Continuation> + Clone,
) -> Result<Option<SyntaxIssue>, DiagnosticResult> {
    let pending = tasks.iter().rev().filter_map(|task| task.continuation());
    let value = frontier(
        decision,
        site.tokens,
        site.cursor,
        pending.chain(continuation),
        work,
    )?;
    if let Some(issue) = override_issue(decision, &value, site, context, work)? {
        return Ok(Some(issue));
    }
    if value.best_arm_internal {
        let arm = value.best_arm.ok_or(DiagnosticResult::Compiler(
            ParseCompilerFailure::InvalidGrammarData,
        ))?;
        let node = arm_node(decision, arm).map_err(DiagnosticResult::Compiler)?;
        let Some(node) = node else {
            return Ok(Some(SyntaxIssue {
                rule: SyntaxRule::from(decision.production().owner()),
                coordinate: boundary_coordinate(
                    site.source,
                    site.source_len,
                    site.tokens,
                    site.cursor,
                    usize::from(value.maximum),
                )
                .map_err(DiagnosticResult::Compiler)?,
                expected: value.expected,
                mechanical_fix: production_fix(decision.production()).map(SyntaxRepair::Fixed),
            }));
        };
        // Retain the enclosing arm's pending work as diagnostic continuation,
        // but never execute past the selected arm after its failed frontier.
        if matches!(decision.kind(), DecisionKind::Repeat0 | DecisionKind::Repeat1) {
            push_probe(
                tasks,
                ProbeTask::Continue(decision.node(), context),
                site.limits,
            )
            .map_err(DiagnosticResult::Resource)?;
        }
        push_probe(tasks, ProbeTask::Boundary, site.limits)
            .map_err(DiagnosticResult::Resource)?;
        push_probe(tasks, ProbeTask::Execute(node, context), site.limits)
            .map_err(DiagnosticResult::Resource)?;
        return Ok(None);
    }
    Ok(Some(SyntaxIssue {
        rule: SyntaxRule::from(decision.production().owner()),
        coordinate: boundary_coordinate(
            site.source,
            site.source_len,
            site.tokens,
            site.cursor,
            usize::from(value.maximum),
        )
        .map_err(DiagnosticResult::Compiler)?,
        expected: value.expected,
        mechanical_fix: production_fix(decision.production()).map(SyntaxRepair::Fixed),
    }))
}

fn direct_mismatch_inner(
    expected_terminal: TerminalPredicate,
    context: ProbeContext,
    site: DiagnosticSite<'_>,
    work: &mut Work,
) -> DiagnosticResult {
    let mut builder = ExpectedBuilder::empty();
    builder.insert(LookaheadPredicate::Terminal(expected_terminal));
    let expected = builder.finish();
    match dotted_override(site.source, site.tokens, site.cursor, expected, work) {
        Ok(Some(issue)) => return DiagnosticResult::Issue(issue),
        Ok(None) => {}
        Err(failure) => return DiagnosticResult::Resource(failure),
    }
    match forbidden_atom_override(
        site.source,
        site.tokens,
        site.cursor,
        context,
        expected,
        work,
    ) {
        Ok(Some(issue)) => return DiagnosticResult::Issue(issue),
        Ok(None) => {}
        Err(failure) => return DiagnosticResult::Resource(failure),
    }
    if let Some(issue) = qualified_value_override(context, site.cursor) {
        return DiagnosticResult::Issue(issue);
    }
    if let Some(token) = site.tokens.get(site.cursor) {
        if let Some(rule) = raw_restriction_owner(token, expected) {
            let coordinate = SyntaxCoordinate::new(
                site.source,
                token.token().id().start(),
                token.token().id().end(),
            );
            // A direct mismatch has one terminal to match: an IDENT written
            // here matches it whatever follows.
            return DiagnosticResult::Issue(SyntaxIssue {
                rule,
                coordinate,
                expected,
                mechanical_fix: reserved_name_repair(rule, true, coordinate),
            });
        }
        let transparent = [
            NamePredicate::Identifier,
            NamePredicate::TypeIdentifier,
            NamePredicate::Label,
            NamePredicate::OperationName,
        ]
        .into_iter()
        .find(|name| name.terminal() == expected_terminal);
        if let Some((rule, admitted)) = name_slot_owner(token, transparent, true) {
            return DiagnosticResult::Issue(SyntaxIssue {
                rule,
                coordinate: SyntaxCoordinate::new(
                    site.source,
                    token.token().id().start(),
                    token.token().id().end(),
                ),
                expected,
                mechanical_fix: name_class_fix(admitted).map(SyntaxRepair::Fixed),
            });
        }
    }
    match boundary_coordinate(site.source, site.source_len, site.tokens, site.cursor, 0) {
        Ok(coordinate) => DiagnosticResult::Issue(SyntaxIssue {
            rule: SyntaxRule::from(context.production.owner()),
            coordinate,
            expected,
            mechanical_fix: production_fix(context.production).map(SyntaxRepair::Fixed),
        }),
        Err(failure) => DiagnosticResult::Compiler(failure),
    }
}

fn probe(
    initial: GrammarNodeId,
    context: ProbeContext,
    site: DiagnosticSite<'_>,
    work: &mut Work,
    continuation: impl Iterator<Item = Continuation> + Clone,
) -> DiagnosticResult {
    let mut cursor = site.cursor;
    let mut tasks = Vec::new();
    if let Err(failure) = push_probe(
        &mut tasks,
        ProbeTask::Execute(initial, context),
        site.limits,
    ) {
        return DiagnosticResult::Resource(failure);
    }
    while let Some(task) = tasks.pop() {
        if let Err(failure) = work.spend(1) {
            return DiagnosticResult::Resource(failure);
        }
        match task {
            ProbeTask::Boundary => {
                return DiagnosticResult::Compiler(
                    ParseCompilerFailure::DiagnosticReachedSuccessfulEnd,
                );
            }
            ProbeTask::Match(expected, task_context) => {
                let matches = site
                    .tokens
                    .get(cursor)
                    .is_some_and(|token| token.terminals().contains(expected));
                if !matches {
                    return direct_mismatch(
                        expected,
                        task_context,
                        DiagnosticSite { cursor, ..site },
                        work,
                    );
                }
                let Some(next) = cursor.checked_add(1) else {
                    return DiagnosticResult::Compiler(ParseCompilerFailure::CounterOverflow);
                };
                cursor = next;
            }
            ProbeTask::Execute(node_id, task_context) => {
                let Some(node) = grammar_node(node_id) else {
                    return DiagnosticResult::Compiler(ParseCompilerFailure::MissingGrammarNode);
                };
                match node.kind() {
                    GrammarNodeKind::Production(production) => {
                        let nested = ProbeContext {
                            production,
                            atom_only: node.is_atom_only_reference(),
                            in_contract: task_context.in_contract
                                || production == Production::ContractBlock,
                            in_contract_define: task_context.in_contract_define
                                || production == Production::ContractDefine,
                            in_range: task_context.in_range
                                || production == Production::RangeClause,
                            infix_operand: match production {
                                Production::InfixTail => true,
                                Production::Psuffix
                                | Production::AtomList
                                | Production::Fieldinit
                                | Production::ForBinding => false,
                                _ => task_context.infix_operand,
                            },
                            completed_infix_operand: task_context.completed_infix_operand,
                            after_value_match: task_context.after_value_match,
                            in_expression: task_context.in_expression
                                || production == Production::Expr,
                            qualified_value: None,
                            match_binder: match production {
                                Production::Arm => true,
                                Production::ResultRoute | Production::LetStmt => false,
                                _ => task_context.match_binder,
                            },
                        };
                        if let Err(failure) = push_probe(
                            &mut tasks,
                            ProbeTask::Execute(production.root(), nested),
                            site.limits,
                        ) {
                            return DiagnosticResult::Resource(failure);
                        }
                    }
                    GrammarNodeKind::TerminalSequence => {
                        for terminal in node.terminals().iter().rev() {
                            let LookaheadPredicate::Terminal(predicate) = terminal else {
                                return DiagnosticResult::Compiler(
                                    ParseCompilerFailure::InvalidGrammarData,
                                );
                            };
                            if let Err(failure) = push_probe(
                                &mut tasks,
                                ProbeTask::Match(*predicate, task_context),
                                site.limits,
                            ) {
                                return DiagnosticResult::Resource(failure);
                            }
                        }
                    }
                    GrammarNodeKind::Sequence => {
                        for child in node.children().iter().rev() {
                            if let Err(failure) = push_probe(
                                &mut tasks,
                                ProbeTask::Execute(*child, task_context),
                                site.limits,
                            ) {
                                return DiagnosticResult::Resource(failure);
                            }
                        }
                    }
                    GrammarNodeKind::Group => {
                        let Some(child) = node.children().first() else {
                            return DiagnosticResult::Compiler(
                                ParseCompilerFailure::InvalidGrammarData,
                            );
                        };
                        if let Err(failure) = push_probe(
                            &mut tasks,
                            ProbeTask::Execute(*child, task_context),
                            site.limits,
                        ) {
                            return DiagnosticResult::Resource(failure);
                        }
                    }
                    GrammarNodeKind::RepeatOne => {
                        let Some(child) = node.children().first() else {
                            return DiagnosticResult::Compiler(
                                ParseCompilerFailure::InvalidGrammarData,
                            );
                        };
                        for next in [
                            ProbeTask::Continue(node_id, task_context),
                            ProbeTask::Execute(*child, task_context),
                        ] {
                            if let Err(failure) = push_probe(&mut tasks, next, site.limits) {
                                return DiagnosticResult::Resource(failure);
                            }
                        }
                    }
                    GrammarNodeKind::Choice
                    | GrammarNodeKind::Optional
                    | GrammarNodeKind::RepeatZero => {
                        let Some(decision) = node.decision().copied() else {
                            return DiagnosticResult::Compiler(
                                ParseCompilerFailure::InvalidGrammarData,
                            );
                        };
                        match select_arm(decision, site.tokens, cursor, work) {
                            Ok(DecisionSelection::Arm(arm)) => {
                                let selected = match arm_node(decision, arm) {
                                    Ok(selected) => selected,
                                    Err(failure) => return DiagnosticResult::Compiler(failure),
                                };
                                if let Some(selected) = selected {
                                    if decision.kind() == DecisionKind::Repeat0
                                        && let Err(failure) = push_probe(
                                            &mut tasks,
                                            ProbeTask::Continue(node_id, task_context),
                                            site.limits,
                                        )
                                    {
                                        return DiagnosticResult::Resource(failure);
                                    }
                                    if let Err(failure) = push_probe(
                                        &mut tasks,
                                        ProbeTask::Execute(selected, task_context),
                                        site.limits,
                                    ) {
                                        return DiagnosticResult::Resource(failure);
                                    }
                                }
                            }
                            Ok(DecisionSelection::NoMatch) => match descend_or_issue(
                                decision,
                                task_context,
                                DiagnosticSite { cursor, ..site },
                                work,
                                &mut tasks,
                                continuation.clone(),
                            ) {
                                Ok(Some(issue)) => return DiagnosticResult::Issue(issue),
                                Ok(None) => {}
                                Err(result) => return result,
                            },
                            Ok(DecisionSelection::Conflict) => {
                                return DiagnosticResult::Compiler(
                                    ParseCompilerFailure::PredictiveConflict,
                                );
                            }
                            Err(result) => return result,
                        }
                    }
                }
            }
            ProbeTask::Continue(node_id, task_context) => {
                let Some(node) = grammar_node(node_id) else {
                    return DiagnosticResult::Compiler(ParseCompilerFailure::MissingGrammarNode);
                };
                let Some(decision) = node.decision().copied() else {
                    return DiagnosticResult::Compiler(ParseCompilerFailure::InvalidGrammarData);
                };
                match select_arm(decision, site.tokens, cursor, work) {
                    Ok(DecisionSelection::Arm(0)) => {
                        let Some(child) = node.children().first() else {
                            return DiagnosticResult::Compiler(
                                ParseCompilerFailure::InvalidGrammarData,
                            );
                        };
                        for next in [
                            ProbeTask::Continue(node_id, task_context),
                            ProbeTask::Execute(*child, task_context),
                        ] {
                            if let Err(failure) = push_probe(&mut tasks, next, site.limits) {
                                return DiagnosticResult::Resource(failure);
                            }
                        }
                    }
                    Ok(DecisionSelection::Arm(1)) => {}
                    Ok(DecisionSelection::Arm(_)) => {
                        return DiagnosticResult::Compiler(
                            ParseCompilerFailure::InvalidGrammarData,
                        );
                    }
                    Ok(DecisionSelection::NoMatch) => match descend_or_issue(
                        decision,
                        task_context,
                        DiagnosticSite { cursor, ..site },
                        work,
                        &mut tasks,
                        continuation.clone(),
                    ) {
                        Ok(Some(issue)) => return DiagnosticResult::Issue(issue),
                        Ok(None) => {}
                        Err(result) => return result,
                    },
                    Ok(DecisionSelection::Conflict) => {
                        return DiagnosticResult::Compiler(
                            ParseCompilerFailure::PredictiveConflict,
                        );
                    }
                    Err(result) => return result,
                }
            }
        }
    }
    DiagnosticResult::Compiler(ParseCompilerFailure::DiagnosticReachedSuccessfulEnd)
}

fn diagnose_decision_inner(
    decision: Decision,
    context: ProbeContext,
    site: DiagnosticSite<'_>,
    work: &mut Work,
    continuation: impl Iterator<Item = Continuation> + Clone,
) -> DiagnosticResult {
    let value = match frontier(decision, site.tokens, site.cursor, continuation.clone(), work) {
        Ok(value) => value,
        Err(result) => return result,
    };
    match override_issue(decision, &value, site, context, work) {
        Ok(Some(issue)) => return DiagnosticResult::Issue(issue),
        Ok(None) => {}
        Err(result) => return result,
    }
    if value.best_arm_internal {
        let Some(arm) = value.best_arm else {
            return DiagnosticResult::Compiler(ParseCompilerFailure::InvalidGrammarData);
        };
        let initial = match arm_node(decision, arm) {
            Ok(Some(node)) => node,
            Ok(None) => {
                return DiagnosticResult::Compiler(
                    ParseCompilerFailure::DiagnosticReachedSuccessfulEnd,
                );
            }
            Err(failure) => return DiagnosticResult::Compiler(failure),
        };
        let repeated = (arm == 0
            && matches!(decision.kind(), DecisionKind::Repeat0 | DecisionKind::Repeat1))
            .then_some(Continuation::Repeat(decision.node()));
        return probe(
            initial,
            context,
            site,
            work,
            repeated.into_iter().chain(continuation),
        );
    }
    let coordinate = match boundary_coordinate(
        site.source,
        site.source_len,
        site.tokens,
        site.cursor,
        usize::from(value.maximum),
    ) {
        Ok(coordinate) => coordinate,
        Err(failure) => return DiagnosticResult::Compiler(failure),
    };
    DiagnosticResult::Issue(SyntaxIssue {
        rule: SyntaxRule::from(decision.production().owner()),
        coordinate,
        expected: value.expected,
        mechanical_fix: production_fix(decision.production()).map(SyntaxRepair::Fixed),
    })
}

pub(crate) fn direct_mismatch(
    expected_terminal: TerminalPredicate,
    context: ProbeContext,
    site: DiagnosticSite<'_>,
    work: &mut Work,
) -> DiagnosticResult {
    let result = direct_mismatch_inner(expected_terminal, context, site, work);
    repairs::attach(result, context, site)
}

pub(crate) fn diagnose_decision(
    decision: Decision,
    context: ProbeContext,
    site: DiagnosticSite<'_>,
    work: &mut Work,
    continuation: impl Iterator<Item = Continuation> + Clone,
) -> DiagnosticResult {
    let result = diagnose_decision_inner(decision, context, site, work, continuation);
    repairs::attach(result, context, site)
}
