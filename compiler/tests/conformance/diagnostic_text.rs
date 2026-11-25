//! Probe: the words of every single-source conformance rejection, the kinds
//! the corpus reaches with the smallest case for each, and the defects of
//! their text.

use std::collections::BTreeMap;

use whitefoot::{CompilerLimits, DiagnosticFormat, SourceInput, check, rejection_kinds};

use super::corpus::{self, Expectation};

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
        if case.module_root().is_some() {
            module_form += 1;
            continue;
        }
        let source = case.source();
        let path = case.logical_path();
        let Err(failure) = check(&[SourceInput::new(&path, &source)], CompilerLimits::default())
        else {
            continue;
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
                .is_none_or(|(size, ..)| source.len() < *size);
            if better {
                smallest.insert(
                    kind.to_owned(),
                    (
                        source.len(),
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
        "module-form reject cases skipped: {module_form}\nlisted kinds: {}; reached by single-source cases: {}\nlisted and unreached: {unreached:?}\nreached and unlisted: {unlisted:?}\n",
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
