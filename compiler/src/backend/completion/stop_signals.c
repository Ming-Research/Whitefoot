#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE 1
#endif
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE 1
#endif
#if !defined(_WIN32) && !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif

/* Invocation stop requests. The host observer never executes WF code. A
 * published record uses the same wake epoch/port as every other completion.
 * One lifecycle lock serializes open/close; another protects the FIFO and its
 * one waiter. Host observation never takes the lifecycle lock. */
#include "bridge.h"
#include "contract.h"
#include "file_adapter.h"
#include "../sched/entry.h"
#include "../sched/prim.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#include <windows.h>
#else
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <unistd.h>
#if defined(__linux__)
#include <sys/signalfd.h>
#elif defined(__APPLE__)
#include <sys/event.h>
#endif
#endif

typedef struct wf_stop_request {
    struct wf_stop_request *next;
    unsigned kind;
} wf_stop_request;
static wf_prim_wait wf_stop_lifecycle;
static wf_prim_wait wf_stop_queue;
static wf_stop_request *wf_stop_head, *wf_stop_tail;
static wf_completion_record *wf_stop_waiter;
static unsigned wf_stop_once;
static int wf_stop_active;
static int wf_stop_initial_error;

static _Noreturn void wf_stop_fail(const char *message) {
    fprintf(stderr, "whitefoot stop signals: %s\n", message);
    fflush(stderr);
    _Exit(70);
}

/* queue lock held. Completion must be the last access to the record: its
 * owner can resume and release it on another driver immediately. */
static void wf_stop_complete(wf_completion_record *record, int kind, int error) {
    record->result.kind = record->request.kind;
    record->result.value = kind;
    record->result.error_code = error;
    wf_completion_record_complete(record);
}

static int wf_stop_expired(wf_completion_record *record) {
    uint64_t deadline = atomic_load_explicit(&record->deadline, memory_order_acquire);
    if (deadline == 0) return 0;
    if (deadline == WF_COMPLETION_DEADLINE_FIRED) return 1;
    if (wf_file_monotonic_ns() < deadline) return 0;
    atomic_store_explicit(&record->deadline, WF_COMPLETION_DEADLINE_FIRED,
                          memory_order_release);
    return 1;
}

void wf__stop_observe(unsigned kind) {
    wf_prim_wait_lock(&wf_stop_queue);
    if (wf_stop_active) {
        wf_completion_record *waiter = wf_stop_waiter;
        wf_stop_waiter = NULL;
        if (waiter != NULL && wf_stop_expired(waiter)) {
            wf_stop_complete(waiter, -1, wf_file_cancelled_error());
            waiter = NULL;
        }
        if (waiter != NULL) {
            wf_stop_complete(waiter, (int)kind, 0);
        } else {
            /* No finite queue can promise to retain every observed request.
             * Grow in the runtime pool, whose exhaustion reports and stops
             * [SCOPE-3], rather than overwriting or silently dropping one. */
            wf_stop_request *request = wf__runtime_take(sizeof(*request));
            request->next = NULL;
            request->kind = kind;
            if (wf_stop_tail != NULL) wf_stop_tail->next = request;
            else wf_stop_head = request;
            wf_stop_tail = request;
        }
    }
    wf_prim_wait_unlock(&wf_stop_queue);
}

#if defined(_WIN32)
/* A close/reopen must not capture an old handler. Each handler remembers the
 * generation it entered; close wakes every handler of that generation before
 * a later open can reuse the listener. No callback retains an event handle
 * that close could free while it is waiting. */
static SRWLOCK wf_stop_console_lock = SRWLOCK_INIT;
static CONDITION_VARIABLE wf_stop_console_closed = CONDITION_VARIABLE_INIT;
static uint64_t wf_stop_console_generation;
static int wf_stop_console_active;

BOOL WINAPI wf__stop_console_handler(DWORD event) {
    unsigned kind;
    int hold;
    switch (event) {
        case CTRL_C_EVENT: kind = 0; hold = 0; break;
        case CTRL_BREAK_EVENT: kind = 1; hold = 0; break;
        case CTRL_CLOSE_EVENT:
        case CTRL_LOGOFF_EVENT:
        case CTRL_SHUTDOWN_EVENT: kind = 1; hold = 1; break;
        default: return FALSE;
    }
    AcquireSRWLockExclusive(&wf_stop_console_lock);
    if (!wf_stop_console_active) {
        ReleaseSRWLockExclusive(&wf_stop_console_lock);
        return FALSE;
    }
    uint64_t generation = wf_stop_console_generation;
    /* The ordinary completion publication posts the IOCP wake packet when
     * that is the waiting context's route, and signals the adapter otherwise. */
    wf__stop_observe(kind);
    while (hold && wf_stop_console_active && generation == wf_stop_console_generation) {
        if (!SleepConditionVariableSRW(&wf_stop_console_closed, &wf_stop_console_lock,
                                       INFINITE, 0))
            wf_stop_fail("console handler wait failed");
    }
    ReleaseSRWLockExclusive(&wf_stop_console_lock);
    return TRUE;
}

static int wf_stop_host_initialize(void) {
    /* A child created in a new console process group inherits Ctrl-C disabled.
     * The invocation supplies the ordinary host default, including Ctrl-C. */
    /* A detached/non-console invocation has no console events. That is not
     * a failure to construct Inputs; opening a listener reports the host's
     * refusal if the invocation still has no console then. */
    (void)SetConsoleCtrlHandler(NULL, FALSE);
    return 0;
}

static int wf_stop_host_change(int open_listener) {
    int error = 0;
    AcquireSRWLockExclusive(&wf_stop_console_lock);
    if (!SetConsoleCtrlHandler(wf__stop_console_handler, open_listener ? TRUE : FALSE)) {
        error = (int)GetLastError();
        if (!open_listener) wf_stop_fail("cannot restore console control default");
    } else {
        wf_stop_console_active = open_listener;
        wf_stop_console_generation += 1;
        if (!open_listener) WakeAllConditionVariable(&wf_stop_console_closed);
    }
    ReleaseSRWLockExclusive(&wf_stop_console_lock);
    return error;
}

static int wf_stop_busy_error(void) { return ERROR_BUSY; }
#else
/* Only the launcher's original thread unblocks SIGINT/SIGTERM on Linux.
 * It blocks them before creating the entry thread; all runtime threads
 * are also created with this mask by wf_prim_thread_start. Thus opening on
 * any driver needs no asynchronous alteration of another thread's mask.
 * With no listener the receiver is unblocked with SIG_DFL, so the kernel
 * itself supplies the normal signal exit status. */
static wf_prim_wait wf_stop_control;
static int wf_stop_pipe[2];
enum {
    WF_STOP_IDLE = -1,
    WF_STOP_CLOSE,
    WF_STOP_OPEN,
    WF_STOP_ENTRY_RETURNED
};
static int wf_stop_command = WF_STOP_IDLE;
static int wf_stop_command_error;
/* Protected by the control lock. Preparation promises that the launcher
 * will enter the loop; callers can submit before it reaches its first poll. */
static int wf_stop_launcher_available;
static sigset_t wf_stop_mask;

static void wf_stop_mask_change(int how) {
    if (pthread_sigmask(how, &wf_stop_mask, NULL) != 0)
        wf_stop_fail("cannot change receiver signal mask");
}

static void wf_stop_disposition(void (*handler)(int)) {
    struct sigaction action;
    memset(&action, 0, sizeof(action));
    action.sa_handler = handler;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGINT, &action, NULL) != 0 || sigaction(SIGTERM, &action, NULL) != 0)
        wf_stop_fail("cannot set stop signal disposition");
}

static int wf_stop_host_open(void) {
#if defined(__linux__)
    wf_stop_mask_change(SIG_BLOCK);
    int descriptor = signalfd(-1, &wf_stop_mask, SFD_CLOEXEC | SFD_NONBLOCK);
    if (descriptor < 0) {
        int error = errno;
        wf_stop_mask_change(SIG_UNBLOCK);
        errno = error;
    }
    return descriptor;
#elif defined(__APPLE__)
    int descriptor = kqueue();
    if (descriptor < 0) return -1;
    if (fcntl(descriptor, F_SETFD, FD_CLOEXEC) < 0) {
        int error = errno;
        close(descriptor);
        errno = error;
        return -1;
    }
    struct kevent events[3];
    EV_SET(&events[0], SIGINT, EVFILT_SIGNAL, EV_ADD | EV_CLEAR, 0, 0, NULL);
    EV_SET(&events[1], SIGTERM, EVFILT_SIGNAL, EV_ADD | EV_CLEAR, 0, 0, NULL);
    EV_SET(&events[2], wf_stop_pipe[0], EVFILT_READ, EV_ADD, 0, 0, NULL);
    if (kevent(descriptor, events, 3, NULL, 0, NULL) < 0) {
        int error = errno;
        close(descriptor);
        errno = error;
        return -1;
    }
    wf_stop_disposition(SIG_IGN);
    return descriptor;
#else
    errno = ENOTSUP;
    return -1;
#endif
}

static void wf_stop_host_release(int descriptor) {
    wf_stop_disposition(SIG_DFL);
    /* Close is never retried: even an interrupted close consumes the fd. */
    if (close(descriptor) != 0 && errno != EINTR)
        wf_stop_fail("cannot close stop descriptor");
#if defined(__linux__)
    wf_stop_mask_change(SIG_UNBLOCK);
#endif
}

#if defined(__linux__)
static void wf_stop_read_observed(int descriptor) {
    struct signalfd_siginfo info;
    wf_completion_record record;
    int64_t amount;
    int error;
    /* Use ordinary stream-read submit/join: offer the read to the ring first,
     * then use the file-adapter path when the ring does not take it.
     * Only this receiver submits reads of the descriptor, and a read is
     * submitted only after poll reports it ready; close runs on this receiver
     * after its read has joined. */
    wf__completion_file_read_submit(descriptor, &info, sizeof(info), &record);
    wf__completion_file_join(&record, &amount, &error);
    if (amount != (int64_t)sizeof(info) || error != 0)
        wf_stop_fail("signalfd read failed");
    wf__stop_observe(info.ssi_signo == SIGINT ? 0u : 1u);
}
#elif defined(__APPLE__)
static int wf_stop_wait_kqueue(int descriptor) {
    struct kevent events[3];
    int count = kevent(descriptor, NULL, 0, events, 3, NULL);
    if (count < 0) {
        if (errno == EINTR) return 0;
        wf_stop_fail("signal kevent failed");
    }
    int control = 0;
    for (int index = 0; index < count; ++index) {
        if (events[index].flags & EV_ERROR) wf_stop_fail("signal filter failed");
        if (events[index].filter == EVFILT_READ) control = 1;
        else {
            /* data counts coalesced signals, not their interleaving [PRE-2]. */
            wf__stop_observe(events[index].ident == SIGINT ? 0u : 1u);
        }
    }
    return control;
}
#endif

void wf__stop_receive(void) {
    int descriptor = -1;
    /* wf__floor_run already attached this thread to the exhaustion floor,
     * including the completion submit/join path used for signalfd reads. */
    wf_stop_mask_change(SIG_UNBLOCK);
    for (;;) {
        struct pollfd watches[2] = {
            {wf_stop_pipe[0], POLLIN, 0}, {descriptor, POLLIN, 0}
        };
#if defined(__APPLE__)
        if (descriptor >= 0) {
            /* Wait on the control pipe and signal filters in the same queue;
             * this needs no assumption that poll supports a kqueue fd. */
            if (!wf_stop_wait_kqueue(descriptor)) continue;
            watches[0].revents = POLLIN;
        } else
#endif
        {
            int ready = poll(watches, 2, -1);
            if (ready < 0) {
                if (errno == EINTR) continue;
                wf_stop_fail("receiver poll failed");
            }
        }
        /* Observation is serialized here, including observations with no WF
         * waiter. Control wins a simultaneous close, which ends interception. */
        if (watches[0].revents & POLLIN) {
            char byte;
            if (read(wf_stop_pipe[0], &byte, 1) != 1) wf_stop_fail("control pipe read failed");
            wf_prim_wait_lock(&wf_stop_control);
            wf_stop_command_error = 0;
            int returned = wf_stop_command == WF_STOP_ENTRY_RETURNED;
            if (wf_stop_command == WF_STOP_OPEN) {
                descriptor = wf_stop_host_open();
                if (descriptor < 0) wf_stop_command_error = errno;
            } else if (descriptor >= 0) {
                wf_stop_host_release(descriptor);
                descriptor = -1;
            }
            if (returned) wf_stop_launcher_available = 0;
            wf_stop_command = WF_STOP_IDLE;
            wf_prim_wait_signal(&wf_stop_control);
            wf_prim_wait_unlock(&wf_stop_control);
            if (returned) {
                close(wf_stop_pipe[0]);
                close(wf_stop_pipe[1]);
                return;
            }
#if defined(__linux__)
        } else if (watches[1].revents & POLLIN) {
            wf_stop_read_observed(descriptor);
#endif
        } else {
            wf_stop_fail("receiver descriptor failed");
        }
    }
}

static int wf_stop_host_initialize(void) {
    /* Inputs and native runtime probes may initialize without a launcher.
     * Do not change their signal masks or dispositions or create a receiver. */
    return wf_prim_wait_init(&wf_stop_control) == 0 ? 0 : ENOMEM;
}

int wf__stop_prepare(void) {
    int error = wf__stop_initialize();
    if (error != 0) return error;
    sigemptyset(&wf_stop_mask);
    sigaddset(&wf_stop_mask, SIGINT);
    sigaddset(&wf_stop_mask, SIGTERM);
    wf_stop_disposition(SIG_DFL);
    if (pipe(wf_stop_pipe) != 0) return errno;
    for (unsigned index = 0; index < 2; ++index) {
        if (fcntl(wf_stop_pipe[index], F_SETFD, FD_CLOEXEC) < 0) {
            int error = errno;
            close(wf_stop_pipe[0]);
            close(wf_stop_pipe[1]);
            return error;
        }
    }
#if defined(__linux__)
    wf_stop_mask_change(SIG_BLOCK);
#endif
    wf_prim_wait_lock(&wf_stop_control);
    wf_stop_launcher_available = 1;
    wf_prim_wait_unlock(&wf_stop_control);
    return 0;
}

/* The lifecycle lock serializes commands, including entry return. */
static int wf_stop_host_change(int command) {
    wf_prim_wait_lock(&wf_stop_control);
    if (!wf_stop_launcher_available) {
        wf_prim_wait_unlock(&wf_stop_control);
        return ENOTSUP;
    }
    wf_stop_command = command;
    char byte = 0;
    ssize_t sent;
    do { sent = write(wf_stop_pipe[1], &byte, 1); } while (sent < 0 && errno == EINTR);
    if (sent != 1) wf_stop_fail("control pipe write failed");
    while (wf_stop_command != WF_STOP_IDLE) wf_prim_wait_sleep(&wf_stop_control);
    int error = wf_stop_command_error;
    wf_prim_wait_unlock(&wf_stop_control);
    return error;
}

static int wf_stop_busy_error(void) { return EBUSY; }
#endif

static void wf_stop_initialize_once(void) {
    if (wf_prim_wait_init(&wf_stop_lifecycle) != 0 || wf_prim_wait_init(&wf_stop_queue) != 0)
        wf_stop_fail("cannot initialize request queue");
    wf_stop_initial_error = wf_stop_host_initialize();
}

int wf__stop_initialize(void) {
    /* Shared by the launcher and ordinary Inputs construction. Only POSIX
     * launcher preparation changes masks, before it creates any threads. */
    wf__sched_once(&wf_stop_once, wf_stop_initialize_once);
    return wf_stop_initial_error;
}

int wf__stop_listen_begin(void) {
    wf_prim_wait_lock(&wf_stop_lifecycle);
    wf_prim_wait_lock(&wf_stop_queue);
    if (wf_stop_active) {
        wf_prim_wait_unlock(&wf_stop_queue);
        wf_prim_wait_unlock(&wf_stop_lifecycle);
        return wf_stop_busy_error();
    }
    wf_prim_wait_unlock(&wf_stop_queue);
    return 0;
}

int wf__stop_listen_finish(int has_credit) {
    if (!has_credit) {
        wf_prim_wait_unlock(&wf_stop_lifecycle);
        return 0;
    }
    wf_prim_wait_lock(&wf_stop_queue);
    wf_stop_active = 1;
    wf_prim_wait_unlock(&wf_stop_queue);
    int error = wf_stop_host_change(1);
    if (error != 0) {
        wf_prim_wait_lock(&wf_stop_queue);
        wf_stop_active = 0;
        wf_prim_wait_unlock(&wf_stop_queue);
    }
    wf_prim_wait_unlock(&wf_stop_lifecycle);
    return error;
}

/* The lifecycle lock is held and the host has stopped interception. */
static void wf_stop_clear(void) {
    wf_prim_wait_lock(&wf_stop_queue);
    if (wf_stop_waiter != NULL) wf_stop_fail("closing a borrowed listener");
    wf_stop_active = 0;
    while (wf_stop_head != NULL) {
        wf_stop_request *request = wf_stop_head;
        wf_stop_head = request->next;
        wf__runtime_give(request, sizeof(*request));
    }
    wf_stop_tail = NULL;
    wf_prim_wait_unlock(&wf_stop_queue);
}

int wf__stop_close(void) {
    wf_prim_wait_lock(&wf_stop_lifecycle);
    int error = wf_stop_host_change(0);
    wf_stop_clear();
    wf_prim_wait_unlock(&wf_stop_lifecycle);
    return error;
}

#if !defined(_WIN32)
void wf__stop_entry_returned(void) {
    wf_prim_wait_lock(&wf_stop_lifecycle);
    if (wf_stop_host_change(WF_STOP_ENTRY_RETURNED) != 0)
        wf_stop_fail("entry returned without a launcher receiver");
    /* A native entry may leave a listener open. Restore its host state before
     * acknowledging return; release retained observations before the join. */
    wf_stop_clear();
    wf_prim_wait_unlock(&wf_stop_lifecycle);
}
#endif

void wf__stop_next(wf_completion_record *record) {
    wf_prim_wait_lock(&wf_stop_queue);
    if (!wf_stop_active || wf_stop_waiter != NULL) wf_stop_fail("invalid listener borrow");
    if (wf_stop_head != NULL) {
        wf_stop_request *request = wf_stop_head;
        unsigned kind = request->kind;
        wf_stop_head = request->next;
        if (wf_stop_head == NULL) wf_stop_tail = NULL;
        wf__runtime_give(request, sizeof(*request));
        wf_stop_complete(record, (int)kind, 0);
    } else if (wf_stop_expired(record)) {
        wf_stop_complete(record, -1, wf_file_cancelled_error());
    } else {
        wf_stop_waiter = record;
    }
    wf_prim_wait_unlock(&wf_stop_queue);
}

void wf__stop_cancel(wf_completion_record *record) {
    wf_prim_wait_lock(&wf_stop_queue);
    if (wf_stop_waiter == record) {
        wf_stop_waiter = NULL;
        wf_stop_complete(record, -1, wf_file_cancelled_error());
    }
    wf_prim_wait_unlock(&wf_stop_queue);
}
