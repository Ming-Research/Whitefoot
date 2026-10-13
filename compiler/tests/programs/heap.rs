use super::support::{compile_and_run, compile_program};

#[test]
fn owned_link_cursors_walk_edit_and_reset_lists_and_trees() {
    let llvm = compile_program("owned_link_cursors.wf");
    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn growable_vector_grows_by_affine_replace_and_runs_its_checks() {
    let llvm = compile_program("growable_vec.wf");
    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn musttail_consumes_a_deep_heap_linked_sequence() {
    let llvm = compile_program("tail_list.wf");
    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn byte_string_builds_searches_and_publishes_its_report() {
    let llvm = compile_program("byte_string.wf");
    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"length=43 brown=10 cat=none\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn a_borrowed_match_binder_passed_on_after_a_read_or_a_write_passes_its_field() {
    let llvm = compile_program("borrowed_binders.wf");
    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn call_results_read_at_their_source_keep_their_field_values() {
    let llvm = compile_program("result_sources.wf");
    let definition = |name: &str| {
        let symbol = format!(" @wf_{name}(");
        let start = llvm
            .match_indices('\n')
            .map(|(newline, _)| newline + 1)
            .find(|&line| {
                let text = &llvm[line..];
                let end = text.find('\n').unwrap_or(text.len());
                text.starts_with("define ") && text[..end].contains(&symbol)
            })
            .unwrap_or_else(|| panic!("missing definition of {name}"));
        let body = &llvm[start..];
        &body[..body.find("\n}\n").expect("a definition end")]
    };
    for (caller, callee) in [
        ("headroom", "account_at"),
        ("owner_and_id", "checked_account"),
        ("checked_account.source", "account_at"),
        ("ledger_owner_balance", "head_account"),
    ] {
        let body = definition(caller);
        assert!(
            body.contains(&format!("call ptr @wf_{callee}.source(")),
            "{caller}: {body}"
        );
    }
    let written = definition("balance_before_deposit");
    assert!(
        written.contains("call void @wf_account_at(") && !written.contains(".source("),
        "{written}"
    );
    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}
