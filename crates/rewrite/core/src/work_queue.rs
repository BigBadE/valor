//! Priority-based work queue for parallel task execution.

use crossbeam::queue::SegQueue;
use std::sync::Arc;

/// Priority level for work items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Critical priority - viewport rendering, user interactions
    Critical,
    /// High priority - near viewport, initial load
    High,
    /// Low priority - off-screen, background work
    Low,
}

/// A work function that can be executed.
type WorkFn = Box<dyn FnOnce() + Send + 'static>;

/// Priority-based work queue.
///
/// Workers pull from critical → high → low in order.
/// Lock-free implementation using crossbeam SegQueue.
#[derive(Clone)]
pub struct WorkQueue {
    critical: Arc<SegQueue<WorkFn>>,
    high: Arc<SegQueue<WorkFn>>,
    low: Arc<SegQueue<WorkFn>>,
}

impl WorkQueue {
    /// Create a new work queue.
    pub fn new() -> Self {
        Self {
            critical: Arc::new(SegQueue::new()),
            high: Arc::new(SegQueue::new()),
            low: Arc::new(SegQueue::new()),
        }
    }

    /// Create a new work queue and start worker threads.
    /// Workers will continuously poll for work from the queue.
    pub fn new_with_workers() -> Self {
        let queue = Self::new();
        queue.start_workers();
        queue
    }

    /// Start worker threads that pull from the work queue.
    /// Spawns one worker thread per CPU core using Rayon.
    pub fn start_workers(&self) {
        let work_queue = self.clone();

        for _ in 0..rayon::current_num_threads() {
            let queue = work_queue.clone();

            rayon::spawn(move || {
                loop {
                    if let Some(work) = queue.pop() {
                        work();
                    } else {
                        std::thread::yield_now();
                    }
                }
            });
        }
    }

    /// Push work to the queue with the given priority.
    pub fn push(&self, priority: Priority, work: impl FnOnce() + Send + 'static) {
        let boxed = Box::new(work);
        match priority {
            Priority::Critical => self.critical.push(boxed),
            Priority::High => self.high.push(boxed),
            Priority::Low => self.low.push(boxed),
        }
    }

    /// Pop work from the queue, checking critical → high → low.
    pub fn pop(&self) -> Option<WorkFn> {
        self.critical
            .pop()
            .or_else(|| self.high.pop())
            .or_else(|| self.low.pop())
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.critical.is_empty() && self.high.is_empty() && self.low.is_empty()
    }

    /// Clear all work from the queue.
    /// This pops and drops all pending work items.
    pub fn clear(&self) {
        while self.pop().is_some() {}
    }
}

impl Default for WorkQueue {
    fn default() -> Self {
        Self::new()
    }
}
