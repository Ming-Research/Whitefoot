use crate::{SourceId, SourceInput};

use super::super::{
    CanonicalCompilerFailure, CanonicalLimit, CanonicalLimits, CanonicalLocation, CanonicalOutcome,
    CanonicalResourceFailure, FinalizeOutcome, audit_canonical, finalize,
};
use super::support::{
    CANONICAL_LIMITS, FINALIZE_LIMITS, reaches_canonical_syntax, rendered_bytes, with_parsed,
};

fn audit_source(source: &[u8], audit: impl FnOnce(CanonicalOutcome)) {
    let inputs = [SourceInput::new("format.wf", source)];
    with_parsed(&inputs, |parsed| {
        let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
            panic!("complete derivation must finalize");
        };
        audit(audit_canonical(*finalized, CANONICAL_LIMITS));
    });
}

#[test]
fn exact_empty_and_nonempty_source_forests_publish_canonical_syntax() {
    for source in [
        b"\n".as_slice(),
        b"fn main() -> status: std::process::ExitStatus pure {\n}\n".as_slice(),
        b"const first: i32 = 1_i32;\n\nconst second: i32 = 2_i32;\n".as_slice(),
    ] {
        audit_source(source, |outcome| {
            let CanonicalOutcome::Complete(unit) = outcome else {
                panic!("exact FORM-2 source must pass: {outcome:?}");
            };
            assert_eq!(unit.classified_bundle().source_bundle().len(), 1);
            assert_eq!(unit.root_extent().len(), 1);
        });
    }
}

/// [FORM-2] a `<` or `>` selected by `compare_op` belongs to neither
/// attachment set, so a comparison is spaced while a type-argument list and
/// its `::` delimiter stay compact; the attached spelling of a comparison is
/// not canonical.
#[test]
fn comparison_angles_are_spaced_while_type_application_attaches() {
    let canonical = b"fn probe(a: u64, b: u64, w: u8) -> result: unit pure {\n  let lt = a < b;\n  let ge = a >= b;\n  let wide = cvt::<u8, u32>(w);\n  invariant bound: a <= b + 1_u64;\n  return unit;\n}\n";
    assert_eq!(
        rendered_bytes(canonical).as_deref(),
        Some(canonical.as_slice())
    );
    audit_source(canonical, |outcome| {
        assert!(
            matches!(outcome, CanonicalOutcome::Complete(_)),
            "spaced comparisons and attached type application are canonical: {outcome:?}"
        );
    });
    let attached = b"fn probe(a: u64, b: u64, w: u8) -> result: unit pure {\n  let lt = a<b;\n  let ge = a >= b;\n  let wide = cvt::<u8, u32>(w);\n  invariant bound: a <= b + 1_u64;\n  return unit;\n}\n";
    assert_eq!(
        rendered_bytes(attached).as_deref(),
        Some(canonical.as_slice())
    );
    audit_source(attached, |outcome| {
        assert!(
            matches!(outcome, CanonicalOutcome::SourceIssue(_)),
            "an attached comparison is a FORM-2 rejection: {outcome:?}"
        );
    });
}

#[test]
fn ordered_canonical_sources_keep_independent_forests() {
    let inputs = [
        SourceInput::new("empty.wf", b"\n"),
        SourceInput::new("first.wf", b"const first: i32 = 1_i32;\n"),
        SourceInput::new("second.wf", b"fn second() -> result: unit pure {\n}\n"),
    ];
    with_parsed(&inputs, |parsed| {
        let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
            panic!("ordered canonical bundle must finalize");
        };
        let CanonicalOutcome::Complete(unit) = audit_canonical(*finalized, CANONICAL_LIMITS) else {
            panic!("each ordered source forest must pass independently");
        };
        assert_eq!(unit.root_extent().len(), 3);
        assert_eq!(unit.root_extent()[0].source(), SourceId::from_ordinal(0));
        assert_eq!(unit.root_extent()[1].source(), SourceId::from_ordinal(1));
        assert_eq!(unit.root_extent()[2].source(), SourceId::from_ordinal(2));
    });
}

#[test]
fn nested_blocks_arms_and_requires_follow_tree_depth() {
    let source = br#"fn guarded(value: i32) -> result: unit pure contract {
  requires value == 0_i32;
} {
  match value {
    Some(payload: item) => {
      let drift = item == payload;
    }
    None() => {
      return unit;
    }
  }
}
"#;
    audit_source(source, |outcome| {
        assert!(
            matches!(outcome, CanonicalOutcome::Complete(_)),
            "nested canonical fixture must pass: {outcome:?}"
        );
    });
}

#[test]
fn plain_and_variant_ensures_round_trip_with_clause_joins() {
    let source = br#"fn plain(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  return value;
}

fn selected(value: i32) -> result: Result<i32, i32> pure contract {
  requires value == value;
  ensures when Ok(value: payload): payload == value;
} {
  return Ok<i32, i32>(value: value);
}
"#;
    only_these_trivia_bytes_render(source);
}

#[test]
fn first_gap_mismatch_uses_exact_source_or_deepest_node_location() {
    audit_source(
        b"fn main() -> status: std::process::ExitStatus pure {}",
        |outcome| {
            let CanonicalOutcome::SourceIssue(issue) = outcome else {
                panic!("one-line block must reject: {outcome:?}");
            };
            assert_eq!(issue.rule(), crate::syntax::parser::SyntaxRule::Form2);
            let CanonicalLocation::SourceNode(path, coordinate) = issue.location() else {
                panic!("inside-item gap must use SourceNode");
            };
            assert_eq!(path.components(), &[0, 0]);
            assert_eq!(coordinate.source(), SourceId::from_ordinal(0));
            assert_eq!(coordinate.start(), coordinate.end());
        },
    );

    audit_source(
        b" fn main() -> status: std::process::ExitStatus pure {\n}\n",
        |outcome| {
            let CanonicalOutcome::SourceIssue(issue) = outcome else {
                panic!("leading trivia must reject: {outcome:?}");
            };
            let CanonicalLocation::SourceBytes(coordinate) = issue.location() else {
                panic!("source-leading gap must use SourceBytes");
            };
            assert_eq!(coordinate.start().value(), 0);
            assert_eq!(coordinate.end().value(), 1);
        },
    );

    audit_source(
        b"fn main() -> status: std::process::ExitStatus pure {\n}",
        |outcome| {
            let CanonicalOutcome::SourceIssue(issue) = outcome else {
                panic!("missing final LF must reject: {outcome:?}");
            };
            let CanonicalLocation::SourceBytes(coordinate) = issue.location() else {
                panic!("source-final gap must use SourceBytes");
            };
            assert_eq!(coordinate.start(), coordinate.end());
        },
    );

    audit_source(
        b"const first: i32 = 1_i32;\nconst second: i32 = 2_i32;\n",
        |outcome| {
            let CanonicalOutcome::SourceIssue(issue) = outcome else {
                panic!("missing inter-item blank line must reject: {outcome:?}");
            };
            assert!(matches!(
                issue.location(),
                CanonicalLocation::SourceBytes(_)
            ));
        },
    );
}

#[test]
fn zero_item_source_has_one_exact_lf_form() {
    audit_source(b"", |outcome| {
        let CanonicalOutcome::SourceIssue(issue) = outcome else {
            panic!("zero-byte source must fail FORM-2: {outcome:?}");
        };
        let CanonicalLocation::SourceBytes(coordinate) = issue.location() else {
            panic!("zero-item source mismatch must use SourceBytes");
        };
        assert_eq!(coordinate.start().value(), 0);
        assert_eq!(coordinate.end().value(), 0);
    });
}

#[test]
fn ordered_sources_stop_at_the_first_form2_mismatch() {
    let inputs = [
        SourceInput::new("first.wf", b"fn first() -> result: unit pure {\n}\n"),
        SourceInput::new("second.wf", b"fn second() -> result: unit pure {}"),
        SourceInput::new("third.wf", b"fn third() -> result: unit pure {}"),
    ];
    with_parsed(&inputs, |parsed| {
        let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
            panic!("ordered bundle must finalize");
        };
        let CanonicalOutcome::SourceIssue(issue) = audit_canonical(*finalized, CANONICAL_LIMITS)
        else {
            panic!("second source must provide first FORM-2 mismatch");
        };
        let coordinate = match issue.location() {
            CanonicalLocation::SourceBytes(coordinate)
            | CanonicalLocation::SourceNode(_, coordinate) => coordinate,
        };
        assert_eq!(coordinate.source(), SourceId::from_ordinal(1));
    });
}

#[test]
fn tree_mutation_with_the_original_tape_cannot_publish_canonical_syntax() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {\n}\n";
    let inputs = [SourceInput::new("mutated.wf", source)];
    with_parsed(&inputs, |parsed| {
        let FinalizeOutcome::Complete(mut finalized) = finalize(parsed, FINALIZE_LIMITS) else {
            panic!("fixture must finalize before the consistency edit");
        };
        let Some(node) = finalized
            .topology
            .nodes
            .iter_mut()
            .find(|node| node.production == crate::syntax::grammar::Production::FnDecl)
        else {
            panic!("fixture must contain fn_decl");
        };
        node.production = crate::syntax::grammar::Production::Item;
        assert!(matches!(
            audit_canonical(*finalized, CANONICAL_LIMITS),
            CanonicalOutcome::CompilerFailure(CanonicalCompilerFailure::InvalidFinalizedTree)
        ));
    });

    with_parsed(&inputs, |parsed| {
        let FinalizeOutcome::Complete(mut finalized) = finalize(parsed, FINALIZE_LIMITS) else {
            panic!("fixture must finalize before the consistency edit");
        };
        finalized.topology.terminals[0].local_ordinal = 1;
        assert!(matches!(
            audit_canonical(*finalized, CANONICAL_LIMITS),
            CanonicalOutcome::CompilerFailure(
                CanonicalCompilerFailure::TerminalBindingDisagreement
            )
        ));
    });
}

#[test]
fn canonical_audit_resource_edges_are_explicit_and_deterministic() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {\n}\n";
    let cases = [
        (
            CanonicalLimit::Work,
            CanonicalLimits {
                max_work: 0,
                ..CANONICAL_LIMITS
            },
        ),
        (
            CanonicalLimit::SourceBytes,
            CanonicalLimits {
                max_source_bytes: 0,
                ..CANONICAL_LIMITS
            },
        ),
        (
            CanonicalLimit::TotalSourceBytes,
            CanonicalLimits {
                max_total_source_bytes: 0,
                ..CANONICAL_LIMITS
            },
        ),
        (
            CanonicalLimit::Gaps,
            CanonicalLimits {
                max_gaps: 0,
                ..CANONICAL_LIMITS
            },
        ),
    ];
    for (expected, limits) in cases {
        let inputs = [SourceInput::new("resource.wf", source)];
        with_parsed(&inputs, |parsed| {
            let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
                panic!("resource fixture must finalize");
            };
            let outcome = audit_canonical(*finalized, limits);
            assert!(
                matches!(
                    outcome,
                    CanonicalOutcome::ResourceFailure(
                        CanonicalResourceFailure::LimitExceeded { limit, .. }
                    ) if limit == expected
                ),
                "unexpected {expected:?} result: {outcome:?}"
            );
        });
    }

    let noncanonical = b"fn main() -> status: std::process::ExitStatus pure {}";
    let inputs = [SourceInput::new("path.wf", noncanonical)];
    with_parsed(&inputs, |parsed| {
        let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
            panic!("path fixture must finalize");
        };
        let outcome = audit_canonical(
            *finalized,
            CanonicalLimits {
                max_path_components: 0,
                ..CANONICAL_LIMITS
            },
        );
        assert!(matches!(
            outcome,
            CanonicalOutcome::ResourceFailure(CanonicalResourceFailure::LimitExceeded {
                limit: CanonicalLimit::PathComponents,
                ..
            })
        ));
    });
}

/// Asserts that a source is canonical and that no single-byte trivia edit is.
///
/// The renderer is held to the same fixtures: canonical bytes must render to
/// themselves, so every layout rule pinned here is pinned in both directions.
fn only_these_trivia_bytes_render(canonical: &[u8]) {
    assert!(reaches_canonical_syntax(canonical));
    assert_eq!(rendered_bytes(canonical).as_deref(), Some(canonical));
    let trivia_positions: Vec<_> = canonical
        .iter()
        .enumerate()
        .filter_map(|(index, byte)| matches!(byte, b' ' | b'\n').then_some(index))
        .collect();
    assert!(!trivia_positions.is_empty());
    for position in trivia_positions {
        let mut removed = canonical.to_vec();
        removed.remove(position);
        assert!(
            !reaches_canonical_syntax(&removed),
            "removing canonical trivia at byte {position} unexpectedly stayed canonical: {:?}",
            String::from_utf8_lossy(&removed)
        );

        let mut duplicated = canonical.to_vec();
        duplicated.insert(position, canonical[position]);
        assert!(
            !reaches_canonical_syntax(&duplicated),
            "duplicating canonical trivia at byte {position} unexpectedly stayed canonical"
        );

        let mut replaced = canonical.to_vec();
        replaced[position] = if canonical[position] == b' ' {
            b'\n'
        } else {
            b' '
        };
        assert!(
            !reaches_canonical_syntax(&replaced),
            "replacing canonical trivia at byte {position} unexpectedly stayed canonical"
        );

        // Whatever a mutation derives, rendering it lands on canonical bytes
        // and stays there. A mutation that keeps the token stream renders back
        // to `canonical`; one that changes it renders that other program. The
        // fixed point is what holds for both, so it is what is asserted.
        for mutation in [&removed, &duplicated, &replaced] {
            let Some(rendered) = rendered_bytes(mutation) else {
                continue;
            };
            assert!(reaches_canonical_syntax(&rendered));
            assert_eq!(rendered_bytes(&rendered).as_ref(), Some(&rendered));
        }
    }
}

#[test]
fn generated_trivia_mutations_never_bypass_the_exact_forest_renderer() {
    only_these_trivia_bytes_render(b"const first: i32 = 1_i32;\n\nfn main() -> status: std::process::ExitStatus pure {\n  let value = 2_i32;\n  return unit;\n}\n");
}

/// Ordinary typed parameters use the same canonical function header.
const ORDINARY_HEADER_HEADER: &[u8] = b"fn main(args: std::text::Args, cwd: std::fs::DirectoryRead, out: std::io::OutputStream, err: std::io::OutputStream, files: std::io::HandleFactory) -> status: std::process::ExitStatus writes(cwd) {";

const ORDINARY_HEADER: &[u8] = b"fn main(args: std::text::Args, cwd: std::fs::DirectoryRead, out: std::io::OutputStream, err: std::io::OutputStream, files: std::io::HandleFactory) -> status: std::process::ExitStatus writes(cwd) {\n  return unit;\n}\n";

#[test]
fn ordinary_parameter_headers_follow_form2() {
    // C2 removes program_kind and input_label; ordinary parameters retain
    // FORM-2 exact trivia, including the negative mutation sweep.
    assert!(ORDINARY_HEADER.starts_with(ORDINARY_HEADER_HEADER));
    only_these_trivia_bytes_render(ORDINARY_HEADER);
}

/// The property the corpus migration rests on: a transform may produce any
/// layout it likes as long as the bytes parse, because rendering makes them
/// canonical. Each pair below holds the token stream fixed and varies only
/// trivia, so the rendered bytes must be the canonical spelling of that same
/// program rather than some other one.
#[test]
fn rendering_normalizes_any_parseable_layout_onto_canonical_bytes() {
    for (sloppy, canonical) in [
        // No trivia at all where FORM-2 requires a break.
        (
            b"fn main() -> status: std::process::ExitStatus pure {}".as_slice(),
            b"fn main() -> status: std::process::ExitStatus pure {\n}\n".as_slice(),
        ),
        // Leading trivia, which no canonical source carries.
        (
            b" fn main() -> status: std::process::ExitStatus pure {\n}\n".as_slice(),
            b"fn main() -> status: std::process::ExitStatus pure {\n}\n".as_slice(),
        ),
        // A missing final newline.
        (
            b"fn main() -> status: std::process::ExitStatus pure {\n}".as_slice(),
            b"fn main() -> status: std::process::ExitStatus pure {\n}\n".as_slice(),
        ),
        // Wrong indentation and a run of blank lines inside a body.
        (
            b"fn main() -> status: std::process::ExitStatus pure {\n\n\n        let value = 2_i32;\n   return std::process::exit_status(code: 0_u8);\n}\n"
                .as_slice(),
            b"fn main() -> status: std::process::ExitStatus pure {\n  let value = 2_i32;\n  return std::process::exit_status(code: 0_u8);\n}\n".as_slice(),
        ),
        // Two top-level items run together; FORM-2 separates them by a blank
        // line, which no amount of local spacing repair would supply.
        (
            b"const first: i32 = 1_i32;\nconst second: i32 = 2_i32;\n".as_slice(),
            b"const first: i32 = 1_i32;\n\nconst second: i32 = 2_i32;\n".as_slice(),
        ),
        // The join line, emitted rather than recognized. A migration writing
        // `if`/`else` from a `match` produces the close and the `else` with no
        // idea they share a line; the renderer is what puts them there.
        (
            b"fn main() -> status: std::process::ExitStatus pure {\nlet flag = True();\nif flag {\nlet then_value = flag;\n}\nelse\n{\nlet else_value = flag;\n}\nreturn unit;\n}\n".as_slice(),
            b"fn main() -> status: std::process::ExitStatus pure {\n  let flag = True();\n  if flag {\n    let then_value = flag;\n  } else {\n    let else_value = flag;\n  }\n  return unit;\n}\n".as_slice(),
        ),
        // A flattened `else if` chain, likewise joined by the renderer.
        (
            b"fn main() -> status: std::process::ExitStatus pure {\nlet flag = True();\nif flag {\nlet first = flag;\n} else if flag {\nlet second = flag;\n} else {\nlet third = flag;\n}\nreturn unit;\n}\n".as_slice(),
            b"fn main() -> status: std::process::ExitStatus pure {\n  let flag = True();\n  if flag {\n    let first = flag;\n  } else if flag {\n    let second = flag;\n  } else {\n    let third = flag;\n  }\n  return unit;\n}\n".as_slice(),
        ),
    ] {
        assert!(!reaches_canonical_syntax(sloppy));
        assert_eq!(rendered_bytes(sloppy).as_deref(), Some(canonical));
        assert!(reaches_canonical_syntax(canonical));
    }
}

/// A source holding no item is one newline, from the emitter side too.
#[test]
fn an_item_free_source_renders_as_one_newline() {
    assert_eq!(rendered_bytes(b"").as_deref(), Some(b"\n".as_slice()));
    assert_eq!(rendered_bytes(b"\n\n\n").as_deref(), Some(b"\n".as_slice()));
}

/// FORM-2's join line. `if_stmt` and `value_if` are the only productions
/// owning two brace blocks, and the close-and-open line `} else {` is a
/// rendering no v0.22 production produced. The `requires` block's `} {` is
/// the precedent both follow.
#[test]
fn if_else_renders_its_join_line_and_indents_both_blocks() {
    // An else-free `if`: one block, ordinary break after the close.
    only_these_trivia_bytes_render(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let flag = True();\n  if flag {\n    let then_value = flag;\n  }\n  return unit;\n}\n",
    );
    // A braced `else`: two blocks joined by `} else {` on one line.
    only_these_trivia_bytes_render(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let flag = True();\n  if flag {\n    let then_value = flag;\n  } else {\n    let else_value = flag;\n  }\n  return unit;\n}\n",
    );
    // An `else if` chain: the nested `if_stmt` owns the second block, so the
    // outer node has one pair plus an `else`, and still suppresses its break.
    only_these_trivia_bytes_render(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let flag = True();\n  if flag {\n    let first = flag;\n  } else if flag {\n    let second = flag;\n  } else {\n    let third = flag;\n  }\n  return unit;\n}\n",
    );
    // A `value_if` initializer delivers from both branches.
    only_these_trivia_bytes_render(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let flag = True();\n  let picked = if flag {\n    give 1_i32;\n  } else {\n    give 2_i32;\n  }\n  return unit;\n}\n",
    );
    // A three-deep chain renders flat: every arm sits at one indent level.
    // This is structural, not a special case. An else-position `if_stmt`
    // begins after the then-block's closing brace, so it is inside no brace
    // pair of its parent and `inside_body` leaves it at the outer format
    // depth. Nothing in the depth computation mentions `if_stmt`, and
    // `has_else` is read only when suppressing the break after a close
    // brace. Do not add a special case here: depth would then accumulate and
    // this fixture would indent each arm one level deeper.
    only_these_trivia_bytes_render(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let flag = True();\n  if flag {\n    let first = flag;\n  } else if flag {\n    let second = flag;\n  } else if flag {\n    let third = flag;\n  } else {\n    let fourth = flag;\n  }\n  return unit;\n}\n",
    );
}

#[test]
fn counted_range_attaches_its_endpoints_and_round_trips_canonically() {
    // `(` remains in FORM-2's general right-attachment set, so the affine
    // parenthesized factor is `*(...)`, with no proof-syntax exception.
    let canonical = b"fn probe(lower: u64, upper: u64) -> result: unit pure {\n  for @range (\n    index in lower..upper,\n    invariant limit: index + 1_u64 *(1_u64) <= upper\n  ) {\n    break @range;\n  }\n  return unit;\n}\n";
    only_these_trivia_bytes_render(canonical);
    let sloppy = b"fn probe(lower:u64,upper:u64)->result:unit pure{\nfor @range( index in lower .. upper,invariant limit : index+1_u64 * ( 1_u64 ) <= upper){\nbreak @range;\n}\nreturn unit;\n}\n";
    assert_eq!(
        rendered_bytes(sloppy).as_deref(),
        Some(canonical.as_slice())
    );
    assert_eq!(
        rendered_bytes(canonical).as_deref(),
        Some(canonical.as_slice())
    );
}

#[test]
fn a_counted_header_without_an_invariant_stays_on_one_line() {
    // FORM-2 breaks a loop header apart only to set its `header_invariant`
    // clauses on their own lines. A counted `for` whose header is nothing but
    // its binding has no invariant to set apart, so the whole header stays on
    // one line and the three-line spelling of that same header is rejected.
    let canonical = b"fn probe(lower: u64, upper: u64) -> result: unit pure {\n  for @range (index in lower..upper) {\n    break @range;\n  }\n  for (other in lower..upper) {\n  }\n  return unit;\n}\n";
    only_these_trivia_bytes_render(canonical);
    let sloppy = b"fn probe(lower:u64,upper:u64)->result:unit pure{\nfor @range(\n  index in lower..upper\n){\nbreak @range;\n}\nfor( other in lower .. upper ){}\nreturn unit;\n}\n";
    assert_eq!(
        rendered_bytes(sloppy).as_deref(),
        Some(canonical.as_slice())
    );
    assert_eq!(
        rendered_bytes(canonical).as_deref(),
        Some(canonical.as_slice())
    );
}

#[test]
fn local_invariant_certificates_render_their_header_and_steps_canonically() {
    let canonical = b"fn probe(left: i32, right: i32) -> result: unit pure {\n  invariant ordered: left + 1_i32 <= right + 1_i32 {\n    use 2 times (left <= right);\n    use earlier;\n  }\n  return unit;\n}\n";
    only_these_trivia_bytes_render(canonical);
    let sloppy = b"fn probe(left:i32,right:i32)->result:unit pure{ invariant ordered : left+1_i32 <= right+1_i32 { use 2 times ( left <= right ); use earlier; } return unit; }";
    assert_eq!(
        rendered_bytes(sloppy).as_deref(),
        Some(canonical.as_slice())
    );
    assert_eq!(
        rendered_bytes(canonical).as_deref(),
        Some(canonical.as_slice())
    );
}

/// The forms v0.60 adds render, and the one new stated space is emitted.
///
/// [FORM-2] makes `heap_decl` a line-bearing item, so `program no_heap;` takes
/// its own line and the blank line every top-level item pair gets. The range
/// step, the payload step and `&` are all attachment members, so `&run[lo..hi]`,
/// `node.Some.value`, `&deref(node).left` and `part: &[Int]` render compactly
/// with no sentence of their own. The single exception is a destructuring
/// consume's rest marker, which takes exactly one space after its preceding
/// `,` — so `, ..` is canonical and `,..` is not, while `Leaf(..)` keeps the
/// left attachment of `(` and takes no space at all.
#[test]
fn the_forms_the_amendment_adds_render_canonically() {
    let canonical = b"program no_heap;\n\nfn probe(part: &[Int], node: &Tree, run: &Slots<Int, 4>, lo: u64, hi: u64) -> result: unit pure {\n  let whole = &run[lo..hi];\n  let through = &deref(node).left;\n  let payload = node.Some.value;\n  let slice_read = part[lo];\n  let Tree(left: kept, ..) = move taken;\n  let Leaf(..) = move spare;\n  return unit;\n}\n";
    only_these_trivia_bytes_render(canonical);
    let sloppy = b"program  no_heap ;\nfn probe(part:&[Int],node:&Tree,run:&Slots<Int,4>,lo:u64,hi:u64)->result:unit pure{\nlet whole = & run [ lo .. hi ];\nlet through = & deref ( node ) . left;\nlet payload = node . Some . value;\nlet slice_read = part [ lo ];\nlet Tree(left: kept,..) = move taken;\nlet Leaf( .. ) = move spare;\nreturn unit;\n}\n";
    assert!(!reaches_canonical_syntax(sloppy));
    assert_eq!(
        rendered_bytes(sloppy).as_deref(),
        Some(canonical.as_slice())
    );
    assert_eq!(
        rendered_bytes(canonical).as_deref(),
        Some(canonical.as_slice())
    );
}

#[test]
fn ordinary_loop_header_invariants_use_the_same_multiline_layout() {
    let canonical = b"fn probe(value: i32) -> result: unit pure {\n  loop @again (\n    invariant stable: value <= value\n  ) {\n    break @again;\n  }\n  return unit;\n}\n";
    only_these_trivia_bytes_render(canonical);
    let sloppy = b"fn probe(value:i32)->result:unit pure{ loop @again(invariant stable : value <= value){ break @again; } return unit; }";
    assert_eq!(
        rendered_bytes(sloppy).as_deref(),
        Some(canonical.as_slice())
    );
}
