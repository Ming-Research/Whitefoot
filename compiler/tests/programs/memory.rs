use super::support::{build_program, compile_program};

#[test]
fn heap_reading_tracks_a_box_and_grown_cell_and_returns_to_its_initial_bound() {
    let program = build_program(&compile_program("memory_statistics.wf"));
    let output = program.run_with_settings(None, &[("WF_DRIVERS", "1")]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn owned_element_assignments_release_displaced_boxes_at_commit_and_scope_exit() {
    let program = build_program(&compile_program("element_assignment_release.wf"));
    // args_count includes argv[0]. Each case runs in a fresh process so a
    // failure in one target shape cannot hide the other shapes' observations.
    // Capacities zero and sixteen check header-only and nonempty allocations.
    let failures = [
        "direct Slots Box field control",
        "direct Slots payload struct",
        "direct Slots whole element",
        "reference Slots Box field control",
        "reference Slots payload struct",
        "reference Slots whole element",
        "interface-generic Box field control",
        "interface-generic payload struct",
        "interface-generic whole element",
        "inline Slots, Array, Ring, boxed Ring and range targets",
        "local, struct field and Box content targets",
        "direct Slots struct with releasing enum and Option fields",
        "reference Slots struct with releasing enum and Option fields",
        "direct Slots concrete generic Holder<Payload>",
        "reference Slots concrete generic Holder<Payload>",
        "direct Slots struct with nonempty releasing container",
        "reference Slots struct with nonempty releasing container",
    ]
    .iter()
    .enumerate()
    .filter_map(|(index, case)| {
        let arguments = vec![b"case".as_slice(); index];
        let output = program.run_with_workers_and_arguments(None, &arguments);
        (output.status.code() != Some(0)).then(|| format!("{case}: {output:?}"))
    })
    .collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn concurrent_contexts_return_to_their_initial_heap_reading() {
    // Warm the same wave first: driver records and the caller's frame arena
    // remain live for the execution, and must count on both sides. The
    // independent sum is 2 waves * 32 contexts * sum(0..128).
    // WF_DRIVERS requests a count, not evidence of counter contributions:
    // hosts without a native ring run one driver, and no program-visible
    // driver identity or existing context report exposes the writers of
    // heap counters. This checks completed work and quiescent balance at
    // both settings; it does not assert that two drivers allocated.
    let program = build_program(&compile_program("memory_contexts.wf"));
    for drivers in ["1", "4"] {
        let output = program.run_with_settings(None, &[("WF_DRIVERS", drivers)]);
        assert_eq!(output.status.code(), Some(0), "drivers {drivers}: {output:?}");
    }
}
