//! The one transient line: a single replacing slot with a five-second clock.

use bevy::prelude::*;

/// How long a [`Notice`] lives: roughly five seconds of unpaused time.
pub const NOTICE_SECONDS: f32 = 5.0;

/// The single transient line: at most one `(text, remaining)` slot, top-right,
/// replaced rather than queued, alive for [`NOTICE_SECONDS`] of unpaused time.
/// It never advances the runner, never cancels a request, never writes the
/// autosave, and never consumes input.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Notice {
    pub text: Option<String>,
    pub remaining: f32,
}

impl Notice {
    /// Replace the current slot and restart the clock.
    pub fn show(&mut self, text: impl Into<String>) {
        self.text = Some(text.into());
        self.remaining = NOTICE_SECONDS;
    }

    /// Age the slot by one unpaused frame. The caller gates this on
    /// `menu::menu_closed`, so the timer freezes while the overlay is open
    /// (`docs/runtime-hardening.md` § Runtime failure posture §3).
    pub fn tick(&mut self, delta_seconds: f32) {
        if self.text.is_none() {
            return;
        }
        self.remaining -= delta_seconds;
        if self.remaining <= 0.0 {
            self.text = None;
            self.remaining = 0.0;
        }
    }

    pub fn is_showing(&self) -> bool {
        self.text.is_some()
    }

    /// Drop the current line immediately (New Game's reset clears it).
    pub fn clear(&mut self) {
        self.text = None;
        self.remaining = 0.0;
    }
}

/// Age the Notice by the frame's unpaused time.
pub fn tick_notice(time: Res<Time>, mut notice: ResMut<Notice>) {
    notice.tick(time.delta_secs());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_notice_is_one_slot_that_replaces() {
        let mut notice = Notice::default();
        notice.show("first");
        notice.show("second");
        assert_eq!(notice.text.as_deref(), Some("second"));
        assert_eq!(notice.remaining, NOTICE_SECONDS);
    }
    #[test]
    fn the_notice_expires_after_roughly_five_seconds_of_unpaused_time() {
        let mut notice = Notice::default();
        notice.show("failed");
        notice.tick(4.9);
        assert!(notice.is_showing(), "still alive before five seconds");
        notice.tick(0.2);
        assert!(!notice.is_showing(), "expired after five seconds");
        assert_eq!(notice.remaining, 0.0);
    }
    #[test]
    fn a_replaced_notice_restarts_its_clock() {
        let mut notice = Notice::default();
        notice.show("first");
        notice.tick(4.0);
        notice.show("second");
        assert_eq!(notice.remaining, NOTICE_SECONDS);
    }
}
