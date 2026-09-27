//! Ordinary HashMap operations for the existing map trace and key-ID oracle.
use std::collections::{HashMap, hash_map::Entry};
use std::hash::{BuildHasher, Hasher, RandomState};

#[cfg(account_only)]
#[path = "../ecosystem-allocator.rs"]
mod ecosystem_allocator;

#[cfg(account_only)]
unsafe extern "C" {
    fn wf_ecosystem_map_geometry(
        kind: u64,
        count: u64,
        usable: u64,
        slots: u64,
        buckets: u64,
        load_factor: f64,
        max_load_factor: f64,
    );
}

const HIT: u64 = 0;
const MISS: u64 = 1;
const REPLACE: u64 = 2;
const CHURN: u64 = 3;
const GROW: u64 = 4;
const SETUP: u64 = 6;
const EDIT: u64 = 7;
const POLICY: u64 = 8;
const RESERVE_CHECK: u64 = 9;
const RESERVE_OMITTED: u64 = 10;
const CEILING: usize = 16384;

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

struct AlignedBuild {
    salt: u64,
    collide: bool,
}
struct AlignedHasher {
    salt: u64,
    collide: bool,
    hash: u64,
}
impl BuildHasher for AlignedBuild {
    type Hasher = AlignedHasher;
    fn build_hasher(&self) -> Self::Hasher {
        AlignedHasher {
            salt: self.salt,
            collide: self.collide,
            hash: 0,
        }
    }
}
impl Hasher for AlignedHasher {
    fn finish(&self) -> u64 {
        self.hash
    }
    fn write_u64(&mut self, value: u64) {
        self.hash = if self.collide {
            0
        } else {
            mix(value ^ self.salt)
        };
    }
    fn write(&mut self, bytes: &[u8]) {
        let word: [u8; 8] = bytes.try_into().expect("the aligned key is one u64");
        self.write_u64(u64::from_ne_bytes(word));
    }
}

trait Payload {
    fn make(seed: u64) -> Self;
    fn identity(&self) -> u64;
    fn content(self, key: u64) -> u64;
    fn increment(&mut self) -> u64;
}
impl Payload for u64 {
    fn make(seed: u64) -> Self {
        seed
    }
    fn identity(&self) -> u64 {
        *self
    }
    fn content(self, key: u64) -> u64 {
        key.wrapping_mul(131).wrapping_add(self)
    }
    fn increment(&mut self) -> u64 {
        *self = self.wrapping_add(1);
        *self
    }
}
struct Record {
    words: [u64; 32],
}
impl Payload for Record {
    fn make(seed: u64) -> Self {
        Self {
            words: std::array::from_fn(|index| seed.wrapping_add(index as u64)),
        }
    }
    fn identity(&self) -> u64 {
        self.words[0]
    }
    fn content(self, mut key: u64) -> u64 {
        for word in self.words {
            key = key.wrapping_mul(131).wrapping_add(word);
        }
        key
    }
    fn increment(&mut self) -> u64 {
        self.words[0] = self.words[0].wrapping_add(1);
        self.words[0]
    }
}

struct Digest {
    ordered: u64,
    sum: u64,
    parity: u64,
    count: u64,
}
impl Digest {
    fn new(seed: u64) -> Self {
        Self {
            ordered: seed,
            sum: 0,
            parity: 0,
            count: 0,
        }
    }
    fn ordered(&mut self, value: u64) {
        self.ordered = self.ordered.wrapping_mul(131).wrapping_add(value);
    }
    fn consume(&mut self, value: u64) {
        self.sum = self.sum.wrapping_add(value);
        self.parity ^= mix(value);
        self.count = self.count.wrapping_add(1);
    }
    fn finish(self) -> u64 {
        mix(self.ordered)
            ^ mix(self.sum)
            ^ self.parity
            ^ self.count.wrapping_mul(0x9e37_79b9_7f4a_7c15)
    }
}

fn key_at(index: u64) -> u64 {
    index.wrapping_mul(2).wrapping_add(1)
}

fn put<V: Payload, S: BuildHasher>(
    map: &mut HashMap<u64, V, S>,
    digest: &mut Digest,
    key: u64,
    value: V,
    ceiling: usize,
) {
    // Equal u64 keys are identical. The standard API retains the stored key,
    // unlike WF's general owned-key replacement, but this trace observes the
    // same old pair and consumes the complete old value.
    if map.len() == ceiling {
        // A full logical map permits replacement, but must not grow to insert
        // a missing key. Avoid Entry's possible reservation on this path.
        if let Some(stored) = map.get_mut(&key) {
            digest.ordered(1);
            digest.ordered(std::mem::replace(stored, value).content(key));
        } else {
            digest.ordered(2);
            digest.ordered(value.content(key));
        }
        return;
    }
    match map.entry(key) {
        Entry::Occupied(mut entry) => {
            digest.ordered(1);
            digest.ordered(entry.insert(value).content(key));
        }
        Entry::Vacant(entry) => {
            entry.insert(value);
            digest.ordered(0);
        }
    }
}

fn trace<V: Payload, S: BuildHasher>(
    capacity: u64,
    count: u64,
    rounds: u64,
    seed: u64,
    path: u64,
    hash: S,
) -> u64 {
    assert!(matches!(
        path,
        HIT | MISS
            | REPLACE
            | CHURN
            | GROW
            | SETUP
            | EDIT
            | POLICY
            | RESERVE_CHECK
            | RESERVE_OMITTED
    ));
    let mut map = HashMap::with_capacity_and_hasher(capacity as usize, hash);
    let mut digest = Digest::new(seed);
    let ceiling = if path == POLICY { 3 } else { CEILING };
    for index in 0..count {
        put(
            &mut map,
            &mut digest,
            key_at(index),
            V::make(seed.wrapping_add(index)),
            ceiling,
        );
    }
    #[cfg(account_only)]
    // capacity() is a guaranteed entry lower bound, not a physical slot count.
    // This scalar snapshot exists only in the separately accounted image.
    unsafe {
        wf_ecosystem_map_geometry(
            1,
            map.len() as u64,
            map.capacity() as u64,
            u64::MAX,
            u64::MAX,
            -1.0,
            -1.0,
        );
    }
    if path == POLICY {
        put(
            &mut map,
            &mut digest,
            key_at(0),
            V::make(seed.wrapping_add(10)),
            ceiling,
        );
        put(
            &mut map,
            &mut digest,
            key_at(count),
            V::make(seed.wrapping_add(20)),
            ceiling,
        );
    }
    for round in 0..rounds {
        if path == GROW || path == RESERVE_CHECK || path == RESERVE_OMITTED {
            let total = if capacity == 0 { 1 } else { capacity * 2 };
            if path != RESERVE_OMITTED {
                map.reserve((total as usize).saturating_sub(map.len()));
            }
            digest.ordered(1);
            if path != GROW {
                digest.ordered(u64::from(map.capacity() >= total as usize));
            }
        }
        for index in 0..count {
            let key = key_at(if path == MISS { count + index } else { index });
            match path {
                HIT | MISS | GROW | RESERVE_CHECK | RESERVE_OMITTED => {
                    let found = map.get(&key);
                    digest.ordered(u64::from(found.is_some()));
                    if let Some(value) = found {
                        digest.ordered(value.identity());
                    }
                }
                REPLACE => {
                    let next = seed
                        .wrapping_add((round + 1).wrapping_mul(count))
                        .wrapping_add(index);
                    put(&mut map, &mut digest, key, V::make(next), ceiling);
                }
                CHURN => {
                    let removed = map.remove_entry(&key);
                    digest.ordered(u64::from(removed.is_some()));
                    if let Some((old_key, value)) = removed {
                        digest.ordered(value.content(old_key));
                    }
                    let absent = map.get(&key);
                    digest.ordered(u64::from(absent.is_some()));
                    if let Some(value) = absent {
                        digest.ordered(value.identity());
                    }
                    let next = seed
                        .wrapping_add((round + 1).wrapping_mul(count))
                        .wrapping_add(index);
                    put(&mut map, &mut digest, key, V::make(next), ceiling);
                }
                EDIT => {
                    let found = map.get_mut(&key);
                    digest.ordered(u64::from(found.is_some()));
                    if let Some(value) = found {
                        digest.ordered(value.increment());
                    }
                }
                SETUP | POLICY => {}
                _ => unreachable!(),
            }
        }
    }
    for (key, value) in map {
        digest.consume(value.content(key));
    }
    digest.finish()
}

macro_rules! entry {
    ($default:ident, $aligned:ident, $value:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $default(
            capacity: u64,
            count: u64,
            rounds: u64,
            seed: u64,
            path: u64,
            collide: u64,
        ) -> u64 {
            assert_eq!(collide, 0, "default hashing cannot impose collisions");
            trace::<$value, _>(capacity, count, rounds, seed, path, RandomState::new())
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn $aligned(
            capacity: u64,
            count: u64,
            rounds: u64,
            seed: u64,
            path: u64,
            collide: u64,
        ) -> u64 {
            trace::<$value, _>(
                capacity,
                count,
                rounds,
                seed,
                path,
                AlignedBuild {
                    salt: seed ^ 0x9e37_79b9_7f4a_7c15,
                    collide: collide != 0,
                },
            )
        }
    };
}
entry!(eco_rust_map_word_default, eco_rust_map_word_aligned, u64);
entry!(
    eco_rust_map_record_default,
    eco_rust_map_record_aligned,
    Record
);
