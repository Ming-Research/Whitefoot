#if !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
/* `pthread_cond_timedwait_relative_np` is a Darwin extension. */
#define _DARWIN_C_SOURCE
#endif

/*
 * The host's one wait set: a mutex and a condition variable.
 *
 * This is the whole of what `runtime.c` cannot write once for both platforms
 * (`contract.h`, "the wait").  Its twin is `wait_windows.c`.  Nothing here
 * knows about the epoch, the announcement or the park protocol -- those are
 * `runtime.c`'s and are one implementation.
 */

#include "contract.h"

#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <string.h>
#include <time.h>

#if defined(WF_COMPLETION_WAIT_RETURN)
extern void WF_COMPLETION_WAIT_RETURN(wf_completion_wait *wait);
#endif

typedef struct wf_completion_wait_host {
    pthread_mutex_t lock;
    pthread_cond_t condition;
} wf_completion_wait_host;

_Static_assert(
    sizeof(wf_completion_wait_host) <= sizeof(wf_completion_wait),
    "the host's wait set must fit the block the shared contract reserves"
);
_Static_assert(
    _Alignof(wf_completion_wait_host) <= _Alignof(wf_completion_wait),
    "the host's wait set must not out-align the block the contract reserves"
);

static wf_completion_wait_host *wf_completion_wait_of(
    wf_completion_wait *wait
) {
    return (wf_completion_wait_host *)(void *)wait;
}

int wf_completion_wait_init(wf_completion_wait *wait) {
    wf_completion_wait_host *host;
    int error;
    if (wait == NULL) {
        return EINVAL;
    }
    memset(wait, 0, sizeof(*wait));
    host = wf_completion_wait_of(wait);
    error = pthread_mutex_init(&host->lock, NULL);
    if (error != 0) {
        return error;
    }
#if defined(__APPLE__)
    error = pthread_cond_init(&host->condition, NULL);
#else
    {
        /* A bounded sleep ends on the monotonic clock, so a calendar the
         * host sets back cannot stretch a deadline's wait [PRE-2]. */
        pthread_condattr_t attributes;
        error = pthread_condattr_init(&attributes);
        if (error == 0) {
            error = pthread_condattr_setclock(&attributes, CLOCK_MONOTONIC);
            if (error == 0) {
                error = pthread_cond_init(&host->condition, &attributes);
            }
            (void)pthread_condattr_destroy(&attributes);
        }
    }
#endif
    if (error != 0) {
        (void)pthread_mutex_destroy(&host->lock);
        return error;
    }
    return 0;
}

int wf_completion_wait_destroy(wf_completion_wait *wait) {
    wf_completion_wait_host *host;
    int first_error;
    int error;
    if (wait == NULL) {
        return EINVAL;
    }
    host = wf_completion_wait_of(wait);
    first_error = pthread_cond_destroy(&host->condition);
    error = pthread_mutex_destroy(&host->lock);
    return first_error != 0 ? first_error : error;
}

void wf_completion_wait_lock(wf_completion_wait *wait) {
    (void)pthread_mutex_lock(&wf_completion_wait_of(wait)->lock);
}

void wf_completion_wait_unlock(wf_completion_wait *wait) {
    (void)pthread_mutex_unlock(&wf_completion_wait_of(wait)->lock);
}

#if !defined(__APPLE__)
static struct timespec wf_completion_wait_deadline(uint32_t milliseconds) {
    struct timespec deadline;
    uint64_t nanoseconds;
    (void)clock_gettime(CLOCK_MONOTONIC, &deadline);
    nanoseconds = (uint64_t)deadline.tv_nsec
        + (uint64_t)(milliseconds % 1000u) * 1000000u;
    deadline.tv_sec += (time_t)(milliseconds / 1000u)
        + (time_t)(nanoseconds / 1000000000u);
    deadline.tv_nsec = (long)(nanoseconds % 1000000000u);
    return deadline;
}
#endif

enum wf_completion_wait_result wf_completion_wait_sleep(
    wf_completion_wait *wait,
    uint32_t timeout_milliseconds
) {
    wf_completion_wait_host *host = wf_completion_wait_of(wait);
    int error;
    if (timeout_milliseconds == UINT32_MAX) {
        error = pthread_cond_wait(&host->condition, &host->lock);
    } else {
#if defined(__APPLE__)
        /* Darwin measures a relative wait on its own monotonic clock and
         * has no way to set a condition's clock. */
        struct timespec interval;
        interval.tv_sec = (time_t)(timeout_milliseconds / 1000u);
        interval.tv_nsec = (long)(timeout_milliseconds % 1000u) * 1000000L;
        error = pthread_cond_timedwait_relative_np(
            &host->condition,
            &host->lock,
            &interval
        );
#else
        struct timespec deadline =
            wf_completion_wait_deadline(timeout_milliseconds);
        error = pthread_cond_timedwait(
            &host->condition,
            &host->lock,
            &deadline
        );
#endif
    }
#if defined(WF_COMPLETION_WAIT_RETURN)
    /* Harness observation only, with the real condition lock reacquired. */
    WF_COMPLETION_WAIT_RETURN(wait);
#endif
    if (error == ETIMEDOUT) {
        return WF_COMPLETION_WAIT_TIMED_OUT;
    }
    return error == 0 ? WF_COMPLETION_WAIT_WOKEN : WF_COMPLETION_WAIT_FAILED;
}

void wf_completion_wait_wake(wf_completion_wait *wait, int all) {
    wf_completion_wait_host *host = wf_completion_wait_of(wait);
    if (all != 0) {
        (void)pthread_cond_broadcast(&host->condition);
        return;
    }
    (void)pthread_cond_signal(&host->condition);
}
