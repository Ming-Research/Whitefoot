//! The gaps of one rejected source whose bytes differ from FORM-2's.
//!
//! [DIAG-1] has the audit report its first mismatch only, and that stays. A
//! writer who repairs layout one rejection per compile spends a round on each
//! fault, so the repair of that rejection lists every gap that differs. The
//! walk below visits the rejected source's terminals as the auditor does and
//! compares each gap with the same [`gap_matches`] bytes the renderer emits
//! from, so what it lists is exactly what rendering would change and none of
//! it is a second layout rule.

use super::format::{attachment, build_gap_styles, gap_after, gap_matches};
use super::{AuditWork, Stop, terminal_element};
use crate::SourceId;
use crate::syntax::parser::finalize::outcome::{
    CanonicalCompilerFailure, CanonicalLimits, FinalizedBundle, LayoutDifference,
};

/// The most differences a repair names; the rest are only counted.
const LISTED: usize = 20;

/// The differences found so far, and the line counter that places them.
struct Listing<'bytes> {
    listed: Vec<LayoutDifference>,
    total: u64,
    bytes: &'bytes [u8],
    /// The offset up to which `line` has counted newlines.
    counted: usize,
    /// The one-based line `counted` is on.
    line: u64,
}

impl<'bytes> Listing<'bytes> {
    fn new(bytes: &'bytes [u8]) -> Self {
        Self {
            listed: Vec::new(),
            total: 0,
            bytes,
            counted: 0,
            line: 1,
        }
    }

    /// The one-based line holding `offset`. Gaps arrive in byte order, so the
    /// count only moves forward and the walk reads each byte once.
    fn line_at(&mut self, offset: usize) -> u64 {
        if let Some(passed) = self.bytes.get(self.counted..offset) {
            let newlines = passed.iter().filter(|byte| **byte == b'\n').count();
            self.line = self
                .line
                .saturating_add(u64::try_from(newlines).unwrap_or(u64::MAX));
            self.counted = offset;
        }
        self.line
    }

    /// Notes one differing gap that begins at `offset`.
    fn record(
        &mut self,
        offset: u64,
        neighbors: (Option<&[u8]>, Option<&[u8]>),
        expected: String,
        found: &[u8],
    ) -> Result<(), Stop> {
        self.total = self
            .total
            .checked_add(1)
            .ok_or(CanonicalCompilerFailure::CounterOverflow)?;
        if self.listed.len() < LISTED {
            let offset =
                usize::try_from(offset).map_err(|_| CanonicalCompilerFailure::CounterOverflow)?;
            let line = self.line_at(offset);
            let spelling = |bytes: Option<&[u8]>| {
                bytes.map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            };
            self.listed.push(LayoutDifference {
                line,
                before: spelling(neighbors.0),
                after: spelling(neighbors.1),
                expected,
                found: String::from_utf8_lossy(found).into_owned(),
            });
        }
        Ok(())
    }
}

/// Lists the gaps of `target` that differ from FORM-2's, in byte order, as
/// the first [`LISTED`] of them and the count of all.
///
/// The walk has its own work ceiling, so a source the audit rejected early may
/// still be too long to list; the caller then keeps the rejection without the
/// list.
pub(super) fn list(
    finalized: &FinalizedBundle,
    limits: CanonicalLimits,
    target: SourceId,
) -> Result<(Vec<LayoutDifference>, u64), Stop> {
    let mut work = AuditWork::new(limits.max_work);
    let gaps = build_gap_styles(&finalized.topology, limits, &mut work)?;
    let classified = &finalized.parsed.classified;
    let file = classified
        .source_bundle()
        .file(target)
        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
    let index =
        usize::try_from(target.ordinal()).map_err(|_| CanonicalCompilerFailure::CounterOverflow)?;
    let start = *classified
        .source_offsets
        .get(index)
        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
    let end = *classified
        .source_offsets
        .get(
            index
                .checked_add(1)
                .ok_or(CanonicalCompilerFailure::CounterOverflow)?,
        )
        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
    let bytes = file.bytes();
    let mut listing = Listing::new(bytes);
    // A source holding no terminal is one newline.
    if start == end {
        if bytes != b"\n" {
            listing.record(0, (None, None), "\n".to_owned(), bytes)?;
        }
        return Ok((listing.listed, listing.total));
    }
    let (first, _) = terminal_element(finalized, start)?;
    let leading_end = usize::try_from(first.id().start().value())
        .map_err(|_| CanonicalCompilerFailure::CounterOverflow)?;
    let leading = bytes
        .get(..leading_end)
        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
    if !leading.is_empty() {
        let first_spelling = classified
            .token_bytes(first)
            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        listing.record(0, (None, Some(first_spelling)), String::new(), leading)?;
    }
    for ordinal in start..end {
        work.spend(1)?;
        let (token, predicate) = terminal_element(finalized, ordinal)?;
        let after = gap_after(finalized, &gaps, ordinal, end)?;
        let gap_start = token.id().end().value();
        let gap_end = after.right_start.unwrap_or_else(|| file.byte_len());
        let from =
            usize::try_from(gap_start).map_err(|_| CanonicalCompilerFailure::CounterOverflow)?;
        let to = usize::try_from(gap_end).map_err(|_| CanonicalCompilerFailure::CounterOverflow)?;
        let actual = bytes
            .get(from..to)
            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        let (agrees, _, expected) = gap_matches(
            actual,
            after.style,
            after.depth,
            Some(attachment(&finalized.topology, ordinal, predicate)?),
            after.right,
            &mut work,
        )?;
        if agrees {
            continue;
        }
        let before = classified
            .token_bytes(token)
            .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?;
        let following = match after.next {
            Some(next) => {
                let (right, _) = terminal_element(finalized, next)?;
                Some(
                    classified
                        .token_bytes(right)
                        .ok_or(CanonicalCompilerFailure::InvalidFinalizedTree)?,
                )
            }
            None => None,
        };
        listing.record(
            gap_start,
            (Some(before), following),
            expected.text(),
            actual,
        )?;
    }
    Ok((listing.listed, listing.total))
}
