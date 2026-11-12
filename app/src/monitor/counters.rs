//! The capture path's three saturating loss, eviction and truncation counters.

use std::sync::atomic::{AtomicU64, Ordering};

/// The three saturating app-lifetime counters. They survive hiding, filtering,
/// loads and New Game because they live with the capture, not the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MonitorCounters {
    /// Retained-history lines aged out of the 1,000 cap, once per line.
    pub evicted: u64,
    /// Pending/admission events discarded (rejection, completed eviction,
    /// formatting error, reentrant suppression or an unavailable queue).
    pub lost: u64,
    /// Successfully admitted events whose rendered line was truncated.
    pub truncated: u64,
}

#[derive(Default)]
pub(super) struct CounterSet {
    evicted: AtomicU64,
    lost: AtomicU64,
    truncated: AtomicU64,
}

impl CounterSet {
    fn bump(counter: &AtomicU64, amount: u64) {
        let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_add(amount))
        });
    }

    pub(super) fn add_evicted(&self, amount: u64) {
        Self::bump(&self.evicted, amount);
    }

    pub(super) fn add_lost(&self, amount: u64) {
        Self::bump(&self.lost, amount);
    }

    pub(super) fn add_truncated(&self, amount: u64) {
        Self::bump(&self.truncated, amount);
    }

    pub(super) fn snapshot(&self) -> MonitorCounters {
        MonitorCounters {
            evicted: self.evicted.load(Ordering::Relaxed),
            lost: self.lost.load(Ordering::Relaxed),
            truncated: self.truncated.load(Ordering::Relaxed),
        }
    }
}
