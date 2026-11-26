//! Tripwire on the LLVM function attributes the backend is allowed to emit.
//!
//! The backend emits no function attributes today, so the assertion below is
//! currently satisfied without the emitter doing anything. That is the point:
//! it is a canary for the effect-attribute channel that a future change will
//! open when it starts translating Whitefoot effect rows (`pure`, `reads`,
//! `writes`) into LLVM attributes. It must fail on the day anyone
//! emits `willreturn`, before the resulting miscompile reaches a program.
//!
//! Why `willreturn` specifically, and why not just the combination:
//!
//! [EFF-3] of the active kernel specification says a call whose row is `pure`
//! "which allocates nothing, and which creates no shared object, frozen
//! object, concurrent hash map or key set [SHARE-1] licenses deduplication and
//! reordering with equal arguments. Elimination of an unused licensed call additionally
//! requires a termination proof; v0 provides no termination checker, so unused
//! calls are not eliminated. The source spelling `pure` excludes state reads
//! and state writes; it does not promise termination."
//! `willreturn` is exactly the termination promise the language
//! does not make, so emitting it asserts something v0 cannot prove.
//!
//! The tripwire is on `willreturn` because it is the attribute no v0 effect row
//! can license. `nounwind` and a narrow `memory(...)` class can eventually be
//! derived from effects, but termination requires its own proof. Do not delete
//! this test as vacuous: it is meant to be silent until the channel opens.
//!
//! The check is on the module the backend emits, not on the module the host
//! optimizer returns. Do not "strengthen" it to the optimized module: the host
//! optimizer infers and writes its own attribute groups (an `-O2` run over
//! `tests/programs/recursive_tree.wf` yields eight `willreturn` occurrences,
//! including on declared allocation helpers), which is LLVM reasoning about its
//! own IR and is not this compiler making a promise. What this test controls is
//! what the backend asserts.

use super::compile;

/// Representative accepted programs from the corpus. The attribute tripwire
/// depends only on each program continuing to emit a function definition.
/// `recursive_tree` exercises a self-recursive call and `generic_instances`
/// exercises monomorphized instances, so the check still covers the call
/// shapes an effect-attribute pass would annotate.
const REPRESENTATIVE_PROGRAMS: &[(&str, &[u8])] = &[
    (
        "tests/programs/sha256_abc.wf",
        include_bytes!("../../../../tests/programs/sha256_abc.wf"),
    ),
    (
        "tests/programs/fir_filter.wf",
        include_bytes!("../../../../tests/programs/fir_filter.wf"),
    ),
    (
        "tests/programs/percent_decode.wf",
        include_bytes!("../../../../tests/programs/percent_decode.wf"),
    ),
    (
        "tests/programs/telemetry_packet.wf",
        include_bytes!("../../../../tests/programs/telemetry_packet.wf"),
    ),
    (
        "tests/programs/generic_instances.wf",
        include_bytes!("../../../../tests/programs/generic_instances.wf"),
    ),
    (
        "tests/programs/recursive_tree.wf",
        include_bytes!("../../../../tests/programs/recursive_tree.wf"),
    ),
];

#[test]
fn no_emitted_module_promises_termination_with_the_willreturn_attribute() {
    for (path, source) in REPRESENTATIVE_PROGRAMS {
        let module = compile(source);
        // Guards against the corpus coverage going vacuous on a degenerate
        // module without coupling that coverage to its current proof count.
        assert!(
            module.contains("define "),
            "{path} must emit at least one function definition"
        );
        assert!(
            !module.contains("willreturn"),
            "{path} emitted the willreturn attribute; v0 has no termination \
             checker, so no effect row licenses that promise [EFF-3]"
        );
    }
}
