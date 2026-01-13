//! Revision tracking for query invalidation.
//!
//! Revisions are monotonically increasing counters that track when inputs change.
//! Each query result is stamped with the revision when it was computed, allowing
//! efficient staleness checks.

use std::sync::atomic::{AtomicU64, Ordering};

/// A revision number representing a point in time in the query system.
///
/// Revisions are monotonically increasing - a higher revision means
/// a more recent computation. Used for:
/// - Tracking when query results were computed
/// - Detecting stale cached values
/// - Ordering concurrent updates
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Revision(u64);

impl Revision {
    /// The initial revision (before any inputs are set).
    pub const INITIAL: Self = Self(0);

    /// Create a revision from a raw value.
    #[inline]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Get the raw revision value.
    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Check if this revision is newer than another.
    #[inline]
    pub const fn is_newer_than(self, other: Self) -> bool {
        self.0 > other.0
    }
}

/// Atomic revision counter for thread-safe revision management.
#[derive(Debug)]
pub struct RevisionCounter {
    current: AtomicU64,
}

impl RevisionCounter {
    /// Create a new revision counter starting at the initial revision.
    #[inline]
    pub const fn new() -> Self {
        Self {
            current: AtomicU64::new(Revision::INITIAL.0),
        }
    }

    /// Get the current revision.
    #[inline]
    pub fn current(&self) -> Revision {
        Revision(self.current.load(Ordering::Acquire))
    }

    /// Increment and return the new revision.
    ///
    /// This should be called whenever an input changes.
    #[inline]
    pub fn increment(&self) -> Revision {
        let new_value = self.current.fetch_add(1, Ordering::AcqRel) + 1;
        Revision(new_value)
    }
}

impl Default for RevisionCounter {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
