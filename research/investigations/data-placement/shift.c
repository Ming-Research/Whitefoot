/* Data-placement moves for research/investigations/data-placement. This unit
 * is linked into a regression image and WF_DATA_SHIFT selects a move at run
 * time, so the moved and the unmoved arm run one image. A move happens in a
 * constructor, before the floor creates the entry thread, and changes no code
 * address. */
#define _GNU_SOURCE
#include <pthread.h>
#include <semaphore.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

static sem_t started;
static void *volatile kept[2];

static void *idle(void *unused) {
    (void)unused;
    /* A thread's first allocation gives it a heap of its own, as the stop
     * receiver's did. */
    kept[0] = malloc(64);
    sem_post(&started);
    for (;;) pause();
}

static void fail(const char *what) {
    fprintf(stderr, "WF_DATA_SHIFT: %s\n", what);
    _exit(2);
}

__attribute__((constructor)) static void shift(void) {
    const char *move = getenv("WF_DATA_SHIFT");
    if (!move || !*move) return;
    if (!strncmp(move, "map:", 4)) {
        /* One idle mapping of the given size ahead of every later one. */
        char *end;
        unsigned long long bytes = strtoull(move + 4, &end, 10);
        if (end == move + 4 || *end) fail("map:BYTES expected");
        if (bytes && mmap(NULL, bytes, PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE,
                          -1, 0) == MAP_FAILED)
            fail("mapping failed");
    } else if (!strcmp(move, "thread")) {
        /* One idle thread with its own heap, started and settled before the
         * entry exists: the stop receiver's effect on the mappings. */
        pthread_t thread;
        if (sem_init(&started, 0, 0) || pthread_create(&thread, NULL, idle, NULL))
            fail("thread failed");
        while (sem_wait(&started)) {}
    } else if (!strcmp(move, "touch")) {
        /* The launching thread's heap in use before the entry exists: one
         * block the allocator maps and one it takes from the program break. */
        kept[0] = malloc((size_t)1 << 20);
        kept[1] = malloc((size_t)1 << 16);
        if (!kept[0] || !kept[1]) fail("allocation failed");
    } else {
        fail("unknown move");
    }
}
