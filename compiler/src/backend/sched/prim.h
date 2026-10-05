#ifndef WHITEFOOT_SCHED_PRIM_H
#define WHITEFOOT_SCHED_PRIM_H
/* Native thread, wait, yield and spin primitives. Deque atomics remain
 * inline in the shared core, with the same ordering on every target. */
#include <stddef.h>
#include <stdint.h>
#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
typedef struct { SRWLOCK lock; CONDITION_VARIABLE signal; } wf_prim_wait;
#else
#include <pthread.h>
typedef struct { pthread_mutex_t lock; pthread_cond_t signal; } wf_prim_wait;
#endif
typedef struct { void (*entry)(void *); void *argument; } wf_prim_thread;
/* One spin round's hint to the core: a spinning lane should give up issue
 * slots to its SMT sibling rather than run the loop at full rate. Windows has
 * YieldProcessor (which mingw and MSVC define as `pause` on x86 and
 * `dmb ishst; yield` on arm); the POSIX hosts had nothing here until the
 * hosted SMT runners showed idle lanes stretching a working sibling, so x86
 * gets `pause` and aarch64 gets `yield`; any other architecture keeps a plain
 * poll. This is a hint to the hardware, never a scheduler call. */
static inline void wf_prim_spin_hint(void) {
#if defined(_WIN32)
    YieldProcessor();
#elif defined(__x86_64__) || defined(__i386__)
    __builtin_ia32_pause();
#elif defined(__aarch64__)
    __asm__ __volatile__("yield" ::: "memory");
#endif
}
int wf_prim_wait_init(wf_prim_wait *wait);
void wf_prim_wait_destroy(wf_prim_wait *wait);
void wf_prim_wait_lock(wf_prim_wait *wait);
void wf_prim_wait_unlock(wf_prim_wait *wait);
void wf_prim_wait_sleep(wf_prim_wait *wait);
void wf_prim_wait_signal(wf_prim_wait *wait);
int wf_prim_thread_start(wf_prim_thread *thread, void (*entry)(void *), void *argument, size_t stack_bytes);
void wf_prim_floor_attach(void);
void wf_prim_yield(void);
unsigned wf_prim_online_cpus(void);
/* How many distinct performance levels the CPUs this process may run on are
 * drawn from. One means they are all alike, and one is also the answer when
 * the host does not say, so a caller reading one behaves exactly as it did
 * before this question could be asked. Above one is an asymmetric machine --
 * Apple's performance and efficiency cores, Intel's hybrid parts, Arm
 * big.LITTLE -- where two CPUs that wf_prim_online_cpus counts alike do not
 * run at the same speed. Never zero: a count of levels has no unknown value
 * distinct from "assume nothing", and one IS assuming nothing here. */
unsigned wf_prim_cpu_levels(void);
/* Monotonic microseconds from an unspecified fixed origin, so only
 * differences between two readings mean anything. Zero is reserved: it is
 * what a host with no usable monotonic clock answers, and a caller that reads
 * zero must fall back to behaviour that needs no clock rather than treat the
 * difference as elapsed time. */
uint64_t wf_prim_monotonic_us(void);
#if defined(WF_PAR_TRACE)
/* The same clock at nanosecond resolution, and it exists ONLY for the lane
 * trace core.c compiles under WF_PAR_TRACE: that instrument times executed
 * chunks and a fixed calibration loop of a few microseconds, neither of which
 * whole microseconds resolve. Nothing that ships reads it, so it lives behind
 * the instrument's own guard and an ordinary build has neither the declaration
 * nor the definition. Zero is reserved on the same terms as the microsecond
 * reading above. */
uint64_t wf_prim_monotonic_ns(void);
#endif
#define WF_PRIM_SETTING_BYTES 64u
int wf_prim_setting_text(const char *name, char *buffer, size_t capacity);
#endif
