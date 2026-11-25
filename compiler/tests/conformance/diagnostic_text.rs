//! Probe: the words of every single-source conformance rejection, the kinds
//! the corpus reaches with the smallest case for each, and the defects of
//! their text.

use std::collections::BTreeMap;

use whitefoot::{
    CompilationFailure, CompilerLimits, DiagnosticFormat, ModuleEntry, ModuleProgramFailure,
    SourceInput, check, check_module_entry, check_module_program, discover_module_sources,
    form_module_program_graph, rejection_kinds,
};

use super::corpus::{self, Expectation};

/// The rejection a module-form case reaches through the ordinary module path.
fn module_failure(case: &corpus::Case, root: &std::path::Path) -> Option<CompilationFailure> {
    let graph = match form_module_program_graph(&root.join("modules.wfg"), CompilerLimits::default())
    {
        Ok(graph) => graph,
        Err(ModuleProgramFailure::Compilation(failure)) => return Some(failure),
        Err(ModuleProgramFailure::Discovery(failure)) => panic!("{}: {failure}", case.id),
    };
    let sources = discover_module_sources(root, &graph)
        .unwrap_or_else(|failure| panic!("{}: {failure}", case.id));
    let inputs: Vec<_> = sources
        .iter()
        .map(|source| {
            SourceInput::new(&source.logical_path, &source.bytes)
                .in_module(source.module, source.role)
        })
        .collect();
    if let Err(failure) = check_module_program(&graph, &inputs, CompilerLimits::default()) {
        return Some(failure);
    }
    for entry in graph.entries() {
        if let Err(failure) = check_module_entry(
            &graph,
            &inputs,
            ModuleEntry::Named(entry.name()),
            CompilerLimits::default(),
        ) {
            return Some(failure);
        }
    }
    None
}

#[test]
fn probe_rejection_text_and_kinds() {
    let mut defects = Vec::new();
    // kind -> (source bytes, case id, rule, repair lines)
    let mut smallest: BTreeMap<String, (usize, String, String, Vec<String>)> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut module_form = 0usize;
    for case in corpus::load() {
        if !matches!(
            case.expect,
            Expectation::Reject(_) | Expectation::RejectDiagnostics(_)
        ) {
            continue;
        }
        let (failure, size) = if let Some(root) = case.module_root() {
            module_form += 1;
            match module_failure(&case, &root) {
                Some(failure) => (failure, usize::MAX / 2),
                None => continue,
            }
        } else {
            let source = case.source();
            let path = case.logical_path();
            match check(&[SourceInput::new(&path, &source)], CompilerLimits::default()) {
                Err(failure) => (failure, source.len()),
                Ok(()) => continue,
            }
        };
        for diagnostic in failure.diagnostics() {
            for defect in diagnostic.text_defects() {
                defects.push(format!("{}: {defect}", case.id));
            }
            let Some(kind) = diagnostic.kind_name() else {
                continue;
            };
            *counts.entry(kind.to_owned()).or_default() += 1;
            let text = diagnostic.render_diagnostic(DiagnosticFormat::Text);
            let fixes: Vec<String> = text
                .lines()
                .filter(|line| line.starts_with("  mechanical_fix:"))
                .map(|line| line.trim().to_owned())
                .collect();
            let better = smallest
                .get(kind)
                .is_none_or(|(size_so_far, ..)| size < *size_so_far);
            if better {
                smallest.insert(
                    kind.to_owned(),
                    (
                        size,
                        case.id.clone(),
                        diagnostic.rule_id().unwrap_or("-").to_owned(),
                        fixes,
                    ),
                );
            }
        }
    }
    let listed = rejection_kinds();
    let unreached: Vec<&&str> = listed
        .iter()
        .filter(|kind| !counts.contains_key(**kind))
        .collect();
    let unlisted: Vec<&String> = counts
        .keys()
        .filter(|kind| !listed.contains(&kind.as_str()))
        .collect();
    let mut report = format!(
        "module-form reject cases read: {module_form}\nlisted kinds: {}; reached by the cases: {}\nlisted and unreached: {unreached:?}\nreached and unlisted: {unlisted:?}\n",
        listed.len(),
        counts.len()
    );
    report.push_str("kind | count | smallest case | rule | repair lines\n");
    for (kind, (_, id, rule, fixes)) in &smallest {
        report.push_str(&format!("{kind} | {} | {id} | {rule} | {fixes:?}\n", counts[kind]));
    }
    report.push_str(&format!("text defects ({}):\n", defects.len()));
    for defect in defects.iter().take(300) {
        report.push_str(&format!("  {defect}\n"));
    }
    panic!("{report}");
}
