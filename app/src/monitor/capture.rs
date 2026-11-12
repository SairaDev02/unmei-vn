//! The bounded pending queue behind the public capture handle.
//!
//! A producer reserves a slot before formatting anything, inserts the finished
//! line under the completed lock, and the app removes lines for handoff while
//! they stay charged until retained history owns each payload. One shared state,
//! no second buffer.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use bevy::log::BoxedLayer;
use bevy::prelude::*;

use super::clock::MonitorClock;
use super::counters::{CounterSet, MonitorCounters};
use super::layer::CaptureLayer;
use super::line::MonitorLine;
use super::PENDING_CAPACITY;

/// The pending buffer's completed side. Keyed by the insertion-assigned
/// admission identity, so `next_sequence` order is exactly drain order and the
/// oldest completed line is the one freed first.
#[derive(Default)]
pub(super) struct Completed {
    lines: BTreeMap<u64, MonitorLine>,
    next_sequence: u64,
}

/// The state shared between the tracing layer (arbitrary producer threads) and
/// the Bevy app. Kept private; [`MonitorCapture`] is the public handle.
pub(super) struct MonitorShared {
    clock: Arc<dyn MonitorClock>,
    install_ms: u64,
    /// In-flight formatter reservations. An atomic so a reservation can always
    /// be released (including from `Drop` during unwinding) without taking the
    /// completed-lines lock.
    pub(super) reserved: AtomicUsize,
    /// Handoff staging: completed lines removed for ingestion but still charged
    /// against [`PENDING_CAPACITY`] until retained history owns each payload.
    staged: AtomicUsize,
    pub(super) completed: Mutex<Completed>,
    pub(super) counters: CounterSet,
    /// Sticky fail-open status: set if the queue was ever poisoned. The future
    /// panel surfaces it; capture never panics over it.
    unavailable: AtomicBool,
}

impl MonitorShared {
    /// Lock the completed side, recovering a poisoned mutex and recording the
    /// local unavailable state. Returns `None` only when the lock is contended.
    fn try_completed(&self) -> Option<MutexGuard<'_, Completed>> {
        match self.completed.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(poisoned)) => {
                self.unavailable.store(true, Ordering::Relaxed);
                Some(poisoned.into_inner())
            }
            Err(TryLockError::WouldBlock) => None,
        }
    }

    /// The total pending payloads: in-flight reservations, handoff staging and
    /// completed lines. Read while the completed lock is held, so a staging
    /// charge added in that same critical section is always seen.
    fn pending_total(&self, completed: &Completed) -> usize {
        self.reserved.load(Ordering::Acquire)
            + self.staged.load(Ordering::Acquire)
            + completed.lines.len()
    }

    /// Claim a pending slot **before** any user value is formatted. On success
    /// the returned guard owns the slot; the caller formats outside the lock.
    pub(super) fn reserve(self: &Arc<Self>) -> Option<Reservation> {
        let prior = self.reserved.fetch_add(1, Ordering::AcqRel);
        if prior >= PENDING_CAPACITY {
            self.reserved.fetch_sub(1, Ordering::AcqRel);
            self.counters.add_lost(1);
            return None;
        }

        let mut completed = match self.try_completed() {
            Some(completed) => completed,
            None => {
                // Contended admission rejects without waiting or retrying.
                self.reserved.fetch_sub(1, Ordering::AcqRel);
                self.counters.add_lost(1);
                return None;
            }
        };

        // Free the oldest completed lines until this reservation fits. Staged
        // handoff payloads are never evicted; all-reserved (or all-staged) means
        // there is nothing evictable and the incoming event is rejected.
        while self.pending_total(&completed) > PENDING_CAPACITY {
            let Some(oldest) = completed.lines.keys().next().copied() else {
                break;
            };
            completed.lines.remove(&oldest);
            self.counters.add_lost(1);
        }

        if self.pending_total(&completed) > PENDING_CAPACITY {
            drop(completed);
            self.reserved.fetch_sub(1, Ordering::AcqRel);
            self.counters.add_lost(1);
            return None;
        }
        drop(completed);

        Some(Reservation {
            shared: Arc::clone(self),
            released: false,
        })
    }

    /// Insert a formatted line as a successful admission. Identity and elapsed
    /// time are assigned here, under the completed lock, so retained order is
    /// insertion order: no cross-producer causal or callback-start order is
    /// invented, while serial per-producer order is preserved. The truncation
    /// counter follows a *successful* insertion, never a reservation. The
    /// pending slot is released in the same critical section and `reservation`
    /// records it, so a concurrent producer never sees the new line and its
    /// spent reservation counted together and `Drop` still releases exactly
    /// once if admission unwinds.
    fn admit(&self, reservation: &mut Reservation, mut line: MonitorLine) {
        let mut inserted = false;
        let mut inserted_truncated = false;
        match self.try_completed() {
            Some(mut completed) => {
                // Sample the clock inside the critical section so displayed
                // elapsed time never goes backwards against insertion order.
                let elapsed_ms = self.clock.now_millis().saturating_sub(self.install_ms);
                line.stamp_admission(elapsed_ms);
                inserted_truncated = line.truncated;
                let sequence = completed.next_sequence;
                completed.next_sequence = completed.next_sequence.wrapping_add(1);
                line.sequence = sequence;
                completed.lines.insert(sequence, line);
                self.reserved.fetch_sub(1, Ordering::AcqRel);
                reservation.released = true;
                drop(completed);
                inserted = true;
            }
            None => {
                self.reserved.fetch_sub(1, Ordering::AcqRel);
                reservation.released = true;
            }
        }
        if inserted {
            if inserted_truncated {
                self.counters.add_truncated(1);
            }
        } else {
            self.counters.add_lost(1);
        }
    }

    /// Remove up to `max` completed lines for handoff, charging them to the
    /// staged budget in the same critical section that removes them so a
    /// producer can never see a freed slot before history owns the payload. The
    /// returned batch releases one charge per line as retained history takes
    /// ownership; a dropped batch counts its leftovers lost.
    fn take_staged(self: &Arc<Self>, max: usize) -> StagedBatch {
        let mut lines = VecDeque::new();
        let mut guard = self.try_completed();
        if let Some(completed) = guard.as_deref_mut() {
            while lines.len() < max {
                let Some(&sequence) = completed.lines.keys().next() else {
                    break;
                };
                if let Some(line) = completed.lines.remove(&sequence) {
                    lines.push_back(line);
                }
            }
            if !lines.is_empty() {
                self.staged.fetch_add(lines.len(), Ordering::AcqRel);
            }
        }
        let charged = lines.len();
        StagedBatch {
            shared: Arc::clone(self),
            lines,
            charged,
        }
    }

    /// Pop up to `max` completed lines without charging staging. Test-only: the
    /// deterministic fixtures need direct queue inspection.
    #[cfg(test)]
    fn take_completed(&self, max: usize) -> Vec<MonitorLine> {
        let mut out = Vec::new();
        let Some(mut completed) = self.try_completed() else {
            return out;
        };
        while out.len() < max {
            let Some(&sequence) = completed.lines.keys().next() else {
                break;
            };
            if let Some(line) = completed.lines.remove(&sequence) {
                out.push(line);
            }
        }
        out
    }
}

/// A batch of completed lines removed from the queue for handoff but still
/// charged against [`PENDING_CAPACITY`] until retained history owns each
/// payload. `ingest_capture` calls [`Self::release_one`] only after
/// [`MonitorHistory::push`] takes ownership; dropping the batch early counts
/// every unreleased payload lost and returns its capacity.
pub(super) struct StagedBatch {
    shared: Arc<MonitorShared>,
    lines: VecDeque<MonitorLine>,
    /// How many of `lines` still hold a staged charge.
    charged: usize,
}

impl StagedBatch {
    /// The next oldest line, if any. The caller pushes it into retained history
    /// and then calls [`Self::release_one`].
    pub(super) fn pop_front(&mut self) -> Option<MonitorLine> {
        self.lines.pop_front()
    }

    /// Release one staged charge after history owns the corresponding payload.
    pub(super) fn release_one(&mut self) {
        if self.charged > 0 {
            self.charged -= 1;
            self.shared.staged.fetch_sub(1, Ordering::AcqRel);
        }
    }

    /// How many lines the batch still holds.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.lines.len()
    }
}

impl Drop for StagedBatch {
    fn drop(&mut self) {
        if self.charged > 0 {
            self.shared.staged.fetch_sub(self.charged, Ordering::AcqRel);
            // The leftovers were discarded rather than handed to history, so
            // each is one lost admission, counted once.
            self.shared.counters.add_lost(self.charged as u64);
            self.charged = 0;
        }
    }
}

/// A claimed pending slot. It releases the reservation on drop and counts the
/// admission lost if the line never transferred — formatting error, abort or
/// panic unwinding included.
pub(super) struct Reservation {
    shared: Arc<MonitorShared>,
    /// Set once the pending slot has been released. `Drop` releases only when it
    /// is still unset, so capacity is returned exactly once on success,
    /// contention, failure or unwinding.
    released: bool,
}

impl Reservation {
    /// Insert the formatted line as an admission, releasing the slot in the same
    /// critical section as the insertion.
    pub(super) fn admit(mut self, line: MonitorLine) {
        let shared = Arc::clone(&self.shared);
        shared.admit(&mut self, line);
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if !self.released {
            self.shared.reserved.fetch_sub(1, Ordering::AcqRel);
            self.shared.counters.add_lost(1);
        }
    }
}

// ---------------------------------------------------------------------------
// The public capture handle
// ---------------------------------------------------------------------------

/// The installed capture. Inserted as a Bevy resource only when the effective
/// dev gate is on; its presence is what makes [`ingest_capture`] do work.
#[derive(Resource, Clone)]
pub struct MonitorCapture {
    pub(super) shared: Arc<MonitorShared>,
}

impl MonitorCapture {
    /// Install a capture whose admission clock is `clock`. Off the gate, no
    /// capture is constructed at all.
    pub fn install(clock: Arc<dyn MonitorClock>) -> Self {
        let install_ms = clock.now_millis();
        Self {
            shared: Arc::new(MonitorShared {
                clock,
                install_ms,
                reserved: AtomicUsize::new(0),
                staged: AtomicUsize::new(0),
                completed: Mutex::new(Completed::default()),
                counters: CounterSet::default(),
                unavailable: AtomicBool::new(false),
            }),
        }
    }

    /// The three saturating counters.
    pub fn counters(&self) -> MonitorCounters {
        self.shared.counters.snapshot()
    }

    /// Whether the queue has ever been poisoned; the drawer's local
    /// capture-unavailable status.
    pub fn unavailable(&self) -> bool {
        self.shared.unavailable.load(Ordering::Relaxed)
    }

    /// Remove up to `max` completed lines for handoff, charged until retained
    /// history owns each payload.
    pub(super) fn take_staged(&self, max: usize) -> StagedBatch {
        self.shared.take_staged(max)
    }

    /// Count one retained-history line aged out of the cap.
    pub(super) fn record_evicted(&self, amount: u64) {
        self.shared.counters.add_evicted(amount);
    }

    #[cfg(test)]
    pub(super) fn shared_arc(&self) -> Arc<MonitorShared> {
        Arc::clone(&self.shared)
    }

    #[cfg(test)]
    pub(super) fn pending_completed(&self, max: usize) -> Vec<MonitorLine> {
        self.shared.take_completed(max)
    }

    #[cfg(test)]
    pub(super) fn pending_len(&self) -> usize {
        self.shared
            .completed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .lines
            .len()
    }

    #[cfg(test)]
    pub(super) fn staged_len(&self) -> usize {
        self.shared.staged.load(Ordering::Acquire)
    }

    /// Poison the completed queue from a panic, so the sticky local-unavailable
    /// state has a deterministic trigger without a real lock failure.
    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        let shared = Arc::clone(&self.shared);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = shared.completed.lock().unwrap();
            panic!("poison for test");
        }));
        let _ = self.shared.take_completed(0);
    }
}

/// Build the tracing layer that admits events into `capture`'s bounded pending
/// queue. Composed through `LogPlugin::custom_layer` alongside the session-file
/// layer and the kira observer; it never installs another global logger.
pub fn capture_layer(capture: &MonitorCapture) -> BoxedLayer {
    Box::new(CaptureLayer {
        shared: Arc::clone(&capture.shared),
    })
}
