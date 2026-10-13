//! Repairs selected by a rejected lexical spelling, without changing its owner or span.

use crate::SourceIssueKind;
use crate::diagnostic_repairs::{BOOLEAN_OPERATION_FIX, RANGE1_BOOLEAN_OR_FIX, lowercase_name};

/// Whether the current `;`-, `{`- or `}`-delimited item text, before a `||` the
/// lexer rejects, already holds the `forall` that opens a RANGE-1 clause.
/// The lexer has no grammar frame, so this is the one place the position is
/// read from bytes.
fn in_range_clause(bytes: &[u8], start: usize) -> bool {
    let item_start = bytes[..start]
        .iter()
        .rposition(|byte| matches!(byte, b';' | b'{' | b'}'))
        .map_or(0, |index| index + 1);
    bytes[item_start..start]
        .split(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
        .any(|word| word == b"forall")
}

pub(crate) fn raw_fix(bytes: &[u8], start: usize, kind: SourceIssueKind) -> Option<String> {
    if kind != SourceIssueKind::UnexpectedByte {
        return None;
    }
    if bytes.get(start..)?.starts_with(b"||") {
        return Some(
            if in_range_clause(bytes, start) {
                RANGE1_BOOLEAN_OR_FIX
            } else {
                BOOLEAN_OPERATION_FIX
            }
            .to_owned(),
        );
    }
    if bytes.get(start) != Some(&b'_') {
        return None;
    }
    // GRAM-1 ends an upper word before `_`; keep that byte's FORM-1 defect
    // and give the complete lower-word spelling FORM-3 admits instead.
    let candidate_start = bytes[..start]
        .iter()
        .rposition(|byte| !byte.is_ascii_alphanumeric())
        .map_or(0, |index| index + 1);
    let word_start = candidate_start
        + bytes[candidate_start..start]
            .iter()
            .position(u8::is_ascii_uppercase)?;
    // A TYPEID name may contain upper words too. Offer the lowercase repair
    // only after the raw `const` introducer, whose binder is an IDENT.
    let prefix = &bytes[..word_start];
    let prefix_end = prefix
        .iter()
        .rposition(|byte| !matches!(byte, b' ' | b'\n'))?
        + 1;
    if prefix_end == word_start {
        return None;
    }
    let prefix_start = prefix[..prefix_end]
        .iter()
        .rposition(|byte| !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && *byte != b'_')
        .map_or(0, |index| index + 1);
    if prefix.get(prefix_start..prefix_end) != Some(b"const".as_slice()) {
        return None;
    }
    let end = bytes[start..]
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
        .map_or(bytes.len(), |length| start + length);
    let spelling = std::str::from_utf8(&bytes[word_start..end]).ok()?;
    Some(lowercase_name(spelling))
}
