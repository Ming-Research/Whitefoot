//! Serves concurrent-map-bench: the Rust comparators and the Rust floor
//! behind the functions `cmap.h` names, one prefix per map.
//!
//! - `papaya`: lock-free reads and incremental resizing; its update may run
//!   its function more than once, so it is flagged optimistic.
//! - `dashmap`: shards of hashbrown tables, each behind a reader-writer lock;
//!   an update holds its shard's write lock.
//! - `scc`: per-bucket locks with lock-free reads; an update holds its
//!   bucket's lock and runs once.
//! - `stdmap`: the standard library's table with no lock, a floor at one
//!   thread.

use std::cell::UnsafeCell;
use std::ffi::c_char;
use std::hash::{BuildHasherDefault, Hasher};

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
const OPTIMISTIC_UPDATE: i32 = 1;
const ONE_THREAD: i32 = 8;

/// The hash every native implementation uses: one multiplication by the
/// 64-bit golden ratio.
#[derive(Default, Clone, Copy)]
pub struct Golden(u64);

impl Hasher for Golden {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!("keys are 64-bit integers")
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n.wrapping_mul(GOLDEN);
    }
}

type Build = BuildHasherDefault<Golden>;

macro_rules! entry_points {
    ($map:ty, $name:literal, $flags:expr,
     $name_fn:ident, $flags_fn:ident, $create:ident, $destroy:ident, $enter:ident, $leave:ident,
     $get:ident, $insert:ident, $remove:ident, $update:ident,
     new: |$cap:ident| $new:expr,
     get: |$gm:ident, $gk:ident| $get_body:expr,
     insert: |$im:ident, $ik:ident, $iv:ident| $insert_body:expr,
     remove: |$rm:ident, $rk:ident| $remove_body:expr,
     update: |$um:ident, $uk:ident| $update_body:expr) => {
        #[no_mangle]
        pub extern "C" fn $name_fn() -> *const c_char {
            concat!($name, "\0").as_ptr().cast()
        }
        #[no_mangle]
        pub extern "C" fn $flags_fn() -> i32 {
            $flags
        }
        #[no_mangle]
        pub extern "C" fn $create(capacity: u64) -> *mut $map {
            let $cap = capacity as usize;
            Box::into_raw(Box::new($new))
        }
        /// # Safety
        /// `map` came from the matching create and is not used afterwards.
        #[no_mangle]
        pub unsafe extern "C" fn $destroy(map: *mut $map) {
            drop(Box::from_raw(map));
        }
        #[no_mangle]
        pub extern "C" fn $enter(_: *mut $map) {}
        #[no_mangle]
        pub extern "C" fn $leave(_: *mut $map) {}
        /// # Safety
        /// `map` is live and `value` is writable.
        #[no_mangle]
        pub unsafe extern "C" fn $get(map: *mut $map, key: u64, value: *mut u64) -> i32 {
            let $gm = &*map;
            let $gk = key;
            match $get_body {
                Some(v) => {
                    *value = v;
                    1
                }
                None => 0,
            }
        }
        /// # Safety
        /// `map` is live.
        #[no_mangle]
        pub unsafe extern "C" fn $insert(map: *mut $map, key: u64, value: u64) -> i32 {
            let $im = &*map;
            let ($ik, $iv) = (key, value);
            i32::from($insert_body)
        }
        /// # Safety
        /// `map` is live.
        #[no_mangle]
        pub unsafe extern "C" fn $remove(map: *mut $map, key: u64) -> i32 {
            let $rm = &*map;
            let $rk = key;
            i32::from($remove_body)
        }
        /// # Safety
        /// `map` is live.
        #[no_mangle]
        pub unsafe extern "C" fn $update(map: *mut $map, key: u64) -> i32 {
            let $um = &*map;
            let $uk = key;
            i32::from($update_body)
        }
    };
}

type Papaya = papaya::HashMap<u64, u64, Build>;
entry_points!(Papaya, "papaya", OPTIMISTIC_UPDATE,
    papaya_name, papaya_flags, papaya_create, papaya_destroy, papaya_enter, papaya_leave,
    papaya_get, papaya_insert, papaya_remove, papaya_update,
    new: |cap| Papaya::with_capacity_and_hasher(cap, Build::default()),
    get: |m, k| m.pin().get(&k).copied(),
    insert: |m, k, v| m.pin().insert(k, v).is_none(),
    remove: |m, k| m.pin().remove(&k).is_some(),
    update: |m, k| m.pin().update(k, |v| v + 1).is_some());

type Dash = dashmap::DashMap<u64, u64, Build>;
entry_points!(Dash, "dashmap", 0,
    dashmap_name, dashmap_flags, dashmap_create, dashmap_destroy, dashmap_enter, dashmap_leave,
    dashmap_get, dashmap_insert, dashmap_remove, dashmap_update,
    new: |cap| Dash::with_capacity_and_hasher(cap, Build::default()),
    get: |m, k| m.get(&k).map(|r| *r),
    insert: |m, k, v| m.insert(k, v).is_none(),
    remove: |m, k| m.remove(&k).is_some(),
    update: |m, k| m.get_mut(&k).map(|mut r| *r += 1).is_some());

type Scc = scc::HashMap<u64, u64, Build>;
entry_points!(Scc, "scc", 0,
    scc_name, scc_flags, scc_create, scc_destroy, scc_enter, scc_leave,
    scc_get, scc_insert, scc_remove, scc_update,
    new: |cap| Scc::with_capacity_and_hasher(cap, Build::default()),
    get: |m, k| m.read_sync(&k, |_, v| *v),
    insert: |m, k, v| m.upsert_sync(k, v).is_none(),
    remove: |m, k| m.remove_sync(&k).is_some(),
    update: |m, k| m.update_sync(&k, |_, v| *v += 1).is_some());

/// The standard table, reached by one thread only (the driver runs a floor at
/// one thread).
pub struct StdMap(UnsafeCell<std::collections::HashMap<u64, u64, Build>>);

impl StdMap {
    #[allow(clippy::mut_from_ref)]
    unsafe fn table(&self) -> &mut std::collections::HashMap<u64, u64, Build> {
        &mut *self.0.get()
    }
}

entry_points!(StdMap, "stdmap", ONE_THREAD,
    stdmap_name, stdmap_flags, stdmap_create, stdmap_destroy, stdmap_enter, stdmap_leave,
    stdmap_get, stdmap_insert, stdmap_remove, stdmap_update,
    new: |cap| StdMap(UnsafeCell::new(std::collections::HashMap::with_capacity_and_hasher(cap, Build::default()))),
    get: |m, k| m.table().get(&k).copied(),
    insert: |m, k, v| m.table().insert(k, v).is_none(),
    remove: |m, k| m.table().remove(&k).is_some(),
    update: |m, k| m.table().get_mut(&k).map(|v| *v += 1).is_some());
