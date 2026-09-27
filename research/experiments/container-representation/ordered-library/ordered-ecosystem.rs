//! BTreeMap completes the ordered trace through its ordinary library APIs.
//! This adapter belongs to the explicit ecosystem targets and retires with them.

use std::cell::Cell;
use std::collections::{BTreeMap, btree_map::Entry};

#[cfg(account_only)]
#[path = "../ecosystem-allocator.rs"]
mod ecosystem_allocator;

const CEILING: usize = 8192;

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(13787848793156543929);
    value = (value ^ (value >> 27)).wrapping_mul(10723151780598845931);
    value ^ (value >> 31)
}

struct Digest { ordered: u64, sum: u64, parity: u64, consumed: u64 }
impl Digest {
    fn new(seed: u64) -> Self { Self { ordered: seed, sum: 0, parity: 0, consumed: 0 } }
    fn visit(&mut self, value: u64) {
        self.ordered = self.ordered.wrapping_mul(131).wrapping_add(value);
    }
    fn consume(&mut self, value: u64) {
        self.sum = self.sum.wrapping_add(value);
        self.parity ^= mix(value);
        self.consumed = self.consumed.wrapping_add(1);
    }
    fn finish(self) -> u64 {
        mix(self.ordered) ^ mix(self.sum) ^ self.parity
            ^ self.consumed.wrapping_mul(11400714819323198485)
    }
}

trait Payload: Eq {
    fn make(seed: u64) -> Self;
    fn content(&self, key: u64) -> u64;
    fn edit(&mut self);
}
impl Payload for u64 {
    fn make(seed: u64) -> Self { seed }
    fn content(&self, key: u64) -> u64 { key.wrapping_mul(131).wrapping_add(*self) }
    fn edit(&mut self) { *self = self.wrapping_add(1); }
}

// The wide inline value moves without Copy, Clone, or per-value allocation.
#[derive(Eq, PartialEq)]
struct Record { words: [u64; 32] }
impl Payload for Record {
    fn make(seed: u64) -> Self {
        Self { words: std::array::from_fn(|index| seed.wrapping_add(index as u64)) }
    }
    fn content(&self, mut key: u64) -> u64 {
        for word in self.words { key = key.wrapping_mul(131).wrapping_add(word); }
        key
    }
    fn edit(&mut self) {
        for word in &mut self.words { *word = word.wrapping_add(1); }
    }
}

struct Put<V> { status: u64, returned: Option<V> }

// Equal u64 keys have the same identity. Retaining an equal stored key is not
// a general substitute for returning a distinct comparator-equal key owner.
fn put<V>(map: &mut BTreeMap<u64, V>, key: u64, value: V, limit: usize) -> Put<V> {
    if map.len() >= limit {
        return match map.get_mut(&key) {
            Some(stored) => Put { status: 1, returned: Some(std::mem::replace(stored, value)) },
            None => Put { status: 2, returned: Some(value) },
        };
    }
    match map.entry(key) {
        Entry::Vacant(position) => {
            position.insert(value);
            Put { status: 0, returned: None }
        }
        Entry::Occupied(mut position) => Put { status: 1, returned: Some(position.insert(value)) },
    }
}

fn consume_put<V: Payload>(result: Put<V>, key: u64, digest: &mut Digest) {
    digest.visit(result.status);
    if let Some(value) = result.returned { digest.consume(value.content(key)); }
}
fn range<V: Payload>(map: &BTreeMap<u64, V>, lower: u64, upper: u64, digest: &mut Digest) {
    if lower < upper {
        for (&key, value) in map.range(lower..upper) { digest.visit(value.content(key)); }
    }
}

fn trace<V: Payload>(count: u64, rounds: u64, seed: u64, path: u64) -> u64 {
    let mut map = BTreeMap::new();
    let mut digest = Digest::new(seed);
    for index in 0..count {
        let key = 2 * ((73 * index) % count) + 1;
        consume_put(put(&mut map, key, V::make(seed.wrapping_add(index)), CEILING), key, &mut digest);
    }
    digest.visit(map.len() as u64);
    for round in 0..rounds {
        for index in 0..count {
            let key = 2 * ((73 * index) % count) + 1;
            match path {
                1 => {
                    let found = map.get(&key);
                    if let Some(value) = found { digest.visit(value.content(key)); }
                    digest.visit(u64::from(found.is_some()));
                    let absent = map.get(&(key - 1));
                    if let Some(value) = absent { digest.visit(value.content(key - 1)); }
                    digest.visit(u64::from(absent.is_some()));
                }
                2 => {
                    let next = seed.wrapping_add(count).wrapping_add(
                        round.wrapping_mul(count).wrapping_add(index).wrapping_mul(2));
                    consume_put(put(&mut map, key, V::make(next), CEILING), key, &mut digest);
                    let found = map.get_mut(&key);
                    let present = found.is_some();
                    if let Some(value) = found {
                        value.edit();
                        digest.visit(value.content(key));
                    }
                    digest.visit(u64::from(present));
                    let taken = map.remove_entry(&key);
                    digest.visit(u64::from(taken.is_some()));
                    if let Some((old_key, value)) = taken { digest.consume(value.content(old_key)); }
                    consume_put(put(&mut map, key, V::make(next.wrapping_add(1)), CEILING), key, &mut digest);
                }
                3 => range(&map, key - 1, key + 31, &mut digest),
                4 => {
                    let next = seed.wrapping_add(count).wrapping_add(
                        round.wrapping_mul(count).wrapping_add(index));
                    consume_put(put(&mut map, key, V::make(next), CEILING), key, &mut digest);
                }
                _ => {}
            }
        }
    }
    digest.visit(map.len() as u64);
    for (&key, value) in &map { digest.visit(value.content(key)); }
    // IntoIterator transfers each stored value and uses native bulk cleanup.
    for (key, value) in map { digest.consume(value.content(key)); }
    digest.finish()
}

fn audit_values<V: Payload>() {
    let mut map = BTreeMap::new();
    for index in 0..CEILING as u64 {
        let key = 2 * ((73 * index) % CEILING as u64) + 1;
        assert_eq!(put(&mut map, key, V::make(key), CEILING).status, 0);
    }
    let replaced = put(&mut map, 1, V::make(42), CEILING);
    assert_eq!(replaced.status, 1);
    assert!(replaced.returned.unwrap() == V::make(1));
    let refused = put(&mut map, 1000001, V::make(37), CEILING);
    assert_eq!(refused.status, 2);
    assert!(refused.returned.unwrap() == V::make(37));
    assert_eq!(map.len(), CEILING);
    assert!(!map.contains_key(&1000001));
    let mut digest = Digest::new(0);
    range(&map, 3, 3, &mut digest);
    range(&map, 9, 3, &mut digest);
    assert_eq!(digest.ordered, 0);
    for index in 0..CEILING as u64 {
        let rank = if index % 2 == 1 { CEILING as u64 - 1 - index / 2 } else { index / 2 };
        let key = 2 * rank + 1;
        let (old_key, value) = map.remove_entry(&key).unwrap();
        assert_eq!(old_key, key);
        assert!(value == V::make(if key == 1 { 42 } else { key }));
        assert!(map.remove_entry(&key).is_none());
    }
    assert!(map.is_empty());
    assert_eq!(put(&mut map, 7, V::make(99), CEILING).status, 0);
    for (key, value) in map { digest.consume(value.content(key)); }
}

struct Owner<'a> { id: usize, released: &'a [Cell<u32>; 6] }
impl Drop for Owner<'_> {
    fn drop(&mut self) {
        assert_eq!(self.released[self.id].replace(1), 0, "owner released once");
    }
}
fn audit_owners() {
    let released = std::array::from_fn(|_| Cell::new(0));
    let owner = |id| Box::new(Owner { id, released: &released });
    {
        let mut map = BTreeMap::new();
        assert_eq!(put(&mut map, 3, owner(0), 2).status, 0);
        let old = put(&mut map, 3, owner(1), 2);
        assert_eq!(old.status, 1);
        assert_eq!(old.returned.as_ref().unwrap().id, 0);
        drop(old);
        assert_eq!(put(&mut map, 5, owner(2), 2).status, 0);
        let at_ceiling = put(&mut map, 3, owner(3), 2);
        assert_eq!(at_ceiling.status, 1);
        assert_eq!(at_ceiling.returned.as_ref().unwrap().id, 1);
        drop(at_ceiling);
        let full = put(&mut map, 7, owner(4), 2);
        assert_eq!(full.status, 2);
        assert_eq!(full.returned.as_ref().unwrap().id, 4);
        drop(full);
        let removed = map.remove_entry(&3).unwrap();
        assert_eq!(removed.1.id, 3);
        drop(removed);
        map.clear();
        assert_eq!(put(&mut map, 7, owner(5), 2).status, 0);
    }
    for count in released { assert_eq!(count.get(), 1, "complete owner ledger"); }
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_ordered_word_trace(n: u64, r: u64, s: u64, p: u64) -> u64 {
    trace::<u64>(n, r, s, p)
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_ordered_record_trace(n: u64, r: u64, s: u64, p: u64) -> u64 {
    trace::<Record>(n, r, s, p)
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_ordered_audit() {
    audit_values::<u64>();
    audit_values::<Record>();
    audit_owners();
}
