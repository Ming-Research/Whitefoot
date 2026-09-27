use super::support::{
    build_program, build_program_with_driver_arguments, compile_sources,
    compile_sources_with_cli_parallel_defaults,
};
#[cfg(unix)]
use super::support::{build_program_with_driver, compile_program};

#[cfg(unix)]
fn dense_expected(words: usize, seed: u64, rounds: u64) -> u64 {
    let mut state = seed;
    let mut values = (0..words)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            state
        })
        .collect::<Vec<_>>();
    state = seed;
    for _ in 0..rounds {
        for value in &mut values {
            state = state
                .wrapping_mul(2_862_933_555_777_941_757)
                .wrapping_add(*value);
            *value = state;
        }
    }
    values.into_iter().fold(0_u64, |digest, value| {
        digest.wrapping_mul(1_099_511_628_211).wrapping_add(value)
    })
}

#[cfg(unix)]
#[test]
fn dense_scalar_and_record_updates_match_the_complete_sequence_digest() {
    let mut llvm = compile_program("containers/dense.wf");
    for name in ["scalar16", "scalar256", "wide16"] {
        let definition = format!("define i64 @wf_dense_{name}(");
        assert_eq!(llvm.matches(&definition).count(), 1);
    }
    for name in ["main", "wf__main_body"] {
        let definition = format!("define i32 @{name}(");
        assert_eq!(llvm.matches(&definition).count(), 1);
        llvm = llvm.replacen(&definition, &format!("define i32 @unused_{name}("), 1);
    }
    llvm.push_str("\ndeclare i32 @wf__main_body(i32, ptr)\n");
    let driver = r#"
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
extern uint64_t wf_dense_scalar16(uint64_t, uint64_t);
extern uint64_t wf_dense_scalar256(uint64_t, uint64_t);
extern uint64_t wf_dense_wide16(uint64_t, uint64_t);
extern int wf__floor_run(int, char **);
int wf__main_body(int argc, char **argv) {
    (void)argc; (void)argv;
    uint64_t (*kernels[])(uint64_t, uint64_t) = {
        wf_dense_scalar16, wf_dense_scalar256, wf_dense_wide16
    };
    const uint64_t rounds[] = {0, 1, 3, 4, 8};
    for (unsigned kernel = 0; kernel < 3; ++kernel)
        for (unsigned round = 0; round < 5; ++round)
            for (uint64_t at = 0; at < 12; ++at) {
                uint64_t seed = UINT64_C(7640891576956012809)
                    + at * UINT64_C(11400714819323198485);
                printf("%" PRIu64 "\n", kernels[kernel](seed, rounds[round]));
            }
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
"#;
    let program = build_program_with_driver(&llvm, Some(driver));
    let output = program.run_with_workers(Some("1"));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let mut expected = String::new();
    for words in [16, 256, 64] {
        for rounds in [0, 1, 3, 4, 8] {
            for at in 0..12_u64 {
                let seed = 7_640_891_576_956_012_809_u64
                    .wrapping_add(at.wrapping_mul(11_400_714_819_323_198_485));
                expected.push_str(&format!("{}\n", dense_expected(words, seed, rounds)));
            }
        }
    }
    assert_eq!(output.stdout, expected.as_bytes());
}

#[cfg(unix)]
#[test]
fn maps_leaf_splits_pages_and_compact_owner_regressions_execute() {
    for name in [
        "hashmap",
        "ordered",
        "packed-page",
        "priority",
        "ordered-runtime-gap",
        "boxed-helper-gap",
        "shared-option-view",
    ] {
        let module = compile_program(&format!("containers/{name}.wf"));
        let output = super::support::compile_and_run(&module);
        assert_eq!(output.status.code(), Some(0), "{name}: {output:?}");
        assert!(output.stdout.is_empty(), "{name}: {output:?}");
        assert!(output.stderr.is_empty(), "{name}: {output:?}");
    }
}

fn execute_container_program(
    name: &str,
    sources: &[(&str, &[u8])],
    expected_allocations: usize,
    check_observer_controls: bool,
) {
    let modes = [
        ("sequential", compile_sources(sources)),
        (
            "parallel",
            compile_sources_with_cli_parallel_defaults(sources),
        ),
    ];

    for (mode, llvm) in modes {
        let context = format!("{name}/{mode}");
        // Exercise ordinary deallocation as well as the observed image below:
        // the ledger quarantines released allocations to distinguish identities.
        let output = build_program(&llvm).run_with_workers(None);
        assert_eq!(output.status.code(), Some(0), "{context}: {output:?}");
        assert!(output.stdout.is_empty(), "{context}: {output:?}");
        assert!(output.stderr.is_empty(), "{context}: {output:?}");

        assert!(
            llvm.contains("@malloc("),
            "{context}: missing allocator calls"
        );
        assert!(llvm.contains("@free("), "{context}: missing release calls");
        assert_eq!(llvm.matches("define i32 @main(").count(), 1, "{context}");
        let observed = llvm
            .replace("@malloc(", "@wf_observe_allocate(")
            .replace("@free(", "@wf_observe_release(")
            .replace("@main(", "@wf_fixture_main(");
        let observer =
            include_str!("../../../tests/programs/containers/container-allocation-observer.c");
        let observed_program = build_program_with_driver_arguments(
            &observed,
            Some(observer),
            &[
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wno-override-module",
            ],
        );
        let output = observed_program.run_with_workers(None);
        assert_eq!(output.status.code(), Some(0), "{context}: {output:?}");
        assert!(output.stderr.is_empty(), "{context}: {output:?}");
        assert_eq!(
            output.stdout,
            format!(
                "container allocation observer: {expected_allocations} allocations, each released exactly once\n"
            )
            .as_bytes(),
            "{context}: {output:?}"
        );

        if check_observer_controls && mode == "parallel" {
            // Reuse this native image for observer controls: simultaneous
            // registration and cross-worker release, then three independent
            // wrong ledgers. No extra WF compilation or C build is needed.
            let concurrent =
                observed_program.run_with_workers_and_arguments(None, &[b"concurrent"]);
            assert_eq!(concurrent.status.code(), Some(0), "{concurrent:?}");
            assert!(concurrent.stderr.is_empty(), "{concurrent:?}");
            assert_eq!(
                concurrent.stdout,
                b"container allocation observer: 32 allocations, each released exactly once\n"
            );
            for (argument, message) in [
                ("double-release", "allocation released twice"),
                (
                    "foreign-release",
                    "release did not return an allocated address",
                ),
                (
                    "missing-release",
                    "every allocation is released exactly once",
                ),
            ] {
                let output =
                    observed_program.run_with_workers_and_arguments(None, &[argument.as_bytes()]);
                assert_eq!(output.status.code(), Some(1), "{argument}: {output:?}");
                assert_eq!(
                    output.stderr,
                    format!("container allocation observer: {message}\n").as_bytes(),
                    "{argument}: {output:?}"
                );
            }
        }
    }
}

#[test]
fn grow_vector_executes_and_releases_every_allocation_in_both_lowering_modes() {
    // The caller names std::collections::vector, which the compiler carries.
    let sources: [(&str, &[u8]); 1] = [(
        "containers/grow-vector-program.wf",
        include_bytes!("../../../tests/programs/containers/grow-vector-program.wf"),
    )];
    execute_container_program("grow-vector", &sources, 25, true);
}

#[test]
fn slab_operations_and_memberships_release_every_owner_in_both_lowering_modes() {
    let sources: [(&str, &[u8]); 2] = [
        (
            "containers/slab-membership-program.wf",
            include_bytes!("../../../tests/programs/containers/slab-membership-program.wf"),
        ),
        (
            "containers/slab-program.wf",
            include_bytes!("../../../tests/programs/containers/slab-program.wf"),
        ),
    ];
    // Seven total backings, nine operation payloads (four affine and five
    // nodrop), and three payloads in the two membership protocols.
    execute_container_program("slab", &sources, 19, false);
}

#[test]
fn deque_wrap_rebase_and_consumption_release_every_owner_in_both_lowering_modes() {
    let sources: [(&str, &[u8]); 1] = [(
        "containers/deque-program.wf",
        include_bytes!("../../../tests/programs/containers/deque-program.wf"),
    )];
    // Six scalar, two affine, three nodrop and two zero-sized backings;
    // three affine and five nodrop payloads. Every rebase creates a new backing.
    execute_container_program("deque", &sources, 21, false);
}

#[test]
fn hash_map_operations_preserve_owned_pairs_in_both_lowering_modes() {
    let sources: [(&str, &[u8]); 1] = [(
        "containers/hash-map-program.wf",
        include_bytes!("../../../tests/programs/containers/hash-map-program.wf"),
    )];
    // Seventeen map backings, ten payload/query child Boxes, and one fresh
    // Box returned by the borrowed edit callback and consumed by its caller.
    execute_container_program("hash-map", &sources, 28, false);
}

#[test]
fn priority_queue_orders_and_preserves_every_owner_in_both_lowering_modes() {
    let sources: [(&str, &[u8]); 1] = [(
        "containers/priority-queue-program.wf",
        include_bytes!("../../../tests/programs/containers/priority-queue-program.wf"),
    )];
    // Twenty-three backings and forty payload Boxes. The independent source
    // oracle sorts a separate array and checks each owner identity. The native
    // ledger additionally observes actual releases, including growth, refused
    // owner retry, zero capacity and zero-sized u64-max logical capacity.
    execute_container_program("priority-queue", &sources, 63, false);
}

#[test]
fn indexed_memberships_match_the_model_and_release_every_owner_in_both_lowering_modes() {
    let sources: [(&str, &[u8]); 2] = [
        (
            "containers/indexed-store.wf",
            include_bytes!("../../../tests/programs/containers/indexed-store.wf"),
        ),
        (
            "containers/indexed-membership-program.wf",
            include_bytes!("../../../tests/programs/containers/indexed-membership-program.wf"),
        ),
    ];
    // Four policy/payload traces each allocate six initial store backings,
    // eight index growth backings and fifteen payload Boxes: 116 total.
    // Capacity/refusal, retirement and zero capacity add nine backings;
    // direct indexed heapify adds one; the nodrop SlabEdit result adds one
    // backing and two payload Boxes. The source model independently checks
    // dictionary membership, sorted expiration and exact owner identities.
    execute_container_program("indexed-membership", &sources, 129, false);
}

#[test]
fn ordered_map_mutations_match_sorted_oracle_and_preserve_every_owner_in_both_lowering_modes() {
    let sources: [(&str, &[u8]); 1] = [(
        "containers/ordered-map-program.wf",
        include_bytes!("../../../tests/programs/containers/ordered-map-program.wf"),
    )];
    // Twenty-two scalar nodes, six owning nodes, and seventy-five payload
    // Boxes. The additional owning map checks leaf and internal replacement
    // below its ceiling: three nodes and thirty-six payloads add 39 to the
    // original 64-allocation public mutation and traversal chain.
    execute_container_program("ordered-map", &sources, 103, false);
}
