/* Windows threads and per-lane waiting; no fibers. */
/* Feature selection, and it has to precede every include, exactly as the POSIX
 * leaf names its own. SYSTEM_CPU_SET_INFORMATION is a Windows 10 type and some
 * headers hide it below that level; naming the level here is guarded so a
 * command line that already names it is not a redefinition. Nothing else in
 * this file needs it. */
#if !defined(_WIN32_WINNT)
#define _WIN32_WINNT 0x0A00
#endif
#include "prim.h"
#include <process.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>

static unsigned __stdcall wf_prim_thread_main(void *opaque) {
    wf_prim_thread *thread = (wf_prim_thread *)opaque;
    thread->entry(thread->argument);
    return 0u;
}

int wf_prim_thread_start(
    wf_prim_thread *thread,
    void (*entry)(void *),
    void *argument,
    size_t stack_bytes
) {
    uintptr_t created;
    if (thread == NULL || entry == NULL || stack_bytes > (size_t)UINT_MAX) {
        return 1;
    }
    thread->entry = entry;
    thread->argument = argument;
    created = _beginthreadex(
        NULL,
        (unsigned)stack_bytes,
        wf_prim_thread_main,
        thread,
        STACK_SIZE_PARAM_IS_A_RESERVATION,
        NULL
    );
    if (created == 0) {
        return 1;
    }
    (void)CloseHandle((HANDLE)created);
    return 0;
}

/* How many CPUs this process may actually run on. The process affinity mask
 * is the honest answer within one processor group and is what a job object or
 * an explicit mask narrows, so it is read first; the active processor count
 * across all groups answers when no mask is available. Zero means the count is
 * unknown, and a caller must then choose the behaviour that assumes nothing
 * about it. */
unsigned wf_prim_online_cpus(void) {
    DWORD_PTR process_mask = 0;
    DWORD_PTR system_mask = 0;
    DWORD active;
    if (GetProcessAffinityMask(GetCurrentProcess(), &process_mask, &system_mask)
        && process_mask != 0) {
        unsigned permitted = 0;
        DWORD_PTR remaining = process_mask;
        while (remaining != 0) {
            remaining &= remaining - 1;
            permitted += 1u;
        }
        return permitted;
    }
    active = GetActiveProcessorCount(ALL_PROCESSOR_GROUPS);
    return active > 0 ? (unsigned)active : 0u;
}

/* The most distinct performance levels this probe will name before it stops
 * distinguishing them, matching the POSIX leaf's ceiling, and the largest CPU
 * set report it will read. Four levels is more than any shipping part has, and
 * 8 KiB of report is upwards of a hundred CPU sets; a machine that needs more
 * than that is not concluded about at all. */
#define WF_PRIM_CPU_LEVEL_CEILING 4u
#define WF_PRIM_CPU_SET_BYTES 8192u

/* How many distinct performance levels the CPUs this process may run on are
 * drawn from. One is both "they are alike" and "this host does not say", and
 * the caller must treat those the same: it is the answer that assumes nothing.
 *
 * Windows publishes the fact as a per-CPU-set `EfficiencyClass`, which the
 * scheduler assigns so that a higher class is a faster core; a machine whose
 * cores are alike gives every set the same class. GetSystemCpuSetInformation
 * takes a process handle, so the sets it reports are already the ones this
 * process could be given, and the group and affinity filter below narrows that
 * to the ones it may actually run on -- the same narrowing wf_prim_online_cpus
 * performs above, and for the same reason.
 *
 * That entry point arrived in Windows 10, and it is resolved here at run time
 * rather than imported. An import would make every program this compiler links
 * refuse to start on an older Windows in order to answer a question whose
 * whole purpose is to be answerable or not; a missing symbol is simply a host
 * that does not say, which is one. The resolution goes through a pointer copy
 * because a cast from FARPROC is a cast between incompatible function types,
 * and the whole point of -Wcast-function-type is that such casts are usually a
 * mistake; on this platform the two pointers have one representation.
 *
 * No allocation. The report's size is asked for first and read into one fixed
 * stack buffer; a report that does not fit, a call that fails, and a walk that
 * finds a malformed entry all answer one rather than concluding from part of
 * the picture. */
typedef BOOL(WINAPI *wf_prim_cpu_set_query)(
    PSYSTEM_CPU_SET_INFORMATION, ULONG, PULONG, HANDLE, ULONG);

unsigned wf_prim_cpu_levels(void) {
    _Alignas(16) unsigned char report[WF_PRIM_CPU_SET_BYTES];
    unsigned char seen[WF_PRIM_CPU_LEVEL_CEILING];
    unsigned levels = 0;
    ULONG needed = 0;
    ULONG returned = 0;
    ULONG offset = 0;
    DWORD_PTR process_mask = 0;
    DWORD_PTR system_mask = 0;
    GROUP_AFFINITY group;
    HMODULE kernel;
    FARPROC symbol;
    wf_prim_cpu_set_query query = NULL;
    int filtered;
    memset(&group, 0, sizeof group);
    kernel = GetModuleHandleW(L"kernel32.dll");
    if (kernel == NULL) {
        return 1u;
    }
    symbol = GetProcAddress(kernel, "GetSystemCpuSetInformation");
    if (symbol == NULL) {
        return 1u;
    }
    memcpy(&query, &symbol, sizeof query);
    if (query(NULL, 0, &needed, GetCurrentProcess(), 0) && needed == 0) {
        return 1u;
    }
    if (needed == 0 || needed > (ULONG)sizeof report) {
        return 1u;
    }
    returned = needed;
    if (!query(
            (PSYSTEM_CPU_SET_INFORMATION)(void *)report,
            needed,
            &returned,
            GetCurrentProcess(),
            0)
        || returned == 0 || returned > (ULONG)sizeof report) {
        return 1u;
    }
    /* The process affinity mask is indexed by the group-relative logical
     * processor index, which is the index a CPU set carries, so the two are
     * comparable only within this thread's group. Without both the mask and
     * the group, every reported set counts. */
    filtered = GetProcessAffinityMask(
                   GetCurrentProcess(), &process_mask, &system_mask)
        && process_mask != 0
        && GetThreadGroupAffinity(GetCurrentThread(), &group);
    while (offset < returned) {
        const SYSTEM_CPU_SET_INFORMATION *entry =
            (const SYSTEM_CPU_SET_INFORMATION *)(const void *)(report + offset);
        unsigned char efficiency;
        unsigned index;
        if (entry->Size == 0 || entry->Size > returned - offset) {
            return 1u;
        }
        offset += entry->Size;
        if (entry->Type != CpuSetInformation) {
            continue;
        }
        if (filtered) {
            if (entry->CpuSet.Group != group.Group
                || entry->CpuSet.LogicalProcessorIndex
                    >= sizeof(DWORD_PTR) * 8u
                || ((process_mask
                        >> entry->CpuSet.LogicalProcessorIndex) & (DWORD_PTR)1)
                    == 0) {
                continue;
            }
        }
        efficiency = entry->CpuSet.EfficiencyClass;
        for (index = 0; index < levels; index += 1u) {
            if (seen[index] == efficiency) {
                break;
            }
        }
        if (index < levels) {
            continue;
        }
        if (levels == WF_PRIM_CPU_LEVEL_CEILING) {
            return WF_PRIM_CPU_LEVEL_CEILING;
        }
        seen[levels] = efficiency;
        levels += 1u;
    }
    return levels > 0u ? levels : 1u;
}

uint64_t wf_prim_monotonic_us(void) {
    LARGE_INTEGER frequency;
    LARGE_INTEGER counter;
    uint64_t ticks;
    uint64_t per_second;
    if (!QueryPerformanceFrequency(&frequency) || frequency.QuadPart <= 0
        || !QueryPerformanceCounter(&counter) || counter.QuadPart < 0) {
        return 0;
    }
    ticks = (uint64_t)counter.QuadPart;
    per_second = (uint64_t)frequency.QuadPart;
    /* Whole seconds first: the counter runs from boot and a plain
     * ticks * 1000000 overflows a 64-bit product on a long-lived machine. */
    return (ticks / per_second) * UINT64_C(1000000)
        + ((ticks % per_second) * UINT64_C(1000000)) / per_second;
}

#if defined(WF_PAR_TRACE)
/* The lane trace's clock; see prim.h. Behind the instrument's guard, so an
 * ordinary build of this file has the same bytes it had before it existed.
 * Same overflow care as the microsecond reading above. */
uint64_t wf_prim_monotonic_ns(void) {
    LARGE_INTEGER frequency;
    LARGE_INTEGER counter;
    uint64_t ticks;
    uint64_t per_second;
    if (!QueryPerformanceFrequency(&frequency) || frequency.QuadPart <= 0
        || !QueryPerformanceCounter(&counter) || counter.QuadPart < 0) {
        return 0;
    }
    ticks = (uint64_t)counter.QuadPart;
    per_second = (uint64_t)frequency.QuadPart;
    return (ticks / per_second) * UINT64_C(1000000000)
        + ((ticks % per_second) * UINT64_C(1000000000)) / per_second;
}
#endif

int wf_prim_setting_text(const char *name, char *buffer, size_t capacity) {
    DWORD written;
    if (name == NULL || buffer == NULL || capacity == 0
        || capacity > (size_t)MAXDWORD) {
        return -1;
    }
    buffer[0] = '\0';
    SetLastError(ERROR_SUCCESS);
    written = GetEnvironmentVariableA(name, buffer, (DWORD)capacity);
    if (written == 0) {
        buffer[0] = '\0';
        return GetLastError() == ERROR_ENVVAR_NOT_FOUND ? 0 : 1;
    }
    if (written >= (DWORD)capacity) {
        buffer[0] = '\0';
        return -1;
    }
    return 1;
}

__attribute__((weak)) void wf__floor_attach_thread(void) {}
void wf_prim_floor_attach(void) { wf__floor_attach_thread(); }
void wf_prim_yield(void) { SwitchToThread(); }
int wf_prim_wait_init(wf_prim_wait *wait) {
    InitializeSRWLock(&wait->lock);
    InitializeConditionVariable(&wait->signal);
    return 0;
}
void wf_prim_wait_destroy(wf_prim_wait *wait) { (void)wait; }
void wf_prim_wait_lock(wf_prim_wait *wait) { AcquireSRWLockExclusive(&wait->lock); }
void wf_prim_wait_unlock(wf_prim_wait *wait) { ReleaseSRWLockExclusive(&wait->lock); }
void wf_prim_wait_sleep(wf_prim_wait *wait) {
    if (!SleepConditionVariableSRW(&wait->signal, &wait->lock, INFINITE, 0)) abort();
}
void wf_prim_wait_signal(wf_prim_wait *wait) { WakeConditionVariable(&wait->signal); }
