/* POSIX thread creation and per-lane waiting; no stack switching. */
/* Feature selection, and it has to precede every include. The monotonic clock
 * is POSIX and the CPU affinity mask is a glibc extension, and `-std=c11`
 * -- which the gate, the emitted link and this file's probes all compile with
 * -- hides both unless one of these is named first. macOS selects the Darwin
 * set for `sysctlbyname`, the same choice `wake_probe.c` makes. Each is
 * guarded so naming it again on a command line is not a redefinition. */
#if defined(__linux__)
#if !defined(_GNU_SOURCE)
#define _GNU_SOURCE 1
#endif
#elif defined(__APPLE__)
#if !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE 1
#endif
#elif !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif
#include "prim.h"
#include <fcntl.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#if defined(__APPLE__)
#include <sys/sysctl.h>
#endif

static void *wf_prim_thread_main(void *opaque) {
    wf_prim_thread *thread = opaque;
    thread->entry(thread->argument);
    return NULL;
}

int wf_prim_thread_start(
    wf_prim_thread *thread,
    void (*entry)(void *),
    void *argument,
    size_t stack_bytes
) {
    pthread_attr_t attributes;
    pthread_t created;
    int error;
    if (thread == NULL || entry == NULL) {
        return 1;
    }
    thread->entry = entry;
    thread->argument = argument;
    if (pthread_attr_init(&attributes) != 0) {
        return 1;
    }
    if (stack_bytes != 0
        && pthread_attr_setstacksize(&attributes, stack_bytes) != 0) {
        (void)pthread_attr_destroy(&attributes);
        return 1;
    }
    (void)pthread_attr_setdetachstate(&attributes, PTHREAD_CREATE_DETACHED);
    error = pthread_create(&created, &attributes, wf_prim_thread_main, thread);
    (void)pthread_attr_destroy(&attributes);
    return error != 0 ? 1 : 0;
}

/* How many CPUs this process may actually run on. The affinity mask is the
 * honest answer where it is cheap to read, because a process confined to two
 * of a machine's four CPUs is oversubscribed at four lanes however many CPUs
 * are online; sysconf answers the rest. Zero means the count is unknown, and
 * a caller must then choose the behaviour that assumes nothing about it. */
unsigned wf_prim_online_cpus(void) {
    long online;
#if defined(__linux__)
    cpu_set_t affinity;
    CPU_ZERO(&affinity);
    if (sched_getaffinity(0, sizeof affinity, &affinity) == 0) {
        int permitted = CPU_COUNT(&affinity);
        if (permitted > 0) {
            return (unsigned)permitted;
        }
    }
#endif
#if defined(__APPLE__)
    int logical = 0;
    size_t width = sizeof(logical);
    if (sysctlbyname("hw.logicalcpu", &logical, &width, NULL, 0) == 0
        && logical > 0) {
        return (unsigned)logical;
    }
#endif
    online = sysconf(_SC_NPROCESSORS_ONLN);
    return online > 0 ? (unsigned)online : 0u;
}

/* The most distinct performance levels this probe will name before it stops
 * distinguishing them. Four is already more than any shipping part has, and a
 * machine that exceeds it is asymmetric several times over, so the ceiling is
 * a bound on the work and never on the conclusion. */
#define WF_PRIM_CPU_LEVEL_CEILING 4u

#if defined(__linux__)
/* One CPU's scheduler capacity, normalized by the kernel so the fastest CPU
 * on the machine reads 1024. Zero return means the file was not there or did
 * not parse, which is not the same as a capacity of zero. Fixed buffers, one
 * open and one read: this runs once per process, at pool start. */
static int wf_prim_cpu_capacity(unsigned cpu, unsigned long *capacity) {
    char path[64];
    char text[32];
    int file;
    ssize_t got;
    char *end = NULL;
    unsigned long value;
    int written = snprintf(
        path, sizeof path, "/sys/devices/system/cpu/cpu%u/cpu_capacity", cpu);
    if (written < 0 || (size_t)written >= sizeof path) {
        return 0;
    }
    file = open(path, O_RDONLY | O_CLOEXEC);
    if (file < 0) {
        return 0;
    }
    got = read(file, text, sizeof text - 1u);
    (void)close(file);
    if (got <= 0) {
        return 0;
    }
    text[got] = '\0';
    value = strtoul(text, &end, 10);
    if (end == text || value == 0ul) {
        return 0;
    }
    *capacity = value;
    return 1;
}
#endif

/* How many distinct performance levels the CPUs this process may run on are
 * drawn from. One is both "they are alike" and "this host does not say", and
 * the caller must treat those the same: it is the answer that assumes nothing.
 *
 * Darwin answers the question directly. `hw.nperflevels` is the number of
 * levels the kernel groups this machine's cores into -- two on every Apple
 * Silicon part that has efficiency cores, one on a part that does not -- and
 * the name simply does not exist on Intel Macs or on macOS before 12, where
 * the sysctl fails and one is the answer.
 *
 * Linux has no single name for it, and what it does have is per CPU: the
 * scheduler's capacity of each CPU in `cpu_capacity`, normalized so the
 * fastest CPU on the machine reads 1024. Counting the distinct values over
 * the affinity mask -- the same set of CPUs wf_prim_online_cpus counts, read
 * the same way -- answers the question wherever that file exists, which on
 * the arm64 hosts that carry big.LITTLE is everywhere. A CPU in the mask
 * whose capacity cannot be read makes the whole answer unknown rather than a
 * count over the CPUs that did answer, because a count over a subset can
 * report one level for a machine that has two.
 *
 * THE GAP, stated rather than guessed around: x86 Linux publishes no
 * `cpu_capacity` at all -- capacity-aware scheduling is not wired to that
 * topology -- so an Intel hybrid part with performance and efficiency cores
 * reads no file here and this probe answers one for it. What would detect it
 * is the per-CPU maximum frequency the `intel_pstate` driver publishes,
 * `cpufreq/cpuinfo_max_freq` under each CPU, whose performance and efficiency
 * values differ on a hybrid part; using it means first telling a hybrid part
 * from a machine whose CPUs merely carry different boost ceilings, and that
 * has not been measured on such a host. Until it is, x86 Linux counts as
 * uniform and the caller keeps the behaviour it had before this probe
 * existed. */
unsigned wf_prim_cpu_levels(void) {
#if defined(__APPLE__)
    int levels = 0;
    size_t width = sizeof levels;
    if (sysctlbyname("hw.nperflevels", &levels, &width, NULL, 0) == 0
        && levels > 0) {
        return (unsigned)levels;
    }
    return 1u;
#elif defined(__linux__)
    cpu_set_t affinity;
    unsigned long seen[WF_PRIM_CPU_LEVEL_CEILING];
    unsigned levels = 0;
    unsigned cpu;
    CPU_ZERO(&affinity);
    if (sched_getaffinity(0, sizeof affinity, &affinity) != 0) {
        return 1u;
    }
    for (cpu = 0; cpu < (unsigned)CPU_SETSIZE; cpu += 1u) {
        unsigned long capacity = 0ul;
        unsigned index;
        if (!CPU_ISSET((int)cpu, &affinity)) {
            continue;
        }
        if (!wf_prim_cpu_capacity(cpu, &capacity)) {
            return 1u;
        }
        for (index = 0; index < levels; index += 1u) {
            if (seen[index] == capacity) {
                break;
            }
        }
        if (index < levels) {
            continue;
        }
        if (levels == WF_PRIM_CPU_LEVEL_CEILING) {
            return WF_PRIM_CPU_LEVEL_CEILING;
        }
        seen[levels] = capacity;
        levels += 1u;
    }
    return levels > 0u ? levels : 1u;
#else
    return 1u;
#endif
}

uint64_t wf_prim_monotonic_us(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
        return 0;
    }
    return (uint64_t)now.tv_sec * UINT64_C(1000000)
        + (uint64_t)now.tv_nsec / UINT64_C(1000);
}

#if defined(WF_PAR_TRACE)
/* The lane trace's clock; see prim.h. Behind the instrument's guard, so an
 * ordinary build of this file has the same bytes it had before it existed. */
uint64_t wf_prim_monotonic_ns(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
        return 0;
    }
    return (uint64_t)now.tv_sec * UINT64_C(1000000000) + (uint64_t)now.tv_nsec;
}
#endif

int wf_prim_setting_text(const char *name, char *buffer, size_t capacity) {
    const char *text;
    size_t length;
    if (name == NULL || buffer == NULL || capacity == 0) {
        return -1;
    }
    buffer[0] = '\0';
    text = getenv(name);
    if (text == NULL) {
        return 0;
    }
    length = strlen(text);
    if (length >= capacity) {
        return -1;
    }
    memcpy(buffer, text, length + 1);
    return 1;
}

__attribute__((weak)) void wf__floor_attach_thread(void) {}
void wf_prim_floor_attach(void) { wf__floor_attach_thread(); }
void wf_prim_yield(void) { sched_yield(); }
int wf_prim_wait_init(wf_prim_wait *wait) {
    if (pthread_mutex_init(&wait->lock, NULL) != 0) return 1;
    if (pthread_cond_init(&wait->signal, NULL) == 0) return 0;
    pthread_mutex_destroy(&wait->lock);
    return 1;
}
void wf_prim_wait_destroy(wf_prim_wait *wait) {
    pthread_cond_destroy(&wait->signal);
    pthread_mutex_destroy(&wait->lock);
}
void wf_prim_wait_lock(wf_prim_wait *wait) { pthread_mutex_lock(&wait->lock); }
void wf_prim_wait_unlock(wf_prim_wait *wait) { pthread_mutex_unlock(&wait->lock); }
void wf_prim_wait_sleep(wf_prim_wait *wait) { pthread_cond_wait(&wait->signal, &wait->lock); }
void wf_prim_wait_signal(wf_prim_wait *wait) { pthread_cond_signal(&wait->signal); }
