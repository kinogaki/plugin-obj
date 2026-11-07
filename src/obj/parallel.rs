//! A minimal ordered parallel map over scoped threads: jobs are taken in order
//! by whichever worker is free, and results come back in job order.
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

/// `f` applied to every job on up to `threads` threads (the caller's included).
pub fn map<T: Send, R: Send>(jobs: Vec<T>, threads: usize, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    let count = jobs.len();
    if threads <= 1 || count <= 1 {
        return jobs.into_iter().map(f).collect();
    }
    let slots: Vec<Mutex<Option<T>>> = jobs.into_iter().map(|job| Mutex::new(Some(job))).collect();
    let results: Vec<Mutex<Option<R>>> = (0..count).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let work = || {
        loop {
            let at = next.fetch_add(1, Ordering::Relaxed);
            if at >= count {
                break;
            }
            let job = lock(&slots[at]).take().expect("each job is taken once");
            let result = f(job);
            *lock(&results[at]) = Some(result);
        }
    };
    std::thread::scope(|scope| {
        for _ in 1..threads.min(count) {
            scope.spawn(work);
        }
        work();
    });
    results
        .into_iter()
        .map(|slot| {
            slot.into_inner()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .expect("every job ran")
        })
        .collect()
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
