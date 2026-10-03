//! Endpoint traces use VecDeque directly. Fresh-backing rebase is excluded.

use std::collections::VecDeque;

#[cfg(account_only)]
#[path = "../ecosystem-allocator.rs"]
mod allocator;

trait Element {
    fn make(seed: u64) -> Self;
    fn consume(self, digest: &mut u64);
}

impl Element for u64 {
    fn make(seed: u64) -> Self {
        seed
    }

    fn consume(self, digest: &mut u64) {
        *digest = digest.wrapping_mul(131).wrapping_add(self);
    }
}

// The inline owner is movable but has neither Copy nor Clone.
struct Record {
    words: [u64; 32],
}

const _: () = assert!(std::mem::size_of::<Record>() == 256);

impl Element for Record {
    fn make(seed: u64) -> Self {
        Self {
            words: std::array::from_fn(|index| seed.wrapping_add(index as u64)),
        }
    }

    fn consume(self, digest: &mut u64) {
        for word in self.words {
            word.consume(digest);
        }
    }
}

fn trace<T: Element>(count: usize, rounds: u64, seed: u64, path: u64) -> u64 {
    if path == 3 {
        let mut checksum = seed;
        for round in 0..rounds {
            let base = seed.wrapping_add(round);
            let mut digest = base;
            let mut values = VecDeque::<T>::with_capacity(count);
            for index in 0..count {
                values.push_back(T::make(base.wrapping_add(index as u64)));
            }
            for index in count..2 * count + 1 {
                values.push_back(T::make(base.wrapping_add(index as u64)));
            }
            for value in values {
                value.consume(&mut digest);
            }
            checksum = checksum.wrapping_mul(257).wrapping_add(digest);
        }
        return checksum;
    }
    let mut values = VecDeque::<T>::with_capacity(count);
    let mut digest = seed;
    for index in 0..count {
        values.push_back(T::make(seed.wrapping_add(index as u64)));
    }
    for round in 0..rounds {
        let base = seed.wrapping_add(round.wrapping_add(1).wrapping_mul(count as u64));
        if path == 1 {
            for index in 0..count {
                values.pop_back().unwrap().consume(&mut digest);
                values.push_front(T::make(base.wrapping_add(index as u64)));
            }
        } else {
            for index in 0..count {
                values.pop_front().unwrap().consume(&mut digest);
                values.push_back(T::make(base.wrapping_add(index as u64)));
            }
        }
    }
    for value in values {
        value.consume(&mut digest);
    }
    digest
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_deque_word_trace(count: u64, rounds: u64, seed: u64, path: u64) -> u64 {
    trace::<u64>(count as usize, rounds, seed, path)
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_deque_record_trace(count: u64, rounds: u64, seed: u64, path: u64) -> u64 {
    trace::<Record>(count as usize, rounds, seed, path)
}
