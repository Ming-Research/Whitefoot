use super::*;

/// The complete native path must preserve the cell's separate effect point:
/// its wrappers call the CAS/read leaves and take no shared-state lock. Before
/// this implementation the fixture could not resolve DecisionCell at all.
#[test]
fn decision_cell_operations_use_independent_leaf_calls_and_shared_lifetime() {
    let module = compile(include_bytes!(
        "../../../../tests/conformance/cases/decision-cell-run-atomic.wf"
    ));
    assert!(module.contains("call ptr @wf__decision_cell_new()"));
    for leaf in [
        "wf__decision_cell_read",
        "wf__claim_written",
        "wf__claim_killed",
    ] {
        let body = module
            .split("define ")
            .find(|body| {
                body.split("\n}\n")
                    .next()
                    .unwrap()
                    .contains(&format!("call i32 @{leaf}(ptr"))
            })
            .expect("one ordinary wrapper calls each cell leaf");
        let body = body.split("\n}\n").next().unwrap();
        assert!(!body.contains("@wf__shared_acquire"), "{body}");
        assert!(!body.contains("@wf__shared_take"), "{body}");
        assert!(!body.contains("llvm.coro.suspend"), "{body}");
        assert!(
            body.contains("= trunc i32 ") && body.contains(" to i8"),
            "the runtime's i32 answer narrows to DecisionState's i8 tag: {body}"
        );
    }
    assert!(module.contains("call void @wf__shared_share(ptr"));
    assert!(module.contains("call i32 @wf__shared_release(ptr"));
    assert!(module.contains("call void @wf__shared_free(ptr"));
}
