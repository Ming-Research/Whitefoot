/* glibc hides the POSIX and Linux entry points this file calls when the
 * translation unit is compiled as strict C11, so the namespace is asked for
 * before any header. */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif

/* The one load generator every server of the TCP echo workload is measured
 * with.
 *
 *   netload PORT CONNECTIONS ROUNDTRIPS BYTES [--threads T]
 *
 * It opens CONNECTIONS connections to 127.0.0.1:PORT, spread over T client
 * threads each holding its own epoll instance, and reports two phases:
 *
 *   the connect phase   from the first connect call until every connection is
 *                       established, which is what "connections accepted per
 *                       second" is measured from
 *   the exchange phase  every connection performing ROUNDTRIPS round trips of
 *                       a BYTES-byte message, all connections active at once,
 *                       which is what round trips per second, bytes per second
 *                       and the latency distribution are measured from
 *
 * Every echoed byte is compared with the byte that was sent. A run that
 * receives anything else, that is refused a connection, or whose peer closes
 * mid-exchange prints one line on stderr and exits nonzero; it never reports a
 * time. That is what makes the three server lines comparable: the generator
 * cannot tell them apart, and none of them can buy a number by publishing the
 * wrong bytes.
 *
 * Allocation is done once, before the first connect: the per-connection send
 * and receive buffers, and one array of CONNECTIONS*ROUNDTRIPS 32-bit
 * microsecond latencies. Nothing is allocated per round trip.
 *
 * One line on stdout, tab separated:
 *
 *   connect_us=  exchange_us=  roundtrips=  rt_per_s=  bytes_per_s=
 *   p50_us=  p90_us=  p99_us=  max_us=  client_busy_max_pct=
 *
 * client_busy_max_pct is the largest share of the exchange phase, as
 * /proc/stat counts it, that any one CPU this process may run on spent doing
 * anything but idle or waiting for I/O. A client whose busiest CPU is nearly
 * saturated limits the rate it reports, so a table that compares servers
 * checks it; -1 means the counters could not be read.
 *
 * bytes_per_s counts the payload once, not once per direction: a round trip of
 * BYTES bytes counts BYTES. */
#include <errno.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <pthread.h>
#include <sched.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/epoll.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

struct connection {
    int descriptor;
    int established;
    int finished;
    unsigned char *outgoing;
    unsigned char *incoming;
    uint64_t sent;
    uint64_t received;
    uint64_t round;
    struct timespec started;
    uint64_t index;
};

struct client {
    pthread_t thread;
    int epoll;
    struct connection *first;
    uint64_t count;
    uint64_t outstanding;
};

static uint64_t option_connections;
static uint64_t option_roundtrips;
static uint64_t option_bytes;
static uint16_t option_port;
static uint32_t *latency_us;
static struct connection *connections;
static struct client *clients;
static pthread_barrier_t gate;

/* One line on stderr and out, from whichever thread reached the failure
 * first. A run that cannot publish the right bytes must not publish a time,
 * and the other threads have nothing left to do. */
static pthread_mutex_t failure_lock = PTHREAD_MUTEX_INITIALIZER;

static void fail(const char *format, ...) {
    pthread_mutex_lock(&failure_lock);
    va_list arguments;
    va_start(arguments, format);
    fprintf(stderr, "netload: ");
    vfprintf(stderr, format, arguments);
    fprintf(stderr, "\n");
    va_end(arguments);
    fflush(stderr);
    _exit(1);
}

static double seconds_between(struct timespec start, struct timespec end) {
    return (double)(end.tv_sec - start.tv_sec) + (double)(end.tv_nsec - start.tv_nsec) / 1e9;
}

static uint64_t microseconds_between(struct timespec start, struct timespec end) {
    double value = seconds_between(start, end) * 1e6;
    if (value < 0.0) {
        return 0;
    }
    return (uint64_t)value;
}

static void raise_descriptor_limit(uint64_t wanted) {
    struct rlimit limit;
    if (getrlimit(RLIMIT_NOFILE, &limit) != 0) {
        return;
    }
    if (limit.rlim_cur >= wanted + 64) {
        return;
    }
    rlim_t target = (rlim_t)(wanted + 64);
    if (limit.rlim_max != RLIM_INFINITY && target > limit.rlim_max) {
        target = limit.rlim_max;
    }
    limit.rlim_cur = target;
    setrlimit(RLIMIT_NOFILE, &limit);
}

static int compare_u32(const void *left, const void *right) {
    uint32_t a = *(const uint32_t *)left;
    uint32_t b = *(const uint32_t *)right;
    return (a > b) - (a < b);
}

/* Nearest-rank percentile over the sorted samples. */
static uint32_t percentile(const uint32_t *sorted, uint64_t count, uint64_t share) {
    if (count == 0) {
        return 0;
    }
    uint64_t rank = (count * share + 99) / 100;
    if (rank == 0) {
        rank = 1;
    }
    if (rank > count) {
        rank = count;
    }
    return sorted[rank - 1];
}

/* The message a connection sends on every round. The payload depends on the
 * connection and on the position within the message, so a server that echoed
 * one connection's bytes to another, or that reordered a message, fails the
 * comparison rather than producing a fast time. The first byte carries the
 * round, so a stale echo cannot pass either. */
static void fill_message(struct connection *link) {
    for (uint64_t at = 0; at < option_bytes; at++) {
        link->outgoing[at] = (unsigned char)(link->index * 131u + at * 7u + 11u);
    }
}

static void begin_round(struct connection *link) {
    link->sent = 0;
    link->received = 0;
    link->outgoing[0] = (unsigned char)(link->round * 17u + 3u);
    clock_gettime(CLOCK_MONOTONIC, &link->started);
}

/* Drives one connection as far as it will go without blocking: send what is
 * left of the current message, receive what is left of its echo, verify it,
 * record the round trip, and start the next one. Returns when the kernel has
 * no more room to send or no more bytes to give. */
static void pump(struct client *owner, struct connection *link) {
    for (;;) {
        while (link->sent < option_bytes) {
            ssize_t moved = send(link->descriptor, link->outgoing + link->sent,
                                 (size_t)(option_bytes - link->sent), MSG_NOSIGNAL);
            if (moved > 0) {
                link->sent += (uint64_t)moved;
                continue;
            }
            if (moved < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) {
                break;
            }
            fail("connection %llu: send failed after %llu of %llu bytes: %s",
                 (unsigned long long)link->index, (unsigned long long)link->sent,
                 (unsigned long long)option_bytes, strerror(errno));
        }
        while (link->received < option_bytes) {
            ssize_t moved = recv(link->descriptor, link->incoming + link->received,
                                 (size_t)(option_bytes - link->received), 0);
            if (moved > 0) {
                link->received += (uint64_t)moved;
                continue;
            }
            if (moved == 0) {
                fail("connection %llu: the peer closed after echoing %llu of %llu bytes of round %llu",
                     (unsigned long long)link->index, (unsigned long long)link->received,
                     (unsigned long long)option_bytes, (unsigned long long)link->round);
            }
            if (errno == EAGAIN || errno == EWOULDBLOCK) {
                return;
            }
            fail("connection %llu: receive failed: %s", (unsigned long long)link->index,
                 strerror(errno));
        }
        if (memcmp(link->incoming, link->outgoing, (size_t)option_bytes) != 0) {
            uint64_t at = 0;
            while (at < option_bytes && link->incoming[at] == link->outgoing[at]) {
                at++;
            }
            fail("connection %llu round %llu: byte %llu is 0x%02x, expected 0x%02x",
                 (unsigned long long)link->index, (unsigned long long)link->round,
                 (unsigned long long)at, link->incoming[at], link->outgoing[at]);
        }
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        uint64_t elapsed = microseconds_between(link->started, now);
        latency_us[link->index * option_roundtrips + link->round] =
            elapsed > UINT32_MAX ? UINT32_MAX : (uint32_t)elapsed;
        link->round++;
        if (link->round == option_roundtrips) {
            link->finished = 1;
            owner->outstanding--;
            return;
        }
        begin_round(link);
    }
}

static void *client_main(void *raw) {
    struct client *owner = raw;
    struct epoll_event events[256];

    /* Phase one: every socket is created before any connect call, so the
     * connect phase measures connects and not socket creation. */
    for (uint64_t at = 0; at < owner->count; at++) {
        struct connection *link = &owner->first[at];
        link->descriptor = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
        if (link->descriptor < 0) {
            fail("socket: %s", strerror(errno));
        }
        int one = 1;
        setsockopt(link->descriptor, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
    }

    pthread_barrier_wait(&gate);

    struct sockaddr_in address;
    memset(&address, 0, sizeof address);
    address.sin_family = AF_INET;
    address.sin_port = htons(option_port);
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    uint64_t pending = 0;
    for (uint64_t at = 0; at < owner->count; at++) {
        struct connection *link = &owner->first[at];
        int outcome = connect(link->descriptor, (struct sockaddr *)&address, sizeof address);
        if (outcome == 0) {
            link->established = 1;
            continue;
        }
        if (errno != EINPROGRESS) {
            fail("connection %llu: connect refused: %s", (unsigned long long)link->index,
                 strerror(errno));
        }
        struct epoll_event registration;
        registration.events = EPOLLOUT;
        registration.data.ptr = link;
        if (epoll_ctl(owner->epoll, EPOLL_CTL_ADD, link->descriptor, &registration) != 0) {
            fail("epoll_ctl on connect: %s", strerror(errno));
        }
        pending++;
    }
    while (pending > 0) {
        int ready = epoll_wait(owner->epoll, events, 256, -1);
        if (ready < 0) {
            if (errno == EINTR) {
                continue;
            }
            fail("epoll_wait on connect: %s", strerror(errno));
        }
        for (int at = 0; at < ready; at++) {
            struct connection *link = events[at].data.ptr;
            int problem = 0;
            socklen_t size = sizeof problem;
            if (getsockopt(link->descriptor, SOL_SOCKET, SO_ERROR, &problem, &size) != 0) {
                fail("connection %llu: getsockopt: %s", (unsigned long long)link->index,
                     strerror(errno));
            }
            if (problem != 0) {
                fail("connection %llu: connect refused: %s", (unsigned long long)link->index,
                     strerror(problem));
            }
            link->established = 1;
            epoll_ctl(owner->epoll, EPOLL_CTL_DEL, link->descriptor, NULL);
            pending--;
        }
    }

    pthread_barrier_wait(&gate);

    /* Phase two: every connection is armed edge triggered for both directions
     * and starts its first round, so all of them are in flight at once. */
    for (uint64_t at = 0; at < owner->count; at++) {
        struct connection *link = &owner->first[at];
        struct epoll_event registration;
        registration.events = EPOLLIN | EPOLLOUT | EPOLLET;
        registration.data.ptr = link;
        if (epoll_ctl(owner->epoll, EPOLL_CTL_ADD, link->descriptor, &registration) != 0) {
            fail("epoll_ctl on exchange: %s", strerror(errno));
        }
    }
    for (uint64_t at = 0; at < owner->count; at++) {
        struct connection *link = &owner->first[at];
        begin_round(link);
        pump(owner, link);
    }
    while (owner->outstanding > 0) {
        int ready = epoll_wait(owner->epoll, events, 256, -1);
        if (ready < 0) {
            if (errno == EINTR) {
                continue;
            }
            fail("epoll_wait on exchange: %s", strerror(errno));
        }
        for (int at = 0; at < ready; at++) {
            struct connection *link = events[at].data.ptr;
            if (link->finished) {
                continue;
            }
            pump(owner, link);
        }
    }

    pthread_barrier_wait(&gate);

    for (uint64_t at = 0; at < owner->count; at++) {
        close(owner->first[at].descriptor);
    }
    return NULL;
}

/* The jiffies one CPU spent busy and in all, from /proc/stat, for every CPU
 * this process may run on. */
#define CPU_SLOTS 1024
struct cpu_counters {
    uint64_t busy[CPU_SLOTS];
    uint64_t total[CPU_SLOTS];
    int allowed[CPU_SLOTS];
    int readable;
};

static void read_cpu_counters(struct cpu_counters *into, int fill_allowed) {
    if (fill_allowed) {
        cpu_set_t mask;
        memset(into->allowed, 0, sizeof into->allowed);
        if (sched_getaffinity(0, sizeof mask, &mask) == 0) {
            for (int cpu = 0; cpu < CPU_SLOTS && cpu < CPU_SETSIZE; cpu++) {
                into->allowed[cpu] = CPU_ISSET(cpu, &mask) ? 1 : 0;
            }
        }
    }
    into->readable = 0;
    FILE *stat = fopen("/proc/stat", "r");
    if (stat == NULL) {
        return;
    }
    char line[512];
    while (fgets(line, sizeof line, stat) != NULL) {
        int cpu = -1;
        unsigned long long user_time = 0, nice_time = 0, system_time = 0, idle = 0, iowait = 0;
        unsigned long long irq = 0, softirq = 0, steal = 0;
        if (strncmp(line, "cpu", 3) != 0 || line[3] < '0' || line[3] > '9') {
            continue;
        }
        if (sscanf(line, "cpu%d %llu %llu %llu %llu %llu %llu %llu %llu", &cpu, &user_time,
                &nice_time, &system_time, &idle, &iowait, &irq, &softirq, &steal) < 5 ||
            cpu < 0 || cpu >= CPU_SLOTS) {
            continue;
        }
        uint64_t all = user_time + nice_time + system_time + idle + iowait + irq + softirq + steal;
        into->total[cpu] = all;
        into->busy[cpu] = all - idle - iowait;
        into->readable = 1;
    }
    fclose(stat);
}

/* The busiest allowed CPU's busy share between two readings, in percent, or
 * -1 when a reading failed or no allowed CPU advanced. */
static double busiest_share(const struct cpu_counters *before, const struct cpu_counters *after) {
    if (!before->readable || !after->readable) {
        return -1.0;
    }
    double most = -1.0;
    for (int cpu = 0; cpu < CPU_SLOTS; cpu++) {
        if (!before->allowed[cpu] || after->total[cpu] <= before->total[cpu]) {
            continue;
        }
        double share = 100.0 * (double)(after->busy[cpu] - before->busy[cpu]) /
                       (double)(after->total[cpu] - before->total[cpu]);
        if (share > most) {
            most = share;
        }
    }
    return most;
}

int main(int argc, char **argv) {
    if (argc < 5) {
        fprintf(stderr, "usage: netload PORT CONNECTIONS ROUNDTRIPS BYTES [--threads T]\n");
        return 2;
    }
    unsigned long port = strtoul(argv[1], NULL, 10);
    option_connections = strtoull(argv[2], NULL, 10);
    option_roundtrips = strtoull(argv[3], NULL, 10);
    option_bytes = strtoull(argv[4], NULL, 10);
    long online = sysconf(_SC_NPROCESSORS_ONLN);
    uint64_t threads = online > 0 ? (uint64_t)online : 1;
    for (int at = 5; at < argc; at++) {
        if (strcmp(argv[at], "--threads") == 0 && at + 1 < argc) {
            threads = strtoull(argv[++at], NULL, 10);
            continue;
        }
        fprintf(stderr, "netload: unknown argument %s\n", argv[at]);
        return 2;
    }
    if (port == 0 || port > 65535 || option_connections == 0 || option_roundtrips == 0 ||
        option_bytes == 0 || threads == 0) {
        fprintf(stderr, "netload: PORT, CONNECTIONS, ROUNDTRIPS, BYTES and T must be positive\n");
        return 2;
    }
    option_port = (uint16_t)port;
    if (threads > option_connections) {
        threads = option_connections;
    }
    raise_descriptor_limit(option_connections);

    connections = calloc((size_t)option_connections, sizeof *connections);
    clients = calloc((size_t)threads, sizeof *clients);
    latency_us = calloc((size_t)(option_connections * option_roundtrips), sizeof *latency_us);
    unsigned char *payloads = malloc((size_t)option_connections * (size_t)option_bytes * 2);
    if (connections == NULL || clients == NULL || latency_us == NULL || payloads == NULL) {
        fprintf(stderr, "netload: out of memory\n");
        return 1;
    }
    for (uint64_t at = 0; at < option_connections; at++) {
        struct connection *link = &connections[at];
        link->index = at;
        link->descriptor = -1;
        link->outgoing = payloads + (size_t)(at * 2) * (size_t)option_bytes;
        link->incoming = link->outgoing + option_bytes;
        fill_message(link);
    }

    /* The connections are spread over the client threads in equal blocks; the
     * first `remainder` threads carry one more. */
    uint64_t block = option_connections / threads;
    uint64_t remainder = option_connections % threads;
    uint64_t handed = 0;
    for (uint64_t at = 0; at < threads; at++) {
        struct client *owner = &clients[at];
        owner->count = block + (at < remainder ? 1 : 0);
        owner->first = &connections[handed];
        owner->outstanding = owner->count;
        handed += owner->count;
        owner->epoll = epoll_create1(0);
        if (owner->epoll < 0) {
            fprintf(stderr, "netload: epoll_create1: %s\n", strerror(errno));
            return 1;
        }
    }

    pthread_barrier_init(&gate, NULL, (unsigned)threads + 1);
    for (uint64_t at = 0; at < threads; at++) {
        if (pthread_create(&clients[at].thread, NULL, client_main, &clients[at]) != 0) {
            fprintf(stderr, "netload: pthread_create failed\n");
            return 1;
        }
    }

    struct timespec connect_start;
    struct timespec connect_end;
    struct timespec exchange_end;
    static struct cpu_counters cpus_at_connect_end;
    static struct cpu_counters cpus_at_exchange_end;
    pthread_barrier_wait(&gate);
    clock_gettime(CLOCK_MONOTONIC, &connect_start);
    pthread_barrier_wait(&gate);
    clock_gettime(CLOCK_MONOTONIC, &connect_end);
    read_cpu_counters(&cpus_at_connect_end, 1);
    pthread_barrier_wait(&gate);
    clock_gettime(CLOCK_MONOTONIC, &exchange_end);
    read_cpu_counters(&cpus_at_exchange_end, 0);
    for (uint64_t at = 0; at < threads; at++) {
        pthread_join(clients[at].thread, NULL);
    }
    pthread_barrier_destroy(&gate);

    uint64_t total = option_connections * option_roundtrips;
    qsort(latency_us, (size_t)total, sizeof *latency_us, compare_u32);
    uint64_t connect_us = microseconds_between(connect_start, connect_end);
    double exchange_seconds = seconds_between(connect_end, exchange_end);
    uint64_t exchange_us = microseconds_between(connect_end, exchange_end);
    double per_second = exchange_seconds > 0.0 ? (double)total / exchange_seconds : 0.0;
    double bytes_per_second =
        exchange_seconds > 0.0 ? (double)total * (double)option_bytes / exchange_seconds : 0.0;
    printf("connect_us=%llu\texchange_us=%llu\troundtrips=%llu\trt_per_s=%.1f"
           "\tbytes_per_s=%.1f\tp50_us=%u\tp90_us=%u\tp99_us=%u\tmax_us=%u"
           "\tclient_busy_max_pct=%.1f\n",
           (unsigned long long)connect_us, (unsigned long long)exchange_us,
           (unsigned long long)total, per_second, bytes_per_second,
           percentile(latency_us, total, 50), percentile(latency_us, total, 90),
           percentile(latency_us, total, 99), total > 0 ? latency_us[total - 1] : 0,
           busiest_share(&cpus_at_connect_end, &cpus_at_exchange_end));
    fflush(stdout);
    free(payloads);
    free(latency_us);
    free(clients);
    free(connections);
    return 0;
}
