use super::support::{build_program, compile_program};

#[test]
fn short_lived_inline_windows_preserve_values_and_release_only_live_elements() {
    // One native build/run checks frame reuse, take/reinsert, wrapped Ring
    // cleanup, struct/enum transport and Array fill. The program checks the
    // heap baseline after every call, with stale freed pointers in raw slots.
    let program = build_program(&compile_program("short_lived_windows.wf"));
    let output = program.run_with_settings(None, &[("WF_DRIVERS", "1")]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn heap_reading_tracks_boxes_grown_cells_and_shared_map_storage() {
    // STOR-1: status 15 catches counted empty headers, zero growth or
    // wrong growth extents (48 bytes for u64, 16 for zero-byte elements);
    // status 16 catches release imbalance after those owners leave scope.
    // PRE-2: the map's two waves distinguish newly carved and reused nodes.
    // A presized table avoids moves masking node deltas: on the unfixed
    // runtime insertion contributes zero bytes and exits with status 8.
    // Status 9 independently catches missing host-mapped cell accounting;
    // 11 catches retained free nodes, and 14 catches drain/drop imbalance.
    let program = build_program(&compile_program("memory_statistics.wf"));
    let output = program.run_with_settings(None, &[("WF_DRIVERS", "1")]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn owned_element_assignments_release_displaced_boxes_at_commit_and_scope_exit() {
    let program = build_program(&compile_program("element_assignment_release.wf"));
    // args_count includes argv[0]. Each case runs in a fresh process so a
    // failure in one target shape cannot hide the other shapes' observations.
    // Capacities zero and sixteen check shared empty headers and nonempty allocations.
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
        "direct Paged whole element",
        "reference Paged payload struct",
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

/// A steady map's reserve goes on request: the release answers the bytes
/// the heap reading falls by, exactly, for a small map whose reserve is a
/// pool block larger than its cells, for a larger pooled one, and for one
/// whose cells are mapped from the host; a second release answers nothing,
/// and every entry stays [SHARE-1, PRE-2]. Exit codes 11 to 15, 21 to 25
/// and 31 to 35 name the failing check of each map.
#[test]
fn releasing_a_map_reserve_lowers_the_heap_reading_by_its_bytes() {
    let program = build_program(&compile_program("map_release_reserve.wf"));
    let output = program.run_with_settings(None, &[("WF_DRIVERS", "1")]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}
