//! Repair pairs whose rejection is reached through a module graph [MOD-1,
//! MOD-5, MOD-8], which no single source can spell.
//!
//! A module pair names a conformance case directory and gives each
//! alternative as the records of that directory it replaces, by path below
//! the case root. The directory is the rejected program, or, where it is
//! accepted, the pair names the records a probe replaces to reject it. The
//! rejection and every alternative run through the same assertions as a
//! single-source pair; the alternative's whole program is checked through
//! each of its entries and must run no judgment in a contradictory state.
//!
//! Two rejections only the toolchain's own standard library can meet [MOD-10]
//! are reached through its records: a module pair rewrites a stretch of a
//! carried library record, and a library graph pair forms a graph record as
//! the library's own.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::super::{CompilerLimits, check_module_entry, check_module_program};
use super::harness::{check_rejection, declared_functions};
use crate::{
    CompilationFailure, ModuleEntry, ModuleProgramFailure, SourceInput, discover_module_sources,
    form_module_program_graph,
};

/// One record of a case directory, replaced by an alternative: its path below
/// the case root and its new bytes.
pub(super) type Replacement = (&'static str, &'static [u8]);

/// One rewrite of a record of the standard library the toolchain carries
/// [MOD-10]: the record's logical path, a stretch of its text and the text
/// written there instead.
pub(super) type LibraryEdit = (&'static str, &'static str, &'static str);

/// One repair pair [DIAG-1] on a module program.
pub(super) struct ModulePair {
    /// Names the pair in a failure: the case directory, and what a probe
    /// writes over it.
    pub(super) name: &'static str,
    /// The conformance case directory holding the module program.
    pub(super) case: &'static str,
    /// The records of that directory a probe replaces to reject an accepted
    /// program; empty where the directory is rejected as it stands.
    pub(super) rejected: &'static [Replacement],
    /// The rewrites of the carried library's records the rejected program
    /// reads; a program the toolchain's own library alone can write is
    /// reached through them, and the alternatives read the library as it is.
    pub(super) library: &'static [LibraryEdit],
    /// The numbered rule the rejection must cite.
    pub(super) rule: &'static str,
    /// Exact substrings of the rendered rejection, as for a source pair.
    pub(super) sentences: &'static [&'static str],
    /// One entry per alternative: the records it replaces.
    pub(super) repaired: &'static [&'static [Replacement]],
}

pub(super) const MODULE_PAIRS: &[ModulePair] = &[ModulePair {
    name: "type11-neg-writable-public-field",
    case: "type11-neg-writable-public-field",
    rejected: &[],
    library: &[],
    rule: "TYPE-11",
    sentences: &[
        "\n  mechanical_fix: write the field `public readonly`, so that code outside the declaring module reads it and only that module writes it, or make it private\n",
    ],
    repaired: &[
        &[(
            "pairs/module.wfm",
            br#"public struct Pair {
  public readonly first: u64;
  public readonly second: u64;
  invariant ordered(pair): pair.first <= pair.second;
}
"#,
        )],
        &[(
            "pairs/module.wfm",
            br#"public struct Pair {
  first: u64;
  public readonly second: u64;
  invariant ordered(pair): pair.first <= pair.second;
}
"#,
        )],
    ],
}];

fn cases_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the compiler package lives directly under the repository root")
        .join("tests")
        .join("conformance")
        .join("cases")
}

/// Copies a case directory to a fresh scratch directory, with `replacements`
/// written over its records.
fn scratch_copy(case: &str, replacements: &[Replacement]) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "whitefoot-module-pair-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    copy_directory(&cases_directory().join(case), &root);
    for (path, bytes) in replacements {
        std::fs::write(root.join(path), bytes).expect("a replaced record is written");
    }
    root
}

fn copy_directory(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the scratch directory is made");
    for entry in std::fs::read_dir(from).expect("the case directory is read") {
        let entry = entry.expect("a case entry is read");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a case entry has a type").is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("a case record is copied");
        }
    }
}

/// The records `library` rewrites, placed in their library modules: the
/// check reads each in place of the record the toolchain carries [MOD-10].
fn edited_library_records(
    case: &str,
    graph: &crate::ModuleGraph,
    library: &[LibraryEdit],
) -> Vec<(&'static str, String, crate::ModuleId, crate::SourceRole)> {
    library
        .iter()
        .map(|(logical, from, to)| {
            let (_, text) = crate::library::RECORDS
                .iter()
                .find(|(path, _)| path == logical)
                .unwrap_or_else(|| panic!("{case}: the library carries no record {logical}"));
            assert!(text.contains(from), "{case}: {logical} holds no `{from}`");
            let (path, role) = crate::library::record_module(logical)
                .unwrap_or_else(|| panic!("{case}: {logical} names no library module"));
            let module = graph
                .modules()
                .iter()
                .position(|module| module.is_at(crate::Package::Standard, &path))
                .and_then(crate::ModuleId::from_index)
                .unwrap_or_else(|| panic!("{case}: the graph registers no module for {logical}"));
            (*logical, text.replacen(from, to, 1), module, role)
        })
        .collect()
}

/// What checking one module program reached: its rejection, or the functions
/// it declares when it is accepted.
enum Reached {
    Rejected(CompilationFailure),
    Accepted { declared: Vec<String> },
}

/// Forms the graph, checks every module and every entry of the program in
/// `root`, and returns the first rejection or the declared functions.
fn reach(case: &str, root: &Path, library: &[LibraryEdit]) -> Reached {
    let graph =
        match form_module_program_graph(&root.join("modules.wfg"), CompilerLimits::default()) {
            Ok(graph) => graph,
            Err(ModuleProgramFailure::Compilation(failure)) => return Reached::Rejected(failure),
            Err(ModuleProgramFailure::Discovery(failure)) => panic!("{case}: {failure}"),
        };
    let sources =
        discover_module_sources(root, &graph).unwrap_or_else(|failure| panic!("{case}: {failure}"));
    let edited = edited_library_records(case, &graph, library);
    let mut inputs: Vec<_> = sources
        .iter()
        .map(|source| {
            SourceInput::new(&source.logical_path, &source.bytes)
                .in_module(source.module, source.role)
        })
        .collect();
    inputs.extend(edited.iter().map(|(logical, text, module, role)| {
        SourceInput::new(logical, text.as_bytes()).in_module(*module, *role)
    }));
    if let Err(failure) = check_module_program(&graph, &inputs, CompilerLimits::default()) {
        return Reached::Rejected(failure);
    }
    for entry in graph.entries() {
        if let Err(failure) = check_module_entry(
            &graph,
            &inputs,
            ModuleEntry::Named(entry.name()),
            CompilerLimits::default(),
        ) {
            return Reached::Rejected(failure);
        }
    }
    let declared = sources
        .iter()
        .flat_map(|source| declared_functions(&source.bytes))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Reached::Accepted { declared }
}

/// The judgments of `root`'s accepted program that succeed only in a
/// contradictory state.
fn contradictions(case: &str, root: &Path, declared: &[String]) -> Vec<String> {
    let graph = form_module_program_graph(&root.join("modules.wfg"), CompilerLimits::default())
        .unwrap_or_else(|_| panic!("{case}: the accepted program forms its graph"));
    let sources =
        discover_module_sources(root, &graph).unwrap_or_else(|failure| panic!("{case}: {failure}"));
    let inputs: Vec<_> = sources
        .iter()
        .map(|source| {
            SourceInput::new(&source.logical_path, &source.bytes)
                .in_module(source.module, source.role)
        })
        .collect();
    let inputs = super::super::with_library_records(&graph, &inputs);
    super::super::with_checked_program(
        &inputs,
        Some(graph.modules()),
        CompilerLimits::default(),
        |checked, _| Ok(checked.contradictory_successes(declared)),
    )
    .unwrap_or_else(|failure| panic!("{case}: the accepted program is checked: {failure}"))
}

/// Checks one module pair like a source pair and returns the kind its
/// rejection prints, or why the pair fails.
pub(super) fn check_module_pair(pair: &ModulePair) -> Result<String, String> {
    let name = pair.name;
    let root = scratch_copy(pair.case, pair.rejected);
    let rejected = reach(name, &root, pair.library);
    let _ = std::fs::remove_dir_all(&root);
    let failure = match rejected {
        Reached::Rejected(failure) => failure,
        Reached::Accepted { .. } => {
            return Err(format!("{name}: the pinned module program is accepted"));
        }
    };
    check_rejection(name, pair.rule, pair.sentences, &failure)?;
    if pair.repaired.is_empty() {
        return Err(format!("{name}: the pair has no accepted program"));
    }
    for (alternative, replacements) in pair.repaired.iter().enumerate() {
        let root = scratch_copy(pair.case, replacements);
        let outcome = match reach(name, &root, &[]) {
            Reached::Rejected(failure) => Err(format!(
                "{name}: alternative {alternative} is rejected:\n{failure}"
            )),
            Reached::Accepted { declared } => {
                let found = contradictions(name, &root, &declared);
                if found.is_empty() {
                    Ok(())
                } else {
                    Err(format!(
                        "{name}: alternative {alternative} succeeds only where its state is contradictory: {found:?}"
                    ))
                }
            }
        };
        let _ = std::fs::remove_dir_all(&root);
        outcome?;
    }
    Ok(failure.kind_name().unwrap_or_default().to_owned())
}

/// One repair pair [DIAG-1] on the standard library's graph record, which
/// only the toolchain's own library writes [MOD-10]: formed as the library's
/// graph, where `pkg` names the library itself.
pub(super) struct LibraryGraphPair {
    /// Names the pair in a failure.
    pub(super) name: &'static str,
    /// The graph record the rejection reads.
    pub(super) rejected: &'static [u8],
    /// The numbered rule the rejection must cite.
    pub(super) rule: &'static str,
    /// Exact substrings of the rendered rejection, as for a source pair.
    pub(super) sentences: &'static [&'static str],
    /// One graph record for each alternative.
    pub(super) repaired: &'static [&'static [u8]],
}

pub(super) const LIBRARY_GRAPH_PAIRS: &[LibraryGraphPair] = &[LibraryGraphPair {
    name: "probe-library-graph-names-std",
    rejected: b"pkg::io: [];\npkg::fs: [std::io];\n",
    rule: "MOD-10",
    sentences: &[],
    repaired: &[b"pkg::io: [];\npkg::fs: [pkg::io];\n"],
}];

/// Forms `bytes` as the standard library's graph record.
fn form_library_graph(bytes: &[u8]) -> Result<crate::ModuleGraph, CompilationFailure> {
    super::super::form_graph_record(
        SourceInput::new("std/modules.wfg", bytes),
        CompilerLimits::default(),
        crate::Package::Standard,
        None,
    )
}

/// Checks one library graph pair like a source pair and returns the kind its
/// rejection prints, or why the pair fails.
pub(super) fn check_library_graph_pair(pair: &LibraryGraphPair) -> Result<String, String> {
    let name = pair.name;
    let failure = match form_library_graph(pair.rejected) {
        Ok(_) => return Err(format!("{name}: the pinned library graph is accepted")),
        Err(failure) => failure,
    };
    check_rejection(name, pair.rule, pair.sentences, &failure)?;
    if pair.repaired.is_empty() {
        return Err(format!("{name}: the pair has no accepted program"));
    }
    for (alternative, bytes) in pair.repaired.iter().enumerate() {
        if let Err(failure) = form_library_graph(bytes) {
            return Err(format!(
                "{name}: alternative {alternative} is rejected:\n{failure}"
            ));
        }
    }
    Ok(failure.kind_name().unwrap_or_default().to_owned())
}
