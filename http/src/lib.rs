//! A simple thread pool.
//!
//! This is the naive-but-educational version from the Rust book's
//! web server chapter. It's not meant to be production grade — it's
//! meant to teach the mechanics of:
//!
//! - spawning a fixed number of worker threads
//! - sharing work via a channel (`mpsc`)
//! - safely sending `Job`s to workers with `Arc` + `Mutex`
//! - shutting down cleanly with `Drop`
//!
//! There are a few compromises I made on purpose (like using `unwrap`
//! instead of graceful error handling, and holding the lock while
//! `recv`ing) — those are the kinds of details you'd tighten up in a
//! real server, but they'd obscure the core ideas if I did them here.

use std::{
    sync::{mpsc, Arc, Mutex},
    thread,
};

/// A `Job` is just a boxed closure that the workers will run.
///
/// `Send + 'static` means: the closure can be safely moved to another
/// thread, and it owns (or has `'static` access to) everything it
/// needs — it can't borrow anything that might go away.
type Job = Box<dyn FnOnce() + Send + 'static>;

/// The thread pool. Owns a sender to push jobs onto a shared queue,
/// and a `Vec` of worker handles.
pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

impl ThreadPool {
    /// Create a new `ThreadPool`.
    ///
    /// `size` is the number of threads in the pool.
    ///
    /// # Panics
    ///
    /// The `new` function will panic if `size` is zero. A pool with no
    /// workers is a programming error — there'd be no one to do the work.
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0, "ThreadPool must have at least 1 worker thread");

        // The channel is my work queue. Workers all share the same
        // receiver, wrapped in `Arc<Mutex<...>>` so they can take turns
        // pulling jobs off it.
        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));

        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        ThreadPool {
            workers,
            sender: Some(sender),
        }
    }

    /// Send a closure to the pool to be executed by a worker.
    ///
    /// This is `FnOnce` because the job runs exactly once. `Send +
    /// 'static` because it moves to (possibly) another thread.
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);

        // I `unwrap` here because `send` only fails if every receiver
        // has been dropped — i.e. the pool is being torn down. In this
        // educational server I treat that as "I'm shutting down,
        // just drop the job".
        self.sender.as_ref().unwrap().send(job).unwrap();
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        // Dropping the sender closes the channel. Workers blocked in
        // `recv()` will get an error and exit their loop.
        drop(self.sender.take());

        // Join each worker so I don't leak threads on shutdown.
        for worker in &mut self.workers {
            println!("Shutting down worker {}", worker.id);

            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

/// A single worker thread. Loops forever, pulling jobs off the shared
/// queue and running them, until the channel is closed.
struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
        let thread = thread::spawn(move || loop {
            // I lock the mutex, then block on `recv()` while holding
            // the lock. The `MutexGuard` is a temporary that lives
            // until the end of this `let` statement, so the lock is
            // held *while I wait* for a job. That means only one
            // worker can be waiting at a time — the others spin on the
            // mutex. Fine for learning; a production server would use
            // a different mechanism (e.g. `crossbeam` or async).
            let job = receiver
                .lock()
                .unwrap()
                .recv()
                .unwrap();

            println!("Worker {id} got a job; executing.");

            job();
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}
