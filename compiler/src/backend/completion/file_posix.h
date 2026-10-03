#ifndef WHITEFOOT_COMPLETION_FILE_POSIX_H
#define WHITEFOOT_COMPLETION_FILE_POSIX_H

/*
 * The POSIX file leaf's own vocabulary: the rules that read a host `stat` mode
 * and a host open flag, and the WF_IO_NOCACHE policy's one host call.
 *
 * These were inline in `file_adapter.h` while that header was POSIX's alone.
 * They are here now because they name `<sys/stat.h>` and `<fcntl.h>`, and the
 * adapter's own header is shared with a platform that has neither.  Their two
 * readers are both POSIX leaves: `file_posix.c`, the adapter's per-operation
 * host switch, and `linux_io_uring.c`, which decides a ring-completed open's
 * kind with the same rule so that one open states one contract whichever
 * engine ran it.
 *
 * The address vocabulary this header used to hold is `socket_address.h`'s
 * now, because the Windows engines convert the same value against the same
 * layout and a second copy of that rule would be a twin of a shared unit.
 * What is left here is what genuinely differs: the POSIX open flags, the
 * POSIX kind rule, and the POSIX socket call.
 */

#include "contract.h"
#include "socket_address.h"

#include <fcntl.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

#if defined(__cplusplus)
extern "C" {
#endif

/* The one rule deciding whether an opened descriptor is the kind the
 * operation asked for.  Every POSIX engine answers with this function, so a
 * FIFO is refused identically whether the open ran on a helper thread, on the
 * joining thread itself, or on a kernel completion ring.  `file_mode` is the
 * host mode word of the descriptor that was actually opened, never of a path
 * inspected a second time. */
static inline enum wf_file_open_outcome wf_file_kind_outcome(
    enum wf_file_expected_kind expected,
    unsigned int file_mode
) {
    if (expected == WF_FILE_EXPECT_ANY
        || (expected == WF_FILE_EXPECT_REGULAR && S_ISREG(file_mode))
        || (expected == WF_FILE_EXPECT_DIRECTORY && S_ISDIR(file_mode))) {
        return WF_FILE_OPEN_SUCCEEDED;
    }
    return S_ISDIR(file_mode) ? WF_FILE_OPEN_IS_DIRECTORY
                              : WF_FILE_OPEN_OTHER_KIND;
}

/* The extra open flag a kind-checked open needs so that opening a waiting
 * facility such as a FIFO cannot block before the kind is known. */
static inline int wf_file_open_kind_flags(enum wf_file_expected_kind expected) {
    return expected == WF_FILE_EXPECT_REGULAR ? O_NONBLOCK : 0;
}

/* One socket of the family one portable address names, closed on exec.
 *
 * A socket made for a listen gets SO_REUSEADDR before its bind
 * (`file_posix.c`, `wf_socket_reuse_address`); a connect's socket needs no
 * option. */
static inline int wf_socket_open(const wf_socket_address *address) {
    int family = wf_socket_address_family(address);
#if defined(SOCK_CLOEXEC)
    return socket(family, SOCK_STREAM | SOCK_CLOEXEC, 0);
#else
    int descriptor = socket(family, SOCK_STREAM, 0);
    if (descriptor >= 0) {
        (void)fcntl(descriptor, F_SETFD, FD_CLOEXEC);
    }
    return descriptor;
#endif
}

/* WF_IO_NOCACHE: a target policy that makes a program's reads genuinely wait.
 *
 * This is runtime policy of the same class as WF_IO_HELPERS and WF_WORKERS,
 * not a language surface. No Whitefoot source names it, no accepted program
 * changes meaning under it, and no byte any read produces differs with it
 * set: it asks the host to keep this file's pages out of its cache, which
 * changes only how long a read takes. Absent — and any value other than the
 * exact text "1" — is today's behaviour exactly, with no host call made at
 * all.
 *
 * It exists because the file measurements this framework was tuned against
 * all ran with a warm page cache, where a read is a memory copy and a
 * completion model that overlaps waits has nothing to overlap. Measuring the
 * completion framework needs reads that wait on a device.
 *
 * Darwin gets F_NOCACHE, which is a mode of the descriptor: every read
 * through it bypasses the unified buffer cache for the whole life of the
 * open. Linux has no such per-descriptor mode, so it gets one
 * POSIX_FADV_DONTNEED of the whole file, which evicts what is cached at the
 * moment of the open. O_DIRECT is deliberately not used: it constrains
 * buffer address, offset, and length alignment, which would change the
 * program's own buffers and so change what is being measured.
 *
 * The setting is read once per process and cached. Two threads that race here
 * compute the same answer from the same environment, so the race is benign. */
static inline int wf_file_uncached_reads_requested(void) {
    /* 0 not yet decided, 1 asked for, 2 not asked for. */
    static _Atomic int decided;
    int state = atomic_load_explicit(&decided, memory_order_relaxed);
    if (state == 0) {
        const char *text = getenv("WF_IO_NOCACHE");
        state = (text != NULL && text[0] == '1' && text[1] == 0) ? 1 : 2;
        atomic_store_explicit(&decided, state, memory_order_relaxed);
    }
    return state == 1;
}

/* The one host call the policy makes, in one place so both POSIX engines make
 * the same one. A build may name a different function here to observe it; the
 * observing build is expected to perform the same host call. */
#if !defined(WF_FILE_UNCACHED_APPLY)
#define WF_FILE_UNCACHED_APPLY wf_file_uncached_apply_host
#else
extern void WF_FILE_UNCACHED_APPLY(int);
#endif

static inline void wf_file_uncached_apply_host(int descriptor) {
#if defined(__APPLE__)
    (void)fcntl(descriptor, F_NOCACHE, 1);
#elif defined(__linux__)
    (void)posix_fadvise(descriptor, 0, 0, POSIX_FADV_DONTNEED);
#else
    (void)descriptor;
#endif
}

/* Applied exactly once to each descriptor an open hands back, by whichever
 * engine produced it. A descriptor the kind check refuses never reaches here,
 * so the count of applications equals the count of successful opens. */
static inline void wf_file_apply_uncached_reads(int descriptor) {
    if (descriptor < 0 || !wf_file_uncached_reads_requested()) {
        return;
    }
    WF_FILE_UNCACHED_APPLY(descriptor);
}

#if defined(__cplusplus)
}
#endif

#endif
