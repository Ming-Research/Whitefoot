/* Native-only independent H lowering reference; retire with qualified append. */
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct { uint64_t len, cap; void *payload; } Owner;
typedef struct { uint64_t words[32]; } Record;
_Static_assert(sizeof(Owner) == 24 && offsetof(Owner, payload) == 16,
               "frozen H descriptor ABI");
_Static_assert(sizeof(Record) == 256, "frozen record layout");
extern int64_t wf_resource_write(const char *, uint64_t);

/* Same heap record and partial-write loop as frozen H's exhaustion floor. */
static _Noreturn void exhausted(void) {
    const char *cursor = "{\"resource\":\"heap\"}\n";
    uint64_t remaining = 20;
    for (;;) {
        int64_t written = wf_resource_write(cursor, remaining);
        if (written == (int64_t)remaining || written <= 0) break;
        cursor += written;
        remaining -= (uint64_t)written;
    }
    abort();
}

/* Valid calls: len <= cap, len < 8193, qualified backing and initialized prefix.
   Full calls imply cap < 8193; larger-cap spare calls retain their backing.
   Cap zero uses H's non-owning anchor. No optimizer assumptions are added. */
#define APPEND(NAME, T, PLACE)                                               \
uint64_t NAME(void *storage, uint64_t seed) {                                 \
    Owner *owner = storage;                                                  \
    const uint64_t len = owner->len, cap = owner->cap;                       \
    T *payload = owner->payload;                                            \
    if (len == cap) {                                                       \
        uint64_t next_cap = cap ? cap * 2 : 1;                              \
        if (next_cap > 8193) next_cap = 8193;                               \
        const uint64_t new_bytes = next_cap * sizeof(T);                     \
        T *fresh;                                                           \
        if (cap == 0) {                                                     \
            fresh = malloc(new_bytes);                                     \
            if (!fresh) exhausted();                                       \
        } else if (len * sizeof(T) <= 2048) {                               \
            fresh = malloc(new_bytes);                                     \
            if (!fresh) exhausted();                                       \
            memcpy(fresh, payload, len * sizeof(T));                         \
            free(payload);                                                  \
        } else {                                                            \
            fresh = realloc(payload, new_bytes);                            \
            if (!fresh) exhausted();                                       \
        }                                                                   \
        owner->payload = fresh;                                             \
        owner->cap = next_cap;                                              \
        payload = fresh;                                                    \
    }                                                                       \
    PLACE;                                                                  \
    owner->len = len + 1;                                                   \
    return len + 1;                                                         \
}
APPEND(c_twin_word_append_one, uint64_t, payload[len] = seed)
APPEND(c_twin_record_append_one, Record,
       for (size_t word = 0; word < 32; ++word)
           payload[len].words[word] = seed + word)
