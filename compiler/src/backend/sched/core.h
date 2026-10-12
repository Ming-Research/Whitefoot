#ifndef WHITEFOOT_SCHED_CORE_H
#define WHITEFOOT_SCHED_CORE_H
#include <stddef.h>
#include <stdint.h>
#define WF_SCHED_FRAME_BYTES 256u
#ifndef WF_SCHED_LANE_SLOTS
#define WF_SCHED_LANE_SLOTS 64u
#endif
_Static_assert((WF_SCHED_LANE_SLOTS & (WF_SCHED_LANE_SLOTS - 1)) == 0, "deque size must be a power of two");
#define WF_SCHED_MAX_THREADS 64u
#ifndef WF_SCHED_STATS
#define WF_SCHED_STATS 1
#endif
/* One owner per lane. Acquire/publish/join/read/release is structured and
 * stays on that owner's ordinary stack. Oversized/full slots refuse; the
 * compiler executes the same ordinary call. These u64 ABI values must stay
 * 64-bit on Windows LLP64 as well as POSIX LP64. */
void *wf__par_acquire_lane(uint64_t bytes);
void wf__par_publish(void *frame, void (*run)(void *));
void wf__par_join(void *frame);
void wf__par_release(void *frame);
int wf__par_pool_active(void);
/* This thread's demand request word, nonzero while an idle thief asks it for
 * work. A --par-demand module polls it with one thread-local load; the
 * accessor returns the same word for native probes. World selection
 * (wf__par_pool_active) starts the pool, registers the word and posts the
 * owner's own first request on it. */
#if defined(WF_PAR_DEMAND)
#if defined(_WIN32)
extern _Thread_local uint64_t wf__par_demand_word;
#else
extern __attribute__((tls_model("initial-exec"))) _Thread_local uint64_t wf__par_demand_word;
#endif
#endif
uint64_t wf__par_demand_requested(void);
uint64_t wf__par_split_budget(uint64_t span, uint64_t weight);
uint64_t wf__par_recursion_budget(void);
unsigned long wf__par_grants(void);
unsigned wf__sched_pool_running(void);
#if defined(WF_PAR_DIAGNOSTIC_OBSERVATIONS) || defined(WF_PAR_DIAGNOSTICS)
#include <stdio.h>
/* Research images only: serialize observations outside every timed call.
 * No observation changes startup, publication or the idle policy. */
void wf__par_diagnostic_observe(FILE *stream);
#endif
#if defined(WF_PAR_DIAGNOSTICS)
void wf__par_diagnostic_snapshot(FILE *stream);
void wf__par_diagnostic_trace(FILE *stream);
#endif
#endif
