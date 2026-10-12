//! Temporary migration inventory for the [OP-5] condition redundancy rule
//! (status board item lm-r1). Remove it when that item's migration is done.
//!
//! `WHITEFOOT_REDUNDANT_CONDITIONS=<scratch TSV>` appends one row for every
//! condition the rule decides in a function the checker judges, including
//! functions that a rejection elsewhere in the same unit already fails, so
//! one gate run lists every site rather than the first one per unit. The
//! variable is read once per process; unset means no file. The verdict is
//! unchanged and output errors go to stderr.
//!
//! The headerless TSV columns are process ID, thread name (a Rust test's own
//! name under the test harness), the source's display path, line, column,
//! function name, the decided truth and the rendered condition. A unit
//! checked more than once writes its rows again; deduplicate before
//! counting.

use std::io::Write;
use std::sync::{Mutex, OnceLock};

use super::DeclarationInventory;
use crate::semantic::entailment::ConditionOutcome;
use crate::semantic::model::CheckedFunction;

static OUTPUT: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

fn output() -> Option<&'static Mutex<std::fs::File>> {
    OUTPUT
        .get_or_init(|| {
            std::env::var_os("WHITEFOOT_REDUNDANT_CONDITIONS").and_then(|path| {
                match std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    Ok(file) => Some(Mutex::new(file)),
                    Err(error) => {
                        eprintln!("redundant condition inventory unavailable: {error}");
                        None
                    }
                }
            })
        })
        .as_ref()
}

/// Appends the row of one decided condition when the inventory is enabled.
pub(super) fn record(
    declarations: &DeclarationInventory<'_>,
    function: &CheckedFunction,
    outcome: Option<&ConditionOutcome>,
) {
    let Some(output) = output() else {
        return;
    };
    let Some(outcome) = outcome else {
        return;
    };
    let Some(decided) = outcome.decided else {
        return;
    };
    let (path, line, column) =
        location(declarations, outcome).unwrap_or_else(|| ("?".to_owned(), 0, 0));
    let clean = |text: &str| text.replace(['\t', '\r', '\n'], " ");
    let thread = std::thread::current();
    let row = format!(
        "{}\t{}\t{}\t{line}\t{column}\t{}\t{}\t{}\n",
        std::process::id(),
        clean(thread.name().unwrap_or("?")),
        clean(&path),
        clean(&function.name),
        if decided { "True" } else { "False" },
        clean(&outcome.residual),
    );
    let written = output
        .lock()
        .map_err(|_| std::io::Error::other("poisoned"))
        .and_then(|mut file| file.write_all(row.as_bytes()));
    if let Err(error) = written {
        eprintln!("redundant condition inventory write failed: {error}");
    }
}

/// The display path and the one-based line and column of the condition.
fn location(
    declarations: &DeclarationInventory<'_>,
    outcome: &ConditionOutcome,
) -> Option<(String, u64, u64)> {
    let node = declarations.tree.node_with_path(&outcome.node_path)?;
    let coordinate = declarations.tree.coordinate(node).ok()?;
    let file = declarations
        .resolved
        .syntax()
        .classified_bundle()
        .source_bundle()
        .file(coordinate.source())?;
    let start = usize::try_from(coordinate.start().value()).ok()?;
    let before = file.bytes().get(..start)?;
    let line = before.iter().filter(|byte| **byte == b'\n').count() as u64 + 1;
    let column = before
        .rsplit(|byte| *byte == b'\n')
        .next()
        .map_or(0, <[u8]>::len) as u64
        + 1;
    Some((file.display_path().to_owned(), line, column))
}
