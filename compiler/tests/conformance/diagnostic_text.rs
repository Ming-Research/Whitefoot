//! Probe: the payload text of every single-source conformance rejection,
//! scanned for `?` placeholders outside quoted source, and the set of
//! rejection kinds the corpus reaches.

use std::collections::{BTreeMap, BTreeSet};

use whitefoot::{CompilerLimits, DiagnosticFormat, SourceInput, check};

use super::corpus::{self, Expectation};
use super::json;

/// The payload lines `label: value` of one rendered rejection, with each
/// double-quoted stretch (exact source) removed.
fn prose_lines(rendered: &str) -> Vec<String> {
    rendered
        .lines()
        .filter(|line| {
            line.starts_with("  ")
                && line[2..]
                    .split_once(": ")
                    .is_some_and(|(label, _)| {
                        !label.is_empty()
                            && label.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                    })
        })
        .map(|line| {
            let mut kept = String::new();
            let mut quoted = false;
            let mut escaped = false;
            for character in line.chars() {
                if quoted {
                    if escaped {
                        escaped = false;
                    } else if character == '\\' {
                        escaped = true;
                    } else if character == '"' {
                        quoted = false;
                    }
                } else if character == '"' {
                    quoted = true;
                } else {
                    kept.push(character);
                }
            }
            kept
        })
        .collect()
}

#[test]
fn probe_rejection_text_and_kinds() {
    let mut placeholders = Vec::new();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut rejected = 0usize;
    let mut skipped = BTreeSet::new();
    for case in corpus::load() {
        if !matches!(
            case.expect,
            Expectation::Reject(_) | Expectation::RejectDiagnostics(_)
        ) {
            continue;
        }
        if case.module_root().is_some() {
            skipped.insert(case.id.clone());
            continue;
        }
        let source = case.source();
        let path = case.logical_path();
        let Err(failure) = check(&[SourceInput::new(&path, &source)], CompilerLimits::default())
        else {
            continue;
        };
        rejected += 1;
        for diagnostic in failure.diagnostics() {
            let value = json::parse(&diagnostic.render_diagnostic(DiagnosticFormat::Json))
                .expect("structured JSON");
            if let Some(kind) = value.get("kind").and_then(json::Value::string) {
                *kinds.entry(kind.to_owned()).or_default() += 1;
            }
            let text = diagnostic.render_diagnostic(DiagnosticFormat::Text);
            for line in prose_lines(&text) {
                if line.contains('?') {
                    placeholders.push(format!("{}: {}", case.id, line.trim()));
                }
            }
        }
    }
    let mut report = format!(
        "rejected single-source cases: {rejected}; module-form skipped: {}\nkinds reached ({}):\n",
        skipped.len(),
        kinds.len()
    );
    for (kind, count) in &kinds {
        report.push_str(&format!("  {kind} {count}\n"));
    }
    report.push_str(&format!("placeholder lines ({}):\n", placeholders.len()));
    for line in &placeholders {
        report.push_str(&format!("  {line}\n"));
    }
    panic!("{report}");
}
