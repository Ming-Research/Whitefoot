/* Serves concurrent-map-bench: liburcu's cds_lfht, a lock-free resizable
 * split-ordered hash table whose readers run under read-copy-update and whose
 * removed nodes are freed after a grace period. It has no section on an
 * entry: an update is an atomic addition to the node's value. */
#define _GNU_SOURCE
#define _LGPL_SOURCE
#include <stdlib.h>
#include <urcu.h>
#include <urcu/rculfhash.h>

#include "cmap.h"

struct cm_map {
    struct cds_lfht *table;
};

struct node {
    struct cds_lfht_node link;
    uint64_t key;
    uint64_t value;
    struct rcu_head rcu;
};

static int match(struct cds_lfht_node *link, const void *key) {
    return caa_container_of(link, struct node, link)->key == *(const uint64_t *)key;
}

static void release(struct rcu_head *head) { free(caa_container_of(head, struct node, rcu)); }

static unsigned long hash_of(uint64_t key) { return (unsigned long)(key * CM_GOLDEN); }

static struct node *find(cm_map *map, uint64_t key) {
    struct cds_lfht_iter iter;
    cds_lfht_lookup(map->table, hash_of(key), match, &key, &iter);
    struct cds_lfht_node *link = cds_lfht_iter_get_node(&iter);
    return link ? caa_container_of(link, struct node, link) : NULL;
}

const char *CM(name)(void) { return "urcu-lfht"; }
int CM(flags)(void) { return CM_ATOMIC_ADD_UPDATE; }

cm_map *CM(create)(uint64_t capacity) {
    unsigned long initial = 1;
    while (initial < capacity && initial < (1ul << 30))
        initial <<= 1;
    cm_map *map = malloc(sizeof *map);
    map->table = cds_lfht_new(initial, 1, 0, CDS_LFHT_AUTO_RESIZE | CDS_LFHT_ACCOUNTING, NULL);
    return map->table ? map : NULL;
}

void CM(destroy)(cm_map *map) {
    struct cds_lfht_iter iter;
    struct cds_lfht_node *link;
    rcu_register_thread();
    rcu_read_lock();
    cds_lfht_for_each(map->table, &iter, link) {
        if (cds_lfht_del(map->table, link) == 0)
            call_rcu(&caa_container_of(link, struct node, link)->rcu, release);
    }
    rcu_read_unlock();
    rcu_unregister_thread();
    rcu_barrier();
    cds_lfht_destroy(map->table, NULL);
    free(map);
}

void CM(enter)(cm_map *map) {
    (void)map;
    rcu_register_thread();
}

void CM(leave)(cm_map *map) {
    (void)map;
    rcu_unregister_thread();
}

int CM(get)(cm_map *map, uint64_t key, uint64_t *value) {
    rcu_read_lock();
    struct node *n = find(map, key);
    if (n)
        *value = uatomic_read(&n->value);
    rcu_read_unlock();
    return n != NULL;
}

int CM(insert)(cm_map *map, uint64_t key, uint64_t value) {
    struct node *n = malloc(sizeof *n);
    cds_lfht_node_init(&n->link);
    n->key = key;
    n->value = value;
    rcu_read_lock();
    struct cds_lfht_node *old = cds_lfht_add_replace(map->table, hash_of(key), match, &key, &n->link);
    rcu_read_unlock();
    if (old)
        call_rcu(&caa_container_of(old, struct node, link)->rcu, release);
    return old == NULL;
}

int CM(remove)(cm_map *map, uint64_t key) {
    rcu_read_lock();
    struct node *n = find(map, key);
    int removed = n && cds_lfht_del(map->table, &n->link) == 0;
    rcu_read_unlock();
    if (removed)
        call_rcu(&n->rcu, release);
    return removed;
}

int CM(update)(cm_map *map, uint64_t key) {
    rcu_read_lock();
    struct node *n = find(map, key);
    if (n)
        uatomic_inc(&n->value);
    rcu_read_unlock();
    return n != NULL;
}
