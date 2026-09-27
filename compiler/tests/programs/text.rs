use whitefoot::FragmentGranularity;

use super::support::{
    build_program, build_program_from_fragments, compile_and_run, compile_program,
    compile_program_with_overlap, emitted_function,
};

#[test]
fn percent_decoder_executes_through_the_ordinary_pipeline() {
    let llvm = compile_program("percent_decode.wf");
    let decode = emitted_function(&llvm, "decode");
    assert!(decode.contains("icmp ult i64"));
    assert!(!decode.contains("call void @wf_trap"));

    // Linking the full overlap lowering catches the original world/phi bug;
    // each run also checks the fixture's expected decoded bytes and extent.
    for (module, widths) in [
        (llvm, &["1"][..]),
        (
            compile_program_with_overlap("percent_decode.wf"),
            &["1", "4"][..],
        ),
    ] {
        let program = build_program(&module);
        for width in widths {
            let output = program.run_with_workers(Some(width));
            assert!(output.status.success(), "{width}: {output:?}");
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        }
    }
}

#[test]
fn utf8_parser_executes_through_the_ordinary_pipeline() {
    let llvm = compile_program("utf8parse.wf");
    let parse = emitted_function(&llvm, "parse");
    assert!(parse.contains("icmp ult i64"));
    assert!(!parse.contains("call void @wf_trap"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// The parser linked from the link fragments a modular build splits it into
/// [MOD-8] runs as the whole module does: each release helper keeps one
/// definition, including one that only another helper names.
#[test]
fn recursive_prefix_parser_linked_from_its_fragments_runs_as_one_module() {
    let llvm = compile_program("prefix_expression.wf");
    for granularity in [FragmentGranularity::Function, FragmentGranularity::Module] {
        let output = build_program_from_fragments(&llvm, granularity).run_with_workers(None);
        assert!(output.status.success(), "{granularity:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{granularity:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{granularity:?}: {output:?}");
    }
}

#[test]
fn recursive_prefix_parser_builds_evaluates_and_drops_its_ast() {
    let llvm = compile_program("prefix_expression.wf");
    let parser = emitted_function(&llvm, "parse_expression");
    assert!(parser.contains("call"));
    assert!(parser.contains("@wf_parse_expression"));
    assert!(llvm.contains("call ptr @malloc"));
    assert!(llvm.contains("call void @free"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
