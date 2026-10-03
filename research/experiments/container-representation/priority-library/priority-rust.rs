//! Practical standard-library control. The C driver owns traces and the oracle;
//! only complete traces cross the ABI. Retire with the ecosystem comparison.
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

#[cfg(account_only)]
#[path = "../ecosystem-allocator.rs"]
mod ecosystem_allocator;

const CEILING: usize = 4096;

trait Element: Ord {
    fn make(seed: u64) -> Self;
    fn key(&self) -> u64;
    fn accept(self, digest: &mut u64);
    fn verify(&self, seed: u64);
}

impl Element for u64 {
    fn make(seed: u64) -> Self { seed }
    fn key(&self) -> u64 { *self }
    fn accept(self, digest: &mut u64) {
        *digest = digest.wrapping_mul(131).wrapping_add(self);
    }
    fn verify(&self, seed: u64) { assert_eq!(*self, seed); }
}

// Deliberately neither Copy nor Clone. Inline movement is the measured cost;
// this record does not stand in for separately allocated nested owners.
struct Record([u64; 32]);
const _: () = assert!(std::mem::size_of::<Record>() == 256);
const _: () = assert!(std::mem::size_of::<Reverse<Record>>() == 256);

impl PartialEq for Record {
    fn eq(&self, other: &Self) -> bool { self.0[0] == other.0[0] }
}
impl Eq for Record {}
impl PartialOrd for Record {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for Record {
    fn cmp(&self, other: &Self) -> Ordering { self.0[0].cmp(&other.0[0]) }
}
impl Element for Record {
    fn make(seed: u64) -> Self {
        Self(std::array::from_fn(|i| seed.wrapping_add(i as u64)))
    }
    fn key(&self) -> u64 { self.0[0] }
    fn accept(self, digest: &mut u64) {
        for word in self.0 { word.accept(digest); }
    }
    fn verify(&self, seed: u64) {
        for (i, word) in self.0.iter().enumerate() {
            assert_eq!(*word, seed.wrapping_add(i as u64));
        }
    }
}

fn next(state: u64) -> u64 {
    state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

// The application ceiling is shared with WF. Native backing capacity and
// growth are otherwise chosen by BinaryHeap/Vec, including native realloc.
fn push<T: Element>(heap: &mut BinaryHeap<Reverse<T>>, value: T) -> Result<(), T> {
    if heap.len() == CEILING { return Err(value); }
    heap.push(Reverse(value));
    Ok(())
}

fn replace_top<T: Element>(heap: &mut BinaryHeap<Reverse<T>>, value: T) -> T {
    let mut top = heap.peek_mut().expect("nonempty replacement");
    // Dropping PeekMut repairs the native heap before returning the old owner.
    std::mem::replace(&mut *top, Reverse(value)).0
}

fn drain<T: Element>(heap: &mut BinaryHeap<Reverse<T>>, digest: &mut u64) {
    while let Some(Reverse(value)) = heap.pop() { value.accept(digest); }
}

fn trace<T: Element>(count: u64, rounds: u64, seed: u64, path: u64) -> u64 {
    assert!(count <= CEILING as u64 && path <= 4);
    let mut state = seed;
    let mut digest = seed;
    if path >= 3 {
        let mut values = Vec::with_capacity(count as usize);
        for _ in 0..count {
            state = next(state);
            values.push(Reverse(T::make(state)));
        }
        if path == 4 {
            // Storage-only control: no heap construction or priority ordering.
            for Reverse(value) in values.into_iter().rev() { value.accept(&mut digest); }
        } else {
            let mut heap = BinaryHeap::from(values);
            drain(&mut heap, &mut digest);
        }
        return digest;
    }
    let mut heap = if path < 2 {
        BinaryHeap::with_capacity(count as usize)
    } else {
        BinaryHeap::new()
    };
    for _ in 0..count {
        state = next(state);
        if let Err(value) = push(&mut heap, T::make(state)) { value.accept(&mut digest); }
    }
    if path < 2 {
        if let Some(Reverse(value)) = heap.peek() { value.key().accept(&mut digest); }
        for _ in 0..rounds {
            for _ in 0..count {
                state = next(state);
                let value = T::make(state);
                if heap.is_empty() {
                    value.accept(&mut digest);
                } else if path == 0 {
                    heap.pop().expect("nonempty pop").0.accept(&mut digest);
                    if let Err(value) = push(&mut heap, value) { value.accept(&mut digest); }
                } else {
                    replace_top(&mut heap, value).accept(&mut digest);
                }
            }
        }
    }
    drain(&mut heap, &mut digest);
    digest
}

fn refusal<T: Element>() -> u64 {
    let mut heap = BinaryHeap::with_capacity(CEILING);
    let mut state = 101;
    let mut digest = 101;
    for _ in 0..CEILING {
        state = next(state);
        assert!(push(&mut heap, T::make(state)).is_ok());
    }
    state = next(state);
    let refused = match push(&mut heap, T::make(state)) {
        Err(value) => value,
        Ok(()) => panic!("full queue accepted an owner"),
    };
    refused.verify(state);
    assert_eq!(heap.len(), CEILING);
    heap.pop().expect("full queue pop").0.accept(&mut digest);
    assert!(push(&mut heap, refused).is_ok());
    drain(&mut heap, &mut digest);
    digest
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn priority_rust_word_trace(count: u64, rounds: u64, seed: u64, path: u64) -> u64 {
    trace::<u64>(count, rounds, seed, path)
}
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn priority_rust_record_trace(count: u64, rounds: u64, seed: u64, path: u64) -> u64 {
    trace::<Record>(count, rounds, seed, path)
}
#[unsafe(no_mangle)]
pub extern "C" fn priority_rust_refusal(wide: u64) -> u64 {
    if wide != 0 { refusal::<Record>() } else { refusal::<u64>() }
}
