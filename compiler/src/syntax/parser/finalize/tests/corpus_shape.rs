//! The two conditional forms no corpus file may carry, asserted over the
//! corpus's own derivation trees.
//!
//! [GRAM-6] rejects a Bool-scrutinee `match` (spell `if`) and an `else` whose
//! block is exactly one `if_stmt` and nothing else (spell `else if`). Both
//! forms still *parse*, so neither the grammar nor the corpus round-trip gate
//! in `compiler/tests/canonical_corpus.rs` can see them, and a file carrying
//! one would sit in the corpus looking canonical.
//!
//! This is not redundant with the checker's own rejection. A file that stops
//! earlier for an unrelated reason never reaches the checker, so the
//! rejection never fires and the surviving form goes unnoticed. Asserting on
//! the tree instead of on the diagnostic removes that dependency.
//!
//! It lives in the library because `FinalizedTopology` is `pub(crate)`: a bin
//! and an integration test are each a separate crate and cannot reach the tree
//! at all. Being a library test also makes it a standing gate rather than a
//! one-shot check run once and discarded.

use std::path::{Path, PathBuf};

use crate::syntax::grammar::Production;
use crate::syntax::parser::{DerivationElement, ParseOutcome, parse};
use crate::{
    ACTIVE_KERNEL_SPEC_HASH, CompilerLimits, LexOutcome, SourceBundle, SourceInput,
    TerminalOutcome, classify_terminals, lex,
};

use super::super::topology::{FinalizedTopology, NodeId};
use super::super::{FinalizeOutcome, FinalizedBundle, finalize};

/// The Bool constructors. An arm naming one is an arm of a Bool `match`.
///
/// A tree carries no types, so this is the syntactic criterion the rule's type
/// judgment reduces to: an arm can only name `True` or `False` when the
/// scrutinee is the prelude Bool — unless the case exists precisely to be
/// rejected for naming them over something else, which is the one file below.
const BOOL_CONSTRUCTORS: [&[u8]; 2] = [b"True", b"False"];

/// The one corpus file that keeps a Bool-constructor arm by ruling.
///
/// `type5-neg-match-non-enum.wf` matches `True()` against an `i32`. Its
/// scrutinee is a scalar, so [GRAM-6] never applies and [TYPE-5] still fires as
/// the case records; the 2026-08-08 ruling examined it beside
/// `reject-err2-nonexhaustive.wf` and left it alone deliberately.
///
/// It is named rather than skipped by a pattern, and asserted to still be here
/// rather than merely permitted, so that neither a second such file nor the
/// silent disappearance of this one can pass unnoticed.
const RULED_BOOL_CONSTRUCTOR_ARMS: [&str; 1] = ["type5-neg-match-non-enum.wf"];

/// What one file's tree says about the two forbidden forms.
#[derive(Debug, Default, PartialEq, Eq)]
struct Forbidden {
    /// `match` arms naming a Bool constructor.
    bool_match_arms: usize,
    /// `else` blocks holding exactly one `if_stmt` and nothing else.
    unflattened_elses: usize,
}

impl Forbidden {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Derives `source` and reports the forbidden forms in its tree.
///
/// A source that reaches no tree is `None` rather than a panic: the corpus
/// holds programs that are rejected before parsing on purpose, and this test
/// does not own the judgment that every file derives —
/// `compiler/tests/canonical_corpus.rs` does.
fn forbidden_forms(source: &[u8]) -> Option<Forbidden> {
    let limits = CompilerLimits::default();
    let inputs = [SourceInput::new("corpus.wf", source)];
    let Ok(bundle) = SourceBundle::with_limits(&inputs, limits.source) else {
        return None;
    };
    let LexOutcome::Complete(lexed) = lex(&bundle, limits.lexer) else {
        return None;
    };
    let TerminalOutcome::Complete(classified) =
        classify_terminals(&lexed, ACTIVE_KERNEL_SPEC_HASH, limits.terminals)
    else {
        return None;
    };
    let ParseOutcome::Complete(parsed) = parse(classified, limits.parser) else {
        return None;
    };
    let FinalizeOutcome::Complete(finalized) = finalize(parsed, limits.finalizer) else {
        return None;
    };

    let mut found = Forbidden::default();
    for index in 0..finalized.topology.nodes.len() {
        let Some(id) = NodeId::from_index(index) else {
            continue;
        };
        let Some(record) = finalized.topology.node(id) else {
            continue;
        };
        match record.production {
            Production::MatchStmt | Production::ValueMatch => {
                found.bool_match_arms += bool_arms(&finalized, id);
            }
            Production::IfStmt | Production::ValueIf
                if holds_one_bare_if_in_its_else(&finalized.topology, id) =>
            {
                found.unflattened_elses += 1;
            }
            _ => {}
        }
    }
    Some(found)
}

/// Counts the arms of one `match` whose constructor is a Bool constructor.
///
/// `arm := TYPEID "(" fieldbind_list? ")" "=>" "{" stmt* "}"`, so the arm's
/// first terminal is the constructor name.
fn bool_arms(finalized: &FinalizedBundle, node: NodeId) -> usize {
    let Some(children) = finalized.topology.node_children(node) else {
        return 0;
    };
    let mut count = 0;
    for child in children {
        let Some(arm) = finalized.topology.node(*child) else {
            continue;
        };
        if arm.production != Production::Arm {
            continue;
        }
        let Some(name) = terminal_bytes(finalized, arm.first_terminal) else {
            continue;
        };
        if BOOL_CONSTRUCTORS.contains(&name) {
            count += 1;
        }
    }
    count
}

/// The source bytes of one terminal, addressed by its ordinal.
fn terminal_bytes(finalized: &FinalizedBundle, ordinal: u64) -> Option<&[u8]> {
    let index = usize::try_from(ordinal).ok()?;
    let record = finalized.topology.terminals.get(index)?;
    let element = finalized.parsed.tree.elements.get(record.element_index)?;
    let DerivationElement::Terminal { token, .. } = *element else {
        return None;
    };
    finalized.classified_bundle().token_bytes(token)
}

/// Whether this `if_stmt` or `value_if` owns a braced `else` whose whole
/// content is one `if_stmt`.
///
/// The flattened `else if` spelling puts the nested `if_stmt` directly under
/// the outer node with no second brace pair, so only the unflattened spelling
/// has an `else` brace pair at all. Inside it, "exactly one `if_stmt` and
/// nothing else" is one `stmt` child whose own child is an `if_stmt`.
///
/// A `value_if` whose else block is exactly one *else-free* `if_stmt` is
/// [GIVE-1]'s rejection rather than [GRAM-6]'s, but it is reported here too:
/// the assertion's job is that no corpus file carries a form some rule
/// rejects, and which rule owns it does not change that.
fn holds_one_bare_if_in_its_else(topology: &FinalizedTopology, node: NodeId) -> bool {
    let Some(record) = topology.node(node) else {
        return false;
    };
    let (Some(open), Some(close)) = (record.else_open, record.else_close) else {
        return false;
    };
    let Some(children) = topology.node_children(node) else {
        return false;
    };
    let mut inside = children.iter().filter(|child| {
        topology.node(**child).is_some_and(|candidate| {
            candidate.first_terminal > open
                && candidate.last_terminal().is_some_and(|last| last < close)
        })
    });
    let (Some(only), None) = (inside.next(), inside.next()) else {
        return false;
    };
    let Some(statement) = topology.node(*only) else {
        return false;
    };
    if statement.production != Production::Stmt {
        return false;
    }
    topology
        .node_children(*only)
        .and_then(<[NodeId]>::first)
        .and_then(|inner| topology.node(*inner))
        .is_some_and(|inner| inner.production == Production::IfStmt)
}

/// Every corpus `.wf` file, in one stable order.
fn corpus_files() -> Vec<PathBuf> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler package must live directly under the repository root");
    let roots = [
        repository.join("tests").join("conformance").join("cases"),
        repository.join("tests").join("programs"),
    ];
    let mut files = Vec::new();
    for root in roots {
        let entries = std::fs::read_dir(&root)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", root.display()));
        for entry in entries {
            let path = entry
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", root.display()))
                .path();
            if path.extension().is_some_and(|extension| extension == "wf") {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty(), "the corpus roots must hold programs");
    files
}

#[test]
fn the_detector_sees_both_forbidden_forms_and_neither_legal_neighbour() {
    // A Bool `match`, and the enum `match` that keeps its spelling.
    assert_eq!(
        forbidden_forms(
            // This control *is* the form under detection.
            b"fn main() -> status: std::process::ExitStatus pure {\n  let b = True();\n  match b {\n    True() => {\n    }\n    False() => {\n    }\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n"
        ),
        Some(Forbidden {
            bool_match_arms: 2,
            unflattened_elses: 0,
        })
    );
    assert_eq!(
        forbidden_forms(
            b"enum Colour {\n  Red();\n  Blue();\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let c = Colour::Red();\n  match c {\n    Red() => {\n    }\n    Blue() => {\n    }\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n"
        ),
        Some(Forbidden::default())
    );

    // The unflattened `else`, and the `else if` chain that replaces it.
    assert_eq!(
        forbidden_forms(
            b"fn main() -> status: std::process::ExitStatus pure {\n  let a = True();\n  let b = True();\n  if a {\n  } else {\n    if b {\n    }\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n"
        ),
        Some(Forbidden {
            bool_match_arms: 0,
            unflattened_elses: 1,
        })
    );
    assert_eq!(
        forbidden_forms(
            b"fn main() -> status: std::process::ExitStatus pure {\n  let a = True();\n  let b = True();\n  if a {\n  } else if b {\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n"
        ),
        Some(Forbidden::default())
    );

    // "and nothing else" is load-bearing: an `else` block holding an `if` plus
    // another statement cannot be spelled `else if` and is not this defect.
    assert_eq!(
        forbidden_forms(
            b"fn main() -> status: std::process::ExitStatus pure {\n  let a = True();\n  let b = True();\n  if a {\n  } else {\n    if b {\n    }\n    return std::process::exit_status(code: 0_u8);\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n"
        ),
        Some(Forbidden::default())
    );

    // An empty `else` is a different [GRAM-6] clause and not this one.
    assert_eq!(
        forbidden_forms(
            b"fn main() -> status: std::process::ExitStatus pure {\n  let a = True();\n  if a {\n  } else {\n  }\n  return std::process::exit_status(code: 0_u8);\n}\n"
        ),
        Some(Forbidden::default())
    );

    // A source that reaches no tree is skipped rather than counted.
    assert_eq!(forbidden_forms(b"fn main( {\n"), None);
}

#[test]
fn no_corpus_file_keeps_a_bool_match_or_an_unflattened_else() {
    let mut bool_matches = Vec::new();
    let mut unflattened = Vec::new();
    let mut derived = 0_usize;
    let mut underived = 0_usize;

    for path in corpus_files() {
        let source = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        let Some(found) = forbidden_forms(&source) else {
            underived += 1;
            continue;
        };
        derived += 1;
        if found.is_empty() {
            continue;
        }
        let name = path
            .file_name()
            .expect("a corpus entry has a file name")
            .to_string_lossy()
            .into_owned();
        if found.bool_match_arms > 0 {
            bool_matches.push(name.clone());
        }
        if found.unflattened_elses > 0 {
            unflattened.push(name);
        }
    }

    let census = format!("{derived} corpus files derived and {underived} did not");
    assert_eq!(
        bool_matches, RULED_BOOL_CONSTRUCTOR_ARMS,
        "{census}; exactly the ruled case may match a Bool constructor"
    );
    assert!(
        unflattened.is_empty(),
        "{census}; these keep the unflattened `else` [GRAM-6] rejects: {unflattened:#?}"
    );
}
