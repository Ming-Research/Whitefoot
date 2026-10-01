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
