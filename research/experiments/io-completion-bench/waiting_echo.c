/* glibc hides the POSIX and Linux entry points this file calls when the
 * translation unit is compiled as strict C11, so the namespace is asked for
 * before any header. */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif

/* The runtime shape of the proposed `waits` functions, written by hand in C so
 * it can be measured before any compiler work: the same contract as
 * `uring_echo` and `epoll_echo`.
 *
 *   waiting_echo PORT CONNECTIONS [--threads N] [--inline-receive]
 *
 * Every connection is served by its own waiting context, which runs ordinary
 * straight-line code on its own small stack:
 *
 *     loop { n = wait receive(connection, buffer); if n == 0 break;
 *            wait send_all(connection, buffer, n); }
 *
 * A `wait` fills one submission entry that names the context, switches to its
 * driver, and continues when the driver hands it the completion. Each driver
 * thread owns one io_uring ring, one SO_REUSEPORT listener, one accepting
 * context and the connection contexts that listener accepted; a context never
 * moves to another thread, so a completion is resumed on the thread that
 * reaped it with no cross-thread wake. Nothing on a driver thread is a compute
 * worker, and no compute worker ever waits.
 *
 * What this measures is the cost of that shape against the hand-written
 * completion state machine of `uring_echo`: one switch into and one out of a
 * context per wait, a per-connection buffer owned by straight-line code instead
 * of a kernel-provided buffer ring, and one single-shot receive per message
 * instead of a multishot one. Sends are tried once without blocking before
 * they go to the ring, the same inline lever the retired runtime measured;
 * `--inline-receive` tries receives the same way.
 *
 * Everything is sized from CONNECTIONS before the first accept: each driver
 * reserves one context slot per possible connection, and a slot's stack and
 * buffer are reserved address space that the kernel commits only as touched. */
#include <errno.h>
#include <linux/io_uring.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/eventfd.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <unistd.h>

#define RING_ENTRIES 4096u
#define STACK_BYTES (64u * 1024u)
#define GUARD_BYTES 4096u
#define BUFFER_BYTES 65536u
#define WAKE_TAG 1u

/* --- switching ----------------------------------------------------------- */

/* wf_switch(save, target) stores the current stack pointer in *save and
 * continues on the stack `target` was saved from. Only the callee-saved
 * registers are kept: the call itself tells the compiler every other register
 * is clobbered. */
void wf_switch(void **save, void *target);
#if defined(__x86_64__)
__asm__(".text\n"
        ".globl wf_switch\n"
        ".type wf_switch,@function\n"
        "wf_switch:\n"
        "  pushq %rbp\n"
        "  pushq %rbx\n"
        "  pushq %r12\n"
        "  pushq %r13\n"
        "  pushq %r14\n"
        "  pushq %r15\n"
        "  movq %rsp, (%rdi)\n"
        "  movq %rsi, %rsp\n"
        "  popq %r15\n"
        "  popq %r14\n"
        "  popq %r13\n"
        "  popq %r12\n"
        "  popq %rbx\n"
        "  popq %rbp\n"
        "  ret\n"
        ".size wf_switch,.-wf_switch\n");
#else
#error "waiting_echo switches stacks with x86-64 code only"
#endif

struct driver;

struct context {
    void *stack_pointer;
    struct driver *driver;
    void (*entry)(struct context *);
    struct context *next;
    unsigned char *stack;
    unsigned char *buffer;
    int descriptor;
    int result;
    int finished;
};

/* --- the ring (the same raw ABI as uring_echo) ---------------------------- */

struct ring {
    int descriptor;
    void *shared;
    size_t shared_bytes;
    struct io_uring_sqe *entries;
    size_t entry_bytes;
    unsigned *submission_head;
    unsigned *submission_tail;
    unsigned *submission_mask;
    unsigned *submission_array;
    unsigned *completion_head;
    unsigned *completion_tail;
    unsigned *completion_mask;
    struct io_uring_cqe *completions;
    unsigned local_tail;
    unsigned unsubmitted;
};

struct driver {
    pthread_t thread;
    int index;
    int listener;
    int wake;
    uint64_t wake_storage;
    struct ring ring;
    void *stack_pointer;
    struct context *current;
    struct context *ready_head;
    struct context *ready_tail;
    struct context *free_list;
    struct context *slots;
    unsigned slot_count;
    unsigned char *memory;
    size_t memory_bytes;
};

static uint64_t option_connections;
static uint16_t option_port;
static unsigned option_threads;
static int option_inline_receive;
static struct driver *drivers;
static _Atomic uint64_t accepted_total;
static _Atomic uint64_t closed_total;
static _Atomic int finished;
static _Atomic int failed;
static _Thread_local struct driver *this_driver;

static void report(const char *what, int error) {
    fprintf(stderr, "waiting_echo: %s: %s\n", what, strerror(error));
    fflush(stderr);
}

static void wake_everyone(void) {
    if (drivers == NULL) {
        return;
    }
    uint64_t one = 1;
    for (unsigned at = 0; at < option_threads; at++) {
        if (drivers[at].wake >= 0) {
            ssize_t written = write(drivers[at].wake, &one, sizeof one);
            (void)written;
        }
    }
}

static void mark_failed(void) {
    atomic_store_explicit(&failed, 1, memory_order_relaxed);
    wake_everyone();
}

static void check_finished(void) {
    if (atomic_load_explicit(&accepted_total, memory_order_relaxed) < option_connections) {
        return;
    }
    if (atomic_load_explicit(&closed_total, memory_order_relaxed) < option_connections) {
        return;
    }
    if (atomic_exchange_explicit(&finished, 1, memory_order_relaxed)) {
        return;
    }
    wake_everyone();
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

static int ring_setup(struct ring *ring, unsigned entries) {
    struct io_uring_params parameters;
    memset(&parameters, 0, sizeof parameters);
    /* One thread submits and waits, as in uring_echo. */
    parameters.flags |= IORING_SETUP_SINGLE_ISSUER | IORING_SETUP_DEFER_TASKRUN;
    long created = syscall(__NR_io_uring_setup, entries, &parameters);
    if (created < 0) {
        report("io_uring_setup", errno);
        return 1;
    }
    ring->descriptor = (int)created;
    if ((parameters.features & IORING_FEAT_SINGLE_MMAP) == 0) {
        fprintf(stderr, "waiting_echo: this kernel has no single-mmap rings\n");
        return 1;
    }
    size_t shared_bytes = parameters.sq_off.array + parameters.sq_entries * sizeof(unsigned);
    size_t completion_bytes =
        parameters.cq_off.cqes + parameters.cq_entries * sizeof(struct io_uring_cqe);
    if (completion_bytes > shared_bytes) {
        shared_bytes = completion_bytes;
    }
    void *map = mmap(NULL, shared_bytes, PROT_READ | PROT_WRITE, MAP_SHARED | MAP_POPULATE,
                     ring->descriptor, IORING_OFF_SQ_RING);
    if (map == MAP_FAILED) {
        report("mmap of the ring", errno);
        return 1;
    }
    ring->shared = map;
    ring->shared_bytes = shared_bytes;
    ring->entry_bytes = parameters.sq_entries * sizeof(struct io_uring_sqe);
    ring->entries = mmap(NULL, ring->entry_bytes, PROT_READ | PROT_WRITE,
                         MAP_SHARED | MAP_POPULATE, ring->descriptor, IORING_OFF_SQES);
    if (ring->entries == MAP_FAILED) {
        report("mmap of the submission entries", errno);
        return 1;
    }
    unsigned char *base = map;
    ring->submission_head = (unsigned *)(base + parameters.sq_off.head);
    ring->submission_tail = (unsigned *)(base + parameters.sq_off.tail);
    ring->submission_mask = (unsigned *)(base + parameters.sq_off.ring_mask);
    ring->submission_array = (unsigned *)(base + parameters.sq_off.array);
    ring->completion_head = (unsigned *)(base + parameters.cq_off.head);
    ring->completion_tail = (unsigned *)(base + parameters.cq_off.tail);
    ring->completion_mask = (unsigned *)(base + parameters.cq_off.ring_mask);
    ring->completions = (struct io_uring_cqe *)(base + parameters.cq_off.cqes);
    ring->unsubmitted = 0;
    ring->local_tail = 0;
    for (unsigned at = 0; at <= *ring->submission_mask; at++) {
        ring->submission_array[at] = at;
    }
    return 0;
}

static int ring_enter(struct ring *ring, unsigned wait_for) {
    unsigned flags = 0;
    unsigned to_submit = ring->unsubmitted;
    if (to_submit > 0) {
        atomic_store_explicit((_Atomic unsigned *)ring->submission_tail, ring->local_tail,
                              memory_order_release);
    }
    if (wait_for > 0) {
        flags |= IORING_ENTER_GETEVENTS;
    }
    if (to_submit == 0 && flags == 0) {
        return 0;
    }
    long entered = syscall(__NR_io_uring_enter, ring->descriptor, to_submit, wait_for, flags,
                           NULL, 0);
    if (entered < 0 && errno != EINTR && errno != EBUSY) {
        report("io_uring_enter", errno);
        return 1;
    }
    unsigned head =
        atomic_load_explicit((_Atomic unsigned *)ring->submission_head, memory_order_acquire);
    ring->unsubmitted = ring->local_tail - head;
    return 0;
}

static struct io_uring_sqe *ring_next(struct ring *ring) {
    for (;;) {
        unsigned head =
            atomic_load_explicit((_Atomic unsigned *)ring->submission_head, memory_order_acquire);
        unsigned mask = *ring->submission_mask;
        if ((ring->local_tail - head) <= mask) {
            struct io_uring_sqe *entry = &ring->entries[ring->local_tail & mask];
            memset(entry, 0, sizeof *entry);
            ring->local_tail++;
            ring->unsubmitted++;
            return entry;
        }
        if (ring_enter(ring, 0) != 0) {
            return NULL;
        }
    }
}

/* --- contexts ------------------------------------------------------------ */

static void make_ready(struct driver *driver, struct context *context) {
    context->next = NULL;
    if (driver->ready_tail == NULL) {
        driver->ready_head = context;
    } else {
        driver->ready_tail->next = context;
    }
    driver->ready_tail = context;
}

/* The first frame of every context. It never returns: when the entry is done
 * the context marks itself finished and gives its thread back to the driver,
 * which recycles the stack once it is no longer running on it. */
static void context_start(void) {
    struct driver *driver = this_driver;
    struct context *self = driver->current;
    self->entry(self);
    self->finished = 1;
    wf_switch(&self->stack_pointer, driver->stack_pointer);
    __builtin_unreachable();
}

static struct context *spawn(struct driver *driver, void (*entry)(struct context *),
                             int descriptor) {
    struct context *context = driver->free_list;
    if (context == NULL) {
        return NULL;
    }
    driver->free_list = context->next;
    context->driver = driver;
    context->entry = entry;
    context->descriptor = descriptor;
    context->finished = 0;
    /* The initial frame wf_switch returns into: six callee-saved registers,
     * then context_start as the return address, then a zero word so that
     * context_start begins with the stack alignment of an ordinary call. */
    uintptr_t *top = (uintptr_t *)(context->stack + STACK_BYTES);
    *--top = 0;
    *--top = (uintptr_t)context_start;
    for (int at = 0; at < 6; at++) {
        *--top = 0;
    }
    context->stack_pointer = top;
    make_ready(driver, context);
    return context;
}

/* The one waiting primitive: the entry is already filled, the context names
 * itself on it and gives its thread back to the driver until the completion
 * arrives. */
static int wait_for_entry(struct io_uring_sqe *entry) {
    struct driver *driver = this_driver;
    struct context *self = driver->current;
    entry->user_data = (uint64_t)(uintptr_t)self;
    wf_switch(&self->stack_pointer, driver->stack_pointer);
    return self->result;
}

static int wait_receive(int descriptor, unsigned char *buffer, unsigned length) {
    if (option_inline_receive) {
        ssize_t got = recv(descriptor, buffer, length, MSG_DONTWAIT);
        if (got >= 0) {
            return (int)got;
        }
        if (errno != EAGAIN && errno != EWOULDBLOCK && errno != EINTR) {
            return -errno;
        }
    }
    struct io_uring_sqe *entry = ring_next(&this_driver->ring);
    if (entry == NULL) {
        return -ENOMEM;
    }
    entry->opcode = IORING_OP_RECV;
    entry->fd = descriptor;
    entry->addr = (unsigned long long)(uintptr_t)buffer;
    entry->len = length;
    return wait_for_entry(entry);
}

static int wait_send(int descriptor, const unsigned char *buffer, unsigned length) {
    ssize_t moved = send(descriptor, buffer, length, MSG_DONTWAIT | MSG_NOSIGNAL);
    if (moved >= 0) {
        return (int)moved;
    }
    if (errno != EAGAIN && errno != EWOULDBLOCK && errno != EINTR) {
        return -errno;
    }
    struct io_uring_sqe *entry = ring_next(&this_driver->ring);
    if (entry == NULL) {
        return -ENOMEM;
    }
    entry->opcode = IORING_OP_SEND;
    entry->fd = descriptor;
    entry->addr = (unsigned long long)(uintptr_t)buffer;
    entry->len = length;
    entry->msg_flags = MSG_NOSIGNAL;
    return wait_for_entry(entry);
}

static int wait_accept(int listener) {
    struct io_uring_sqe *entry = ring_next(&this_driver->ring);
    if (entry == NULL) {
        return -ENOMEM;
    }
    entry->opcode = IORING_OP_ACCEPT;
    entry->fd = listener;
    return wait_for_entry(entry);
}

/* --- the program, written as straight-line waiting code ------------------ */

static void serve(struct context *self) {
    int descriptor = self->descriptor;
    unsigned char *buffer = self->buffer;
    for (;;) {
        int got = wait_receive(descriptor, buffer, BUFFER_BYTES);
        if (got == 0) {
            break;
        }
        if (got < 0) {
            report("receive", -got);
            mark_failed();
            break;
        }
        int sent = 0;
        while (sent < got) {
            int moved = wait_send(descriptor, buffer + sent, (unsigned)(got - sent));
            if (moved <= 0) {
                report("send", moved < 0 ? -moved : EPIPE);
                mark_failed();
                return;
            }
            sent += moved;
        }
    }
    close(descriptor);
    atomic_fetch_add_explicit(&closed_total, 1, memory_order_relaxed);
    check_finished();
}

static void accept_connections(struct context *self) {
    struct driver *driver = self->driver;
    while (atomic_load_explicit(&accepted_total, memory_order_relaxed) < option_connections) {
        int descriptor = wait_accept(driver->listener);
        if (descriptor < 0) {
            if (descriptor != -ECANCELED) {
                report("accept", -descriptor);
                mark_failed();
            }
            return;
        }
        atomic_fetch_add_explicit(&accepted_total, 1, memory_order_relaxed);
        if (spawn(driver, serve, descriptor) == NULL) {
            fprintf(stderr, "waiting_echo: no free context for a connection\n");
            mark_failed();
            return;
        }
    }
}

/* --- the driver ------------------------------------------------------------ */

static void arm_wake(struct driver *driver) {
    struct io_uring_sqe *entry = ring_next(&driver->ring);
    if (entry == NULL) {
        mark_failed();
        return;
    }
    entry->opcode = IORING_OP_READ;
    entry->fd = driver->wake;
    entry->addr = (unsigned long long)(uintptr_t)&driver->wake_storage;
    entry->len = sizeof driver->wake_storage;
    entry->user_data = WAKE_TAG;
}

static int driver_reserve(struct driver *driver) {
    /* One slot per possible connection plus the accepting context. Each slot
     * is a guard page, a stack and a receive buffer of reserved address space;
     * the kernel commits a page only when the context first touches it. */
    driver->slot_count = (unsigned)option_connections + 1;
    size_t slot_bytes = GUARD_BYTES + STACK_BYTES + BUFFER_BYTES;
    driver->memory_bytes = slot_bytes * driver->slot_count;
    driver->memory = mmap(NULL, driver->memory_bytes, PROT_READ | PROT_WRITE,
                          MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
    driver->slots = calloc(driver->slot_count, sizeof *driver->slots);
    if (driver->memory == MAP_FAILED || driver->slots == NULL) {
        report("reserving contexts", errno);
        return 1;
    }
    driver->free_list = NULL;
    for (unsigned at = driver->slot_count; at > 0; at--) {
        struct context *context = &driver->slots[at - 1];
        unsigned char *slot = driver->memory + slot_bytes * (at - 1);
        if (mprotect(slot, GUARD_BYTES, PROT_NONE) != 0) {
            report("guard page", errno);
            return 1;
        }
        context->stack = slot + GUARD_BYTES;
        context->buffer = slot + GUARD_BYTES + STACK_BYTES;
        context->next = driver->free_list;
        driver->free_list = context;
    }
    return 0;
}

static void *driver_main(void *raw) {
    struct driver *driver = raw;
    this_driver = driver;
    if (ring_setup(&driver->ring, RING_ENTRIES) != 0 || driver_reserve(driver) != 0) {
        mark_failed();
        return NULL;
    }
    arm_wake(driver);
    if (spawn(driver, accept_connections, -1) == NULL) {
        mark_failed();
        return NULL;
    }
    while (!atomic_load_explicit(&finished, memory_order_relaxed) &&
           !atomic_load_explicit(&failed, memory_order_relaxed)) {
        /* Run every context that has something to continue with, each until it
         * waits or ends. */
        while (driver->ready_head != NULL) {
            struct context *context = driver->ready_head;
            driver->ready_head = context->next;
            if (driver->ready_head == NULL) {
                driver->ready_tail = NULL;
            }
            driver->current = context;
            wf_switch(&driver->stack_pointer, context->stack_pointer);
            driver->current = NULL;
            if (context->finished) {
                context->next = driver->free_list;
                driver->free_list = context;
            }
        }
        if (atomic_load_explicit(&finished, memory_order_relaxed) ||
            atomic_load_explicit(&failed, memory_order_relaxed)) {
            break;
        }
        /* Submit what the contexts asked for and wait for at least one
         * completion; then hand every completion to the context it names. */
        if (ring_enter(&driver->ring, 1) != 0) {
            mark_failed();
            break;
        }
        unsigned head = atomic_load_explicit((_Atomic unsigned *)driver->ring.completion_head,
                                             memory_order_relaxed);
        unsigned tail = atomic_load_explicit((_Atomic unsigned *)driver->ring.completion_tail,
                                             memory_order_acquire);
        while (head != tail) {
            struct io_uring_cqe *completion =
                &driver->ring.completions[head & *driver->ring.completion_mask];
            uint64_t tag = completion->user_data;
            int result = completion->res;
            head++;
            if (tag == WAKE_TAG) {
                if (!atomic_load_explicit(&finished, memory_order_relaxed)) {
                    arm_wake(driver);
                }
                continue;
            }
            struct context *context = (struct context *)(uintptr_t)tag;
            context->result = result;
            make_ready(driver, context);
        }
        atomic_store_explicit((_Atomic unsigned *)driver->ring.completion_head, head,
                              memory_order_release);
    }
    return NULL;
}

static int listener_for(uint16_t port) {
    int descriptor = socket(AF_INET, SOCK_STREAM, 0);
    if (descriptor < 0) {
        report("socket", errno);
        return -1;
    }
    int one = 1;
    if (setsockopt(descriptor, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one) != 0 ||
        setsockopt(descriptor, SOL_SOCKET, SO_REUSEPORT, &one, sizeof one) != 0) {
        report("setsockopt", errno);
        close(descriptor);
        return -1;
    }
    setsockopt(descriptor, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
    struct sockaddr_in address;
    memset(&address, 0, sizeof address);
    address.sin_family = AF_INET;
    address.sin_port = htons(port);
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (bind(descriptor, (struct sockaddr *)&address, sizeof address) != 0) {
        report("bind", errno);
        close(descriptor);
        return -1;
    }
    if (listen(descriptor, 4096) != 0) {
        report("listen", errno);
        close(descriptor);
        return -1;
    }
    return descriptor;
}

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: waiting_echo PORT CONNECTIONS [--threads N] [--inline-receive]\n");
        return 2;
    }
    unsigned long port = strtoul(argv[1], NULL, 10);
    option_connections = strtoull(argv[2], NULL, 10);
    long online = sysconf(_SC_NPROCESSORS_ONLN);
    option_threads = online > 0 ? (unsigned)online : 1u;
    for (int at = 3; at < argc; at++) {
        if (strcmp(argv[at], "--threads") == 0 && at + 1 < argc) {
            option_threads = (unsigned)strtoul(argv[++at], NULL, 10);
            continue;
        }
        if (strcmp(argv[at], "--inline-receive") == 0) {
            option_inline_receive = 1;
            continue;
        }
        fprintf(stderr, "waiting_echo: unknown argument %s\n", argv[at]);
        return 2;
    }
    if (port == 0 || port > 65535 || option_connections == 0 || option_threads == 0) {
        fprintf(stderr, "waiting_echo: PORT, CONNECTIONS and --threads must be positive\n");
        return 2;
    }
    option_port = (uint16_t)port;
    raise_descriptor_limit(option_connections);
    drivers = calloc(option_threads, sizeof *drivers);
    if (drivers == NULL) {
        report("calloc", errno);
        return 1;
    }
    for (unsigned at = 0; at < option_threads; at++) {
        drivers[at].index = (int)at;
        drivers[at].listener = listener_for(option_port);
        drivers[at].wake = eventfd(0, EFD_CLOEXEC);
        if (drivers[at].listener < 0 || drivers[at].wake < 0) {
            return 1;
        }
    }
    for (unsigned at = 0; at < option_threads; at++) {
        if (pthread_create(&drivers[at].thread, NULL, driver_main, &drivers[at]) != 0) {
            report("pthread_create", errno);
            return 1;
        }
    }
    for (unsigned at = 0; at < option_threads; at++) {
        pthread_join(drivers[at].thread, NULL);
    }
    return atomic_load_explicit(&failed, memory_order_relaxed) ? 1 : 0;
}
