#![allow(clippy::panic)]

// Five suites of this module retired whole with the v0.60 amendment, because
// the mechanism each one tested no longer exists. Each names its successor:
//
// - `borrows` (33 tests) retired with [OWN-2], [OWN-5], [OWN-6], [OWN-9],
//   [OWN-12] and [OWN-14]: there are no borrow modes, loans, holders, child
//   reborrows or suspension. Successor: `references`, over [REF-1] paths,
//   [REF-2] validity, [REF-3] escape and [EFF-5]'s pairwise call check.
// - `slices` (30 tests) retired with [VIEW-1], [VIEW-2], [VIEW-4] and
//   [VIEW-6]: slices and views are not values, and `Slice<T>`/`MutSlice<T>`
//   are not types. Successor: `range_references`, over [REF-4]'s `&x[lo..hi]`
//   and the `&[T]` parameter kind, with [CALL-3]'s transport.
// - `buffers` (22 tests) retired with [BLK-1] through [BLK-4], [PROV-1] and
//   the `buffer` / `FixedVector` / `Vector` storage names. Successor:
//   `windows`, over [TYPE-9]'s shapes, [WIN-1] through [WIN-3] and the
//   [PRE-1] window, construction and release records [OP-10, OP-13, OP-14].
// - `replace` (14 tests) retired with [SET-2]: `let x = replace p = e;` has no
//   v0.60 production, exchange being the built-in `swap` [OP-11] and
//   replacement the ordinary `set` whose old value takes [WIN-3]'s
//   disposition. Successor: the [SET-1] tests in this file, `owned_places`,
//   and the `swap` tests in `windows`. [OP-12]'s atomic in-place update
//   `set p = f(move p, args...);` has alias-path coverage in `references`
//   and formal-result boundary coverage in `contracts`.
// - `arenas` (15 tests) retired with [OWN-3], [OWN-4], [OWN-10], [FORM-8] and
//   [STOR-4]: v0.60 has no regions, lifetimes, arenas or store brands at all.
//   There is no successor rule; a pool or an arena is ordinary `Slots` usage
//   under [OP-13] over the one heap [STOR-8].
mod arithmetic_obligations;
mod arrays;
mod boolean_composition;
mod cells;
mod checked_division;
mod conditionals;
mod const_eval;
mod contracts;
mod counted_ranges;
mod derivation;
mod descriptor_invalidation;
mod division_obligations;
mod entailment;
mod entailment_sources;
mod entry_form;
mod exclusive_contracts;
mod float_conversion;
mod floating;
mod generics;
mod infix;
mod integer_absolute;
mod integer_conversion;
mod integer_extended;
mod integer_negation;
mod loop_invariants;
mod loop_permission;
mod operation_table;
mod options;
mod ordinary_effects;
mod originating_acceptance;
mod owned_places;
mod permission;
mod postconditions;
mod range_references;
mod references;
mod reinterpret;
mod requires;
mod source_proofs;
mod windows;

use crate::lexer::{LexLimits, LexOutcome, lex};
use crate::{
    ACTIVE_KERNEL_SPEC_HASH, ACTIVE_KERNEL_SPEC_TEXT, CanonicalLimits, CanonicalOutcome,
    FinalizeLimits, FinalizeOutcome, ParseLimits, ParseOutcome, ResolutionOutcome,
    SemanticIssueKind, SemanticLocation, SemanticOutcome, SemanticRule, SourceBundle, SourceInput,
    SourceLimits, TerminalLimits, TerminalOutcome, UnsupportedSemanticFeature, audit_canonical,
    check_semantics, classify_terminals, finalize, parse, resolve,
};

use super::model::{CheckedExpression, CheckedStatement};

// [PRE-1] the prelude contributes one source per declaration record, so a
// harness bundle is that record count plus the test's own sources; the
// ceiling is the driver's [`crate::driver`] rather than a number the record
// list can grow past.
const SOURCE_LIMITS: SourceLimits = SourceLimits {
    max_sources: 1_024,
    max_logical_path_bytes: 128,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_binding_bytes: 1_048_576,
};

const LEX_LIMITS: LexLimits = LexLimits {
    max_sources: 1_024,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_token_bytes: 16_384,
    max_tokens: 131_072,
    max_lexemes: 262_144,
};

// These are the harness's own resource ceilings, well below the driver's
// [`crate::driver`], and they bound how large a semantic-test source may be
// rather than what the language admits. [GRAM-4]'s `affine_factor` now reaches
// its IDENT through an `atom` and a `place` [MSR-5], which is three derivation
// elements per affine atom where it was one, and the certificate-capacity
// fixture writes four thousand `use` steps over two atoms each. The parse and
// finalize ceilings rise so that fixture still reaches the checker; no
// judgment and no language rule changes with them.
const PARSE_LIMITS: ParseLimits = ParseLimits {
    max_work: 32_000_000,
    max_tasks: 524_288,
    max_frames: 8_192,
    max_elements: 524_288,
};

const FINALIZE_LIMITS: FinalizeLimits = FinalizeLimits {
    max_work: 32_000_000,
    max_roots: 262_144,
    max_shape_tasks: 262_144,
    max_nodes: 262_144,
    max_child_edges: 262_144,
    max_terminals: 131_072,
    max_sources: 1_024,
};

const CANONICAL_LIMITS: CanonicalLimits = CanonicalLimits {
    max_work: 8_000_000,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_gaps: 131_072,
    max_path_components: 8_192,
};

fn with_semantics<ResultValue>(
    source: &[u8],
    run: impl FnOnce(SemanticOutcome) -> ResultValue,
) -> ResultValue {
    let inputs = [SourceInput::new("test.wf", source)];
    with_semantics_inputs(&inputs, run)
}

/// Runs the ordinary front end through canonicalization and exposes the
/// resolver outcome directly. Contract definitions now precede every
/// postcondition in one block, so lookup failures in those definitions are
/// resolver-owned rather than delayed semantic entry failures.
fn with_resolution<ResultValue>(
    source: &[u8],
    run: impl FnOnce(ResolutionOutcome) -> ResultValue,
) -> ResultValue {
    let inputs = [SourceInput::new("test.wf", source)];
    let Ok(bundle) = SourceBundle::with_limits(&inputs, SOURCE_LIMITS) else {
        panic!("resolution test bundle must be valid");
    };
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("resolution test source must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) else {
        panic!("resolution test source must classify");
    };
    let ParseOutcome::Complete(parsed) = parse(classified, PARSE_LIMITS) else {
        panic!("resolution test source must parse");
    };
    let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
        panic!("resolution test derivation must finalize");
    };
    let CanonicalOutcome::Complete(canonical) = audit_canonical(*finalized, CANONICAL_LIMITS)
    else {
        panic!("resolution test source must be canonical");
    };
    run(resolve(canonical))
}

/// Asserts that one source is refused at the parse stage citing one rule.
///
/// A grammar the tables cannot derive is refused before the checker sees it,
/// and [DIAG-1] cites the production's own rule there. This is the assertion
/// for a source whose defect the grammar itself decides.
fn assert_parse_rule(source: &[u8], rule: crate::SyntaxRule) {
    let Ok(bundle) =
        SourceBundle::with_limits(&[SourceInput::new("parse.wf", source)], SOURCE_LIMITS)
    else {
        panic!("parse test bundle must be valid");
    };
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("parse test source must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) else {
        panic!("parse test source must classify");
    };
    let outcome = parse(classified, PARSE_LIMITS);
    let ParseOutcome::SourceIssue(issue) = outcome else {
        panic!("parse test source must be refused at the parse stage");
    };
    assert_eq!(issue.rule(), rule);
}

fn with_semantics_inputs<ResultValue>(
    inputs: &[SourceInput<'_>],
    run: impl FnOnce(SemanticOutcome) -> ResultValue,
) -> ResultValue {
    with_resolved_semantics_inputs(inputs, |_, outcome| run(outcome))
}

/// [`with_semantics`] that also lends the resolved unit the program was
/// checked over, for a test that reads resolution records beside the checked
/// program.
fn with_resolved_semantics<ResultValue>(
    source: &[u8],
    run: impl FnOnce(&crate::ResolvedSyntaxUnit, SemanticOutcome) -> ResultValue,
) -> ResultValue {
    let inputs = [SourceInput::new("test.wf", source)];
    with_resolved_semantics_inputs(&inputs, run)
}

fn with_resolved_semantics_inputs<ResultValue>(
    inputs: &[SourceInput<'_>],
    run: impl FnOnce(&crate::ResolvedSyntaxUnit, SemanticOutcome) -> ResultValue,
) -> ResultValue {
    let Ok(bundle) = SourceBundle::with_prelude(inputs, SOURCE_LIMITS) else {
        panic!("semantic test bundle must be valid");
    };
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("semantic test source must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) else {
        panic!("semantic test source must classify");
    };
    let ParseOutcome::Complete(parsed) = parse(classified, PARSE_LIMITS) else {
        panic!("semantic test source must parse");
    };
    let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
        panic!("semantic test derivation must finalize");
    };
    let canonical = audit_canonical(*finalized, CANONICAL_LIMITS);
    let CanonicalOutcome::Complete(canonical) = canonical else {
        panic!("semantic test source must be canonical: {canonical:?}");
    };
    let outcome = resolve(canonical);
    let ResolutionOutcome::Complete(resolved) = outcome else {
        panic!("semantic test source must resolve: {outcome:?}");
    };
    let checked =
        crate::native_test_support::timed("semantic-check", || check_semantics(&resolved));
    crate::native_test_support::timed("semantic-test-assertions", || run(&resolved, checked))
}

/// [`with_semantics`] through the test-only dark checker, which retains every
/// [ENT-6] obligation instead of rejecting at the first one, so engine unit
/// tests can observe complete per-function derivations.
fn with_semantics_dark<ResultValue>(
    source: &[u8],
    run: impl FnOnce(SemanticOutcome) -> ResultValue,
) -> ResultValue {
    let inputs = [SourceInput::new("test.wf", source)];
    let Ok(bundle) = SourceBundle::with_prelude(&inputs, SOURCE_LIMITS) else {
        panic!("semantic test bundle must be valid");
    };
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("semantic test source must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) else {
        panic!("semantic test source must classify");
    };
    let ParseOutcome::Complete(parsed) = parse(classified, PARSE_LIMITS) else {
        panic!("semantic test source must parse");
    };
    let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
        panic!("semantic test derivation must finalize");
    };
    let canonical = audit_canonical(*finalized, CANONICAL_LIMITS);
    let CanonicalOutcome::Complete(canonical) = canonical else {
        panic!("semantic test source must be canonical: {canonical:?}");
    };
    let ResolutionOutcome::Complete(resolved) = resolve(canonical) else {
        panic!("semantic test source must resolve");
    };
    run(super::check::check_semantics_dark(&resolved))
}

fn assert_rule(source: &[u8], rule: SemanticRule, kind: SemanticIssueKind) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected {rule:?}/{kind:?}, got {outcome:?}");
        };
        assert_eq!(issue.rule(), rule);
        assert_eq!(issue.kind(), &kind);
    });
}

/// Checks a module program from its graph and its records, each placed in
/// the module its logical path's directory names and in the interface role
/// when it is that directory's `module.wfm` [MOD-1, MOD-2].
fn check_module_sources(
    graph: &[u8],
    records: &[(&str, &[u8])],
) -> Result<(), crate::CompilationFailure> {
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", graph),
        crate::CompilerLimits::default(),
    )?;
    let inputs = records
        .iter()
        .map(|(path, bytes)| {
            let (directory, file) = path.rsplit_once('/').unwrap_or(("", path));
            let components: Vec<String> = if directory.is_empty() {
                Vec::new()
            } else {
                directory.split('/').map(str::to_owned).collect()
            };
            let module = graph
                .modules()
                .iter()
                .position(|module| module.path() == components.as_slice())
                .and_then(crate::ModuleId::from_index)
                .expect("every record lies in a registered module");
            let role = if file == "module.wfm" {
                crate::SourceRole::Interface
            } else {
                crate::SourceRole::Implementation
            };
            SourceInput::new(path, bytes).in_module(module, role)
        })
        .collect::<Vec<_>>();
    crate::check_module_program(&graph, &inputs, crate::CompilerLimits::default())
}

/// Checks one module-form conformance case directory through the ordinary
/// graph formation and record discovery [MOD-1, MOD-2].
fn check_case_directory(case: &str) -> Result<(), crate::CompilationFailure> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/conformance/cases")
        .join(case);
    let graph_bytes = std::fs::read(root.join("modules.wfg")).expect("read the case's graph");
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", &graph_bytes),
        crate::CompilerLimits::default(),
    )?;
    let sources = crate::discover_module_sources(&root, &graph).expect("read the case's records");
    let inputs = sources
        .iter()
        .map(|source| {
            SourceInput::new(&source.logical_path, &source.bytes)
                .in_module(source.module, source.role)
        })
        .collect::<Vec<_>>();
    crate::check_module_program(&graph, &inputs, crate::CompilerLimits::default())
}

/// Asserts a rejection's rule and which issue kind it cited, without pinning a
/// payload the call site does not state.
///
/// These call sites predate the payloads batch 0100 gave `TypeMismatch`,
/// `EffectMismatch`, `InvalidEffectRow`, and `InvalidBorrowLifetime`, and each
/// asserts here exactly what it asserted when those kinds were unit variants:
/// which rule rejected, and which kind it cited. Nothing was narrowed. The
/// payload text those kinds carry is pinned by
/// `driver::pinned_sentences`, one row per sentence, which is where a change
/// to the wording has to be made deliberately.
fn assert_rule_kind(source: &[u8], rule: SemanticRule, kind: fn(&SemanticIssueKind) -> bool) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected {rule:?} with a matching kind, got {outcome:?}");
        };
        assert_eq!(issue.rule(), rule);
        assert!(kind(issue.kind()), "unexpected kind {:?}", issue.kind());
    });
}

/// Asserts a rejection and the exact source bytes it cites, for the rules
/// that name *which* operand or node they land on.
/// Asserts that one complete source is accepted by every whole-unit judgment.
fn assert_accepts(source: &[u8]) {
    with_semantics(source, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "expected an accepted program, got {outcome:?}"
        );
    });
}

fn assert_rule_at(source: &[u8], rule: SemanticRule, cited: &str) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected {rule:?} at {cited:?}, got {outcome:?}");
        };
        assert_eq!(issue.rule(), rule);
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("offset fits");
        let end = usize::try_from(coordinate.end().value()).expect("offset fits");
        let actual = std::str::from_utf8(&source[start..end]).expect("cited bytes must be text");
        assert_eq!(actual, cited, "citation landed on the wrong node");
    });
}

fn assert_unsupported(source: &[u8], feature: UnsupportedSemanticFeature) {
    with_semantics(source, |outcome| {
        let SemanticOutcome::Unsupported { unsupported, .. } = outcome else {
            panic!("expected unsupported {feature:?}, got {outcome:?}");
        };
        assert_eq!(unsupported.feature(), feature);
    });
}

#[test]
fn repeated_normalized_uses_are_a_prf1_rejection() {
    let source = br#"fn combine(value: u64, limit: u64) -> result: unit pure contract {
  requires value <= limit;
} {
  invariant scaled: 3_u64 * value <= 3_u64 * limit {
    use (value <= limit);
    use (value <= limit);
    use (value <= limit);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Prf1, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedSourceProof { .. })
    });
}

#[test]
fn a_local_invariant_equality_is_an_inv1_target() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  invariant held: 0_u64 == 0_u64;
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_accepts(source);
}

#[test]
fn an_unproved_blockless_local_invariant_target_is_an_inv1_rejection() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  invariant impossible: 1_u64 <= 0_u64;
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Inv1, |kind| {
        matches!(
            kind,
            SemanticIssueKind::UndischargedLocalInvariant { name, .. }
                if name == "impossible"
        )
    });
}

#[test]
fn a_non_ordered_use_relation_is_a_prf1_rejection() {
    let source = br#"fn check(value: u64, limit: u64) -> result: unit pure {
  invariant scaled: 2_u64 * value <= 2_u64 * limit {
    use (value == limit);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(source, SemanticRule::Prf1, |kind| {
        matches!(kind, SemanticIssueKind::InvalidSourceProof { .. })
    });
}

#[test]
fn scalar_constants_calls_and_operations_publish_one_checked_program() {
    let source = br#"const base: i32 = 40_i32;

fn add(x: i32, y: i32) -> result: i32 pure {
  return x +wrap y;
}

fn main() -> status: std::process::ExitStatus pure {
  let result = add(x: base, y: 2_i32);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("complete scalar family must check: {outcome:?}");
        };
        assert_eq!(checked.function_count(), 2);
        assert_eq!(checked.entry_function_name(), "main");
    });
}

#[test]
fn semantic_rule_owners_remain_distinct() {
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = 128_i8;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Form7,
        SemanticIssueKind::InvalidIntegerLiteral,
    );
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  return 0_i32;\n}\n",
        SemanticRule::Fn1,
        SemanticIssueKind::ReturnMismatch,
    );
    assert_rule_kind(
        b"fn main() -> status: std::process::ExitStatus pure {\n  invariant bad: 0_u64 != 0_u64;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Inv1,
        |kind| matches!(kind, SemanticIssueKind::InvalidInvariant { .. }),
    );
    // [EFF-2] rows are checked both ways: the first declares less than the
    // body exhibits, the second more. Both arms named a provider path in
    // v0.59; v0.60 roots every `effect_path` at a reference parameter, and
    // allocation and release carry no entry at all [EFF-1, STOR-8], so the
    // `allocates(heap)` arm is now a row a writer cannot even spell.
    assert_rule_kind(
        include_bytes!("../../../tests/conformance/cases/eff2-neg-undeclared-exhibited.wf"),
        SemanticRule::Eff2,
        |kind| matches!(kind, SemanticIssueKind::EffectMismatch { .. }),
    );
    assert_rule_kind(
        include_bytes!("../../../tests/conformance/cases/eff2-neg-declared-unexhibited.wf"),
        SemanticRule::Eff2,
        |kind| matches!(kind, SemanticIssueKind::EffectMismatch { .. }),
    );
}

#[test]
fn function_control_is_checked_and_main_has_an_ordinary_signature() {
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n}\n",
        SemanticRule::Fn1,
        SemanticIssueKind::FunctionFallthrough,
    );
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Fn1,
        SemanticIssueKind::UnreachableStatement,
    );
    // v0.58 FN-7 leaves launcher selection outside source acceptance.
    with_semantics(
        b"fn main(value: i32) -> result: unit pure {\n  return unit;\n}\n",
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "{outcome:?}"
            )
        },
    );
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  loop @done {\n    break @done;\n    return std::process::exit_status(code: 0_u8);\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Fn1,
        SemanticIssueKind::UnreachableStatement,
    );
}

/// [OWN-11] the per-iteration judgment, which is [LIV-1]'s liveness agreement
/// read at a loop head.
///
/// A body that leaves an outer binding dead on the backedge is the rejection;
/// a body that moves one and commits a value back into it before the backedge
/// agrees with the entering edge and is accepted.
#[test]
fn loops_enforce_own11_for_outer_affine_moves() {
    assert_rule(
        br#"fn measure(cell: Slots<u8, 4>) -> size: u64 pure {
  let n = cell.len;
  return n;
}

fn main() -> status: std::process::ExitStatus pure {
  let c = slots_new::<u8, 4>();
  for (i in 0_u64..2_u64) {
    let taken = measure(cell: move c);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Own11,
        SemanticIssueKind::MoveOuterBindingInLoop {
            binding: "c".to_owned(),
            mechanical_fix: "one iteration must leave every outer binding in the status the next \
                             one starts from: commit a value back into it before the backedge, or \
                             declare and consume it inside the body",
        },
    );
    with_semantics(
        br#"fn measure(cell: Slots<u8, 4>) -> size: u64 pure {
  let n = cell.len;
  return n;
}

fn main() -> status: std::process::ExitStatus pure {
  let c = slots_new::<u8, 4>();
  for (i in 0_u64..2_u64) {
    let taken = measure(cell: move c);
    set c = slots_new::<u8, 4>();
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            let SemanticOutcome::Complete(_) = outcome else {
                panic!(
                    "a body that reinitializes the outer binding agrees at the backedge: {outcome:?}"
                );
            };
        },
    );
    with_semantics(
        b"fn main() -> status: std::process::ExitStatus pure {\n  loop @forever {\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| {
            assert!(
                matches!(outcome, SemanticOutcome::Complete(_)),
                "a break-free loop has a contradictory continuation rather than an unsupported shape: {outcome:?}"
            );
        },
    );
}

#[test]
fn loop_break_and_backedge_cleanup_is_explicit() {
    let source = br#"nocopy struct Cell {
  value: i32;
}

fn main() -> status: std::process::ExitStatus pure {
  loop @again {
    let first = Cell(value: 1_i32);
    if True() {
      break @again;
    }
    let second = Cell(value: 2_i32);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("loop cleanup source must check: {outcome:?}");
        };
        let main = &checked.data.functions[0];
        let CheckedStatement::Loop {
            body,
            backedge_drops,
            ..
        } = &main.body.as_ref().expect("WF body")[0]
        else {
            panic!("first statement must be the checked loop");
        };
        assert_eq!(backedge_drops.len(), 2);
        assert!(backedge_drops[0].binding.0 > backedge_drops[1].binding.0);
        let CheckedStatement::Match { arms, .. } = &body[1] else {
            panic!("second loop statement must be the match");
        };
        let CheckedStatement::Break { drops, .. } = &arms[0].body[0] else {
            panic!("True arm must contain the checked break");
        };
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].binding, backedge_drops[1].binding);
    });
}

#[test]
fn named_arguments_and_copy_move_spelling_are_checked_generally() {
    let wrong_name = br#"fn take(value: i32) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  take(other: 1_i32);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        wrong_name,
        SemanticRule::Gram11,
        SemanticIssueKind::InvalidNamedArguments {
            callee: "take".to_owned(),
            declared_parameters: vec!["value".to_owned()],
        },
    );
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let a = 1_i32;\n  let b = move a;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Own1,
        SemanticIssueKind::MoveOfCopy {
            mechanical_fix: "use the copy place without `move`",
        },
    );
}

/// Both shapes are carried on rows that keep their callee name. [OP-7]'s
/// one-spelling rule moved the twenty respelled rows out of the callee-name
/// inventory, so `iadd.wrap` — which carried both shapes in v0.22 — is no
/// longer a name at all and reaches OP-1 at resolution before either shape can
/// be judged. The concerns survive on the rows that still have names: a call
/// missing the arguments its row mandates, and one written with named
/// arguments.
#[test]
fn operation_call_shapes_keep_their_exact_rule_owners() {
    // A retained-argument row with its mandatory arguments absent. This was
    // recorded as FN-2 and flagged then as a witness that would move if the
    // citation question were settled the other way; it was, so it did.
    // [DIAG-1] gives a table operation the rule [OP-2] selects and never FN-2,
    // and [TYPE-5] is what mandates these arguments, so their absence is its
    // violation — the reading `finf`/`fnan` already carried.
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = 4_i32;\n  let narrowed = cvt(value);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        SemanticIssueKind::InvalidOperation,
    );
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let left = 1_i32;\n  let right = 2_i32;\n  let value = imin(left: left, right: right);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Gram11,
        SemanticIssueKind::InvalidNamedArguments {
            callee: "imin".to_owned(),
            declared_parameters: Vec::new(),
        },
    );
}

/// [DIAG-1] "The cited rule is the rule selected by the callee's class": FN-2
/// for a user-generic call, and for a table operation the rule [OP-2] selects.
/// The compiler used to choose from the *kind* of argument problem instead, so
/// it was wrong in both directions at once — a table operation missing its
/// mandatory arguments cited FN-2, and a user-generic call missing or
/// miscounting its arguments cited TYPE-5.
///
/// One argument problem is held fixed across the two classes so that only the
/// callee varies: each pair below is the same failure, so the rule must follow
/// the callee and nothing else.
#[test]
fn the_cited_rule_follows_the_callee_class_and_not_the_argument_problem() {
    // Missing the arguments the callee's class mandates.
    assert_rule_kind(
        b"struct Held {\n  v: i32;\n}\n\nfn pick<T: drop>(value: T) -> result: T pure {\n  return move value;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let a = Held(v: 1_i32);\n  let b = pick(value: move a);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Fn2,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = 4_i32;\n  let narrowed = cvt(value);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        SemanticIssueKind::InvalidOperation,
    );

    // A wrong-count argument list, the same failure on both classes.
    assert_rule_kind(
        b"struct Held {\n  v: i32;\n}\n\nfn pick<T: drop>(value: T) -> result: T pure {\n  return move value;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let a = Held(v: 1_i32);\n  let b = pick::<Held, Held>(value: move a);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Fn2,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let value = 4_i32;\n  let narrowed = cvt::<i32>(value);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );

    // A generic nominal's construct is a third class: [TYPE-5] owns its
    // written arguments, and it shares the argument-list reader with the
    // user-generic call, so it is the control that the rule is not simply
    // keyed on that reader.
    assert_rule_kind(
        b"struct Pair<T: drop> {\n  v: T;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let p = Pair(v: 1_i32);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

#[test]
fn effect_mismatch_is_located_at_the_written_effect_row() {
    // [EFF-2] the body places one value into the window behind its reference
    // parameter, which exhibits `writes(target.next)` and `writes(target.len)`
    // [OP-10, WIN-2], and the declaration writes the empty row instead. The
    // citation lands on the written row, which is the `pure` atom.
    let source = br#"fn fill(target: &Slots<u8, 4>) -> result: unit pure contract {
  requires deref(target).len < deref(target).cap;
} {
  place_back(window: target, value: 7_u8);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let block = slots_new::<u8, 4>();
  fill(target: &block);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::SourceIssue { issue, .. } = outcome else {
            panic!("expected EFF-2 mismatch, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Eff2);
        let SemanticLocation::SourceNode(_, coordinate) = issue.location();
        let start = usize::try_from(coordinate.start().value()).expect("test offset fits usize");
        let end = usize::try_from(coordinate.end().value()).expect("test offset fits usize");
        assert_eq!(&source[start..end], b"pure");
    });
}

#[test]
fn nominal_diagnostics_retain_required_lists_and_repairs() {
    assert_rule(
        include_bytes!("../../../tests/conformance/cases/x-struct-neg-field-order.wf"),
        SemanticRule::Gram8,
        SemanticIssueKind::InvalidConstructionFields {
            constructor: "Pair".to_owned(),
            declared_fields: vec!["a".to_owned(), "b".to_owned()],
        },
    );
    assert_rule(
        include_bytes!("../../../tests/conformance/cases/x-match-gram10-out-of-order-fields.wf"),
        SemanticRule::Gram10,
        SemanticIssueKind::InvalidMatchFields {
            variant: "Both".to_owned(),
            declared_fields: vec!["a".to_owned(), "b".to_owned()],
        },
    );
    assert_rule(
        include_bytes!("../../../tests/conformance/cases/err2-neg-missing-variant.wf"),
        SemanticRule::Err2,
        SemanticIssueKind::NonExhaustiveMatch {
            missing_variants: vec!["Blue".to_owned()],
        },
    );
    assert_rule(
        b"struct Pair {\n  x: i32;\n  x: i32;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type6,
        SemanticIssueKind::DuplicateFieldLabel {
            label: "x".to_owned(),
        },
    );
    assert_rule(
        b"enum Pairing {\n  Both(a: i32, b: i32);\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let pair = Pairing::Both(a: 1_i32, b: 2_i32);\n  match pair {\n    Both(a: first) => {\n    }\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Gram10,
        SemanticIssueKind::InvalidMatchFields {
            variant: "Both".to_owned(),
            declared_fields: vec!["a".to_owned(), "b".to_owned()],
        },
    );
}

#[test]
fn give_completeness_rejects_each_structural_failure() {
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let flag = True();\n  let result = if flag {\n  } else {\n    give 0_i32;\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Give1,
        SemanticIssueKind::InvalidGive,
    );
    assert_rule(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let flag = True();\n  let result = if flag {\n    give 1_i32;\n    give 2_i32;\n  } else {\n    give 0_i32;\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Give1,
        SemanticIssueKind::InvalidGive,
    );
}

#[test]
fn enum_equality_exclusions_reach_the_intended_rule() {
    assert_rule(
        b"enum PayloadEq {\n  PayloadEmpty();\n  PayloadValue(value: u32);\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let left = PayloadEq::PayloadEmpty();\n  let right = PayloadEq::PayloadEmpty();\n  let equal = eeq(left, right);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Op1,
        SemanticIssueKind::InvalidOperation,
    );
    assert_rule_kind(
        b"enum LeftEq {\n  LeftFirst();\n}\n\nenum RightEq {\n  RightFirst();\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let left = LeftEq::LeftFirst();\n  let right = RightEq::RightFirst();\n  let equal = eeq(left, right);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

#[test]
fn nominal_adjacent_unimplemented_behavior_stays_non_language_failure() {
    // Keep the capability control smaller than the conformance program: this
    // test isolates set-field construction and read-back, while
    // `x-struct-set-field.wf` additionally proves its exact increment from
    // the S5 post-write image.
    with_semantics(
        b"struct Counter {\n  n: i32;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let c = Counter(n: 1_i32);\n  set c.n = 41_i32;\n  let v = c.n;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| assert!(matches!(outcome, SemanticOutcome::Complete(_))),
    );
    // Borrow-mode parameters and `let` borrows of scalars and enums, and the
    // [OWN-13] borrowed match this exercises, are on the normal path now.
    // Also written inline rather than read from
    // `x-enum-borrow-payload-live.wf` for the same reason: that case's
    // `deref(x) + 1_i32` is an undischarged v0.31 class site (residual
    // `deref(x) <= 2147483646`), so it too stays outside this capability
    // control. The shape kept here is the one under test — a payload enum
    // borrow-matched through `&'r` whose scrutinee stays live for a second
    // read, with each derived binder explicitly dereferenced.
    with_semantics(
        b"enum Cell {\n  Full(v: i32);\n  Void();\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let c = Cell::Full(v: 20_i32);\n  let p = &c;\n  let a = match deref(p) {\n    Full(v: x) => {\n      give deref(x);\n    }\n    Void() => {\n      give 0_i32;\n    }\n  }\n  let q = &c;\n  let b = match deref(q) {\n    Full(v: y) => {\n      give deref(y);\n    }\n    Void() => {\n      give 0_i32;\n    }\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| assert!(matches!(outcome, SemanticOutcome::Complete(_))),
    );
    assert_unsupported(
        b"struct Node {\n  next: Node;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        UnsupportedSemanticFeature::RecursiveNominalLayout,
    );
    assert_unsupported(
        b"enum Flag {\n  A();\n  B();\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let flag = Flag::A();\n  match flag {\n    A() => {\n    }\n    A() => {\n    }\n    B() => {\n    }\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n",
        UnsupportedSemanticFeature::DuplicateMatchArm,
    );
    // Each iteration consumes the local run and installs the returned run in
    // that binding. Ordinary ownership checks the complete replacement without
    // recovering the returned owner's ancestry from the helper body.
    with_semantics(
        br#"fn consume(cell: Slots<u8, 4>) -> out: Slots<u8, 4> pure {
  return move cell;
}

fn main() -> status: std::process::ExitStatus pure {
  let c = slots_new::<u8, 4>();
  for (i in 0_u64..2_u64) {
    set c = consume(cell: move c);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| assert!(matches!(outcome, SemanticOutcome::Complete(_))),
    );
}

#[test]
fn ordinary_signature_effects_reject_both_row_directions() {
    // Rows are checked in both directions [EFF-2]: first an unexhibited
    // declaration, then an undeclared exhibited read. Both are now written
    // over a reference parameter, because [EFF-1] gives a by-value parameter
    // no effect entry at all; a row rooted at one is the third assertion.
    assert_rule_kind(
        b"fn probe(args: &std::text::Args) -> result: unit reads(args) {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Eff2,
        |kind| matches!(kind, SemanticIssueKind::EffectMismatch { .. }),
    );
    assert_rule_kind(
        b"fn probe(args: &std::text::Args) -> result: u64 pure {\n  let total = std::text::args_count(args: args);\n  return total;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Eff2,
        |kind| matches!(kind, SemanticIssueKind::EffectMismatch { .. }),
    );
    assert_rule_kind(
        b"fn probe(args: std::text::Args) -> result: unit reads(args) {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Eff1,
        |kind| matches!(kind, SemanticIssueKind::InvalidEffectRow { .. }),
    );
}

#[test]
fn a_bare_reference_subscript_is_a_type7_missing_dereference() {
    // [TYPE-7] a reference variable denotes the reference, and its referent is
    // reached only through `deref`, so a bare reference operand in a
    // consuming context is the missing dereference. v0.59 also routed a `box`
    // holder here; v0.60 does not, a `Box` being an opaque struct and not a
    // reference at all [TYPE-2, TYPE-9], so the `Box` half of this test moved
    // to `box_holders_are_refused_by_each_positions_own_rule` below with the
    // rule each position actually owns.
    assert_rule(
        include_bytes!("../../../tests/conformance/cases/type7-neg-index-reference-holder.wf"),
        SemanticRule::Type7,
        SemanticIssueKind::MissingDereference {
            mechanical_fix: "write `deref(holder)`",
        },
    );
}

/// [TYPE-7] the reference positions, each citing TYPE-7 and its own
/// mechanical `deref(.)` repair.
#[test]
fn reference_holders_written_bare_are_type7_missing_dereferences() {
    assert_rule(
        include_bytes!("../../../tests/conformance/cases/type7-neg-match-reference-holder.wf"),
        SemanticRule::Type7,
        SemanticIssueKind::MissingDereference {
            mechanical_fix: "write `deref(holder)`",
        },
    );
}

/// [TYPE-9] a `Box` holder written where its content is required is refused by
/// the rule that owns the position, not by [TYPE-7]: a `Box` is an opaque
/// struct and its content is the field `inner`, so nothing about it is a
/// missing dereference.
///
/// This replaces the v0.59 test that routed all three positions to TYPE-7,
/// which rested on `box<T>` being a holder kind [TYPE-7 v0.59]; that reading
/// is retired.
#[test]
fn box_holders_are_refused_by_each_positions_own_rule() {
    assert_rule_kind(
        include_bytes!("../../../tests/conformance/cases/type7-neg-propagate-box-holder.wf"),
        SemanticRule::Err3,
        |kind| matches!(kind, SemanticIssueKind::InvalidPropagation),
    );
    assert_rule_kind(
        include_bytes!("../../../tests/conformance/cases/type7-neg-match-box-holder.wf"),
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
    assert_rule_kind(
        include_bytes!("../../../tests/conformance/cases/type7-neg-index-box-holder.wf"),
        SemanticRule::Op4,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

#[test]
fn result_construction_and_propagation_keep_context_and_rule_owners() {
    let source = br#"enum StepError {
  Failed();
}

struct Pair {
  value: i32;
}

fn step(value: i32) -> result: Result<i32, StepError> pure {
  return Ok<i32, StepError>(value: value);
}

fn forward(value: i32) -> result: Result<Pair, StepError> pure {
  let accepted = propagate step(value: value);
  let pair = Pair(value: accepted);
  return Ok<Pair, StepError>(value: pair);
}

fn direct(error: StepError) -> result: Result<Pair, StepError> pure {
  let accepted = propagate Err<i32, StepError>(error: error);
  let pair = Pair(value: accepted);
  return Ok<Pair, StepError>(value: pair);
}

fn bare(outcome: Result<i32, StepError>) -> result: Result<Pair, StepError> pure {
  let accepted = propagate outcome;
  let pair = Pair(value: accepted);
  return Ok<Pair, StepError>(value: pair);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("Result family must check: {outcome:?}");
        };
        let forward = &checked.data.functions[1];
        let CheckedStatement::PropagateLet {
            ok_type, context, ..
        } = &forward.body.as_ref().expect("WF body")[0]
        else {
            panic!("forward must retain its checked propagation edge");
        };
        assert_eq!(
            *ok_type,
            super::model::CheckedType::Integer(super::model::IntegerType::I32)
        );
        assert_eq!(context.function, "forward");
        assert!(!context.node_path.components().is_empty());
    });

    assert_rule(
        br#"nocopy enum StepError {
  Failed();
}

fn reuse(outcome: Result<i32, StepError>) -> result: Result<i32, StepError> pure {
  let accepted = propagate outcome;
  match outcome {
    Ok(value: second_value) => {
    }
    Err(error: second_error) => {
    }
  }
  return Ok<i32, StepError>(value: accepted);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Own1,
        SemanticIssueKind::UseAfterMove {
            mechanical_fix: "introduce a new `let` binding before reuse",
        },
    );

    assert_rule(
        include_bytes!("../../../tests/conformance/cases/err3-neg-error-type-mismatch.wf"),
        SemanticRule::Err3,
        SemanticIssueKind::InvalidPropagation,
    );
    assert_rule_kind(
        br#"enum Flag {
  First();
  Second();
}

fn main() -> status: std::process::ExitStatus pure {
  let flag = Flag::First();
  match Err(error: flag) {
    Ok(value: ok_value) => {
    }
    Err(error: err_value) => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
    assert_rule_kind(
        include_bytes!("../../../tests/conformance/cases/x-enum-result-payload-type-mismatch.wf"),
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

#[test]
fn set_retains_checked_copy_places_for_root_and_nested_field_updates() {
    let source = br#"struct Inner {
  value: i32;
}

struct Outer {
  inner: Inner;
  other: i32;
}

fn main() -> status: std::process::ExitStatus pure {
  let number = 1_i32;
  set number = 2_i32;
  let inner = Inner(value: 3_i32);
  let outer = Outer(inner: inner, other: 4_i32);
  set outer.inner.value = number;
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("copy-place set must check: {outcome:?}");
        };
        let body = checked.data.functions[0].body.as_ref().expect("WF body");
        let CheckedStatement::Set { target, .. } = &body[1] else {
            panic!("second statement must be the root set");
        };
        let super::model::CheckedSetTarget::Place(target) = target else {
            panic!("root set must retain an ordinary writable place");
        };
        assert!(target.fields.is_empty());
        let CheckedStatement::Set { target, .. } = &body[4] else {
            panic!("fifth statement must be the nested-field set");
        };
        let super::model::CheckedSetTarget::Place(target) = target else {
            panic!("nested set must retain an ordinary writable place");
        };
        assert_eq!(target.fields, vec![0, 0]);
    });
}

#[test]
fn set_rejections_keep_their_exact_rule_owners() {
    assert_rule(
        b"const answer: i32 = 1_i32;\n\nfn main() -> status: std::process::ExitStatus pure {\n  set answer = 2_i32;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Const2,
        SemanticIssueKind::ImmutableSetTarget,
    );
    // [STOR-1]'s affine-set-target rejection retired with the `replace`
    // statement it restructured to [SET-2]: under [WIN-3] assigning over an
    // owned place releases the old value when it is affine, and is a hard
    // error only when it is linear. The linear half is the live successor.
    assert_rule(
        b"nodrop struct Token {\n  value: i32;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let left = Token(value: 1_i32);\n  let right = Token(value: 2_i32);\n  set left = move right;\n  let Token(value: v) = move left;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Win3,
        SemanticIssueKind::LinearAssignmentTarget {
            target_type: "Token".to_owned(),
            mechanical_fix: "take the linear value out and consume it before writing this place",
        },
    );
    assert_rule_kind(
        b"fn main() -> status: std::process::ExitStatus pure {\n  let number = 1_i32;\n  set number = True();\n  return std::process::exit_status(code: 0_u8);\n}\n",
        SemanticRule::Type5,
        |kind| matches!(kind, SemanticIssueKind::TypeMismatch { .. }),
    );
}

/// [WIN-3] assigning over an owned place releases the old value when it is
/// affine.
///
/// v0.59's [LIV-2] admitted an affine `set` target only when the right-hand
/// side read the previous value out, and refused it otherwise with [STOR-1]'s
/// `replace` restructuring. Both halves retire: `replace` has no v0.60
/// production and [WIN-3] gives the old affine value its compiler-derived
/// release at the commit, so the plain overwrite and the consuming form are
/// both accepted. The remaining refusal is a linear target, checked above.
#[test]
fn an_affine_assignment_releases_its_old_value() {
    with_semantics(
        b"nocopy struct Cell {\n  value: i32;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let left = Cell(value: 1_i32);\n  let right = Cell(value: 2_i32);\n  set left = move right;\n  let seen = left.value;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        |outcome| {
            let SemanticOutcome::Complete(_) = outcome else {
                panic!("an affine overwrite releases the old value: {outcome:?}");
            };
        },
    );
    with_semantics(
        br#"nocopy struct Counts {
  lines: u64;
  bytes: u64;
}

fn walk(running: Counts) -> result: Counts pure {
  return move running;
}

fn main() -> status: std::process::ExitStatus pure {
  let totals = Counts(lines: 0_u64, bytes: 0_u64);
  set totals = walk(running: move totals);
  let lines = totals.lines;
  return std::process::exit_status(code: 0_u8);
}
"#,
        |outcome| {
            let SemanticOutcome::Complete(_) = outcome else {
                panic!("[OP-12]'s atomic in-place update must check: {outcome:?}");
            };
        },
    );
    // The two-statement form stays accepted beside it: [OP-12] adds a
    // spelling and removes none.
    with_semantics(
        br#"nocopy struct Counts {
  lines: u64;
  bytes: u64;
}

fn walk(running: Counts) -> result: Counts pure {
  return move running;
}

fn main() -> status: std::process::ExitStatus pure {
  let totals = Counts(lines: 0_u64, bytes: 0_u64);
  let sub = walk(running: move totals);
  let lines = sub.lines;
  let bytes = sub.bytes;
  let total = lines +wrap bytes;
  let empty = total == 0_u64;
  if empty {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#,
        |outcome| {
            let SemanticOutcome::Complete(_) = outcome else {
                panic!("the two-statement form must check: {outcome:?}");
            };
        },
    );
}

#[test]
fn entry_dead_owner_reinitialization_has_no_displaced_owner_write() {
    let source = r#"fn reuse(file: Box<u64>, incoming: Box<u64>) -> (current: Box<u64>, previous: Box<u64>) pure {
  let previous = move file;
  set file = move incoming;
  return move file, move previous;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source.as_bytes(), |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "an entry-dead binding initializes without writing its displaced owner: {outcome:?}"
        );
    });
    // v0.59 read the spurious `writes(file)` as an unexhibited declaration
    // [EFF-2]. [EFF-1] now gives a by-value parameter no effect entry at all,
    // so the same row is refused one rule earlier, at its own root.
    let spurious = source.replacen(" pure {", " writes(file) {", 1);
    assert_rule_kind(spurious.as_bytes(), SemanticRule::Eff1, |kind| {
        matches!(kind, SemanticIssueKind::InvalidEffectRow { .. })
    });
}

// Retired with [LIV-2]'s same-statement read-out:
// `same_statement_owner_readout_retains_its_atomic_commit_write` wrote its row
// over a by-value parameter (`reads(file), writes(file)` on `file: own
// ReadFile`), which [EFF-1] now refuses outright, and the shape it tested is
// [OP-12]'s atomic in-place update `set p = f(move p, args...);`. Its current
// successor assertions are `an_affine_assignment_releases_its_old_value`, the
// argument-order controls below, and the singleton/union target controls in
// `tests/references.rs`.
//
// Retired with [OWN-5]: `a_prior_rhs_borrow_cannot_retarget_a_later_atomic_readout`
// asserted a `BorrowConflict` between an exclusive loan and a later read-out,
// and it spelled `&uniq`, `replace` and the multi-target `set (a, b) = ...`,
// none of which has a v0.60 production. Its successors are [EFF-5]'s pairwise
// comparison of a call's substituted paths, exercised in
// `tests/references.rs::two_overlapping_substituted_writes_are_refused`, and
// [REF-2]'s invalidation of a bystander reference, exercised beside it.

/// [OWN-1, DIAG-1] each actual is checked before the call's EFF-5 comparison.
/// A repeated move therefore fails at the later actual, including an owner
/// after one of its fields was consumed. None of these is OP-12: the first
/// actual is a different place from the assignment target. EFF-5's own
/// overlapping-reference refusal remains covered by
/// `references::two_overlapping_substituted_writes_are_refused`.
#[test]
fn repeated_by_value_moves_are_rejected_before_call_effect_comparison() {
    assert_rule_kind(
        br#"fn pair(other: Slots<u8, 4>, left: Slots<u8, 4>, right: Slots<u8, 4>) -> out: Slots<u8, 4> pure {
  return move left;
}

fn main() -> status: std::process::ExitStatus pure {
  let c = slots_new::<u8, 4>();
  let spare = slots_new::<u8, 4>();
  set c = pair(other: move spare, left: move c, right: move c);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Own1,
        |kind| matches!(kind, SemanticIssueKind::UseAfterMove { .. }),
    );
    assert_rule_kind(
        br#"struct Holder {
  run: Slots<u8, 4>;
}

fn pair(other: Slots<u8, 4>, left: Slots<u8, 4>, right: Slots<u8, 4>) -> out: Slots<u8, 4> pure {
  return move left;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = slots_new::<u8, 4>();
  let spare = slots_new::<u8, 4>();
  let holder = Holder(run: move first);
  set holder.run = pair(other: move spare, left: move holder.run, right: move holder.run);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Own1,
        |kind| matches!(kind, SemanticIssueKind::UseAfterMove { .. }),
    );
    assert_rule_kind(
        br#"struct Holder {
  run: Slots<u8, 4>;
  spare: Slots<u8, 4>;
}

fn take(other: Slots<u8, 4>, left: Slots<u8, 4>, right: Holder) -> out: Slots<u8, 4> pure {
  return move left;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = slots_new::<u8, 4>();
  let second = slots_new::<u8, 4>();
  let spare = slots_new::<u8, 4>();
  let holder = Holder(run: move first, spare: move second);
  set holder.run = take(other: move spare, left: move holder.run, right: move holder);
  return std::process::exit_status(code: 0_u8);
}
"#,
        SemanticRule::Own1,
        |kind| matches!(kind, SemanticIssueKind::UseAfterMove { .. }),
    );
}

#[test]
fn only_an_op12_first_actual_gets_the_commit_read_out() {
    let ordinary_whole = br#"nocopy struct Cell {
  value: u64;
}

fn forward(other: u64, value: Cell) -> result: Cell pure {
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 7_u64);
  set cell = forward(other: 0_u64, value: move cell);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(ordinary_whole, |outcome| {
        assert!(
            matches!(outcome, SemanticOutcome::Complete(_)),
            "a complete binding is reinitialized from post-RHS liveness: {outcome:?}"
        );
    });

    let projected_later = br#"nocopy struct Cell {
  value: u64;
}

nocopy struct Holder {
  cell: Cell;
  spare: Cell;
}

fn forward(other: u64, value: Cell) -> result: Cell pure {
  return move value;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = Cell(value: 7_u64);
  let second = Cell(value: 8_u64);
  let holder = Holder(cell: move first, spare: move second);
  set holder.cell = forward(other: 0_u64, value: move holder.cell);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(projected_later, SemanticRule::Own1, |kind| {
        matches!(kind, SemanticIssueKind::UseAfterMove { .. })
    });

    let failed_result = br#"nocopy struct Cell {
  value: u64;
}

fn extract(value: Cell) -> result: u64 pure {
  return value.value;
}

fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 7_u64);
  set cell = extract(value: move cell);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(failed_result, SemanticRule::Own1, |kind| {
        matches!(kind, SemanticIssueKind::UseAfterMove { .. })
    });

    let routed_result = br#"fn retain(value: Result<u64, Box<u64>>) -> result: Result<u64, Box<u64>> pure contract {
  ensures when Ok(value: payload): payload == payload;
} {
  match move value {
    Ok(value: payload) => {
      return Ok<u64, Box<u64>>(value: payload);
    }
    Err(error: problem) => {
      return Err<u64, Box<u64>>(error: move problem);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let owner = box_new::<u64>(value: 7_u64);
  let wrapped = Err<u64, Box<u64>>(error: move owner);
  set wrapped = retain(value: move wrapped);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule_kind(routed_result, SemanticRule::Own1, |kind| {
        matches!(kind, SemanticIssueKind::UseAfterMove { .. })
    });
}

#[test]
fn set_revalidates_the_target_after_rhs_ownership_changes() {
    let source = br#"nocopy struct Cell {
  value: i32;
}

fn take(cell: Cell) -> result: i32 pure {
  return cell.value;
}

fn main() -> status: std::process::ExitStatus pure {
  let cell = Cell(value: 1_i32);
  set cell.value = take(cell: move cell);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source,
        SemanticRule::Own1,
        SemanticIssueKind::UseAfterMove {
            mechanical_fix: "introduce a new `let` binding before reuse",
        },
    );
}

#[test]
fn checked_cleanup_edges_cover_every_current_affine_exit() {
    let source = br#"nocopy struct Cell {
  value: i32;
}

struct Inner {
  selected: Cell;
  sibling: Cell;
}

struct Outer {
  inner: Inner;
  sibling: Cell;
}

enum Holder {
  Held(cell: Cell);
  Empty();
}

fn make() -> result: Cell pure {
  let cell = Cell(value: 1_i32);
  return move cell;
}

fn discard_call() -> result: unit pure {
  make();
  return unit;
}

fn drop_binder(value: Holder) -> result: unit pure {
  match move value {
    Held(cell: item) => {
    }
    Empty() => {
    }
  }
  return unit;
}

fn drop_before_give(flag: Bool) -> result: i32 pure {
  let selected = if flag {
    let temporary = Cell(value: 2_i32);
    give 1_i32;
  } else {
    give 0_i32;
  }
  return selected;
}

fn move_through_give(flag: Bool) -> result: Cell pure {
  let selected = if flag {
    let temporary = Cell(value: 3_i32);
    give move temporary;
  } else {
    let temporary = Cell(value: 4_i32);
    give move temporary;
  }
  return move selected;
}

fn reverse_order() -> result: unit pure {
  let first = Cell(value: 5_i32);
  let second = Cell(value: 6_i32);
  return unit;
}

fn consume_projection() -> result: unit pure {
  let selected = Cell(value: 7_i32);
  let inner_sibling = Cell(value: 8_i32);
  let inner = Inner(selected: move selected, sibling: move inner_sibling);
  let outer_sibling = Cell(value: 9_i32);
  let outer = Outer(inner: move inner, sibling: move outer_sibling);
  let taken = move outer.inner.selected;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("cleanup fixture must check: {outcome:?}");
        };
        let function = |name: &str| {
            checked
                .data
                .functions
                .iter()
                .find(|function| function.name == name)
                .unwrap_or_else(|| panic!("missing checked function {name}"))
        };

        let make = function("make");
        let CheckedStatement::Return { drops, .. } = &make.body.as_ref().expect("WF body")[1]
        else {
            panic!("make must end in return");
        };
        assert!(drops.is_empty(), "returned affine value must not also drop");

        let discard = function("discard_call");
        assert!(matches!(
            discard.body.as_ref().expect("WF body")[0],
            CheckedStatement::DropExpression { .. }
        ));

        let drop_binder = function("drop_binder");
        let CheckedStatement::Match { arms, .. } = &drop_binder.body.as_ref().expect("WF body")[0]
        else {
            panic!("drop_binder must start with match");
        };
        assert_eq!(arms[0].fallthrough_drops.len(), 1);
        assert!(arms[1].fallthrough_drops.is_empty());

        let drop_before_give = function("drop_before_give");
        let CheckedStatement::ValueMatchLet { arms, .. } =
            &drop_before_give.body.as_ref().expect("WF body")[0]
        else {
            panic!("drop_before_give must start with value match");
        };
        let CheckedStatement::Give { drops, .. } = &arms[0].body[1] else {
            panic!("first arm must end in give");
        };
        assert_eq!(drops.len(), 1);

        let move_through_give = function("move_through_give");
        let CheckedStatement::ValueMatchLet { arms, .. } =
            &move_through_give.body.as_ref().expect("WF body")[0]
        else {
            panic!("move_through_give must start with value match");
        };
        for arm in arms {
            let CheckedStatement::Give { drops, .. } = &arm.body[1] else {
                panic!("each arm must end in give");
            };
            assert!(drops.is_empty(), "given affine value must not also drop");
        }

        let reverse = function("reverse_order");
        let CheckedStatement::Return { drops, .. } = &reverse.body.as_ref().expect("WF body")[2]
        else {
            panic!("reverse_order must end in return");
        };
        assert_eq!(drops.len(), 2);
        assert!(drops[0].binding.0 > drops[1].binding.0);

        let projection = function("consume_projection");
        let CheckedStatement::Let {
            binding: taken,
            value:
                CheckedExpression::Project {
                    consume_root: true,
                    residual_drops,
                    ..
                },
            ..
        } = &projection.body.as_ref().expect("WF body")[5]
        else {
            panic!("affine field move must consume its root");
        };
        assert_eq!(residual_drops.len(), 2);
        // The partial consume excludes its selected field; the remaining
        // release graph is still visited in PROV-6 declaration order.
        assert_eq!(residual_drops[0].fields, vec![0, 1]);
        assert_eq!(residual_drops[1].fields, vec![1]);
        let CheckedStatement::Return { drops, .. } = &projection.body.as_ref().expect("WF body")[6]
        else {
            panic!("consume_projection must end in return");
        };
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].binding, *taken);
    });
}

/// [DIAG-1] the same-node citation rank is the rules' definition order in
/// the active specification; `SemanticRule::definition_rank` must agree with
/// the specification bytes for every citable rule.
///
/// The set under check is **walked from the enum**, not listed here. Both
/// `next_in_definition_order` and `definition_rank` are exhaustive matches, so
/// a new variant does not compile until it appears in each; this test then
/// makes the two check each other, since walking the chain must yield the
/// ranks 0, 1, 2, … in order. A hand-maintained list stood here until
/// 2026-08-08 and silently omitted `Gram6`, reporting every rule verified
/// while one was not.
#[test]
fn definition_rank_matches_the_active_specification() {
    let mut all = Vec::new();
    let mut rule = Some(SemanticRule::FIRST);
    while let Some(current) = rule {
        assert!(
            !all.contains(&current),
            "the definition-order chain revisits {}",
            current.id()
        );
        all.push(current);
        rule = current.next_in_definition_order();
    }

    // The chain and the rank table are separate exhaustive matches; this is
    // where they are made to agree, so neither can drift alone.
    for (position, rule) in all.iter().enumerate() {
        assert_eq!(
            rule.definition_rank(),
            position,
            "{} sits at chain position {position} but ranks {}",
            rule.id(),
            rule.definition_rank()
        );
    }

    let definition_line = |rule: SemanticRule| {
        let prefix = format!("[{}]", rule.id());
        ACTIVE_KERNEL_SPEC_TEXT
            .lines()
            .position(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("no definition line for {}", rule.id()))
    };
    let mut by_specification = all.clone();
    by_specification.sort_by_key(|rule| definition_line(*rule));
    for (walked, specified) in all.iter().zip(&by_specification) {
        assert_eq!(
            walked.id(),
            specified.id(),
            "definition_rank disagrees with the active specification order"
        );
    }
}
