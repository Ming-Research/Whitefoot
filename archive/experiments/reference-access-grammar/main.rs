//! Compare token grammars with the compiler's unchanged generator.
//! Raw token formation and canonical source bytes are separate: compact `*p`
//! does not form the prefix candidate's tokens under the current lexical rule.
//! Run from OWNERSHIP.md; retire this comparison when the selected spelling's
//! formal grammar and conformance cases supersede the candidate evidence.
#[path = "../../../compiler/src/syntax/grammar/generator.rs"]
mod generator;

const PLACE: &str = "place          := pbase psuffix*";
const BASE: &str =
    "pbase          := IDENT | \"deref\" \"(\" place \")\" | \"entry\" \"(\" IDENT \")\"";
const SUFFIX: &str =
    "psuffix        := \".\" IDENT | \".\" TYPEID \".\" IDENT | \"[\" atom range_tail? \"]\"";
const POSTFIX_BASE: &str = "pbase          := IDENT | \"entry\" \"(\" IDENT \")\"";

fn replace_once(source: String, from: &str, to: &str) -> String {
    assert_eq!(
        source.matches(from).count(),
        1,
        "baseline production changed: {from}"
    );
    source.replacen(from, to, 1)
}

fn candidate(source: &str, name: &str) -> String {
    let source = source.to_owned();
    if name == "baseline" {
        return source;
    }
    if name == "conflict-control" {
        return replace_once(
            source,
            BASE,
            "pbase          := IDENT | IDENT \".\" IDENT | \"deref\" \"(\" place \")\" | \"entry\" \"(\" IDENT \")\"",
        );
    }
    let source = replace_once(source, BASE, POSTFIX_BASE);
    match name {
        // The unchanged generator has no Caret terminal. Once removed from
        // pbase, Deref is an unused, disjoint fixed predicate: relabel it as
        // the proposed standalone `^` punctuation for this token-grammar
        // comparison only. This does not scan or accept caret source bytes.
        "caret" => replace_once(source, SUFFIX, &format!("{SUFFIX} | \"deref\"")),
        "caret-conflict-control" => replace_once(
            source,
            SUFFIX,
            &format!("{SUFFIX} | \"deref\" | \"deref\" \".\" IDENT"),
        ),
        "dot-star" => replace_once(source, SUFFIX, &format!("{SUFFIX} | \".\" \"*\"")),
        "arrow-step" => replace_once(source, SUFFIX, &format!("{SUFFIX} | \"->\"")),
        "arrow-members" => {
            let source = replace_once(
                source,
                PLACE,
                "place          := pbase psuffix* | \"deref\" \"(\" place \")\" (\"[\" atom range_tail? \"]\" psuffix*)?",
            );
            replace_once(
                source,
                SUFFIX,
                &format!("{SUFFIX} | \"->\" (IDENT | TYPEID \".\" IDENT)"),
            )
        }
        "arrow-selectors" | "arrow-prefix" => {
            let source = replace_once(
                source,
                PLACE,
                if name == "arrow-prefix" {
                    "place          := pbase psuffix* | \"*\" place"
                } else {
                    "place          := pbase psuffix* | \"deref\" \"(\" place \")\""
                },
            );
            replace_once(
                source,
                SUFFIX,
                &format!(
                    "{SUFFIX} | \"->\" (IDENT | TYPEID \".\" IDENT | \"[\" atom range_tail? \"]\")"
                ),
            )
        }
        "arrow-total" => replace_once(
            source,
            PLACE,
            "place          := pbase psuffix* (\"->\" (IDENT | TYPEID \".\" IDENT | \"[\" atom range_tail? \"]\") psuffix*)* \"->\"?",
        ),
        _ => panic!("unknown candidate: {name}"),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let spec_path = args
        .next()
        .expect("usage: reference-access-grammar SPEC [CANDIDATE]");
    let selected = args.next();
    assert!(args.next().is_none(), "too many arguments");
    let source = std::fs::read_to_string(spec_path).expect("read specification");
    let names = [
        "baseline",
        "dot-star",
        "arrow-step",
        "arrow-members",
        "arrow-selectors",
        "arrow-prefix",
        "arrow-total",
        "conflict-control",
        "caret",
        "caret-conflict-control",
    ];
    for name in names {
        if selected.as_deref().is_some_and(|selected| selected != name) {
            continue;
        }
        let text = candidate(&source, name);
        let result = std::panic::catch_unwind(|| generator::generate(name, &text));
        if name.ends_with("conflict-control") {
            let error =
                result.expect_err("negative control incorrectly admitted a conflicting grammar");
            let message = error
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| error.downcast_ref::<&str>().copied())
                .unwrap_or("");
            let production = if name == "caret-conflict-control" {
                "`psuffix`"
            } else {
                "`pbase`"
            };
            assert!(
                message.contains("[GRAM-1]") && message.contains(production),
                "negative control failed for an unrelated reason: {message}"
            );
            println!("{name}: expected GRAM-1 prediction conflict");
        } else {
            let table = result.unwrap_or_else(|error| std::panic::resume_unwind(error));
            assert!(!table.is_empty(), "empty generated grammar");
            if name == "caret" {
                assert!(
                    table.contains("FixedTerminal::Deref"),
                    "missing caret surrogate"
                );
                println!("caret: strong LL(2), using Deref as the fresh ^ predicate");
            } else {
                println!("{name}: strong LL(2)");
            }
        }
    }
    assert!(
        selected
            .as_deref()
            .is_none_or(|selected| names.contains(&selected)),
        "unknown candidate"
    );
}
