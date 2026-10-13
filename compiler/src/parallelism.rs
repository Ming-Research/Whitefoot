//! One in-process budget for checker workers and native-build jobs.

use std::sync::atomic::{AtomicUsize, Ordering};

const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

/// Additional workers beyond each operation's calling thread.
struct WorkerBudget {
    in_use: AtomicUsize,
}

impl WorkerBudget {
    const fn new() -> Self {
        Self {
            in_use: AtomicUsize::new(0),
        }
    }

    fn reserve(&self, maximum: usize, capacity: usize) -> WorkerReservation<'_> {
        let spare = capacity.saturating_sub(1);
        let wanted = maximum.saturating_sub(1).min(spare);
        let mut used = self.in_use.load(Ordering::Acquire);
        let count = loop {
            let take = wanted.min(spare.saturating_sub(used));
            if take == 0 {
                break 0;
            }
            match self.in_use.compare_exchange(
                used,
                used + take,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break take,
                Err(current) => used = current,
            }
        };
        WorkerReservation {
            budget: self,
            count,
        }
    }
}

static WORKERS: WorkerBudget = WorkerBudget::new();

struct WorkerReservation<'a> {
    budget: &'a WorkerBudget,
    count: usize,
}

impl<'a> WorkerReservation<'a> {
    fn split(mut self) -> Vec<Self> {
        let workers = (0..self.count)
            .map(|_| Self {
                budget: self.budget,
                count: 1,
            })
            .collect();
        self.count = 0;
        workers
    }
}

impl Drop for WorkerReservation<'_> {
    fn drop(&mut self) {
        self.budget.in_use.fetch_sub(self.count, Ordering::AcqRel);
    }
}

fn capacity() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// Applies `work` to every item and returns results in input order, sharing
/// additional workers with all concurrent compiler operations in this process.
/// The calling thread also takes work; each worker returns its share as soon
/// as it runs out of items, including during unwinding.
pub fn in_parallel<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    in_parallel_with_limit(items, usize::MAX, work)
}

/// As [`in_parallel`], with at most `maximum` jobs including the calling thread.
/// The shared budget may grant fewer jobs; a zero limit still permits the caller.
pub fn in_parallel_with_limit<T: Sync, R: Send>(
    items: &[T],
    maximum: usize,
    work: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    parallel_with_budget(&WORKERS, capacity(), items, maximum, work)
}

/// Reserves the shared compiler budget for an external operation such as a
/// ThinLTO link, passes its actual job count to `work`, and returns the share
/// when `work` finishes or unwinds. The count includes the operation's caller
/// and is at least one, even when no additional workers are free.
pub fn with_compiler_parallelism<R>(maximum: usize, work: impl FnOnce(usize) -> R) -> R {
    with_budget(&WORKERS, capacity(), maximum, work)
}

fn with_budget<R>(
    budget: &WorkerBudget,
    capacity: usize,
    maximum: usize,
    work: impl FnOnce(usize) -> R,
) -> R {
    let reservation = budget.reserve(maximum.max(1), capacity);
    let result = work(reservation.count + 1);
    drop(reservation);
    result
}

fn parallel_with_budget<T: Sync, R: Send>(
    budget: &WorkerBudget,
    capacity: usize,
    items: &[T],
    maximum: usize,
    work: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let reservation = budget.reserve(maximum.max(1).min(items.len()), capacity);
    if reservation.count == 0 {
        return items.iter().map(work).collect();
    }
    let reservations = reservation.split();
    let next = AtomicUsize::new(0);
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
        // A refused spawn drops its closure and reservation; remaining jobs
        // still take every item through the same shared counter.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Mutex, mpsc};

    const DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

    /// An isolated instance of the production budget fixes the capacity even
    /// when other tests hold the process-wide budget or CI has one processor.
    #[test]
    fn pool_is_bounded_parallel_and_returns_input_order() {
        let budget = WorkerBudget::new();
        let (started, starts) = mpsc::channel();
        let (release, releases) = mpsc::channel();
        let releases = Mutex::new(releases);
        let active = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let pool = scope.spawn(|| {
                parallel_with_budget(&budget, 4, &[0, 1, 2, 3, 4, 5], 3, |value| {
                    let running = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(running, Ordering::SeqCst);
                    started
                        .send(std::thread::current().id())
                        .expect("report worker");
                    if *value < 3 {
                        releases
                            .lock()
                            .unwrap()
                            .recv_timeout(DEADLINE)
                            .expect("worker released");
                    }
                    active.fetch_sub(1, Ordering::SeqCst);
                    value * 2
                })
            });
            let workers = (0..3)
                .map(|_| starts.recv_timeout(DEADLINE).expect("three jobs overlap"))
                .collect::<HashSet<_>>();
            assert_eq!(
                workers.len(),
                3,
                "three distinct workers run the first jobs"
            );
            assert!(starts.try_recv().is_err());
            for _ in 0..3 {
                release.send(()).expect("release worker");
            }
            assert_eq!(pool.join().expect("pool joins"), vec![0, 2, 4, 6, 8, 10]);
            assert_eq!(
                maximum.load(Ordering::SeqCst),
                3,
                "requested ceiling bounds workers"
            );
        });
        assert_eq!(budget.in_use.load(Ordering::Acquire), 0);
    }

    #[test]
    fn external_jobs_hold_the_same_budget_as_the_pool() {
        let budget = WorkerBudget::new();
        let (held, holds) = mpsc::channel();
        let (release, releases) = mpsc::channel();
        let releases = Mutex::new(releases);
        std::thread::scope(|scope| {
            let external = scope.spawn(|| {
                with_budget(&budget, 3, 3, |jobs| {
                    held.send(jobs).expect("report reservation");
                    releases
                        .lock()
                        .unwrap()
                        .recv_timeout(DEADLINE)
                        .expect("release external jobs");
                });
            });
            assert_eq!(
                holds.recv_timeout(DEADLINE).expect("external jobs start"),
                3
            );
            let caller = std::thread::current().id();
            let results =
                parallel_with_budget(&budget, 3, &[0, 1, 2], 3, |_| std::thread::current().id());
            release.send(()).expect("release external jobs");
            external.join().expect("external operation joins");
            assert_eq!(
                results,
                vec![caller; 3],
                "a held link leaves no additional pool workers"
            );
        });
        assert_eq!(
            with_budget(&budget, 3, 3, |jobs| jobs),
            3,
            "external completion returns the share"
        );
    }

    #[test]
    fn pool_jobs_hold_the_same_budget_as_external_jobs() {
        let budget = WorkerBudget::new();
        let (started, starts) = mpsc::channel();
        let (release, releases) = mpsc::channel();
        let releases = Mutex::new(releases);
        std::thread::scope(|scope| {
            let pool = scope.spawn(|| {
                parallel_with_budget(&budget, 3, &[0, 1, 2], 3, |value| {
                    started.send(()).expect("report item");
                    releases
                        .lock()
                        .unwrap()
                        .recv_timeout(DEADLINE)
                        .expect("release item");
                    *value
                })
            });
            for _ in 0..3 {
                starts
                    .recv_timeout(DEADLINE)
                    .expect("three pool jobs start");
            }
            let granted = with_budget(&budget, 3, 3, |jobs| jobs);
            for _ in 0..3 {
                release.send(()).expect("release item");
            }
            assert_eq!(pool.join().expect("pool joins"), vec![0, 1, 2]);
            assert_eq!(
                granted, 1,
                "a held pool leaves the link only its caller share"
            );
        });
        assert_eq!(with_budget(&budget, 3, 3, |jobs| jobs), 3);
    }

    #[test]
    fn finished_workers_return_shares_before_a_slow_caller_finishes() {
        let budget = WorkerBudget::new();
        let (started, starts) = mpsc::channel();
        let (release_worker, worker_releases) = mpsc::channel();
        let worker_releases = Mutex::new(worker_releases);
        let (release_caller, caller_releases) = mpsc::channel();
        let caller_releases = Mutex::new(caller_releases);
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let (returned, returns) = mpsc::channel();
        std::thread::scope(|scope| {
            let pool = scope.spawn(|| {
                let caller = std::thread::current().id();
                parallel_with_budget(&budget, 3, &[0, 1, 2], 3, |value| {
                    let is_caller = std::thread::current().id() == caller;
                    started.send(is_caller).expect("report item");
                    if is_caller {
                        caller_releases
                            .lock()
                            .unwrap()
                            .recv_timeout(DEADLINE)
                            .expect("release slow caller");
                    } else {
                        worker_releases
                            .lock()
                            .unwrap()
                            .recv_timeout(DEADLINE)
                            .expect("release worker");
                    }
                    *value
                })
            });
            let callers = (0..3)
                .filter(|_| starts.recv_timeout(DEADLINE).expect("three items start"))
                .count();
            assert_eq!(callers, 1);
            let observer = scope.spawn(|| {
                while budget.in_use.load(Ordering::Acquire) != 0 {
                    if cancelled.load(Ordering::Acquire) {
                        return;
                    }
                    std::thread::yield_now();
                }
                returned.send(()).expect("report returned worker shares");
            });
            for _ in 0..2 {
                release_worker.send(()).expect("release worker");
            }
            let observed = returns.recv_timeout(DEADLINE);
            let granted = with_budget(&budget, 3, 3, |jobs| jobs);
            cancelled.store(true, Ordering::Release);
            release_caller.send(()).expect("release caller");
            observer.join().expect("observer joins");
            assert_eq!(pool.join().expect("pool joins"), vec![0, 1, 2]);
            observed.expect("workers return shares while the caller is still blocked");
            assert_eq!(granted, 3, "early returned shares are usable immediately");
        });
    }

    #[test]
    fn external_panic_returns_every_reserved_share() {
        let budget = WorkerBudget::new();
        let panic = std::panic::catch_unwind(|| {
            with_budget(&budget, 4, 4, |jobs| {
                assert_eq!(jobs, 4);
                panic!("external operation failed");
            });
        });
        assert!(panic.is_err());
        assert_eq!(with_budget(&budget, 4, 4, |jobs| jobs), 4);
    }
}
