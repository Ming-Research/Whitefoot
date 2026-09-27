//! Emission of exact FORM-2 bytes from a finalized derivation tree.
//!
//! The auditor beside this module answers whether given bytes are canonical;
//! this one produces the canonical bytes a tree denotes. They share the layout
//! rules rather than restating them: [`build_gap_styles`] decides every
//! terminal boundary's style and [`canonical_gap`] turns a style, a format
//! depth and the two neighbouring predicates into exact bytes, so a change to
//! either rule moves the auditor and the renderer together.

use super::format::{GapStyle, attachment, build_gap_styles, canonical_gap};
use super::{AuditWork, Stop, expected_terminal_bytes, preflight_sources, terminal_element};
use crate::syntax::parser::finalize::outcome::{
    CanonicalCompilerFailure, CanonicalLimit, CanonicalLimits, CanonicalResourceFailure,
    CanonicalStorage, FinalizedBundle, RenderOutcome, RenderedSource,
};

/// Appends `bytes` to one source's rendering under its byte ceiling.
fn extend_rendering(
    out: &mut Vec<u8>,
    bytes: impl Iterator<Item = u8>,
    len: usize,
    limits: CanonicalLimits,
    work: &mut AuditWork,
) -> Result<(), Stop> {
    let requested = u64::try_from(len).map_err(|_| CanonicalCompilerFailure::CounterOverflow)?;
    work.spend(requested)?;
    let total = u64::try_from(out.len())
        .ok()
        .and_then(|used| used.checked_add(requested))
        .ok_or(CanonicalResourceFailure::AddressSpaceExceeded {
            storage: CanonicalStorage::RenderedSource,
            requested: u64::MAX,
        })?;
    if total > limits.max_source_bytes {
        return Err(CanonicalResourceFailure::LimitExceeded {
            limit: CanonicalLimit::SourceBytes,
            maximum: limits.max_source_bytes,
            actual: total,
        }
        .into());
    }
    out.try_reserve(len)
        .map_err(|_| CanonicalResourceFailure::StorageUnavailable {
            storage: CanonicalStorage::RenderedSource,
            requested: total,
        })?;
    out.extend(bytes);
    Ok(())
}

fn render(
    finalized: &FinalizedBundle,
    limits: CanonicalLimits,
    work: &mut AuditWork,
) -> Result<Vec<RenderedSource>, Stop> {
    preflight_sources(finalized, limits, work)?;
    let gaps = build_gap_styles(&finalized.topology, limits, work)?;
    let classified = &finalized.parsed.classified;
    let mut rendered = Vec::new();
    let sources = classified.source_bundle().len();
    rendered.try_reserve_exact(sources).map_err(|_| {
        CanonicalResourceFailure::StorageUnavailable {
            storage: CanonicalStorage::RenderedSource,
            requested: u64::try_from(sources).unwrap_or(u64::MAX),
        }
    })?;

    for (source, _) in classified.source_bundle().iter() {
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
        let mut bytes = Vec::new();

        // A source holding no terminal is one newline; there is no leading
        // trivia in any source, so rendering starts at the first terminal.
        if start == end {
            extend_rendering(&mut bytes, core::iter::once(b'\n'), 1, limits, work)?;
            rendered.push(RenderedSource { source, bytes });
            continue;
        }

        for ordinal in start..end {
            work.spend(1)?;
            let (token, predicate) = terminal_element(finalized, ordinal)?;
            let terminal = expected_terminal_bytes(classified, token, predicate)
                .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
            extend_rendering(
                &mut bytes,
                terminal.iter().copied(),
                terminal.len(),
                limits,
                work,
            )?;

            // The gap belongs to the boundary before the next terminal, so its
            // style and indentation are that terminal's. After the last one the
            // source ends in a bare newline, which is a depth-zero break.
            let next = ordinal
                .checked_add(1)
                .ok_or(CanonicalCompilerFailure::CounterOverflow)?;
            let (style, depth, right_attachment) = if next < end {
                let (_, right_predicate) = terminal_element(finalized, next)?;
                let right_attachment = attachment(&finalized.topology, next, right_predicate)?;
                let owner = finalized
                    .topology
                    .terminals
                    .get(next)
                    .and_then(|record| record.owner)
                    .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
                let depth = finalized
                    .topology
                    .node(owner)
                    .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?
                    .terminal_depth(
                        u64::try_from(next)
                            .map_err(|_| CanonicalCompilerFailure::CounterOverflow)?,
                    );
                let style = *gaps
                    .get(next)
                    .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
                (style, depth, Some(right_attachment))
            } else {
                (GapStyle::Break, 0, None)
            };
            let left_attachment = attachment(&finalized.topology, ordinal, predicate)?;
            let gap = canonical_gap(style, depth, Some(left_attachment), right_attachment)?;
            extend_rendering(&mut bytes, gap.bytes(), gap.len()?, limits, work)?;
        }
        rendered.push(RenderedSource { source, bytes });
    }
    Ok(rendered)
}

/// Renders exact per-source FORM-2 bytes from the finalized derivation tree.
///
/// The result is canonical by construction: auditing it always succeeds.
#[must_use]
pub fn render_canonical(finalized: &FinalizedBundle, limits: CanonicalLimits) -> RenderOutcome {
    let mut work = AuditWork::new(limits.max_work);
    match render(finalized, limits, &mut work) {
        Ok(sources) => RenderOutcome::Complete(sources),
        Err(Stop::Resource(failure)) => RenderOutcome::ResourceFailure(failure),
        Err(Stop::Compiler(failure)) => RenderOutcome::CompilerFailure(failure),
    }
}
