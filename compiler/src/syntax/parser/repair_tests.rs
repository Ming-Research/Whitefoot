//! Attribution and position controls for syntax repair hints.

use super::*;

fn outcome(source: &[u8], graph: bool) -> ParseOutcome {
    let inputs = [SourceInput::new(
        if graph { "modules.wfg" } else { "hints.wf" },
        source,
    )];
    let bundle = bundle(&inputs);
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("repair fixture must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits { max_tokens: 1024 },
    ) else {
        panic!("repair fixture must classify");
    };
    if graph {
        crate::syntax::parser::parse_graph(classified, PARSE_LIMITS)
    } else {
        parse(classified, PARSE_LIMITS)
    }
}

#[test]
fn nested_infix_operand_uses_gram9_and_spans_its_two_token_start() {
    for operator in ["+wrap", "<"] {
        let source =
            format!("fn f() -> r: u32 pure {{ return 0_u32 +wrap 1_u32 {operator} 2_u32; }}");
        let result = outcome(source.as_bytes(), false);
        let ParseOutcome::SourceIssue(issue) = result else {
            panic!("nested operation must reject: {result:?}");
        };
        assert_eq!(issue.rule(), SyntaxRule::Gram9);
        let start = usize::try_from(issue.coordinate().start().value()).expect("small source");
        let end = usize::try_from(issue.coordinate().end().value()).expect("small source");
        assert_eq!(&source[start..end], format!("1_u32 {operator}"));
        assert_eq!(
            issue.mechanical_fix,
            Some(
                "bind each operation with its own preceding `let` and pass the binding as the operand; an expression has one operation over two atoms, with no chaining or parenthesized subexpressions [GRAM-6, GRAM-9]"
            )
        );
    }
}

#[test]
fn expression_hints_do_not_attach_to_function_header_errors() {
    for source in [
        b"fn f[() -> r: u32 pure { return 0_u32; }".as_slice(),
        b"fn f() -> r: u32 pure + { return 0_u32; }".as_slice(),
    ] {
        let result = outcome(source, false);
        let ParseOutcome::SourceIssue(issue) = result else {
            panic!("malformed header must reject: {result:?}");
        };
        assert_eq!(issue.mechanical_fix, None);
    }
}

#[test]
fn graph_row_hint_gives_complete_declaration_forms() {
    let result = outcome(b"module geometry {", true);
    let ParseOutcome::SourceIssue(issue) = result else {
        panic!("foreign graph row must reject: {result:?}");
    };
    assert_eq!(
        issue.mechanical_fix,
        Some(
            "write graph declarations as `package name = \"location\";`, then module rows `module_path: [dependency_path, ...];`, then entries `entry name = module_path;`; examples are `package util = \"../util\";`, `pkg::geometry: [];`, and `entry app = pkg::geometry::main;` [GRAM-2]"
        )
    );
    assert!(matches!(outcome(b"package util = \"../util\";\n\npkg::geometry: [];\n\nentry app = pkg::geometry::main;\n", true), ParseOutcome::Complete(_)));
}

#[test]
fn entry_prefix_hint_names_the_graph_declaration() {
    let result = outcome(b"entry fn main() -> r: u32 pure { return 0_u32; }", false);
    let ParseOutcome::SourceIssue(issue) = result else {
        panic!("entry modifier must reject: {result:?}");
    };
    assert_eq!(
        issue.mechanical_fix,
        Some(
            "remove `entry` before `fn`; declare the entry in the module graph file, for example `entry app = pkg::main;` [GRAM-2, MOD-1]"
        )
    );
    assert!(matches!(
        outcome(b"fn main() -> r: u32 pure { return 0_u32; }", false),
        ParseOutcome::Complete(_)
    ));
    assert!(matches!(
        outcome(b"pkg: [];\n\nentry app = pkg::main;\n", true),
        ParseOutcome::Complete(_)
    ));
}

#[test]
fn subscript_offset_does_not_inherit_its_enclosing_infix_operand_role() {
    let source = b"fn f() -> r: u32 pure { return 0_u32 +wrap values[0_u64 +wrap 1_u64]; }";
    let result = outcome(source, false);
    let ParseOutcome::SourceIssue(issue) = result else {
        panic!("operation in subscript offset must reject: {result:?}");
    };
    assert_eq!(issue.rule(), SyntaxRule::Gram5);
    let start = usize::try_from(issue.coordinate().start().value()).expect("small source");
    let end = usize::try_from(issue.coordinate().end().value()).expect("small source");
    assert_eq!(&source[start..end], b"+wrap");
}

#[test]
fn bracket_in_a_call_type_argument_does_not_suggest_an_expression_list() {
    let result = outcome(b"fn f() -> r: u32 pure { return value::<[u8]>(); }", false);
    let ParseOutcome::SourceIssue(issue) = result else {
        panic!("unsupported type form must reject: {result:?}");
    };
    assert_eq!(issue.mechanical_fix, None);
}

#[test]
fn result_route_does_not_offer_match_arm_omission_syntax() {
    let source = b"fn f() -> r: Option<u32> pure contract { ensures when Some(x): x == 0_u32; } { return Some<u32>(value: 0_u32); }";
    let result = outcome(source, false);
    let ParseOutcome::SourceIssue(issue) = result else {
        panic!("positional result route must reject: {result:?}");
    };
    let start = usize::try_from(issue.coordinate().start().value()).expect("small source");
    let end = usize::try_from(issue.coordinate().end().value()).expect("small source");
    assert_eq!(&source[start..end], b")");
    assert!(issue.expected.iter().any(|predicate| {
        predicate
            == crate::syntax::grammar::LookaheadPredicate::Terminal(
                crate::syntax::terminal::TerminalPredicate::Fixed(
                    crate::syntax::terminal::FixedTerminal::Colon,
                ),
            )
    }));
    assert_eq!(issue.mechanical_fix, None);
}
