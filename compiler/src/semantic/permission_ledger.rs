//! The non-normative permission ledger: one developer-channel line per
//! analyzed sibling-call pair, one per eligible chain, one per counted loop,
//! and one more for a refused loop a hand-written index split would reach.
//!
//! The ledger is the visible half of the permission judgment. It states, for
//! every pair the judgment looked at, whether overlap is permitted and — when
//! it is not — exactly which numbered condition refused it and at which source
//! text. That makes a sequentialization a reported fact rather than a silent
//! one, and gives a writer a gradient: a condition-2 line names the two places
//! to separate.
//!
//! Pairs alone do not say what will be handed out. A chain of three permitted
//! pairs and three separate permitted pairs read identically as pairs and are
//! completely different work, so every eligible chain gets a `run` line naming
//! its members. What the *backend* then keeps is narrower still — one call
//! definition per site, all members in one block, no addressed binding but the
//! last — and that narrowing happens after this ledger is rendered, so a `run`
//! line states what the judgment permits and not what the emitter actualizes.
//!
//! A `loop` line is the same statement for a counted loop, whose iterations
//! are judged by their own rule rather than as a pair. It names the condition
//! that refused the loop, or, for a permitted one, the operation its
//! accumulator recombines under — the fact that makes the overlap safe. See
//! [`super::loop_permission`] for what is admitted and why no float operation
//! ever is.
//!
//! A `hint` line survives underneath it for exactly one refusal: a loop this
//! version declines only because it carries several accumulators is still one
//! a hand-written recursion can split, and the line says so and names the
//! condition that refused the loop itself. Every other refusal is a reason the
//! rewrite would be refused too, and advice a writer cannot safely take is
//! worse than silence.
//!
//! Nothing here participates in acceptance, in lowering, or in any mandatory
//! [DIAG-3] record. It reads the finished permission table and the source
//! text, renders text, and returns it to the caller to print on the developer
//! channel.

use crate::NodePath;

use super::loop_permission::{LoopDenial, LoopPermission, LoopVerdict};
use super::permission::{
    Access, Denial, ExitKind, PairSide, PermissionMetadata, PermissionVerdict,
};

#[derive(Clone, Debug, Eq, PartialEq)]
/// One rendered line of the ordinary permission ledger.
pub(crate) struct LedgerLine {
    /// The rendered line, identical in both channels.
    pub(crate) text: String,
}

/// The source facts one ledger line needs, supplied by the checker because
/// only the checker still holds the syntax tree and the source bundle.
pub(crate) trait LedgerSource {
    /// Why a node path could not be resolved.
    type Error;

    /// The name a reader is shown for one node's source, and its one-based
    /// line. That is the host path the driver read the bytes from, not the
    /// bundle's own portable key, because every consumer of this pair prints
    /// it for a person to open.
    fn location(&self, path: &NodePath) -> Result<(String, u64), Self::Error>;

    /// The exact canonical source spelling of one node.
    fn spelling(&self, path: &NodePath) -> Result<String, Self::Error>;
}

/// Renders the whole permission table as ledger lines in source order.
///
/// The table is dense by `FunctionId`, so one source function that is
/// monomorphized more than once contributes its pairs more than once. Lines
/// that come out byte-identical at the same source site are
/// therefore collapsed: the ledger reports source sites, and two instances of
/// one generic that agree on the verdict are one reported site. Two instances
/// that disagree keep both lines.
pub(crate) fn render_ledger<Source: LedgerSource>(
    metadata: &PermissionMetadata,
    source: &Source,
) -> Result<Vec<LedgerLine>, Source::Error> {
    // A chain starts at its first member's statement, so it shares a location
    // with the pair that opens it. This ordinal sorts the pairs of one
    // location ahead of the chain they compose into, which is the order a
    // writer reads them in.
    const PAIR: u8 = 0;
    const CHAIN: u8 = 1;
    const LOOP: u8 = 2;
    const HINT: u8 = 3;
    let mut entries: Vec<Entry> = Vec::new();
    for permissions in &metadata.functions {
        for pair in &permissions.pairs {
            let (display_path, line) = source.location(&pair.first.statement)?;
            let verdict = match &pair.verdict {
                PermissionVerdict::PermittedEligible => "permitted",
                PermissionVerdict::Denied(_) => "denied",
            };
            let detail = detail(&pair.verdict, source)?;
            entries.push(Entry::new(
                display_path.clone(),
                line,
                PAIR,
                format!(
                    "PAR {verdict:<10}  {display_path}:{line}  pair({}, {})  {detail}",
                    pair.first.callee_name, pair.second.callee_name
                ),
            ));
        }
        for run in &permissions.runs {
            let (display_path, line) = source.location(&run.sites[0].statement)?;
            let last = run.sites.last().expect("a chain has at least two members");
            let (_, last_line) = source.location(&last.statement)?;
            let members = run
                .sites
                .iter()
                .map(|site| site.callee_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            entries.push(Entry::new(
                display_path.clone(),
                line,
                CHAIN,
                format!(
                    "PAR chain       {display_path}:{line}  run({members})  {} members through line {last_line}",
                    run.sites.len()
                ),
            ));
        }
        for judged in &permissions.loops {
            let (display_path, line) = source.location(&judged.statement)?;
            let verdict = if judged.verdict.is_permitted() {
                "permitted"
            } else {
                "denied"
            };
            let detail = loop_detail(judged, source)?;
            entries.push(Entry::new(
                display_path.clone(),
                line,
                LOOP,
                format!("PAR loop        {display_path}:{line}  loop  {verdict:<10}  {detail}"),
            ));
            if !judged.advises_split {
                continue;
            }
            let condition = judged
                .verdict
                .denied_condition()
                .expect("only a refused loop carries split advice");
            entries.push(Entry::new(
                display_path.clone(),
                line,
                HINT,
                format!(
                    "PAR hint        {display_path}:{line}  loop  refused by condition {condition}; a recursive split over its index range would be eligible, combining under {}",
                    judged.combines.join(", ")
                ),
            ));
        }
    }
    collapse(&mut entries);
    Ok(entries
        .into_iter()
        .map(|entry| LedgerLine { text: entry.text })
        .collect())
}

/// One rendered line before collapsing, with the keys that order it and the
/// keys that identify it.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Entry {
    display_path: String,
    line: u64,
    kind: u8,
    text: String,
}

impl Entry {
    fn new(display_path: String, line: u64, kind: u8, text: String) -> Self {
        Self {
            display_path,
            line,
            kind,
            text,
        }
    }
}

/// Sorts the rendered lines into source order and collapses the duplicates one
/// generic's several instances produce.
///
/// The rendered text carries the path, line, and kind, giving a deterministic
/// reported identity across instances.
fn collapse(entries: &mut Vec<Entry>) {
    entries.sort_by(|left, right| {
        left.display_path
            .cmp(&right.display_path)
            .then(left.line.cmp(&right.line))
            .then(left.kind.cmp(&right.kind))
            .then(left.text.cmp(&right.text))
    });
    entries.dedup_by(|removed, retained| removed.text == retained.text);
}

/// The part of the line that states the outcome.
fn detail<Source: LedgerSource>(
    verdict: &PermissionVerdict,
    source: &Source,
) -> Result<String, Source::Error> {
    Ok(match verdict {
        PermissionVerdict::PermittedEligible => "eligible".to_owned(),
        PermissionVerdict::Denied(denial) => denied_detail(denial, source)?,
    })
}

pub(crate) fn denied_detail<Source: LedgerSource>(
    denial: &Denial,
    source: &Source,
) -> Result<String, Source::Error> {
    // The number comes from the judgment itself, so the reported condition
    // cannot drift from the condition that actually refused the pair.
    let condition = denial.condition();
    let reason = match denial {
        // The kind comes from the conflict loop that found it, so a
        // read/write conflict is never reported as two writes.
        Denial::Footprint {
            kind,
            left,
            right,
            sides,
        } => {
            let (left_half, right_half) = kind.halves();
            format!(
                "the {left_half} of {} overlaps the {right_half} of {} at {} vs {}",
                statement_name(sides.0),
                statement_name(sides.1),
                access(left, source)?,
                access(right, source)?
            )
        }
        Denial::UnresolvedFootprint { side, argument } => format!(
            "unresolved footprint through {} of {}",
            source.spelling(argument)?,
            statement_name(*side)
        ),
        // The disclosure half: a form this judgment does not account for is
        // reported here rather than passed over silently, so the writer sees
        // the statement that costs the overlap.
        Denial::UnclassifiedForm { side, form } => {
            format!("{} is {form}", statement_name(*side))
        }
        Denial::SkippingExit { side, kind } => {
            let edge = match kind {
                ExitKind::PropagateError => "Err edge",
                ExitKind::BlockExit => "exit edge",
            };
            format!(
                "the {edge} of {} may skip the statement written after it",
                statement_name(*side)
            )
        }
        Denial::WaitingCall { side, call } => format!(
            "{} contains the waiting call {}",
            statement_name(*side),
            source.spelling(call)?
        ),
    };
    Ok(format!("condition {condition}: {reason}"))
}

/// The part of a loop line that states the outcome.
fn loop_detail<Source: LedgerSource>(
    judged: &LoopPermission,
    source: &Source,
) -> Result<String, Source::Error> {
    // A permitted loop's accumulator is the fact that makes the overlap safe,
    // so the line carries it rather than leaving a reader to infer it from
    // silence.
    let carried = match judged.combines.as_slice() {
        [] => "no accumulator".to_owned(),
        combines => format!("one accumulator under {}", combines.join(", ")),
    };
    Ok(match &judged.verdict {
        LoopVerdict::PermittedEligible => format!("eligible; {carried}"),
        LoopVerdict::Denied(denial) => loop_denied_detail(denial, source)?,
    })
}

pub(crate) fn loop_denied_detail<Source: LedgerSource>(
    denial: &LoopDenial,
    source: &Source,
) -> Result<String, Source::Error> {
    // The number comes from the judgment itself, so the reported condition
    // cannot drift from the condition that actually refused the loop.
    let condition = denial.condition();
    let reason = match denial {
        LoopDenial::NotAReduction { statement } => format!(
            "the loop writes storage outliving the iteration that no exactly associative operation reduces, at {}",
            source.spelling(statement)?
        ),
        LoopDenial::ManyAccumulators { accumulators } => {
            format!("the body carries {accumulators} accumulators, and this rule recombines one")
        }
        LoopDenial::AccumulatorRead { statement, reads } => format!(
            "the accumulator is read {reads} times in the body and a reduction reads it once, at {}",
            source.spelling(statement)?
        ),
        LoopDenial::SharedWrite { argument } => format!(
            "the body writes storage that is neither introduced by the iteration nor the accumulator, at {}",
            source.spelling(argument)?
        ),
        LoopDenial::UnresolvedWrite { argument } => format!(
            "unresolved write footprint through {}",
            source.spelling(argument)?
        ),
        // The disclosure half: a form this judgment does not account for is
        // reported here rather than passed over silently, so the writer sees
        // the statement that costs the overlap.
        LoopDenial::BodyForm { form } => format!("the body contains {form}"),
        LoopDenial::Exit { edge } => format!("{edge} leaves the loop"),
        LoopDenial::WaitingCall { call } => format!(
            "the body contains the waiting call {}",
            source.spelling(call)?
        ),
    };
    Ok(format!("condition {condition}: {reason}"))
}

/// One footprint element as the writer wrote it.
fn access<Source: LedgerSource>(access: &Access, source: &Source) -> Result<String, Source::Error> {
    source.spelling(&access.argument)
}

/// How the ledger names one member of an adjacency.
const fn statement_name(side: PairSide) -> &'static str {
    match side {
        PairSide::First => "s1",
        PairSide::Second => "s2",
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, collapse};

    #[test]
    fn duplicate_instance_lines_collapse_but_distinct_verdicts_stay() {
        let line = |text: &str| Entry::new("walk.wf".to_owned(), 5, 2, text.to_owned());
        let mut entries = vec![
            line("PAR loop permitted"),
            line("PAR loop denied"),
            line("PAR loop permitted"),
        ];
        collapse(&mut entries);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "PAR loop denied");
        assert_eq!(entries[1].text, "PAR loop permitted");
    }
}
