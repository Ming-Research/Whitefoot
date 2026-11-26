//! [RANGE-2, RANGE-3, RANGE-5] the range judgment.
//!
//! Range facts are judged over the completed checked program, after every
//! function's ordinary entailment. Every deferrable record left open is judged
//! at its source site; unrelated ordinary debt never disables the walk. The walk proves each deferred record at
//! its own program point before assuming a local invariant's target. One forward walk per function carries the facts
//! a function's requirements, its loops' invariants and its callees'
//! postconditions establish, discharges every range fact owed at a call, a
//! loop's entry, a back edge or an exit, and checks each counted loop's
//! cross-iteration certificate. A
//! certificate that holds is retained for the counted permission judgment
//! [PAR-2]; it grants nothing by itself.

mod constants;
pub(crate) mod e1;
mod facts;
pub(crate) use constants::judge as judge_constant_invariant;
mod selection;
mod solver;
mod walk;
mod world;

use crate::NodePath;

use super::model::{CheckedFunction, CheckedNominal};

/// One range judgment failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RangeIssue {
    /// A range fact owed at `node` is not established [RANGE-3].
    Undischarged {
        node: NodePath,
        fact: String,
        site: &'static str,
        /// The conclusion the derivation left open, when the fact was written.
        relation: Option<NodePath>,
        /// The structural ceiling the derivation reached, if it reached one.
        capacity: Option<&'static str>,
    },
    /// No exit of an inhabited instance selects the range postcondition
    /// written at `node` [RANGE-3].
    NoSelectedExit { node: NodePath },
    /// A counted loop's certificate does not separate two iterations
    /// [RANGE-5].
    Apart {
        node: NodePath,
        failure: ApartFailure,
    },
    /// The judgment at `node` needs what this checker does not implement,
    /// so the function's verdict is an unsupported capability, never a
    /// rejection: the derivation left the checker's `i128` arithmetic,
    /// where the specified arithmetic is exact [RANGE-3], or a loop nest was
    /// deeper, or a header's written set took more walks to settle, than the
    /// checker follows, where RANGE-2 forgets only what the body can write.
    Unsupported {
        node: NodePath,
        feature: super::UnsupportedSemanticFeature,
    },
}

/// Why a certificate does not hold.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ApartFailure {
    /// A write of one iteration and an access of another are not separated.
    Overlap {
        write: NodePath,
        other: NodePath,
        other_write: bool,
    },
    /// An access reaches storage another iteration writes at no one element.
    Unplaced { access: NodePath },
    /// A `use` step does not form an instance.
    Use {
        step: NodePath,
        reason: &'static str,
    },
    /// One pair's derivation reached a structural ceiling.
    Capacity {
        write: NodePath,
        other: NodePath,
        ceiling: &'static str,
    },
}

pub(crate) use super::range_facts::CheckedCertifiedLoop as CertifiedLoop;

/// The judgment of one function.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RangeJudgment {
    pub(crate) issues: Vec<RangeIssue>,
    pub(crate) certified: Vec<CertifiedLoop>,
    pub(crate) discharged: Vec<usize>,
    /// Records whose unrepresentable site or structural ceiling is reported by
    /// `issues`; they cannot supply an ordinary unproved-source diagnostic.
    pub(crate) inconclusive: Vec<usize>,
}

/// Range clauses are formed in both scopes [RANGE-1]. Deferred ordinary
/// obligations are judged in both; range-clause obligations wait for the
/// concrete instance, where a noninteger substitution can omit the clause.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum JudgmentScope {
    Symbolic,
    Concrete,
}

/// Whether a function takes part: it states a range clause, or it calls a
/// function with range requirements or postconditions.
pub(crate) fn takes_part(
    function: &CheckedFunction,
    boundary: impl Fn(super::model::FunctionId) -> bool,
) -> bool {
    if !function.range_facts.is_empty() {
        return true;
    }
    let mut calls = false;
    for_each_call(
        function.body.as_deref().unwrap_or_default(),
        &mut |callee, _| {
            calls |= boundary(callee);
        },
    );
    calls
}

/// Visits every ordinary call in `statements` with its callee and its call
/// node.
pub(crate) fn for_each_call(
    statements: &[super::model::CheckedStatement],
    visit: &mut dyn FnMut(super::model::FunctionId, &crate::NodePath),
) {
    use super::model::{CheckedStatement, expression_children};
    fn expression(
        value: &super::model::CheckedExpression,
        visit: &mut dyn FnMut(super::model::FunctionId, &crate::NodePath),
    ) {
        if let super::model::CheckedExpression::UserCall { function, call, .. } = value {
            visit(*function, call);
        }
        for child in expression_children(value) {
            expression(child, visit);
        }
    }
    for statement in statements {
        match statement {
            CheckedStatement::Let { value, .. }
            | CheckedStatement::DestructuringLet { value, .. }
            | CheckedStatement::Set { value, .. }
            | CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. }
            | CheckedStatement::Return { value, .. }
            | CheckedStatement::Give { value, .. } => expression(value, visit),
            CheckedStatement::PropagateLet { scrutinee, .. } => expression(scrutinee, visit),
            CheckedStatement::Match {
                scrutinee, arms, ..
            }
            | CheckedStatement::ValueMatchLet {
                scrutinee, arms, ..
            } => {
                expression(scrutinee, visit);
                for arm in arms {
                    for_each_call(&arm.body, visit);
                }
            }
            CheckedStatement::Loop { body, .. } => for_each_call(body, visit),
            CheckedStatement::CountedRange {
                lower, upper, body, ..
            } => {
                expression(lower, visit);
                expression(upper, visit);
                for_each_call(body, visit);
            }
            CheckedStatement::Atomic {
                targets,
                guard,
                body,
                ..
            } => {
                for target in targets
                    .iter()
                    .flat_map(crate::semantic::CheckedTarget::expressions)
                {
                    expression(target, visit);
                }
                if let Some(guard) = guard {
                    expression(guard, visit);
                }
                for_each_call(body, visit);
            }
            CheckedStatement::Proof(_)
            | CheckedStatement::Break { .. }
            | CheckedStatement::Continue { .. } => {}
        }
    }
}

/// Judges every selected function of a checked program, dense by function.
pub(crate) fn judge_program(
    functions: &[&CheckedFunction],
    nominals: &[CheckedNominal],
    elements: &[super::model::CheckedType],
    selected: &[bool],
    constants: &[super::model::CheckedConstant],
    scope: JudgmentScope,
    resolved: &crate::ResolvedSyntaxUnit,
) -> Vec<RangeJudgment> {
    let prelude = functions
        .iter()
        .filter_map(|function| {
            let declaration = resolved.declaration(function.declaration)?;
            let file = resolved
                .syntax()
                .classified_bundle()
                .source_bundle()
                .file(declaration.origin().coordinate().source())?;
            file.prelude().map(|_| function.declaration)
        })
        .collect::<std::collections::BTreeSet<_>>();
    functions
        .iter()
        .enumerate()
        .map(|(index, function)| {
            if !selected.get(index).copied().unwrap_or(false)
                || function.body.is_none()
                || !takes_part(function, |callee| {
                    functions
                        .get(callee.0 as usize)
                        .is_some_and(|callee| callee.range_facts.has_boundary())
                })
            {
                e1_shadow(
                    functions, nominals, elements, function, selected.get(index).copied(),
                    constants, scope, &prelude,
                );
                return RangeJudgment::default();
            }
            let e1_walk = std::time::Instant::now();
            let mut walker = walk::Walker::new(
                functions,
                nominals,
                elements,
                function,
                deferred_records(function, functions, nominals, elements),
                constants,
                scope,
                &prelude,
            );
            walker.run();
            // A walk that forgot more than RANGE-2 does cannot reject: what
            // it left unproved may hold.
            let unproved = walker
                .deferred
                .iter()
                .any(|(_, answer)| *answer != walk::DeferredAnswer::Proved);
            let imprecise = walker
                .imprecise
                .take()
                .map(|node| (node, super::UnsupportedSemanticFeature::RangeLoopNesting));
            let issues = match imprecise {
                Some((node, feature)) if !walker.issues.is_empty() || unproved => {
                    vec![RangeIssue::Unsupported { node, feature }]
                }
                _ => walker.issues,
            };
            e1::walk_time(e1_walk.elapsed().as_nanos());
            e1_shadow(
                functions, nominals, elements, function, selected.get(index).copied(),
                constants, scope, &prelude,
            );
            RangeJudgment {
                issues,
                certified: walker.certified,
                discharged: walker
                    .deferred
                    .iter()
                    .filter_map(|(index, answer)| {
                        (*answer == walk::DeferredAnswer::Proved).then_some(*index)
                    })
                    .collect(),
                inconclusive: walker
                    .deferred
                    .iter()
                    .filter_map(|(index, answer)| {
                        (*answer == walk::DeferredAnswer::Inconclusive).then_some(*index)
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Collect every deferrable record; unrelated debt never disables the walk.
fn deferred_records(
    function: &CheckedFunction,
    functions: &[&CheckedFunction],
    nominals: &[CheckedNominal],
    elements: &[super::model::CheckedType],
) -> Vec<(usize, walk::DeferredAnswer)> {
    use super::entailment::ObligationFamily;
    use super::obligations::ObligationSubject;
    let mut pending = Vec::new();
    for (index, record) in function.obligations.iter().enumerate() {
        let Some(answer) = function.entailment.answers.get(index).copied().flatten() else {
            continue;
        };
        if answer.discharged(&function.entailment) {
            continue;
        }
        match record.subject {
            ObligationSubject::Source {
                family:
                    ObligationFamily::Bounds
                    | ObligationFamily::IntegerDomain
                    | ObligationFamily::ConversionDomain,
                ..
            }
            | ObligationSubject::LoopInvariant { .. } => {
                pending.push((index, walk::DeferredAnswer::Pending))
            }
            ObligationSubject::SourceProof => {
                let super::obligations::RecordAnswer::SourceProof(proof) = answer else {
                    continue;
                };
                let Some(proof) = function.entailment.source_proofs.get(proof) else {
                    continue;
                };
                let check = &proof.check;
                // A failed target can be deferred; an invalid ordinary certificate cannot.
                if check.redundant
                    || check.source_failure.is_some()
                    || check.certificate_failure.is_some()
                    || check.residual_failure.is_some()
                    || check.first_unproved_premise.is_some()
                {
                    continue;
                }
                pending.push((index, walk::DeferredAnswer::Pending));
            }
            ObligationSubject::CallRequirement { .. }
                if record.rule != super::SemanticRule::Type11 =>
            {
                // TYPE-11 is judged as a state invariant. PRE-1 requirements
                // remain ordinary calls even when OP-14 owns their diagnostic.
                pending.push((index, walk::DeferredAnswer::Pending));
            }
            _ => {}
        }
    }
    if !pending.is_empty() {
        let selected = selection::selected_sites(function, functions, nominals, elements);
        pending.retain(|(index, _)| selected.selects(&function.obligations[*index]));
    }
    pending
}

/// E1: the shadow walks of one function, one per configuration, posing every
/// obligation record; their answers and counts go only to the counter file.
#[allow(clippy::too_many_arguments)]
fn e1_shadow<'p>(
    functions: &'p [&'p CheckedFunction],
    nominals: &'p [CheckedNominal],
    elements: &'p [super::model::CheckedType],
    function: &'p CheckedFunction,
    selected: Option<bool>,
    constants: &'p [super::model::CheckedConstant],
    scope: JudgmentScope,
    prelude: &'p std::collections::BTreeSet<crate::DeclarationId>,
) {
    use super::obligations::ObligationSubject;
    let configs = e1::configs();
    if configs.is_empty() || selected != Some(true) || function.body.is_none() {
        return;
    }
    let is_prelude = prelude.contains(&function.declaration);
    let scope_name = match scope {
        JudgmentScope::Symbolic => "symbolic",
        JudgmentScope::Concrete => "concrete",
    };
    for config in configs {
        let deferred = (0..function.obligations.len())
            .map(|index| (index, walk::DeferredAnswer::Pending))
            .collect();
        let ((answers, imprecise), counted, nanos) = e1::shadow(config, || {
            let mut walker = walk::Walker::new(
                functions,
                nominals,
                elements,
                function,
                deferred,
                constants,
                scope,
                prelude,
            );
            walker.run();
            (walker.deferred, walker.imprecise.is_some())
        });
        if !e1::data() {
            continue;
        }
        let mut by_record: std::collections::BTreeMap<usize, e1::Acc> =
            std::collections::BTreeMap::new();
        let mut conditions: std::collections::BTreeMap<
            usize,
            (Option<e1::Acc>, Option<e1::Acc>),
        > = std::collections::BTreeMap::new();
        let mut condition_answers: std::collections::BTreeMap<usize, &'static str> =
            std::collections::BTreeMap::new();
        let mut rows = Vec::new();
        for (key, acc) in &counted {
            match key {
                e1::Key::Records(indices) => {
                    for index in indices {
                        by_record.insert(*index, acc.clone());
                    }
                }
                e1::Key::Clause(site) => rows.push(e1::Row {
                    config,
                    scope: scope_name,
                    prelude: is_prelude,
                    function: &function.name,
                    record: "-".to_owned(),
                    rule: "RANGE-3".to_owned(),
                    kind: "range-clause".to_owned(),
                    site: site.clone(),
                    ordinary: "-",
                    route: "-".to_owned(),
                    shadow: e1_answer(acc, acc.queries > 0 && acc.refuted == acc.queries, false),
                    acc: Some(acc),
                    walk_nanos: nanos,
                }),
                e1::Key::Condition(index, truth) => {
                    let entry = conditions.entry(*index).or_default();
                    if *truth {
                        entry.0 = Some(acc.clone());
                    } else {
                        entry.1 = Some(acc.clone());
                    }
                }
            }
        }
        for (index, (when_true, when_false)) in &conditions {
            let decided = |acc: &Option<e1::Acc>| {
                acc.as_ref()
                    .is_some_and(|acc| acc.queries > 0 && acc.refuted == acc.queries)
            };
            let noted = |acc: &Option<e1::Acc>| acc.as_ref().is_some_and(|acc| !acc.notes.is_empty());
            let answer = match (decided(when_true), decided(when_false)) {
                (true, true) => "decided-both",
                (true, false) => "decided-true",
                (false, true) => "decided-false",
                (false, false) if noted(when_true) || noted(when_false) => "inconclusive",
                (false, false) => "undecided",
            };
            let default = e1::Acc::default();
            let merged = when_true
                .as_ref()
                .unwrap_or(&default)
                .merged(when_false.as_ref().unwrap_or(&default));
            by_record.insert(*index, merged);
            condition_answers.insert(*index, answer);
        }
        for (index, answer) in answers {
            let record = &function.obligations[index];
            let ordinary = match function.entailment.answers.get(index).copied().flatten() {
                Some(answer) if matches!(record.subject, ObligationSubject::Condition) => {
                    if answer.discharged(&function.entailment) {
                        "undecided"
                    } else {
                        "decided"
                    }
                }
                Some(answer) if answer.discharged(&function.entailment) => "proved",
                Some(_) => "unproved",
                None => "unanswered",
            };
            let acc = by_record.get(&index);
            let shadow = match (answer, acc) {
                _ if condition_answers.contains_key(&index) => {
                    condition_answers[&index].to_owned()
                }
                (walk::DeferredAnswer::Proved, Some(acc)) if acc.queries > 0 => "proved".to_owned(),
                (walk::DeferredAnswer::Proved, _) => "unreached".to_owned(),
                (walk::DeferredAnswer::Unproved, _) => "unproved".to_owned(),
                (walk::DeferredAnswer::Inconclusive, Some(acc)) => {
                    e1_answer(acc, false, imprecise)
                }
                (walk::DeferredAnswer::Inconclusive | walk::DeferredAnswer::Pending, None) => {
                    format!("not-posed:{}", e1_kind(&record.subject))
                }
                (walk::DeferredAnswer::Pending, Some(_)) => "pending".to_owned(),
            };
            rows.push(e1::Row {
                config,
                scope: scope_name,
                prelude: is_prelude,
                function: &function.name,
                record: index.to_string(),
                rule: format!("{:?}", record.rule),
                kind: e1_kind(&record.subject),
                site: format!("{:?}", record.site),
                ordinary,
                route: "-".to_owned(),
                shadow,
                acc,
                walk_nanos: nanos,
            });
        }
        e1::write(&rows);
    }
}

fn e1_answer(acc: &e1::Acc, proved: bool, imprecise: bool) -> String {
    if proved {
        return "proved".to_owned();
    }
    if acc.notes.is_empty() {
        if imprecise {
            return "inconclusive:imprecise".to_owned();
        }
        return if acc.queries == 0 {
            "not-posed".to_owned()
        } else {
            "unproved".to_owned()
        };
    }
    let notes: Vec<&str> = acc.notes.iter().copied().collect();
    format!("inconclusive:{}", notes.join(","))
}

fn e1_kind(subject: &super::obligations::ObligationSubject) -> String {
    use super::obligations::ObligationSubject;
    match subject {
        ObligationSubject::Condition => "condition".to_owned(),
        ObligationSubject::Source { family, conjunct } => format!("{family:?}#{conjunct}"),
        ObligationSubject::CallRequirement { subject, .. } => {
            if subject.is_some() {
                "call-requirement-type11".to_owned()
            } else {
                "call-requirement".to_owned()
            }
        }
        ObligationSubject::LoopInvariant { .. } => "loop-invariant".to_owned(),
        ObligationSubject::SourceProof => "source-proof".to_owned(),
        ObligationSubject::Postcondition { .. } => "postcondition".to_owned(),
    }
}
