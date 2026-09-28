/* Windows resource-exhaustion floor. The command and compute workers use
 * ordinary threads with the declared stack reservation. Each installs
 * SetThreadStackGuarantee; the process handler classifies only
 * EXCEPTION_STACK_OVERFLOW, leaving other exceptions to normal handling.
 * The exception path performs no allocation, stdio or locking. */

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0600
#endif

#include <windows.h>

#include <process.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

extern int wf__main_body(int argc, char **argv);

/* Validate runtime settings before user code. Standalone emitted-module
 * probes can link the floor alone and use this weak no-op. */
__attribute__((weak)) void wf__runtime_start(void) {}

#define WF_FLOOR_STACK_BYTES ((size_t)1024u * 1024u * 1024u)
#define WF_FLOOR_EXCEPTION_STACK_BYTES ((ULONG)64u * 1024u)

size_t wf__floor_stack_bytes(void) { return WF_FLOOR_STACK_BYTES; }

static volatile int wf__floor_latch;

_Static_assert(sizeof(int) == sizeof(LONG), "floor latch width");
_Static_assert(_Alignof(int) == _Alignof(LONG), "floor latch alignment");

volatile int *wf__floor_record_latch(void) { return &wf__floor_latch; }

static const char WF_FLOOR_STACK_RECORD[] = "{\"resource\":\"stack\"}\n";

/* The floor's own writer.
 *
 * `WriteFile` on the standard error handle rather than `stdio`, because the
 * record's first writer is an exception handler running on a stack that has
 * just overflowed: the emergency stack `SetThreadStackGuarantee` reserves is
 * enough for a system call and not for the CRT's buffered path. */
static void wf__floor_write_error(const char *text, DWORD length) {
    HANDLE error_handle = GetStdHandle(STD_ERROR_HANDLE);
    DWORD offset = 0;

    if (error_handle == NULL || error_handle == INVALID_HANDLE_VALUE) {
        return;
    }
    while (offset < length) {
        DWORD written = 0;
        if (WriteFile(
                error_handle,
                text + offset,
                length - offset,
                &written,
                NULL
            ) == FALSE
            || written == 0) {
            return;
        }
        offset += written;
    }
}

static void wf__floor_emit_stack_record(void) {
    wf__floor_write_error(
        WF_FLOOR_STACK_RECORD,
        (DWORD)(sizeof(WF_FLOOR_STACK_RECORD) - 1u)
    );
}

/* This floor's one fail-stop, for the ends that are *not* a classified
 * exhaustion.
 *
 * A floor that cannot install its handler, or cannot start or join the thread
 * it runs the program on, has no classified boundary left to offer, and that
 * is a trusted-computing-base defect rather than a program outcome.  It says
 * which one before it ends the process: a bare `abort` under the release UCRT
 * takes the fast-fail path, which a shell reports as a bare status with no
 * message at all.
 *
 * The classified exhaustion itself does not come through here.  It has already
 * written the one record the boundary is defined as, and a second line beside
 * it would be a second record on the one channel that is allowed exactly
 * one. */
static _Noreturn void wf__floor_fail(const char *reason, DWORD length) {
    static const char prefix[] = "whitefoot floor: ";
    wf__floor_write_error(prefix, (DWORD)(sizeof(prefix) - 1u));
    wf__floor_write_error(reason, length);
    wf__floor_write_error("\n", 1u);
    abort();
}

#define WF_FLOOR_FAIL(text) wf__floor_fail((text), (DWORD)(sizeof(text) - 1u))

static LONG CALLBACK wf__floor_exception_handler(
    EXCEPTION_POINTERS *exception
) {
    if (exception == NULL || exception->ExceptionRecord == NULL
        || exception->ExceptionRecord->ExceptionCode
            != EXCEPTION_STACK_OVERFLOW) {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    if (InterlockedCompareExchange(
            (volatile LONG *)&wf__floor_latch,
            1,
            0
        ) == 0) {
        /* The one classified exhaustion: the record above *is* the message,
         * and this abort is deliberately bare so the channel carries exactly
         * that one line (see `wf__floor_fail`). */
        wf__floor_emit_stack_record();
        abort();
    }

    /* A different record writer owns the one process-wide channel. It will
     * abort the process; this thread must not race it with a second record. */
    for (;;) {
        Sleep(INFINITE);
    }
}

static INIT_ONCE wf__floor_install_once = INIT_ONCE_STATIC_INIT;
static PVOID wf__floor_handler;

static BOOL CALLBACK wf__floor_install_handler(
    PINIT_ONCE once,
    PVOID parameter,
    PVOID *context
) {
    (void)once;
    (void)parameter;
    (void)context;
    wf__floor_handler = AddVectoredExceptionHandler(
        1u,
        wf__floor_exception_handler
    );
    return wf__floor_handler != NULL;
}

/* The handler is process-wide; the emergency stack guarantee is per thread.
 * Each ordinary command or worker thread attaches on its own stack before
 * executing WF code. Nested helping keeps the calling thread's guarantee. */
void wf__floor_attach_thread(void) {
    ULONG stack_guarantee = WF_FLOOR_EXCEPTION_STACK_BYTES;
    if (InitOnceExecuteOnce(
            &wf__floor_install_once,
            wf__floor_install_handler,
            NULL,
            NULL
        ) == FALSE
        || SetThreadStackGuarantee(&stack_guarantee) == FALSE) {
        /* A runtime thread without the process handler or its emergency stack
         * cannot preserve Whitefoot's one classified exhaustion boundary.
         * Continuing would be a silent change of runtime semantics, so the
         * native backend is unavailable rather than degraded. */
        WF_FLOOR_FAIL(
            "the exhaustion handler or its emergency stack could not be installed"
        );
    }
}

typedef struct wf__floor_call {
    int argc;
    char **argv;
    int status;
} wf__floor_call;

static unsigned __stdcall wf__floor_entry(void *opaque) {
    wf__floor_call *call = (wf__floor_call *)opaque;
    wf__floor_attach_thread();
    call->status = wf__main_body(call->argc, call->argv);
    return 0;
}

/* Start the command on its declared ordinary stack. Failure to reserve that
 * stack remains a host-boundary failure on Windows. */
int wf__floor_run(int argc, void *argv) {
    wf__floor_call call;
    uintptr_t thread_value;
    HANDLE thread;

    call.argc = argc;
    call.argv = (char **)argv;
    call.status = 0;

    /* The host-created thread is armed too because it owns runtime startup and
     * the failure path. */
    wf__floor_attach_thread();

    wf__runtime_start();

    thread_value = _beginthreadex(
        NULL,
        (unsigned)WF_FLOOR_STACK_BYTES,
        wf__floor_entry,
        &call,
        STACK_SIZE_PARAM_IS_A_RESERVATION,
        NULL
    );
    if (thread_value == 0) {
        WF_FLOOR_FAIL("the program's own thread could not be started");
    }
    thread = (HANDLE)thread_value;

    if (WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0) {
        WF_FLOOR_FAIL("the program's own thread could not be joined");
    }
    (void)CloseHandle(thread);
    return call.status;
}
