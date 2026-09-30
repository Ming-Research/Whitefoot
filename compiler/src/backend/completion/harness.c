#if !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE 1
#endif

#include "contract.h"
#include "bridge.h"
#include "socket_test.h"
#include "file_adapter.h"
#include "file_posix.h"
#include "../sched/core.h"
#include "../sched/entry.h"

#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <pthread.h>
#include <poll.h>
#include <sched.h>
#include <stdarg.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <netinet/in.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

/* One operation record block, exactly as an ordinary linked body reserves it:
 * the size and alignment the contract states, and no knowledge of the layout. */
typedef struct wf_harness_record {
    _Alignas(WF_COMPLETION_RECORD_ALIGN)
        unsigned char bytes[WF_COMPLETION_RECORD_BYTES];
} wf_harness_record;

#define CHECK(condition)                                                      \
    do {                                                                      \
        if (!(condition)) {                                                   \
            fprintf(                                                          \
                stderr,                                                       \
                "completion harness: %s:%d: check failed: %s\n",            \
                __FILE__,                                                     \
                __LINE__,                                                     \
                #condition                                                    \
            );                                                                \
            return 1;                                                         \
        }                                                                     \
    } while (0)

#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
static _Atomic int wf_directory_scripted;
static unsigned wf_directory_host_calls;
static unsigned wf_directory_poll_calls;
static int wf_directory_poll_descriptor;
static short wf_directory_poll_events;
static int wf_directory_poll_timeout;

/* Deterministic enumeration answers for whichever family this build has:
 * interruption, readiness refusal, then one successful byte. The pipe
 * descriptor used by the test is already readable when the adapter performs
 * its exact POLLIN wait.
 *
 * One body serves both families; only the signature differs, because Darwin's
 * facility carries a base-position cell and Linux's keeps the whole cursor in
 * the descriptor. The facility this stands in for is selected by the same
 * macro the file adapter uses, so the adapter is exercised exactly as shipped
 * with only the host call replaced. */
static ssize_t wf_completion_test_directory_batch(void *buffer, size_t count) {
    wf_directory_host_calls += 1;
    if (wf_directory_host_calls == 1) {
        errno = EINTR;
        return -1;
    }
    if (wf_directory_host_calls == 2) {
        errno = EAGAIN;
        return -1;
    }
    if (buffer == NULL || count == 0) {
        errno = EINVAL;
        return -1;
    }
    ((unsigned char *)buffer)[0] = 'd';
    return 1;
}

#if defined(__APPLE__)
ssize_t wf_completion_test_getdirentries64(
    int descriptor,
    void *buffer,
    size_t count,
    int64_t *position
) {
    ssize_t reported;
    if (!atomic_load(&wf_directory_scripted)) {
        extern ssize_t __getdirentries64(int, void *, size_t, int64_t *);
        return __getdirentries64(descriptor, buffer, count, position);
    }
    if (position == NULL) {
        wf_directory_host_calls += 1;
        errno = EINVAL;
        return -1;
    }
    reported = wf_completion_test_directory_batch(buffer, count);
    if (reported > 0) {
        *position = 17;
    }
    return reported;
}
#else
ssize_t wf_completion_test_getdents64(int descriptor, void *buffer, size_t count) {
    if (!atomic_load(&wf_directory_scripted)) {
        extern ssize_t getdents64(int, void *, size_t);
        return getdents64(descriptor, buffer, count);
    }
    return wf_completion_test_directory_batch(buffer, count);
}
#endif
#endif

/* The adapter's own `openat`, named so this build performs the same host
 * call the shipped build does. */
int wf_completion_test_openat(
    int directory,
    const char *path,
    int flags,
    ...
) {
    if ((flags & O_CREAT) != 0) {
        va_list arguments;
        mode_t mode;
        va_start(arguments, flags);
        mode = (mode_t)va_arg(arguments, int);
        va_end(arguments);
        return openat(directory, path, flags, mode);
    }
    return openat(directory, path, flags);
}

/* Only the current empty-pipe case arms this observer. Publish the host
 * wait arguments before letting its writer make the descriptor readable. */
static _Atomic int wf_observed_poll_fd = -1;
static _Atomic unsigned wf_observed_poll_calls;
static short wf_observed_poll_events;
static int wf_observed_poll_timeout;

int wf_completion_test_poll(
    struct pollfd *descriptors,
    nfds_t count,
    int timeout
) {
    if (count == 1 && descriptors[0].fd == atomic_load(&wf_observed_poll_fd)) {
        wf_observed_poll_events = descriptors[0].events;
        wf_observed_poll_timeout = timeout;
        atomic_fetch_add_explicit(&wf_observed_poll_calls, 1u, memory_order_release);
    }
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
    if (atomic_load(&wf_directory_scripted)) {
        wf_directory_poll_calls += 1;
        wf_directory_poll_descriptor = count == 1 ? descriptors[0].fd : -1;
        wf_directory_poll_events = count == 1 ? descriptors[0].events : 0;
        wf_directory_poll_timeout = timeout;
    }
#endif
    return poll(descriptors, count, timeout);
}

/* ------------------------------------------------------------- retirements
 *
 * Retired with the record pool (design §7, "The record's pool machinery:
 * deleted, not answered").  Every property each of these asserted was a
 * property of that pool, and the pool is gone rather than answered: the record
 * is a block of the submitting frame, nothing claims it, nothing recycles it,
 * and no queue anywhere can refuse an operation.
 *
 *   - `test_capacity_and_product_state`: the slot phases, the milestone
 *     product and WF_COMPLETION_OWNERSHIP_COMPLETE.  A record has one state
 *     that goes PENDING to DONE once, and no product of independent facts.
 *   - `test_generation_and_duplicate_terminal`: the generation and the
 *     duplicate-terminal refusal.  Exactly one terminal per submission is now
 *     an impossibility rather than a checked refusal, and it is tested in its
 *     new form by `test_exactly_one_completion_per_submission_under_race`.
 *   - `test_named_drain_refuses_a_recycled_slot`: a recycled slot.  There is
 *     no slot and no recycling; the frame joins its record before any
 *     terminator, so no publisher can meet a reused one.
 *   - `test_concurrent_single_claims_are_unique`: claim uniqueness.  There is
 *     nothing to claim; each frame supplies its own record, which is what the
 *     race test above submits from many threads at once.
 *   - `test_bounded_drain_and_lane_independence`: the bounded drain and the
 *     ready-event count it moved.  A completion is published straight into its
 *     record and there is no event to drain.
 *   - `test_drain_wakes_the_registered_token_owner`: the consume-wait
 *     registration.  Its property -- a completion elsewhere wakes the thread
 *     that registered for it -- survives as the in-place waiter, and is tested
 *     by `test_a_completion_publishes_results` and
 *     `test_bridge_read_completes_after_observed_wait`.
 *   - `test_bridge_capacity_falls_back_per_operation` and
 *     `test_open_capacity_refuses_and_resubmits`: the per-operation capacity
 *     fallback and the readmission after a release.  There is no capacity to
 *     exhaust, no refusal to fall back from, and the owner's rule forbids a
 *     per-operation blocking fallback; the property that replaces both is
 *     `test_submissions_cross_the_native_queue_boundary`.
 *   - `test_capacity_release_wakes_before_blocking_work`: the capacity
 *     notification and the ordering it bought.  `wf_completion_notify_capacity`
 *     and its six callers are deleted with the capacity they announced.
 *   - `test_a_name_no_record_can_hold_opens_directly`: the path-does-not-fit
 *     demotion and its counter.  An open's path bytes are the submitting
 *     frame's own and are never copied, so no name can be too long for a
 *     record; the property that replaces it is
 *     `test_long_borrowed_path_reaches_the_host`.
 *   - The overwrite arm of `test_submitted_open_owns_its_path_bytes`, which
 *     rewrote the caller's buffer immediately after submitting.  The bytes are
 *     the frame's and (ordinary native library)'s loan on them holds until the join (design §5),
 *     so rewriting them while the operation is outstanding is no longer a
 *     thing a conforming caller does; what remains -- that an open resolves
 *     the name it was given -- is asserted by
 *     `test_submitted_open_resolves_the_submitters_bytes`.
 */

/* ---------------------------------------------------------------- helpers */

typedef struct park_context {
    wf_completion_runtime *runtime;
    uint64_t epoch;
    enum wf_completion_park_result result;
} park_context;

static _Atomic(wf_completion_wait *) watched_wait;
static _Atomic unsigned watched_returns;

void wf_completion_test_wait_return(wf_completion_wait *wait) {
    if (atomic_load_explicit(&watched_wait, memory_order_acquire) == wait) {
        atomic_fetch_add_explicit(&watched_returns, 1u, memory_order_release);
    }
}

static void *park_thread(void *opaque) {
    park_context *context = opaque;
    context->result = wf_completion_park_if_unchanged(
        context->runtime,
        context->epoch,
        5000
    );
    return NULL;
}

static int wait_until_parked_count(
    wf_completion_runtime *runtime,
    unsigned expected
) {
    struct timespec delay = {.tv_sec = 0, .tv_nsec = 1000000};
    unsigned attempts;
    for (attempts = 0; attempts < 5000; ++attempts) {
        if (wf_completion_parked_scheduler_count(runtime) == expected) {
            return 0;
        }
        (void)nanosleep(&delay, NULL);
    }
    return 1;
}

static int wait_until_parked(wf_completion_runtime *runtime) {
    return wait_until_parked_count(runtime, 1u);
}

static void record_host_wake(void *context) {
    _Atomic unsigned *count = context;
    atomic_fetch_add_explicit(count, 1, memory_order_relaxed);
}

/* One record, filled the way a submit fills it, for the cases that drive an
 * adapter directly rather than through the bridge. */
static void harness_record_init(
    wf_completion_record *record,
    enum wf_file_operation_kind kind
) {
    memset(record, 0, sizeof(*record));
    wf_completion_record_init(record);
    record->request.kind = kind;
    record->opened_descriptor = -1;
    record->open_outcome = WF_FILE_OPEN_SUCCEEDED;
}

static int harness_record_done(const wf_completion_record *record) {
    return atomic_load_explicit(&record->state, memory_order_acquire)
        == WF_COMPLETION_DONE;
}

/* Submits one record to the bounded adapter and runs it on this thread. */
static int harness_run_on_adapter(
    wf_file_adapter *adapter,
    wf_completion_record *record
) {
    CHECK(wf_file_adapter_submit(adapter, record) == WF_FILE_TARGET_OWNS);
    CHECK(wf_file_adapter_progress(adapter, 1) == 1);
    CHECK(harness_record_done(record));
    return 0;
}

/* ------------------------------------------------- the record protocol */

#define WF_HARNESS_RACERS 12
#define WF_HARNESS_RACER_ROUNDS 64

typedef struct submission_race_context {
    int descriptor;
    unsigned lane;
    _Atomic unsigned *start;
    int failed;
} submission_race_context;

/* One lane submitting and joining through records on its own frames.
 *
 * Every record here is a block of this thread's own stack frame, which is what
 * the emitter reserves (design §5).  No two lanes can name one record, so the
 * property the old token pool checked by refusing a second terminal is checked
 * here by construction: each submission has exactly one publication, and the
 * whole run's publication count says so. */
static void *submission_racer(void *opaque) {
    submission_race_context *context = opaque;
    unsigned round;
    while (atomic_load_explicit(context->start, memory_order_acquire) == 0) {
        sched_yield();
    }
    for (round = 0; round < WF_HARNESS_RACER_ROUNDS; ++round) {
        wf_harness_record record;
        unsigned char byte = 0xff;
        uint64_t offset = (uint64_t)((context->lane + round) % 8u);
        int64_t value = -1;
        int error = -1;
        wf__completion_file_pread_submit(
            context->descriptor,
            &byte,
            1,
            offset,
            record.bytes
        );
        wf__completion_file_join(record.bytes, &value, &error);
        if (value != 1 || error != 0
            || byte != (unsigned char)('a' + offset)) {
            context->failed = 1;
            return NULL;
        }
    }
    return NULL;
}

/* Exactly one completion per submission, under a race.
 *
 * This is what replaces the token pool's duplicate-terminal refusal and its
 * claim-uniqueness check: twelve threads submit and join at once, each through
 * records on its own frames, and the runtime publishes exactly one completion
 * for each submission.  A second publication would show as a publication count
 * above the submission count; a lost one would hang the join, which the
 * harness watchdog reports by name. */
static int test_exactly_one_completion_per_submission_under_race(
    const char *scratch_directory
) {
    pthread_t threads[WF_HARNESS_RACERS];
    submission_race_context contexts[WF_HARNESS_RACERS];
    _Atomic unsigned start;
    uint64_t publications_before;
    uint64_t publications_after;
    char path[256];
    int descriptor;
    unsigned index;

    CHECK(scratch_directory != NULL);
    CHECK(
        snprintf(
            path,
            sizeof(path),
            "%s/wf-completion-race-%ld",
            scratch_directory,
            (long)getpid()
        ) > 0
    );
    (void)unlink(path);
    descriptor = open(path, O_CREAT | O_EXCL | O_RDWR, 0600);
    CHECK(descriptor >= 0);
    CHECK(write(descriptor, "abcdefgh", 8) == 8);

    atomic_init(&start, 0);
    publications_before = wf__completion_publications();
    for (index = 0; index < WF_HARNESS_RACERS; ++index) {
        contexts[index].descriptor = descriptor;
        contexts[index].lane = index;
        contexts[index].start = &start;
        contexts[index].failed = 0;
        CHECK(
            pthread_create(
                &threads[index],
                NULL,
                submission_racer,
                &contexts[index]
            ) == 0
        );
    }
    atomic_store_explicit(&start, 1, memory_order_release);
    for (index = 0; index < WF_HARNESS_RACERS; ++index) {
        CHECK(pthread_join(threads[index], NULL) == 0);
        CHECK(contexts[index].failed == 0);
    }
    publications_after = wf__completion_publications();
    CHECK(
        publications_after - publications_before
        == (uint64_t)WF_HARNESS_RACERS * WF_HARNESS_RACER_ROUNDS
    );

    CHECK(close(descriptor) == 0);
    CHECK(unlink(path) == 0);
    return 0;
}

/* Publication makes a record's result available through DONE, including
 * when no caller has registered a wait. This checks record publication;
 * the concurrent wake protocol is exercised separately. */
static int test_a_completion_publishes_results(void) {
    wf_completion_record record;

    harness_record_init(&record, WF_FILE_PREAD);
    CHECK(record.state == WF_COMPLETION_PENDING);
    record.result.kind = WF_FILE_PREAD;
    record.result.value = 7;
    record.result.error_code = EACCES;
    record.result.open_outcome = WF_FILE_OPEN_FAILED;
    wf_completion_record_complete(&record);
    CHECK(harness_record_done(&record));
    CHECK(record.result.kind == WF_FILE_PREAD);
    CHECK(record.result.value == 7);
    CHECK(record.result.error_code == EACCES);
    CHECK(record.result.open_outcome == WF_FILE_OPEN_FAILED);

    /* A record no one waits on is published just the same, and the publisher
     * touches nothing after DONE. */
    harness_record_init(&record, WF_FILE_CLOSE);
    record.result.kind = WF_FILE_CLOSE;
    wf_completion_record_complete(&record);
    CHECK(harness_record_done(&record));
    return 0;
}

/* ------------------------------------------------------- the wake epoch */

static int test_unified_wake_epoch(void) {
    wf_completion_runtime runtime;
    wf_completion_statistics before;
    wf_completion_statistics after;
    park_context parked;
    pthread_t thread;
    _Atomic unsigned host_wakes;
    uint64_t epoch_before;

    atomic_init(&host_wakes, 0);
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(
        wf_completion_set_wake_callback(
            &runtime,
            record_host_wake,
            &host_wakes
        ) == 0
    );
    before = wf_completion_statistics_snapshot(&runtime);
    epoch_before = wf_completion_wake_epoch(&runtime);
    /* A transition with no announced sleeper raises the epoch and performs no
     * host wake at all. */
    wf_completion_notify_target(&runtime);
    after = wf_completion_statistics_snapshot(&runtime);
    CHECK(after.wake_signals == before.wake_signals);
    CHECK(after.target_notifications == before.target_notifications + 1);
    CHECK(atomic_load_explicit(&host_wakes, memory_order_relaxed) == 0);
    CHECK(wf_completion_wake_epoch(&runtime) == epoch_before + 1);
    /* And a park against the epoch it raised does not sleep. */
    CHECK(
        wf_completion_park_if_unchanged(&runtime, epoch_before, 1)
        == WF_COMPLETION_PARK_EPOCH_CHANGED
    );

    /* An announced sleeper is woken once, through the host endpoint and the
     * condition variable both. */
    parked.runtime = &runtime;
    parked.epoch = wf_completion_wake_epoch(&runtime);
    parked.result = WF_COMPLETION_PARK_FAILED;
    CHECK(pthread_create(&thread, NULL, park_thread, &parked) == 0);
    CHECK(wait_until_parked(&runtime) == 0);
    /* Pass the final epoch recheck before notifying: observing the count
     * alone also permits a not-yet-sleeping thread to cancel its park. */
    wf_completion_wait_lock(&runtime.wait);
    wf_completion_wait_unlock(&runtime.wait);
    before = wf_completion_statistics_snapshot(&runtime);
    wf_completion_notify_compute(&runtime);
    CHECK(pthread_join(thread, NULL) == 0);
    after = wf_completion_statistics_snapshot(&runtime);
    CHECK(parked.result == WF_COMPLETION_PARK_WOKEN);
    CHECK(after.wake_signals == before.wake_signals + 1);
    CHECK(after.compute_notifications == before.compute_notifications + 1);
    CHECK(atomic_load_explicit(&host_wakes, memory_order_relaxed) == 1);

    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    return 0;
}

/* A delayed notifier's final locked reset/broadcast can reach a newer
 * waiter that already captured its epoch. Deliver that exact tail, observe
 * the real condition wait returning, then require a later notification to
 * wake the re-sleeping waiter. No sleep duration selects the interleaving. */
static int test_equal_epoch_notification_rearms_before_resleep(void) {
    wf_completion_runtime runtime;
    park_context parked;
    pthread_t thread;
    unsigned attempts;
    unsigned rearmed;
    struct timespec delay = {.tv_sec = 0, .tv_nsec = 1000000};

    CHECK(wf_completion_runtime_init(&runtime) == 0);
    wf_completion_notify_compute(&runtime);
    parked.runtime = &runtime;
    parked.epoch = wf_completion_wake_epoch(&runtime);
    parked.result = WF_COMPLETION_PARK_FAILED;
    atomic_store_explicit(&watched_returns, 0u, memory_order_relaxed);
    atomic_store_explicit(&watched_wait, &runtime.wait, memory_order_release);
    CHECK(pthread_create(&thread, NULL, park_thread, &parked) == 0);
    CHECK(wait_until_parked(&runtime) == 0);
    wf_completion_wait_lock(&runtime.wait);
    atomic_store_explicit(&watched_returns, 0u, memory_order_relaxed);
    atomic_store_explicit(&runtime.wake_needed, 0u, memory_order_seq_cst);
    wf_completion_wait_wake(&runtime.wait, 1);
    wf_completion_wait_unlock(&runtime.wait);
    for (attempts = 0; attempts < 5000; ++attempts) {
        if (atomic_load_explicit(&watched_returns, memory_order_acquire) != 0u) {
            break;
        }
        (void)nanosleep(&delay, NULL);
    }
    CHECK(attempts != 5000);
    /* The return hook runs with this mutex held. Taking it after observing
     * the hook passes the waiter's next loop iteration and actual re-sleep. */
    wf_completion_wait_lock(&runtime.wait);
    rearmed = atomic_load_explicit(&runtime.wake_needed, memory_order_relaxed);
    wf_completion_wait_unlock(&runtime.wait);
    wf_completion_notify_compute(&runtime);
    CHECK(pthread_join(thread, NULL) == 0);
    atomic_store_explicit(&watched_wait, NULL, memory_order_release);
    CHECK(rearmed == 1u);
    CHECK(parked.result == WF_COMPLETION_PARK_WOKEN);
    CHECK(wf_completion_parked_scheduler_count(&runtime) == 0u);
    CHECK(wf_completion_statistics_snapshot(&runtime).parks == 1u);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    return 0;
}

static int test_one_epoch_wakes_every_announced_thread(void) {
    wf_completion_runtime runtime;
    wf_completion_statistics before;
    wf_completion_statistics after;
    park_context parked[2];
    pthread_t threads[2];
    _Atomic unsigned host_wakes;
    uint64_t epoch;
    size_t index;

    atomic_init(&host_wakes, 0);
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(
        wf_completion_set_wake_callback(
            &runtime,
            record_host_wake,
            &host_wakes
        ) == 0
    );
    epoch = wf_completion_wake_epoch(&runtime);
    for (index = 0; index < 2; ++index) {
        parked[index].runtime = &runtime;
        parked[index].epoch = epoch;
        parked[index].result = WF_COMPLETION_PARK_FAILED;
        CHECK(
            pthread_create(
                &threads[index],
                NULL,
                park_thread,
                &parked[index]
            ) == 0
        );
    }
    CHECK(wait_until_parked_count(&runtime, 2u) == 0);
    wf_completion_wait_lock(&runtime.wait);
    wf_completion_wait_unlock(&runtime.wait);
    before = wf_completion_statistics_snapshot(&runtime);
    wf_completion_notify_compute(&runtime);
    for (index = 0; index < 2; ++index) {
        CHECK(pthread_join(threads[index], NULL) == 0);
        CHECK(parked[index].result == WF_COMPLETION_PARK_WOKEN);
    }
    after = wf_completion_statistics_snapshot(&runtime);
    CHECK(after.compute_notifications == before.compute_notifications + 1);
    CHECK(after.wake_signals == before.wake_signals + 1);
    CHECK(atomic_load_explicit(&host_wakes, memory_order_relaxed) == 1);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    return 0;
}

static int test_condition_notifications_coalesce_without_suppressing_external_wakes(void) {
    wf_completion_runtime runtime;
    _Atomic unsigned host_wakes;
    uint64_t epoch;
    unsigned index;

    atomic_init(&host_wakes, 0);
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    epoch = wf_completion_wake_epoch(&runtime);

    /* Model a condition waiter whose notified consumer has not resumed to
     * withdraw its announcement. No actual host sleep is needed for this
     * interval; the real-thread test above checks delivery to every sleeper. */
    wf_completion_wait_lock(&runtime.wait);
    wf_completion_announce_park_locked(&runtime);
    wf_completion_wait_unlock(&runtime.wait);
    for (index = 0; index < 3u; index += 1u) {
        wf_completion_notify_target(&runtime);
    }
    CHECK(wf_completion_wake_epoch(&runtime) == epoch + 3u);
    CHECK(wf_completion_statistics_snapshot(&runtime).wake_signals == 1u);

    /* A new announcement must rearm notification even while the old waiter
     * remains announced. Its newer epoch cannot inherit the earlier signal. */
    wf_completion_wait_lock(&runtime.wait);
    wf_completion_announce_park_locked(&runtime);
    wf_completion_wait_unlock(&runtime.wait);
    for (index = 0; index < 3u; index += 1u) {
        wf_completion_notify_compute(&runtime);
    }
    CHECK(wf_completion_wake_epoch(&runtime) == epoch + 6u);
    CHECK(wf_completion_statistics_snapshot(&runtime).wake_signals == 2u);
    CHECK(wf_completion_parked_scheduler_count(&runtime) == 2u);

    wf_completion_wait_lock(&runtime.wait);
    atomic_fetch_sub_explicit(&runtime.parked_schedulers, 2u, memory_order_relaxed);
    wf_completion_wait_unlock(&runtime.wait);
    wf_completion_notify_target(&runtime);
    CHECK(wf_completion_statistics_snapshot(&runtime).wake_signals == 2u);

    /* A cancelled announcement leaves no sleeper. A notification clears its
     * pending flag, and a later real announcement still rearms the endpoint. */
    wf_completion_wait_lock(&runtime.wait);
    wf_completion_announce_park_locked(&runtime);
    atomic_fetch_sub_explicit(&runtime.parked_schedulers, 1u, memory_order_relaxed);
    wf_completion_wait_unlock(&runtime.wait);
    wf_completion_notify_target(&runtime);
    CHECK(wf_completion_statistics_snapshot(&runtime).wake_signals == 2u);
    wf_completion_wait_lock(&runtime.wait);
    wf_completion_announce_park_locked(&runtime);
    wf_completion_wait_unlock(&runtime.wait);
    wf_completion_notify_target(&runtime);
    CHECK(wf_completion_statistics_snapshot(&runtime).wake_signals == 3u);
    wf_completion_wait_lock(&runtime.wait);
    atomic_fetch_sub_explicit(&runtime.parked_schedulers, 1u, memory_order_relaxed);
    wf_completion_wait_unlock(&runtime.wait);

    /* External callbacks retain the original per-publication behavior: a
     * consumable port token does not imply every old waiter has been woken. */
    CHECK(wf_completion_set_wake_callback(&runtime, record_host_wake, &host_wakes) == 0);
    wf_completion_wait_lock(&runtime.wait);
    wf_completion_announce_park_locked(&runtime);
    wf_completion_wait_unlock(&runtime.wait);
    for (index = 0; index < 3u; index += 1u) {
        wf_completion_notify_target(&runtime);
    }
    CHECK(atomic_load_explicit(&host_wakes, memory_order_relaxed) == 3u);
    wf_completion_wait_lock(&runtime.wait);
    atomic_fetch_sub_explicit(&runtime.parked_schedulers, 1u, memory_order_relaxed);
    wf_completion_wait_unlock(&runtime.wait);
    wf_completion_notify_target(&runtime);
    CHECK(atomic_load_explicit(&host_wakes, memory_order_relaxed) == 3u);

    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    return 0;
}

/* --------------------------------------------------------- the adapters */

/* Every typed operation the bounded adapter carries, driven by one thread
 * that is itself the queue's only engine.
 *
 * The backpressure arm this case used to carry -- a queue of one, a second
 * submission answered WAIT_CAPACITY, and the same request accepted after a
 * progress pass -- is retired with the fixed queue (design §7).  The list is
 * threaded through the records, so a second submission is queued rather than
 * refused, and this case now submits two before running either. */
static int test_single_thread_file_progress(const char *scratch_directory) {
    wf_completion_runtime runtime;
    wf_file_adapter adapter;
    wf_completion_record open_record;
    wf_completion_record bad_close;
    wf_completion_record operation;
    char name[96];
    char read_back[32] = {0};
    const char payload[] = "completion-core";
    int root;
    int descriptor;

    CHECK(scratch_directory != NULL);
    root = open(scratch_directory, O_RDONLY | O_DIRECTORY);
    CHECK(root >= 0);
    (void)snprintf(
        name,
        sizeof(name),
        "wf-completion-harness-%ld",
        (long)getpid()
    );
    (void)unlinkat(root, name, 0);

    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 0, 0) == 0);

    harness_record_init(&open_record, WF_FILE_OPEN_AT);
    open_record.request.operation.open_at.directory = root;
    open_record.request.operation.open_at.path = name;
    open_record.request.operation.open_at.flags = O_CREAT | O_EXCL | O_RDWR;
    open_record.request.operation.open_at.mode = 0600;
    open_record.request.operation.open_at.has_mode = 1;
    harness_record_init(&bad_close, WF_FILE_CLOSE);
    bad_close.request.operation.close.descriptor = -1;

    /* Two operations queued at once: the list has no capacity to refuse. */
    CHECK(
        wf_file_adapter_submit(&adapter, &open_record) == WF_FILE_TARGET_OWNS
    );
    CHECK(
        wf_file_adapter_submit(&adapter, &bad_close) == WF_FILE_TARGET_OWNS
    );
    CHECK(wf_file_adapter_queued(&adapter) == 2);
    CHECK(wf_file_adapter_progress(&adapter, 2) == 2);
    CHECK(harness_record_done(&open_record));
    CHECK(harness_record_done(&bad_close));
    CHECK(open_record.result.error_code == 0);
    CHECK(open_record.result.value >= 0);
    descriptor = (int)open_record.result.value;
    CHECK(bad_close.result.value == -1);
    CHECK(bad_close.result.error_code == EBADF);

    /* Seed through the host; positioned-write is not a delivered WF API. */
    CHECK(pwrite(descriptor, payload, sizeof(payload) - 1, 0)
          == (ssize_t)(sizeof(payload) - 1));

    harness_record_init(&operation, WF_FILE_PREAD);
    operation.request.operation.pread.descriptor = descriptor;
    operation.request.operation.pread.buffer = read_back;
    operation.request.operation.pread.count = sizeof(payload) - 1;
    operation.request.operation.pread.offset = 0;
    CHECK(harness_run_on_adapter(&adapter, &operation) == 0);
    CHECK(operation.result.error_code == 0);
    CHECK(operation.result.value == (int64_t)(sizeof(payload) - 1));
    CHECK(memcmp(read_back, payload, sizeof(payload) - 1) == 0);

    harness_record_init(&operation, WF_FILE_CLOSE);
    operation.request.operation.close.descriptor = descriptor;
    CHECK(harness_run_on_adapter(&adapter, &operation) == 0);
    CHECK(operation.result.error_code == 0);
    CHECK(operation.result.value == 0);

    CHECK(wf_file_adapter_shutdown(&adapter) == 0);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    CHECK(unlinkat(root, name, 0) == 0);
    CHECK(close(root) == 0);
    return 0;
}

/* ---------------------------------------------------------- the bridge */

static int test_bridge_independent_positioned_reads(
    const char *scratch_directory
) {
    wf_harness_record first_record;
    wf_harness_record second_record;
    int64_t first_value = -1;
    int64_t second_value = -1;
    int first_error = -1;
    int second_error = -1;
    char first[5] = {0};
    char second[5] = {0};
    const char payload[] = "zeroONE-twoTHREE";
    char path[256];
    int descriptor;
    uint64_t native_before = wf__completion_native_ring_submissions();
    uint64_t fallback_before = wf__completion_file_fallback_submissions();
    uint64_t submissions_before = wf__completion_file_submissions();

    CHECK(scratch_directory != NULL);
    CHECK(
        snprintf(
            path,
            sizeof(path),
            "%s/wf-completion-pread-%ld",
            scratch_directory,
            (long)getpid()
        ) > 0
    );
    (void)unlink(path);
    descriptor = open(path, O_CREAT | O_EXCL | O_RDWR, 0600);
    CHECK(descriptor >= 0);
    CHECK(
        pwrite(descriptor, payload, sizeof(payload) - 1, 0)
        == (ssize_t)(sizeof(payload) - 1)
    );

    /* Both records are submitted before either result is joined.  The two
     * requests name one descriptor but independent file offsets; pread leaves
     * the shared cursor out of their semantics.
     *
     * A written WF_IO_HELPERS keeps both on the queued route, which is what
     * the submission count below asserts. */
    wf__completion_file_pread_submit(
        descriptor,
        first,
        4,
        4,
        first_record.bytes
    );
    wf__completion_file_pread_submit(
        descriptor,
        second,
        4,
        11,
        second_record.bytes
    );
    wf__completion_file_join(
        second_record.bytes,
        &second_value,
        &second_error
    );
    wf__completion_file_join(first_record.bytes, &first_value, &first_error);
    CHECK(first_error == 0);
    CHECK(second_error == 0);
    CHECK(first_value == 4);
    CHECK(second_value == 4);
    CHECK(memcmp(first, "ONE-", 4) == 0);
    CHECK(memcmp(second, "THRE", 4) == 0);
    CHECK(wf__completion_file_submissions() == submissions_before + 2u);
    if (wf__completion_native_ring_submissions() == native_before + 2u) {
        CHECK(wf__completion_file_fallback_submissions() == fallback_before);
    } else {
        CHECK(getenv("WF_REQUIRE_LINUX_IO_URING") == NULL);
        CHECK(wf__completion_native_ring_submissions() == native_before);
        CHECK(wf__completion_file_fallback_submissions() == fallback_before + 2u);
    }

    /* An offset the signed native type cannot represent is a refusal the host
     * itself would make, not a contract violation: a writer may spell any
     * `u64` offset, and there is no direct wrapper left to take the shape and
     * answer the host's EINVAL (design section 8, "One lowering for every I/O
     * operation").  The submit therefore publishes that same answer into the
     * record -- `value = -1`, `error_code = EINVAL` -- so the join reads what
     * the direct route used to read, one publication is counted for it, and
     * nothing was executed. */
    {
        wf_harness_record refused_record;
        uint64_t publications_before = wf__completion_publications();
        uint64_t executions_before = wf__completion_inline_executions();
        char refused[5] = {0};
        int64_t refused_value = 0;
        int refused_error = 0;
        wf__completion_file_pread_submit(
            descriptor,
            refused,
            4,
            (uint64_t)INT64_MAX + 1u,
            refused_record.bytes
        );
        wf__completion_file_join(
            refused_record.bytes,
            &refused_value,
            &refused_error
        );
        CHECK(refused_value == -1);
        CHECK(refused_error == EINVAL);
        CHECK(wf__completion_publications() == publications_before + 1u);
        CHECK(wf__completion_inline_executions() == executions_before);
        CHECK(memcmp(refused, "\0\0\0\0", 4) == 0);
    }

    CHECK(close(descriptor) == 0);
    CHECK(unlink(path) == 0);
    return 0;
}

/* One controlled fixture covers typed acquisition, kind refusals, absent
 * names and descriptor disposal. Each row names its complete outcome. */
static int test_bridge_typed_open_outcomes(const char *scratch_directory) {
    char regular[256], fifo[256], missing[256];
    CHECK(snprintf(regular, sizeof(regular), "%s/wf-open-regular-%ld",
          scratch_directory, (long)getpid()) > 0);
    CHECK(snprintf(fifo, sizeof(fifo), "%s/wf-open-fifo-%ld",
          scratch_directory, (long)getpid()) > 0);
    CHECK(snprintf(missing, sizeof(missing), "%s/wf-open-missing-%ld",
          scratch_directory, (long)getpid()) > 0);
    (void)unlink(regular);
    (void)unlink(fifo);
    (void)unlink(missing);
    CHECK(mkfifo(fifo, 0600) == 0);
    struct open_case {
        const char *name;
        const char *path;
        int flags;
        enum wf_file_expected_kind kind;
        unsigned outcome;
        int error;
        int close_twice;
    } rows[] = {
        {"regular creation and repeated close", regular, O_CREAT | O_EXCL | O_RDWR,
         WF_FILE_EXPECT_REGULAR, WF_FILE_OPEN_SUCCEEDED, 0, 1},
        {"FIFO refused as regular", fifo, O_RDONLY,
         WF_FILE_EXPECT_REGULAR, WF_FILE_OPEN_OTHER_KIND, 0, 0},
        {"directory refused as regular", scratch_directory, O_RDONLY,
         WF_FILE_EXPECT_REGULAR, WF_FILE_OPEN_IS_DIRECTORY, 0, 0},
        {"absent name", missing, O_RDONLY,
         WF_FILE_EXPECT_REGULAR, WF_FILE_OPEN_FAILED, ENOENT, 0},
        {"regular refused as directory", regular, O_RDONLY,
         WF_FILE_EXPECT_DIRECTORY, WF_FILE_OPEN_OTHER_KIND, 0, 0},
        {"directory acquisition", scratch_directory, O_RDONLY,
         WF_FILE_EXPECT_DIRECTORY, WF_FILE_OPEN_SUCCEEDED, 0, 0},
    };
    for (size_t i = 0; i < sizeof(rows) / sizeof(rows[0]); ++i) {
        const struct open_case *row = &rows[i];
        wf_harness_record record;
        int64_t value = -1;
        int error = -1;
        unsigned outcome = WF_FILE_OPEN_FAILED;
        uint64_t native = wf__completion_native_ring_submissions();
        wf__completion_file_open_at_submit(AT_FDCWD, row->path, row->flags,
            0600u, (row->flags & O_CREAT) != 0, row->kind, record.bytes);
        wf__completion_file_open_join(record.bytes, &value, &error, &outcome);
        if (outcome != row->outcome || error != row->error) {
            fprintf(stderr, "typed open row failed: %s outcome=%u error=%d\n",
                    row->name, outcome, error);
            return 1;
        }
        if (getenv("WF_REQUIRE_LINUX_IO_URING") != NULL)
            CHECK(wf__completion_native_ring_submissions() == native + 1u);
        if (outcome == WF_FILE_OPEN_FAILED) {
            CHECK(value < 0);
        } else if (outcome != WF_FILE_OPEN_SUCCEEDED) {
            CHECK(value >= 0);
            errno = 0;
            CHECK(fcntl((int)value, F_GETFD) == -1 && errno == EBADF);
        } else {
            CHECK(value >= 0 && fcntl((int)value, F_GETFD) != -1);
            int descriptor = (int)value;
            wf__completion_file_close_submit(descriptor, record.bytes);
            wf__completion_file_join(record.bytes, &value, &error);
            CHECK(value == 0 && error == 0);
            if (row->close_twice) {
                /* No intervening descriptor allocation: this checks EBADF,
                 * not the separate concurrent-close/reused-slot property. */
                wf__completion_file_close_submit(descriptor, record.bytes);
                wf__completion_file_join(record.bytes, &value, &error);
                CHECK(value == -1 && error == EBADF);
            }
            if (getenv("WF_REQUIRE_LINUX_IO_URING") != NULL)
                CHECK(wf__completion_native_ring_submissions()
                      == native + 2u + (unsigned)row->close_twice);
        }
    }
    CHECK(unlink(regular) == 0);
    CHECK(unlink(fifo) == 0);
    return 0;
}

/* One file of exactly one byte, so the byte a later read returns names which
 * file was opened. */
/* Closes one descriptor the way every close is performed now: submit, then
 * join.  The direct family is gone with the second lowering that named it
 * (`research/investigations/io-model/PARK-ON-MISS.md` §8), so this is the one
 * close, here as in emitted code.  It reports the way a host call does so the
 * cases below read unchanged. */
static int wf_harness_close(int descriptor) {
    wf_harness_record record;
    int64_t value = -1;
    int error_code = -1;
    wf__completion_file_close_submit(descriptor, record.bytes);
    wf__completion_file_join(record.bytes, &value, &error_code);
    if (value < 0) {
        errno = error_code;
        return -1;
    }
    return 0;
}

static int wf_harness_write_marker_file(int root, const char *name, char byte) {
    int descriptor = openat(root, name, O_CREAT | O_TRUNC | O_WRONLY, 0600);
    if (descriptor < 0) {
        return -1;
    }
    if (write(descriptor, &byte, 1) != 1) {
        (void)close(descriptor);
        return -1;
    }
    return close(descriptor);
}

/* A submitted open resolves the bytes the submitting frame gave it, and two
 * outstanding opens resolve their own.
 *
 * Nothing is copied any more: the path is the frame's own storage and
 * (ordinary native library)'s loan on it holds until the join, so the kernel or the helper
 * resolves the caller's bytes in place (design §5).  The property that
 * survives the copy's removal is the one an emitted program depends on --
 * that two independent opens outstanding at once each resolve their own name
 * -- and it is checked with two marker files whose byte says which was
 * opened.  Both legs run on every platform: the bridge leg reaches the
 * io_uring ring on Linux and the bounded POSIX adapter on Darwin. */
static int test_submitted_open_resolves_the_submitters_bytes(
    const char *scratch_directory
) {
    wf_completion_runtime runtime;
    wf_file_adapter adapter;
    wf_completion_record record;
    wf_harness_record first_block;
    wf_harness_record second_block;
    char first[64];
    char second[64];
    char first_directory[64];
    char marker = 0;
    int64_t value = -1;
    int64_t other_value = -1;
    int error_code = -1;
    int other_error = -1;
    unsigned open_outcome = WF_FILE_OPEN_FAILED;
    unsigned other_outcome = WF_FILE_OPEN_FAILED;
    int root;
    int descriptor;
    int marker_descriptor;
    uint64_t native_before = wf__completion_native_ring_submissions();

    CHECK(scratch_directory != NULL);
    root = open(scratch_directory, O_RDONLY | O_DIRECTORY);
    CHECK(root >= 0);
    CHECK(
        snprintf(first, sizeof(first), "wf-open-name-a-%ld", (long)getpid())
        > 0
    );
    CHECK(
        snprintf(second, sizeof(second), "wf-open-name-b-%ld", (long)getpid())
        > 0
    );
    CHECK(wf_harness_write_marker_file(root, first, 'A') == 0);
    CHECK(wf_harness_write_marker_file(root, second, 'B') == 0);

    /* The bounded POSIX adapter, driven by this thread alone, so the host call
     * demonstrably runs after the submission returned. */
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 0, 0) == 0);
    harness_record_init(&record, WF_FILE_OPEN_AT);
    record.request.operation.open_at.directory = root;
    record.request.operation.open_at.path = first;
    record.request.operation.open_at.flags = O_RDONLY;
    record.request.operation.open_at.expected_kind = WF_FILE_EXPECT_REGULAR;
    CHECK(wf_file_adapter_submit(&adapter, &record) == WF_FILE_TARGET_OWNS);
    /* The request names the caller's own bytes; nothing was copied. */
    CHECK(record.request.operation.open_at.path == first);
    CHECK(wf_file_adapter_progress(&adapter, 1) == 1);
    CHECK(harness_record_done(&record));
    CHECK(record.result.error_code == 0 && record.result.value >= 0);
    CHECK(record.result.open_outcome == WF_FILE_OPEN_SUCCEEDED);
    descriptor = (int)record.result.value;
    CHECK(pread(descriptor, &marker, 1, 0) == 1);
    CHECK(marker == 'A');
    CHECK(close(descriptor) == 0);
    CHECK(wf_file_adapter_shutdown(&adapter) == 0);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);

    /* The shipped bridge, with both opens outstanding at once. */
    wf__completion_file_open_at_submit(
        root,
        first,
        O_RDONLY,
        0u,
        0u,
        WF_FILE_EXPECT_REGULAR,
        first_block.bytes
    );
    wf__completion_file_open_at_submit(
        root,
        second,
        O_RDONLY,
        0u,
        0u,
        WF_FILE_EXPECT_REGULAR,
        second_block.bytes
    );
    wf__completion_file_open_join(
        second_block.bytes,
        &other_value,
        &other_error,
        &other_outcome
    );
    wf__completion_file_open_join(
        first_block.bytes,
        &value,
        &error_code,
        &open_outcome
    );
    CHECK(
        value >= 0 && error_code == 0
        && open_outcome == WF_FILE_OPEN_SUCCEEDED
    );
    CHECK(
        other_value >= 0 && other_error == 0
        && other_outcome == WF_FILE_OPEN_SUCCEEDED
    );
    marker = 0;
    CHECK(pread((int)value, &marker, 1, 0) == 1);
    CHECK(marker == 'A');
    marker = 0;
    CHECK(pread((int)other_value, &marker, 1, 0) == 1);
    CHECK(marker == 'B');
    CHECK(wf_harness_close((int)value) == 0);
    CHECK(wf_harness_close((int)other_value) == 0);

    /* `open_directory` carries its name through exactly the same request, so
     * it is checked the same way: the marker file exists only under the first
     * directory, and finding it proves which name was resolved. */
    CHECK(
        snprintf(
            first_directory,
            sizeof(first_directory),
            "wf-open-dir-a-%ld",
            (long)getpid()
        ) > 0
    );
    (void)unlinkat(root, first_directory, AT_REMOVEDIR);
    CHECK(mkdirat(root, first_directory, 0700) == 0);
    marker_descriptor = openat(root, first_directory, O_RDONLY | O_DIRECTORY);
    CHECK(marker_descriptor >= 0);
    CHECK(wf_harness_write_marker_file(marker_descriptor, "marker", 'A') == 0);
    CHECK(close(marker_descriptor) == 0);

    wf__completion_file_open_at_submit(
        root,
        first_directory,
        O_RDONLY | O_DIRECTORY,
        0u,
        0u,
        WF_FILE_EXPECT_DIRECTORY,
        first_block.bytes
    );
    wf__completion_file_open_join(
        first_block.bytes,
        &value,
        &error_code,
        &open_outcome
    );
    CHECK(
        value >= 0 && error_code == 0
        && open_outcome == WF_FILE_OPEN_SUCCEEDED
    );
    descriptor = (int)value;
    marker_descriptor = openat(descriptor, "marker", O_RDONLY);
    CHECK(marker_descriptor >= 0);
    CHECK(close(marker_descriptor) == 0);
    CHECK(wf_harness_close(descriptor) == 0);

#if defined(__linux__)
    /* Where the ring is the qualified target, the three bridge opens and their three closes are ring
     * operations. Without this the leg would still pass on a silent fallback
     * to the bounded adapter, which is the other half of what this test is
     * about. */
    if (getenv("WF_REQUIRE_LINUX_IO_URING") != NULL) {
        CHECK(wf__completion_native_ring_submissions() == native_before + 6);
    }
#endif
    (void)native_before;

    marker_descriptor = openat(root, first_directory, O_RDONLY | O_DIRECTORY);
    CHECK(marker_descriptor >= 0);
    CHECK(unlinkat(marker_descriptor, "marker", 0) == 0);
    CHECK(close(marker_descriptor) == 0);
    CHECK(unlinkat(root, first, 0) == 0);
    CHECK(unlinkat(root, second, 0) == 0);
    CHECK(unlinkat(root, first_directory, AT_REMOVEDIR) == 0);
    CHECK(close(root) == 0);
    return 0;
}

#define WF_HARNESS_LONG_COMPONENTS 5u
#define WF_HARNESS_LONG_COMPONENT_BYTES 240u
#define WF_HARNESS_LONG_PATH_BYTES                                            \
    (WF_HARNESS_LONG_COMPONENTS * (WF_HARNESS_LONG_COMPONENT_BYTES + 1u) + 16u)
/* A multi-component borrowed path reaches the host unchanged. The same
 * host creates the fixture and supplies the expected success/ENAMETOOLONG
 * result. No removed runtime buffer size is a language path-length limit. */
static int test_long_borrowed_path_reaches_the_host(
    const char *scratch_directory
) {
    char relative[WF_HARNESS_LONG_PATH_BYTES];
    wf_harness_record record;
    size_t length = 0;
    size_t built = 0;
    size_t level;
    uint64_t submissions_before;
    int root;
    int descriptor;
    int host_error = 0;
    int64_t value = -1;
    int error_code = -1;
    unsigned open_outcome = WF_FILE_OPEN_FAILED;
    char marker = 0;

    CHECK(scratch_directory != NULL);
    root = open(scratch_directory, O_RDONLY | O_DIRECTORY);
    CHECK(root >= 0);
    /* Long total length comes from nesting ordinary components. If the host
     * refuses the accumulated path, retain the actual created prefix so that
     * teardown removes exactly this fixture. */
    for (level = 0; level < WF_HARNESS_LONG_COMPONENTS; ++level) {
        if (level != 0) {
            relative[length] = '/';
            length += 1u;
        }
        memset(relative + length, 'd', WF_HARNESS_LONG_COMPONENT_BYTES);
        length += WF_HARNESS_LONG_COMPONENT_BYTES;
        relative[length] = 0;
        if (mkdirat(root, relative, 0700) == 0 || errno == EEXIST) {
            built = length;
            continue;
        }
        CHECK(errno == ENAMETOOLONG);
    }
    memcpy(relative + length, "/marker", sizeof("/marker"));
    length += sizeof("/marker") - 1u;
    CHECK(length == WF_HARNESS_LONG_COMPONENTS
          * (WF_HARNESS_LONG_COMPONENT_BYTES + 1u) + sizeof("marker") - 1u);
    descriptor = openat(root, relative, O_CREAT | O_TRUNC | O_WRONLY, 0600);
    if (descriptor < 0) {
        host_error = errno;
        CHECK(host_error == ENAMETOOLONG);
    } else {
        CHECK(write(descriptor, "L", 1) == 1);
        CHECK(close(descriptor) == 0);
    }

    submissions_before = wf__completion_file_submissions();
    wf__completion_file_open_at_submit(
        root,
        relative,
        O_RDONLY,
        0u,
        0u,
        WF_FILE_EXPECT_REGULAR,
        record.bytes
    );
    wf__completion_file_open_join(
        record.bytes,
        &value,
        &error_code,
        &open_outcome
    );
    /* The borrowed path uses the same submitted operation as a short path. */
    CHECK(wf__completion_file_submissions() == submissions_before + 1u);
    if (host_error == 0) {
        CHECK(
            value >= 0 && error_code == 0
            && open_outcome == WF_FILE_OPEN_SUCCEEDED
        );
        CHECK(pread((int)value, &marker, 1, 0) == 1);
        CHECK(marker == 'L');
        CHECK(wf_harness_close((int)value) == 0);
        CHECK(unlinkat(root, relative, 0) == 0);
    } else {
        CHECK(
            value < 0 && error_code == host_error
            && open_outcome == WF_FILE_OPEN_FAILED
        );
    }

    /* Remove exactly the directories that were created, deepest first. */
    length = built;
    while (length != 0) {
        relative[length] = 0;
        CHECK(unlinkat(root, relative, AT_REMOVEDIR) == 0);
        while (length != 0 && relative[length - 1u] != '/') {
            length -= 1u;
        }
        if (length != 0) {
            length -= 1u;
        }
    }
    CHECK(close(root) == 0);
    return 0;
}

/* Submit past the current 64-entry native SQ before joining any record.
 * Full-queue progress must admit the next request and preserve every offset's
 * result. Operations may finish during submission: this is not a claim that
 * all 96 were simultaneously pending in the kernel. */
#define WF_HARNESS_OUTSTANDING 96u

static int test_submissions_cross_the_native_queue_boundary(
    const char *scratch_directory
) {
    static wf_harness_record records[WF_HARNESS_OUTSTANDING];
    static unsigned char bytes[WF_HARNESS_OUTSTANDING];
    char path[256];
    int descriptor;
    unsigned index;
    uint64_t native_before = wf__completion_native_ring_submissions();
    uint64_t enters_before = wf__completion_native_ring_submission_enters();

    CHECK(scratch_directory != NULL);
    CHECK(
        snprintf(
            path,
            sizeof(path),
            "%s/wf-completion-outstanding-%ld",
            scratch_directory,
            (long)getpid()
        ) > 0
    );
    (void)unlink(path);
    descriptor = open(path, O_CREAT | O_EXCL | O_RDWR, 0600);
    CHECK(descriptor >= 0);
    CHECK(write(descriptor, "abcdefgh", 8) == 8);

    for (index = 0; index < WF_HARNESS_OUTSTANDING; ++index) {
        bytes[index] = 0xa5u;
        wf__completion_file_pread_submit(
            descriptor,
            &bytes[index],
            1,
            (uint64_t)(index % 8u),
            records[index].bytes
        );
    }
    if (wf__completion_native_ring_submissions() != native_before) {
        CHECK(wf__completion_native_ring_submissions()
              == native_before + WF_HARNESS_OUTSTANDING);
        CHECK(wf__completion_native_ring_submission_enters() > enters_before);
    }
    for (index = 0; index < WF_HARNESS_OUTSTANDING; ++index) {
        int64_t value = -1;
        int error_code = -1;
        wf__completion_file_join(records[index].bytes, &value, &error_code);
        CHECK(value == 1 && error_code == 0);
        CHECK(bytes[index] == (unsigned char)('a' + (index % 8u)));
    }

    CHECK(close(descriptor) == 0);
    CHECK(unlink(path) == 0);
    return 0;
}

static void wf_schedule_tick(unsigned *attempts, const char *phase);

typedef struct wf_open_waiter {
    char path[256];
    char marker;
    _Atomic(wf_completion_record *) submitted;
    _Atomic unsigned completed;
    int result;
} wf_open_waiter;

static void *wf_open_waiter_main(void *opaque) {
    wf_open_waiter *waiter = opaque;
    wf_harness_record record;
    int64_t value = -1;
    int error_code = -1;
    unsigned open_outcome = WF_FILE_OPEN_FAILED;
    unsigned attempts = 0;
    char marker = 0;
    waiter->result = 1;
    wf__completion_file_open_at_submit(AT_FDCWD, waiter->path, O_RDONLY,
        0u, 0u, WF_FILE_EXPECT_REGULAR, record.bytes);
    atomic_store_explicit(&waiter->submitted,
        (wf_completion_record *)record.bytes, memory_order_release);
    while (!atomic_load_explicit(&waiter->completed, memory_order_acquire))
        wf_schedule_tick(&attempts, "owner's completed open");
    /* The coordinating thread drove completion without ending this frame.
     * This owner's join must consume its already-published typed result. */
    if (!harness_record_done((wf_completion_record *)record.bytes)) return NULL;
    wf__completion_file_open_join(record.bytes, &value, &error_code, &open_outcome);
    if (value < 0 || error_code != 0 || open_outcome != WF_FILE_OPEN_SUCCEEDED)
        return NULL;
    int matched = pread((int)value, &marker, 1, 0) == 1 && marker == waiter->marker;
    int closed = wf_harness_close((int)value) == 0;
    waiter->result = matched && closed ? 0 : 1;
    return NULL;
}

static int test_open_results_reach_every_independent_owner(
    const char *scratch_directory
) {
    enum { WF_OPEN_WAITERS = 2u };
    pthread_t threads[WF_OPEN_WAITERS];
    wf_open_waiter waiters[WF_OPEN_WAITERS];
    for (unsigned i = 0; i < WF_OPEN_WAITERS; ++i) {
        wf_open_waiter *waiter = &waiters[i];
        waiter->marker = (char)('A' + i);
        CHECK(snprintf(waiter->path, sizeof(waiter->path),
              "%s/wf-completion-owner-%ld-%c", scratch_directory,
              (long)getpid(), waiter->marker) > 0);
        CHECK(wf_harness_write_marker_file(AT_FDCWD, waiter->path, waiter->marker) == 0);
        atomic_init(&waiter->submitted, NULL);
        atomic_init(&waiter->completed, 0u);
        waiter->result = 1;
        CHECK(pthread_create(&threads[i], NULL, wf_open_waiter_main, waiter) == 0);
    }
    /* Observe both submissions before driving either; reverse the owners'
     * join order without pretending this fixes kernel completion order. */
    for (unsigned i = 0; i < WF_OPEN_WAITERS; ++i) {
        unsigned attempts = 0;
        while (atomic_load_explicit(&waiters[i].submitted, memory_order_acquire) == NULL)
            wf_schedule_tick(&attempts, "two independent open submissions");
    }
    for (unsigned i = WF_OPEN_WAITERS; i-- != 0;) {
        int64_t value;
        int error;
        wf_completion_record *record = atomic_load_explicit(
            &waiters[i].submitted, memory_order_acquire);
        wf__completion_file_join(record, &value, &error);
        CHECK(harness_record_done(record));
        CHECK(value >= 0 && error == 0);
        atomic_store_explicit(&waiters[i].completed, 1u, memory_order_release);
    }
    for (unsigned i = 0; i < WF_OPEN_WAITERS; ++i) {
        CHECK(pthread_join(threads[i], NULL) == 0);
        CHECK(waiters[i].result == 0);
        CHECK(unlink(waiters[i].path) == 0);
    }
    return 0;
}

/* The scripted clock the helper policy measures host calls with.
 *
 * Unscripted it is the host's own monotonic clock, so every other case in this
 * file measures what the shipped build measures.  A case that is about what
 * the growth rule *decides* scripts it instead, and then the decision is a
 * property of the rule rather than of how loaded the machine running the test
 * happened to be. */
static _Atomic uint64_t wf_scripted_clock_step_ns;
static _Atomic uint64_t wf_scripted_clock_now_ns;

uint64_t wf_completion_test_monotonic_ns(void) {
    uint64_t step = atomic_load_explicit(
        &wf_scripted_clock_step_ns,
        memory_order_relaxed
    );
    if (step == 0) {
        struct timespec now;
        if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
            return 0;
        }
        return (uint64_t)now.tv_sec * 1000000000u + (uint64_t)now.tv_nsec;
    }
    /* Every reading advances by the scripted step, so the pair of readings
     * around one host call measures exactly that step. */
    return atomic_fetch_add_explicit(
        &wf_scripted_clock_now_ns,
        step,
        memory_order_relaxed
    ) + step;
}

static void wf_script_clock(uint64_t step_ns) {
    atomic_store_explicit(
        &wf_scripted_clock_step_ns,
        step_ns,
        memory_order_relaxed
    );
}

/* Counts every application of the WF_IO_NOCACHE target policy and then makes
 * the same host call the shipped build makes.  The harness build names this
 * function as WF_FILE_UNCACHED_APPLY, so the policy under test is the one in
 * file_adapter.h and only the counting is added. */
static _Atomic unsigned wf_uncached_applications;

void wf_completion_test_uncached_apply(int descriptor) {
    atomic_fetch_add_explicit(
        &wf_uncached_applications,
        1u,
        memory_order_relaxed
    );
    wf_file_uncached_apply_host(descriptor);
}

static unsigned wf_uncached_applications_now(void) {
    return atomic_load_explicit(
        &wf_uncached_applications,
        memory_order_relaxed
    );
}

/* The open flags an open really carried, with the bits that are a property of
 * the host rather than of the request masked away.  Both runs of this test
 * assert the same values, which is what makes the pair a statement that the
 * policy adds nothing to and removes nothing from an open. */
static int wf_uncached_open_flags(int descriptor) {
    int flags = fcntl(descriptor, F_GETFL);
    if (flags < 0) {
        return -1;
    }
    return flags & (O_ACCMODE | O_NONBLOCK | O_APPEND
#if defined(O_DIRECT)
                    | O_DIRECT
#endif
    );
}

/* WF_IO_NOCACHE is target policy, not a language surface: it changes how long
 * a read takes and nothing else.
 *
 * The same test runs twice from the Makefile, once with the setting absent
 * and once with it written.  Absent, the policy makes no host call at all and
 * the count does not move; written, it is applied exactly once for each
 * descriptor an open hands back and never for one a kind check refused.  Both
 * runs assert the identical open flags, the identical typed outcomes, and the
 * identical bytes, so what separates them is a cache hint and nothing else. */
static int test_uncached_reads_are_target_policy_only(
    const char *scratch_directory
) {
    const char *setting = getenv("WF_IO_NOCACHE");
    int asked = setting != NULL && setting[0] == '1' && setting[1] == 0;
    wf_harness_record record;
    int64_t value = -1;
    int error_code = -1;
    unsigned open_outcome = WF_FILE_OPEN_SUCCEEDED;
    unsigned char content[8] = {'n', 'o', 'c', 'a', 'c', 'h', 'e', '!'};
    unsigned char window[8];
    char regular[256];
    char missing[256];
    unsigned before;
    unsigned after;
    int descriptor;

    CHECK(scratch_directory != NULL);
    CHECK(
        snprintf(
            regular,
            sizeof(regular),
            "%s/wf-completion-nocache-%ld",
            scratch_directory,
            (long)getpid()
        ) > 0
    );
    CHECK(
        snprintf(
            missing,
            sizeof(missing),
            "%s/wf-completion-nocache-absent-%ld",
            scratch_directory,
            (long)getpid()
        ) > 0
    );
    (void)unlink(regular);
    (void)unlink(missing);
    descriptor = open(regular, O_CREAT | O_EXCL | O_RDWR, 0600);
    CHECK(descriptor >= 0);
    CHECK(write(descriptor, content, sizeof(content)) == (ssize_t)sizeof(content));
    CHECK(close(descriptor) == 0);

    before = wf_uncached_applications_now();

    /* One kind-checked open through the submitted path.  Its flags are the
     * request's O_RDONLY plus the O_NONBLOCK a kind check needs, and nothing
     * else; the bytes it reads are the bytes that were written. */
    wf__completion_file_open_at_submit(
        AT_FDCWD,
        regular,
        O_RDONLY,
        0u,
        0u,
        WF_FILE_EXPECT_REGULAR,
        record.bytes
    );
    wf__completion_file_open_join(
        record.bytes,
        &value,
        &error_code,
        &open_outcome
    );
    CHECK(value >= 0);
    CHECK(error_code == 0);
    CHECK(open_outcome == WF_FILE_OPEN_SUCCEEDED);
    CHECK(wf_uncached_open_flags((int)value) == (O_RDONLY | O_NONBLOCK));
    CHECK(wf_uncached_applications_now() - before == (asked ? 1u : 0u));
    memset(window, 0, sizeof(window));
    wf__completion_file_pread_submit(
        (int)value,
        window,
        (uint64_t)sizeof(window),
        0u,
        record.bytes
    );
    {
        int64_t moved = -1;
        int read_error = -1;
        wf__completion_file_join(record.bytes, &moved, &read_error);
        CHECK(moved == (int64_t)sizeof(window));
        CHECK(read_error == 0);
        CHECK(memcmp(window, content, sizeof(content)) == 0);
    }
    CHECK(close((int)value) == 0);

    /* An unchecked open carries exactly the request's own flags. */
    wf__completion_file_open_at_submit(
        AT_FDCWD,
        regular,
        O_RDONLY,
        0u,
        0u,
        WF_FILE_EXPECT_ANY,
        record.bytes
    );
    wf__completion_file_open_join(
        record.bytes,
        &value,
        &error_code,
        &open_outcome
    );
    CHECK(value >= 0);
    CHECK(error_code == 0);
    CHECK(open_outcome == WF_FILE_OPEN_SUCCEEDED);
    CHECK(wf_uncached_open_flags((int)value) == O_RDONLY);
    CHECK(wf_uncached_applications_now() - before == (asked ? 2u : 0u));
    CHECK(close((int)value) == 0);

    /* A refused open hands back no descriptor, so the policy has nothing to
     * apply to it. */
    wf__completion_file_open_at_submit(
        AT_FDCWD,
        regular,
        O_RDONLY,
        0u,
        0u,
        WF_FILE_EXPECT_DIRECTORY,
        record.bytes
    );
    wf__completion_file_open_join(
        record.bytes,
        &value,
        &error_code,
        &open_outcome
    );
    CHECK(open_outcome == WF_FILE_OPEN_OTHER_KIND);
    CHECK(wf_uncached_applications_now() - before == (asked ? 2u : 0u));

    /* Neither does an open that never resolved a name. */
    wf__completion_file_open_at_submit(
        AT_FDCWD,
        missing,
        O_RDONLY,
        0u,
        0u,
        WF_FILE_EXPECT_REGULAR,
        record.bytes
    );
    wf__completion_file_open_join(
        record.bytes,
        &value,
        &error_code,
        &open_outcome
    );
    CHECK(value < 0);
    CHECK(open_outcome == WF_FILE_OPEN_FAILED);

    /* Two opens produced a descriptor for the policy to apply to; the third
     * leg was the direct family's copy of the first and left with it
     * (design §8). */
    after = wf_uncached_applications_now();
    CHECK(after - before == (asked ? 2u : 0u));

    CHECK(unlink(regular) == 0);
    return 0;
}

static int test_process_wide_target_helper_budget(void) {
    const char *text = getenv("WF_IO_HELPERS");
    uint64_t held = wf__completion_target_helper_count();
    unsigned long ceiling = 8;
    if (text != NULL && *text != '\0') {
        char *end = NULL;
        unsigned long pinned;
        errno = 0;
        pinned = strtoul(text, &end, 10);
        if (errno == 0 && end != text && *end == '\0') {
            if (pinned > ceiling) {
                pinned = ceiling;
            }
            CHECK(held == pinned);
            return 0;
        }
    }
    CHECK(held <= ceiling);
    return 0;
}


/* Submits `count` positioned reads of one open descriptor, one at a time, and
 * waits for each to complete.
 *
 * The wait is on the record rather than on the queue emptying: a helper takes
 * a request out of the queue before it executes it, so an empty queue is not
 * an answer, and waiting for one is a race this thread loses whenever the pool
 * has grown.  This thread also offers to execute, so the case runs the same
 * either way. */
static int drive_reads_to_completion(
    wf_file_adapter *adapter,
    int descriptor,
    unsigned count
) {
    struct timespec delay = {.tv_sec = 0, .tv_nsec = 1000000};
    unsigned index;
    for (index = 0; index < count; ++index) {
        wf_completion_record record;
        unsigned char byte = 0;
        unsigned attempts;
        harness_record_init(&record, WF_FILE_PREAD);
        record.request.operation.pread.descriptor = descriptor;
        record.request.operation.pread.buffer = &byte;
        record.request.operation.pread.count = 1;
        record.request.operation.pread.offset = 0;
        CHECK(
            wf_file_adapter_submit(adapter, &record) == WF_FILE_TARGET_OWNS
        );
        for (attempts = 0; attempts < 5000; ++attempts) {
            if (harness_record_done(&record)) {
                break;
            }
            if (wf_file_adapter_progress(adapter, 1) == 0) {
                (void)nanosleep(&delay, NULL);
            }
        }
        CHECK(harness_record_done(&record));
        CHECK(record.result.error_code == 0);
        CHECK(record.result.value == 1 && byte == 'w');
    }
    return 0;
}

/* A program whose operations do not wait gets no helper, however much width it
 * states.
 *
 * This is the half of the growth rule batch 0096 added, and it is the half
 * that decides what the warm path costs: queue depth says a program stated
 * independent work, not that the work waits on anything, and a helper handed
 * an operation the host has already answered adds a queue crossing and two
 * thread wakes to nothing.  The clock is scripted so the case is about the
 * rule rather than about this machine's page cache. */
static int test_pool_stays_empty_when_operations_do_not_wait(
    const char *directory
) {
    wf_completion_runtime runtime;
    wf_file_adapter adapter;
    char path[512];
    int descriptor;

    CHECK(directory != NULL);
    CHECK(
        snprintf(
            path,
            sizeof(path),
            "%s/wf-completion-pool-no-wait-%ld",
            directory,
            (long)getpid()
        ) > 0
    );
    descriptor = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    CHECK(descriptor >= 0);
    CHECK(write(descriptor, "wf", 2) == 2);

    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 4, 0) == 0);
    CHECK(wf_file_adapter_set_helper_cap(&adapter, 4) == 0);

    /* Nothing measured yet is not evidence of anything, and neither policy
     * fires on it: the first operations take the completion path unchanged. */
    CHECK(wf_file_adapter_wait_verdict(&adapter) == WF_FILE_WAIT_UNMEASURED);
    CHECK(wf_file_adapter_transfer_runs_on_caller(&adapter) == 0);

    /* One microsecond a call: real work, and an order of magnitude under the
     * wait that would make a second thread worth its handoff. */
    wf_script_clock(1000u);
    /* Cross the one-in-sixteen sampling boundary twice. */
    CHECK(drive_reads_to_completion(&adapter, descriptor, 32) == 0);
    wf_script_clock(0);
    CHECK(wf_file_adapter_helper_count(&adapter) == 0);
    CHECK(wf_file_adapter_wait_verdict(&adapter) == WF_FILE_WAIT_SHORT);
    /* With no helper, nothing queued and no wait measured, a transfer
     * submitted now would be executed by the submitting thread itself. */
    CHECK(wf_file_adapter_transfer_runs_on_caller(&adapter) == 1);

    CHECK(wf_file_adapter_shutdown(&adapter) == 0);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    CHECK(close(descriptor) == 0);
    CHECK(unlink(path) == 0);
    return 0;
}

/* How many requests the two growth cases queue behind a pool that cannot
 * empty it.
 *
 * Growth adds at most one helper per submission and only while the queue is
 * deeper than the pool, so a pool of `n` needs a submission numbered past
 * `2n` to reach `n + 1` in the worst interleaving, where every helper takes
 * one entry the instant it is created.  Twenty carries a pool of eight, which
 * is the largest ceiling either case below asks for. */
#define WF_POOL_GROWTH_DEPTH 20u

/* Drives the helper pool against a queue it cannot empty and answers how
 * large the pool became.
 *
 * `drive_reads_to_completion` waits for each request before submitting the
 * next, so its queue never holds more than one entry.  Growth is gated on
 * `queue_count > held`, which that shape satisfies only while the pool is
 * empty: the first helper it creates is also its last, however high the cap
 * is.  A case that asserts an upper bound above one therefore asserts nothing
 * under that driver -- the bound is not approached, let alone reached, and
 * deleting the code that enforces it changes no verdict.
 *
 * These requests are reads of an empty pipe instead, so a helper that takes
 * one blocks in the host call and never returns for another.  At most one
 * entry per helper ever leaves the queue, so after `i` submissions the queue
 * holds at least `i - held` entries and every submission past twice the
 * current pool size finds `queue_count > held` and adds a helper.  Growth
 * then stops only where the cap stops it, which is the question both cases
 * below ask.
 *
 * The first read is served by a byte written before it, because growth also
 * requires a measured wait and an adapter that has executed nothing has
 * measured none.  Its execution is the sampled one -- the interval starts at
 * the first -- and the caller performs it, because the pool is still empty.
 * The pipe is fed the rest of its bytes only after the pool count has been
 * read, so that count is the pool's final one. */
static int grow_pool_against_a_blocked_queue(
    wf_file_adapter *adapter,
    int read_descriptor,
    int write_descriptor,
    size_t *held
) {
    struct timespec delay = {.tv_sec = 0, .tv_nsec = 1000000};
    static wf_completion_record records[WF_POOL_GROWTH_DEPTH];
    unsigned char bytes[WF_POOL_GROWTH_DEPTH];
    char feed[WF_POOL_GROWTH_DEPTH];
    wf_completion_record primer;
    unsigned char primer_byte = 0;
    unsigned index;

    CHECK(write(write_descriptor, "w", 1) == 1);
    harness_record_init(&primer, WF_FILE_READ);
    primer.request.operation.read.descriptor = read_descriptor;
    primer.request.operation.read.buffer = &primer_byte;
    primer.request.operation.read.count = 1;
    /* Nothing is measured yet, so this submission cannot grow the pool
     * whatever its cap says, and the caller is the queue's only engine. */
    CHECK(wf_file_adapter_submit(adapter, &primer) == WF_FILE_TARGET_OWNS);
    CHECK(wf_file_adapter_helper_count(adapter) == 0);
    CHECK(wf_file_adapter_progress(adapter, 1) == 1);
    CHECK(harness_record_done(&primer));
    CHECK(primer.result.error_code == 0);
    CHECK(primer.result.value == 1);
    CHECK(primer_byte == 'w');
    /* One scripted millisecond a call is a wait by any reading of the
     * threshold, so the growth rule's measured half now holds. */
    CHECK(wf_file_adapter_wait_verdict(adapter) == WF_FILE_WAIT_LONG);

    /* The deep queue.  Nothing feeds the pipe from here, so every helper the
     * pool gains blocks on its first read and the queue only deepens. */
    for (index = 0; index < WF_POOL_GROWTH_DEPTH; ++index) {
        bytes[index] = 0;
        feed[index] = 'q';
        harness_record_init(&records[index], WF_FILE_READ);
        records[index].request.operation.read.descriptor = read_descriptor;
        records[index].request.operation.read.buffer = &bytes[index];
        records[index].request.operation.read.count = 1;
        CHECK(
            wf_file_adapter_submit(adapter, &records[index])
            == WF_FILE_TARGET_OWNS
        );
    }
    /* Admission has stopped and growth happens only on admission, so this is
     * the size the policy settled on. */
    *held = wf_file_adapter_helper_count(adapter);

    /* One byte for each submitted read: from here no read of this pipe can
     * block, so the pool -- whatever size it reached -- drains the queue, and
     * the caller helps in case the pool is empty. */
    CHECK(
        write(write_descriptor, feed, sizeof(feed))
        == (ssize_t)sizeof(feed)
    );
    for (index = 0; index < WF_POOL_GROWTH_DEPTH; ++index) {
        unsigned attempts;
        for (attempts = 0; attempts < 100000u; ++attempts) {
            if (harness_record_done(&records[index])) {
                break;
            }
            if (wf_file_adapter_progress(adapter, 1) == 0) {
                (void)nanosleep(&delay, NULL);
            }
        }
        CHECK(harness_record_done(&records[index]));
        CHECK(records[index].result.error_code == 0);
        CHECK(records[index].result.value == 1);
        CHECK(bytes[index] == 'q');
    }
    return 0;
}

/* A program whose operations do wait gets helpers, up to the cap and no
 * further.
 *
 * Both ends of that promise need a queue the pool cannot empty.  With one
 * request in flight at a time the pool stops itself at one helper, so a cap of
 * four is neither reached nor tested; against the blocked queue the pool
 * climbs until the cap is the only thing left stopping it, and the count it
 * settles on is the cap exactly. */
static int test_pool_grows_when_operations_wait(void) {
    wf_completion_runtime runtime;
    wf_file_adapter adapter;
    int descriptors[2];
    size_t held = 0;

    CHECK(pipe(descriptors) == 0);
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 4, 0) == 0);
    CHECK(wf_file_adapter_set_helper_cap(&adapter, 4) == 0);

    /* Nothing measured yet is not evidence of anything, and neither policy
     * fires on it. */
    CHECK(wf_file_adapter_wait_verdict(&adapter) == WF_FILE_WAIT_UNMEASURED);

    wf_script_clock(1000000u);
    CHECK(
        grow_pool_against_a_blocked_queue(
            &adapter,
            descriptors[0],
            descriptors[1],
            &held
        ) == 0
    );
    wf_script_clock(0);
    CHECK(held == 4);
    CHECK(wf_file_adapter_wait_verdict(&adapter) == WF_FILE_WAIT_LONG);
    /* A transfer submitted now would be overlapped by a helper, so the
     * submitting thread must not take it itself. */
    CHECK(wf_file_adapter_transfer_runs_on_caller(&adapter) == 0);

    CHECK(wf_file_adapter_shutdown(&adapter) == 0);
    /* Every entry that would take `queue_lock` or read the record's measured
     * state is turned away by the flag instead. */
    CHECK(wf_file_adapter_queued(&adapter) == 0);
    CHECK(wf_file_adapter_helper_count(&adapter) == 0);
    CHECK(wf_file_adapter_transfer_runs_on_caller(&adapter) == 0);
    CHECK(wf_file_adapter_wait_verdict(&adapter) == WF_FILE_WAIT_UNMEASURED);
    CHECK(wf_file_adapter_set_helper_cap(&adapter, 2) == EINVAL);
    /* And shutdown itself is one of those entries. */
    CHECK(wf_file_adapter_shutdown(&adapter) == EINVAL);

    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    CHECK(close(descriptors[0]) == 0);
    CHECK(close(descriptors[1]) == 0);
    return 0;
}

/* The ceiling a policy asks for is a wish; the bound the caller gave
 * `wf_file_adapter_init` is the fact, and `wf_file_adapter_set_helper_cap`
 * clamps the cap to it rather than trusting it.  A cap of eight against a bound
 * of two leaves the clamp as the only thing keeping the pool at the number of
 * helpers this adapter was told it may ever hold.
 *
 * The bound used to be the length of an array of thread handles the caller
 * owned, and this case was named for that array.  The helpers are the
 * runtime's own detached threads now (`wf_prim_thread_start`), so the caller
 * hands over no storage and the bound is a plain number -- which is the same
 * property to hold and the same clamp that holds it, with the write past the
 * end of a frame no longer among the ways to break it. */
static int test_helper_growth_stops_at_the_declared_bound(void) {
    wf_completion_runtime runtime;
    wf_file_adapter adapter;
    int descriptors[2];
    size_t held = 0;

    CHECK(pipe(descriptors) == 0);
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 2, 0) == 0);
    /* Accepted, and silently reduced to the two helpers init was given. */
    CHECK(wf_file_adapter_set_helper_cap(&adapter, 8) == 0);

    wf_script_clock(1000000u);
    CHECK(
        grow_pool_against_a_blocked_queue(
            &adapter,
            descriptors[0],
            descriptors[1],
            &held
        ) == 0
    );
    wf_script_clock(0);
    /* Two, not the eight that were asked for: the queue stayed deeper than
     * the pool for every one of the twenty submissions, so the cap is the
     * only thing that stopped it, and the cap is the declared bound. */
    CHECK(held == 2);

    CHECK(wf_file_adapter_shutdown(&adapter) == 0);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    CHECK(close(descriptors[0]) == 0);
    CHECK(close(descriptors[1]) == 0);
    return 0;
}

/* An initial count above the declared bound is refused outright: unlike the
 * cap, it is not a ceiling to grow towards but threads to start right now. */
static int test_helper_count_above_its_bound_is_refused(void) {
    wf_completion_runtime runtime;
    wf_file_adapter adapter;

    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 1, 2) == EINVAL);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    return 0;
}

/* A failed schedule premise is a bounded test failure, not a different
 * interleaving accepted after a fixed sleep. The outer alarm names the case. */
static void wf_schedule_tick(unsigned *attempts, const char *phase) {
    const struct timespec delay = {.tv_sec = 0, .tv_nsec = 1000000};
    if (++*attempts > 5000u) {
        fprintf(stderr, "runtime schedule not observed: %s\n", phase);
        _Exit(9);
    }
    (void)nanosleep(&delay, NULL);
}

static void wf_wait_for_poll(void) {
    unsigned attempts = 0;
    while (atomic_load_explicit(&wf_observed_poll_calls, memory_order_acquire) == 0)
        wf_schedule_tick(&attempts, "empty-pipe readiness wait");
}

static void wf_wait_for_join(uint64_t announcements) {
    unsigned attempts = 0;
    while (wf__completion_wait_announcements() == announcements)
        wf_schedule_tick(&attempts, "join wait announcement");
}

typedef struct wf_pipe_writer {
    int descriptor;
    unsigned char byte;
    int expect_poll;
    int wait_for_join;
    uint64_t announcements;
    ssize_t written;
} wf_pipe_writer;

static void *write_after_observed_wait(void *opaque) {
    wf_pipe_writer *context = opaque;
    if (context->expect_poll) wf_wait_for_poll();
    if (context->wait_for_join) wf_wait_for_join(context->announcements);
    context->written = write(context->descriptor, &context->byte, 1);
    return NULL;
}

static int observe_empty_pipe(int descriptor) {
    int flags = fcntl(descriptor, F_GETFL, 0);
    CHECK(flags >= 0);
    CHECK(fcntl(descriptor, F_SETFL, flags | O_NONBLOCK) == 0);
    atomic_store(&wf_observed_poll_calls, 0u);
    atomic_store(&wf_observed_poll_fd, descriptor);
    return 0;
}

static int finish_observed_pipe(const wf_pipe_writer *writer) {
    CHECK(writer->written == 1);
    CHECK(atomic_load(&wf_observed_poll_calls) == (unsigned)writer->expect_poll);
    if (writer->expect_poll) {
        CHECK(wf_observed_poll_events == POLLIN);
        CHECK(wf_observed_poll_timeout == -1);
    }
    atomic_store(&wf_observed_poll_fd, -1);
    return 0;
}

/* A completion elsewhere ends a join that is waiting in place.
 *
 * The join announces a wait on the record's current-state predicate. The
 * thread that finishes the operation publishes its result through
 * `wf_completion_record_complete` and wakes an announced native waiter.
 *
 * The operation is a read of an empty pipe, so it genuinely cannot finish
 * until another thread writes: a join that did not wait, or a completion that
 * did not wake, is a hang the harness watchdog reports by name rather than a
 * wrong answer.  It runs on all four helper settings the gate uses, which is
 * what covers both engines. Linux may submit this pipe read to io_uring;
 * only the adapter route has a host poll observation. */
static int test_bridge_read_completes_after_observed_wait(void) {
    wf_harness_record record;
    wf_pipe_writer writer_context;
    pthread_t writer;
    int descriptors[2];
    unsigned char byte = 0;
    int64_t value = -1;
    int error_code = -1;
    uint64_t publications_before;
    uint64_t fallback_before = wf__completion_file_fallback_submissions();
    uint64_t native_before = wf__completion_native_ring_submissions();

    CHECK(pipe(descriptors) == 0);
    CHECK(observe_empty_pipe(descriptors[0]) == 0);
    publications_before = wf__completion_publications();
    wf__completion_file_read_submit(
        descriptors[0],
        &byte,
        1,
        record.bytes
    );
    writer_context.expect_poll = wf__completion_native_ring_submissions() == native_before;
    writer_context.wait_for_join = !writer_context.expect_poll ||
        wf__completion_target_helper_count() != 0;
    writer_context.announcements = wf__completion_wait_announcements();
    writer_context.descriptor = descriptors[1];
    writer_context.byte = 'w';
    CHECK(
        pthread_create(&writer, NULL, write_after_observed_wait, &writer_context)
        == 0
    );
    /* A helper must have taken the request before the joining caller can
     * claim it. With zero helpers the caller itself reaches poll. */
    if (writer_context.expect_poll && writer_context.wait_for_join) wf_wait_for_poll();
    wf__completion_file_join(record.bytes, &value, &error_code);
    CHECK(pthread_join(writer, NULL) == 0);
    CHECK(finish_observed_pipe(&writer_context) == 0);
    CHECK(value == 1 && error_code == 0);
    CHECK(byte == 'w');
    CHECK(wf__completion_publications() == publications_before + 1u);
    /* A non-positioned read is never executed inside submit, so this really
     * did go to the queue and come back from an engine. */
    CHECK(wf__completion_file_fallback_submissions() ==
          fallback_before + (unsigned)writer_context.expect_poll);
    CHECK(wf__completion_native_ring_submissions() ==
          native_before + (unsigned)!writer_context.expect_poll);
    CHECK(close(descriptors[0]) == 0);
    CHECK(close(descriptors[1]) == 0);
    return 0;
}

/* A publisher waits for the ordinary joining thread to announce its wait. */
typedef struct current_stack_join_context {
    wf_harness_record record;
    int64_t value;
    int error_code;
    uint64_t announcements;
} current_stack_join_context;

static void *complete_after_observed_join(void *opaque) {
    current_stack_join_context *context = opaque;
    wf_completion_record *record = (wf_completion_record *)context->record.bytes;
    wf_wait_for_join(context->announcements);
    record->result.kind = WF_FILE_READ;
    record->result.value = 11;
    record->result.error_code = 0;
    wf_completion_record_complete(record);
    return NULL;
}

/* Generic suspended-stack resumption is retired. The same delayed
 * publication must wake a native thread blocked in the delivered I/O join. */
static int test_an_io_join_waits_on_the_current_stack(void) {
    current_stack_join_context context;
    pthread_t publisher;
    memset(&context, 0, sizeof(context));
    /* A delivered record comes from submit, which initializes the bridge.
     * Establish that premise before directly constructing a pending record. */
    wf__completion_file_read_submit(-1, NULL, 0, context.record.bytes);
    wf__completion_file_join(context.record.bytes, &context.value, &context.error_code);
    CHECK(context.value == 0 && context.error_code == 0);
    wf_completion_record *record = (wf_completion_record *)context.record.bytes;
    harness_record_init(record, WF_FILE_READ);
    uint64_t before = wf__completion_publications();
    uint64_t parks = wf__completion_wait_announcements();
    context.announcements = parks;
    CHECK(pthread_create(&publisher, NULL, complete_after_observed_join, &context) == 0);
    wf__completion_file_join(context.record.bytes, &context.value, &context.error_code);
    CHECK(pthread_join(publisher, NULL) == 0);
    CHECK(context.value == 11 && context.error_code == 0);
    CHECK(wf__completion_publications() == before + 1);
    CHECK(wf__completion_wait_announcements() > parks);
    return 0;
}

/* Interruption and readiness refusal are adapter progress, never an outcome.
 *
 * The read below is of a nonblocking empty pipe, so its first host attempt is
 * refused with EAGAIN; the adapter waits for exactly one POLLIN and repeats
 * it.  One submission still produces exactly one completion, which the
 * publication count says. */
static int test_readiness_refusal_is_not_a_terminal_outcome(void) {
    wf_completion_runtime runtime;
    wf_file_adapter adapter;
    wf_completion_record record;
    wf_pipe_writer writer_context;
    pthread_t writer;
    uint64_t publications_before;
    int descriptors[2];
    unsigned char byte = 0;

    CHECK(pipe(descriptors) == 0);
    CHECK(observe_empty_pipe(descriptors[0]) == 0);
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 0, 0) == 0);
    harness_record_init(&record, WF_FILE_READ);
    record.request.operation.read.descriptor = descriptors[0];
    record.request.operation.read.buffer = &byte;
    record.request.operation.read.count = 1;
    publications_before = wf__completion_publications();
    CHECK(wf_file_adapter_submit(&adapter, &record) == WF_FILE_TARGET_OWNS);
    writer_context.descriptor = descriptors[1];
    writer_context.byte = 'r';
    writer_context.expect_poll = 1;
    writer_context.wait_for_join = 0;
    writer_context.announcements = 0;
    CHECK(
        pthread_create(&writer, NULL, write_after_observed_wait, &writer_context)
        == 0
    );
    CHECK(wf_file_adapter_progress(&adapter, 1) == 1);
    CHECK(pthread_join(writer, NULL) == 0);
    CHECK(finish_observed_pipe(&writer_context) == 0);
    CHECK(byte == 'r');
    CHECK(harness_record_done(&record));
    CHECK(record.result.value == 1);
    CHECK(record.result.error_code == 0);
    CHECK(wf__completion_publications() == publications_before + 1u);
    CHECK(wf_file_adapter_shutdown(&adapter) == 0);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    CHECK(close(descriptors[0]) == 0);
    CHECK(close(descriptors[1]) == 0);
    return 0;
}
/* The io_uring doorbell is deferred, and every path that can stop reaches the
 * kernel before it does.
 *
 * Staging a submission-queue entry is a store to a page the kernel already
 * maps, so it costs no system call; `io_uring_enter` is what tells the kernel
 * the entry exists. Ringing it once per submission is 15,360 calls on the
 * eight-wide benchmark and 2,048 when it is deferred (LOOP-PIPELINE.md §9.1).
 * Deferring is only safe while the two facts below hold, and they are what
 * this checks: a submission alone rings nothing, and the first thing that
 * could wait — a join — rings it first.
 *
 * The enter counter is meaningful only on the native route; other routes
 * return after the typed open rather than repeating the read success matrix. */
static int test_a_submitted_operation_is_kicked_before_it_waits(
    const char *scratch_directory
) {
    char path[256];
    unsigned char first[8];
    unsigned char second[8];
    wf_harness_record open_record;
    wf_harness_record read_record;
    wf_harness_record other_record;
    int64_t value = -1;
    int error_code = -1;
    unsigned open_outcome = WF_FILE_OPEN_FAILED;
    int descriptor;
    int writer;
    uint64_t submissions_before;
    uint64_t enters;
    int ring_route;

    CHECK(scratch_directory != NULL);
    CHECK(
        snprintf(
            path,
            sizeof(path),
            "%s/wf-completion-doorbell-%ld",
            scratch_directory,
            (long)getpid()
        ) > 0
    );
    (void)unlink(path);
    writer = open(path, O_CREAT | O_EXCL | O_WRONLY, 0600);
    CHECK(writer >= 0);
    CHECK(write(writer, "doorbell", 8) == 8);
    CHECK(close(writer) == 0);

    submissions_before = wf__completion_native_ring_submissions();
    wf__completion_file_open_at_submit(
        AT_FDCWD,
        path,
        O_RDONLY,
        0u,
        0u,
        WF_FILE_EXPECT_REGULAR,
        open_record.bytes
    );
    wf__completion_file_open_join(
        open_record.bytes,
        &value,
        &error_code,
        &open_outcome
    );
    CHECK(value >= 0 && error_code == 0);
    CHECK(open_outcome == WF_FILE_OPEN_SUCCEEDED);
    descriptor = (int)value;
    ring_route =
        wf__completion_native_ring_submissions() > submissions_before;
    if (!ring_route) {
        CHECK(getenv("WF_REQUIRE_LINUX_IO_URING") == NULL);
        CHECK(wf_harness_close(descriptor) == 0);
        CHECK(unlink(path) == 0);
        return 0;
    }

    /* A submission alone rings nothing, and the join that follows it rings it
     * first.  The blocking direct host call this leg used to make between the
     * two went with the direct family (design section 8); a caller's thread
     * reaches a host call now only by running its own queued record, and that
     * path rings the doorbell for the same reason before the same kind of
     * call. */
    enters = wf__completion_native_ring_submission_enters();
    memset(first, 0, sizeof(first));
    wf__completion_file_pread_submit(
        descriptor,
        first,
        sizeof(first),
        0,
        read_record.bytes
    );
    if (ring_route) {
        CHECK(wf__completion_native_ring_submission_enters() == enters);
    }
    wf__completion_file_join(read_record.bytes, &value, &error_code);
    if (ring_route) {
        CHECK(wf__completion_native_ring_submission_enters() == enters + 1u);
    }
    CHECK(value == 8 && error_code == 0);
    CHECK(memcmp(first, "doorbell", 8) == 0);

    /* Two submissions, one doorbell: the first join carries both, and the
     * second join needs no further call. This is the whole of what deferring
     * buys, stated as a count. */
    enters = wf__completion_native_ring_submission_enters();
    memset(first, 0, sizeof(first));
    memset(second, 0, sizeof(second));
    wf__completion_file_pread_submit(
        descriptor,
        first,
        4,
        0,
        read_record.bytes
    );
    wf__completion_file_pread_submit(
        descriptor,
        second,
        4,
        4,
        other_record.bytes
    );
    if (ring_route) {
        CHECK(wf__completion_native_ring_submission_enters() == enters);
    }
    wf__completion_file_join(read_record.bytes, &value, &error_code);
    CHECK(value == 4 && error_code == 0);
    if (ring_route) {
        CHECK(wf__completion_native_ring_submission_enters() == enters + 1u);
    }
    wf__completion_file_join(other_record.bytes, &value, &error_code);
    CHECK(value == 4 && error_code == 0);
    if (ring_route) {
        CHECK(wf__completion_native_ring_submission_enters() == enters + 1u);
    }
    CHECK(memcmp(first, "door", 4) == 0);
    CHECK(memcmp(second, "bell", 4) == 0);

    wf__completion_file_close_submit(descriptor, read_record.bytes);
    wf__completion_file_join(read_record.bytes, &value, &error_code);
    CHECK(value == 0 && error_code == 0);
    CHECK(unlink(path) == 0);
    return 0;
}

/* One loopback connection through the submitted TCP kinds, and the accounting
 * a connection's two directions carry (ordinary native library).
 *
 * This is the whole socket lifecycle at the bridge's own ABI: listen, connect,
 * accept, send, receive, and the two half-closes. The accounting under test is
 * the pair's: a connection is one descriptor and two owners, so it takes
 * exactly two releases, the first is that direction's half-close and leaves
 * the target's object open, and the second releases it. That is the runtime's
 * half of the (ordinary native library) credit rule -- one connection costs the target one
 * native handle, whichever way the pair is released -- and it is checked here
 * by asking the host whether the descriptor is still this program's.
 *
 * The listener's own port is the kernel's choice, and no operation reports it,
 * so this frame asks the host directly. That is the same thing the program
 * corpus does from the other side, where the harness picks the port. */
static int test_socket_lifecycle_and_the_pair_two_count(void) {
    unsigned port;
    return wf_test_socket_lifecycle(&port);
}

/* Opens one listening socket on the loopback and reports the port the host
 * chose, for the peer-bound case below.
 *
 * It is the host's own three calls rather than the bridge's socket kinds
 * because the case under it is about the adapter's queue and not about the
 * submitted listen: a listener built here is one this frame can hand to an
 * adapter it made itself, with the helper policy the case names rather than
 * the process-wide one. */
static int harness_open_listener(unsigned *port) {
    struct sockaddr_in address;
    socklen_t length = (socklen_t)sizeof(address);
    int listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return -1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    address.sin_port = 0;
    if (bind(listener, (struct sockaddr *)&address, sizeof(address)) != 0
        || listen(listener, 8) != 0
        || getsockname(listener, (struct sockaddr *)&address, &length) != 0) {
        (void)close(listener);
        return -1;
    }
    *port = (unsigned)ntohs(address.sin_port);
    return listener;
}

/* Connects one peer to the loopback port, and leaves it silent. */
static int harness_connect_peer(unsigned port) {
    struct sockaddr_in address;
    int peer = socket(AF_INET, SOCK_STREAM, 0);
    if (peer < 0) {
        return -1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    address.sin_port = htons((unsigned short)port);
    if (connect(peer, (struct sockaddr *)&address, sizeof(address)) != 0) {
        (void)close(peer);
        return -1;
    }
    return peer;
}

/* A request whose kind waits on a peer is a helper's work, not a scheduler
 * thread's.
 *
 * The wait an accept or a receive makes is ended by another program, so no
 * measurement of this adapter's own host calls can see it coming and nothing
 * this runtime does can shorten it. A scheduler thread that enters one is a
 * thread another program has taken, along with every continuation it was
 * holding, which is exactly how `tests/programs/tcp_fanout.wf` lost its fourth
 * peer: three workers inside receives from silent peers and no thread left to
 * accept the connection whose peer was about to speak.
 *
 * So the kind is the whole rule, and this case is its three halves. The
 * submission of a peer-bound request starts a helper whatever the measured
 * verdict says -- here it says nothing at all, because this adapter has
 * executed nothing. The scheduler thread's progress pass then leaves the
 * queued accept alone and reports that it moved nothing, rather than blocking
 * in it. And a pool pinned at zero helpers, which is a policy that makes the
 * calling thread the queue's only engine, still runs one on the calling
 * thread, because there refusing it would leave it with no engine at all.
 *
 * The queue is brought to a state the rule can be read off by filling the pool
 * first: two helpers, both inside an accept of a listener no peer has reached,
 * and a third accept behind them that no helper can take. What is queued at
 * the progress call is therefore exactly one peer-bound request, and a
 * regression that takes it does not fail this check -- it blocks in the host
 * call and the watchdog reports the case by name. */
static int test_a_peer_bound_request_is_left_to_a_helper(void) {
    wf_completion_runtime runtime;
    wf_file_adapter adapter;
    wf_completion_record accepts[3];
    wf_completion_record pinned;
    struct timespec delay = {.tv_sec = 0, .tv_nsec = 1000000};
    unsigned port = 0;
    unsigned index;
    unsigned attempts;
    int listener;
    int peers[3];

    listener = harness_open_listener(&port);
    CHECK(listener >= 0);
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 2, 0) == 0);
    CHECK(wf_file_adapter_set_helper_cap(&adapter, 2) == 0);
    CHECK(wf_file_adapter_helper_count(&adapter) == 0);

    /* (a) The first peer-bound submission starts the first helper, under the
     * queue lock, before it returns.  Nothing has been executed, so the
     * measured verdict is the one that stops every other growth. */
    CHECK(wf_file_adapter_wait_verdict(&adapter) == WF_FILE_WAIT_UNMEASURED);
    for (index = 0; index < 3; ++index) {
        harness_record_init(&accepts[index], WF_FILE_SOCKET_ACCEPT);
        accepts[index].request.operation.accept.descriptor = listener;
        CHECK(
            wf_file_adapter_submit(&adapter, &accepts[index])
            == WF_FILE_TARGET_OWNS
        );
        /* One helper per submission, and never past the cap. */
        CHECK(wf_file_adapter_helper_count(&adapter) == (index < 2 ? index + 1 : 2));
    }
    CHECK(wf_file_adapter_wait_verdict(&adapter) == WF_FILE_WAIT_UNMEASURED);

    /* Both helpers are inside an accept of a listener no peer has reached, so
     * the third submission is the one thing left on the queue. */
    for (attempts = 0; attempts < 5000u; ++attempts) {
        if (wf_file_adapter_queued(&adapter) == 1) {
            break;
        }
        (void)nanosleep(&delay, NULL);
    }
    CHECK(wf_file_adapter_queued(&adapter) == 1);

    /* (b) The progress pass leaves it there and says so. */
    CHECK(wf_file_adapter_progress(&adapter, 1) == 0);
    CHECK(wf_file_adapter_queued(&adapter) == 1);
    for (index = 0; index < 3; ++index) {
        CHECK(!harness_record_done(&accepts[index]));
    }

    /* (c) Every one of them completes once the peers act. */
    for (index = 0; index < 3; ++index) {
        peers[index] = harness_connect_peer(port);
        CHECK(peers[index] >= 0);
    }
    for (index = 0; index < 3; ++index) {
        for (attempts = 0; attempts < 20000u; ++attempts) {
            if (harness_record_done(&accepts[index])) {
                break;
            }
            (void)nanosleep(&delay, NULL);
        }
        CHECK(harness_record_done(&accepts[index]));
        CHECK(accepts[index].result.error_code == 0);
        CHECK(accepts[index].result.value >= 0);
        CHECK(close((int)accepts[index].result.value) == 0);
        CHECK(close(peers[index]) == 0);
    }
    CHECK(wf_file_adapter_shutdown(&adapter) == 0);
    CHECK(close(listener) == 0);

    /* (d) A pool pinned at zero helpers keeps the calling thread as the
     * queue's engine for a peer-bound request too.  The peer is connected
     * first, so what this asserts is which thread ran the accept and not how
     * long one waits. */
    listener = harness_open_listener(&port);
    CHECK(listener >= 0);
    peers[0] = harness_connect_peer(port);
    CHECK(peers[0] >= 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 2, 0) == 0);
    CHECK(wf_file_adapter_set_helper_cap(&adapter, 0) == 0);
    harness_record_init(&pinned, WF_FILE_SOCKET_ACCEPT);
    pinned.request.operation.accept.descriptor = listener;
    CHECK(wf_file_adapter_submit(&adapter, &pinned) == WF_FILE_TARGET_OWNS);
    CHECK(wf_file_adapter_helper_count(&adapter) == 0);
    CHECK(wf_file_adapter_progress(&adapter, 1) == 1);
    CHECK(harness_record_done(&pinned));
    CHECK(pinned.result.error_code == 0);
    CHECK(pinned.result.value >= 0);
    CHECK(close((int)pinned.result.value) == 0);
    CHECK(close(peers[0]) == 0);
    CHECK(wf_file_adapter_shutdown(&adapter) == 0);
    CHECK(close(listener) == 0);

    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    return 0;
}

/* One more pipe than the pool's ceiling, each with a reader and a writer. */
#define WF_PEER_PIPES (WF_FILE_MAX_HELPERS + 1u)

/* Waits, bounded, until every record of `records` is done. */
static int harness_await_all(wf_completion_record *records, unsigned count) {
    struct timespec delay = {.tv_sec = 0, .tv_nsec = 1000000};
    unsigned attempts;
    unsigned index;
    for (attempts = 0; attempts < 10000u; ++attempts) {
        for (index = 0; index < count; ++index) {
            if (!harness_record_done(&records[index])) {
                break;
            }
        }
        if (index == count) {
            return 1;
        }
        (void)nanosleep(&delay, NULL);
    }
    return 0;
}

/* Once contexts run, a request that may wait on another context is never
 * left behind a pool whose every helper is inside such a wait [WAIT-2].
 *
 * Nine readers of nine empty pipes are submitted first, so on the eight
 * helpers of the ceiling one reader and every writer would wait in the
 * queue behind eight reads whose writers never run.  With the pool allowed
 * to grow for peers, every read ends with its writer's byte; a pinned pool
 * keeps its eight. */
static int test_a_peer_wait_never_waits_behind_peer_waits(void) {
    wf_completion_runtime runtime;
    wf_file_adapter adapter;
    static wf_completion_record records[2u * WF_PEER_PIPES];
    unsigned char landed[WF_PEER_PIPES];
    unsigned char sent[WF_PEER_PIPES];
    int pipes[WF_PEER_PIPES][2];
    unsigned index;

    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, WF_FILE_MAX_HELPERS, 0) == 0);
    CHECK(wf_file_adapter_set_helper_cap(&adapter, WF_FILE_MAX_HELPERS) == 0);
    CHECK(wf_file_adapter_hold_for_contexts(&adapter, 1) == 0);
    for (index = 0; index < WF_PEER_PIPES; ++index) {
        CHECK(pipe(pipes[index]) == 0);
        landed[index] = 0;
        sent[index] = (unsigned char)(index + 1u);
        harness_record_init(&records[index], WF_FILE_READ);
        records[index].request.operation.read.descriptor = pipes[index][0];
        records[index].request.operation.read.buffer = &landed[index];
        records[index].request.operation.read.count = 1;
        CHECK(wf_file_adapter_submit(&adapter, &records[index]) == WF_FILE_TARGET_OWNS);
    }
    for (index = 0; index < WF_PEER_PIPES; ++index) {
        wf_completion_record *writer = &records[WF_PEER_PIPES + index];
        harness_record_init(writer, WF_FILE_WRITE);
        writer->request.operation.write.descriptor = pipes[index][1];
        writer->request.operation.write.buffer = &sent[index];
        writer->request.operation.write.count = 1;
        CHECK(wf_file_adapter_submit(&adapter, writer) == WF_FILE_TARGET_OWNS);
    }
    CHECK(harness_await_all(records, 2u * WF_PEER_PIPES));
    for (index = 0; index < WF_PEER_PIPES; ++index) {
        CHECK(records[index].result.error_code == 0);
        CHECK(records[index].result.value == 1);
        CHECK(landed[index] == sent[index]);
    }
    CHECK(wf_file_adapter_helper_count(&adapter) > WF_FILE_MAX_HELPERS);
    CHECK(wf_file_adapter_shutdown(&adapter) == 0);

    /* A pinned pool keeps its count: the same requests leave the ninth read
     * and every writer queued behind eight helpers, until this thread writes
     * each pipe itself. */
    CHECK(wf_file_adapter_init(&adapter, &runtime, WF_FILE_MAX_HELPERS, 0) == 0);
    CHECK(wf_file_adapter_set_helper_cap(&adapter, WF_FILE_MAX_HELPERS) == 0);
    CHECK(wf_file_adapter_hold_for_contexts(&adapter, 0) == 0);
    for (index = 0; index < 2u * WF_PEER_PIPES; ++index) {
        harness_record_init(
            &records[index],
            index < WF_PEER_PIPES ? WF_FILE_READ : WF_FILE_WRITE
        );
        if (index < WF_PEER_PIPES) {
            records[index].request.operation.read.descriptor = pipes[index][0];
            records[index].request.operation.read.buffer = &landed[index];
            records[index].request.operation.read.count = 1;
        } else {
            unsigned pipe_index = index - WF_PEER_PIPES;
            records[index].request.operation.write.descriptor = pipes[pipe_index][1];
            records[index].request.operation.write.buffer = &sent[pipe_index];
            records[index].request.operation.write.count = 1;
        }
        CHECK(wf_file_adapter_submit(&adapter, &records[index]) == WF_FILE_TARGET_OWNS);
    }
    {
        struct timespec delay = {.tv_sec = 0, .tv_nsec = 1000000};
        unsigned attempts;
        for (attempts = 0; attempts < 5000u; ++attempts) {
            if (wf_file_adapter_queued(&adapter) == WF_PEER_PIPES + 1u) {
                break;
            }
            (void)nanosleep(&delay, NULL);
        }
    }
    CHECK(wf_file_adapter_queued(&adapter) == WF_PEER_PIPES + 1u);
    CHECK(wf_file_adapter_helper_count(&adapter) == WF_FILE_MAX_HELPERS);
    for (index = 0; index < WF_PEER_PIPES; ++index) {
        CHECK(write(pipes[index][1], &sent[index], 1) == 1);
    }
    CHECK(harness_await_all(records, 2u * WF_PEER_PIPES));
    CHECK(wf_file_adapter_helper_count(&adapter) == WF_FILE_MAX_HELPERS);
    CHECK(wf_file_adapter_shutdown(&adapter) == 0);

    for (index = 0; index < WF_PEER_PIPES; ++index) {
        CHECK(close(pipes[index][0]) == 0);
        CHECK(close(pipes[index][1]) == 0);
    }
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    return 0;
}


#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
/* The base-position cell after one progressing attempt: the value the Darwin
 * stub above writes, and the caller's own sentinel on a family whose facility
 * takes no such cell. */
static int64_t wf_expected_position(void) {
#if defined(__APPLE__)
    return 17;
#else
    return 4242;
#endif
}
#endif

/* One admitted enumeration is one progress-producing host attempt, whichever
 * family supplies the facility: interruption repeats the attempt, readiness
 * refusal waits for exactly one POLLIN on the enumerated descriptor and
 * repeats it, and only progress crosses the ABI. The Darwin facility's
 * base-position cell is written on the progressing attempt; Linux's facility
 * has none, and this case asserts the cell is left as the caller gave it. */
static int test_directory_progress_is_internal(void) {
#if defined(WF_FILE_HAS_DIRECTORY_NEXT)
    int descriptors[2];
    wf_harness_record record;
    unsigned char readiness = 'r';
    unsigned char byte = 0;
    int64_t position = 4242;
    int64_t value = -1;
    int error_code = -1;
    atomic_store(&wf_directory_scripted, 1);
    wf_directory_host_calls = 0;
    wf_directory_poll_calls = 0;
    wf_directory_poll_descriptor = -1;
    wf_directory_poll_events = 0;
    wf_directory_poll_timeout = 0;
    CHECK(pipe(descriptors) == 0);
    CHECK(write(descriptors[1], &readiness, 1) == 1);
    /* One lowering, so the enumeration is asserted once, on the submitted
     * route the direct entry's leg used to duplicate (design §8).  The poll
     * the readiness refusal waited on is checked here, where the direct leg
     * used to check it. */
    wf__completion_directory_next_submit(
        descriptors[0],
        &byte,
        1,
        &position,
        record.bytes
    );
    wf__completion_file_join(record.bytes, &value, &error_code);
    CHECK(value == 1 && error_code == 0);
    CHECK(wf_directory_host_calls == 3);
    CHECK(wf_directory_poll_calls == 1);
    CHECK(wf_directory_poll_descriptor == descriptors[0]);
    CHECK(wf_directory_poll_events == POLLIN);
    CHECK(wf_directory_poll_timeout == -1);
    CHECK(byte == 'd');
    CHECK(position == wf_expected_position());
    atomic_store(&wf_directory_scripted, 0);
    CHECK(close(descriptors[0]) == 0);
    CHECK(close(descriptors[1]) == 0);
#endif
    return 0;
}


/* The name of the test now running, for the watchdog below.  Written by the
 * main thread and read by a signal handler, which is why it is a plain
 * pointer to a string literal: the store is a single aligned word and the
 * handler only prints it. */
static const char *volatile wf_harness_running = "startup";

/* A deadlock is a failure mode of this suite, so it gets a name.
 *
 * A join that waits in place makes threads wait for each other on purpose,
 * and a mistake in the wake does not produce a wrong answer — it produces no
 * answer, which without this would surface as a build job that never ends and
 * a log with nothing in it. The bound is three orders of magnitude above what
 * the whole suite takes, so it can only fire on a schedule that has genuinely
 * stopped. */
static void wf_harness_watchdog(int signal_number) {
    static const char message[] = "completion harness: no progress in test: ";
    ssize_t ignored;
    (void)signal_number;
    ignored = write(2, message, sizeof(message) - 1u);
    ignored = write(2, wf_harness_running, strlen(wf_harness_running));
    ignored = write(2, "\n", 1);
    (void)ignored;
    _exit(9);
}

int wf_ordinary_values_tests(const char *scratch, const char *group);

int main(int argc, char **argv) {
    unsigned cases = 0;
    int trace = getenv("WF_COMPLETION_TRACE") != NULL;
    struct sigaction watchdog;
    memset(&watchdog, 0, sizeof(watchdog));
    watchdog.sa_handler = wf_harness_watchdog;
    if (sigaction(SIGALRM, &watchdog, NULL) != 0) {
        fprintf(stderr, "completion harness: no watchdog\n");
        return 2;
    }
    (void)alarm(300u);
#define RUN_TEST(...)                                                         \
    do {                                                                      \
        wf_harness_running = #__VA_ARGS__;                                    \
        if (trace) {                                                          \
            fprintf(stderr, "completion harness: begin %s\n", #__VA_ARGS__); \
        }                                                                     \
        ++cases;                                                            \
        if ((__VA_ARGS__) != 0) {                                             \
            return 1;                                                         \
        }                                                                     \
        if (trace) {                                                          \
            fprintf(stderr, "completion harness: end %s\n", #__VA_ARGS__);   \
        }                                                                     \
    } while (0)
    if (argc != 2 && argc != 3) {
        fprintf(stderr, "usage: %s SCRATCH_DIRECTORY [all|core|bridge|adapter|cache|ordinary-text|ordinary-io]\n", argv[0]);
        return 2;
    }
    const char *group = argc == 3 ? argv[2] : "all";
    int all = strcmp(group, "all") == 0;
    int core = all || strcmp(group, "core") == 0;
    int bridge = all || strcmp(group, "bridge") == 0;
    int adapter = all || strcmp(group, "adapter") == 0;
    int cache = all || strcmp(group, "cache") == 0;
    int text = all || strcmp(group, "ordinary-text") == 0;
    int ordinary = all || bridge || strcmp(group, "ordinary-io") == 0;
    if (!(core || bridge || adapter || cache || text || ordinary)) {
        fprintf(stderr, "unknown runtime test group: %s\n", group);
        return 2;
    }
    /* The bridge cases below assert which route an operation took, and under
     * the demand-driven policy that is the runtime's choice: it runs a
     * positioned read inside submit once it has measured this host's reads as
     * not waiting, so whether a case sees the queued route would depend on
     * what the cases before it happened to execute. A written WF_IO_HELPERS
     * pins the route with the count, so an invocation that did not name one
     * gets one here and every case asserts a route that is fixed.
     *
     * What that leaves untested here is the policy itself, and it is tested
     * where it can be decided rather than sampled:
     * `test_pool_stays_empty_when_operations_do_not_wait` and
     * `test_pool_grows_when_operations_wait` script the clock the policy
     * measures with, `bridge_default_probe.c` runs the unset policy end to
     * end, and `compiler/tests/programs` runs whole compiled programs under
     * it. */
    if (getenv("WF_IO_HELPERS") == NULL) {
        (void)setenv("WF_IO_HELPERS", "1", 0);
    }
    if (core) {
        RUN_TEST(test_an_io_join_waits_on_the_current_stack());
        RUN_TEST(test_a_completion_publishes_results());
        RUN_TEST(test_unified_wake_epoch());
        RUN_TEST(test_equal_epoch_notification_rearms_before_resleep());
        RUN_TEST(test_one_epoch_wakes_every_announced_thread());
        RUN_TEST(test_condition_notifications_coalesce_without_suppressing_external_wakes());
    }
    if (bridge) {
        RUN_TEST(test_exactly_one_completion_per_submission_under_race(argv[1]));
        RUN_TEST(test_bridge_independent_positioned_reads(argv[1]));
        RUN_TEST(test_bridge_typed_open_outcomes(argv[1]));
        RUN_TEST(test_submitted_open_resolves_the_submitters_bytes(argv[1]));
        RUN_TEST(test_long_borrowed_path_reaches_the_host(argv[1]));
        RUN_TEST(test_submissions_cross_the_native_queue_boundary(argv[1]));
        RUN_TEST(test_open_results_reach_every_independent_owner(argv[1]));
        RUN_TEST(test_bridge_read_completes_after_observed_wait());
        RUN_TEST(test_a_submitted_operation_is_kicked_before_it_waits(argv[1]));
        RUN_TEST(test_socket_lifecycle_and_the_pair_two_count());
        RUN_TEST(test_process_wide_target_helper_budget());
    }
    if (adapter) {
        RUN_TEST(test_directory_progress_is_internal());
        RUN_TEST(test_a_peer_bound_request_is_left_to_a_helper());
        RUN_TEST(test_a_peer_wait_never_waits_behind_peer_waits());
        RUN_TEST(test_readiness_refusal_is_not_a_terminal_outcome());
        RUN_TEST(test_single_thread_file_progress(argv[1]));
        RUN_TEST(test_pool_stays_empty_when_operations_do_not_wait(argv[1]));
        RUN_TEST(test_pool_grows_when_operations_wait());
        RUN_TEST(test_helper_growth_stops_at_the_declared_bound());
        RUN_TEST(test_helper_count_above_its_bound_is_refused());
    }
    if (cache) RUN_TEST(test_uncached_reads_are_target_policy_only(argv[1]));
    if (text) RUN_TEST(wf_ordinary_values_tests(argv[1], "text"));
    if (ordinary) {
        RUN_TEST(wf_ordinary_values_tests(argv[1], "file"));
        RUN_TEST(wf_ordinary_values_tests(argv[1], "directory"));
        RUN_TEST(wf_ordinary_values_tests(argv[1], "tcp"));
    }
#undef RUN_TEST
    (void)alarm(0);
    printf("completion-core-harness: PASS group=%s cases=%u racers=%d "
           "native=%" PRIu64 " adapter=%" PRIu64 "\n", group, cases,
           WF_HARNESS_RACERS, wf__completion_native_ring_submissions(),
           wf__completion_file_fallback_submissions());
    return 0;
}
