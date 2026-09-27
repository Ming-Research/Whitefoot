use crate::syntax::grammar::Production;
use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};
use crate::{ByteOffset, SourceId, SourceInput};

use crate::syntax::parser::tree::DerivationExtent;
use crate::syntax::parser::{DerivationElement, ParsedBundle};

use super::super::{
    FinalizeCompilerFailure, FinalizeLimit, FinalizeLimits, FinalizeOutcome,
    FinalizeResourceFailure, finalize,
};
use super::support::{FINALIZE_LIMITS, source_offsets, with_parsed};

#[test]
fn one_finalizer_proves_bundle_root_counts_and_ordered_source_extents() {
    let inputs = [
        SourceInput::new("empty.wf", b"\n"),
        SourceInput::new("main.wf", b"fn main() -> status: ExitStatus pure {\n}\n"),
        SourceInput::new("constant.wf", b"const answer: i32 = 42_i32;\n"),
    ];
    let source_lengths = [1_u64, 41, 28];
    with_parsed(&inputs, |parsed| {
        let token_count = parsed.terminal_count();
        let production_count = parsed.production_count();
        let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
            panic!("complete derivation must finalize");
        };
        assert_eq!(finalized.terminal_count(), token_count as usize);
        assert_eq!(finalized.node_count(), production_count as usize);
        assert_eq!(finalized.root_extent().len(), 3);
        for (ordinal, extent) in finalized.root_extent().iter().enumerate() {
            assert_eq!(
                extent.source(),
                SourceId::from_ordinal(u32::try_from(ordinal).unwrap_or(u32::MAX))
            );
            assert_eq!(extent.start(), ByteOffset::new(0));
            assert_eq!(extent.end().value(), source_lengths[ordinal]);
        }
    });
}

fn assert_mutant(
    source: &[u8],
    mutate: impl FnOnce(&mut ParsedBundle),
    expected: FinalizeCompilerFailure,
) {
    let inputs = [SourceInput::new("mutant.wf", source)];
    with_parsed(&inputs, |mut parsed| {
        mutate(&mut parsed);
        let outcome = finalize(parsed, FINALIZE_LIMITS);
        assert!(
            matches!(outcome, FinalizeOutcome::CompilerFailure(actual) if actual == expected),
            "unexpected mutant result: {outcome:?}"
        );
    });
}

#[test]
fn inconsistent_postorder_root_shape_and_extent_are_rejected() {
    let source = b"fn main() -> status: ExitStatus pure {\n}\n";
    assert_mutant(
        source,
        |parsed| {
            let Some(DerivationElement::Production { production, .. }) =
                parsed.tree.elements.last_mut()
            else {
                panic!("program root must be last");
            };
            *production = Production::Item;
        },
        FinalizeCompilerFailure::InvalidSourceExtent,
    );
    assert_mutant(
        source,
        |parsed| {
            let Some(DerivationElement::Production {
                subtree_elements, ..
            }) = parsed.tree.elements.last_mut()
            else {
                panic!("program root must be last");
            };
            *subtree_elements = subtree_elements.saturating_add(1);
        },
        FinalizeCompilerFailure::InvalidPostorder,
    );
    assert_mutant(
        source,
        |parsed| {
            let Some(DerivationElement::Production { extent, .. }) =
                parsed.tree.elements.iter_mut().find(|element| {
                    matches!(
                        element,
                        DerivationElement::Production {
                            production: Production::FnDecl,
                            ..
                        }
                    )
                })
            else {
                panic!("fixture must contain fn_decl");
            };
            *extent = DerivationExtent::Source {
                source: SourceId::from_ordinal(0),
                start: ByteOffset::new(0),
                end: ByteOffset::new(1),
            };
        },
        FinalizeCompilerFailure::InvalidSourceExtent,
    );
    assert_mutant(
        source,
        |parsed| {
            parsed.tree.production_count = parsed.tree.production_count.saturating_add(1);
        },
        FinalizeCompilerFailure::CountDisagreement,
    );
}

#[test]
fn inconsistent_token_identity_and_predicate_are_rejected() {
    let source = b"fn probe() -> result: unit pure {\n  return unit;\n}\n";
    assert_mutant(
        source,
        |parsed| {
            let mut first = None;
            for element in &mut parsed.tree.elements {
                if let DerivationElement::Terminal { token, .. } = element {
                    if let Some(replacement) = first {
                        *token = replacement;
                        return;
                    }
                    first = Some(*token);
                }
            }
            panic!("fixture must have two terminal leaves");
        },
        FinalizeCompilerFailure::InvalidTokenCoverage,
    );
    assert_mutant(
        source,
        |parsed| {
            let Some(DerivationElement::Terminal { predicate, .. }) =
                parsed.tree.elements.iter_mut().find(|element| {
                    matches!(
                        element,
                        DerivationElement::Terminal {
                            predicate: TerminalPredicate::Fixed(FixedTerminal::Fn),
                            ..
                        }
                    )
                })
            else {
                panic!("fixture must contain fn terminal");
            };
            *predicate = TerminalPredicate::TypeIdentifier;
        },
        FinalizeCompilerFailure::InvalidTerminalPredicate,
    );
    assert_mutant(
        source,
        |parsed| {
            let Some(DerivationElement::Terminal { predicate, .. }) =
                parsed.tree.elements.iter_mut().find(|element| {
                    matches!(
                        element,
                        DerivationElement::Terminal {
                            predicate: TerminalPredicate::Fixed(FixedTerminal::Unit),
                            ..
                        }
                    )
                })
            else {
                panic!("fixture must contain unit terminal");
            };
            *predicate = TerminalPredicate::Literal;
        },
        FinalizeCompilerFailure::InvalidProductionShape,
    );
}

#[test]
fn exact_finalizer_resource_families_are_observable() {
    let source = b"fn main() -> status: ExitStatus pure {\n}\n";
    let cases = [
        (
            FinalizeLimit::Work,
            FinalizeLimits {
                max_work: 0,
                ..FINALIZE_LIMITS
            },
        ),
        (
            FinalizeLimit::Roots,
            FinalizeLimits {
                max_roots: 0,
                ..FINALIZE_LIMITS
            },
        ),
        (
            FinalizeLimit::ShapeTasks,
            FinalizeLimits {
                max_shape_tasks: 0,
                ..FINALIZE_LIMITS
            },
        ),
        (
            FinalizeLimit::Nodes,
            FinalizeLimits {
                max_nodes: 0,
                ..FINALIZE_LIMITS
            },
        ),
        (
            FinalizeLimit::ChildEdges,
            FinalizeLimits {
                max_child_edges: 0,
                ..FINALIZE_LIMITS
            },
        ),
        (
            FinalizeLimit::Terminals,
            FinalizeLimits {
                max_terminals: 0,
                ..FINALIZE_LIMITS
            },
        ),
        (
            FinalizeLimit::Sources,
            FinalizeLimits {
                max_sources: 0,
                ..FINALIZE_LIMITS
            },
        ),
    ];
    for (expected, limits) in cases {
        let inputs = [SourceInput::new("resource.wf", source)];
        with_parsed(&inputs, |parsed| {
            let outcome = finalize(parsed, limits);
            assert!(
                matches!(
                    outcome,
                    FinalizeOutcome::ResourceFailure(
                        FinalizeResourceFailure::LimitExceeded { limit, .. }
                    ) if limit == expected
                ),
                "unexpected {expected:?} result: {outcome:?}"
            );
        });
    }
}

#[test]
fn finalized_token_partitions_match_the_classified_source_offsets() {
    let inputs = [
        SourceInput::new("one.wf", b"\n"),
        SourceInput::new("two.wf", b"const a: i32 = 1_i32;\n"),
        SourceInput::new("three.wf", b"const b: i32 = 2_i32;\n"),
    ];
    with_parsed(&inputs, |parsed| {
        let expected = source_offsets(parsed.classified_bundle());
        let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
            panic!("ordered sources must finalize");
        };
        assert_eq!(expected.first(), Some(&0));
        assert_eq!(expected.last(), Some(&finalized.terminal_count()));
    });
}
