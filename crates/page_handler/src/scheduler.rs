use std::time::{Duration, Instant};

/// Simple frame scheduler to coalesce layout work per frame with a given time budget.
pub struct FrameScheduler {
    budget: Duration,
    last_frame_start: Option<Instant>,
    /// Number of times a layout request was deferred due to frame budget limits (spillover).
    deferred_count: u64,
}

impl FrameScheduler {
    pub fn new(budget: Duration) -> Self {
        Self {
            budget,
            last_frame_start: None,
            deferred_count: 0,
        }
    }
    /// Return the configured frame budget duration.
    pub fn budget(&self) -> Duration {
        self.budget
    }
    /// Returns true if a new frame budget window has started and we can run layout now.
    pub fn allow(&mut self) -> bool {
        let now = Instant::now();
        match self.last_frame_start {
            None => {
                self.last_frame_start = Some(now);
                true
            }
            Some(start) => {
                if now.duration_since(start) >= self.budget {
                    self.last_frame_start = Some(now);
                    true
                } else {
                    false
                }
            }
        }
    }
    /// Increment the number of deferred layout attempts due to frame budgeting.
    pub fn incr_deferred(&mut self) {
        self.deferred_count = self.deferred_count.saturating_add(1);
    }
    /// Return the number of times layout was deferred due to budgeting during this session.
    pub fn deferred(&self) -> u64 {
        self.deferred_count
    }
}
