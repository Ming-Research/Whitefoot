#if !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif
/* F_NOCACHE, the Darwin half of the WF_IO_NOCACHE target policy, is a BSD
 * extension the POSIX macro above hides, exactly as in bridge.c. */
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE 1
#endif

#include "contract.h"
#include "file_adapter.h"
#include "snapshot_export.h"
#include "../sched/core.h"

#include <errno.h>
#include <stdlib.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

/* This isolated adapter fixture supplies its closed snapshot leaf just as it
 * supplies completion publication. No snapshot request belongs to this probe. */
void wf_snapshot_run_finish(wf_snapshot_owner *owner, uint32_t abort_requested) {
    (void)owner;
    (void)abort_requested;
    abort();
}

/*
 * Isolated positioned-read qualification probe.
 *
 * This executable deliberately links only the scheduler core, runtime.c and
 * file_adapter.c.  It must remain independent of bridge.c and the
 * generated-code ABI.  The Makefile fixes the complete source whitelist and
 * supplements it with an nm guard after these behavioural checks pass.
 *
 * Retired with the record pool (design §7, "The record's pool machinery:
 * deleted, not answered"), because every property each one asserted was a
 * property of that pool:
 *
 *   - `test_capacity_wait_and_retry`: the claim's WAIT_CAPACITY answer, the
 *     adapter's queue-full answer, and the retry the caller made after a
 *     release.  There is no slot to claim, no fixed queue to fill, and no
 *     capacity answer anywhere in the runtime; an operation that reaches the
 *     adapter is queued on a list threaded through the records themselves.
 *   - `test_inline_stale_and_duplicate_terminal`: the generation check and the
 *     duplicate-terminal refusal.  A record has no generation and cannot be
 *     recycled under a late publisher: it is a block of the submitting frame
 *     and the frame joins it before any terminator.  "Exactly one terminal per
 *     submission" is now an impossibility rather than a checked refusal, and
 *     the property is tested in its new form -- under a race, by many threads
 *     -- in `harness.c`.
 *   - `test_drain_consume_and_scheduler_parking`: the drain, the consume, and
 *     the ready-event count they moved.  A completion is published straight
 *     into its record and there is no event to drain.  The epoch park it also
 *     exercised survives, and is tested on the real bridge, where the park and
 *     the wake are the ones a program actually uses, by `harness.c`'s
 *     `test_unified_wake_epoch`, `test_one_epoch_wakes_every_announced_thread`
 *     and `test_helper_completion_wakes_an_in_place_waiter`.
 */

#define CHECK(condition)                                                      \
    do {                                                                      \
        if (!(condition)) {                                                   \
            fprintf(                                                          \
                stderr,                                                       \
                "completion core/read probe: %s:%d: check failed: %s\n",    \
                __FILE__,                                                     \
                __LINE__,                                                     \
                #condition                                                    \
            );                                                                \
            return 1;                                                         \
        }                                                                     \
    } while (0)

static _Atomic uint64_t positioned_read_host_calls;

/* Narrow link-time hook used only by this probe.  Nonempty positioned reads
 * still reach the real host facility, while the empty case can prove that it
 * never crosses this boundary. */
ssize_t wf_completion_test_pread(
    int descriptor,
    void *buffer,
    size_t count,
    off_t offset
) {
    atomic_fetch_add_explicit(
        &positioned_read_host_calls,
        1,
        memory_order_relaxed
    );
    return pread(descriptor, buffer, count, offset);
}

/* The bridge supplies this entry point in a delivered program. This adapter
 * probe has no waiting caller, so it publishes the result with the contract's
 * release store and needs neither the bridge nor a compute scheduler. */

void wf_completion_record_complete(wf_completion_record *record) {
    wf_completion_record_publish(record);
}

static int record_is_done(const wf_completion_record *record) {
    return record->state == WF_COMPLETION_DONE;
}

/* Runs one positioned read through the adapter and reads its record back.
 *
 * There is no token, no drain and no consume: the record is the caller's own
 * storage, the adapter completes it in place, and the caller reads the result
 * head out of it. */
static int complete_positioned_read(
    wf_file_adapter *adapter,
    int descriptor,
    void *buffer,
    size_t count,
    int64_t offset,
    wf_file_result_head *result
) {
    wf_completion_record record;

    memset(&record, 0, sizeof(record));
    wf_completion_record_init(&record);
    record.request.kind = WF_FILE_PREAD;
    record.request.operation.pread.descriptor = descriptor;
    record.request.operation.pread.buffer = buffer;
    record.request.operation.pread.count = count;
    record.request.operation.pread.offset = offset;

    CHECK(wf_file_adapter_submit(adapter, &record) == WF_FILE_TARGET_OWNS);
    CHECK(wf_file_adapter_progress(adapter, 1) == 1);
    CHECK(record_is_done(&record));
    *result = record.result;
    return 0;
}

static int create_payload_file(
    const char *scratch_directory,
    char *path,
    size_t path_capacity
) {
    static const char payload[] = "abcdef";
    int descriptor;
    int written = snprintf(
        path,
        path_capacity,
        "%s/wf-completion-core-read-%ld",
        scratch_directory,
        (long)getpid()
    );
    CHECK(written > 0 && (size_t)written < path_capacity);
    (void)unlink(path);
    descriptor = open(path, O_CREAT | O_EXCL | O_RDWR, 0600);
    CHECK(descriptor >= 0);
    CHECK(write(descriptor, payload, sizeof(payload) - 1u)
          == (ssize_t)(sizeof(payload) - 1u));
    return descriptor;
}

static int test_positioned_read_result_boundaries(
    const char *scratch_directory
) {
    wf_completion_runtime runtime;
    wf_file_adapter adapter;
    wf_file_result_head result;
    unsigned char empty_destination = 0xa5u;
    unsigned char full[3] = {0xa5u, 0xa5u, 0xa5u};
    unsigned char short_read[4] = {0xa5u, 0xa5u, 0xa5u, 0xa5u};
    unsigned char eof[3] = {0xa5u, 0xa5u, 0xa5u};
    unsigned char error[3] = {0xa5u, 0xa5u, 0xa5u};
    unsigned char negative[3] = {0xa5u, 0xa5u, 0xa5u};
    unsigned char unrepresentable[3] = {0xa5u, 0xa5u, 0xa5u};
    uint64_t host_calls_before;
    char path[256];
    int descriptor = create_payload_file(
        scratch_directory,
        path,
        sizeof(path)
    );

    CHECK(descriptor >= 0);
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 0, 0) == 0);

    host_calls_before = atomic_load_explicit(
        &positioned_read_host_calls,
        memory_order_relaxed
    );
    CHECK(
        complete_positioned_read(
            &adapter,
            -1,
            &empty_destination,
            0,
            -1,
            &result
        ) == 0
    );
    CHECK(result.kind == WF_FILE_PREAD);
    CHECK(result.value == 0 && result.error_code == 0);
    CHECK(empty_destination == 0xa5u);
    CHECK(
        atomic_load_explicit(
            &positioned_read_host_calls,
            memory_order_relaxed
        ) == host_calls_before
    );

    CHECK(
        complete_positioned_read(
            &adapter,
            descriptor,
            full,
            sizeof(full),
            0,
            &result
        ) == 0
    );
    CHECK(result.value == 3 && result.error_code == 0);
    CHECK(memcmp(full, "abc", sizeof(full)) == 0);

    CHECK(
        complete_positioned_read(
            &adapter,
            descriptor,
            short_read,
            sizeof(short_read),
            4,
            &result
        ) == 0
    );
    CHECK(result.value == 2 && result.error_code == 0);
    CHECK(short_read[0] == 'e' && short_read[1] == 'f');
    CHECK(short_read[2] == 0xa5u && short_read[3] == 0xa5u);

    CHECK(
        complete_positioned_read(
            &adapter,
            descriptor,
            eof,
            sizeof(eof),
            6,
            &result
        ) == 0
    );
    CHECK(result.value == 0 && result.error_code == 0);
    CHECK(eof[0] == 0xa5u && eof[1] == 0xa5u && eof[2] == 0xa5u);

    CHECK(
        complete_positioned_read(
            &adapter,
            -1,
            error,
            sizeof(error),
            0,
            &result
        ) == 0
    );
    CHECK(result.value == -1 && result.error_code == EBADF);
    CHECK(error[0] == 0xa5u && error[1] == 0xa5u && error[2] == 0xa5u);

    host_calls_before = atomic_load_explicit(
        &positioned_read_host_calls,
        memory_order_relaxed
    );
    CHECK(
        complete_positioned_read(
            &adapter,
            descriptor,
            negative,
            sizeof(negative),
            -1,
            &result
        ) == 0
    );
    CHECK(result.value == -1 && result.error_code == EINVAL);
    CHECK(
        negative[0] == 0xa5u && negative[1] == 0xa5u
        && negative[2] == 0xa5u
    );
    CHECK(
        atomic_load_explicit(
            &positioned_read_host_calls,
            memory_order_relaxed
        ) == host_calls_before + 1u
    );

    /* The request stores an int64_t offset.  On the supported 64-bit POSIX
     * targets every stored value is representable by off_t.  Keep the narrow
     * ABI case qualified for any target where off_t is smaller: rejection must
     * happen before the hook and must leave the destination untouched. */
    if ((int64_t)(off_t)INT64_MAX != INT64_MAX) {
        host_calls_before = atomic_load_explicit(
            &positioned_read_host_calls,
            memory_order_relaxed
        );
        CHECK(
            complete_positioned_read(
                &adapter,
                descriptor,
                unrepresentable,
                sizeof(unrepresentable),
                INT64_MAX,
                &result
            ) == 0
        );
        CHECK(result.value == -1 && result.error_code == EINVAL);
        CHECK(
            unrepresentable[0] == 0xa5u
            && unrepresentable[1] == 0xa5u
            && unrepresentable[2] == 0xa5u
        );
        CHECK(
            atomic_load_explicit(
                &positioned_read_host_calls,
                memory_order_relaxed
            ) == host_calls_before
        );
    }

    CHECK(wf_file_adapter_shutdown(&adapter) == 0);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    CHECK(close(descriptor) == 0);
    CHECK(unlink(path) == 0);
    return 0;
}

static int test_independent_reads_complete_in_reverse_order(
    const char *scratch_directory
) {
    wf_completion_runtime runtime;
    wf_completion_record first;
    wf_completion_record second;
    wf_file_adapter adapter;
    unsigned char first_byte = 0xa5u;
    unsigned char second_byte = 0xa5u;
    char path[256];
    int descriptor = create_payload_file(
        scratch_directory,
        path,
        sizeof(path)
    );

    CHECK(descriptor >= 0);
    CHECK(wf_completion_runtime_init(&runtime) == 0);
    CHECK(wf_file_adapter_init(&adapter, &runtime, 0, 0) == 0);

    memset(&first, 0, sizeof(first));
    wf_completion_record_init(&first);
    first.request.kind = WF_FILE_PREAD;
    first.request.operation.pread.descriptor = descriptor;
    first.request.operation.pread.buffer = &first_byte;
    first.request.operation.pread.count = 1;
    first.request.operation.pread.offset = 0;

    second = first;
    wf_completion_record_init(&second);
    second.request.operation.pread.buffer = &second_byte;
    second.request.operation.pread.offset = 5;

    CHECK(wf_file_adapter_submit(&adapter, &first) == WF_FILE_TARGET_OWNS);
    CHECK(wf_file_adapter_submit(&adapter, &second) == WF_FILE_TARGET_OWNS);

    /* Zero-helper progress deliberately takes the newest request.  This makes
     * the reverse completion observable instead of merely allowed. */
    CHECK(wf_file_adapter_progress(&adapter, 1) == 1);
    CHECK(record_is_done(&second));
    CHECK(!record_is_done(&first));
    CHECK(second.result.value == 1 && second.result.error_code == 0);
    CHECK(second_byte == 'f');
    CHECK(first_byte == 0xa5u);

    CHECK(wf_file_adapter_progress(&adapter, 1) == 1);
    CHECK(record_is_done(&first));
    CHECK(first.result.value == 1 && first.result.error_code == 0);
    CHECK(first_byte == 'a');

    CHECK(wf_file_adapter_shutdown(&adapter) == 0);
    CHECK(wf_completion_runtime_destroy(&runtime) == 0);
    CHECK(close(descriptor) == 0);
    CHECK(unlink(path) == 0);
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s SCRATCH_DIRECTORY\n", argv[0]);
        return 2;
    }
    atomic_init(&positioned_read_host_calls, 0);
    CHECK(test_positioned_read_result_boundaries(argv[1]) == 0);
    CHECK(test_independent_reads_complete_in_reverse_order(argv[1]) == 0);
    puts("completion-core-read-probe: PASS");
    return 0;
}
