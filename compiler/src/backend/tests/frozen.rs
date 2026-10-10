use super::*;

#[test]
fn frozen_cleanup_tests_the_last_reference_before_releasing_content_and_block() {
    let module = compile(include_bytes!(
        "../../../../tests/conformance/cases/share1-pos-frozen-release-heap.wf"
    ));
    assert!(module.contains("call ptr @wf__frozen_new(i64"));
    assert!(module.contains("call void @wf__frozen_share(ptr"));
    let helper = module
        .split("define ")
        .find(|body| body.contains("%last = call i32 @wf__frozen_release(ptr %value)"))
        .expect("one frozen handle drop helper");
    let helper = helper.split("\n}\n").next().unwrap();
    let release = helper.find("@wf__frozen_release").unwrap();
    let branch = helper
        .find("br i1 %is.last, label %state, label %done")
        .unwrap();
    let content = helper.find("@wf__heap_give").unwrap();
    let free = helper.find("@wf__frozen_free").unwrap();
    assert!(
        release < branch && branch < content && content < free,
        "last-reference path: {helper}"
    );
    assert!(
        !helper.contains("@wf__shared_"),
        "frozen release has no shared lock or state hold"
    );
}

#[test]
fn frozen_inner_keeps_one_pointer_layout_and_direct_content_loads() {
    let module = compile(include_bytes!(
        "../../../../tests/conformance/cases/share1-pos-frozen-read-in-atomic.wf"
    ));
    assert!(module.contains("call ptr @wf__frozen_new(i64"));
    assert!(module.contains("load i8, ptr"));
    assert!(module.contains("@wf__shared_acquire"));
    assert!(
        module
            .lines()
            .any(|line| line.contains("= type { ptr, i64 }")),
        "{module}"
    );
}
