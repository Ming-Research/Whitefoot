mod format;
mod render;

use crate::syntax::terminal::TerminalPredicate;
use crate::{ByteOffset, ClassifiedBundle, SourceId};

use crate::syntax::parser::{DerivationElement, SyntaxCoordinate};

use self::format::{GapStyle, attachment, build_gap_styles, bytes_match, gap_matches};
pub use self::render::render_canonical;
use super::outcome::{
    CanonicalCompilerFailure, CanonicalIssue, CanonicalLimit, CanonicalLimits, CanonicalLocation,
    CanonicalOutcome, CanonicalResourceFailure, CanonicalStorage, CanonicalSyntaxUnit,
    FinalizedBundle, NodePath,
};
use super::topology::{FinalizedTopology, NodeId};

enum Stop {
    Resource(CanonicalResourceFailure),
    Compiler(CanonicalCompilerFailure),
}

impl From<CanonicalResourceFailure> for Stop {
    fn from(value: CanonicalResourceFailure) -> Self {
        Self::Resource(value)
    }
}

impl From<CanonicalCompilerFailure> for Stop {
    fn from(value: CanonicalCompilerFailure) -> Self {
        Self::Compiler(value)
    }
}

struct AuditWork {
    used: u64,
    maximum: u64,
}

impl AuditWork {
    const fn new(maximum: u64) -> Self {
        Self { used: 0, maximum }
    }

    fn spend(&mut self, amount: u64) -> Result<(), CanonicalResourceFailure> {
        let actual =
            self.used
                .checked_add(amount)
                .ok_or(CanonicalResourceFailure::LimitExceeded {
                    limit: CanonicalLimit::Work,
                    maximum: self.maximum,
                    actual: u64::MAX,
                })?;
        if actual > self.maximum {
            return Err(CanonicalResourceFailure::LimitExceeded {
                limit: CanonicalLimit::Work,
                maximum: self.maximum,
                actual,
            });
        }
        self.used = actual;
        Ok(())
    }
}

fn terminal_element(
    finalized: &FinalizedBundle,
    ordinal: usize,
) -> Result<(crate::lexer::Token, TerminalPredicate), Stop> {
    let record = finalized
        .topology
        .terminals
        .get(ordinal)
        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
    let element = finalized
        .parsed
        .tree
        .elements
        .get(record.element_index)
        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
    let DerivationElement::Terminal { token, predicate } = *element else {
        return Err(CanonicalCompilerFailure::InvalidFinalizedTree.into());
    };
    Ok((token, predicate))
}

/// The bytes a terminal renders as: a fixed terminal's own spelling, and
/// otherwise the token's source bytes; `None` for a token the bundle does
/// not hold.
fn expected_terminal_bytes(
    classified: &ClassifiedBundle,
    token: crate::lexer::Token,
    predicate: TerminalPredicate,
) -> Option<&[u8]> {
    match predicate {
        TerminalPredicate::Fixed(fixed) => Some(fixed.spelling_bytes()),
        _ => classified.token_bytes(token),
    }
}

fn source_coordinate(source: SourceId, start: u64, end: u64) -> SyntaxCoordinate {
    SyntaxCoordinate::new(source, ByteOffset::new(start), ByteOffset::new(end))
}

fn terminal_top_item(topology: &FinalizedTopology, terminal: usize) -> Result<NodeId, Stop> {
    let owner = topology
        .terminals
        .get(terminal)
        .and_then(|record| record.owner)
        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
    topology
        .node(owner)
        .and_then(|node| node.top_item)
        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree.into())
}

fn lowest_common_ancestor(
    topology: &FinalizedTopology,
    left_terminal: usize,
    right_terminal: usize,
    work: &mut AuditWork,
) -> Result<NodeId, Stop> {
    let mut left = topology
        .terminals
        .get(left_terminal)
        .and_then(|record| record.owner)
        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
    let mut right = topology
        .terminals
        .get(right_terminal)
        .and_then(|record| record.owner)
        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
    loop {
        work.spend(1)?;
        let left_node = topology
            .node(left)
            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        let right_node = topology
            .node(right)
            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        if left_node.tree_depth > right_node.tree_depth {
            left = left_node
                .parent
                .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        } else if right_node.tree_depth > left_node.tree_depth {
            right = right_node
                .parent
                .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        } else if left == right {
            return Ok(left);
        } else {
            left = left_node
                .parent
                .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
            right = right_node
                .parent
                .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        }
    }
}

fn node_path(
    topology: &FinalizedTopology,
    mut node: NodeId,
    limits: CanonicalLimits,
    work: &mut AuditWork,
) -> Result<NodePath, Stop> {
    let mut components = Vec::new();
    while node != topology.root {
        work.spend(1)?;
        let record = topology
            .node(node)
            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        let actual = u64::try_from(components.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(CanonicalResourceFailure::AddressSpaceExceeded {
                storage: CanonicalStorage::NodePath,
                requested: u64::MAX,
            })?;
        if actual > limits.max_path_components {
            return Err(CanonicalResourceFailure::LimitExceeded {
                limit: CanonicalLimit::PathComponents,
                maximum: limits.max_path_components,
                actual,
            }
            .into());
        }
        components
            .try_reserve(1)
            .map_err(|_| CanonicalResourceFailure::StorageUnavailable {
                storage: CanonicalStorage::NodePath,
                requested: actual,
            })?;
        components.push(record.child_ordinal);
        node = record
            .parent
            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
    }
    components.reverse();
    Ok(NodePath { components })
}

struct MismatchGap {
    source: SourceId,
    start: u64,
    end: u64,
    left_terminal: Option<usize>,
    right_terminal: Option<usize>,
    /// The exact trivia bytes FORM-2 requires here, and the bytes the source
    /// carries instead. The auditor decided the first and read the second, so
    /// the rejection carries both rather than a coordinate to bisect from.
    expected: String,
    found: String,
}

fn mismatch_issue(
    topology: &FinalizedTopology,
    gap: MismatchGap,
    limits: CanonicalLimits,
    work: &mut AuditWork,
) -> Result<CanonicalIssue, Stop> {
    let coordinate = source_coordinate(gap.source, gap.start, gap.end);
    let expected = gap.expected;
    let found = gap.found;
    let (Some(left), Some(right)) = (gap.left_terminal, gap.right_terminal) else {
        return Ok(CanonicalIssue {
            location: CanonicalLocation::SourceBytes(coordinate),
            expected,
            found,
        });
    };
    if terminal_top_item(topology, left)? != terminal_top_item(topology, right)? {
        return Ok(CanonicalIssue {
            location: CanonicalLocation::SourceBytes(coordinate),
            expected,
            found,
        });
    }
    let owner = lowest_common_ancestor(topology, left, right, work)?;
    let path = node_path(topology, owner, limits, work)?;
    Ok(CanonicalIssue {
        location: CanonicalLocation::SourceNode(path, coordinate),
        expected,
        found,
    })
}

fn preflight_sources(
    finalized: &FinalizedBundle,
    limits: CanonicalLimits,
    work: &mut AuditWork,
) -> Result<(), Stop> {
    let mut total = 0_u64;
    for (_, file) in finalized.parsed.classified.source_bundle().iter() {
        work.spend(1)?;
        let length = file.byte_len();
        if length > limits.max_source_bytes {
            return Err(CanonicalResourceFailure::LimitExceeded {
                limit: CanonicalLimit::SourceBytes,
                maximum: limits.max_source_bytes,
                actual: length,
            }
            .into());
        }
        total = total
            .checked_add(length)
            .ok_or(CanonicalCompilerFailure::CounterOverflow)?;
        if total > limits.max_total_source_bytes {
            return Err(CanonicalResourceFailure::LimitExceeded {
                limit: CanonicalLimit::TotalSourceBytes,
                maximum: limits.max_total_source_bytes,
                actual: total,
            }
            .into());
        }
    }
    Ok(())
}

fn audit(
    finalized: &FinalizedBundle,
    limits: CanonicalLimits,
    work: &mut AuditWork,
) -> Result<Option<CanonicalIssue>, Stop> {
    preflight_sources(finalized, limits, work)?;
    let gaps = build_gap_styles(&finalized.topology, limits, work)?;
    let classified = &finalized.parsed.classified;
    for (source, file) in classified.source_bundle().iter() {
        work.spend(1)?;
        let source_index = usize::try_from(source.ordinal())
            .map_err(|_| CanonicalCompilerFailure::CounterOverflow)?;
        let start = *classified
            .source_offsets
            .get(source_index)
            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        let end = *classified
            .source_offsets
            .get(
                source_index
                    .checked_add(1)
                    .ok_or(CanonicalCompilerFailure::CounterOverflow)?,
            )
            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        let source_bytes = file.bytes();
        if start == end {
            if !bytes_match(source_bytes, core::iter::once(b'\n'), 1, work)? {
                return mismatch_issue(
                    &finalized.topology,
                    MismatchGap {
                        source,
                        start: 0,
                        end: file.byte_len(),
                        left_terminal: None,
                        right_terminal: None,
                        expected: "\n".to_owned(),
                        found: String::from_utf8_lossy(source_bytes).into_owned(),
                    },
                    limits,
                    work,
                )
                .map(Some);
            }
            continue;
        }
        let (first_token, _) = terminal_element(finalized, start)?;
        let first_start = first_token.id().start().value();
        let leading_end =
            usize::try_from(first_start).map_err(|_| CanonicalCompilerFailure::CounterOverflow)?;
        let leading = source_bytes
            .get(..leading_end)
            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        if !bytes_match(leading, core::iter::empty(), 0, work)? {
            return mismatch_issue(
                &finalized.topology,
                MismatchGap {
                    source,
                    start: 0,
                    end: first_start,
                    left_terminal: None,
                    right_terminal: Some(start),
                    expected: String::new(),
                    found: String::from_utf8_lossy(leading).into_owned(),
                },
                limits,
                work,
            )
            .map(Some);
        }
        let mut expected_source_len = 0_u64;
        for ordinal in start..end {
            work.spend(1)?;
            let (token, predicate) = terminal_element(finalized, ordinal)?;
            let terminal_record = finalized
                .topology
                .terminals
                .get(ordinal)
                .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
            let local_ordinal = ordinal
                .checked_sub(start)
                .and_then(|value| u64::try_from(value).ok())
                .ok_or(CanonicalCompilerFailure::CounterOverflow)?;
            let classified_token = classified
                .tokens()
                .get(ordinal)
                .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
            let token_id = token.id();
            let expected_id = classified_token.token().id();
            if token_id.source() != source
                || terminal_record.source != source
                || terminal_record.local_ordinal != local_ordinal
                || token_id.source() != expected_id.source()
                || token_id.start() != expected_id.start()
                || token_id.end() != expected_id.end()
                || !classified_token.terminals().contains(predicate)
            {
                return Err(CanonicalCompilerFailure::TerminalBindingDisagreement.into());
            }
            let actual_terminal = classified
                .token_bytes(token)
                .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
            let expected_terminal = expected_terminal_bytes(classified, token, predicate)
                .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
            expected_source_len = expected_source_len
                .checked_add(
                    u64::try_from(expected_terminal.len())
                        .map_err(|_| CanonicalCompilerFailure::CounterOverflow)?,
                )
                .ok_or(CanonicalCompilerFailure::CounterOverflow)?;
            if !bytes_match(
                actual_terminal,
                expected_terminal.iter().copied(),
                expected_terminal.len(),
                work,
            )? {
                return Err(CanonicalCompilerFailure::TerminalBindingDisagreement.into());
            }
            let next = ordinal.checked_add(1);
            let (gap_end, right_attachment, right_terminal, style, depth) =
                if next.is_some_and(|value| value < end) {
                    let next_ordinal = next.ok_or(CanonicalCompilerFailure::CounterOverflow)?;
                    let (right_token, right_predicate) = terminal_element(finalized, next_ordinal)?;
                    let right_attachment =
                        attachment(&finalized.topology, next_ordinal, right_predicate)?;
                    let owner = finalized
                        .topology
                        .terminals
                        .get(next_ordinal)
                        .and_then(|record| record.owner)
                        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
                    let depth = finalized
                        .topology
                        .node(owner)
                        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?
                        .terminal_depth(
                            u64::try_from(next_ordinal)
                                .map_err(|_| CanonicalCompilerFailure::CounterOverflow)?,
                        );
                    (
                        right_token.id().start().value(),
                        Some(right_attachment),
                        Some(next_ordinal),
                        *gaps
                            .get(next_ordinal)
                            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?,
                        depth,
                    )
                } else {
                    (file.byte_len(), None, None, GapStyle::Break, 0)
                };
            let gap_start = token_id.end().value();
            let actual_start = usize::try_from(gap_start)
                .map_err(|_| CanonicalCompilerFailure::CounterOverflow)?;
            let actual_end =
                usize::try_from(gap_end).map_err(|_| CanonicalCompilerFailure::CounterOverflow)?;
            let actual_gap = source_bytes
                .get(actual_start..actual_end)
                .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
            let (gap_agrees, expected_gap_len, expected_gap) = gap_matches(
                actual_gap,
                style,
                depth,
                Some(attachment(&finalized.topology, ordinal, predicate)?),
                right_attachment,
                work,
            )?;
            if !gap_agrees {
                return mismatch_issue(
                    &finalized.topology,
                    MismatchGap {
                        source,
                        start: gap_start,
                        end: gap_end,
                        left_terminal: Some(ordinal),
                        right_terminal,
                        expected: expected_gap.text(),
                        found: String::from_utf8_lossy(actual_gap).into_owned(),
                    },
                    limits,
                    work,
                )
                .map(Some);
            }
            expected_source_len = expected_source_len
                .checked_add(expected_gap_len)
                .ok_or(CanonicalCompilerFailure::CounterOverflow)?;
        }
        if expected_source_len != file.byte_len() {
            return Err(CanonicalCompilerFailure::InvalidFinalizedTree.into());
        }
    }
    Ok(None)
}

/// Audits exact per-source FORM-2 bytes from the finalized derivation tree.
#[must_use]
pub fn audit_canonical(finalized: FinalizedBundle, limits: CanonicalLimits) -> CanonicalOutcome {
    let mut work = AuditWork::new(limits.max_work);
    match audit(&finalized, limits, &mut work) {
        Ok(None) => CanonicalOutcome::Complete(CanonicalSyntaxUnit { finalized }),
        Ok(Some(issue)) => CanonicalOutcome::SourceIssue(issue),
        Err(Stop::Resource(failure)) => CanonicalOutcome::ResourceFailure(failure),
        Err(Stop::Compiler(failure)) => CanonicalOutcome::CompilerFailure(failure),
    }
}
