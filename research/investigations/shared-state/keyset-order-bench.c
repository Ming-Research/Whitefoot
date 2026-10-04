/* Building a ten-key set and putting it in lock order, three ways:
 *   ordered: v0.89's key set, byte order kept at every insertion (binary
 *            search with memcmp, a move of the later items, the key copied);
 *   e:       proposal E, a 64-bit hash per key, a small index table to find
 *            repeats, the key appended and copied, the indices sorted by
 *            (hash, length, bytes) when the statement takes its hold;
 *   tagged:  cea9188d4's collection, the key's place and hash appended with
 *            no copy, a heap sort by (hash, length, bytes) at the hold.
 * Each builds a set from the same 10 keys "key:%012d", the redis-benchmark
 * MSET shape, then yields the keys in lock order to a checksum. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static uint64_t hash_bytes(const unsigned char *p, uint64_t n) {
    uint64_t h = 0x243F6A8885A308D3ull ^ (n * 0x9E3779B97F4A7C15ull);
    while (n >= 8) {
        uint64_t w;
        memcpy(&w, p, 8);
        h = (h ^ w) * 0xBF58476D1CE4E5B9ull;
        h ^= h >> 31;
        p += 8;
        n -= 8;
    }
    if (n != 0) {
        uint64_t w = 0;
        memcpy(&w, p, (size_t)n);
        h = (h ^ w) * 0x94D049BB133111EBull;
        h ^= h >> 29;
    }
    h ^= h >> 32;
    h *= 0xD6E8FEB86659FD93ull;
    h ^= h >> 32;
    return h & ((1ull << 62) - 1);
}

static int compare_keys(const unsigned char *a, uint64_t al, const unsigned char *b, uint64_t bl) {
    uint64_t s = al < bl ? al : bl;
    int o = s == 0 ? 0 : memcmp(a, b, (size_t)s);
    if (o != 0)
        return o;
    return al < bl ? -1 : al > bl ? 1 : 0;
}

enum { N = 10, ROOM = 16, ARENA = 512 };

/* v0.89 */
typedef struct { uint64_t offset, length, payload; } item;
typedef struct { uint64_t len, used; unsigned char bytes[ARENA]; item items[ROOM]; } ordered_set;
static void ordered_put(ordered_set *s, const unsigned char *k, uint64_t n, uint64_t payload) {
    uint64_t lo = 0, hi = s->len;
    while (lo < hi) {
        uint64_t mid = lo + (hi - lo) / 2;
        int o = compare_keys(s->bytes + s->items[mid].offset, s->items[mid].length, k, n);
        if (o == 0) {
            s->items[mid].payload = payload;
            return;
        }
        if (o < 0)
            lo = mid + 1;
        else
            hi = mid;
    }
    memcpy(s->bytes + s->used, k, n);
    memmove(&s->items[lo + 1], &s->items[lo], (size_t)(s->len - lo) * sizeof(item));
    s->items[lo] = (item){s->used, n, payload};
    s->used += n;
    s->len++;
}

/* E */
typedef struct { uint64_t offset, length, tag; } eitem;
typedef struct { uint64_t len, used; unsigned char bytes[ARENA]; eitem items[ROOM]; uint8_t slot[32]; uint8_t order[ROOM]; } e_set;
static uint64_t e_insert(e_set *s, const unsigned char *k, uint64_t n) {
    uint64_t tag = hash_bytes(k, n);
    for (uint64_t i = tag & 31;; i = (i + 1) & 31) {
        uint8_t at = s->slot[i];
        if (at == 0)
            break;
        eitem *it = &s->items[at - 1];
        if (it->tag == tag && it->length == n && memcmp(s->bytes + it->offset, k, n) == 0)
            return at - 1;
    }
    for (uint64_t i = tag & 31;; i = (i + 1) & 31)
        if (s->slot[i] == 0) {
            s->slot[i] = (uint8_t)(s->len + 1);
            break;
        }
    memcpy(s->bytes + s->used, k, n);
    s->items[s->len] = (eitem){s->used, n, tag};
    s->used += n;
    return s->len++;
}
static int e_before(const e_set *s, uint8_t a, uint8_t b) {
    const eitem *x = &s->items[a], *y = &s->items[b];
    if (x->tag != y->tag)
        return x->tag < y->tag;
    return compare_keys(s->bytes + x->offset, x->length, s->bytes + y->offset, y->length) < 0;
}
static void e_lock_order(e_set *s) {
    for (uint64_t i = 0; i < s->len; i++) {
        uint8_t v = (uint8_t)i;
        uint64_t j = i;
        while (j > 0 && e_before(s, v, s->order[j - 1])) {
            s->order[j] = s->order[j - 1];
            j--;
        }
        s->order[j] = v;
    }
}

/* cea9188d4 */
typedef struct { const unsigned char *key; uint64_t length, tag; } held;
static int held_order(const held *a, const held *b) {
    if (a->tag != b->tag)
        return a->tag < b->tag ? -1 : 1;
    if (a->length != b->length)
        return a->length < b->length ? -1 : 1;
    return a->length == 0 ? 0 : memcmp(a->key, b->key, (size_t)a->length);
}
static void sift(held *e, uint64_t root, uint64_t count) {
    for (;;) {
        uint64_t child = 2 * root + 1;
        if (child >= count)
            return;
        if (child + 1 < count && held_order(&e[child], &e[child + 1]) < 0)
            child++;
        if (held_order(&e[root], &e[child]) >= 0)
            return;
        held t = e[root];
        e[root] = e[child];
        e[child] = t;
        root = child;
    }
}

static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + t.tv_nsec * 1e-9;
}

int main(int argc, char **argv) {
    long rounds = argc > 1 ? atol(argv[1]) : 2000000;
    enum { SETS = 4096 };
    static unsigned char keys[SETS][N][16];
    srand(42);
    for (int s = 0; s < SETS; s++)
        for (int i = 0; i < N; i++) {
            char t[17];
            snprintf(t, sizeof t, "key:%012d", rand() % 100000);
            memcpy(keys[s][i], t, 16);
        }
    for (int way = 0; way < 3; way++) {
        static const char *names[] = {"ordered", "e", "tagged"};
        uint64_t sum = 0;
        double best = 1e9;
        for (int rep = 0; rep < 5; rep++) {
            double t0 = now();
            for (long r = 0; r < rounds; r++) {
                unsigned char (*k)[16] = keys[r & (SETS - 1)];
                if (way == 0) {
                    ordered_set s;
                    s.len = s.used = 0;
                    for (int i = 0; i < N; i++)
                        ordered_put(&s, k[i], 16, (uint64_t)i);
                    for (uint64_t i = 0; i < s.len; i++)
                        sum += hash_bytes(s.bytes + s.items[i].offset, s.items[i].length) ^ s.items[i].payload;
                } else if (way == 1) {
                    e_set s;
                    s.len = s.used = 0;
                    memset(s.slot, 0, sizeof s.slot);
                    uint64_t last[ROOM];
                    for (int i = 0; i < N; i++)
                        last[e_insert(&s, k[i], 16)] = (uint64_t)i;
                    e_lock_order(&s);
                    for (uint64_t i = 0; i < s.len; i++)
                        sum += s.items[s.order[i]].tag ^ last[s.order[i]];
                } else {
                    held h[N];
                    for (int i = 0; i < N; i++)
                        h[i] = (held){k[i], 16, hash_bytes(k[i], 16)};
                    for (uint64_t i = N / 2; i-- > 0;)
                        sift(h, i, N);
                    for (uint64_t end = N; end-- > 1;) {
                        held t = h[0];
                        h[0] = h[end];
                        h[end] = t;
                        sift(h, 0, end);
                    }
                    for (int i = 0; i < N; i++)
                        sum += h[i].tag;
                }
            }
            double ns = (now() - t0) / rounds * 1e9;
            if (ns < best)
                best = ns;
        }
        printf("%-8s %7.1f ns per set (checksum %llu)\n", names[way], best, (unsigned long long)sum);
    }
    return 0;
}
