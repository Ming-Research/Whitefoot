use crate::lexer::TokenKind;
use crate::syntax::grammar::{
    Decision, DecisionContext, DecisionKind, GrammarNodeId, GrammarNodeKind, LookaheadPredicate,
    NamePredicate, Production, SelectRow, grammar_node,
};
use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};
use crate::{ByteOffset, SourceId};

use crate::ClassifiedToken;

use super::{
    ExpectedBuilder, ParseCompilerFailure, ParseLimit, ParseLimits, ParseResourceFailure,
    ParseStorage, SyntaxCoordinate, SyntaxIssue, SyntaxRule, Work,
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
    pub(crate) tokens: &'tokens [ClassifiedToken],
    pub(crate) cursor: usize,
    pub(crate) limits: ParseLimits,
}

#[derive(Clone, Copy)]
pub(crate) struct ProbeContext {
    pub(crate) production: Production,
    pub(crate) atom_only: bool,
    /// Whether an open production frame is the `contract_block` of GRAM-2.
    ///
    /// The repair GRAM-9 admits is the same restructuring in both positions —
    /// bind the inner call first and write the binder here — but the binding
    /// statement is not: a body has `let_stmt` and a `contract_block` has only
    /// `contract_define`. The grammar position decides which, so it is read
    /// from the open frames and never from the text of the offending line.
    pub(crate) in_contract: bool,
}

/// GRAM-9's repair in the two grammar positions that admit a binding.
const GRAM9_BODY_FIX: &str = "a `call` or `construct` in an atom position does not derive [GRAM-9]: bind the inner call with its own preceding `let` in this body and write that binder in the atom position — `let inner = f(x: 0_u64); let outer = g(y: inner);`";
const GRAM9_CONTRACT_FIX: &str = "a `call` or `construct` in an atom position does not derive [GRAM-9]: a `contract_block` has no `let`, so bind the inner call with a preceding `define` in this same block and write that binder in the atom position — `define inner = f(x: 0_u64); requires g(y: inner);`";

/// [FORM-3]'s lexical class for each name slot, with the repair a slot filled
/// from another class admits.
///
/// A name slot's class is a grammar position: `const_decl` writes IDENT,
/// `struct_decl` writes TYPEID, and no class is admitted in another's slot.
/// The expectation list names the class and never says what the class is, so
/// a writer who spelled a const `Limit` read only `expected: ["IDENT"]`.
const FORM3_IDENT_FIX: &str = "an IDENT slot admits only [FORM-3]'s IDENT `[a-z][a-z0-9_]*`, so a `const`, `fn`, parameter, `let`, field, or binder name is lowercase and is never a TYPEID `[A-Z][A-Za-z0-9]*`, a LABEL `@[a-z][a-z0-9_]*`, or an OPNAME; rename the name written here to the IDENT shape";
const FORM3_TYPEID_FIX: &str = "a TYPEID slot admits only [FORM-3]'s TYPEID `[A-Z][A-Za-z0-9]*`, so a struct, enum, contract, variant, or constructor name is capitalized and is never an IDENT `[a-z][a-z0-9_]*`, a LABEL `@[a-z][a-z0-9_]*`, or an OPNAME; rename the name written here to the TYPEID shape";
const FORM3_LABEL_FIX: &str = "a LABEL slot admits only [FORM-3]'s LABEL `@[a-z][a-z0-9_]*`, so write the leading `@`; an IDENT `[a-z][a-z0-9_]*`, a TYPEID `[A-Z][A-Za-z0-9]*`, and an OPNAME are other lexical classes and none is admitted here";

/// [GRAM-2] fixes the order of a `contract_block`: every `contract_define`,
/// then every `requires_clause`, then every `ensures_clause`. A clause written
/// out of that order leaves the frontier at a position whose expectation list
/// names only the sections still open, which states what is admitted next
/// without ever stating the order that was broken.
const GRAM2_CONTRACT_ORDER_FIX: &str = "a `contract_block` is written in one fixed order: all `define` definitions first, then all `requires` requirements, then all `ensures` postconditions. A clause of an earlier section written after a later one is not admitted, so move it above the first clause of the later section";

/// The repair the failing production admits, when the production itself fixes
/// one. The decision is the grammar position, never the text of the line.
const fn production_fix(production: Production) -> Option<&'static str> {
    match production {
        Production::ContractBlock => Some(GRAM2_CONTRACT_ORDER_FIX),
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
    for row in decision.rows() {
        work.spend(1).map_err(DiagnosticResult::Resource)?;
        if row_score(*row, tokens, cursor).map_err(DiagnosticResult::Compiler)? != 2 {
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

fn forbidden_atom_override(
    source: SourceId,
    tokens: &[ClassifiedToken],
    cursor: usize,
    atom_only: bool,
    in_contract: bool,
    expected: super::ExpectedTerminals,
) -> Option<SyntaxIssue> {
    if !atom_only {
        return None;
    }
    let first = tokens.get(cursor)?;
    let second = tokens.get(cursor.checked_add(1)?)?;
    // [DIAG-1] row 2: a `call` start is a name followed by `(` or by the
    // `::` type-application or qualified-path delimiter, or `pkg ::`; a
    // `construct` start is a TYPEID followed by `(`, `<` or the `::` of a
    // type-owned variant [TYPE-6]. `IDENT <` is a comparison, never a call.
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
    if call_start || construct_start {
        return Some(SyntaxIssue {
            rule: SyntaxRule::Gram9,
            coordinate: SyntaxCoordinate::new(
                source,
                first.token().id().start(),
                second.token().id().end(),
            ),
            expected,
            mechanical_fix: Some(if in_contract {
                GRAM9_CONTRACT_FIX
            } else {
                GRAM9_BODY_FIX
            }),
        });
    }
    None
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
}

fn frontier(
    decision: Decision,
    tokens: &[ClassifiedToken],
    cursor: usize,
    work: &mut Work,
) -> Result<Frontier, DiagnosticResult> {
    let arm_count = usize::from(decision.arm_count());
    if arm_count == 0 || arm_count > 64 {
        return Err(DiagnosticResult::Compiler(
            ParseCompilerFailure::InvalidGrammarData,
        ));
    }
    let mut arm_scores = [0_u8; 64];
    let mut maximum = 0_u8;
    for row in decision.rows() {
        work.spend(1).map_err(DiagnosticResult::Resource)?;
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
        arm_scores[arm] = arm_scores[arm].max(score);
        maximum = maximum.max(score);
    }
    let mut expected = ExpectedBuilder::empty();
    let mut transparent_name = None;
    let mut transparent_disagreement = false;
    let mut atom_only = false;
    for row in decision.rows() {
        work.spend(1).map_err(DiagnosticResult::Resource)?;
        if row_score(*row, tokens, cursor).map_err(DiagnosticResult::Compiler)? != maximum {
            continue;
        }
        let atom = row
            .position(usize::from(maximum))
            .ok_or(DiagnosticResult::Compiler(
                ParseCompilerFailure::InvalidGrammarData,
            ))?;
        expected.insert(atom.predicate());
        atom_only |= atom.is_atom_only();
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
        .filter(|score| **score == maximum)
        .count();
    let best_arm = if best_arms == 1 {
        arm_scores[..arm_count]
            .iter()
            .position(|score| *score == maximum)
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
    if let Some(issue) = forbidden_atom_override(
        site.source,
        site.tokens,
        site.cursor,
        context.atom_only || frontier.atom_only,
        context.in_contract,
        frontier.expected,
    ) {
        return Ok(Some(issue));
    }
    if let Some(token) = site.tokens.get(boundary) {
        if let Some(rule) = raw_restriction_owner(token, frontier.expected) {
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
                mechanical_fix: None,
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
                mechanical_fix: name_class_fix(admitted),
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
) -> Result<Option<SyntaxIssue>, DiagnosticResult> {
    let value = frontier(decision, site.tokens, site.cursor, work)?;
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
                mechanical_fix: production_fix(decision.production()),
            }));
        };
        tasks.clear();
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
        mechanical_fix: production_fix(decision.production()),
    }))
}

pub(crate) fn direct_mismatch(
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
    if let Some(issue) = forbidden_atom_override(
        site.source,
        site.tokens,
        site.cursor,
        context.atom_only,
        context.in_contract,
        expected,
    ) {
        return DiagnosticResult::Issue(issue);
    }
    if let Some(token) = site.tokens.get(site.cursor) {
        if let Some(rule) = raw_restriction_owner(token, expected) {
            return DiagnosticResult::Issue(SyntaxIssue {
                rule,
                coordinate: SyntaxCoordinate::new(
                    site.source,
                    token.token().id().start(),
                    token.token().id().end(),
                ),
                expected,
                mechanical_fix: None,
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
                mechanical_fix: name_class_fix(admitted),
            });
        }
    }
    match boundary_coordinate(site.source, site.source_len, site.tokens, site.cursor, 0) {
        Ok(coordinate) => DiagnosticResult::Issue(SyntaxIssue {
            rule: SyntaxRule::from(context.production.owner()),
            coordinate,
            expected,
            mechanical_fix: production_fix(context.production),
        }),
        Err(failure) => DiagnosticResult::Compiler(failure),
    }
}

fn probe(
    initial: GrammarNodeId,
    context: ProbeContext,
    site: DiagnosticSite<'_>,
    work: &mut Work,
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

pub(crate) fn diagnose_decision(
    decision: Decision,
    context: ProbeContext,
    site: DiagnosticSite<'_>,
    work: &mut Work,
) -> DiagnosticResult {
    let value = match frontier(decision, site.tokens, site.cursor, work) {
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
        return probe(initial, context, site, work);
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
        mechanical_fix: production_fix(decision.production()),
    })
}
