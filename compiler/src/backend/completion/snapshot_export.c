#define _GNU_SOURCE
#include "snapshot_export.h"
#include "fork_capsule.h"

#if defined(__linux__) && defined(__x86_64__) && defined(__LP64__) && defined(__GLIBC__)
#include "bridge.h"
#include "contract.h"
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

/* The whole held map is immutable in the child. Read its private layout
 * directly in this restricted unit, leaving normal map instrumentation on. */
#define WF_CMAP_HOST_FIELDS wf_watch_list watch;
#include "../concurrent_map_layout.h"
#undef WF_CMAP_HOST_FIELDS

int wf_snapshot_map_next(const void *image, uint64_t *cursor,
    uint64_t tag_offset, uint32_t tag_width, uint64_t none_tag,
    const unsigned char **key, uint64_t *length, const void **slot) {
    const wf_cmap *map = image;
    const table *t = atomic_load_explicit(&map->current, memory_order_relaxed);
    while (*cursor < t->capacity) {
        cell *c = &t->cells[(*cursor)++];
        uint64_t k = atomic_load_explicit(&c->key, memory_order_relaxed) & KEY_MASK;
        if (k == EMPTY || k == REMOVED) continue;
        const node *n = (const node *)(uintptr_t)
            (atomic_load_explicit(&c->value, memory_order_relaxed) & ADDRESS_MASK);
        uint64_t offset = sizeof(node) + n->length;
        offset = (offset + map->slot_align - 1) / map->slot_align * map->slot_align;
        const unsigned char *s = (const unsigned char *)n + offset;
        uint64_t tag = 0;
        for (uint32_t i = 0; i < tag_width; ++i)
            tag |= (uint64_t)s[tag_offset + i] << (8u * i);
        if (tag_width == 1) tag &= 1u;
        if (tag == none_tag) continue;
        *key = n->bytes;
        *length = n->length;
        *slot = s;
        return 1;
    }
    return 0;
}

enum { SNAP_SCRATCH = 1, SNAP_OUTPUT, SNAP_LENGTH, SNAP_PIPE, SNAP_SETUP };
typedef struct snap_frame {
    const wf_snapshot_type *type;
    const unsigned char *data;
    uint64_t at, length, state;
    const wf_snapshot_field *fields;
} snap_frame;
typedef struct snap_parse_frame {
    const wf_snapshot_type *type;
    const wf_snapshot_field *fields;
    uint64_t remaining, state, number;
    uint64_t accumulator;
    unsigned bytes;
} snap_parse_frame;

struct wf_snapshot_owner {
    wf_fork_capsule capsule;
    wf_fork_capsule_job job;
    struct wf_snapshot_owner *next;
    const void *map;
    const wf_snapshot_type *value;
    uint64_t quota, cut, tag_offset, none_tag, value_offset;
    uint32_t tag_width;
    int data_read, data_write, capture_error;
    uint64_t delivered, discarded, total, completed;
    uint64_t parser_depth, parser_capacity;
    /* Outer framing state and little-endian field accumulator. */
    uint64_t key_left, chunk_left, field, accumulator, record_cut;
    uint64_t value_scalar_left;
    uint32_t outer, field_left, body_started, value_done;
    uint32_t eof, failed, malformed, terminal;
    uint32_t previous_chunk;
    wf_snapshot_outcome outcome;
    unsigned char chunk[4096];
    _Alignas(16) unsigned char stack[32768];
};
_Static_assert(sizeof(struct wf_snapshot_owner) <= 65536,
               "snapshot fixed control plus private child stack bound");

static _Thread_local uint64_t snap_saved_mask;
static _Thread_local unsigned snap_atomic_depth;
static _Thread_local wf_snapshot_owner *snap_pending;

static uint64_t snap_u64(const void *data) {
    const unsigned char *p = data;
    uint64_t n = 0;
    for (unsigned i = 0; i < 8; ++i) n |= (uint64_t)p[i] << (8u * i);
    return n;
}
static const unsigned char *snap_pointer(const void *data) {
    return (const unsigned char *)(uintptr_t)snap_u64(data);
}
static uint64_t snap_tag(const void *data, uint64_t width) {
    const unsigned char *p = data;
    uint64_t n = 0;
    for (uint64_t i = 0; i < width; ++i) n |= (uint64_t)p[i] << (8u * i);
    return width == 1 ? n & 1u : n;
}

void wf_snapshot_atomic_begin(void) {
    if (snap_atomic_depth == 0) {
        const uint64_t all = UINT64_MAX;
        long error = wf_fork_capsule_raw(SYS_rt_sigprocmask, SIG_SETMASK,
            (long)&all, (long)&snap_saved_mask, sizeof(all), 0, 0);
        /* This qualified kernel operation has no resource-dependent failure. */
        if (error < 0) abort();
        snap_atomic_depth = 1;
    }
}

void wf_snapshot_discard(wf_snapshot_owner *owner) {
    if (owner == NULL) return;
    wf_fork_capsule_dispose(&owner->capsule);
    if (owner->data_read >= 0) (void)close(owner->data_read);
    if (owner->data_write >= 0) (void)close(owner->data_write);
    (void)munmap(owner, sizeof(*owner));
}

void wf_snapshot_atomic_end(void) {
    if (snap_atomic_depth == 0) abort();
    snap_atomic_depth = 0;
    wf_snapshot_owner *pending = snap_pending;
    snap_pending = NULL;
    while (pending != NULL) {
        wf_snapshot_owner *next = pending->next;
        int error = wf_fork_capsule_parent_start_owned(&pending->capsule, &pending->job);
        if (pending->capture_error) wf_snapshot_discard(pending);
        else {
            /* Valid prepared sequencing cannot lose an already created child. */
            if (error != 0) abort();
            (void)close(pending->data_write);
            pending->data_write = -1;
        }
        pending = next;
    }
    (void)wf_fork_capsule_raw(SYS_rt_sigprocmask, SIG_SETMASK,
        (long)&snap_saved_mask, 0, sizeof(uint64_t), 0, 0);
}

typedef struct snap_writer {
    wf_snapshot_owner *owner;
    int output, error;
    uint64_t written, records;
    size_t used;
    unsigned char *buffer;
} snap_writer;

/* Raw-only child frontier; no libc errno, memcpy, allocator or TLS. */
static void snap_write(snap_writer *writer, const unsigned char *p, size_t n) {
    if (writer->error != 0) return;
    if ((uint64_t)n > writer->owner->quota - writer->written) {
        writer->error = SNAP_OUTPUT;
        return;
    }
    while (n != 0) {
        long amount = wf_fork_capsule_raw(SYS_write, writer->output,
                                         (long)p, n, 0, 0, 0);
        if (amount == -EINTR) continue;
        if (amount <= 0) { writer->error = SNAP_PIPE; return; }
        writer->written += (uint64_t)amount;
        p += amount;
        n -= (size_t)amount;
    }
}
static void snap_word(snap_writer *writer, uint64_t number, unsigned width) {
    unsigned char bytes[8];
    for (unsigned i = 0; i < width; ++i) bytes[i] = (unsigned char)(number >> (8u * i));
    snap_write(writer, bytes, width);
}
static void snap_flush(snap_writer *writer) {
    if (writer->used == 0 || writer->error != 0) return;
    const unsigned char marker = 2;
    snap_write(writer, &marker, 1);
    snap_word(writer, writer->used, 4);
    snap_write(writer, writer->buffer, writer->used);
    writer->used = 0;
}
static void snap_body(snap_writer *writer, const unsigned char *bytes, uint64_t n) {
    while (n != 0 && writer->error == 0) {
        writer->buffer[writer->used++] = *bytes++;
        --n;
        if (writer->used == 4096) snap_flush(writer);
    }
}
static void snap_body_word(snap_writer *writer, uint64_t n, unsigned width) {
    unsigned char bytes[8];
    for (unsigned i = 0; i < width; ++i) bytes[i] = (unsigned char)(n >> (8u * i));
    snap_body(writer, bytes, width);
}

static const unsigned char *snap_element(const wf_snapshot_type *type,
                                        const unsigned char *data, uint64_t i) {
    if (type->shape == WF_SNAPSHOT_SEGMENTS) {
        uint64_t segments = snap_u64(data);
        uint64_t header = (segments + 2) * 8;
        uint64_t align = type->element->alignment;
        header = (header + align - 1) / align * align;
        return data + header + i * type->element->size;
    }
    if (type->shape == WF_SNAPSHOT_PAGED) {
        const unsigned char *page = snap_pointer(data + type->data_offset
                                               + (i / type->page_elements) * 8);
        return page + (i % type->page_elements) * type->element->size;
    }
    if (type->shape == WF_SNAPSHOT_RING) {
        uint64_t cap = type->capacity_offset == UINT64_MAX
            ? type->fixed_capacity : snap_u64(data + type->capacity_offset);
        uint64_t head = snap_u64(data + type->head_offset);
        /* Avoid head+i overflow, although both are below cap. */
        i = i >= cap - head ? i - (cap - head) : head + i;
    }
    return data + type->data_offset + i * type->element->size;
}
static uint64_t snap_length(const wf_snapshot_type *type, const unsigned char *data) {
    if (type->shape == WF_SNAPSHOT_ARRAY && type->length_offset == UINT64_MAX)
        return type->fixed_capacity;
    if (type->shape == WF_SNAPSHOT_SEGMENTS) {
        uint64_t count = snap_u64(data);
        return snap_u64(data + 8 + count * 8);
    }
    return snap_u64(data + type->length_offset);
}
static int snap_push(snap_writer *writer, snap_frame *stack, size_t *depth,
                     size_t capacity, const wf_snapshot_type *type,
                     const unsigned char *data, uint64_t state) {
    if (*depth == capacity) { writer->error = SNAP_SCRATCH; return 0; }
    snap_frame *frame = &stack[(*depth)++];
    frame->type = type;
    frame->data = data;
    frame->state = state;
    frame->at = frame->length = 0;
    frame->fields = NULL;
    return 1;
}

static uint64_t snap_ordered_count(snap_writer *writer,
    const wf_snapshot_type *map_type, const unsigned char *map,
    void *arena, size_t bytes) {
    const wf_snapshot_field *root = &map_type->fields[0];
    const unsigned char *root_data = map + root->offset;
    if (snap_length(root->type, root_data) == 0) return 0;
    snap_frame *stack = arena;
    size_t depth = 0, capacity = bytes / sizeof(snap_frame);
    uint64_t count = 0;
    const wf_snapshot_type *node_type = map_type->element;
    const unsigned char *box = snap_element(root->type, root_data, 0);
    snap_push(writer, stack, &depth, capacity, node_type, snap_pointer(box), 0);
    while (depth != 0 && writer->error == 0) {
        snap_frame *frame = &stack[depth - 1];
        const wf_snapshot_field *entries = &node_type->fields[1];
        const unsigned char *entry_data = frame->data + entries->offset;
        if (frame->state == 0) {
            frame->length = snap_length(entries->type, entry_data);
            if (frame->length > UINT64_MAX - count) { writer->error = SNAP_LENGTH; break; }
            count += frame->length;
            frame->state = 1;
            const wf_snapshot_field *leading = &node_type->fields[0];
            const unsigned char *child = frame->data + leading->offset;
            if (snap_length(leading->type, child) != 0) {
                box = snap_element(leading->type, child, 0);
                snap_push(writer, stack, &depth, capacity, node_type, snap_pointer(box), 0);
            }
            continue;
        }
        if (frame->at == frame->length) { --depth; continue; }
        const wf_snapshot_type *entry = entries->type->element;
        const unsigned char *item = snap_element(entries->type, entry_data, frame->at++);
        const wf_snapshot_field *after = &entry->fields[1];
        const unsigned char *child = item + after->offset;
        if (snap_length(after->type, child) != 0) {
            box = snap_element(after->type, child, 0);
            snap_push(writer, stack, &depth, capacity, node_type, snap_pointer(box), 0);
        }
    }
    return count;
}

/* state 100 walks an OrderedNode without exposing its physical struct. */
static void snap_encode(snap_writer *writer, const wf_snapshot_type *root,
                         const unsigned char *data, void *arena, size_t arena_size) {
    snap_frame *stack = arena;
    size_t depth = 0, capacity = arena_size / sizeof(snap_frame);
    if (!snap_push(writer, stack, &depth, capacity, root, data, 0)) return;
    while (depth != 0 && writer->error == 0) {
        snap_frame *frame = &stack[depth - 1];
        const wf_snapshot_type *type = frame->type;
        if (frame->state >= 100) {
            const wf_snapshot_field *leading = &type->fields[0];
            const wf_snapshot_field *entries = &type->fields[1];
            const unsigned char *entry_data = frame->data + entries->offset;
            if (frame->state == 100) {
                frame->length = snap_length(entries->type, entry_data);
                frame->state = 101;
                const unsigned char *lead = frame->data + leading->offset;
                if (snap_length(leading->type, lead) != 0) {
                    const unsigned char *box = snap_element(leading->type, lead, 0);
                    snap_push(writer, stack, &depth, capacity, type, snap_pointer(box), 100);
                }
                continue;
            }
            if (frame->at == frame->length) { --depth; continue; }
            const wf_snapshot_type *entry = entries->type->element;
            const unsigned char *item = snap_element(entries->type, entry_data, frame->at);
            if (frame->state == 101) {
                frame->state = 102;
                const wf_snapshot_field *pair = &entry->fields[0];
                /* Logical pair omits the pair struct tag/count. */
                const wf_snapshot_field *value = &pair->type->fields[1];
                const wf_snapshot_field *key = &pair->type->fields[0];
                snap_push(writer, stack, &depth, capacity, value->type,
                          item + pair->offset + value->offset, 0);
                snap_push(writer, stack, &depth, capacity, key->type,
                          item + pair->offset + key->offset, 0);
            } else {
                frame->state = 101;
                ++frame->at;
                const wf_snapshot_field *after = &entry->fields[1];
                const unsigned char *child = item + after->offset;
                if (snap_length(after->type, child) != 0) {
                    const unsigned char *box = snap_element(after->type, child, 0);
                    snap_push(writer, stack, &depth, capacity, type, snap_pointer(box), 100);
                }
            }
            continue;
        }
        if (frame->state == 0) {
            snap_body_word(writer, type->tag, 1);
            frame->state = 1;
            switch (type->shape) {
            case WF_SNAPSHOT_SCALAR:
                snap_body(writer, frame->data, type->width);
                --depth;
                continue;
            case WF_SNAPSHOT_BOX:
                frame->type = type->element;
                frame->data = snap_pointer(frame->data);
                frame->state = 0;
                continue;
            case WF_SNAPSHOT_STRUCT:
                frame->length = type->count;
                frame->fields = type->fields;
                snap_body_word(writer, type->count, 8);
                break;
            case WF_SNAPSHOT_ENUM: {
                uint64_t tag = snap_tag(frame->data, type->width);
                if (tag >= type->count) { writer->error = SNAP_LENGTH; break; }
                frame->length = type->variants[tag].count;
                frame->fields = type->variants[tag].fields;
                snap_body_word(writer, tag, 8);
                snap_body_word(writer, frame->length, 8);
                break;
            }
            case WF_SNAPSHOT_HASH: {
                const wf_snapshot_field *cells = &type->fields[0];
                frame->data = snap_pointer(frame->data + cells->offset);
                frame->length = snap_length(cells->type->element, frame->data);
                uint64_t pairs = 0;
                for (uint64_t i = 0; i < frame->length; ++i) {
                    const unsigned char *bucket = snap_element(cells->type->element, frame->data, i);
                    if (snap_tag(bucket, type->element->width) == 2) ++pairs;
                }
                snap_body_word(writer, pairs, 8);
                break;
            }
            case WF_SNAPSHOT_ORDERED: {
                const wf_snapshot_field *root_field = &type->fields[0];
                /* Counting uses the still-free arena suffix, preserving the
                 * outer value walk's frames. No data copy or callback. */
                size_t used = depth * sizeof(snap_frame);
                uint64_t pairs = snap_ordered_count(writer, type, frame->data,
                    (unsigned char *)arena + used, arena_size - used);
                snap_body_word(writer, pairs, 8);
                const unsigned char *root_data = frame->data + root_field->offset;
                --depth;
                if (snap_length(root_field->type, root_data) != 0) {
                    const unsigned char *box = snap_element(root_field->type, root_data, 0);
                    snap_push(writer, stack, &depth, capacity, type->element,
                              snap_pointer(box), 100);
                }
                continue;
            }
            default:
                frame->length = snap_length(type, frame->data);
                snap_body_word(writer, frame->length, 8);
                break;
            }
        }
        if (frame->at == frame->length) { --depth; continue; }
        if (type->shape == WF_SNAPSHOT_HASH) {
            const wf_snapshot_type *slots = type->fields[0].type->element;
            const unsigned char *bucket = snap_element(slots, frame->data, frame->at++);
            const wf_snapshot_type *slot = type->element;
            if (snap_tag(bucket, slot->width) != 2) continue;
            const wf_snapshot_field *fields = slot->variants[2].fields;
            snap_push(writer, stack, &depth, capacity, fields[1].type,
                      bucket + fields[1].offset, 0);
            snap_push(writer, stack, &depth, capacity, fields[0].type,
                      bucket + fields[0].offset, 0);
        } else if (frame->fields != NULL) {
            const wf_snapshot_field *field = &frame->fields[frame->at++];
            snap_push(writer, stack, &depth, capacity, field->type,
                      frame->data + field->offset, 0);
        } else {
            const unsigned char *item = snap_element(type, frame->data, frame->at++);
            snap_push(writer, stack, &depth, capacity, type->element, item, 0);
        }
    }
}

static int snap_child(const void *captured, void *arena, size_t arena_size,
                       int output, int status, int setup_error) {
    wf_snapshot_owner *owner = (wf_snapshot_owner *)captured;
    const size_t align = _Alignof(snap_frame);
    size_t overhead = (sizeof(snap_writer) + 4096 + align - 1) / align * align;
    uint64_t written = 0, records = 0;
    int error = setup_error ? SNAP_SETUP : SNAP_SCRATCH;
    if (setup_error != 0 || arena_size < overhead) goto report;
    snap_writer *writer = arena;
    writer->owner = owner;
    writer->output = output;
    writer->error = 0;
    writer->written = writer->records = writer->used = 0;
    writer->buffer = (unsigned char *)arena + sizeof(*writer);
    void *frames = (unsigned char *)arena + overhead;
    size_t frame_bytes = arena_size - overhead;
    const unsigned char magic[8] = {'W','F','S','N','A','P','0','1'};
    snap_write(writer, magic, 8);
    snap_word(writer, owner->cut, 8);
    uint64_t cursor = 0;
    const unsigned char *key;
    const void *slot;
    uint64_t length;
    while (writer->error == 0 && wf_snapshot_map_next(owner->map, &cursor,
        owner->tag_offset, owner->tag_width, owner->none_tag, &key, &length, &slot)) {
        snap_word(writer, 1, 1);
        snap_word(writer, length, 8);
        snap_word(writer, owner->cut, 8);
        snap_body(writer, key, length);
        snap_encode(writer, owner->value, (const unsigned char *)slot + owner->value_offset,
                    frames, frame_bytes);
        snap_flush(writer);
        snap_word(writer, 3, 1);
        if (writer->error == 0) {
            if (writer->records == UINT64_MAX) writer->error = SNAP_LENGTH;
            else ++writer->records;
        }
    }
    snap_word(writer, 255, 1);
    snap_word(writer, writer->records, 8);
    written = writer->written;
    records = writer->records;
    error = writer->error;
report:;
    unsigned char result[32] = {'W','F','S','S','T','A','T','1'};
    result[8] = (unsigned char)error;
    for (unsigned i = 0; i < 8; ++i) {
        result[16 + i] = (unsigned char)(written >> (8u * i));
        result[24 + i] = (unsigned char)(records >> (8u * i));
    }
    long amount;
    do { amount = wf_fork_capsule_raw(SYS_write, status, (long)result, 32, 0, 0, 0); }
    while (amount == -EINTR);
    return amount == 32 ? 0 : 1;
}

wf_snapshot_owner *wf_snapshot_prepare(uint64_t scratch, uint64_t output,
                                       uint32_t *error) {
    *error = 0;
    wf_snapshot_owner *owner = mmap(NULL, sizeof(*owner), PROT_READ | PROT_WRITE,
                                    MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (owner == MAP_FAILED) { *error = 1; return NULL; }
    owner->data_read = owner->data_write = -1;
    owner->capsule.result_read = owner->capsule.result_write = -1;
    owner->job.result_read = owner->job.pidfd = -1;
    int pipe[2];
    if (pipe2(pipe, O_CLOEXEC) != 0) {
        *error = errno == ENOMEM ? 1u : 2u;
        wf_snapshot_discard(owner);
        return NULL;
    }
    owner->data_read = pipe[0];
    owner->data_write = pipe[1];
    int capacity = fcntl(pipe[1], F_GETPIPE_SZ);
    if (capacity < 0 || (capacity > 65536
        && fcntl(pipe[1], F_SETPIPE_SZ, 65536) < 0)) {
        *error = errno == ENOMEM ? 1u : 2u;
        wf_snapshot_discard(owner);
        return NULL;
    }
    int refused = wf_fork_capsule_prepare_source(&owner->capsule, pipe[1],
        (size_t)scratch, owner->stack, sizeof(owner->stack), snap_child);
    if (refused != 0) {
        *error = refused == ENOMEM ? 1u : 2u;
        wf_snapshot_discard(owner);
        return NULL;
    }
    owner->quota = output;
    owner->parser_capacity = scratch / sizeof(snap_parse_frame);
    return owner;
}

wf_snapshot_owner *wf_snapshot_capture(wf_snapshot_owner *owner, const void *map,
    const uint64_t *cut, const wf_snapshot_type *value, uint64_t tag_offset,
    uint32_t tag_width, uint64_t none_tag, uint64_t value_offset) {
    /* Held call placement stays on a physical bridge executor. Its loop
     * outlives context and driver-role handoffs until launcher completion;
     * source linearity requires every job to be reaped before that point.
     * The fork's parent-death identity is this physical thread, not a role. */
    if (snap_atomic_depth == 0) abort();
    owner->map = map;
    owner->cut = *cut;
    owner->value = value;
    owner->tag_offset = tag_offset;
    owner->tag_width = tag_width;
    owner->none_tag = none_tag;
    owner->value_offset = value_offset;
    if (wf_fork_capsule_adopt_blocked(&owner->capsule, snap_saved_mask) != 0) abort();
    pid_t pid = wf_fork_capsule_capture_held(&owner->capsule, owner, NULL);
    owner->capture_error = pid < 0;
    owner->next = snap_pending;
    snap_pending = owner;
    return pid < 0 ? NULL : owner;
}

static void snap_parse_push(wf_snapshot_owner *owner, const wf_snapshot_type *type) {
    if (owner->parser_depth == owner->parser_capacity) { owner->failed = 1; return; }
    snap_parse_frame *frames = owner->capsule.scratch;
    snap_parse_frame *frame = &frames[owner->parser_depth++];
    frame->type = type;
    frame->fields = NULL;
    frame->remaining = frame->state = frame->number = frame->accumulator = 0;
    frame->bytes = 0;
}

static const wf_snapshot_type *snap_pair_type(const wf_snapshot_type *type, unsigned n) {
    if (type->shape == WF_SNAPSHOT_HASH)
        return type->element->variants[2].fields[n].type;
    const wf_snapshot_type *entry = type->element->fields[1].type->element;
    return entry->fields[0].type->fields[n].type;
}

/* Complete zero-byte transitions before consuming the next body octet. */
static void snap_parse_advance(wf_snapshot_owner *owner) {
    snap_parse_frame *frames = owner->capsule.scratch;
    while (owner->parser_depth != 0 && !owner->failed && !owner->malformed) {
        snap_parse_frame *frame = &frames[owner->parser_depth - 1];
        if (frame->state != 4) break;
        if (frame->remaining == 0) { --owner->parser_depth; continue; }
        /* Delay a pending child until its next input byte. This permits a
         * frame-exact empty aggregate and lets the next read report actual
         * workspace exhaustion before writing the caller's window. */
        if (owner->parser_depth == owner->parser_capacity) break;
        const wf_snapshot_type *next;
        if (frame->fields != NULL) next = frame->fields[frame->number++].type;
        else if (frame->type->shape == WF_SNAPSHOT_HASH
                 || frame->type->shape == WF_SNAPSHOT_ORDERED)
            next = snap_pair_type(frame->type, (unsigned)(frame->number++ & 1));
        else next = frame->type->element;
        --frame->remaining;
        snap_parse_push(owner, next);
    }
    if (owner->parser_depth == 0) owner->value_done = 1;
}

static void snap_parse_value(wf_snapshot_owner *owner, unsigned char byte) {
    if (!owner->body_started) {
        owner->body_started = 1;
        snap_parse_push(owner, owner->value);
    }
    snap_parse_advance(owner);
    if (owner->value_done || owner->failed) { owner->malformed = 1; return; }
    snap_parse_frame *frames = owner->capsule.scratch;
    snap_parse_frame *frame = &frames[owner->parser_depth - 1];
    const wf_snapshot_type *type = frame->type;
    if (frame->state == 0) {
        if (byte != type->tag) { owner->malformed = 1; return; }
        if (type->shape == WF_SNAPSHOT_SCALAR) {
            frame->remaining = type->width;
            frame->state = 5;
            if (frame->remaining == 0) --owner->parser_depth;
        } else if (type->shape == WF_SNAPSHOT_BOX) {
            frame->type = type->element;
        } else {
            frame->state = type->shape == WF_SNAPSHOT_ENUM ? 2 : 1;
            frame->bytes = 8;
        }
    } else if (frame->state == 5) {
        if (--frame->remaining == 0) --owner->parser_depth;
    } else {
        frame->accumulator |= (uint64_t)byte << (8u * (8u - frame->bytes));
        if (--frame->bytes != 0) return;
        uint64_t number = frame->accumulator;
        frame->accumulator = 0;
        if (frame->state == 2) {
            if (number >= type->count) { owner->malformed = 1; return; }
            frame->number = number;
            frame->state = 3;
            frame->bytes = 8;
            return;
        }
        if (type->shape == WF_SNAPSHOT_STRUCT) {
            if (number != type->count) { owner->malformed = 1; return; }
            frame->fields = type->fields;
        } else if (frame->state == 3) {
            if (number != type->variants[frame->number].count) {
                owner->malformed = 1; return;
            }
            frame->fields = type->variants[frame->number].fields;
        } else if (type->shape == WF_SNAPSHOT_HASH || type->shape == WF_SNAPSHOT_ORDERED) {
            if (number > UINT64_MAX / 2) { owner->malformed = 1; return; }
            number *= 2;
        } else if (type->shape == WF_SNAPSHOT_ARRAY && type->length_offset == UINT64_MAX) {
            if (number != type->fixed_capacity) { owner->malformed = 1; return; }
        } else if ((type->shape == WF_SNAPSHOT_SLOTS || type->shape == WF_SNAPSHOT_RING)
                   && type->capacity_offset == UINT64_MAX) {
            if (number > type->fixed_capacity) { owner->malformed = 1; return; }
        }
        frame->number = 0;
        frame->remaining = number;
        frame->state = 4;
    }
    snap_parse_advance(owner);
}

/* Outer byte grammar. State 0 magic, 1 header cut, 2 record/terminal marker,
 * 3 key length, 4 repeated cut, 5 chunk/end marker, 6 chunk length,
 * 7 body, 8 terminal record count, 9 after terminal. */
static void snap_parse(wf_snapshot_owner *owner, const unsigned char *data, uint64_t n) {
    const unsigned char magic[8] = {'W','F','S','N','A','P','0','1'};
    for (uint64_t i = 0; i < n && !owner->failed; ++i) {
        unsigned char byte = data[i];
        switch (owner->outer) {
        case 0:
            if (byte != magic[owner->field++]) owner->malformed = 1;
            if (owner->field == 8) { owner->outer = 1; owner->field = 0; }
            break;
        case 2:
            if (byte == 1) owner->outer = 3;
            else if (byte == 255) owner->outer = 8;
            else owner->malformed = 1;
            owner->field = owner->accumulator = 0;
            break;
        case 5:
            if (byte == 2) {
                if (owner->previous_chunk != 0 && owner->previous_chunk != 4096)
                    owner->malformed = 1;
                owner->outer = 6;
                owner->field = owner->accumulator = 0;
            } else if (byte == 3) {
                if (owner->previous_chunk == 0 || owner->key_left != 0 || !owner->value_done)
                    owner->malformed = 1;
                if (owner->completed == UINT64_MAX) owner->malformed = 1;
                else ++owner->completed;
                owner->outer = 2;
            } else if (byte == 255) {
                owner->malformed = 1; /* terminal before record completion */
                owner->outer = 8;
                owner->field = owner->accumulator = 0;
            } else owner->malformed = 1;
            break;
        case 7:
            if (owner->key_left != 0) --owner->key_left;
            else if (!owner->malformed) snap_parse_value(owner, byte);
            if (--owner->chunk_left == 0) owner->outer = 5;
            break;
        case 9: owner->malformed = 1; break;
        default: {
            unsigned width = owner->outer == 6 ? 4 : 8;
            owner->accumulator |= (uint64_t)byte << (8u * owner->field++);
            if (owner->field != width) break;
            uint64_t number = owner->accumulator;
            owner->field = owner->accumulator = 0;
            if (owner->outer == 1) {
                if (number != owner->cut) owner->malformed = 1;
                owner->outer = 2;
            } else if (owner->outer == 3) {
                owner->key_left = number;
                owner->outer = 4;
            } else if (owner->outer == 4) {
                if (number != owner->cut) owner->malformed = 1;
                owner->previous_chunk = owner->value_done = owner->body_started = 0;
                owner->parser_depth = 0;
                owner->outer = 5;
            } else if (owner->outer == 6) {
                if (number == 0 || number > 4096) owner->malformed = 1;
                owner->chunk_left = number;
                owner->previous_chunk = (uint32_t)number;
                owner->outer = number == 0 ? 5u : 7u;
            } else {
                if (number != owner->completed) owner->malformed = 1;
                owner->terminal = 1;
                owner->outer = 9;
            }
            break;
        }
        }
    }
}

static uint64_t snap_read_capacity(wf_snapshot_owner *owner) {
    if (owner->malformed) return 4096;
    if (owner->body_started) snap_parse_advance(owner);
    uint64_t free_frames = owner->parser_capacity - owner->parser_depth;
    if (free_frames != 0) return free_frames;
    if (owner->outer != 7 || owner->key_left != 0 || owner->value_done) return 1;
    if (!owner->body_started || owner->parser_depth == 0) return 0;
    snap_parse_frame *frames = owner->capsule.scratch;
    return frames[owner->parser_depth - 1].state == 4 ? 0 : 1;
}

int wf_snapshot_read_start(wf_snapshot_owner *owner, void *out, uint64_t count,
    uint64_t deadline, uint32_t has_deadline, void *cancel, void *record,
    wf_snapshot_read_result *result) {
    result->count = 0;
    result->error = 0;
    if (owner->failed) { result->error = 1; return 0; }
    if (owner->eof) return 0;
    uint64_t free_frames = snap_read_capacity(owner);
    /* Reserve the worst possible new frames before exposing a caller window
     * to a host read, so parsing can never fail halfway through that read. */
    if (free_frames == 0) { owner->failed = 1; result->error = 1; return 0; }
    if (count > free_frames) count = free_frames;
    if (count > INT64_MAX) count = INT64_MAX;
    uint64_t limit = !has_deadline || deadline == WF_COMPLETION_DEADLINE_FIRED
        ? 0 : deadline == 0 ? 1 : deadline;
    wf__completion_next_deadline(limit);
    void *watched = wf__cancel_wait_state(cancel);
    if (watched != NULL)
        return wf__completion_file_read_watched_submit(owner->data_read, out, count, watched, record);
    wf__completion_file_read_submit(owner->data_read, out, count, record);
    return wf__completion_pending(record) ? 2 : 1;
}

void wf_snapshot_read_finish(wf_snapshot_owner *owner, const void *out,
    void *opaque, wf_snapshot_read_result *result) {
    wf_completion_record *record = opaque;
    int64_t count;
    int error;
    wf__completion_file_join(record, &count, &error);
    result->count = 0;
    result->error = 0;
    if (wf__completion_deadline_passed(record)) { result->error = 2; return; }
    if (wf__completion_cancelled(record)) { result->error = 3; return; }
    if (count < 0 || error != 0) { owner->failed = 1; result->error = 1; return; }
    if (count == 0) { owner->eof = 1; return; }
    snap_parse(owner, out, (uint64_t)count);
    owner->total += (uint64_t)count;
    owner->delivered += (uint64_t)count;
    result->count = (uint64_t)count;
}

static void snap_finish(wf_snapshot_owner *owner, wf_snapshot_outcome *outcome,
                        uint32_t abort_requested) {
    memset(outcome, 0, sizeof(*outcome));
    outcome->abort_requested = abort_requested;
    if (abort_requested && owner->job.pid > 0) (void)kill(owner->job.pid, SIGKILL);
    /* Drain on a helper thread, never a source executor, before waiting for
     * exit. A full data pipe therefore cannot deadlock child termination. */
    while (!owner->eof) {
        uint64_t free_frames = snap_read_capacity(owner);
        size_t count = sizeof(owner->chunk);
        if (!owner->failed && !owner->malformed) {
            if (free_frames == 0) owner->failed = 1;
            else if (count > free_frames) count = (size_t)free_frames;
        }
        ssize_t amount;
        do { amount = read(owner->data_read, owner->chunk, count); }
        while (amount < 0 && errno == EINTR);
        if (amount < 0) { owner->failed = 1; break; }
        if (amount == 0) { owner->eof = 1; break; }
        if (!owner->failed)
            snap_parse(owner, owner->chunk, (uint64_t)amount);
        owner->total += (uint64_t)amount;
        owner->discarded += (uint64_t)amount;
    }
    if (!owner->eof) {
        (void)close(owner->data_read);
        owner->data_read = -1;
    }
    wf_fork_capsule_result termination;
    /* Reaping failures retain ownership, not an outcome that loses a child.
     * WAIT-2 imposes no wall-clock bound on a host eventually allowing reap. */
    while (wf_fork_capsule_reap_owned(&owner->job, &termination) != 0) {
        (void)poll(NULL, 0, 1);
    }
    unsigned char status[33];
    ssize_t n;
    do { n = read(owner->job.result_read, status, sizeof(status)); }
    while (n < 0 && errno == EINTR);
    int valid = n == 32 && memcmp(status, "WFSSTAT1", 8) == 0 && status[8] <= 5;
    if (valid) for (unsigned i = 9; i < 16; ++i) if (status[i] != 0) valid = 0;
    unsigned char extra;
    ssize_t tail;
    do { tail = read(owner->job.result_read, &extra, 1); }
    while (tail < 0 && errno == EINTR);
    if (tail != 0) valid = 0;
    if (termination.signal_number != 0) {
        outcome->child = 3; outcome->detail = (uint32_t)termination.signal_number;
    } else if (termination.exit_code != 0) {
        outcome->child = 2; outcome->detail = (uint32_t)termination.exit_code;
    } else if (!valid) {
        outcome->child = 4; outcome->detail = n == 0 ? 0u : 1u;
    } else if (status[8] != 0) {
        outcome->child = 1; outcome->detail = status[8] - 1u;
    } else if (!owner->terminal || owner->failed) {
        outcome->child = 4; outcome->detail = 2;
    } else if (owner->malformed || snap_u64(status + 16) != owner->total
               || snap_u64(status + 24) != owner->completed) {
        outcome->child = 4; outcome->detail = 3;
    } else {
        outcome->bytes = owner->total;
        outcome->records = owner->completed;
    }
    outcome->delivered = owner->delivered;
    outcome->discarded = owner->discarded;
    outcome->collection = owner->failed ? 2u : owner->discarded ? 1u : 0u;
    if (owner->data_read >= 0) (void)close(owner->data_read);
    owner->data_read = -1;
    (void)close(owner->job.result_read);
    owner->job.result_read = -1;
    wf_fork_capsule_dispose(&owner->capsule);
}

void wf_snapshot_run_finish(wf_snapshot_owner *owner, uint32_t abort_requested) {
    snap_finish(owner, &owner->outcome, abort_requested);
}
void wf_snapshot_take_outcome(wf_snapshot_owner *owner, wf_snapshot_outcome *result) {
    *result = owner->outcome;
    (void)munmap(owner, sizeof(*owner));
}

#else
#include <stdlib.h>
/* Atomic hooks are target-independent instrumentation; a composition with
 * an export is diagnosed before native emission on these targets. If native
 * code nevertheless invokes an export entry, stop rather than substitute a
 * different route or fabricate success. */
void wf_snapshot_atomic_begin(void) {}
void wf_snapshot_atomic_end(void) {}
wf_snapshot_owner *wf_snapshot_prepare(uint64_t a, uint64_t b, uint32_t *c) {
    (void)a; (void)b; (void)c; abort();
}
void wf_snapshot_discard(wf_snapshot_owner *a) { (void)a; abort(); }
wf_snapshot_owner *wf_snapshot_capture(wf_snapshot_owner *a, const void *b,
    const uint64_t *c, const wf_snapshot_type *d, uint64_t e, uint32_t f,
    uint64_t g, uint64_t h) {
    (void)a; (void)b; (void)c; (void)d; (void)e; (void)f; (void)g; (void)h; abort();
}
int wf_snapshot_read_start(wf_snapshot_owner *a, void *b, uint64_t c, uint64_t d,
    uint32_t e, void *f, void *g, wf_snapshot_read_result *h) {
    (void)a; (void)b; (void)c; (void)d; (void)e; (void)f; (void)g; (void)h; abort();
}
void wf_snapshot_read_finish(wf_snapshot_owner *a, const void *b, void *c,
    wf_snapshot_read_result *d) { (void)a; (void)b; (void)c; (void)d; abort(); }
void wf_snapshot_run_finish(wf_snapshot_owner *a, uint32_t b) { (void)a; (void)b; abort(); }
void wf_snapshot_take_outcome(wf_snapshot_owner *a, wf_snapshot_outcome *b) {
    (void)a; (void)b; abort();
}
#endif
