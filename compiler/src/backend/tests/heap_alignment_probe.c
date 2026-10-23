/* Run by the native Windows group: the platform allocator, the runtime pool
 * and the frozen header give every block the 16-byte alignment target.rs
 * records, and the metered byte counts follow each request. */
#include "completion/bridge.h"
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#define ALIGNMENT 16u
int alignment_probe(void) {
    uint64_t before = wf__heap_in_use();
    if ((uintptr_t)wf__empty_window % ALIGNMENT != 0) return 1;
    unsigned char *p = wf__heap_take(37);
    if (p == NULL || (uintptr_t)p % ALIGNMENT != 0) return 2;
    if (wf__heap_in_use() != before + 37) return 3;
    memset(p, 93, 37);
    p = wf__heap_retake(p, 37, 96);
    if (p == NULL || (uintptr_t)p % ALIGNMENT != 0) return 4;
    if (wf__heap_in_use() != before + 96) return 5;
    for (int i = 0; i < 37; ++i) if (p[i] != 93) return 6;
    p = wf__heap_retake(p, 96, 19);
    if (p == NULL || (uintptr_t)p % ALIGNMENT != 0) return 7;
    for (int i = 0; i < 19; ++i) if (p[i] != 93) return 8;
    if (wf__heap_in_use() != before + 19) return 9;
    wf__heap_give(p, 19);
    if (wf__heap_in_use() != before) return 10;
    p = wf__heap_retake((void *)wf__empty_window, ALIGNMENT, ALIGNMENT + 1);
    if (p == NULL || (uintptr_t)p % ALIGNMENT != 0) return 11;
    for (uint64_t i = 0; i < ALIGNMENT; ++i) if (p[i] != 0) return 12;
    if (wf__heap_in_use() != before + ALIGNMENT + 1) return 13;
    wf__heap_give(p, ALIGNMENT + 1);
    wf__heap_give((void *)wf__empty_window, ALIGNMENT);
    if (wf__heap_in_use() != before) return 14;
    void *shared = wf__shared_new(37);
    unsigned char *state = (unsigned char *)shared + WF_SHARED_STATE_OFFSET;
    if (shared == NULL || (uintptr_t)state % ALIGNMENT != 0) return 15;
    memset(state, 61, 37);
    wf__shared_share(shared);
    if (wf__shared_release(shared) || !wf__shared_release(shared)) return 16;
    wf__shared_free(shared);
    if (wf__heap_in_use() != before) return 17;
    void *frozen = wf__frozen_new(37);
    if (frozen == NULL || (uintptr_t)frozen % ALIGNMENT != 0) return 18;
    memset(frozen, 61, 37);
    wf__frozen_share(frozen);
    if (wf__frozen_release(frozen) || !wf__frozen_release(frozen)) return 19;
    wf__frozen_free(frozen);
    if (wf__heap_in_use() != before) return 20;
    return 0;
}
