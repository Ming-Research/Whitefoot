/* Shared by the backend executable test and the native Windows group. */
#include "completion/bridge.h"
#include <stddef.h>
#include <stdint.h>
#include <string.h>
int alignment_probe(void) {
    uint64_t before = wf__heap_in_use();
    if ((uintptr_t)wf__empty_window % WF_HEAP_MAX_ALIGNMENT != 0) return 1;
    for (uint64_t align = 16; align <= WF_HEAP_MAX_ALIGNMENT; align *= 2) {
        unsigned char *p = wf__heap_take_aligned(37, align);
        if (p == NULL || (uintptr_t)p % align != 0) return 2;
        if (wf__heap_in_use() != before + 37) return 3;
        memset(p, 93, 37);
        p = wf__heap_retake_aligned(p, 37, 96, align);
        if (p == NULL || (uintptr_t)p % align != 0) return 4;
        if (wf__heap_in_use() != before + 96) return 5;
        for (int i = 0; i < 37; ++i) if (p[i] != 93) return 6;
        p = wf__heap_retake_aligned(p, 96, 19, align);
        if (p == NULL || (uintptr_t)p % align != 0) return 7;
        for (int i = 0; i < 19; ++i) if (p[i] != 93) return 8;
        if (wf__heap_in_use() != before + 19) return 9;
        void *refused = wf__heap_retake_aligned(p, 19, 40, 8192);
        if (refused != NULL || wf__heap_in_use() != before + 19) return 10;
        wf__heap_give_aligned(p, 19, align);
        if (wf__heap_in_use() != before) return 11;
        p = wf__heap_retake_aligned((void *)wf__empty_window, align, align + 1, align);
        if (p == NULL || (uintptr_t)p % align != 0) return 12;
        for (uint64_t i = 0; i < align; ++i) if (p[i] != 0) return 13;
        if (wf__heap_in_use() != before + align + 1) return 14;
        wf__heap_give_aligned(p, align + 1, align);
        wf__heap_give_aligned((void *)wf__empty_window, align, align);
        if (wf__heap_in_use() != before) return 15;
        void *shared = wf__shared_new_aligned(37, align);
        unsigned char *state = (unsigned char *)shared + WF_SHARED_STATE_OFFSET;
        if ((uintptr_t)state % align != 0) return 16;
        uint64_t request = 37 + WF_SHARED_STATE_OFFSET + sizeof(void *) + align - 1;
        if (wf__heap_in_use() != before + wf__runtime_granted(request)) return 17;
        memset(state, 61, 37);
        wf__shared_share(shared);
        if (wf__shared_release(shared) || !wf__shared_release(shared)) return 18;
        wf__shared_free(shared);
        if (wf__heap_in_use() != before) return 19;
        void *frozen = wf__frozen_new_aligned(37, align);
        request = 37 + WF_FROZEN_HEADER_BYTES + align - 1;
        if ((uintptr_t)frozen % align != 0 ||
            wf__heap_in_use() != before + wf__runtime_granted(request)) return 20;
        memset(frozen, 61, 37);
        wf__frozen_share(frozen);
        if (wf__frozen_release(frozen) || !wf__frozen_release(frozen)) return 21;
        wf__frozen_free(frozen);
        if (wf__heap_in_use() != before) return 22;
    }
    if (wf__heap_take_aligned(1, 8192) != NULL) return 23;
    if (wf__heap_take_aligned(1, 24) != NULL) return 24;
    if (wf__heap_take_aligned(UINT64_MAX, 16) != NULL) return 25;
    return 0;
}
