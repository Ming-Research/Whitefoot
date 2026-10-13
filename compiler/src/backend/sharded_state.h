/* Shared shards [SHARE-1]: one header and separately locked value slots in
 * one pool allocation. These offsets are the generated code's private ABI. */
#ifndef WF_SHARDED_STATE_H
#define WF_SHARDED_STATE_H

#include <stdint.h>

#define WF_SHARDS_HEADER_SIZE 64u
#define WF_SHARDS_LEN_OFFSET 8u
#define WF_SHARDS_IDENTITY_OFFSET 16u
#define WF_SHARDS_STRIDE_OFFSET 24u
#define WF_SHARDS_LOCK_SIZE 16u
#define WF_SHARDS_SERVING_OFFSET 8u
#define WF_SHARDS_VALUE_OFFSET 16u

#if defined(__cplusplus)
extern "C" {
extern thread_local unsigned wf__map_user;
#else
extern _Thread_local unsigned wf__map_user;
#endif

/* Makes uninitialized value slots and initializes every lock. Generated
 * code calls its maker in index order before publishing the first handle.
 * Values have alignment at most 16. The ordinary shared share/release
 * functions operate on the count at offset zero; free follows last release
 * and the generated per-value cleanup. */
void *wf__shards_new(uint64_t value_bytes);
void wf__shards_free(void *object);
/* Slow part of the inlined ticket lock, after reserving ticket from the
 * first word. It never suspends the context or registers a guard watch. */
void wf__shards_wait(void *lock, uint64_t ticket);

#if defined(__cplusplus)
}
#endif

#endif
