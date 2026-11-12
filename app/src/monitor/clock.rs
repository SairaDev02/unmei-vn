//! The admission clock the capture path stamps lines with.

use std::time::Instant;

/// The monotonic millisecond clock the capture path stamps admission time with.
///
/// The production implementation is [`SystemClock`]; tests supply a controllable
/// fake so elapsed metadata is deterministic without sleeps.
pub trait MonitorClock: Send + Sync + 'static {
    /// Monotonic milliseconds since an arbitrary, fixed origin.
    fn now_millis(&self) -> u64;
}

/// The production [`MonitorClock`]: milliseconds since the process-lifetime
/// instant the clock was constructed.
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorClock for SystemClock {
    fn now_millis(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }
}
