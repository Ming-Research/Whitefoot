/* W4rd-only observer, appended to a scratch copy of completion/bridge.c by
 * read-trace.py. No production runtime or emitted program is changed.
 * W4rd has exactly one sleeping context and one Shards object. Its first
 * exported clock reading after sleep is before the read, its next is after.
 * Scheduler clocks use wf_file_monotonic_ns directly and do not advance this
 * protocol. Retire this observer when the whole-read tail is attributed.
 */
#include <inttypes.h>
#include <sys/resource.h>

enum { WF_TRACE_OFF, WF_TRACE_ARMED, WF_TRACE_READING };
typedef struct {
    unsigned phase, executor_before, executor_after, executor_changed;
    unsigned segment_active, rusage_valid;
    uint64_t begun, wait_shards, spins, yields, wait_max_ns;
    int64_t wait_max_shard;
    uint64_t context_switches, driver_switches, service_yields, quiescence_ns, collects;
    wf_driver *driver_before;
    uint64_t driver_runs_before;
    uint64_t nvcsw, nivcsw, minflt, majflt;
    struct rusage segment;
} wf_read_observation;

/* The immutable context identity is published once. The observation itself
 * has one owner: that context, carried across migration by the normal frame
 * publication. Other contexts never read/write the observation. */
/* Readers on every executor inspect the immutable identity. Separate it
 * from the mutable observation so diagnostics add no false-sharing line. */
static _Alignas(64) _Atomic(wf_context *) wf_trace_reader;
static _Alignas(64) wf_read_observation wf_trace_read;
static wf_shards *wf_trace_object;
static _Thread_local uint64_t wf_trace_quiescence_started;
static _Thread_local unsigned wf_trace_returning;
typedef struct { _Alignas(64) _Atomic uint64_t runs; } wf_trace_driver_counter;
_Static_assert(sizeof(wf_trace_driver_counter) == 64u, "one diagnostic driver counter per line");
static wf_trace_driver_counter wf_trace_driver_runs[WF_DRIVER_LIMIT];

static int wf_trace_is_reader(wf_context *context) {
    return context != NULL
        && context == atomic_load_explicit(&wf_trace_reader, memory_order_acquire);
}

static int wf_trace_reading(void) {
    return wf_trace_is_reader(wf_context_current)
        && wf_trace_read.phase == WF_TRACE_READING;
}

static void wf_trace_segment_begin(void) {
    if (getrusage(RUSAGE_THREAD, &wf_trace_read.segment) != 0) {
        wf_trace_read.rusage_valid = 0;
        wf_trace_read.segment_active = 0;
    } else wf_trace_read.segment_active = 1;
}

static void wf_trace_segment_end(void) {
    if (!wf_trace_read.segment_active) return;
    struct rusage end;
    if (getrusage(RUSAGE_THREAD, &end) != 0
        || end.ru_nvcsw < wf_trace_read.segment.ru_nvcsw
        || end.ru_nivcsw < wf_trace_read.segment.ru_nivcsw
        || end.ru_minflt < wf_trace_read.segment.ru_minflt
        || end.ru_majflt < wf_trace_read.segment.ru_majflt) {
        wf_trace_read.rusage_valid = 0;
    } else {
        wf_trace_read.nvcsw += (uint64_t)(end.ru_nvcsw - wf_trace_read.segment.ru_nvcsw);
        wf_trace_read.nivcsw += (uint64_t)(end.ru_nivcsw - wf_trace_read.segment.ru_nivcsw);
        wf_trace_read.minflt += (uint64_t)(end.ru_minflt - wf_trace_read.segment.ru_minflt);
        wf_trace_read.majflt += (uint64_t)(end.ru_majflt - wf_trace_read.segment.ru_majflt);
    }
    wf_trace_read.segment_active = 0;
}

static void wf_read_trace_sleep(void) {
    wf_context *self = wf_context_current;
    if (self == NULL) wf_bridge_fail("W4rd trace sleep outside a context");
    wf_context *reader = atomic_load_explicit(&wf_trace_reader, memory_order_acquire);
    if (reader == NULL) {
        atomic_store_explicit(&wf_trace_reader, self, memory_order_release);
        fprintf(stderr, "WF_READ_TRACE enabled=1\n");
    } else if (reader != self) wf_bridge_fail("W4rd trace requires one sleeping context");
    if (wf_trace_read.phase == WF_TRACE_READING)
        wf_bridge_fail("W4rd trace sleep before ending read timestamp");
    wf_trace_read.phase = WF_TRACE_ARMED;
}

/* The baseline rusage syscall is BEFORE the original before timestamp, and
 * the final syscall is AFTER the original after timestamp. Samples slightly
 * enclose read_ns; they do not lengthen that interval with endpoint syscalls.
 * All diagnostic reads need the baseline; only slow reads are recorded. */
static void wf_read_trace_clock_before(void) {
    if (!wf_trace_is_reader(wf_context_current)
        || wf_trace_read.phase != WF_TRACE_ARMED) return;
    memset(&wf_trace_read, 0, sizeof(wf_trace_read));
    wf_trace_read.phase = WF_TRACE_ARMED;
    wf_trace_read.wait_max_shard = -1;
    wf_trace_read.rusage_valid = 1;
    wf_trace_read.executor_before = wf__map_user;
    wf_trace_read.executor_after = wf__map_user;
    wf_trace_read.driver_before = wf_driver_self;
    wf_trace_segment_begin();
}

static void wf_read_trace_clock_after(uint64_t now) {
    if (!wf_trace_is_reader(wf_context_current)) return;
    if (wf_trace_read.phase == WF_TRACE_ARMED) {
        wf_trace_read.begun = now;
        wf_trace_read.driver_runs_before = atomic_load_explicit(
            &wf_trace_driver_runs[wf_trace_read.driver_before->index].runs,
            memory_order_relaxed);
        wf_trace_read.phase = WF_TRACE_READING;
        return;
    }
    if (wf_trace_read.phase != WF_TRACE_READING) return;
    wf_trace_read.executor_after = wf__map_user;
    wf_trace_read.executor_changed |= wf__map_user != wf_trace_read.executor_before;
    wf_trace_read.driver_switches = atomic_load_explicit(
        &wf_trace_driver_runs[wf_trace_read.driver_before->index].runs,
        memory_order_relaxed) - wf_trace_read.driver_runs_before;
    wf_trace_segment_end();
    wf_trace_read.phase = WF_TRACE_OFF;
    if (now < wf_trace_read.begun) wf_bridge_fail("W4rd trace clock moved backwards");
    uint64_t span = now - wf_trace_read.begun;
    if (span <= UINT64_C(50000)) return;
    if (!wf_trace_read.rusage_valid) {
        wf_trace_read.nvcsw = wf_trace_read.nivcsw = 0;
        wf_trace_read.minflt = wf_trace_read.majflt = 0;
    }
    fprintf(stderr,
        "WF_READ_TAIL read_ns=%" PRIu64 " wait_shards=%" PRIu64
        " spins=%" PRIu64 " yields=%" PRIu64 " wait_max_ns=%" PRIu64
        " wait_max_shard=%" PRId64 " context_switches=%" PRIu64
        " executor_changed=%u nvcsw=%" PRIu64 " nivcsw=%" PRIu64
        " rusage_valid=%u minflt=%" PRIu64 " majflt=%" PRIu64
        " quiescence_ns=%" PRIu64 " collects=%" PRIu64
        " executor_before=%u executor_after=%u service_yields=%" PRIu64
        " driver_switches=%" PRIu64 "\n",
        span, wf_trace_read.wait_shards, wf_trace_read.spins, wf_trace_read.yields,
        wf_trace_read.wait_max_ns, wf_trace_read.wait_max_shard,
        wf_trace_read.context_switches, wf_trace_read.executor_changed,
        wf_trace_read.nvcsw, wf_trace_read.nivcsw, wf_trace_read.rusage_valid,
        wf_trace_read.minflt, wf_trace_read.majflt, wf_trace_read.quiescence_ns,
        wf_trace_read.collects, wf_trace_read.executor_before,
        wf_trace_read.executor_after, wf_trace_read.service_yields,
        wf_trace_read.driver_switches);
}

/* Called while SERVICE is still held, before publishing OUTSIDE. This is a
 * single service-owner load/store, not an RMW; a replacement acquires that
 * publication before it can update the same counter. The reader observes
 * atomically even if its old logical role runs peers on a replacement. */
static void wf_read_trace_run(wf_driver *driver) {
    _Atomic uint64_t *runs = &wf_trace_driver_runs[driver->index].runs;
    uint64_t count = atomic_load_explicit(runs, memory_order_relaxed);
    if (count == UINT64_MAX) wf_bridge_fail("W4rd trace driver count exhausted");
    atomic_store_explicit(runs, count + 1u, memory_order_relaxed);
}

static void wf_read_trace_resume(wf_context *context) {
    wf_trace_returning = 0;
    if (!wf_trace_is_reader(context) || wf_trace_read.phase != WF_TRACE_READING) return;
    wf_trace_read.executor_changed |= wf__map_user != wf_trace_read.executor_before;
    wf_trace_segment_begin();
}

static void wf_read_trace_unwind(wf_context *context) {
    if (!wf_trace_is_reader(context) || wf_trace_read.phase != WF_TRACE_READING) return;
    wf_trace_segment_end();
    wf_trace_read.context_switches += 1u;
    wf_trace_returning = 1;
}

/* Stop touching this context before adopt/ingress can publish it to a new
 * owner. Service-return retries happen with context_current already NULL. */
static void wf_read_trace_returned(void) { wf_trace_returning = 0; }

static void wf_read_trace_quiescence_begin(void) {
    wf_trace_quiescence_started = wf_trace_reading() ? wf_file_monotonic_ns() : 0;
}

static void wf_read_trace_quiescence_end(void) {
    if (wf_trace_quiescence_started != 0) {
        wf_trace_read.quiescence_ns += wf_file_monotonic_ns() - wf_trace_quiescence_started;
        wf_trace_quiescence_started = 0;
    }
}

static void wf_read_trace_collect(void) {
    if (wf_trace_reading()) wf_trace_read.collects += 1u;
}

static void wf_read_trace_service_yield(void) {
    if (wf_trace_returning) wf_trace_read.service_yields += 1u;
}

static void wf_read_trace_new_shards(void *object) {
    if (wf_trace_object != NULL) wf_bridge_fail("W4rd trace requires one shards object");
    wf_trace_object = object;
}

/* This is the SAME 64-pause/OS-yield ticket algorithm, observed only for the
 * reader's contended calls. Writer inline acquire/release instructions are
 * untouched. Clocks occur only on an actual wait. */
static int wf_read_trace_wait(void *opaque, uint64_t ticket) {
    if (!wf_trace_reading()) return 0;
    wf_shard_lock *lock = opaque;
    uintptr_t offset = (uintptr_t)opaque - (uintptr_t)wf_trace_object;
    if (wf_trace_object == NULL || offset < WF_SHARDS_HEADER_SIZE
        || (offset - WF_SHARDS_HEADER_SIZE) % wf_trace_object->stride != 0)
        wf_bridge_fail("W4rd trace unknown shard lock");
    uint64_t index = (offset - WF_SHARDS_HEADER_SIZE) / wf_trace_object->stride;
    if (index >= wf_trace_object->len) wf_bridge_fail("W4rd trace invalid shard index");
    uint64_t begun = wf_file_monotonic_ns();
    wf_trace_read.wait_shards += 1u;
    unsigned spins = 64u;
    while (atomic_load_explicit(&lock->serving, memory_order_acquire) != ticket) {
        if (spins != 0u) {
            --spins;
            wf_trace_read.spins += 1u;
            wf_prim_spin_hint();
        } else {
            wf_trace_read.yields += 1u;
            wf__runtime_yield();
            spins = 64u;
        }
    }
    uint64_t span = wf_file_monotonic_ns() - begun;
    if (wf_trace_read.wait_max_shard < 0 || span > wf_trace_read.wait_max_ns) {
        wf_trace_read.wait_max_ns = span;
        wf_trace_read.wait_max_shard = (int64_t)index;
    }
    return 1;
}
