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

use std::ffi::c_void;

// The C driver supplies max_align_t-aligned storage for three pointer words.
const _: () = {
    assert!(std::mem::size_of::<Vec<u64>>() <= 3 * std::mem::size_of::<*mut c_void>());
    assert!(std::mem::size_of::<Vec<Record>>() <= 3 * std::mem::size_of::<*mut c_void>());
    assert!(std::mem::align_of::<Vec<u64>>() <= std::mem::align_of::<*mut c_void>());
    assert!(std::mem::align_of::<Vec<Record>>() <= std::mem::align_of::<*mut c_void>());
};

#[repr(C)]
pub struct ApiObservation {
    pub length: u64,
    pub capacity: u64,
    pub checksum: u64,
    pub valid: u64,
}

trait ApiElement: Element {
    fn inspect(&self, expected: u64, digest: &mut u64) -> bool;
}

impl ApiElement for u64 {
    fn inspect(&self, expected: u64, digest: &mut u64) -> bool {
        *digest = digest.wrapping_mul(131).wrapping_add(*self);
        *self == expected
    }
}

impl ApiElement for Record {
    fn inspect(&self, expected: u64, digest: &mut u64) -> bool {
        let mut valid = true;
        for (word, value) in self.words.iter().enumerate() {
            *digest = digest.wrapping_mul(131).wrapping_add(*value);
            valid &= *value == expected.wrapping_add(word as u64);
        }
        valid
    }
}

unsafe fn api_prepare<T>(capacity: u64, storage: *mut c_void) {
    unsafe {
        storage
            .cast::<Vec<T>>()
            .write(Vec::with_capacity(capacity as usize))
    };
}

unsafe fn api_append_batch<T: Element>(storage: *mut c_void, count: u64, seed: u64) -> u64 {
    // The width-specific caller retains the matching initialized storage exclusively.
    let values = unsafe { &mut *storage.cast::<Vec<T>>() };
    for index in 0..count {
        values.push(T::make(seed.wrapping_add(index)));
    }
    values.len() as u64
}

unsafe fn api_inspect_reset<T: ApiElement>(
    storage: *mut c_void,
    count: u64,
    seed: u64,
    observation: &mut ApiObservation,
) -> u64 {
    // The descriptor is live in the same C-owned storage initialized by prepare.
    let values = unsafe { &mut *storage.cast::<Vec<T>>() };
    let mut digest = seed;
    let mut valid = values.len() as u64 == count;
    for (index, value) in values.iter().enumerate() {
        valid &= value.inspect(seed.wrapping_add(index as u64), &mut digest);
    }
    *observation = ApiObservation {
        length: values.len() as u64,
        capacity: values.capacity() as u64,
        checksum: digest,
        valid: u64::from(valid),
    };
    values.clear();
    observation.valid
}

unsafe fn api_destroy<T>(storage: *mut c_void) -> u8 {
    // Drop releases the backing; the C driver retains its descriptor storage.
    unsafe { std::ptr::drop_in_place(storage.cast::<Vec<T>>()) };
    0
}

macro_rules! api_exports {
    ($element:ty, $prepare:ident, $append:ident, $inspect:ident, $destroy:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $prepare(capacity: u64, storage: *mut c_void) {
            unsafe { api_prepare::<$element>(capacity, storage) }
        }

        // The C driver passes live, aligned, exclusive descriptor/observation slots.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $append(storage: *mut c_void, count: u64, seed: u64) -> u64 {
            unsafe { api_append_batch::<$element>(storage, count, seed) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $inspect(
            storage: *mut c_void,
            count: u64,
            seed: u64,
            observation: *mut ApiObservation,
        ) -> u64 {
            unsafe { api_inspect_reset::<$element>(storage, count, seed, &mut *observation) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $destroy(storage: *mut c_void) -> u8 {
            unsafe { api_destroy::<$element>(storage) }
        }
    };
}

api_exports!(
    u64,
    rust_vector_api_word_prepare,
    rust_vector_api_word_append_batch,
    rust_vector_api_word_inspect_reset,
    rust_vector_api_word_destroy
);
api_exports!(
    Record,
    rust_vector_api_record_prepare,
    rust_vector_api_record_append_batch,
    rust_vector_api_record_inspect_reset,
    rust_vector_api_record_destroy
);
