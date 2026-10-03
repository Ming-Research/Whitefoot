//! Allocation observer for the explicit ecosystem accounting images only.
//! The five family adapters include this module under `cfg(account_only)`.
//! Timed images retain the ordinary allocator without these hooks. Retire this
//! module with the comparison or when a maintained runner supersedes it.

use std::alloc::{GlobalAlloc, Layout, System};

unsafe extern "C" {
    fn wf_ecosystem_note_alloc(bytes: u64);
    fn wf_ecosystem_note_dealloc(bytes: u64);
    fn wf_ecosystem_note_realloc(old_bytes: u64, new_bytes: u64);
}

struct ObservedSystem;

// SAFETY: Every pointer and Layout is passed unchanged to System. The hooks
// only update the C driver's counters and never allocate, deallocate, or retain
// a pointer. The experiment runs one whole trace at a time on one thread.
unsafe impl GlobalAlloc for ObservedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            unsafe { wf_ecosystem_note_alloc(layout.size() as u64) };
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            unsafe { wf_ecosystem_note_alloc(layout.size() as u64) };
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { wf_ecosystem_note_dealloc(layout.size() as u64) };
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            // The allocator may resize in place. The logical live request
            // changes old -> new; hidden transient backing is not measured.
            unsafe { wf_ecosystem_note_realloc(layout.size() as u64, new_size as u64) };
        }
        replacement
    }
}

#[global_allocator]
static ALLOCATOR: ObservedSystem = ObservedSystem;
