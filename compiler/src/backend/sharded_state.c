/* A sharded object's slots and first-come locks. The completion bridge
 * compiles this unit in; its pool grants at least 512-byte aligned blocks
 * and counts the whole grant in the ordinary heap reading. */
#include <stddef.h>
#include <stdatomic.h>
#include <stdint.h>
#include <string.h>

#include "sharded_state.h"
#include "completion/bridge.h"

typedef struct {
    _Atomic uint64_t handles;
    uint64_t len;
    uint64_t identity;
    uint64_t stride;
    uint64_t pool_bytes;
    unsigned char padding[24];
} wf_shards;

typedef struct {
    _Atomic uint64_t next;
    _Atomic uint64_t serving;
} wf_shard_lock;

_Static_assert(sizeof(wf_shards) == WF_SHARDS_HEADER_SIZE, "one shards header line");
_Static_assert(offsetof(wf_shards, handles) == 0u, "ordinary shared handle count");
_Static_assert(offsetof(wf_shards, len) == WF_SHARDS_LEN_OFFSET, "shards length ABI");
_Static_assert(offsetof(wf_shards, identity) == WF_SHARDS_IDENTITY_OFFSET, "shards identity ABI");
_Static_assert(offsetof(wf_shards, stride) == WF_SHARDS_STRIDE_OFFSET, "shards stride ABI");
_Static_assert(sizeof(wf_shard_lock) == WF_SHARDS_LOCK_SIZE, "shard ticket lock ABI");
_Static_assert(offsetof(wf_shard_lock, serving) == WF_SHARDS_SERVING_OFFSET, "shard serving ABI");
_Static_assert(WF_SHARDS_VALUE_OFFSET == sizeof(wf_shard_lock), "value follows the lock");

void *wf__shards_new(uint64_t value_bytes) {
    uint64_t drivers = wf__context_driver_count();
    uint64_t len = drivers == 1u ? 1u : 2u * drivers;
    if (value_bytes > SIZE_MAX - WF_SHARDS_LOCK_SIZE - (WF_SHARDS_HEADER_SIZE - 1u))
        wf__runtime_exhausted();
    uint64_t stride = (WF_SHARDS_LOCK_SIZE + value_bytes + WF_SHARDS_HEADER_SIZE - 1u)
        / WF_SHARDS_HEADER_SIZE * WF_SHARDS_HEADER_SIZE;
    if (len == 0u || stride > (SIZE_MAX - WF_SHARDS_HEADER_SIZE) / len)
        wf__runtime_exhausted();
    uint64_t bytes = WF_SHARDS_HEADER_SIZE + len * stride;
    wf_shards *object = wf__runtime_take(bytes);
    atomic_init(&object->handles, 1u);
    object->len = len;
    object->identity = (uint64_t)(uintptr_t)object;
    object->stride = stride;
    object->pool_bytes = wf__runtime_granted(bytes);
    memset(object->padding, 0, sizeof(object->padding));
    for (uint64_t i = 0; i < len; ++i) {
        wf_shard_lock *lock = (wf_shard_lock *)((unsigned char *)object
            + WF_SHARDS_HEADER_SIZE + i * stride);
        atomic_init(&lock->next, 0u);
        atomic_init(&lock->serving, 0u);
    }
    return object;
}

void wf__shards_free(void *object) {
    wf_shards *shards = object;
    wf__runtime_give(shards, shards->pool_bytes);
}

void wf__shards_wait(void *lock, uint64_t ticket) {
    wf_shard_lock *shard = lock;
    while (atomic_load_explicit(&shard->serving, memory_order_acquire) != ticket)
        wf__runtime_yield();
}
