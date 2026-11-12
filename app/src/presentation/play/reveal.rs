//! The typewriter reveal for the parked say-line, and the two halves of the one
//! dialogue text node it renders.

use bevy::prelude::*;
use unmei_engine::runner::Wait;

/// The typewriter reveal for the parked say-line: which line it belongs to and
/// how far it has drawn. A resource so the core's advance path can complete a
/// line in place; the presentation is its only writer and only reader of the
/// rendered slice. Nothing in `engine/` sees it.
#[derive(Resource, Default)]
pub(crate) struct TextReveal {
    line: Option<RevealLine>,
}

struct RevealLine {
    speaker: String,
    text: String,
    /// Revealed characters, fractional; advancing adds `delta / speed`, so a
    /// later speed change alters the rate but never rolls the line back. Frozen
    /// while the overlay is open, because [`advance_reveal`] returns before
    /// aging it.
    progress: f32,
    /// Set by the one press that completes a revealing line, so the shown count
    /// is never at the mercy of float rounding at the final character.
    completed: bool,
}

impl TextReveal {
    /// Arm (or drop) the reveal for a newly parked wait. Only a say-line with a
    /// non-zero pace reveals; every other park — a choice, the decision
    /// placeholder, the end-of-scene marker — is whole immediately, exactly as
    /// before.
    pub(crate) fn arm(&mut self, wait: &Option<Wait>, speed: f32) {
        self.line = match wait {
            Some(Wait::Say { speaker, text }) if speed > 0.0 => Some(RevealLine {
                speaker: speaker.clone(),
                text: text.clone(),
                progress: 0.0,
                completed: false,
            }),
            _ => None,
        };
    }

    /// Whether `wait` is the say-line this reveal belongs to. A stale reveal for
    /// a different line never completes or draws against the current park.
    pub(crate) fn is_current(&self, wait: &Wait) -> bool {
        match (&self.line, wait) {
            (Some(line), Wait::Say { speaker, text }) => {
                &line.speaker == speaker && &line.text == text
            }
            _ => false,
        }
    }

    /// How many `chars()` of the current line are drawn at `speed`. `speed <= 0`
    /// is Instant, and a `None`/stale reveal is treated as whole by the caller.
    pub(crate) fn chars_shown(&self, speed: f32) -> usize {
        let Some(line) = &self.line else {
            return 0;
        };
        let total = line.text.chars().count();
        if line.completed || speed <= 0.0 {
            return total;
        }
        let shown = line.progress.floor();
        if !shown.is_finite() || shown <= 0.0 {
            return 0;
        }
        (shown as usize).min(total)
    }

    /// Drop the armed line entirely (New Game's reset). The next park re-arms
    /// through [`TextReveal::arm`].
    pub(crate) fn reset(&mut self) {
        self.line = None;
    }

    /// Whether every char of the current line is drawn.
    pub(crate) fn is_complete(&self, speed: f32) -> bool {
        match &self.line {
            Some(line) => self.chars_shown(speed) >= line.text.chars().count(),
            None => true,
        }
    }

    /// Whether the reveal for `wait` is finished at `speed`: Instant is always
    /// finished, a reveal armed for a different line is not (the new line has
    /// not been armed yet), and a current line is finished when every char is
    /// drawn. This is the auto dwell's gate.
    pub(crate) fn is_whole_for(&self, wait: &Wait, speed: f32) -> bool {
        if speed <= 0.0 {
            return true;
        }
        self.is_current(wait) && self.is_complete(speed)
    }

    /// Age the reveal by `delta` unpaused seconds; a no-op unless `wait` is the
    /// armed line.
    pub(crate) fn advance(&mut self, delta: f32, wait: &Wait, speed: f32) {
        if !self.is_current(wait) || speed <= 0.0 {
            return;
        }
        let Some(line) = &mut self.line else {
            return;
        };
        if line.completed {
            return;
        }
        line.progress += delta.max(0.0) / speed;
    }

    /// Complete the armed line: the one press that consumes a revealing line.
    pub(crate) fn complete(&mut self, speed: f32) {
        if let Some(line) = &mut self.line {
            if speed > 0.0 {
                line.completed = true;
            }
        }
    }
}

/// The dialogue band's two halves for a wait: the mono root text and the sans
/// prose span. A say-line's root is the complete `Speaker: ` prefix and its
/// span is the revealed `chars()` of the prose; every non-say wait renders
/// whole in the root with an empty span
/// (`docs/title-and-menu.md` § Type system §4).
pub(crate) fn rendered_line_parts(
    wait: &Option<Wait>,
    reveal: &TextReveal,
    speed: f32,
) -> (String, String) {
    let Some(wait) = wait else {
        return ("— end of scene —".to_string(), String::new());
    };
    match wait {
        Wait::Say { speaker, text } => {
            let shown = if reveal.is_current(wait) {
                reveal.chars_shown(speed)
            } else {
                text.chars().count()
            };
            let revealed: String = text.chars().take(shown).collect();
            (format!("{speaker}: "), revealed)
        }
        Wait::Choice { .. } => (String::new(), String::new()),
        Wait::Decision { npc_id, .. } => (format!("{npc_id} is deciding…"), String::new()),
    }
}

/// The one line the dialogue band shows for a wait, as one string. The pure
/// reveal tests read this; the app itself writes the two halves separately
/// through [`rendered_line_parts`].
#[cfg(test)]
pub(crate) fn rendered_line(wait: &Option<Wait>, reveal: &TextReveal, speed: f32) -> String {
    let (root, prose) = rendered_line_parts(wait, reveal, speed);
    format!("{root}{prose}")
}
