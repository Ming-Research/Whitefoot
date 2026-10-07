#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! The Whitefoot research compiler.
//!
//! The crate contains one path for the active specification, from ordered sources through the
//! frontend and direct resolver into semantic and ownership checking, a
//! typed control-flow IR with selected-target optional loop actualization,
//! conservative textual LLVM, and a
//! host compiler executable. These stages remain evolvable implementation
//! APIs, not stable protocols.

mod backend;
mod cycles;
mod driver;
mod graph;
mod ir;
mod lexer;
mod library;
mod lowering;
mod prelude;
mod resolution;
mod semantic;
mod source;
mod spec;
/// Machine-derived identity of the embedded active specification, computed by
/// `build.rs` from those bytes. Not committed: the bytes are the only copy.
pub mod spec_identity {
    include!(concat!(env!("OUT_DIR"), "/spec_identity.rs"));
}
mod syntax;
mod target;
mod toolchain;

pub use toolchain::clang_executable;

/// The stack each worker of [`in_parallel`] runs on, as large as the
/// compiler driver's own: a worker runs the same recursive checks.
const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

/// Worker threads every [`in_parallel`] call together has running, beyond
/// the threads that called it, so nested calls stay within the processors.
static WORKERS_IN_USE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Returns its reserved workers when the call ends, even by a panic.
struct WorkerReservation(usize);

impl Drop for WorkerReservation {
    fn drop(&mut self) {
        WORKERS_IN_USE.fetch_sub(self.0, std::sync::atomic::Ordering::AcqRel);
    }
}

/// `work` applied to every item, with the results in item order. The calling
/// thread takes items alongside up to one worker per further available
/// processor that no other call holds; with none free, it runs every item.
pub(crate) fn in_parallel<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::atomic::Ordering;
    let spare = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .saturating_sub(1);
    let wanted = spare.min(items.len().saturating_sub(1));
    let mut used = WORKERS_IN_USE.load(Ordering::Acquire);
    let reserved = loop {
        let take = wanted.min(spare.saturating_sub(used));
        if take == 0 {
            break 0;
        }
        match WORKERS_IN_USE.compare_exchange(
            used,
            used + take,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => break take,
            Err(current) => used = current,
        }
    };
    if reserved == 0 {
        return items.iter().map(work).collect();
    }
    // Each worker returns its reservation as soon as no item is left for
    // it, so a nested call made by a slower item can use the freed share.
    let reservations = (0..reserved)
        .map(|_| WorkerReservation(1))
        .collect::<Vec<_>>();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let take_items = || {
        let mut done = Vec::new();
        loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            let Some(item) = items.get(index) else {
                break done;
            };
            done.push((index, work(item)));
        }
    };
    let finished = std::thread::scope(|scope| {
        let workers = reservations
            .into_iter()
            .map(|reservation| {
                std::thread::Builder::new()
                    .stack_size(WORKER_STACK_BYTES)
                    .spawn_scoped(scope, move || {
                        let done = take_items();
                        drop(reservation);
                        done
                    })
            })
            .collect::<Vec<_>>();
        // A thread the host refuses leaves its share to the others.
        let mut finished = take_items();
        for worker in workers.into_iter().flatten() {
            finished.extend(
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            );
        }
        finished
    });
    let mut slots = items.iter().map(|_| None).collect::<Vec<Option<R>>>();
    for (index, result) in finished {
        slots[index] = Some(result);
    }
    slots
        .into_iter()
        .map(|slot| slot.expect("every item is taken by some thread"))
        .collect()
}

// Unit and integration tests use the same immutable native-object builder.
// This alias lets the shared test module name the existing exported inputs.
#[cfg(test)]
extern crate self as whitefoot;
#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod native_test_support;

/// The parallel runtime a module that hands work out must be linked against,
/// and the predicate that decides whether one must.
pub use backend::{
    Architecture, COMPLETION_BRIDGE_HEADER, COMPLETION_BRIDGE_SOURCE, COMPLETION_CONTRACT_HEADER,
    COMPLETION_FILE_ADAPTER_HEADER, COMPLETION_FILE_ADAPTER_SOURCE, COMPLETION_FILE_POSIX_HEADER,
    COMPLETION_FILE_POSIX_SOURCE, COMPLETION_FILE_WINDOWS_SOURCE, COMPLETION_LINUX_IO_URING_HEADER,
    COMPLETION_LINUX_IO_URING_SOURCE, COMPLETION_RUNTIME_SOURCE, COMPLETION_SOCKET_ADDRESS_HEADER,
    COMPLETION_WAIT_HOST_SOURCE, COMPLETION_WAIT_WINDOWS_SOURCE, COMPLETION_WINDOWS_IOCP_HEADER,
    COMPLETION_WINDOWS_IOCP_SOURCE, CONCURRENT_MAP_HEADER, CONCURRENT_MAP_SOURCE,
    DISPATCH_LEDGER_PREFIX, FLOOR_RUNTIME_SOURCE, FLOOR_STACK_BYTES, FLOOR_WINDOWS_RUNTIME_SOURCE,
    KEYED_TABLE_SOURCE, ORDINARY_VALUES_HEADER, ORDINARY_VALUES_LLVM, ORDINARY_VALUES_SOURCE,
    SCHED_CORE_HEADER, SCHED_CORE_SOURCE, SCHED_ENTRY_HEADER, SCHED_ENTRY_SOURCE,
    SCHED_PRIM_HEADER, SCHED_PRIM_HOST_SOURCE, SCHED_PRIM_WINDOWS_SOURCE, WINDOWS_RUNTIME_HEADER,
    WINDOWS_RUNTIME_SOURCE, module_requires_parallel_runtime, stack_ledger,
};
pub use backend::{FragmentGranularity, LlvmModule, SplitFailure, split_module};
pub use driver::*;
pub use graph::*;
/// Where one call into an actualized recursive component starts counting the
/// levels that may still hand work out.
pub use ir::RecursionBudget;
pub use lexer::*;
/// The compile-time choice of whether the backend actualizes the permission
/// judgment's overlap groups.
pub use lowering::{CallGrain, OverlapLowering};
pub use resolution::*;
pub use source::*;
pub use spec::*;
pub use syntax::grammar::*;
pub use syntax::terminal::*;
pub use syntax::*;

pub(crate) use backend::*;
pub(crate) use ir::*;
pub(crate) use lowering::*;
pub(crate) use semantic::*;
