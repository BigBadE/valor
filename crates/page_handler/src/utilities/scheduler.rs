use core::time::Duration;
use std::time::Instant;

/// Simple frame scheduler to coalesce layout work per frame with a given time budget.
///
/// This scheduler prevents excessive layout recomputation by enforcing a minimum
/// time interval between layout passes. Layout requests made within the budget
/// window are deferred until the next frame boundary.
pub struct FrameScheduler {
    /// The minimum time interval between layout passes.
    budget: Duration,
    /// Timestamp of the most recent frame start that was allowed.
    last_frame_start: Option<Instant>,
    /// Number of times a layout request was deferred due to frame budget limits (spillover).
    deferred_count: u64,
}

impl FrameScheduler {
    /// Creates a new frame scheduler with the specified time budget.
    ///
    /// # Arguments
    ///
    /// * `budget` - The minimum duration between layout passes.
    #[inline]
    #[must_use]
    pub const fn new(budget: Duration) -> Self {
        Self {
            budget,
            last_frame_start: None,
            deferred_count: 0,
        }
    }

    /// Returns the configured frame budget duration.
    #[inline]
    #[must_use]
    pub const fn budget(&self) -> Duration {
        self.budget
    }

    /// Checks if a new frame budget window has started.
    ///
    /// Returns `true` if sufficient time has elapsed since the last frame start
    /// and layout can proceed now. Returns `false` if the current frame budget
    /// has not yet expired.
    #[inline]
    #[must_use]
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

    /// Increments the count of deferred layout attempts.
    ///
    /// This should be called each time a layout request is denied due to
    /// frame budget constraints.
    #[inline]
    pub const fn incr_deferred(&mut self) {
        self.deferred_count = self.deferred_count.saturating_add(1);
    }

    /// Returns the total number of deferred layout attempts since creation.
    #[inline]
    #[must_use]
    pub const fn deferred(&self) -> u64 {
        self.deferred_count
    }
}
