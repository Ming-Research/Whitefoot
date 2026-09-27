/* The resource-exhaustion floor, linked into every Whitefoot program.
 *
 * Two jobs, both about the one abnormal end a *correct* program can reach.
 *
 * The first is that the ceiling should be the compiler's number rather than
 * the environment's. A program's depth limit was whatever `ulimit -s` happened
 * to leave it, so the same binary on the same input succeeded on one shell and
 * died on another. `wf__floor_run` runs the entry on a stack this file sizes —
 * an ordinary thread with the declared reservation — so the limit travels with the
 * program.
 *
 * The second is that running out should be a defined, reported abort instead
 * of a bare host signal. A guard-page hit is converted into one fixed record
 * on standard error followed by `abort`; every other fault is put back exactly
 * as it was, so a genuine memory defect keeps its own signal, its own exit
 * status, and its core dump. A diagnostic that made a wild pointer and an
 * exhausted stack look alike would be worse than none, because it would
 * misdirect the reader.
 *
 * Everything below the handler boundary is async-signal-safe: no allocation,
 * no stdio, no locks, and no pthread queries. The stack bounds the handler
 * reads are captured outside signal context once when each thread attaches.
 */

#if !defined(__APPLE__)
#define _GNU_SOURCE
#endif

#include <errno.h>
#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <unistd.h>

/* The entry body the emitted module defines. `wf__floor_run` is the only
 * caller, and the module's own weak fallback calls it directly when this
 * translation unit is not linked. */
extern int wf__main_body(int argc, char **argv);

/* ------------------------------------------------------------ the ceiling */

/* The stack the entry runs on.
 *
 * One deliberate number, chosen by the compiler rather than inherited from the
 * environment. It is a reservation, not a commitment: the pages are populated
 * as the program actually descends, so a program that never recurses deeply
 * pays no resident memory for the headroom and no time to reserve it.
 *
 * Raising it costs address space and nothing else; the reason not to raise it
 * further is that a runaway recursion should still end in a bounded time. */
#define WF_FLOOR_STACK_BYTES ((size_t)1024u * 1024u * 1024u)

/* Command and worker stacks use the same reservation. Nested helping uses
 * the caller's remaining stack; the exhaustion floor applies there too. */
size_t wf__floor_stack_bytes(void) { return WF_FLOOR_STACK_BYTES; }

/* ------------------------------------------------------- per-thread state */

/* Captured once per ordinary thread, before executing WF code. */
static _Thread_local unsigned long wf__floor_stack_low;
static _Thread_local unsigned long wf__floor_stack_high;
static _Thread_local int wf__floor_stack_known;

/* The alternate stack the handler runs on, because the ordinary one is exactly
 * what has just run out. One mapping per thread, populated on first use. */
#define WF_FLOOR_ALTSTACK_BYTES ((size_t)64u * 1024u)

/* How far below the usable stack a fault still counts as this thread running
 * out rather than as a wild pointer.
 *
 * This is the probe's geometry, not a margin of comfort. Every generated
 * definition carries the target's `probe-stack` attribute, so a frame walks
 * its pages on the way down in strides of one page: the first touch below the
 * stack is at most one stride under it, and no descent can step past the guard
 * page into whatever lies beyond. Below that a leaf may touch its ABI red zone
 * without moving the stack pointer at all, which is 128 bytes on x86-64 and
 * none on AArch64. One stride plus the red zone is therefore the whole set of
 * addresses a thread running out can fault at.
 *
 * Sizing it any wider is not free slack: every extra byte is a range of wild
 * faults reported as exhaustion, which is exactly the misdirection this file
 * exists to avoid. The band was 1 MiB and ate real corruption faults up to
 * roughly 128,000 times further from the stack than a legitimate overflow
 * lands.
 *
 * The stride is the host's page size, read once outside signal context because
 * `sysconf` is not async-signal-safe. Setup must establish it before any WF
 * code runs; an unavailable page size cannot silently narrow the floor. */
#define WF_FLOOR_RED_ZONE_BYTES ((unsigned long)128u)
static volatile unsigned long wf__floor_guard_band;

/* ------------------------------------------------------------- the record */

/* The bytes an exhausted execution writes before aborting.
 *
 * Fixed by the resource class alone. Two independent constraints force that
 * and agree on it: a signal handler may reach only async-signal-safe
 * facilities, which admits a constant and a raw `write` and essentially
 * nothing more; and [PAR-1] requires a program's observables to be identical
 * under every permitted schedule, so the record may not name a worker, a
 * thread, a depth, or an address.
 *
 * The absent fields are the point. It carries no `rule_id`, no function, and
 * no node path: exhaustion violates no source obligation, so the record names
 * only the unavailable external resource. */
static const char WF_FLOOR_STACK_RECORD[] = "{\"resource\":\"stack\"}\n";

/* One latch for every resource record any thread of this process can write.
 *
 * It has to be one rather than one per writer. The signal handler below writes
 * the stack record; an emitted module that allocates writes the heap record.
 * Those are different threads reaching different resource limits, so a latch
 * per writer leaves them unserialized against each other and two records can
 * interleave on the same channel — which is exactly what "exactly one record"
 * is supposed to rule out. The module asks for the address rather than keeping
 * its own, and a module linked without this unit falls back to one of its own. */
static volatile int wf__floor_latch;

volatile int *wf__floor_record_latch(void) { return &wf__floor_latch; }

static void wf__floor_emit(const char *bytes, size_t length) {
    while (length > 0) {
        ssize_t written = write(2, bytes, length);
        if (written <= 0) {
            if (written < 0 && errno == EINTR) {
                continue;
            }
            return;
        }
        bytes += (size_t)written;
        length -= (size_t)written;
    }
}

/* A failed prerequisite is a runtime-start failure, not a classified stack
 * overflow. Stop before executing a program without its exhaustion floor. */
static _Noreturn void wf__floor_setup_failed(void) {
    static const char record[] =
        "whitefoot floor: stack exhaustion protection could not be installed\n";
    wf__floor_emit(record, sizeof(record) - 1);
    abort();
}

/* ------------------------------------------------------------ the handler */

static void wf__floor_handler(int signo, siginfo_t *info, void *context) {
    unsigned long fault = (unsigned long)(uintptr_t)(info ? info->si_addr : 0);
    int guard_hit = 0;
    (void)context;

    if (wf__floor_stack_known) {
        guard_hit = (fault < wf__floor_stack_high)
                    && (fault + wf__floor_guard_band >= wf__floor_stack_low);
    }

    if (!guard_hit) {
        /* Not this mechanism's fault class. Put the default disposition back
         * and deliver the signal under it, so the process dies exactly as it
         * would have without this file: same signal, same status, same core
         * dump. The floor adds nothing here and hides nothing.
         *
         * The re-raise is what makes that true for a signal that has no
         * faulting instruction to re-execute. An externally delivered SIGBUS
         * or SIGSEGV reaches this path too — on this host it arrives with a
         * null `si_addr`, indistinguishable from a null dereference — and
         * simply returning would swallow it *and* leave the disposition at
         * SIG_DFL process-wide, so every later thread's overflow would arrive
         * as a bare host signal with zero bytes. The restore is per-signal and
         * process-wide while the classification above is per-thread, so the
         * only safe thing to do after restoring is to make sure this process
         * does not outlive it. Raising while the signal is still blocked
         * queues it for delivery on return, with this thread's faulting
         * context intact. */
        struct sigaction restore;
        memset(&restore, 0, sizeof(restore));
        restore.sa_handler = SIG_DFL;
        sigemptyset(&restore.sa_mask);
        sigaction(signo, &restore, NULL);
        pthread_kill(pthread_self(), signo);
        return;
    }

    if (__sync_bool_compare_and_swap(&wf__floor_latch, 0, 1)) {
        wf__floor_emit(WF_FLOOR_STACK_RECORD, sizeof(WF_FLOOR_STACK_RECORD) - 1);
        abort();
    }
    /* A second faulting thread parks rather than racing the writer; the
     * winner's abort takes the process down underneath it. */
    for (;;) {
        pause();
    }
}

/* -------------------------------------------------------------- installing */

/* Captures the calling thread's stack bounds. Called on the thread itself and
 * outside any signal context, so the ordinary pthread queries are available. */
static int wf__floor_capture_bounds(void) {
#if defined(__APPLE__)
    /* Darwin reports the high address — one past the top — and the size. */
    void *top = pthread_get_stackaddr_np(pthread_self());
    size_t size = pthread_get_stacksize_np(pthread_self());
    if (top == NULL || size == 0 || size > (uintptr_t)top) {
        return 0;
    }
    wf__floor_stack_high = (unsigned long)(uintptr_t)top;
    wf__floor_stack_low = wf__floor_stack_high - (unsigned long)size;
    wf__floor_stack_known = 1;
#else
    pthread_attr_t attributes;
    void *base = NULL;
    size_t size = 0;
    int error;
    if (pthread_getattr_np(pthread_self(), &attributes) != 0) {
        return 0;
    }
    error = pthread_attr_getstack(&attributes, &base, &size);
    /* Unconditional: the query can succeed and the read still fail, and the
     * attribute object is owned either way. */
    pthread_attr_destroy(&attributes);
    if (error != 0 || base == NULL || size == 0 || size > UINTPTR_MAX - (uintptr_t)base) {
        return 0;
    }
    wf__floor_stack_low = (unsigned long)(uintptr_t)base;
    wf__floor_stack_high = wf__floor_stack_low + (unsigned long)size;
    wf__floor_stack_known = 1;
#endif
    return 1;
}

/* Every WF thread attaches once: capture its stack bounds and reserve the
 * alternate signal stack used to report exhaustion. */
void wf__floor_attach_thread(void) {
    stack_t alternate;
    void *memory;
    if (!wf__floor_capture_bounds()) {
        wf__floor_setup_failed();
    }
    memory = mmap(NULL, WF_FLOOR_ALTSTACK_BYTES, PROT_READ | PROT_WRITE,
                  MAP_PRIVATE | MAP_ANON, -1, 0);
    if (memory == MAP_FAILED) {
        wf__floor_setup_failed();
    }
    alternate.ss_sp = memory;
    alternate.ss_size = WF_FLOOR_ALTSTACK_BYTES;
    alternate.ss_flags = 0;
    if (sigaltstack(&alternate, NULL) != 0) {
        munmap(memory, WF_FLOOR_ALTSTACK_BYTES);
        wf__floor_setup_failed();
    }
}

/* ------------------------------------------------------- waiting contexts */

/* The stacks waiting contexts run on [WAIT-2], and the switch between them.
 *
 * A context is a stack of its own on the thread that started it. The floor
 * owns it for the reason it owns the entry's stack: the size is the
 * compiler's number, the guard page below it is what turns running out into
 * the stack record, and the thread-local bounds the handler classifies with
 * must describe whichever stack is running. So every switch goes through
 * here and moves the bounds with it.
 *
 * The completion bridge schedules contexts; this unit only creates, switches
 * and releases them. A switch saves exactly the registers the platform's
 * calling convention requires a callee to preserve, plus the floating-point
 * control state, because a switch is an ordinary call as far as the code on
 * either side can tell. */

/* A context's stack reservation. Smaller than the entry's, because a server
 * may hold one per connection; a reservation, so untouched pages cost
 * nothing resident. A context that runs out of it ends in the stack record
 * exactly as the entry does. */
#define WF_FLOOR_CONTEXT_STACK_BYTES ((size_t)64u * 1024u * 1024u)

/* Released stacks kept for the next start, so a server that starts one context
 * per connection does not map and unmap one per connection. */
#define WF_FLOOR_CONTEXT_CACHE 64u

typedef struct wf__floor_context {
    void *sp;
    unsigned long low;
    unsigned long high;
    void *mapping;
    size_t mapping_bytes;
    void (*entry)(void *header);
    void *header;
    struct wf__floor_context *cached_next;
} wf__floor_context;

/* The calling thread's own stack, as a context the first switch saves into. */
static _Thread_local wf__floor_context wf__floor_thread_context;
static _Thread_local int wf__floor_thread_context_known;
static _Thread_local wf__floor_context *wf__floor_context_cache;
static _Thread_local unsigned wf__floor_context_cached;

void wf__floor_context_swap(void **save, void *target);
void wf__floor_context_trampoline(void);

/* The first code a new context runs, called by the trampoline with its own
 * record. The entry never returns: a finished context is switched away from
 * by the bridge and released by the context that runs next. */
void wf__floor_context_main(wf__floor_context *context) {
    context->entry(context->header);
    abort();
}

#if defined(__APPLE__)
#define WF_FLOOR_SYMBOL(name) "_" #name
#else
#define WF_FLOOR_SYMBOL(name) #name
#endif

#if defined(__x86_64__)
/* System V: rbx, rbp and r12-r15 are preserved, with MXCSR's control bits and
 * the x87 control word. The new context's frame holds the same slots, with
 * rbx carrying its record and the return address naming the trampoline. */
__asm__(
    ".text\n"
    ".globl " WF_FLOOR_SYMBOL(wf__floor_context_swap) "\n"
    WF_FLOOR_SYMBOL(wf__floor_context_swap) ":\n"
    "  pushq %rbp\n"
    "  pushq %rbx\n"
    "  pushq %r12\n"
    "  pushq %r13\n"
    "  pushq %r14\n"
    "  pushq %r15\n"
    "  subq $8, %rsp\n"
    "  stmxcsr (%rsp)\n"
    "  fnstcw 4(%rsp)\n"
    "  movq %rsp, (%rdi)\n"
    "  movq %rsi, %rsp\n"
    "  ldmxcsr (%rsp)\n"
    "  fldcw 4(%rsp)\n"
    "  addq $8, %rsp\n"
    "  popq %r15\n"
    "  popq %r14\n"
    "  popq %r13\n"
    "  popq %r12\n"
    "  popq %rbx\n"
    "  popq %rbp\n"
    "  ret\n"
    ".globl " WF_FLOOR_SYMBOL(wf__floor_context_trampoline) "\n"
    WF_FLOOR_SYMBOL(wf__floor_context_trampoline) ":\n"
    "  movq %rbx, %rdi\n"
    "  call " WF_FLOOR_SYMBOL(wf__floor_context_main) "\n"
    "  ud2\n"
);
#define WF_FLOOR_SWITCH_FRAME_BYTES 64u
static void wf__floor_context_frame(wf__floor_context *context, uintptr_t top) {
    uint64_t *frame = (uint64_t *)(top - WF_FLOOR_SWITCH_FRAME_BYTES);
    uint32_t mxcsr = 0x1f80u;
    uint16_t x87 = 0x037fu;
    memset(frame, 0, WF_FLOOR_SWITCH_FRAME_BYTES);
    memcpy(&frame[0], &mxcsr, sizeof(mxcsr));
    memcpy((char *)&frame[0] + 4, &x87, sizeof(x87));
    /* frame[1..4] are r15, r14, r13 and r12. */
    frame[5] = (uint64_t)(uintptr_t)context; /* rbx */
    frame[6] = 0;                            /* rbp */
    frame[7] = (uint64_t)(uintptr_t)wf__floor_context_trampoline;
    context->sp = frame;
}
#elif defined(__aarch64__)
/* AAPCS64: x19-x28, the frame and link registers and the low halves of
 * v8-v15 are preserved, with FPCR. x18 is the platform register on Darwin
 * and is not touched. The new context's link register names the trampoline
 * and x19 carries its record. */
__asm__(
    ".text\n"
    ".globl " WF_FLOOR_SYMBOL(wf__floor_context_swap) "\n"
    ".p2align 2\n"
    WF_FLOOR_SYMBOL(wf__floor_context_swap) ":\n"
    "  sub sp, sp, #176\n"
    "  stp x19, x20, [sp, #0]\n"
    "  stp x21, x22, [sp, #16]\n"
    "  stp x23, x24, [sp, #32]\n"
    "  stp x25, x26, [sp, #48]\n"
    "  stp x27, x28, [sp, #64]\n"
    "  stp x29, x30, [sp, #80]\n"
    "  stp d8, d9, [sp, #96]\n"
    "  stp d10, d11, [sp, #112]\n"
    "  stp d12, d13, [sp, #128]\n"
    "  stp d14, d15, [sp, #144]\n"
    "  mrs x9, fpcr\n"
    "  str x9, [sp, #160]\n"
    "  mov x9, sp\n"
    "  str x9, [x0]\n"
    "  mov sp, x1\n"
    "  ldr x9, [sp, #160]\n"
    "  msr fpcr, x9\n"
    "  ldp x19, x20, [sp, #0]\n"
    "  ldp x21, x22, [sp, #16]\n"
    "  ldp x23, x24, [sp, #32]\n"
    "  ldp x25, x26, [sp, #48]\n"
    "  ldp x27, x28, [sp, #64]\n"
    "  ldp x29, x30, [sp, #80]\n"
    "  ldp d8, d9, [sp, #96]\n"
    "  ldp d10, d11, [sp, #112]\n"
    "  ldp d12, d13, [sp, #128]\n"
    "  ldp d14, d15, [sp, #144]\n"
    "  add sp, sp, #176\n"
    "  ret\n"
    ".globl " WF_FLOOR_SYMBOL(wf__floor_context_trampoline) "\n"
    ".p2align 2\n"
    WF_FLOOR_SYMBOL(wf__floor_context_trampoline) ":\n"
    "  mov x0, x19\n"
    "  bl " WF_FLOOR_SYMBOL(wf__floor_context_main) "\n"
    "  brk #0\n"
);
#define WF_FLOOR_SWITCH_FRAME_BYTES 176u
static void wf__floor_context_frame(wf__floor_context *context, uintptr_t top) {
    uint64_t *frame = (uint64_t *)(top - WF_FLOOR_SWITCH_FRAME_BYTES);
    memset(frame, 0, WF_FLOOR_SWITCH_FRAME_BYTES);
    frame[0] = (uint64_t)(uintptr_t)context; /* x19 */
    frame[10] = 0;                           /* x29 */
    frame[11] = (uint64_t)(uintptr_t)wf__floor_context_trampoline; /* x30 */
    /* frame[20] is FPCR: zero is the default rounding and no traps. */
    context->sp = frame;
}
#else
#error "waiting contexts need a switch for this architecture"
#endif

static size_t wf__floor_page_bytes(void) {
    return (size_t)wf__floor_guard_band - (size_t)WF_FLOOR_RED_ZONE_BYTES;
}

static size_t wf__floor_round_up(size_t value, size_t unit) {
    return (value + unit - 1u) / unit * unit;
}

/* The calling thread's own stack as a context. Its bounds are the ones the
 * thread captured when it attached. */
void *wf__floor_context_thread(void) {
    if (!wf__floor_thread_context_known) {
        wf__floor_thread_context.low = wf__floor_stack_low;
        wf__floor_thread_context.high = wf__floor_stack_high;
        wf__floor_thread_context_known = 1;
    }
    return &wf__floor_thread_context;
}

/* A new context whose first switch runs entry(header) on a stack of its own.
 * header points to header_bytes of storage, 16-aligned, that lives exactly as
 * long as the context. Returns NULL when no stack could be reserved. */
void *wf__floor_context_create(
    size_t header_bytes,
    void (*entry)(void *header),
    void **header
) {
    size_t page = wf__floor_page_bytes();
    size_t record = wf__floor_round_up(sizeof(wf__floor_context), 16u);
    size_t reserved = wf__floor_round_up(header_bytes, 16u);
    wf__floor_context *context;
    uintptr_t top;

    if (page == 0 || entry == NULL || header == NULL
        || reserved > WF_FLOOR_CONTEXT_STACK_BYTES / 4u) {
        return NULL;
    }
    context = wf__floor_context_cache;
    if (context != NULL) {
        wf__floor_context_cache = context->cached_next;
        wf__floor_context_cached -= 1u;
    } else {
        size_t mapping_bytes = page + WF_FLOOR_CONTEXT_STACK_BYTES;
        int flags = MAP_PRIVATE | MAP_ANON;
#if defined(MAP_NORESERVE)
        flags |= MAP_NORESERVE;
#endif
#if defined(MAP_STACK)
        flags |= MAP_STACK;
#endif
        void *mapping = mmap(NULL, mapping_bytes, PROT_READ | PROT_WRITE, flags, -1, 0);
        if (mapping == MAP_FAILED) {
            return NULL;
        }
        /* The lowest page is the guard the probed frames fault on. */
        if (mprotect(mapping, page, PROT_NONE) != 0) {
            munmap(mapping, mapping_bytes);
            return NULL;
        }
        top = (uintptr_t)mapping + mapping_bytes;
        context = (wf__floor_context *)(top - record);
        context->mapping = mapping;
        context->mapping_bytes = mapping_bytes;
        context->low = (unsigned long)((uintptr_t)mapping + page);
    }
    top = (uintptr_t)context;
    context->header = (void *)(top - reserved);
    context->high = (unsigned long)(top - reserved);
    context->entry = entry;
    context->cached_next = NULL;
    wf__floor_context_frame(context, top - reserved);
    *header = context->header;
    return context;
}

/* Releases a context that will never run again. The caller is running on
 * another stack. */
void wf__floor_context_release(void *opaque) {
    wf__floor_context *context = (wf__floor_context *)opaque;
    if (context == NULL || context == &wf__floor_thread_context) {
        return;
    }
    if (wf__floor_context_cached < WF_FLOOR_CONTEXT_CACHE) {
        context->cached_next = wf__floor_context_cache;
        wf__floor_context_cache = context;
        wf__floor_context_cached += 1u;
        return;
    }
    munmap(context->mapping, context->mapping_bytes);
}

/* Saves the running context into from and resumes to. The handler's bounds
 * move first, so a fault on the stack about to run is classified by its own
 * guard page. */
void wf__floor_context_switch(void *from, void *to) {
    wf__floor_context *source = (wf__floor_context *)from;
    wf__floor_context *target = (wf__floor_context *)to;
    wf__floor_stack_low = target->low;
    wf__floor_stack_high = target->high;
    wf__floor_context_swap(&source->sp, target->sp);
}

/* No stack could be reserved for a context the program starts: the same
 * resource, and the same record, as running out of one. */
_Noreturn void wf__floor_context_exhausted(void) {
    if (__sync_bool_compare_and_swap(&wf__floor_latch, 0, 1)) {
        wf__floor_emit(WF_FLOOR_STACK_RECORD, sizeof(WF_FLOOR_STACK_RECORD) - 1);
    }
    abort();
}

/* The process-wide half: one disposition for the two fault signals.
 *
 * Both are required. A stack that runs out on the entry thread arrives as
 * SIGSEGV and one that runs out on a pool lane arrives as SIGBUS, so a
 * SIGSEGV-only disposition would miss every worker overflow — which is
 * precisely the case the parallel default introduced.
 *
 * SA_NODEFER is deliberately absent: with the signal blocked for the duration
 * of the handler, the restore-and-return path above is what re-raises it, and
 * a fault inside the handler cannot re-enter the handler.
 *
 * Installation is required before running the command or starting workers.
 * Setup refusal stops at the host boundary without a source outcome. */
static void wf__floor_install(void) {
    struct sigaction action;
    long page = sysconf(_SC_PAGESIZE);
    if (page <= 0) {
        wf__floor_setup_failed();
    }
    wf__floor_guard_band = (unsigned long)page + WF_FLOOR_RED_ZONE_BYTES;
    memset(&action, 0, sizeof(action));
    action.sa_sigaction = wf__floor_handler;
    action.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGSEGV, &action, NULL) != 0) {
        wf__floor_setup_failed();
    }
    if (sigaction(SIGBUS, &action, NULL) != 0) {
        wf__floor_setup_failed();
    }
    wf__floor_attach_thread();
}

/* ------------------------------------------------------------- the entry */

struct wf__floor_call {
    int argc;
    char **argv;
    int status;
};

static void *wf__floor_entry(void *opaque) {
    struct wf__floor_call *call = (struct wf__floor_call *)opaque;
    wf__floor_attach_thread();
    call->status = wf__main_body(call->argc, call->argv);
    return NULL;
}

/* Validate runtime settings before user code. Standalone emitted-module
 * probes can link the floor alone and use this weak no-op. */
__attribute__((weak)) void wf__runtime_start(void) {}

/* Run on an ordinary thread with the declared stack reservation. If host
 * thread creation fails, retain the existing fallback to the original
 * thread, whose exhaustion handler and bounds are already installed. */
int wf__floor_run(int argc, char **argv) {
    pthread_attr_t attributes;
    pthread_t thread;
    struct wf__floor_call call;

    call.argc = argc;
    call.argv = argv;
    call.status = 0;

    wf__floor_install();

    wf__runtime_start();

    if (pthread_attr_init(&attributes) != 0) {
        return wf__main_body(argc, argv);
    }
    if (pthread_attr_setstacksize(&attributes, WF_FLOOR_STACK_BYTES) != 0
        || pthread_create(&thread, &attributes, wf__floor_entry, &call) != 0) {
        pthread_attr_destroy(&attributes);
        return wf__main_body(argc, argv);
    }
    pthread_attr_destroy(&attributes);
    /* The thread was created joinable by this thread, so the two documented
     * failures — joining a non-joinable thread and joining oneself — are both
     * unreachable here. Re-running the entry on a failure would run the whole
     * program twice, which is why nothing here retries. */
    pthread_join(thread, NULL);
    return call.status;
}
