#!/usr/bin/env python3
"""Build a W4rd-only observed runtime copy; never patch the checkout.

The compiler has no diagnostic native-runtime override. Reuse runtime.mk and
the original emitted LLVM, adding fail-closed observers to scratch runtime
sources. No acceptance, lock policy, clock result or scheduling policy changes.
Retire with this whole-reader attribution experiment. CI only.
"""
import shutil
import sys
from pathlib import Path


DECLARATIONS = """
/* Scratch W4rd observer definitions are appended after sharded_state.c. */
static void wf_read_trace_sleep(void);
static void wf_read_trace_clock_before(void);
static void wf_read_trace_clock_after(uint64_t);
static void wf_read_trace_resume(wf_context *);
static void wf_read_trace_unwind(wf_context *);
static void wf_read_trace_returned(void);
static void wf_read_trace_run(wf_driver *);
static void wf_read_trace_quiescence_begin(void);
static void wf_read_trace_quiescence_end(void);
static void wf_read_trace_collect(void);
static void wf_read_trace_service_yield(void);
static void wf_read_trace_new_shards(void *);
static int wf_read_trace_wait(void *, uint64_t);
"""

# A diagnostic must refuse a changed policy instead of replacing it with the
# observer's earlier copy. This exact body is the current production policy.
TICKET_WAIT = """void wf__shards_wait(void *lock, uint64_t ticket) {
    wf_shard_lock *shard = lock;
    unsigned spins = 64u;
    while (atomic_load_explicit(&shard->serving, memory_order_acquire) != ticket) {
        if (spins != 0u) {
            --spins;
            wf_prim_spin_hint();
        } else {
            /* The holder can be descheduled: keep the local pause loop
             * bounded, then let the host run it before polling again. */
            wf__runtime_yield();
            spins = 64u;
        }
    }
}"""

PATCHES = {
    "completion/bridge.c": [
        ("#if !defined(_POSIX_C_SOURCE)",
         "#ifndef _GNU_SOURCE\n#define _GNU_SOURCE 1\n#endif\n#if !defined(_POSIX_C_SOURCE)"),
        ("static void wf_context_set_current(wf_context *context) {",
         DECLARATIONS + "\nstatic void wf_context_set_current(wf_context *context) {"),
        ("static int wf_invocations_collect(uint64_t sequence[WF_EXECUTOR_LIMIT]) {",
         "static int wf_invocations_collect(uint64_t sequence[WF_EXECUTOR_LIMIT]) {\n    wf_read_trace_collect();"),
        ("static int wf_invocations_unchanged(const uint64_t sequence[WF_EXECUTOR_LIMIT]) {",
         "static int wf_invocations_unchanged(const uint64_t sequence[WF_EXECUTOR_LIMIT]) {\n    wf_read_trace_collect();"),
        ("    wf__sched_once(&wf_quiescence_once, wf_quiescence_init);\n    wf_prim_wait_lock(&wf_quiescence);",
         "    wf_read_trace_quiescence_begin();\n    wf__sched_once(&wf_quiescence_once, wf_quiescence_init);\n    wf_prim_wait_lock(&wf_quiescence);\n    wf_read_trace_quiescence_end();"),
        ("        wf_prim_yield();\n    }\n    wf_driver_service = driver;",
         "        wf_read_trace_service_yield();\n        wf_prim_yield();\n    }\n    wf_driver_service = driver;"),
        ("    wf__coro_resume(frame);\n    wf_context_set_current(NULL);",
         "    wf_read_trace_resume(next);\n    wf__coro_resume(frame);\n    wf_read_trace_unwind(next);\n    wf_context_set_current(NULL);"),
        ("    wf_invocation_begin();\n    uint64_t outside = wf_driver_leave_service(driver);",
         "    wf_invocation_begin();\n    wf_read_trace_run(driver);\n    uint64_t outside = wf_driver_leave_service(driver);"),
        ("    int retained = wf_driver_return_service(driver, outside);",
         "    int retained = wf_driver_return_service(driver, outside);\n    wf_read_trace_returned();"),
        ("void wf__completion_sleep_submit(\n    uint64_t deadline,\n    void *record\n) {",
         "void wf__completion_sleep_submit(\n    uint64_t deadline,\n    void *record\n) {\n    wf_read_trace_sleep();"),
        ("int wf__completion_sleep_watched_submit(uint64_t deadline, void *cancel, void *record) {",
         "int wf__completion_sleep_watched_submit(uint64_t deadline, void *cancel, void *record) {\n    wf_read_trace_sleep();"),
        ("uint64_t wf__completion_monotonic_ns(void) {\n    return wf_file_monotonic_ns();\n}",
         "uint64_t wf__completion_monotonic_ns(void) {\n    wf_read_trace_clock_before();\n    uint64_t now = wf_file_monotonic_ns();\n    wf_read_trace_clock_after(now);\n    return now;\n}"),
    ],
    "sharded_state.c": [
        ("    return object;\n}", "    wf_read_trace_new_shards(object);\n    return object;\n}"),
        (TICKET_WAIT, TICKET_WAIT.replace(
            "    wf_shard_lock *shard = lock;",
            "    if (wf_read_trace_wait(lock, ticket)) return;\n    wf_shard_lock *shard = lock;", 1)),
    ],
}


def replace_once(text, old, new):
    count = text.count(old)
    if count != 1:
        raise ValueError(f"expected exactly one observer anchor, found {count}: {old!r}")
    return text.replace(old, new, 1)


def changed_sources(root):
    # This observer is deliberately limited to the current W4rd clock/sleep
    # protocol. Extra exported clock calls would silently mislabel samples.
    reader = Path(__file__).with_name("sharded_reader.wf").read_text()
    if reader.count("std::time::now(clock: &clock)") != 3:
        raise ValueError("W4rd observer requires exactly three static clock calls")
    anchors = (
        "std::time::sleep_until(deadline: wake, cancel: &never);",
        "let before = std::time::now(clock: &clock);",
        "atomic all = &counter {",
        "let after = std::time::now(clock: &clock);",
    )
    if any(reader.count(anchor) != 1 for anchor in anchors):
        raise ValueError("W4rd observer read interval is missing or ambiguous")
    if list(map(reader.index, anchors)) != sorted(map(reader.index, anchors)):
        raise ValueError("W4rd observer clock/sleep order changed")
    changed = {}
    for relative, patches in PATCHES.items():
        text = (root / relative).read_text()
        if "wf_read_trace_" in text:
            raise ValueError(f"{relative}: observer already present")
        for old, new in patches:
            text = replace_once(text, old, new)
        changed[relative] = text
    observer = Path(__file__).with_suffix(".c").read_text()
    changed["completion/bridge.c"] += "\n" + observer
    return changed


def self_test(root):
    changed_sources(root)
    # Every missing/duplicated anchor must fail before any runtime is copied.
    for patches in PATCHES.values():
        for old, new in patches:
            assert replace_once(old, old, new) == new
            for wrong in ("", old + "\n" + old):
                try:
                    replace_once(wrong, old, new)
                except ValueError:
                    continue
                raise AssertionError("observer accepted a missing/duplicate anchor")
    print("W4rd observer anchors and missing/duplicate sensitivity controls passed")


def main(arguments):
    if len(arguments) == 2 and arguments[0] == "--self-test":
        self_test(Path(arguments[1]))
        return
    if len(arguments) != 2:
        raise ValueError("usage: read-trace.py BACKEND_ROOT SCRATCH_NATIVE_ROOT | --self-test BACKEND_ROOT")
    root, output = map(Path, arguments)
    changed = changed_sources(root)  # Check everything before writing anything.
    if output.exists():
        raise ValueError("scratch native output already exists")
    shutil.copytree(root, output)
    for relative, text in changed.items():
        (output / relative).write_text(text)
    (output.parent / "patches.txt").write_text(
        "W4rd observer only; original emitted LLVM and ticket policy unchanged.\n"
        + "\n".join(f"{relative}: {len(patches)} guarded observer insertions"
                    for relative, patches in PATCHES.items()) + "\n")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (OSError, ValueError) as error:
        sys.exit(f"invalid W4rd observer: {error}")
