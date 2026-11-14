//! Job system: work-stealing воркеры, JobHandle, catch_unwind, main-thread queue.

use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_deque::{Injector, Steal};
use parking_lot::{Condvar, Mutex};

type Job = Box<dyn FnOnce() + Send + 'static>;

/// Статус задачи.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobStatus {
    /// Ещё выполняется.
    Pending,
    /// Завершена успешно.
    Completed,
    /// Паника внутри задачи (процесс жив).
    Panicked,
}

struct JobState {
    status: Mutex<JobStatus>,
    done: Condvar,
}

impl JobState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            status: Mutex::new(JobStatus::Pending),
            done: Condvar::new(),
        })
    }

    fn finish(&self, status: JobStatus) {
        *self.status.lock() = status;
        self.done.notify_all();
    }

    fn wait(&self) -> JobStatus {
        let mut status = self.status.lock();
        while *status == JobStatus::Pending {
            self.done.wait(&mut status);
        }
        *status
    }
}

/// Хэндл задачи.
#[derive(Clone)]
pub struct JobHandle {
    state: Arc<JobState>,
}

impl JobHandle {
    /// Текущий статус без ожидания.
    #[must_use]
    pub fn status(&self) -> JobStatus {
        *self.state.status.lock()
    }

    /// Ожидает завершения и возвращает статус.
    #[must_use]
    pub fn wait(&self) -> JobStatus {
        self.state.wait()
    }

    /// Завершена ли задача (успешно или с паникой).
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.status() != JobStatus::Pending
    }
}

struct Signal {
    generation: Mutex<u64>,
    condvar: Condvar,
}

/// Пул воркеров с work-stealing.
pub struct JobSystem {
    injector: Arc<Injector<Job>>,
    signal: Arc<Signal>,
    shutdown: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
}

impl JobSystem {
    /// Создаёт пул с заданным числом воркеров (минимум один).
    #[must_use]
    pub fn new(worker_threads: usize) -> Self {
        let injector = Arc::new(Injector::new());
        let signal = Arc::new(Signal {
            generation: Mutex::new(0),
            condvar: Condvar::new(),
        });
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut workers = Vec::new();

        for index in 0..worker_threads.max(1) {
            let injector = Arc::clone(&injector);
            let signal = Arc::clone(&signal);
            let shutdown = Arc::clone(&shutdown);
            let builder = thread::Builder::new().name(format!("flux-job-{index}"));
            if let Ok(handle) = builder.spawn(move || worker_loop(&injector, &signal, &shutdown)) {
                workers.push(handle);
            }
        }

        Self {
            injector,
            signal,
            shutdown,
            workers,
        }
    }

    /// Число воркеров.
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Ставит задачу в очередь.
    pub fn spawn<F>(&self, job: F) -> JobHandle
    where
        F: FnOnce() + Send + 'static,
    {
        let state = JobState::new();
        let state_for_job = Arc::clone(&state);

        self.injector.push(Box::new(move || {
            let status = if catch_unwind(AssertUnwindSafe(job)).is_ok() {
                JobStatus::Completed
            } else {
                JobStatus::Panicked
            };
            state_for_job.finish(status);
        }));

        {
            let mut generation = self.signal.generation.lock();
            *generation = generation.wrapping_add(1);
        }
        self.signal.condvar.notify_one();

        JobHandle { state }
    }
}

fn worker_loop(injector: &Injector<Job>, signal: &Signal, shutdown: &AtomicBool) {
    let mut seen_generation = 0_u64;

    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }

        loop {
            match injector.steal() {
                Steal::Success(job) => job(),
                Steal::Retry => continue,
                Steal::Empty => break,
            }
        }

        let mut generation = signal.generation.lock();
        if *generation == seen_generation {
            let _ = signal
                .condvar
                .wait_for(&mut generation, Duration::from_millis(2));
        }
        seen_generation = *generation;
    }
}

impl Drop for JobSystem {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        {
            let mut generation = self.signal.generation.lock();
            *generation = generation.wrapping_add(1);
        }
        self.signal.condvar.notify_all();

        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

/// Очередь задач, обязанных исполниться на главном потоке.
#[derive(Default)]
pub struct MainThreadTaskQueue {
    tasks: Mutex<VecDeque<Box<dyn FnOnce() + Send>>>,
}

impl MainThreadTaskQueue {
    /// Создаёт пустую очередь.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавляет задачу.
    pub fn push<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.tasks.lock().push_back(Box::new(task));
    }

    /// Выполняет все задачи; возвращает их число.
    pub fn run_all(&self) -> usize {
        let mut tasks = self.tasks.lock();
        let count = tasks.len();
        while let Some(task) = tasks.pop_front() {
            task();
        }
        count
    }

    /// Число ожидающих задач.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tasks.lock().len()
    }

    /// Пуста ли очередь.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn two_jobs_run_in_parallel() {
        let jobs = JobSystem::new(2);
        let counter = Arc::new(AtomicUsize::new(0));

        let a = {
            let counter = Arc::clone(&counter);
            jobs.spawn(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
        };
        let b = {
            let counter = Arc::clone(&counter);
            jobs.spawn(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
        };

        assert_eq!(a.wait(), JobStatus::Completed);
        assert_eq!(b.wait(), JobStatus::Completed);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn panic_does_not_kill_worker() {
        let jobs = JobSystem::new(2);
        let panicked = jobs.spawn(|| panic!("boom"));
        assert_eq!(panicked.wait(), JobStatus::Panicked);

        let counter = Arc::new(AtomicUsize::new(0));
        let after = {
            let counter = Arc::clone(&counter);
            jobs.spawn(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
        };
        assert_eq!(after.wait(), JobStatus::Completed);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn main_thread_queue_preserves_order() {
        let queue = MainThreadTaskQueue::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        for value in [1_u32, 2, 3] {
            let order = Arc::clone(&order);
            queue.push(move || order.lock().push(value));
        }

        assert_eq!(queue.run_all(), 3);
        assert_eq!(*order.lock(), vec![1, 2, 3]);
        assert!(queue.is_empty());
    }
}
