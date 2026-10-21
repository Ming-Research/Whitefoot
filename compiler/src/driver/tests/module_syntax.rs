//! Program-wide syntax admission precedes individual module verdicts.

use super::{CacheDirectory, module_inputs};
use crate::{CompilationFailure, CompilerLimits, SourceInput};

const GRAPH: &[u8] = b"pkg::leaf: [];\npkg: [pkg::leaf];\n";
const LEAF_INTERFACE: &[u8] =
    b"public fn first(a: u64) -> r: u64 pure doc \"Returns its input.\";\n";
const ROOT_INTERFACE: &[u8] =
    b"public fn second(a: u64) -> r: u64 pure doc \"Returns its input.\";\n";
const LEAF_BODY: &[u8] = b"fn first(a: u64) -> r: u64 pure {\n  return a;\n}\n";
const ROOT_BODY: &[u8] = b"fn second(a: u64) -> r: u64 pure {\n  return a;\n}\n";
const BAD_LEAF_BODY: &[u8] = b"fn first(a: u64) -> r: u64 {\n  return a;\n}\n";
const BAD_ROOT_BODY: &[u8] = b"fn second(a: u64) -> r: u64 {\n  return a;\n}\n";

fn graph() -> crate::ModuleGraph {
    crate::form_module_graph(
        SourceInput::new("modules.wfg", GRAPH),
        CompilerLimits::default(),
    )
    .expect("both module rows form")
}

fn records<'a>(leaf: &'a [u8], root: &'a [u8]) -> [(&'static str, &'a [u8]); 4] {
    [
        ("leaf/module.wfm", LEAF_INTERFACE),
        ("leaf/a.wf", leaf),
        ("module.wfm", ROOT_INTERFACE),
        ("a.wf", root),
    ]
}

fn coordinates(failure: &CompilationFailure) -> Vec<(u32, u64, u64)> {
    failure
        .diagnostics()
        .map(|diagnostic| {
            assert_eq!(diagnostic.rule_id(), Some("GRAM-3"));
            assert_eq!(
                diagnostic
                    .record
                    .expected_terminals()
                    .expect("header continuation"),
                &["pure".to_owned(), "reads".to_owned(), "writes".to_owned()]
            );
            let coordinate = diagnostic
                .source_coordinate()
                .expect("original source coordinate");
            (
                coordinate.source().ordinal(),
                coordinate.start().value(),
                coordinate.end().value(),
            )
        })
        .collect()
}

/// Both module bodies reject in original unit order, including after the
/// cache has retained each module's individual source rejection.
#[test]
fn program_segment_errors_span_modules_on_cold_and_warm_cache_paths() {
    let graph = graph();
    let records = records(BAD_LEAF_BODY, BAD_ROOT_BODY);
    let inputs = module_inputs(&graph, &records);
    let limits = CompilerLimits::default();
    let plain = crate::check_module_program(&graph, &inputs, limits)
        .expect_err("both independent headers reject");
    assert_eq!(coordinates(&plain), vec![(1, 27, 28), (3, 28, 29)]);

    let directory = CacheDirectory::new("cross-module-syntax");
    let cache = directory.open();
    let cached_failure = || {
        crate::check_module_program_with_cache(&graph, &inputs, limits, &cache)
            .expect_err("the program rejects before any module verdict can hide a later defect")
    };
    let cold = cached_failure();
    assert_eq!(coordinates(&cold), coordinates(&plain));
    assert_eq!(
        cold.render(crate::DiagnosticFormat::Json),
        plain.render(crate::DiagnosticFormat::Json)
    );
    for name in ["pkg::leaf", "pkg"] {
        let verdict = super::super::module_verdict(&graph, &inputs, name, false, limits, Some(&cache))
            .expect("an individual source verdict is recordable");
        assert!(matches!(
            verdict.outcome(),
            crate::CheckOutcome::Rejected { .. }
        ));
        let warm = super::super::module_verdict(&graph, &inputs, name, false, limits, Some(&cache))
            .expect("the same individual verdict is reusable");
        assert!(warm.reused());
    }
    let warm = cached_failure();
    assert_eq!(coordinates(&warm), coordinates(&plain));
    assert_eq!(
        warm.render(crate::DiagnosticFormat::Json),
        plain.render(crate::DiagnosticFormat::Json)
    );

    let entry = || {
        crate::entry_verdict(
            &graph,
            &inputs,
            crate::ModuleEntry::Function {
                module: "pkg",
                function: "second",
            },
            limits,
            Some(&cache),
            &[],
        )
        .expect("the entry's closure records both module syntax rejections")
    };
    let cold_entry = entry();
    assert!(!cold_entry.reused());
    let crate::CheckOutcome::Rejected { diagnostics, .. } = cold_entry.outcome() else {
        panic!("the entry must reject both invalid headers");
    };
    let recorded = diagnostics
        .iter()
        .map(|diagnostic| {
            assert_eq!(diagnostic.rule(), "GRAM-3");
            let coordinate = diagnostic.source_coordinate().expect("original coordinate");
            (
                coordinate.source().ordinal(),
                coordinate.start().value(),
                coordinate.end().value(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(recorded, coordinates(&plain));
    let warm_entry = entry();
    assert!(warm_entry.reused());
    assert_eq!(warm_entry.outcome(), cold_entry.outcome());
}

/// A later module's syntax rejection gates the earlier module's canonical
/// rendering and name resolution, which each still reject alone.
#[test]
fn later_module_syntax_precedes_earlier_canonical_and_resolution_defects() {
    let graph = graph();
    let limits = CompilerLimits::default();
    for (leaf, earlier_rule) in [
        (
            &b"fn first(a: u64) -> r: u64 pure {\n return a;\n}\n"[..],
            "FORM-2",
        ),
        (
            &b"fn first(a: u64) -> r: u64 pure {\n  return missing;\n}\n"[..],
            "TYPE-5",
        ),
    ] {
        let records = records(leaf, BAD_ROOT_BODY);
        let inputs = module_inputs(&graph, &records);
        let isolated = crate::check_module(&graph, &inputs, "pkg::leaf", false, limits)
            .expect_err("the earlier module has a real later-stage defect");
        assert_eq!(isolated.rule_id(), Some(earlier_rule));
        let program = crate::check_module_program(&graph, &inputs, limits)
            .expect_err("the later module's missing effect row gates later stages");
        assert_eq!(coordinates(&program), vec![(3, 28, 29)]);
    }
}

/// An interface read by two module verdicts remains one original source
/// record and contributes its declaration rejection only once.
#[test]
fn shared_dependency_interface_syntax_is_reported_once() {
    let graph = graph();
    let mut records = records(LEAF_BODY, ROOT_BODY);
    records[0].1 = b"public fn first(a: u64) -> r: u64;\n";
    let inputs = module_inputs(&graph, &records);
    let failure = crate::check_module_program(&graph, &inputs, CompilerLimits::default())
        .expect_err("the shared interface header omits its effect row");
    // Recognition admits `u64 ;` through the type production's SELECT_2
    // rows, so DIAG-1 stops at `effects`, whose grammar owner is EFF-1.
    // The body's `u64 {` instead stops within `type` and cites GRAM-3.
    assert_eq!(failure.diagnostics().count(), 1);
    assert_eq!(failure.stage(), crate::CompilationStage::Parsing);
    assert_eq!(failure.rule_id(), Some("EFF-1"));
    assert_eq!(
        failure.record.expected_terminals().expect("effect row"),
        &["pure".to_owned(), "reads".to_owned(), "writes".to_owned()]
    );
    let coordinate = failure.source_coordinate().expect("interface coordinate");
    assert_eq!(coordinate.source().ordinal(), 0);
    assert_eq!(coordinate.start().value(), 33);
    assert_eq!(coordinate.end().value(), 34);
}

/// Program-wide syntax admission does not expand the source selection of a
/// query for one module, or an interface-only query [MOD-8].
#[test]
fn individual_module_and_interface_checks_keep_their_record_scope() {
    let graph = graph();
    let records = records(LEAF_BODY, BAD_ROOT_BODY);
    let inputs = module_inputs(&graph, &records);
    let limits = CompilerLimits::default();
    crate::check_module(&graph, &inputs, "pkg::leaf", false, limits)
        .expect("the unrelated root implementation is not read");
    crate::check_module(&graph, &inputs, "pkg", true, limits)
        .expect("an interface query does not read its module's implementation");
    let failure = crate::check_module(&graph, &inputs, "pkg", false, limits)
        .expect_err("the module query reads its own invalid body");
    assert_eq!(failure.diagnostics().count(), 1);
    assert_eq!(failure.rule_id(), Some("GRAM-3"));
}

/// A canonical trivia gap spanning lines keeps FORM-2's last-line anchor
/// when the program gate relocates the retained per-record diagnostic.
#[test]
fn canonical_gap_relocation_preserves_the_offending_line() {
    let graph = graph();
    let leaf = b"fn first(a: u64) -> r: u64 pure {\n\n  return a;\n}\n";
    let records = records(leaf, ROOT_BODY);
    let inputs = module_inputs(&graph, &records);
    let limits = CompilerLimits::default();
    let isolated = crate::check_module(&graph, &inputs, "pkg::leaf", false, limits)
        .expect_err("a blank line inside the function is noncanonical");
    let program = crate::check_module_program(&graph, &inputs, limits)
        .expect_err("the same canonical gap rejects the program");
    assert_eq!(isolated.rule_id(), Some("FORM-2"));
    assert_eq!(program.rule_id(), Some("FORM-2"));
    assert_eq!(
        program.render(crate::DiagnosticFormat::Json),
        isolated.render(crate::DiagnosticFormat::Json)
    );
}

/// A later record's resource stop prevents a complete program-wide language
/// judgment and replaces an already collected declaration rejection.
#[test]
fn later_module_resource_stop_replaces_earlier_segment_rejection() {
    let graph = graph();
    let root = b"fn second(a: u64) -> r: u64 pure {\n  let value = 0_u64;\n  let other = 1_u64;\n  return a;\n}\n";
    let records = records(BAD_LEAF_BODY, root);
    let inputs = module_inputs(&graph, &records);
    let ordinary = crate::check_module_program(&graph, &inputs, CompilerLimits::default())
        .expect_err("only the earlier missing effect row rejects");
    assert_eq!(coordinates(&ordinary), vec![(1, 27, 28)]);
    let mut limits = CompilerLimits::default();
    limits.lexer.max_tokens = 20;
    let failure = crate::check_module_program(&graph, &inputs, limits)
        .expect_err("the later declaration exceeds its token ceiling");
    assert_eq!(failure.kind(), crate::CompilationFailureKind::Resource);
    assert_eq!(failure.diagnostics().count(), 1);
    assert_eq!(failure.rule_id(), None);
    assert_eq!(failure.record.expected_terminals(), None);
}

/// Entry syntax admission reads its closure only: unrelated malformed bytes
/// and their source-size limit do not change either a cold or cached verdict.
/// Reordering those records changes a cached rejection's original ordinal,
/// even while the selected records and their order remain unchanged.
#[test]
fn entry_syntax_gate_ignores_unrelated_oversized_records() {
    let graph = crate::form_module_graph(
        SourceInput::new(
            "modules.wfg",
            b"pkg::leaf: [];\npkg: [];\n\nentry leaf = pkg::leaf::first;\n",
        ),
        CompilerLimits::default(),
    )
    .expect("the independent module rows and entry form");
    let unrelated = vec![b'@'; 20_001];
    let accepted_records = records(LEAF_BODY, &unrelated);
    let inputs = module_inputs(&graph, &accepted_records);
    let mut limits = CompilerLimits::default();
    limits.source.max_source_bytes = 10_000;
    let all = crate::check_module_program(&graph, &inputs, limits)
        .expect_err("the full program reads the oversized record");
    assert_eq!(all.kind(), crate::CompilationFailureKind::Resource);
    crate::check_module_entry(&graph, &inputs, crate::ModuleEntry::Named("leaf"), limits)
        .expect("the unrelated module is outside the entry's closure");

    let directory = CacheDirectory::new("entry-unrelated-syntax");
    let cache = directory.open();
    let entry = || {
        crate::entry_verdict(
            &graph,
            &inputs,
            crate::ModuleEntry::Named("leaf"),
            limits,
            Some(&cache),
            &[],
        )
        .expect("the entry verdict reads no unrelated oversized record")
    };
    let cold = entry();
    assert!(matches!(cold.outcome(), crate::CheckOutcome::Accepted { .. }));
    let warm = entry();
    assert!(warm.reused());
    assert_eq!(warm.outcome(), cold.outcome());

    let bad_records = records(BAD_LEAF_BODY, &unrelated);
    let bad_inputs = module_inputs(&graph, &bad_records);
    let rejected_entry = |inputs: &[SourceInput<'_>]| {
        crate::entry_verdict(
            &graph,
            inputs,
            crate::ModuleEntry::Named("leaf"),
            limits,
            Some(&cache),
            &[],
        )
        .expect("the selected header rejects without reading unrelated oversized bytes")
    };
    let source_ordinal = |verdict: &crate::CheckVerdict| {
        let crate::CheckOutcome::Rejected { diagnostics, .. } = verdict.outcome() else {
            panic!("the selected leaf header must reject");
        };
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule(), "GRAM-3");
        let coordinate = diagnostics[0]
            .source_coordinate()
            .expect("the original record coordinate is retained");
        assert_eq!(coordinate.start().value(), 27);
        assert_eq!(coordinate.end().value(), 28);
        coordinate.source().ordinal()
    };
    let cold_rejection = rejected_entry(&bad_inputs);
    assert!(!cold_rejection.reused());
    assert_eq!(source_ordinal(&cold_rejection), 1);
    let warm_rejection = rejected_entry(&bad_inputs);
    assert!(warm_rejection.reused());
    assert_eq!(warm_rejection.outcome(), cold_rejection.outcome());

    let reordered = [bad_records[2], bad_records[3], bad_records[0], bad_records[1]];
    let reordered_inputs = module_inputs(&graph, &reordered);
    let reordered_rejection = rejected_entry(&reordered_inputs);
    assert!(!reordered_rejection.reused());
    assert_eq!(source_ordinal(&reordered_rejection), 3);
    let rewarmed_rejection = rejected_entry(&reordered_inputs);
    assert!(rewarmed_rejection.reused());
    assert_eq!(rewarmed_rejection.outcome(), reordered_rejection.outcome());
}
