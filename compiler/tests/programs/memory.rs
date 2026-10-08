use super::support::{build_program, compile_program};

#[test]
fn heap_reading_tracks_a_box_and_returns_to_a_zero_byte_bound() {
    let program = build_program(&compile_program("memory_statistics.wf"));
    let output = program.run_with_settings(None, &[("WF_DRIVERS", "1")]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
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
