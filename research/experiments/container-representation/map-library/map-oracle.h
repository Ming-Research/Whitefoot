// Shared behavioral oracle for historical controls and ecosystem traces.
// Kept with the map experiment; retire with its callers.

/* The oracle uses key IDs and generations, not buckets, probing, a reverse
 * index, or any of the control implementations. All arithmetic wraps in u64. */
static uint64_t oracle_content(bool wide, uint64_t key, uint64_t seed) {
    unsigned words = wide ? WORDS : 1;
    for (unsigned i = 0; i < words; ++i) key = key * UINT64_C(131) + seed + i;
    return key;
}
static uint64_t oracle_edited_content(bool wide, uint64_t key, uint64_t seed, uint64_t rounds) {
    unsigned words = wide ? WORDS : 1;
    for (unsigned i = 0; i < words; ++i) {
        uint64_t word = seed + i;
        if (i == 0) word += rounds;
        key = key * UINT64_C(131) + word;
    }
    return key;
}
static uint64_t oracle(bool wide, uint64_t count, uint64_t rounds,
                       uint64_t seed, unsigned path) {
    Digest digest = {seed, 0, 0, 0};
    for (uint64_t i = 0; i < count; ++i) ordered(&digest, INSERTED);
    for (uint64_t round = 0; round < rounds; ++round) {
        if (path == REHASH) {
            for (uint64_t i = 0; i < count; i += 2) {
                ordered(&digest, true);
                ordered(&digest, oracle_content(wide, key_at(i), seed + round * count + i));
                ordered(&digest, false);
            }
            ordered(&digest, true);
            for (uint64_t i = 0; i < count; ++i) {
                ordered(&digest, i % 2 != 0);
                if (i % 2 != 0) ordered(&digest, seed + i);
            }
            for (uint64_t i = 0; i < count; i += 2) ordered(&digest, INSERTED);
            continue;
        }
        if (path == GROW) ordered(&digest, true);
        for (uint64_t i = 0; i < count; ++i) {
            uint64_t key = key_at(i);
            if (path == HIT || path == GROW) {
                ordered(&digest, true); ordered(&digest, seed + i);
            } else if (path == MISS) ordered(&digest, false);
            else if (path == REPLACE) {
                ordered(&digest, REPLACED);
                ordered(&digest, oracle_content(wide, key, seed + round * count + i));
            } else if (path == CHURN) {
                ordered(&digest, true);
                ordered(&digest, oracle_content(wide, key, seed + round * count + i));
                ordered(&digest, false); ordered(&digest, INSERTED);
            } else if (path == EDIT) {
                ordered(&digest, true);
                ordered(&digest, seed + i + round + 1);
            }
        }
    }
    for (uint64_t i = 0; i < count; ++i) {
        uint64_t generation = path == REPLACE || path == CHURN
            || (path == REHASH && i % 2 == 0) ? rounds : 0;
        final_value(&digest, path == EDIT
            ? oracle_edited_content(wide, key_at(i), seed + i, rounds)
            : oracle_content(wide, key_at(i), seed + generation * count + i));
    }
    return finish(digest);
}

