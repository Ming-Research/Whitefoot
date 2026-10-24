use super::support::{build_program, compile_and_run, compile_program, emitted_function};

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
fn heap_reading_tracks_boxes_release_grown_cells_maps_and_key_set_storage() {
    // PRE-1: release returns a nonempty boxed window and its boxed element,
    // an empty window (whose header STOR-1 does not count), and a struct
    // owning two cells to the baseline.
    // STOR-1: status 15 catches counted empty headers, zero growth or
    // wrong growth extents (48 bytes for u64, 16 for zero-byte elements);
    // status 16 catches release imbalance after those owners leave scope.
    // PRE-2: the map's two waves distinguish newly carved and reused nodes.
    // A presized table avoids moves masking node deltas: on the unfixed
    // runtime insertion contributes zero bytes and exits with status 8.
    // Status 9 independently catches missing host-mapped cell accounting;
    // 11 catches retained free nodes, and 14 catches drain/drop imbalance.
    // SHARE-1: key_set_bytes includes unused capacity, insertion and scan
    // growth, and answers exactly the bytes explicit release returns. The
    // prior compiler refuses that operation; omitting growth from its answer
    // yields 29 or 33, and retaining a released grant yields 40 or 42.
    let program = build_program(&compile_program("memory_statistics.wf"));
    let output = program.run_with_settings(None, &[("WF_DRIVERS", "1")]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn paged_small_owners_count_only_their_actual_page_extent_and_release_to_baseline() {
    // Zero capacity and empty release count nothing. U64 capacities 1, 2,
    // 3 and 4 use 8, 16, 32 and 32 page bytes plus the 32-byte cell.
    // The program checks sparse page-zero replacement, growth past B,
    // every retained element, small/full cleanup and zero-stride bytes.
    let program = build_program(&compile_program("paged_small_owners.wf"));
    let output = program.run_with_settings(None, &[("WF_DRIVERS", "1")]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
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
        assert_eq!(
            output.status.code(),
            Some(0),
            "drivers {drivers}: {output:?}"
        );
    }
}

/// A steady map's reserve goes on request, made in a statement that holds
/// the map: the release answers the bytes
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

/// A call result may take the storage of a copied input that is dead after
/// the call, so `advance` and `advance_set`, the same replacement spelled with
/// and without a separate binding, pass `mix` one address as both its result
/// destination and its input. `mix` writes a candidate result before it reads
/// that input again; the destination-result ABI requires it to keep reading
/// the original input. Exit codes 1 to 4 and 7 to 10 name a word of a
/// replaced result that differs, and 5, 6 and 11 a kept result.
#[test]
fn a_call_result_sharing_a_copied_input_reads_the_original_input() {
    let llvm = compile_program("copy_result_storage.wf");
    for caller in ["advance", "advance_set"] {
        let body = emitted_function(&llvm, caller);
        let call = body
            .lines()
            .find(|line| line.contains("@wf_mix("))
            .unwrap_or_else(|| panic!("{caller} calls mix: {body}"));
        let (_, arguments) = call.split_once("@wf_mix(").expect("call operands");
        let mut pointers = arguments.split(", ");
        let destination = pointers.next().expect("result destination");
        assert!(destination.starts_with("ptr "), "{call}");
        assert_eq!(Some(destination), pointers.next(), "{caller}: {call}");
    }

    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}
