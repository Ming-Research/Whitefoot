//! Repairs selected after grammar attribution, without changing its result.

use super::{DiagnosticResult, DiagnosticSite, ProbeContext, fixed, has};
use crate::ClassifiedToken;
use crate::diagnostic_repairs::{
    ALIAS_FIX, ARRAY_FIX, BOOLEAN_OPERATION_FIX, BORROW_MUT_FIX, EFFECT_ROW_FIX, ENTRY_FIX,
    FLAT_OPERATION_FIX, GRAPH_FIX, INVARIANT_FIX, LET_MUT_FIX, LET_TYPE_FIX, LOOP_FIX,
    MATCH_BINDER_FIX, VALUE_MATCH_FIX,
};
use crate::syntax::grammar::{LookaheadPredicate, Production};
use crate::syntax::terminal::{FixedTerminal as F, TerminalPredicate as T};

pub(super) fn is_infix_operator(token: &ClassifiedToken) -> bool {
    [
        F::Plus,
        F::PlusWrap,
        F::PlusDefined,
        F::PlusChecked,
        F::PlusSat,
        F::Minus,
        F::MinusWrap,
        F::MinusDefined,
        F::MinusChecked,
        F::MinusSat,
        F::Star,
        F::StarWrap,
        F::StarDefined,
        F::StarChecked,
        F::StarSat,
        F::Slash,
        F::SlashDefined,
        F::SlashChecked,
        F::Percent,
        F::PercentDefined,
        F::PercentChecked,
        F::EqualEqual,
        F::BangEqual,
        F::LeftAngle,
        F::LessEqual,
        F::RightAngle,
        F::GreaterEqual,
    ]
    .into_iter()
    .any(|terminal| fixed(token, terminal))
}

pub(super) fn attach(
    mut result: DiagnosticResult,
    context: ProbeContext,
    site: DiagnosticSite<'_>,
) -> DiagnosticResult {
    if let DiagnosticResult::Issue(issue) = &mut result {
        let boundary = site
            .tokens
            .iter()
            .position(|token| token.token().id().start() == issue.coordinate.start())
            .unwrap_or(site.tokens.len());
        let at =
            |index: usize, terminal| site.tokens.get(index).is_some_and(|t| fixed(t, terminal));
        let previous = |distance: usize, terminal| {
            boundary
                .checked_sub(distance)
                .is_some_and(|i| at(i, terminal))
        };
        let expects = |terminal| {
            issue
                .expected
                .iter()
                .any(|p| p == LookaheadPredicate::Terminal(terminal))
        };
        let word = |index: usize, text: &[u8]| {
            let Some(token) = site.tokens.get(index) else {
                return false;
            };
            let id = token.token().id();
            let Some(file) = site.bundle.file(site.source) else {
                return false;
            };
            let Ok(start) = usize::try_from(id.start().value()) else {
                return false;
            };
            let Ok(end) = usize::try_from(id.end().value()) else {
                return false;
            };
            file.bytes().get(start..end) == Some(text)
        };
        let prior_word = |distance: usize, text: &[u8]| {
            boundary
                .checked_sub(distance)
                .is_some_and(|i| word(i, text))
        };
        let graph = matches!(
            context.production,
            Production::GraphFile
                | Production::GraphSegment
                | Production::ModuleRow
                | Production::ModulePath
                | Production::PackageDecl
                | Production::EntryDecl
        );
        let in_expression = expects(T::Literal)
            || matches!(
                context.production,
                Production::Expr
                    | Production::Atom
                    | Production::Place
                    | Production::Pbase
                    | Production::Psuffix
                    | Production::InfixTail
                    | Production::OrdinaryLetRhs
                    | Production::PropagateLetRhs
                    | Production::Fieldinit
                    | Production::AtomList
                    | Production::ReturnStmt
                    | Production::GiveStmt
                    | Production::SetStmt
            );
        let hint = if graph {
            Some(GRAPH_FIX)
        } else if expects(T::Fixed(F::Pure))
            && (at(boundary, F::LeftBrace) || at(boundary, F::Waits))
        {
            Some(EFFECT_ROW_FIX)
        } else if (context.after_value_match || context.production == Production::ValueMatch)
            && at(boundary, F::Semicolon)
            && previous(1, F::RightBrace)
        {
            Some(VALUE_MATCH_FIX)
        } else if (at(boundary, F::RightParen)
            && previous(1, F::LeftParen)
            && (previous(2, F::Loop)
                || boundary.checked_sub(2).is_some_and(|i| {
                    site.tokens.get(i).is_some_and(|t| has(t, T::Label))
                        && i.checked_sub(1).is_some_and(|j| at(j, F::Loop))
                })))
            || (at(boundary, F::LeftParen)
                && at(boundary + 1, F::RightParen)
                && previous(1, F::Loop))
        {
            Some(LOOP_FIX)
        } else if (previous(1, F::Invariant) || previous(2, F::Invariant))
            && matches!(
                context.production,
                Production::HeaderInvariant | Production::LoopStmt | Production::ForStmt
            )
        {
            Some(INVARIANT_FIX)
        } else if context.match_binder
            && matches!(
                context.production,
                Production::Fieldbind | Production::FieldbindList | Production::Arm
            )
            && (expects(T::Fixed(F::Colon)) || at(boundary, F::RightParen))
        {
            Some(MATCH_BINDER_FIX)
        } else if (word(boundary, b"mut") && previous(1, F::Let))
            || (prior_word(1, b"mut") && previous(2, F::Let))
        {
            Some(LET_MUT_FIX)
        } else if at(boundary, F::Colon) && previous(2, F::Let) {
            Some(LET_TYPE_FIX)
        } else if (word(boundary, b"mut") && previous(1, F::Ampersand))
            || (prior_word(1, b"mut") && previous(2, F::Ampersand))
        {
            Some(BORROW_MUT_FIX)
        } else if matches!(context.production, Production::Program | Production::Item)
            && at(boundary, F::Use)
        {
            Some(ALIAS_FIX)
        } else if matches!(context.production, Production::Program | Production::Item)
            && at(boundary, F::Entry)
            && at(boundary + 1, F::Fn)
        {
            Some(ENTRY_FIX)
        } else if at(boundary, F::Ampersand)
            && (at(boundary + 1, F::Ampersand) || previous(1, F::Ampersand))
        {
            Some(BOOLEAN_OPERATION_FIX)
        } else if !context.in_contract
            && at(boundary, F::LeftBracket)
            && (in_expression
                || (context.in_expression
                    && previous(1, F::RightAngle)
                    && expects(T::Fixed(F::LeftParen))))
        {
            Some(ARRAY_FIX)
        } else if !context.in_contract
            && ((at(boundary, F::LeftParen) && in_expression)
                || (in_expression
                    && site.tokens.get(boundary).is_some_and(is_infix_operator)
                    && !expects(T::Fixed(F::Colon))))
        {
            Some(FLAT_OPERATION_FIX)
        } else {
            None
        };
        issue.mechanical_fix = hint.or(issue.mechanical_fix);
    }
    result
}
