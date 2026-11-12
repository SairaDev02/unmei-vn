//! The settings value mapping: the slider/step vocabulary, the load-time clamps,
//! and the amplitude/speed arithmetic the menu and presentation read.

use super::state::SettingsState;

/// The audio controls the Settings view exposes, in menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioControl {
    Master,
    Music,
    Sfx,
}

impl AudioControl {
    /// Every slider, loudest/most general first.
    pub const ALL: [AudioControl; 3] =
        [AudioControl::Master, AudioControl::Music, AudioControl::Sfx];
}

/// The control one Settings track belongs to: an audio slider, or the stepped
/// text-speed control. The drag/repaint machinery is shared, so this is the one
/// place the two track kinds differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackControl {
    Audio(AudioControl),
    TextSpeed,
}

impl TrackControl {
    /// The control's fill position as a 0–100 percentage.
    pub fn percent(self, settings: &SettingsState) -> u32 {
        match self {
            TrackControl::Audio(control) => settings.percent(control),
            TrackControl::TextSpeed => settings.text_speed_percent(),
        }
    }

    /// Snap a `0.0..=1.0` pointer fraction to the control's own steps and store
    /// it — the sliders' 5% grid, or the text-speed ladder's named steps.
    pub fn set_fraction(self, settings: &mut SettingsState, fraction: f32) {
        match self {
            TrackControl::Audio(control) => settings.set_fraction(control, fraction),
            TrackControl::TextSpeed => settings.set_text_speed_fraction(fraction),
        }
    }

    /// Move the control one step left (`-1`) or right (`1`).
    pub fn step(self, settings: &mut SettingsState, steps: i32) {
        match self {
            TrackControl::Audio(control) => settings.step(control, steps),
            TrackControl::TextSpeed => settings.step_text_speed(steps),
        }
    }
}

/// The decoded decibel value kira treats as silence; `log10(0)` is `-inf`, so
/// the mapping floors there instead.
pub const SILENCE_DECIBELS: f32 = -60.0;

/// The named reveal ladder, `(step name, seconds per character)`, Instant first.
/// The Settings control walks these five steps; the file stores the physical
/// pace, exactly as the audio group stores amplitudes.
pub const TEXT_SPEED_LADDER: [(&str, f32); 5] = [
    ("Instant", 0.0),
    ("Fast", 0.012),
    ("Normal", 0.02),
    ("Slow", 0.04),
    ("Slowest", 0.08),
];

/// The slowest reveal the menu offers, in seconds per character (`0.0` is
/// Instant). The stored `text_speed` clamps to `0.0..=MAX_TEXT_SPEED`.
pub const MAX_TEXT_SPEED: f32 = TEXT_SPEED_LADDER[TEXT_SPEED_LADDER.len() - 1].1;

/// Clamp a stored amplitude into `0.0..=1.0`; a non-finite value (RON's `nan` /
/// `inf`) takes the field's default instead.
pub(super) fn normalise_amplitude(value: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        default
    }
}

/// Clamp a stored reveal pace into `0.0..=MAX_TEXT_SPEED`; a non-finite value
/// takes the field default (`0.0`, Instant).
pub(super) fn normalise_text_speed(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, MAX_TEXT_SPEED)
    } else {
        super::PlaySettings::default().text_speed
    }
}

/// The effective channel amplitude: master scales each channel, and mute forces
/// silence whatever the stored levels hold.
pub fn effective_amplitude(master: f32, channel: f32, muted: bool) -> f32 {
    if muted {
        0.0
    } else {
        master * channel
    }
}

/// The reveal pace in effect: the stored `play.text_speed`, or Instant when no
/// [`SettingsState`] exists (a headless host that never installed one).
pub fn effective_text_speed(settings: Option<&SettingsState>) -> f32 {
    settings.map(SettingsState::text_speed).unwrap_or(0.0)
}

/// Linear amplitude → decibels, with `0.0` and every non-finite value mapped to
/// [`SILENCE_DECIBELS`] rather than `-inf`.
pub fn amplitude_to_decibels(amplitude: f32) -> f32 {
    if !amplitude.is_finite() || amplitude <= 0.0 {
        SILENCE_DECIBELS
    } else {
        20.0 * amplitude.log10()
    }
}

/// Snap a `0.0..=1.0` fraction to the nearest 5% step, rendered as a percentage.
pub(super) fn snap_percent(fraction: f32) -> u32 {
    if !fraction.is_finite() {
        return 0;
    }
    ((fraction.clamp(0.0, 1.0) * 20.0).round() * 5.0) as u32
}

/// The ladder index whose stored pace is closest to `value`; a tie keeps the
/// earlier (faster) step, so `0.03` reads as `Normal` rather than `Slow`.
pub(super) fn nearest_text_speed_step(value: f32) -> usize {
    let mut best = 0;
    let mut best_distance = f32::INFINITY;
    for (index, (_, step)) in TEXT_SPEED_LADDER.iter().enumerate() {
        let distance = (value - step).abs();
        if distance < best_distance {
            best_distance = distance;
            best = index;
        }
    }
    best
}

/// Snap a `0.0..=1.0` fraction across the ladder to the nearest step index
/// (`0` = Instant, `1` = Slowest) — the click/drag path, matching the slider's
/// grid snap in spirit.
pub(super) fn snap_text_speed_step(fraction: f32) -> usize {
    let last = TEXT_SPEED_LADDER.len() - 1;
    if !fraction.is_finite() {
        return 0;
    }
    ((fraction.clamp(0.0, 1.0) * last as f32).round() as usize).min(last)
}

/// A ladder index as the track fill's 0–100 percentage.
pub(super) fn text_speed_percent(step: usize) -> u32 {
    let last = TEXT_SPEED_LADDER.len() - 1;
    (step * 100 / last) as u32
}

/// A pointer's x on a slider track, normalized so the track's centre is `0.0`
/// and its ends are `±0.5`, to a `0.0..=1.0` fraction. Outside that span the
/// value is clamped, so an ongoing drag that leaves the track still reaches the
/// end step.
pub(crate) fn track_fraction(normalized_x: f32) -> f32 {
    (normalized_x + 0.5).clamp(0.0, 1.0)
}

/// [`track_fraction`] for a press or drag, rejecting a pointer that is outside
/// the track's horizontal span: a press on the row's label or value text has no
/// track under it, so focusing the row never snaps the control. `None` means
/// "do not change the value this frame".
pub(crate) fn pointer_fraction(normalized_x: f32) -> Option<f32> {
    (normalized_x.abs() <= 0.5).then(|| track_fraction(normalized_x))
}
