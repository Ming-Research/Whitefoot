/* The cost of one call of wf_file_wait_readiness (completion/file_posix.c), the
 * runtime's one C frame over a page, whose prologue the stack-probe arguments
 * change. Every entry names one readable pipe and the timeout is zero, so the
 * poll answers at once: the cheapest call the function makes, which puts the
 * prologue's share of a call at its largest. One process times one batch of
 * calls and prints the nanoseconds per call. A call must answer at least one
 * entry; Linux answers every one, while macOS's poll folds entries that name
 * the same descriptor, which both variants of a comparison meet alike.
 *
 * usage: wait-bench COUNT CALLS
 * Built and run by wait-bench.sh; see README.md, "The cost of the probe
 * prologue". */
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>

#include "file_adapter.h"

static double now_ns(void) {
    struct timespec at;
    clock_gettime(CLOCK_MONOTONIC, &at);
    return (double)at.tv_sec * 1e9 + (double)at.tv_nsec;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s COUNT CALLS\n", argv[0]);
        return 2;
    }
    size_t count = (size_t)strtoul(argv[1], NULL, 10);
    unsigned long calls = strtoul(argv[2], NULL, 10);
    int fds[2];
    if (count == 0 || count > WF_FILE_READINESS_BATCH || calls == 0 || pipe(fds) != 0
        || write(fds[1], "x", 1) != 1) {
        fprintf(stderr, "cannot set up the benchmark\n");
        return 2;
    }
    wf_file_readiness *entries = calloc(count, sizeof(*entries));
    if (entries == NULL) {
        return 2;
    }
    for (size_t index = 0; index < count; index++) {
        entries[index].descriptor = fds[0];
        entries[index].events = WF_FILE_READABLE;
    }
    /* The first calls also touch the stack pages and the descriptor table. */
    for (unsigned long call = 0; call < calls / 10 + 1; call++) {
        int answered = wf_file_wait_readiness(entries, count, 0);
        if (answered < 1 || (size_t)answered > count) {
            fprintf(stderr, "the wait answered %d of %zu entries\n", answered, count);
            return 1;
        }
    }
    double begin = now_ns();
    for (unsigned long call = 0; call < calls; call++) {
        int answered = wf_file_wait_readiness(entries, count, 0);
        if (answered < 1 || (size_t)answered > count) {
            fprintf(stderr, "the wait answered %d of %zu entries\n", answered, count);
            return 1;
        }
    }
    printf("%.2f\n", (now_ns() - begin) / (double)calls);
    return 0;
}
