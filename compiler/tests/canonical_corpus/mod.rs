#![forbid(unsafe_code)]

//! The corpus is canonical by construction, not by assertion.
//!
//! Every `.wf` file the repository keeps as a program — the conformance cases
//! and the program corpus — is rendered from its own derivation tree and
//! compared with itself. A file that is canonical must reproduce its exact
//! bytes; a file that is deliberately not must render to something else, and
//! that something else must be canonical.
//!
//! This is what gives the renderer a permanent reason to exist and what makes
//! a scripted corpus migration safe: the transform may produce any layout that
//! parses, because this gate holds the result to FORM-2 for every file.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::conformance::corpus as conformance_manifest;

use conformance_manifest::Expectation;

use whitefoot::{
    ACTIVE_KERNEL_SPEC_HASH, CanonicalOutcome, CompilerLimits, FinalizeOutcome, LexOutcome,
    ParseOutcome, RenderOutcome, SourceBundle, SourceInput, TerminalOutcome, audit_canonical,
    classify_terminals, finalize, lex, parse, parse_graph, render_canonical,
};

/// The two corpus roots, reached from the compiler package.
fn corpus_roots() -> [PathBuf; 2] {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler package must live directly under the repository root");
    [
        repository.join("tests").join("conformance").join("cases"),
        repository.join("tests").join("programs"),
    ]
}

/// Every corpus `.wf` file, in one stable order.
fn corpus_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut directories = Vec::from(corpus_roots());
    while let Some(root) = directories.pop() {
        let entries = std::fs::read_dir(&root)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", root.display()));
        for entry in entries {
            let path = entry
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", root.display()))
                .path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|extension| {
                extension == "wf" || extension == "wfm" || extension == "wfg"
            }) {
                // A module-form case or program keeps its interfaces and its
                // graph beside its implementation records [MOD-1, MOD-2].
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty(), "the corpus roots must hold programs");
    files
}

/// What one corpus file's own derivation says about it.
struct Reading {
    /// The file's bytes are exactly what FORM-2 requires.
    canonical: bool,
    /// The bytes the file's derivation tree denotes.
    rendered: Vec<u8>,
}

/// Derives a source and renders it, or reports that it reaches no tree.
///
/// A source issue is `None` rather than a panic: a corpus that deliberately
/// holds rejected programs must be able to hold programs that do not parse.
fn read_through_the_tree(logical_path: &str, source: &[u8]) -> Option<Reading> {
    let limits = CompilerLimits::default();
    let inputs = [SourceInput::new(logical_path, source)];
    let bundle = SourceBundle::with_limits(&inputs, limits.source)
        .unwrap_or_else(|error| panic!("{logical_path}: source envelope must be valid: {error:?}"));
    let LexOutcome::Complete(lexed) = lex(&bundle, limits.lexer) else {
        return None;
    };
    let TerminalOutcome::Complete(classified) =
        classify_terminals(&lexed, ACTIVE_KERNEL_SPEC_HASH, limits.terminals)
    else {
        return None;
    };
    // A graph file takes the `graph_file` start and every source record the
    // `program` start [GRAM-2, MOD-1].
    let parsed = if logical_path.ends_with(".wfg") {
        parse_graph(classified, limits.parser)
    } else {
        parse(classified, limits.parser)
    };
    let ParseOutcome::Complete(parsed) = parsed else {
        return None;
    };
    let FinalizeOutcome::Complete(finalized) = finalize(parsed, limits.finalizer) else {
        return None;
    };
    let rendered = match render_canonical(&finalized, limits.canonical) {
        RenderOutcome::Complete(sources) => {
            let [rendered] = <[_; 1]>::try_from(sources)
                .unwrap_or_else(|_| panic!("{logical_path}: one input renders one source"));
            rendered.bytes
        }
        other => panic!("{logical_path}: a finalized tree must render: {other:?}"),
    };
    let canonical = matches!(
        audit_canonical(*finalized, limits.canonical),
        CanonicalOutcome::Complete(_)
    );
    Some(Reading {
        canonical,
        rendered,
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .expect("a corpus entry has a file name")
        .to_string_lossy()
        .into_owned()
}

/// Exact conformance source paths and the expectations declared for them.
/// A module-form case's directory stands for every record below it.
fn manifest_expectations() -> BTreeMap<PathBuf, Expectation> {
    let cases = conformance_manifest::corpus_directory().join("cases");
    let mut expectations = BTreeMap::new();
    for case in conformance_manifest::load() {
        let path = case
            .module_root()
            .unwrap_or_else(|| cases.join(format!("{}.wf", case.id)));
        assert!(
            expectations.insert(path.clone(), case.expect).is_none(),
            "{} appears more than once in the conformance manifest",
            path.display()
        );
    }
    assert!(
        !expectations.is_empty(),
        "the conformance manifest must declare cases"
    );
    expectations
}

/// The expectation one corpus file is held to: its own case's, or that of the
/// module-form case whose directory holds it.
fn expectation_for<'a>(
    expectations: &'a BTreeMap<PathBuf, Expectation>,
    path: &Path,
) -> Option<&'a Expectation> {
    path.ancestors()
        .find_map(|candidate| expectations.get(candidate))
}

/// The two ways a corpus source may need a manifest-declared exclusion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Exclusion {
    /// Lexing, terminal classification, parsing, or finalization produced no tree.
    Underived,
    /// A tree rendered successfully, but the source bytes were not canonical.
    Noncanonical,
}

/// Whether one exact manifest expectation permits an exclusion from this gate.
///
/// Any declared rejection may fail before a tree exists. Once a tree does
/// exist, only a `FORM-*` rejection may deliberately retain non-canonical
/// bytes. Semantic rejects still have to round-trip like every other derived
/// source.
fn manifest_allows_exclusion(exclusion: Exclusion, expectation: Option<&Expectation>) -> bool {
    match (exclusion, expectation) {
        (Exclusion::Underived, Some(Expectation::Reject(_))) => true,
        (Exclusion::Noncanonical, Some(Expectation::Reject(rule))) => rule.starts_with("FORM-"),
        _ => false,
    }
}

#[test]
fn manifest_exclusions_cover_only_the_declared_failure_edges() {
    let grammar_reject = Expectation::Reject("GRAM-9".to_owned());
    let form_reject = Expectation::Reject("FORM-2".to_owned());
    let semantic_reject = Expectation::Reject("TYPE-5".to_owned());

    assert!(manifest_allows_exclusion(
        Exclusion::Underived,
        Some(&grammar_reject)
    ));
    assert!(manifest_allows_exclusion(
        Exclusion::Underived,
        Some(&form_reject)
    ));
    assert!(manifest_allows_exclusion(
        Exclusion::Underived,
        Some(&semantic_reject)
    ));
    assert!(manifest_allows_exclusion(
        Exclusion::Noncanonical,
        Some(&form_reject)
    ));

    assert!(!manifest_allows_exclusion(
        Exclusion::Noncanonical,
        Some(&semantic_reject)
    ));
    assert!(!manifest_allows_exclusion(Exclusion::Underived, None));
    assert!(!manifest_allows_exclusion(Exclusion::Noncanonical, None));
    for expectation in [
        Expectation::Accept,
        Expectation::Run(0),
        Expectation::Unsupported,
    ] {
        assert!(!manifest_allows_exclusion(
            Exclusion::Underived,
            Some(&expectation)
        ));
        assert!(!manifest_allows_exclusion(
            Exclusion::Noncanonical,
            Some(&expectation)
        ));
    }
}

#[test]
fn every_canonical_corpus_file_re_renders_to_itself() {
    the_case_covering_ex1_reproduces_its_normative_bytes();
    let files = corpus_files();
    let expectations = manifest_expectations();
    let mut round_tripped = Vec::new();
    let mut manifest_noncanonical = Vec::new();
    let mut manifest_underived = Vec::new();
    let mut unexpected_noncanonical = Vec::new();
    let mut unexpected_underived = Vec::new();

    for path in &files {
        let name = file_name(path);
        let source = std::fs::read(path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        let expectation = expectation_for(&expectations, path);
        let Some(reading) = read_through_the_tree(&name, &source) else {
            if manifest_allows_exclusion(Exclusion::Underived, expectation) {
                manifest_underived.push(name);
            } else {
                unexpected_underived.push((name, expectation.cloned()));
            }
            continue;
        };

        // Whatever went in, what comes out is canonical. This is the property
        // a migration relies on, so it is checked for every file rather than
        // only for the ones that were canonical already.
        let rendered_again = read_through_the_tree(&name, &reading.rendered)
            .unwrap_or_else(|| panic!("{name}: rendered bytes must derive"));
        assert!(
            rendered_again.canonical,
            "{name}: rendered bytes must be canonical"
        );
        assert_eq!(
            rendered_again.rendered, reading.rendered,
            "{name}: rendering must be idempotent"
        );

        if reading.canonical {
            assert_eq!(
                String::from_utf8_lossy(&reading.rendered),
                String::from_utf8_lossy(&source),
                "{name}: a canonical file must re-render to itself"
            );
            round_tripped.push(name);
        } else {
            assert_ne!(
                reading.rendered, source,
                "{name}: a non-canonical file must render to different bytes"
            );
            if manifest_allows_exclusion(Exclusion::Noncanonical, expectation) {
                manifest_noncanonical.push(name);
            } else {
                unexpected_noncanonical.push((name, expectation.cloned()));
            }
        }
    }

    // Reported together so one new failure does not hide the rest of the
    // corpus split. The manifest supplies the exclusions; no filename or
    // expected count is encoded here.
    let total = round_tripped.len()
        + manifest_noncanonical.len()
        + manifest_underived.len()
        + unexpected_noncanonical.len()
        + unexpected_underived.len();
    let report = format!(
        "{total} corpus files: {} round-tripped, {} manifest-declared non-canonical, {} manifest-declared underived, {} unexpectedly non-canonical, {} unexpectedly underived",
        round_tripped.len(),
        manifest_noncanonical.len(),
        manifest_underived.len(),
        unexpected_noncanonical.len(),
        unexpected_underived.len(),
    );
    assert!(
        unexpected_underived.is_empty(),
        "{report}\na source may fail to derive only when its exact manifest case declares reject; unexpected: {unexpected_underived:?}"
    );
    assert!(
        unexpected_noncanonical.is_empty(),
        "{report}\na derived source may be non-canonical only when its exact manifest case declares a FORM-* rejection; unexpected: {unexpected_noncanonical:?}"
    );
    assert_eq!(
        total,
        files.len(),
        "{report}\nevery corpus file is accounted for"
    );
}

/// The anchor that locates [EX-1]'s program block in the active specification.
///
/// It is the rule's own normative sentence rather than a section number or a
/// line offset, so a renumbered section keeps working and a rewritten rule
/// fails loudly instead of matching some other fenced block.
const EX1_ANCHOR: &str = "[EX-1] The following complete program is byte-exact canonical form:";

/// [EX-1]'s program block, taken from the specification the compiler embeds.
fn ex1_block() -> &'static str {
    let occurrences = whitefoot::ACTIVE_KERNEL_SPEC_TEXT
        .matches(EX1_ANCHOR)
        .count();
    assert_eq!(
        occurrences, 1,
        "[EX-1]'s sentence must occur exactly once, or this check is reading the wrong block"
    );
    let after = whitefoot::ACTIVE_KERNEL_SPEC_TEXT
        .split_once(EX1_ANCHOR)
        .expect("the anchor was just counted")
        .1;
    let opened = after
        .split_once("```\n")
        .expect("[EX-1]'s sentence must be followed by a fenced block")
        .1;
    opened
        .split_once("```")
        .expect("[EX-1]'s fenced block must close")
        .0
}

/// The one conformance case that is designated to embody [EX-1].
///
/// Identified through the manifest rather than by filename: the case is the
/// row naming `EX-1` among its rules, and exactly one row does, so renaming or
/// replacing the file cannot leave this check pointing at nothing.
fn ex1_case() -> PathBuf {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler package must live directly under the repository root");
    let manifest = repository
        .join("tests")
        .join("conformance")
        .join("manifest.jsonl");
    let text = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", manifest.display()));
    let mut named: Vec<String> = Vec::new();
    for row in text.lines() {
        let row = row.trim();
        if row.is_empty() || row.starts_with('#') {
            continue;
        }
        let Some(rules) = row.split_once("\"rules\"") else {
            continue;
        };
        let rules = rules.1.split_once(']').expect("a rules array must close").0;
        if !rules.contains("\"EX-1\"") {
            continue;
        }
        let id = row
            .split_once("\"id\"")
            .expect("a case row states an id")
            .1
            .split('"')
            .nth(1)
            .expect("an id is a quoted string")
            .to_owned();
        named.push(id);
    }
    assert_eq!(
        named.len(),
        1,
        "exactly one conformance case may cover [EX-1]; found {named:?}"
    );
    repository
        .join("tests")
        .join("conformance")
        .join("cases")
        .join(format!("{}.wf", named[0]))
}

/// [EX-1] is normative bytes — §19 is titled "Worked example (normative
/// bytes)", the rule requires byte-exact canonical form, and [SCOPE-2] accepts
/// a program only if it satisfies every rule — and one conformance case exists
/// to reproduce them. Nothing compared the two until a four-byte divergence in
/// a `doc` string survived from candidate assembly into an owner review packet.
///
/// **This pins the copy to the block, never the block to reality.** It says
/// nothing about whether [EX-1]'s bytes are the right bytes. It also must not
/// be extended to `run-ex1-value-match`, an EX-1-*class* program that names no
/// EX-1 among its rules and is deliberately a superset of the block.
fn the_case_covering_ex1_reproduces_its_normative_bytes() {
    let case = ex1_case();
    let bytes = std::fs::read(&case)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", case.display()));
    let source = String::from_utf8(bytes).expect("a corpus file is UTF-8");
    assert_eq!(
        source,
        ex1_block(),
        "{} must reproduce [EX-1]'s program block byte for byte",
        case.display()
    );
}
