//! Private native-engine regression tests.
//!
//! The linked implementation may keep its own completion engine. Its queues,
//! syscall retries and wakeups confer no source acceptance or overlap rule.
//! C2 retires the former source completion/staged-lowering assertions; the
//! ordinary host fixtures retain their observable library behavior.

use super::test_directory;
use std::process::Command;

#[test]
fn the_compiler_owned_c_units_compile_in_the_default_dialect() {
    let directory = test_directory();
    // Keep the repository's directory layout so relative includes resolve as
    // they do when the compiler stages these native units for linking.
    let units = [
        ("completion/contract.h", crate::COMPLETION_CONTRACT_HEADER),
        (
            "completion/file_adapter.h",
            crate::COMPLETION_FILE_ADAPTER_HEADER,
        ),
        (
            "completion/file_posix.h",
            crate::COMPLETION_FILE_POSIX_HEADER,
        ),
        (
            "completion/socket_address.h",
            crate::COMPLETION_SOCKET_ADDRESS_HEADER,
        ),
        ("completion/bridge.h", crate::COMPLETION_BRIDGE_HEADER),
        (
            "completion/linux_io_uring.h",
            crate::COMPLETION_LINUX_IO_URING_HEADER,
        ),
        ("sched/core.h", crate::SCHED_CORE_HEADER),
        ("sched/prim.h", crate::SCHED_PRIM_HEADER),
        ("sched/entry.h", crate::SCHED_ENTRY_HEADER),
        ("completion/runtime.c", crate::COMPLETION_RUNTIME_SOURCE),
        ("completion/wait_host.c", crate::COMPLETION_WAIT_HOST_SOURCE),
        (
            "completion/file_adapter.c",
            crate::COMPLETION_FILE_ADAPTER_SOURCE,
        ),
        (
            "completion/file_posix.c",
            crate::COMPLETION_FILE_POSIX_SOURCE,
        ),
        ("completion/bridge.c", crate::COMPLETION_BRIDGE_SOURCE),
        (
            "completion/linux_io_uring.c",
            crate::COMPLETION_LINUX_IO_URING_SOURCE,
        ),
        ("sched/core.c", crate::SCHED_CORE_SOURCE),
        ("sched/prim_host.c", crate::SCHED_PRIM_HOST_SOURCE),
        ("sched/entry.c", crate::SCHED_ENTRY_SOURCE),
        ("completion/floor.c", crate::FLOOR_RUNTIME_SOURCE),
    ];
    for staged in ["completion", "sched"] {
        std::fs::create_dir_all(directory.join(staged)).expect("stage runtime directory");
    }
    for (name, source) in units {
        std::fs::write(directory.join(name), source).expect("write compiler-owned C unit");
    }
    for (name, _) in units {
        if !name.ends_with(".c") {
            continue;
        }
        let checked = Command::new("/usr/bin/clang")
            .arg("-fsyntax-only")
            .arg("-pthread")
            .arg("-I")
            .arg(directory.join("completion"))
            .arg("-x")
            .arg("c")
            .arg(directory.join(name))
            .output()
            .expect("invoke host clang");
        assert!(
            checked.status.success(),
            "{name} needs a dialect the shipped link may not select:\n{}",
            String::from_utf8_lossy(&checked.stderr)
        );
    }
    for (name, _) in units {
        std::fs::remove_file(directory.join(name)).expect("remove compiler-owned C unit");
    }
    for staged in ["completion", "sched"] {
        std::fs::remove_dir(directory.join(staged)).expect("remove staged runtime directory");
    }
    std::fs::remove_dir(directory).expect("remove the default-dialect directory");
}

#[test]
fn the_writer_ready_cells_have_one_capacity_source() {
    let bridge = crate::COMPLETION_BRIDGE_SOURCE;

    for gone in [
        "WF_COMPLETION_SLOT_CAPACITY",
        "WF_WRITER_READY_CAPACITY",
        "wf__writer_scheduler_ready",
        "wf__writer_run_root",
    ] {
        assert!(
            !bridge.contains(gone),
            "the bridge still names the retired writer scheduler: {gone}"
        );
    }
    assert!(
        !crate::COMPLETION_BRIDGE_HEADER.contains("#include \"writer_scheduler.h\""),
        "the bridge header still reaches for the retired writer scheduler"
    );

    // The bridge keeps no operation capacity, no slot array and no queue
    // array, so nothing there can refuse an operation.
    for gone in [
        "WF_BRIDGE_OPERATION_CAPACITY",
        "WF_BRIDGE_SLOT_COUNT",
        "WF_BRIDGE_QUEUE_COUNT",
        "wf_bridge_slots",
        "wf_bridge_queue",
        "wf_bridge_linux_entries",
        "wf_completion_claim",
        "WAIT_CAPACITY",
        "wf_completion_notify_capacity",
    ] {
        assert!(
            !bridge.contains(gone),
            "the bridge still names the deleted pool machinery: {gone}"
        );
    }
    assert!(
        !crate::COMPLETION_CONTRACT_HEADER.contains("wf_completion_slot"),
        "the contract header still declares a slot pool"
    );
    assert!(
        !crate::COMPLETION_CONTRACT_HEADER.contains("wf_completion_token"),
        "the contract header still declares a token"
    );
}

#[test]
fn linux_native_wait_unifies_cq_compute_and_capacity_without_polling() {
    let adapter = crate::COMPLETION_LINUX_IO_URING_SOURCE;
    let contract = crate::COMPLETION_CONTRACT_HEADER;
    let runtime = crate::COMPLETION_RUNTIME_SOURCE;
    let bridge = crate::COMPLETION_BRIDGE_SOURCE;

    assert!(adapter.contains("epoll_create1(EPOLL_CLOEXEC)"));
    assert!(adapter.contains("eventfd(0, EFD_CLOEXEC | EFD_NONBLOCK)"));
    assert!(adapter.contains("EPOLL_CTL_ADD"));
    assert!(adapter.contains("adapter->ring_descriptor"));
    assert!(adapter.contains("wf_completion_wake_epoch(adapter->runtime) != observed_epoch"));
    assert!(adapter.contains("wf_linux_drain_wake_descriptor"));
    assert!(adapter.contains("errno == EAGAIN"));
    assert!(contract.contains("wf_completion_set_wake_callback"));

    let notify = runtime
        .split_once("static void wf_completion_notify_scheduler(wf_completion_runtime *runtime) {")
        .expect("completion runtime has one scheduler announcer")
        .1
        .split_once("\nuint64_t wf_completion_wake_epoch")
        .expect("announcer precedes the epoch reader")
        .0;
    let parked = notify
        .find("parked_schedulers")
        .expect("host wake is conditional on an announced sleeper");
    let callback = notify
        .find("runtime->wake_callback(runtime->wake_context)")
        .expect("announced native sleeper receives the unified wake");
    assert!(parked < callback);

    // The ring's own bounded pass, which is the Linux arm of the bridge's one
    // ring seam. A failed pass is a fail-stop rather than a value the routing
    // could interpret, so the fail-stop is what is pinned here. It is
    // `wf_bridge_fail_with_code` rather than a bare `abort` because a
    // fail-stop that writes nothing cannot be diagnosed from a crash log, and
    // because the code the target answered is a different question from which
    // call failed: EPROTO from a reaping pass says the ring handed back
    // something that is not one of this runtime's records, which no site name
    // alone distinguishes from a host error in the same place
    // (`completion/bridge.c`, "the bridge's one fail-stop").
    let progress = bridge
        .split_once("static int wf_bridge_ring_progress(void) {")
        .expect("the Linux ring arm has a bounded progress pass")
        .1
        .split_once("\nstatic void wf_bridge_ring_flush(void) {")
        .expect("progress precedes the flush")
        .0;
    assert!(progress.contains("wf_linux_io_uring_progress("));
    assert!(progress.contains("if (reap_error != 0) {"));
    assert!(progress.contains("wf_bridge_fail_with_code("));
    assert!(progress.contains("reap_error\n"));
    assert!(!progress.contains("abort();"));
    assert!(!progress.contains("(void)wf_linux_io_uring_progress"));
    assert!(!bridge.contains("wf_completion_park_if_unchanged(\n                    &wf_bridge_runtime,\n                    epoch,\n                    1u"));
}

#[test]
fn typed_io_joins_do_not_reenter_the_compute_scheduler() {
    let bridge = crate::COMPLETION_BRIDGE_SOURCE;
    // Ordinary result, typed open and socket accept are the three live joins.
    // The removed private status operation had no ordinary-library caller.
    for name in ["file_join", "file_open_join", "socket_accept_join"] {
        let body = bridge
            .split_once(&format!("void wf__completion_{name}("))
            .unwrap_or_else(|| panic!("missing ordinary join: {name}"))
            .1
            .split_once("\n}")
            .expect("complete join body")
            .0;
        assert!(body.contains("wf_bridge_join(held)"), "{name}");
    }
    for forbidden in [
        "wf_sched_join(",
        "wf__sched_current_stack(",
        "wf__par_help_once(",
        "wf__sched_host_park(",
    ] {
        assert!(
            !bridge.contains(forbidden),
            "I/O wait reentered compute: {forbidden}"
        );
    }
}

#[test]
fn a_positioned_read_the_submitting_thread_would_run_itself_runs_there() {
    let bridge = crate::COMPLETION_BRIDGE_SOURCE;
    let adapter = crate::COMPLETION_FILE_ADAPTER_SOURCE;

    let rule = bridge
        .split_once("static int wf_bridge_positioned_read_runs_on_caller(uint64_t count) {")
        .expect("one rule decides whether a positioned read runs on its caller")
        .1
        .split_once("\n}\n")
        .expect("the rule ends with the function")
        .0;
    assert!(
        rule.contains("count != 0"),
        "a read of nothing makes no host call and is left alone: {rule}"
    );
    assert!(
        rule.contains("wf_bridge_helpers_pinned == 0"),
        "a written helper count takes nothing inline: {rule}"
    );
    assert!(
        rule.contains("wf_file_adapter_transfer_runs_on_caller(&wf_bridge_adapter)"),
        "the adapter answers whether the submitting thread would run it: {rule}"
    );

    // Exactly one submission entry point asks, and it is the positioned one.
    assert_eq!(
        bridge
            .matches("wf_bridge_positioned_read_runs_on_caller(count)")
            .count(),
        1,
        "only the positioned read may run on its caller"
    );
    let pread = bridge
        .split_once("void wf__completion_file_pread_submit(")
        .expect("the bridge exposes positioned read")
        .1
        .split_once("void wf__completion_file_write_submit(")
        .expect("positioned read precedes write")
        .0;
    assert!(
        pread.find("wf_bridge_positioned_read_runs_on_caller(count)")
            < pread.find("wf_bridge_submit_file(held);\n}"),
        "the decision comes before the record reaches an engine: {pread}"
    );
    assert!(
        pread.find("wf_bridge_ring_offer(held)")
            < pread.find("wf_bridge_positioned_read_runs_on_caller(count)"),
        "a native completion path is tried before the bounded adapter's rule"
    );
    // Whichever arm it takes, the record is published: there is no `0`.
    assert!(
        pread.contains("wf_bridge_execute_here(held);"),
        "the inline arm publishes the record rather than answering 0: {pread}"
    );
    // The one refusal a writer can spell: an offset the target ABI cannot
    // express. It is the host's own EINVAL, published into the record, and no
    // longer a reason to terminate now that no direct wrapper can take the
    // shape instead (design section 8).
    assert!(
        pread.contains("wf_bridge_complete_refused(held, EINVAL);"),
        "an offset above INT64_MAX is published as EINVAL: {pread}"
    );
    let submits = bridge
        .split_once("void wf__completion_file_read_submit(")
        .expect("the submit family starts at the plain read")
        .1
        .split_once("/* ------------------------------------------------------- the statistics */")
        .expect("the submit family ends before the statistics queries")
        .0;
    assert!(
        !submits.contains("return 0;"),
        "no submit answers 0 any more: {submits}"
    );

    // The adapter's half: no helper, nothing queued, and a measured
    // non-wait — never the absence of a measurement.
    let answer = adapter
        .split_once("int wf_file_adapter_transfer_runs_on_caller(const wf_file_adapter *adapter) {")
        .expect("the adapter answers in one place")
        .1
        .split_once("\n}\n")
        .expect("the answer ends with the function")
        .0;
    assert!(answer.contains("!= WF_FILE_WAIT_SHORT"));
    assert!(answer.contains("wf_file_adapter_helper_count(adapter) != 0"));
    assert!(answer.contains("wf_file_adapter_queued(adapter) == 0"));
    let verdict = adapter
        .split_once("enum wf_file_wait_verdict wf_file_adapter_wait_verdict(")
        .expect("one verdict function")
        .1
        .split_once("\n}\n")
        .expect("the verdict ends with the function")
        .0;
    assert!(
        verdict.contains("return WF_FILE_WAIT_UNMEASURED;"),
        "an adapter that has executed nothing must say so: {verdict}"
    );
}

#[test]
fn an_unset_helper_setting_selects_a_bounded_demand_driven_pool() {
    let bridge = crate::COMPLETION_BRIDGE_SOURCE;
    let adapter = crate::COMPLETION_FILE_ADAPTER_SOURCE;
    let policy = bridge
        .split_once("static void wf_bridge_helper_policy(size_t *initial, size_t *cap) {")
        .expect("the helper policy is one function")
        .1
        .split_once("\nstatic ")
        .expect("the policy ends before the next definition")
        .0;
    // A written value pins both ends, so growth cannot move it, and it is read
    // under the one rule every startup setting of this runtime follows.
    assert!(policy.contains("wf__sched_setting(\"WF_IO_HELPERS\""));
    assert!(policy.contains("*initial = (size_t)written;"));
    assert!(policy.contains("*cap = (size_t)written;"));
    // Unset starts with no helper; the fallback uses the implementation's
    // helper ceiling independently of the core count.
    assert!(
        policy.contains("*initial = 0u;"),
        "an unset setting must start with no helper: {policy}"
    );
    assert!(
        policy.contains("*cap = WF_BRIDGE_MAX_HELPERS;"),
        "the fallback ceiling is the bridge's helper limit: {policy}"
    );
    assert!(
        !policy.contains("(size_t)online < WF_BRIDGE_MAX_HELPERS"),
        "the fallback helper ceiling must be independent of core count: {policy}"
    );

    let growth = adapter
        .split_once("static void wf_file_grow_helpers_locked(\n    wf_file_adapter *adapter,\n    int peer_bound\n) {")
        .expect("growth is one named function")
        .1
        .split_once("\n}\n")
        .expect("growth ends with the function")
        .0;
    assert!(
        growth.contains("held >= adapter->helper_cap"),
        "growth must stop at the cap: {growth}"
    );
    assert!(
        growth.contains("adapter->queue_count <= held"),
        "growth must require a queue that has outrun the pool: {growth}"
    );
    assert!(
        growth.contains("wf_file_adapter_wait_verdict(adapter) != WF_FILE_WAIT_LONG"),
        "growth must also require a measured wait to overlap: {growth}"
    );
    // Both of those terms are about a wait this adapter has already seen, and
    // a peer-bound request's wait has not started: it is ended by another
    // program, so the kind is the only thing that can know it is coming. A
    // peer-bound submission therefore takes neither term and grows on the cap
    // alone, which is what keeps the two terms above from serializing several
    // connections onto one helper.
    assert!(
        growth.contains("if (peer_bound == 0) {"),
        "the two measured terms must be the non-peer-bound arm: {growth}"
    );
    let peer_bound_arm = growth
        .split_once("if (peer_bound == 0) {")
        .expect("the measured terms are one arm")
        .1;
    assert!(
        peer_bound_arm.contains("adapter->queue_count <= held")
            && peer_bound_arm.contains("WF_FILE_WAIT_LONG"),
        "both measured terms belong inside that arm: {growth}"
    );
    // Growth runs inside the one enqueue that already holds the queue lock,
    // so it creates at most one helper per submission and needs no second
    // lock, and every kind of queued work reaches the pool the same way.
    let enqueue = adapter
        .split_once("static int wf_file_enqueue_locked(")
        .expect("one place appends an accepted queue entry")
        .1
        .split_once("\n}\n")
        .expect("the enqueue ends with the function")
        .0;
    assert!(
        enqueue.contains("wf_file_grow_helpers_locked(")
            && enqueue.contains("wf_file_request_held_locked(adapter, &record->request)"),
        "the enqueue is where growth happens, and it is what reads the kind: {enqueue}"
    );
    // What it reads is the kind, or, once the program runs contexts, the
    // hold that leaves every queued request to the helpers [WAIT-2].
    let held = adapter
        .split_once("static int wf_file_request_held_locked(")
        .expect("one predicate decides what the helpers are left")
        .1
        .split_once("\n}\n")
        .expect("the predicate ends with the function")
        .0;
    assert!(
        held.contains("adapter->hold_for_contexts != 0")
            && held.contains("wf_file_request_is_peer_bound(request)"),
        "the predicate reads the context hold and the kind: {held}"
    );
    // One queued request wakes one helper, never every helper, only a helper
    // that is actually asleep, and never from inside the queue lock: a signal
    // issued under that lock wakes a helper whose next act is to block on it.
    assert!(
        enqueue.contains("wake = adapter->blocked_helpers != 0;"),
        "the enqueue decides the wake under the lock: {enqueue}"
    );
    assert!(
        !enqueue.contains("wf_completion_wait_wake"),
        "the enqueue must not issue the wake while it holds the lock: {enqueue}"
    );
    let submit = adapter
        .split_once("enum wf_file_submit_result wf_file_adapter_submit(")
        .expect("one submission entry point")
        .1
        .split_once("\n}\n")
        .expect("the submission ends with the function")
        .0;
    let unlock = submit
        .find("wf_completion_wait_unlock(&adapter->queue_wait);\n    if (wake != 0) {")
        .expect("the wake follows the unlock");
    let signal = submit
        .find("wf_completion_wait_wake(&adapter->queue_wait, 0)")
        .expect("a submission announces to exactly one helper");
    assert!(unlock < signal, "the wake is issued outside the queue lock");
    assert!(
        !submit.contains("wf_completion_wait_wake(&adapter->queue_wait, 1)"),
        "a submission must not wake every helper"
    );
    // The bridge owns no second helper pool layered over this one.
    assert!(
        !bridge.contains("wf_bridge_target_helpers"),
        "the bridge must not keep a helper pool of its own"
    );
}

#[test]
fn a_native_ring_carries_opens_and_closes_under_one_kind_rule() {
    let ring = crate::COMPLETION_LINUX_IO_URING_SOURCE;
    let bridge = crate::COMPLETION_BRIDGE_SOURCE;
    let posix_header = crate::COMPLETION_FILE_POSIX_HEADER;

    for opcode in ["IORING_OP_OPENAT", "IORING_OP_CLOSE"] {
        assert!(
            ring.contains(opcode),
            "the ring adapter must submit {opcode}"
        );
    }
    // The kind decision is one rule, stated once, called by both POSIX
    // engines. It moved out of the adapter's own header when that header
    // became shared with a platform that has no `struct stat`, and into the
    // POSIX leaf both engines include (design section 7).
    assert!(
        posix_header.contains("wf_file_kind_outcome("),
        "the open-kind rule belongs to the POSIX file leaf's contract"
    );
    assert!(
        ring.contains("wf_file_kind_outcome("),
        "the ring adapter must answer with the shared open-kind rule"
    );
    assert!(
        crate::COMPLETION_FILE_POSIX_SOURCE.contains("wf_file_kind_outcome("),
        "the bounded POSIX adapter must answer with the shared open-kind rule"
    );
    // The part of an open that can wait is the path resolution, and no
    // scheduler thread may perform that: it is a ring operation or nothing.
    let decision = ring
        .split_once("static void wf_linux_decide_open(")
        .expect("the open decision is one named function")
        .1
        .split_once("\n}\n")
        .expect("the open decision ends with the function")
        .0;
    assert!(
        !decision.contains("openat("),
        "an open's path resolution belongs to the ring: {decision}"
    );
    assert!(
        !ring.contains("submission->opcode = IORING_OP_STATX"),
        "the kind check is one fstat of an open descriptor, not a second ring \
         round trip that measured 31 percent slower"
    );
    assert!(
        decision.contains("fstat(record->opened_descriptor"),
        "the kind check reads the mode of the descriptor the open produced: \
         {decision}"
    );
    // Every submit routes through one dispatcher, which asks the ring whether
    // it has a form for this kind before it reaches the bounded adapter. The
    // question is asked before the record is offered, so a kind the ring does
    // not carry is never refused after the operation was already the ring's.
    let dispatch = bridge
        .split_once("static void wf_bridge_dispatch(wf_completion_record *record) {")
        .expect("one dispatcher")
        .1
        .split_once("\n}\n")
        .expect("the dispatcher ends with the function")
        .0;
    let native = dispatch
        .find("wf_bridge_ring_offer(record)")
        .expect("the dispatcher asks the ring first");
    let fallback = dispatch
        .find("wf_bridge_submit_file(record);")
        .expect("the dispatcher keeps the bounded adapter");
    assert!(
        native < fallback,
        "the ring is asked before the bounded POSIX adapter: {dispatch}"
    );
    assert!(
        ring.contains("int wf_linux_io_uring_carries(const wf_completion_record *record) {"),
        "the ring answers which kinds it has a form for"
    );
}

#[test]
fn linked_c_units_avoid_identifiers_the_host_compiler_predefines() {
    for (name, source) in [
        ("bridge.c", crate::COMPLETION_BRIDGE_SOURCE),
        ("runtime.c", crate::COMPLETION_RUNTIME_SOURCE),
        ("wait_host.c", crate::COMPLETION_WAIT_HOST_SOURCE),
        ("wait_windows.c", crate::COMPLETION_WAIT_WINDOWS_SOURCE),
        ("file_adapter.c", crate::COMPLETION_FILE_ADAPTER_SOURCE),
        ("file_posix.c", crate::COMPLETION_FILE_POSIX_SOURCE),
        ("file_windows.c", crate::COMPLETION_FILE_WINDOWS_SOURCE),
        ("linux_io_uring.c", crate::COMPLETION_LINUX_IO_URING_SOURCE),
        ("windows_iocp.c", crate::COMPLETION_WINDOWS_IOCP_SOURCE),
        ("sched/core.c", crate::SCHED_CORE_SOURCE),
        ("sched/prim_host.c", crate::SCHED_PRIM_HOST_SOURCE),
        ("sched/prim_windows.c", crate::SCHED_PRIM_WINDOWS_SOURCE),
        ("sched/entry.c", crate::SCHED_ENTRY_SOURCE),
        ("contract.h", crate::COMPLETION_CONTRACT_HEADER),
        ("bridge.h", crate::COMPLETION_BRIDGE_HEADER),
        ("file_adapter.h", crate::COMPLETION_FILE_ADAPTER_HEADER),
        ("file_posix.h", crate::COMPLETION_FILE_POSIX_HEADER),
        ("socket_address.h", crate::COMPLETION_SOCKET_ADDRESS_HEADER),
        ("linux_io_uring.h", crate::COMPLETION_LINUX_IO_URING_HEADER),
        ("windows_iocp.h", crate::COMPLETION_WINDOWS_IOCP_HEADER),
        ("sched/core.h", crate::SCHED_CORE_HEADER),
        ("sched/prim.h", crate::SCHED_PRIM_HEADER),
        ("sched/entry.h", crate::SCHED_ENTRY_HEADER),
    ] {
        for reserved in ["linux", "unix"] {
            for shape in [format!(" {reserved};"), format!(".{reserved}")] {
                assert!(
                    !source.contains(&shape),
                    "{name} spells `{shape}`, and the host compiler's default \
                     dialect predefines `{reserved}` as a macro"
                );
            }
        }
    }
}
