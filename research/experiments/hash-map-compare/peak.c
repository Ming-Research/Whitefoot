/* Peak heap observer for the memory cell. The Makefile renames the emitted
 * program's calls to wf__heap_take, wf__heap_retake and wf__heap_give to the
 * three functions below, and its main to wf_fixture_main. Each forwards to the
 * runtime's own function and then reads the heap counter that heap_in_use
 * reports [PRE-2], keeping its maximum. After the program returns, one more
 * row is printed: "peak", the maximum heap bytes in use, and the process's
 * maximum resident set in KiB. The program's own share of the counter moves
 * only inside these calls. Reading it after each one sees every value it
 * takes except one: the runtime grows a block smaller than
 * WF_HEAP_RETAKE_COPY_BELOW by taking the new block before giving the old one
 * back, so inside that call the counter passes the reading before it plus the
 * new size, which wf_peak_retake records itself. A larger block is resized in
 * place and counted as the change in its size, as the counter counts it. */
#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/resource.h>

extern int wf_fixture_main(int argc, char **argv);
extern void *wf__heap_take(uint64_t bytes);
extern void *wf__heap_retake(void *block, uint64_t old_bytes, uint64_t new_bytes);
extern void wf__heap_give(void *block, uint64_t bytes);
extern uint64_t wf__heap_in_use(void);
extern const unsigned char wf__empty_window[64];

/* compiler/src/backend/heap.c's threshold; the Makefile refuses to build when
 * the runtime defines another value. */
#define WF_HEAP_RETAKE_COPY_BELOW UINT64_C(1024)

/* The measured program is sequential and runs with one driver, so only one
 * thread calls these. */
static uint64_t peak;

static void note(void) {
    uint64_t now = wf__heap_in_use();
    if (now > peak) peak = now;
}

void *wf_peak_take(uint64_t bytes) {
    void *block = wf__heap_take(bytes);
    note();
    return block;
}

void *wf_peak_retake(void *block, uint64_t old_bytes, uint64_t new_bytes) {
    int copied = block != (const void *)wf__empty_window && old_bytes < WF_HEAP_RETAKE_COPY_BELOW;
    uint64_t passing = wf__heap_in_use() + new_bytes;
    void *moved = wf__heap_retake(block, old_bytes, new_bytes);
    if (copied && moved != NULL && passing > peak) peak = passing;
    note();
    return moved;
}

void wf_peak_give(void *block, uint64_t bytes) {
    wf__heap_give(block, bytes);
    note();
}

int main(int argc, char **argv) {
    int status = wf_fixture_main(argc, argv);
    struct rusage usage;
    long resident = getrusage(RUSAGE_SELF, &usage) == 0 ? usage.ru_maxrss : -1;
    printf("peak\t%" PRIu64 "\t%ld\n", peak, resident);
    return status;
}
