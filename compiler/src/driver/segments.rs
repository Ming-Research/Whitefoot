//! DIAG-1's independent raw-line segments, before whole-source canonical form.

use core::ops::Range;

use crate::syntax::grammar::{LookaheadPredicate, Production, grammar_prefixes};
use crate::syntax::terminal::{TerminalPredicate, is_identifier};
use crate::{ByteOffset, ParsedBundle, SourceBundle, SourceInput, SyntaxCoordinate};

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

/// A successful whole derivation also derives every diagnostic segment when
/// no raw boundary cuts one of its complete root children. Successful lexing
/// cannot form a token across LF, so these boundaries preserve lexical and
/// terminal membership results too. Each resulting source segment is an
/// `item*`, and each graph segment is a sequence of complete graph rows.
pub(super) fn aligns(bundle: &SourceBundle, parsed: &ParsedBundle, graph: bool) -> bool {
    let boundaries: Vec<Vec<usize>> = bundle
        .iter()
        .map(|(_, file)| {
            if file.prelude().is_some() {
                Vec::new()
            } else {
                ranges(file.bytes(), graph)
                    .into_iter()
                    .map(|range| range.start)
                    .collect()
            }
        })
        .collect();
    parsed.top_level_extents().all(|extent| {
        let starts = &boundaries[extent.source().ordinal() as usize];
        let next = starts.partition_point(|offset| *offset as u64 <= extent.start().value());
        starts
            .get(next)
            .is_none_or(|offset| *offset as u64 >= extent.end().value())
    })
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
    use super::{aligns, check, ranges, starts_segment};
    use crate::driver::{
        CompilationFailureKind, CompilerLimits, DiagnosticFormat, canonical_syntax, derive_syntax,
    };
    use crate::{SourceBundle, SourceInput};

    #[test]
    fn complete_root_children_certify_source_and_graph_segments() {
        let limits = CompilerLimits::default();
        let inputs = [
            SourceInput::new(
                "first.wf",
                b"alias Value = pkg::Value;\n\nfn first() -> r: unit pure {\n  return unit;\n}\n",
            ),
            SourceInput::new(
                "second.wf",
                b"interface Reader {\n  fn read() -> r: unit pure;\n}\n",
            ),
            SourceInput::new("empty.wf", b"\n"),
        ];
        let bundle = SourceBundle::with_limits(&inputs, limits.source).expect("source bundle");
        let parsed = derive_syntax(&bundle, limits, false, false).expect("whole source derives");
        assert_eq!(
            parsed
                .top_level_extents()
                .map(|extent| extent.source().ordinal())
                .collect::<Vec<_>>(),
            vec![1, 0, 0]
        );
        assert!(aligns(&bundle, &parsed, false));
        check(&bundle, limits, false).expect("the certified segments independently derive");
        canonical_syntax(&bundle, limits, false).expect("the reused derivation is canonical");

        let graph = SourceBundle::with_limits(
            &[SourceInput::new(
                "modules.wfg",
                b"pkg: [];\nother: [pkg];\n\nentry run = pkg;\n",
            )],
            limits.source,
        )
        .expect("graph bundle");
        let parsed = derive_syntax(&graph, limits, true, false).expect("whole graph derives");
        assert_eq!(parsed.top_level_extents().count(), 3);
        assert!(aligns(&graph, &parsed, true));
        check(&graph, limits, true).expect("the certified graph segments independently derive");
        canonical_syntax(&graph, limits, true).expect("the reused graph derivation is canonical");
    }

    #[test]
    fn whole_derivation_does_not_hide_a_boundary_inside_a_declaration_or_graph_row() {
        let limits = CompilerLimits::default();
        for (source, graph) in [
            ("public\nfn f() -> r: unit pure {\n  return unit;\n}\n", false),
            ("interface Reader {\nfn read() -> r: unit pure;\n}\n", false),
            ("pkg: [\nother\n];\n", true),
        ] {
            let bundle = SourceBundle::with_limits(
                &[SourceInput::new("split.wf", source.as_bytes())],
                limits.source,
            )
            .expect("bundle");
            let parsed = derive_syntax(&bundle, limits, graph, false)
                .expect("the complete token sequence derives");
            assert!(!aligns(&bundle, &parsed, graph), "{source}");
            let independent = check(&bundle, limits, graph).expect_err("a raw segment rejects");
            let actual = canonical_syntax(&bundle, limits, graph)
                .expect_err("independent syntax failures precede canonical rendering");
            assert_eq!(
                actual.render(DiagnosticFormat::Json),
                independent.render(DiagnosticFormat::Json),
                "{source}"
            );
        }
    }

    #[test]
    fn whole_source_failure_keeps_independent_segment_order_and_resource_precedence() {
        let inputs = [
            SourceInput::new("first.wf", b"fn first() -> r: unit {\n  return unit;\n}\n"),
            SourceInput::new(
                "second.wf",
                b"fn second() -> r: unit pure {\n  // bad\n}\n",
            ),
        ];
        let limits = CompilerLimits::default();
        let bundle = SourceBundle::with_limits(&inputs, limits.source).expect("bundle");
        let independent = check(&bundle, limits, false).expect_err("two independent failures");
        let actual = canonical_syntax(&bundle, limits, false).expect_err("two failures");
        assert_eq!(actual.diagnostics().count(), 2);
        assert_eq!(
            actual.render(DiagnosticFormat::Json),
            independent.render(DiagnosticFormat::Json)
        );

        let mut limits = limits;
        limits.lexer.max_tokens = 20;
        let bundle = SourceBundle::with_limits(
            &[SourceInput::new(
                "limited.wf",
                b"fn first() -> r: unit {\n  return unit;\n}\nfn second() -> r: unit pure {\n  return unit;\n}\n",
            )],
            limits.source,
        )
        .expect("bundle");
        assert_eq!(
            derive_syntax(&bundle, limits, false, false)
                .expect_err("the complete source exceeds the token ceiling")
                .kind(),
            CompilationFailureKind::Resource
        );
        let independent = check(&bundle, limits, false).expect_err("the first segment rejects");
        assert_eq!(independent.kind(), CompilationFailureKind::Source);
        let actual = canonical_syntax(&bundle, limits, false)
            .expect_err("the source rejection still precedes the whole-source resource failure");
        assert_eq!(
            actual.render(DiagnosticFormat::Json),
            independent.render(DiagnosticFormat::Json)
        );
    }

    #[test]
    fn later_segment_resource_failure_replaces_accumulated_source_rejections() {
        let first = "fn first() -> r: unit {\n  return unit;\n}\n";
        let second = "fn second() -> r: unit pure {\n  let value = 0_u64;\n  let other = 1_u64;\n  return unit;\n}\n";
        let source = format!("{first}{second}");
        let mut limits = CompilerLimits::default();
        let bundle = SourceBundle::with_limits(
            &[SourceInput::new("later-resource.wf", source.as_bytes())],
            limits.source,
        )
        .expect("bundle");
        let ordinary = check(&bundle, limits, false).expect_err("only the first segment rejects");
        assert_eq!(ordinary.kind(), CompilationFailureKind::Source);
        assert_eq!(ordinary.diagnostics().count(), 1);

        limits.lexer.max_tokens = 20;
        let first_bundle = SourceBundle::with_limits(
            &[SourceInput::new("later-resource.wf", first.as_bytes())],
            limits.source,
        )
        .expect("first segment bundle");
        let first_failure =
            check(&first_bundle, limits, false).expect_err("the earlier source rejection completes");
        assert_eq!(first_failure.kind(), CompilationFailureKind::Source);
        assert_eq!(
            first_failure.render(DiagnosticFormat::Json),
            ordinary.render(DiagnosticFormat::Json)
        );

        let second_bundle = SourceBundle::with_limits(
            &[SourceInput::new("later-resource.wf", second.as_bytes())],
            limits.source,
        )
        .expect("second segment bundle");
        let second_failure = check(&second_bundle, limits, false)
            .expect_err("the later segment alone exceeds the token ceiling");
        assert_eq!(second_failure.kind(), CompilationFailureKind::Resource);

        let independent = check(&bundle, limits, false)
            .expect_err("the later resource stop replaces the earlier source rejection");
        let actual = canonical_syntax(&bundle, limits, false)
            .expect_err("the optimized whole-source path preserves segment failure precedence");
        for failure in [&independent, &actual] {
            assert_eq!(failure.kind(), CompilationFailureKind::Resource);
            assert_eq!(failure.diagnostics().count(), 1);
            assert_eq!(failure.rule_id(), None);
            assert_eq!(failure.record.expected_terminals(), None);
        }
        assert_eq!(
            actual.render(DiagnosticFormat::Json),
            independent.render(DiagnosticFormat::Json)
        );
    }

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
