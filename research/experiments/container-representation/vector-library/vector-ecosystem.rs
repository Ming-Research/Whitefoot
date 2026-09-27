//! The application trace uses Vec's own growth and owned-removal APIs.

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

// Deliberately neither Copy nor Clone; there is no per-element allocation.
struct Record {
    words: [u64; 32],
}

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

fn work<T: Element>(values: &mut Vec<T>, count: usize, seed: u64) -> u64 {
    let mut digest = seed;
    for index in 0..count {
        values.push(T::make(seed.wrapping_add(index as u64)));
    }
    let middle = count / 2;
    values.insert(middle, T::make(seed ^ 11_400_714_819_323_198_485));
    values.remove(middle).consume(&mut digest);
    if count != 0 {
        values.swap_remove(0).consume(&mut digest);
    }
    let retained = values.len() / 2;
    for value in values.drain(retained..) {
        value.consume(&mut digest);
    }
    for value in values.drain(..) {
        value.consume(&mut digest);
    }
    digest
}

fn trace<T: Element>(count: usize, rounds: u64, seed: u64, path: u64) -> u64 {
    let mut checksum = seed;
    if path >= 3 {
        let removed = (path - 3).min(count as u64) as usize;
        let retained = count - removed;
        let mut values = Vec::<T>::with_capacity(count + 1);
        for index in 0..retained {
            values.push(T::make(seed.wrapping_add(index as u64)));
        }
        for round in 0..rounds {
            let base = seed.wrapping_add(round);
            let mut digest = base;
            for index in retained..count {
                values.push(T::make(base.wrapping_add(index as u64)));
            }
            for value in values.drain(retained..) {
                value.consume(&mut digest);
            }
            checksum = checksum.wrapping_mul(257).wrapping_add(digest);
        }
        let mut digest = seed;
        for value in values.drain(..) {
            value.consume(&mut digest);
        }
        return checksum.wrapping_mul(257).wrapping_add(digest);
    }
    if path == 2 {
        let mut values = Vec::<T>::with_capacity(count + 1);
        for round in 0..rounds {
            let digest = work(&mut values, count, seed.wrapping_add(round));
            checksum = checksum.wrapping_mul(257).wrapping_add(digest);
        }
    } else {
        for round in 0..rounds {
            let mut values = if path == 0 {
                Vec::<T>::with_capacity(count + 1)
            } else {
                Vec::<T>::new()
            };
            let digest = work(&mut values, count, seed.wrapping_add(round));
            checksum = checksum.wrapping_mul(257).wrapping_add(digest);
        }
    }
    checksum
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_vector_word_trace(count: u64, rounds: u64, seed: u64, path: u64) -> u64 {
    trace::<u64>(count as usize, rounds, seed, path)
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_vector_record_trace(count: u64, rounds: u64, seed: u64, path: u64) -> u64 {
    trace::<Record>(count as usize, rounds, seed, path)
}
