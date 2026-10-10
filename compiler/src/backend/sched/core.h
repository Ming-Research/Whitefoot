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
/* Logical allocation account, independent of the thread's context pointer.
 * Drivers install it on resume; compute callbacks save/restore it even when
 * a joining driver helps work offered by another context. Slot lifetime is
 * protected by the runner's activity through its structured joins. */
unsigned wf__scope_current(void);
unsigned wf__scope_swap(unsigned scope);
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
uint64_t wf__par_split_budget(uint64_t span, uint64_t weight);
uint64_t wf__par_recursion_budget(void);
unsigned long wf__par_grants(void);
unsigned wf__sched_pool_running(void);
#endif
