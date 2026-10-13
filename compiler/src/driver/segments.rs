//! DIAG-1's independent raw-line segments, before whole-source canonical form.

use core::ops::Range;

use crate::syntax::grammar::{LookaheadPredicate, Production, grammar_prefixes};
use crate::syntax::terminal::{TerminalPredicate, is_identifier};
use crate::{ByteOffset, SourceBundle, SourceInput, SyntaxCoordinate};

use super::{CompilationFailure, CompilationFailureKind, CompilerLimits, derive_syntax};

/// The maximal lowercase word at a physical line's first byte. This boundary
/// operation reads raw bytes, including bytes inside an unfinished token.
fn leading_word(line: &[u8]) -> &[u8] {
    if !line.first().is_some_and(u8::is_ascii_lowercase) {
        return &[];
    }
    let length = line
        .iter()
        .take_while(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || **byte == b'_')
        .count();
    &line[..length]
}

fn starts_segment(line: &[u8], graph: bool) -> bool {
    let word = leading_word(line);
    let productions: &[Production] = if graph {
        &[
            Production::PackageDecl,
            Production::ModuleRow,
            Production::EntryDecl,
        ]
    } else {
        &[Production::Item]
    };
    productions.iter().any(|production| {
        grammar_prefixes(production.root(), false).iter().any(|row| {
            row[0].is_some_and(|atom| match atom.predicate() {
                LookaheadPredicate::Terminal(TerminalPredicate::Fixed(terminal)) => {
                    word == terminal.spelling_bytes()
                }
                LookaheadPredicate::Terminal(TerminalPredicate::Identifier) => is_identifier(word),
                _ => false,
            })
        })
    })
}

fn ranges(bytes: &[u8], graph: bool) -> Vec<Range<usize>> {
    let mut starts = vec![0];
    let mut offset = 0;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if offset != 0 && starts_segment(line, graph) {
            starts.push(offset);
        }
        offset += line.len();
    }
    starts.push(bytes.len());
    starts.windows(2).map(|pair| pair[0]..pair[1]).collect()
}

pub(super) fn check(
    bundle: &SourceBundle,
    limits: CompilerLimits,
    graph: bool,
) -> Result<(), CompilationFailure> {
    let mut rejected: Option<CompilationFailure> = None;
    for (source, file) in bundle.iter() {
        // The ordinary whole-unit pass validates compiler-owned prelude heads,
        // whose private signature grammar is not writer source segmentation.
        if file.prelude().is_some() {
            continue;
        }
        for range in ranges(file.bytes(), graph) {
            let input = SourceInput::from_host_path(
                file.logical_path().as_str(),
                file.display_path(),
                &file.bytes()[range.clone()],
            );
            let segment = SourceBundle::with_limits(&[input], limits.source)
                .map_err(CompilationFailure::source_envelope)?;
            if let Err(mut failure) = derive_syntax(&segment, limits, graph, graph) {
                if failure.kind() != CompilationFailureKind::Source {
                    return Err(failure);
                }
                if let Some(local) = failure.source_coordinate() {
                    let offset = range.start as u64;
                    let coordinate = SyntaxCoordinate::new(
                        source,
                        ByteOffset::new(offset + local.start().value()),
                        ByteOffset::new(offset + local.end().value()),
                    );
                    failure.coordinate = Some(coordinate);
                    failure.record.relocate(bundle, coordinate);
                }
                match &mut rejected {
                    Some(first) => first.additional.push(failure),
                    None => rejected = Some(failure),
                }
            }
        }
    }
    rejected.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::{ranges, starts_segment};

    #[test]
    fn declaration_boundaries_are_generated_first_words_at_column_zero() {
        for keyword in [
            "alias", "public", "fn", "struct", "enum", "interface", "binding", "const",
            "program", "opaque", "nocopy", "nodrop",
        ] {
            assert!(
                starts_segment(format!("{keyword} ").as_bytes(), false),
                "{keyword}"
            );
            assert!(!starts_segment(format!(" {keyword} ").as_bytes(), false));
            assert!(!starts_segment(format!("{keyword}_extra ").as_bytes(), false));
        }
        for text in ["doc ", "return ", "let ", "entry ", "package ", "fn0 "] {
            assert!(!starts_segment(text.as_bytes(), false), "{text}");
        }
    }

    #[test]
    fn prefix_empty_source_and_unfinished_quotes_keep_raw_boundaries() {
        assert_eq!(ranges(b"", false), vec![0..0]);
        assert_eq!(ranges(b"\nfn a\nfn b\n", false), vec![0..1, 1..6, 6..11]);
        assert_eq!(ranges(b"doc \"unfinished\nfn b\n", false), vec![0..16, 16..21]);
        assert_eq!(ranges(b"fn a\n  fn b\nfn c", false), vec![0..12, 12..16]);
    }

    #[test]
    fn graph_boundaries_use_identifier_membership_and_row_keywords() {
        for text in ["package x", "entry run", "pkg:", "std::io:", "other::child:"] {
            assert!(starts_segment(text.as_bytes(), true), "{text}");
        }
        for text in [" fn", "fn", "Upper", "0_name", "pkg_extra:"] {
            assert_eq!(
                starts_segment(text.as_bytes(), true),
                text == "pkg_extra:",
                "{text}"
            );
        }
        assert_eq!(
            ranges(b"pkg: []; other: [];\nentry run = pkg;\n", true),
            vec![0..20, 20..37]
        );
    }
}
