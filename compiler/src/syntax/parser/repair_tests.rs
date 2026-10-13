//! Attribution and position controls for syntax repair hints.

use super::*;

fn outcome(source: &[u8], graph: bool) -> ParseOutcome {
    outcome_from(
        source,
        if graph {
            Production::GraphFile
        } else {
            Production::Program
        },
    )
}

fn outcome_from(source: &[u8], entry: Production) -> ParseOutcome {
    let inputs = [SourceInput::new(
        if entry == Production::Program {
            "hints.wf"
        } else {
            "modules.wfg"
        },
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
    match entry {
        Production::Program => parse(classified, PARSE_LIMITS),
        Production::GraphFile => crate::syntax::parser::parse_graph(classified, PARSE_LIMITS),
        Production::GraphSegment => {
            crate::syntax::parser::parse_graph_segment(classified, PARSE_LIMITS)
        }
        _ => panic!("repair fixture has an unsupported entry production"),
    }
}

#[test]
fn infix_in_named_and_positional_arguments_keeps_gram5_and_the_flat_repair() {
    for argument in ["value: 1_u32", "1_u32"] {
        for operator in ["+wrap", "<"] {
            let source = format!(
                "fn f() -> r: u32 pure {{ return identity({argument} {operator} 2_u32); }}"
            );
            let result = outcome(source.as_bytes(), false);
            let ParseOutcome::SourceIssue(issue) = result else {
                panic!("operation in argument must reject: {result:?}");
            };
            // The atom has completed; the argument list admits only its next
            // comma or the call's closing parenthesis. DIAG-1 row 2 attributes
            // nested infix starts only in infix operands, not call arguments.
            assert_eq!(issue.rule(), SyntaxRule::Gram5);
            assert_eq!(issue_bytes(source.as_bytes(), issue), operator.as_bytes());
            assert!(issue.expected.iter().eq([
                crate::syntax::grammar::LookaheadPredicate::Terminal(
                    crate::syntax::terminal::TerminalPredicate::Fixed(
                        crate::syntax::terminal::FixedTerminal::RightParen,
                    ),
                ),
                crate::syntax::grammar::LookaheadPredicate::Terminal(
                    crate::syntax::terminal::TerminalPredicate::Fixed(
                        crate::syntax::terminal::FixedTerminal::Comma,
                    ),
                ),
            ]));
            assert_eq!(
                issue.mechanical_fix,
                Some(crate::diagnostic_repairs::FLAT_OPERATION_FIX)
            );
        }
    }
}

#[test]
fn nested_infix_operand_uses_gram9_and_spans_its_two_token_start() {
    for operator in ["+wrap", "<"] {
        for statement in ["return", "let result ="] {
            let source = format!(
                "fn f() -> r: u32 pure {{ {statement} 0_u32 +wrap 1_u32 {operator} 2_u32; }}"
            );
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
}

#[test]
fn completed_infix_operand_keeps_its_contract_repair() {
    let source = b"fn f() -> r: u32 pure contract { define value = 0_u32 +wrap 1_u32 +wrap 2_u32; } { return 0_u32; }";
    let result = outcome(source, false);
    let ParseOutcome::SourceIssue(issue) = result else {
        panic!("nested contract operation must reject: {result:?}");
    };
    assert_eq!(issue.rule(), SyntaxRule::Gram9);
    assert_eq!(issue_bytes(source, issue), b"1_u32 +wrap");
    assert_eq!(
        issue.mechanical_fix,
        Some(crate::diagnostic_repairs::FLAT_CONTRACT_OPERATION_FIX)
    );
}

#[test]
fn nested_call_in_a_definition_keeps_the_definition_repair() {
    let source = b"fn f(value: u8) -> r: u64 pure contract { define wide = cvt::<u32, u64>(cvt::<u8, u32>(value)); ensures r == wide; } { return cvt::<u8, u64>(value); }";
    let result = outcome(source, false);
    let ParseOutcome::SourceIssue(issue) = result else {
        panic!("nested definition call must reject: {result:?}");
    };
    assert_eq!(issue.rule(), SyntaxRule::Gram9);
    assert_eq!(issue_bytes(source, issue), b"cvt::");
    assert_eq!(
        issue.mechanical_fix,
        Some(crate::diagnostic_repairs::GRAM9_CONTRACT_FIX)
    );
}

#[test]
fn marked_call_starts_keep_atom_attribution_and_frontier_expectations() {
    for marker in ["musttail", "spawn"] {
        for (callee, head) in [
            ("inner()", "inner"),
            ("ineg.checked(0_i32)", "ineg.checked"),
            ("Packet(value: 0_u32)", "Packet"),
            ("pkg::inner()", "pkg"),
            ("std::inner()", "std"),
        ] {
            for (before, after) in [
                ("outer(", ");"),
                ("outer(0_u32, ", ");"),
                ("outer(value: ", ");"),
                ("return 0_u32 +wrap ", ";"),
                ("return values[", "];"),
                ("for (index in ", "..1_u64) {}"),
                ("for (index in 0_u64..", ") {}"),
            ] {
                let source = format!("fn f() -> r: u32 pure {{ {before}{marker} {callee}{after} }}");
                let result = outcome(source.as_bytes(), false);
                let ParseOutcome::SourceIssue(issue) = result else {
                    panic!("marked call in atom position must reject: {source}: {result:?}");
                };
                assert_eq!(issue.rule(), SyntaxRule::Gram9, "{source}");
                assert_eq!(
                    issue_bytes(source.as_bytes(), issue),
                    format!("{marker} {head}").as_bytes(),
                    "{source}"
                );
                // Hoisting musttail to a let fails FN-10; spawn admission
                // needs the callee's WAIT-3 conditions. Neither has an
                // established repair at this syntax-only stopping point.
                assert_eq!(issue.mechanical_fix, None, "{source}");

                // A noncallee follower reaches the same maximal-prefix
                // frontier, but row 2 does not apply. Attribution must retain
                // that frontier's expected set rather than inventing a new one.
                let control = format!("fn f() -> r: u32 pure {{ {before}{marker} 0_u32{after} }}");
                let result = outcome(control.as_bytes(), false);
                let ParseOutcome::SourceIssue(control_issue) = result else {
                    panic!("marked noncall must reject: {control}: {result:?}");
                };
                assert_eq!(control_issue.rule(), SyntaxRule::Form3, "{control}");
                assert_eq!(
                    issue_bytes(control.as_bytes(), control_issue),
                    marker.as_bytes(),
                    "{control}"
                );
                assert!(
                    issue.expected.iter().eq(control_issue.expected.iter()),
                    "{source}"
                );
            }
        }
    }
}

#[test]
fn marked_atom_starts_require_a_callee_head() {
    for marker in ["musttail", "spawn"] {
        for follower in [
            "0_u32",
            "unit",
            "\"text\"",
            "move",
            "entry",
            "&",
            "(",
            "::",
            "<",
            "musttail",
            "spawn",
            "@label",
            "+wrap",
            "",
        ] {
            let source = format!("fn f() -> r: unit pure {{ outer({marker} {follower}); }}");
            let result = outcome(source.as_bytes(), false);
            let ParseOutcome::SourceIssue(issue) = result else {
                panic!("noncallee head must reject: {source}: {result:?}");
            };
            // FIRST(callee) excludes these predicates; the fixed lowercase
            // marker still fails the expected IDENT's spelling restrictions.
            assert_eq!(issue.rule(), SyntaxRule::Form3, "{source}");
            assert_eq!(
                issue_bytes(source.as_bytes(), issue),
                marker.as_bytes(),
                "{source}"
            );
            assert_eq!(issue.mechanical_fix, None, "{source}");
        }
    }
}

#[test]
fn a_marker_after_an_atom_prefix_does_not_start_another_atom() {
    for marker in ["musttail", "spawn"] {
        for (prefix, rule) in [
            ("value", SyntaxRule::Gram5),
            ("&", SyntaxRule::Form3),
            ("move", SyntaxRule::Form3),
        ] {
            let source = format!("fn f() -> r: unit pure {{ outer({prefix} {marker} inner()); }}");
            let result = outcome(source.as_bytes(), false);
            let ParseOutcome::SourceIssue(issue) = result else {
                panic!("marker after atom prefix must reject: {source}: {result:?}");
            };
            // DIAG-1 row 2 examines the atom occurrence's start, whose
            // first token is the prefix above. The marker at position 1
            // remains the ordinary frontier defect rather than a new atom.
            assert_eq!(issue.rule(), rule, "{source}");
            assert_eq!(
                issue_bytes(source.as_bytes(), issue),
                marker.as_bytes(),
                "{source}"
            );
            assert_eq!(issue.mechanical_fix, None, "{source}");
        }
    }
}

#[test]
fn marked_calls_outside_atom_only_positions_keep_their_derivations() {
    for marker in ["musttail", "spawn"] {
        for callee in [
            "inner()",
            "ineg.checked(0_i32)",
            "Packet(value: 0_u32)",
            "pkg::inner()",
            "std::inner()",
        ] {
            let source = format!("fn f() -> r: u32 pure {{ return {marker} {callee}; }}");
            assert!(
                matches!(outcome(source.as_bytes(), false), ParseOutcome::Complete(_)),
                "{source}"
            );
        }
    }
}

#[test]
fn range_and_slice_endpoints_attribute_calls_to_gram9() {
    for prefix in ["", "musttail ", "spawn "] {
        for (before, after) in [
            (
                "fn f() -> r: unit pure contract { requires forall each(i in ",
                "..1_u64): i == i; } { return unit; }",
            ),
            (
                "fn f() -> r: unit pure contract { requires forall each(i in 0_u64..",
                "): i == i; } { return unit; }",
            ),
            (
                "fn f() -> r: unit pure { let slice = &values[0_u64..",
                "]; return unit; }",
            ),
        ] {
            let source = format!("{before}{prefix}weigh(){after}");
            let result = outcome(source.as_bytes(), false);
            let ParseOutcome::SourceIssue(issue) = result else {
                panic!("call in atom-only endpoint must reject: {source}: {result:?}");
            };
            assert_eq!(issue.rule(), SyntaxRule::Gram9, "{source}");
            let start = if prefix.is_empty() {
                "weigh(".to_owned()
            } else {
                format!("{prefix}weigh")
            };
            assert_eq!(
                issue_bytes(source.as_bytes(), issue),
                start.as_bytes(),
                "{source}"
            );
        }
    }
}

#[test]
fn affine_factor_calls_keep_their_derivations_and_callee_attribution() {
    for prefix in ["", "musttail ", "spawn "] {
        let source = format!(
            "fn f() -> r: unit pure contract {{ requires {prefix}weigh() == 0_u64; }} {{ return unit; }}"
        );
        assert!(
            matches!(outcome(source.as_bytes(), false), ParseOutcome::Complete(_)),
            "{source}"
        );
    }

    // These call alternatives are legal grammar starts in affine_factor.
    // A malformed qualification must reach its callee's expected IDENT,
    // rather than letting the atom alternative claim the call as GRAM-9.
    for callee in ["weigh::)", "pkg::)", "std::)"] {
        let source = format!(
            "fn f() -> r: unit pure contract {{ requires {callee} == 0_u64; }} {{ return unit; }}"
        );
        let result = outcome(source.as_bytes(), false);
        let ParseOutcome::SourceIssue(issue) = result else {
            panic!("malformed affine callee must reject: {source}: {result:?}");
        };
        assert_eq!(issue.rule(), SyntaxRule::Gram5, "{source}");
        assert_eq!(issue_bytes(source.as_bytes(), issue), b")", "{source}");
    }
}

#[test]
fn marked_contract_calls_do_not_offer_a_definition_hoist() {
    for marker in ["musttail", "spawn"] {
        for clause in ["define value =", "requires"] {
            let source = format!(
                "fn f() -> r: u32 pure contract {{ {clause} outer({marker} inner()); }} {{ return 0_u32; }}"
            );
            let result = outcome(source.as_bytes(), false);
            let ParseOutcome::SourceIssue(issue) = result else {
                panic!("marked contract argument must reject: {source}: {result:?}");
            };
            assert_eq!(issue.rule(), SyntaxRule::Gram9, "{source}");
            assert_eq!(
                issue_bytes(source.as_bytes(), issue),
                format!("{marker} inner").as_bytes(),
                "{source}"
            );
            assert_eq!(issue.mechanical_fix, None, "{source}");
        }
    }
}

#[test]
fn contract_definition_does_not_offer_to_hoist_a_constructor() {
    for constructor in ["Some<u8>(value: value)", "Packet(value: value)"] {
        let source = format!(
            "fn f(value: u8) -> r: u8 pure contract {{ define inner = inspect({constructor}); requires value == 0_u8; }} {{ return value; }}"
        );
        let result = outcome(source.as_bytes(), false);
        let ParseOutcome::SourceIssue(issue) = result else {
            panic!("nested constructor must reject: {result:?}");
        };
        assert_eq!(issue.rule(), SyntaxRule::Gram9);
        assert_eq!(issue.mechanical_fix, None);
    }
}

#[test]
fn contract_clauses_do_not_hoist_values_into_definition_scope() {
    for clause in [
        "requires cvt::<u32, u64>(cvt::<u8, u32>(value)) == 0_u64;",
        "requires cvt::<u32, u64>(value: cvt::<u8, u32>(value)) == 0_u64;",
        "ensures cvt::<u32, u64>(cvt::<u8, u32>(r)) == 0_u64;",
        "ensures when Some(value: payload): cvt::<u32, u64>(cvt::<u8, u32>(payload)) == 0_u64;",
        "ensures when r is Some(value: payload): cvt::<u32, u64>(cvt::<u8, u32>(payload)) == 0_u64;",
        "requires forall each(i in 0_u64..1_u64): values^[cvt::<u64, u64>(i)] == 0_u64;",
    ] {
        // A preceding completed definition must not leak its context flag
        // into the clause. These are parser controls, not typed programs.
        let source = format!(
            "fn f(value: u8) -> r: u8 pure contract {{ define first = 0_u64; {clause} }} {{ return value; }}"
        );
        let result = outcome(source.as_bytes(), false);
        let ParseOutcome::SourceIssue(issue) = result else {
            panic!("nested clause call must reject: {result:?}");
        };
        assert_eq!(issue.rule(), SyntaxRule::Gram9, "{clause}");
        assert_eq!(issue_bytes(source.as_bytes(), issue), b"cvt::", "{clause}");
        assert_eq!(issue.mechanical_fix, None, "{clause}");
    }
}

#[test]
fn routed_clause_affine_arithmetic_is_not_a_flat_expression_error() {
    let source = b"fn f(value: u8) -> r: Option<u8> pure contract { ensures when Some(value: payload): payload + 1_u8 + 2_u8 == value; } { return Some<u8>(value: value); }";
    assert!(matches!(outcome(source, false), ParseOutcome::Complete(_)));
}

#[test]
fn completed_infix_operand_does_not_reach_a_later_statement() {
    let source = b"fn f() -> r: u32 pure { let value = 0_u32 +wrap 1_u32; return +wrap 2_u32; }";
    let result = outcome(source, false);
    let ParseOutcome::SourceIssue(issue) = result else {
        panic!("missing expression atom must reject: {result:?}");
    };
    assert_ne!(issue.rule(), SyntaxRule::Gram9);
    assert_eq!(issue_bytes(source, issue), b"+wrap");
}

#[test]
fn completed_infix_operand_keeps_the_two_token_start_restriction() {
    let source = b"fn f() -> r: u32 pure { return 0_u32 +wrap value.field +wrap 2_u32; }";
    let result = outcome(source, false);
    let ParseOutcome::SourceIssue(issue) = result else {
        panic!("chained operation must reject: {result:?}");
    };
    // Its atom occurrence starts with `value .`, not `value infix_op`, so
    // DIAG-1 row 2 cannot replace the ordinary frontier's attribution.
    assert_ne!(issue.rule(), SyntaxRule::Gram9);
    assert_eq!(issue_bytes(source, issue), b"+wrap");
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
    for entry in [Production::GraphFile, Production::GraphSegment] {
        let result = outcome_from(b"module geometry {", entry);
        let ParseOutcome::SourceIssue(issue) = result else {
            panic!("foreign graph row must reject: {result:?}");
        };
        assert_eq!(issue.rule(), SyntaxRule::Gram2);
        assert_eq!(issue_bytes(b"module geometry {", issue), b"geometry");
        assert_eq!(
            issue.mechanical_fix,
            Some(
                "write graph declarations as `package name = \"location\";`, then module rows `module_path: [dependency_path, ...];`, then entries `entry name = module_path;`; examples are `package util = \"../util\";`, `pkg::geometry: [];`, and `entry app = pkg::geometry::main;` [GRAM-2]"
            ),
            "{entry:?}"
        );
    }
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
fn call_type_argument_does_not_suggest_an_expression_repair() {
    for source in [
        b"fn f() -> r: u32 pure { return value::<[u8]>(); }".as_slice(),
        b"fn f() -> r: u32 pure { return value::<u32 +wrap u32>(); }".as_slice(),
    ] {
        let result = outcome(source, false);
        let ParseOutcome::SourceIssue(issue) = result else {
            panic!("unsupported type form must reject: {result:?}");
        };
        assert_eq!(issue.mechanical_fix, None);
    }
}

#[test]
fn result_route_does_not_offer_match_arm_omission_syntax() {
    for result_selector in ["", "r is "] {
        let source = format!(
            "fn f() -> r: Option<u32> pure contract {{ ensures when {result_selector}Some(x): x == 0_u32; }} {{ return Some<u32>(value: 0_u32); }}"
        );
        let result = outcome(source.as_bytes(), false);
        let ParseOutcome::SourceIssue(issue) = result else {
            panic!("positional result route must reject: {result:?}");
        };
        assert_eq!(issue.rule(), SyntaxRule::Gram4);
        assert_eq!(issue_bytes(source.as_bytes(), issue), b")");
        assert!(issue.expected.iter().eq([
            crate::syntax::grammar::LookaheadPredicate::Terminal(
                crate::syntax::terminal::TerminalPredicate::Fixed(
                    crate::syntax::terminal::FixedTerminal::Colon,
                ),
            ),
        ]));
        assert_eq!(issue.mechanical_fix, None);
    }
}

#[test]
fn ordinary_contract_conjunctions_do_not_offer_a_definition_hoist() {
    for clause in [
        "requires value == 0_u8 && value == 0_u8;",
        "ensures r == value && r == value;",
        "ensures when Some(value: payload): payload == value && payload == value;",
        "ensures when r is Some(value: payload): payload == value && payload == value;",
    ] {
        let source = format!(
            "fn f(value: u8) -> r: u8 pure contract {{ define first = 0_u64; {clause} }} {{ return value; }}"
        );
        let result = outcome(source.as_bytes(), false);
        let ParseOutcome::SourceIssue(issue) = result else {
            panic!("clause conjunction must reject: {result:?}");
        };
        assert_eq!(issue_bytes(source.as_bytes(), issue), b"&", "{clause}");
        assert_eq!(issue.mechanical_fix, None, "{clause}");
    }
}

#[test]
fn definition_conjunction_keeps_the_applicable_boolean_repair() {
    let source = b"fn f(a: Bool, b: Bool) -> r: unit pure contract { define combined = a && b; requires combined; } { return unit; }";
    let result = outcome(source, false);
    let ParseOutcome::SourceIssue(issue) = result else {
        panic!("definition conjunction must reject: {result:?}");
    };
    assert_eq!(issue_bytes(source, issue), b"&");
    assert_eq!(
        issue.mechanical_fix,
        Some(crate::diagnostic_repairs::BOOLEAN_OPERATION_FIX)
    );
}

#[test]
fn routed_range_conjunction_keeps_the_range_repair() {
    for result_selector in ["", "r is "] {
        for relations in [
            "when payload is True && flag is True: payload is True",
            "when payload is True: payload is True && flag is True",
        ] {
            let source = format!(
                "fn f(flag: Bool) -> r: Option<Bool> pure contract {{ ensures when {result_selector}Some(value: payload): forall each(i in 0_u64 .. 1_u64) {relations}; }} {{ return Some<Bool>(value: flag); }}"
            );
            let result = outcome(source.as_bytes(), false);
            let ParseOutcome::SourceIssue(issue) = result else {
                panic!("range conjunction must reject: {result:?}");
            };
            // The optional `not` exit matches TYPEID `True`, then its
            // caller's relation separators reject the following `&`.
            assert_eq!(issue.rule(), SyntaxRule::Gram5);
            assert_eq!(issue_bytes(source.as_bytes(), issue), b"&");
            assert_eq!(
                issue.coordinate().start().value(),
                u64::try_from(source.find("&&").expect("conjunction in fixture"))
                    .expect("small source")
            );
            assert_eq!(
                issue.mechanical_fix,
                Some(crate::diagnostic_repairs::RANGE1_BOOLEAN_AND_FIX)
            );
            let repaired = source.replace("&&", ",");
            assert!(matches!(
                outcome(repaired.as_bytes(), false),
                ParseOutcome::Complete(_)
            ));
        }
    }
}
