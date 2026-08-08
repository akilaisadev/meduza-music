//! Bounded background worker pools + a single shared Tokio runtime.
//!
//! Every background task in the app flows through one of these fixed-size
//! pools instead of a bare `std::thread::spawn`:
//!
//! * thread count is bounded (crucial on low-end PCs where thread explosions
//!   and per-call Tokio runtime spin-up were the main CPU/RAM culprits),
//! * each pool is sized for its workload, so long-running jobs (downloads,
//!   pre-loader waits) can never starve short latency-sensitive jobs (radio
//!   fetches, URL warm-ups),
//! * the queue is bounded — when the app is already slammed, extra work is
//!   dropped rather than piled up and executed later with stale data.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

type Job = Box<dyn FnOnce() + Send + 'static>;

/// Bounded FIFO blocked by a Condvar; `try_push` returns false when full so a
/// swamped pool drops work instead of piling it up.
struct JobQueue {
    inner: Mutex<VecDeque<Job>>,
    cv: Condvar,
    capacity: usize,
}

impl JobQueue {
    fn new(capacity: usize) -> Self {
        Self { inner: Mutex::new(VecDeque::with_capacity(capacity)), cv: Condvar::new(), capacity }
    }

    fn try_push(&self, job: Job) -> bool {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if q.len() >= self.capacity {
            return false; // pool is slammed — drop the work rather than queue it
        }
        q.push_back(job);
        self.cv.notify_one();
        true
    }

    fn pop(&self) -> Job {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(job) = q.pop_front() {
                return job;
            }
            q = self.cv.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// A fixed-size worker pool with a bounded, drop-when-full task queue.
pub struct WorkerPool {
    queue: Arc<JobQueue>,
    _handles: Vec<std::thread::JoinHandle<()>>,
}

impl WorkerPool {
    /// Spawn `threads` workers with small stacks (256 KiB) to reduce resident
    /// memory on low-end machines.
    pub fn new(name: &'static str, threads: usize) -> Self {
        let queue = Arc::new(JobQueue::new(threads * 4));
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let q = Arc::clone(&queue);
            handles.push(
                std::thread::Builder::new()
                    .name(format!("meduza-{name}"))
                    .stack_size(256 * 1024)
                    .spawn(move || loop {
                        let job = q.pop();
                        job();
                    })
                    .expect("failed to spawn worker"),
            );
        }
        Self { queue, _handles: handles }
    }

    /// Queue a task. If the bounded queue is full the task is silently dropped
    /// (the machine is already busy; executing stale work would only hurt).
    pub fn submit<F>(&self, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let _ = self.queue.try_push(Box::new(job));
    }
}

/// Short, latency-sensitive background work: radio fetches, radio refills,
/// stream-URL warm-ups, resolve+play, auto-advance.
fn io_pool() -> &'static WorkerPool {
    static IO: OnceLock<WorkerPool> = OnceLock::new();
    IO.get_or_init(|| WorkerPool::new("io", 4))
}

/// Long-running background work that holds a thread for a long time: data
/// saver downloads, pre-loader wait-then-append. Kept separate from `io` so a
/// big download can never block a stream resolve.
fn download_pool() -> &'static WorkerPool {
    static DL: OnceLock<WorkerPool> = OnceLock::new();
    DL.get_or_init(|| WorkerPool::new("dl", 2))
}

/// CPU-heavy decodes (image decode → RGBA, dominant-color extraction).
fn decode_pool() -> &'static WorkerPool {
    static DECODE: OnceLock<WorkerPool> = OnceLock::new();
    DECODE.get_or_init(|| WorkerPool::new("decode", 1))
}

pub fn io() -> &'static WorkerPool { io_pool() }
pub fn download() -> &'static WorkerPool { download_pool() }
pub fn decode() -> &'static WorkerPool { decode_pool() }

// ── Shared Tokio runtime for all InnerTube (async) work ─────────────────────
// Previously a brand-new current-thread runtime was created inside every
// per-track thread — a notable CPU/idle-cost on low-end machines. One small
// multi-threaded runtime is shared process-wide instead.

pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("failed to start shared Tokio runtime")
    })
}

/// Block on an async future using the shared runtime. Meant to be called from
/// non-Tokio threads (pool workers, the UI thread), never from inside the
/// runtime itself.
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    runtime().block_on(future)
}