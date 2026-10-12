/* Linux host-query regression: the supplied kernel needs a wider affinity
 * mask than cpu_set_t and permits exactly three CPUs, two above CPU_SETSIZE.
 * The old primitive reports the unrelated online count and cannot inspect
 * those CPUs' performance levels. Failure fixtures guard against replacing
 * an unreadable restricted set with that online count. This image includes
 * the production primitive with local host-call substitutions; no shipping
 * branch selects a fixture. Keep beside the scheduler policy probes while
 * these native CPU queries have consumers. */
#define _GNU_SOURCE
#include "prim.h"
#include "entry.h"
#include <errno.h>
#include <fcntl.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "cpu-affinity-probe:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

enum { GROW, UNAVAILABLE, UNSUPPORTED, DENIED, FORBIDDEN, EMPTY, TOO_WIDE,
       ALLOCATION_REFUSED, GROWTH_ALLOCATION_REFUSED };
static unsigned fixture;
static unsigned attempts, allocations, frees, online_reads, capacity_reads;
static size_t previous_bytes;
static unsigned fixture_cpus[3] = {5u, CPU_SETSIZE + 17u, 3u * CPU_SETSIZE + 19u};
static cpu_set_t *probe_cpu_alloc(unsigned width) {
    ++allocations;
    if ((fixture == ALLOCATION_REFUSED && allocations == 1u)
        || (fixture == GROWTH_ALLOCATION_REFUSED && allocations == 2u)) return NULL;
    return malloc(CPU_ALLOC_SIZE(width));
}
static void probe_cpu_free(cpu_set_t *cpus) { ++frees; free(cpus); }

static int probe_getaffinity(pid_t process, size_t bytes, cpu_set_t *cpus) {
    CHECK(process == 0);
    CHECK(bytes == (previous_bytes == 0u ? CPU_ALLOC_SIZE(CPU_SETSIZE) : 2u * previous_bytes));
    previous_bytes = bytes;
    ++attempts;
    if (fixture == UNAVAILABLE) { errno = ENOSYS; return -1; }
    if (fixture == UNSUPPORTED) { errno = ENOTSUP; return -1; }
    if (fixture == DENIED) { errno = EACCES; return -1; }
    if (fixture == FORBIDDEN) { errno = EPERM; return -1; }
    if (fixture == TOO_WIDE || bytes < CPU_ALLOC_SIZE(4u * CPU_SETSIZE)) {
        errno = EINVAL;
        return -1;
    }
    CPU_ZERO_S(bytes, cpus);
    if (fixture != EMPTY)
        for (unsigned i = 0; i < 3u; ++i) CPU_SET_S(fixture_cpus[i], bytes, cpus);
    return 0;
}
static long probe_sysconf(int name) {
    CHECK(name == _SC_NPROCESSORS_ONLN);
    ++online_reads;
    return 8192;
}
static int probe_capacity_open(const char *path, int flags, ...) {
    unsigned cpu = 0;
    int consumed = 0;
    CHECK(flags == (O_RDONLY | O_CLOEXEC));
    CHECK(sscanf(path, "/sys/devices/system/cpu/cpu%u/cpu_capacity%n", &cpu, &consumed) == 1);
    CHECK(path[consumed] == '\0');
    for (unsigned i = 0; i < 3u; ++i)
        if (cpu == fixture_cpus[i]) return 100 + (int)i;
    CHECK(0);
    return -1;
}
static ssize_t probe_capacity_read(int file, void *buffer, size_t bytes) {
    CHECK(file >= 100 && file <= 102);
    const char *capacity = file == 101 ? "512\n" : "1024\n";
    size_t length = strlen(capacity);
    CHECK(bytes >= length);
    memcpy(buffer, capacity, length);
    ++capacity_reads;
    return (ssize_t)length;
}
static int probe_capacity_close(int file) { CHECK(file >= 100 && file <= 102); return 0; }

#define sched_getaffinity probe_getaffinity
#define sysconf probe_sysconf
#define open probe_capacity_open
#define read probe_capacity_read
#define close probe_capacity_close
#undef CPU_ALLOC
#define CPU_ALLOC(width) probe_cpu_alloc(width)
#undef CPU_FREE
#define CPU_FREE(cpus) probe_cpu_free(cpus)
#include "prim_host.c"
#undef sched_getaffinity
#undef sysconf
#undef open
#undef read
#undef close
#undef CPU_ALLOC
#undef CPU_FREE

static void reset(unsigned mode) {
    fixture = mode;
    attempts = allocations = frees = online_reads = capacity_reads = 0u;
    previous_bytes = 0u;
}

int main(void) {
    reset(GROW);
    CHECK(wf_prim_online_cpus() == 3u);
    CHECK(attempts == 3u && allocations == 3u && frees == 3u && online_reads == 0u);
    reset(GROW);
    CHECK(wf_prim_cpu_levels() == 2u);
    CHECK(attempts == 3u && frees == 3u && capacity_reads == 3u && online_reads == 0u);
    for (unsigned mode = UNAVAILABLE; mode <= GROWTH_ALLOCATION_REFUSED; ++mode) {
        reset(mode);
        int unavailable = mode == UNAVAILABLE || mode == UNSUPPORTED;
        CHECK(wf_prim_online_cpus() == (unavailable ? 8192u : 0u));
        CHECK(online_reads == (unavailable ? 1u : 0u));
        CHECK(frees == allocations - (mode == ALLOCATION_REFUSED || mode == GROWTH_ALLOCATION_REFUSED));
        if (mode == TOO_WIDE) {
            CHECK(previous_bytes == CPU_ALLOC_SIZE(WF_PRIM_AFFINITY_CPU_LIMIT));
            CHECK(attempts > 3u);
        }
        reset(mode);
        CHECK(wf_prim_cpu_levels() == 1u && online_reads == 0u);
        CHECK(frees == allocations - (mode == ALLOCATION_REFUSED || mode == GROWTH_ALLOCATION_REFUSED));
    }
    /* The default worker query calls this same primitive, not a separate
     * topology path. Its ordinary entry configuration sees three lanes. */
    reset(GROW);
    CHECK(unsetenv("WF_WORKERS") == 0);
    CHECK(unsetenv("WF_SCHED_REPORT") == 0);
    CHECK(unsetenv("WF_SPLIT_WORK") == 0);
    CHECK(wf__sched_lanes() == 3);
    CHECK(attempts == 3u && frees == 3u && online_reads == 0u);
    return 0;
}
