/* Linux host-query regression: the supplied kernel permits exactly three
 * CPUs, including CPUs above CPU_SETSIZE, in either the initial stack mask
 * or a mask reached by two mapped growth steps. The old fixed-mask primitive
 * reports the unrelated online count and cannot inspect those CPUs' levels;
 * the heap-mask replacement unnecessarily depends on the program allocator.
 * Failure fixtures guard against replacing
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
#include <sys/mman.h>

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "cpu-affinity-probe:%d: %s\n", __LINE__, #test); exit(1); \
} } while (0)

enum { FIT, GROW, UNAVAILABLE, UNSUPPORTED, DENIED, FORBIDDEN, EMPTY, TOO_WIDE,
       MAPPING_REFUSED, GROWTH_MAPPING_REFUSED };
static unsigned fixture;
static unsigned attempts, mappings, unmaps, online_reads, capacity_reads;
static size_t previous_bytes, required_bytes, mapped_bytes;
static void *mapped_mask;
static unsigned fixture_cpus[3];
static void *probe_mask_map(void *address, size_t bytes, int protections,
                            int flags, int file, off_t offset) {
    CHECK(address == NULL && mapped_mask == NULL);
    CHECK(protections == (PROT_READ | PROT_WRITE));
    CHECK(flags == (MAP_PRIVATE | MAP_ANONYMOUS) && file == -1 && offset == 0);
    ++mappings;
    if ((fixture == MAPPING_REFUSED && mappings == 1u)
        || (fixture == GROWTH_MAPPING_REFUSED && mappings == 2u)) {
        errno = ENOMEM;
        return MAP_FAILED;
    }
    mapped_mask = mmap(address, bytes, protections, flags, file, offset);
    CHECK(mapped_mask != MAP_FAILED);
    mapped_bytes = bytes;
    return mapped_mask;
}
static int probe_mask_unmap(void *mask, size_t bytes) {
    CHECK(mask == mapped_mask && bytes == mapped_bytes);
    CHECK(munmap(mask, bytes) == 0);
    ++unmaps;
    mapped_mask = NULL;
    mapped_bytes = 0u;
    return 0;
}

static int probe_getaffinity(pid_t process, size_t bytes, cpu_set_t *cpus) {
    CHECK(process == 0);
    CHECK(bytes == (previous_bytes == 0u ? 1024u : 2u * previous_bytes));
    previous_bytes = bytes;
    ++attempts;
    if (fixture == UNAVAILABLE) { errno = ENOSYS; return -1; }
    if (fixture == UNSUPPORTED) { errno = ENOTSUP; return -1; }
    if (fixture == DENIED) { errno = EACCES; return -1; }
    if (fixture == FORBIDDEN) { errno = EPERM; return -1; }
    if (fixture == TOO_WIDE || bytes < required_bytes) {
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
#define mmap probe_mask_map
#define munmap probe_mask_unmap
#include "prim_host.c"
#undef sched_getaffinity
#undef sysconf
#undef open
#undef read
#undef close
#undef mmap
#undef munmap

static void reset(unsigned mode) {
    CHECK(mapped_mask == NULL);
    fixture = mode;
    attempts = mappings = unmaps = online_reads = capacity_reads = 0u;
    previous_bytes = 0u;
    int grow = mode == GROW || mode == TOO_WIDE
        || mode == MAPPING_REFUSED || mode == GROWTH_MAPPING_REFUSED;
    unsigned width = grow ? 8192u : CPU_SETSIZE;
    fixture_cpus[0] = 5u;
    fixture_cpus[1] = width + 17u;
    fixture_cpus[2] = 3u * width + 19u;
    required_bytes = grow ? 4096u : 1024u;
}

int main(void) {
    for (unsigned mode = FIT; mode <= GROW; ++mode) {
        reset(mode);
        CHECK(wf_prim_online_cpus() == 3u);
        CHECK(attempts == (mode == FIT ? 1u : 3u));
        CHECK(mappings == (mode == FIT ? 0u : 2u) && unmaps == mappings && online_reads == 0u);
        reset(mode);
        CHECK(wf_prim_cpu_levels() == 2u);
        CHECK(attempts == (mode == FIT ? 1u : 3u));
        CHECK(mappings == (mode == FIT ? 0u : 2u) && unmaps == mappings);
        CHECK(capacity_reads == 3u && online_reads == 0u);
    }
    for (unsigned mode = UNAVAILABLE; mode <= GROWTH_MAPPING_REFUSED; ++mode) {
        reset(mode);
        int unavailable = mode == UNAVAILABLE || mode == UNSUPPORTED;
        CHECK(wf_prim_online_cpus() == (unavailable ? 8192u : 0u));
        CHECK(online_reads == (unavailable ? 1u : 0u));
        CHECK(unmaps == mappings - (mode == MAPPING_REFUSED || mode == GROWTH_MAPPING_REFUSED));
        if (mode == TOO_WIDE) {
            CHECK(previous_bytes == CPU_ALLOC_SIZE(WF_PRIM_AFFINITY_CPU_LIMIT));
            CHECK(attempts > 3u);
        }
        reset(mode);
        CHECK(wf_prim_cpu_levels() == 1u && online_reads == 0u);
        CHECK(unmaps == mappings - (mode == MAPPING_REFUSED || mode == GROWTH_MAPPING_REFUSED));
    }
    /* The default worker query calls this same primitive, not a separate
     * topology path. Its ordinary entry configuration sees three lanes. */
    reset(GROW);
    CHECK(unsetenv("WF_WORKERS") == 0);
    CHECK(unsetenv("WF_SCHED_REPORT") == 0);
    CHECK(wf__sched_lanes() == 3);
    CHECK(attempts == 3u && unmaps == mappings && mappings == 2u && online_reads == 0u);
    return 0;
}
