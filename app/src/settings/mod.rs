//! The pack-adjacent settings file: `<pack-dir>.settings.ron`.
//!
//! Player preferences — the audio mix and the player-comfort `play` group —
//! live in a sibling of the pack directory, exactly like the save slots. The
//! file is **sectioned** so each slice owns a struct, **unversioned** so a newer
//! build's file still parses, and **permissive on read, lossy on rewrite**: every
//! field carries `#[serde(default)]`, unknown keys are ignored, and the next
//! write drops what this build does not know ([ADR 0007]).
//!
//! Failure posture is deliberately unlike a save slot's ([ADR 0006]): a save is
//! unrecoverable player data, a preference is a cheap value the player restores
//! by dragging a slider. A missing file is defaults in memory and nothing
//! written; a malformed one is *all* defaults plus a warning (row 19) with the
//! bytes left untouched; out-of-range amplitudes clamp into `0.0..=1.0`, an
//! out-of-range `text_speed` clamps into `0.0..=0.08`, and non-finite ones take
//! the field default. The startup read is done by `main` before the app exists —
//! the loaded values ride in on [`StartupSettings`] and `play.fullscreen`
//! decides the window's created mode — so a `SettingsState` always has a value.
//!
//! This module owns the types, the path, load/save, the [`SettingsState`]
//! resource the menu mutates, the `play.fullscreen` → window-mode mapping, and
//! the one atomic write per flush. Nothing here crosses into `engine/`: every
//! value is presentation state the headless runner never reads.
//!
//! Each responsibility owns a module: [`file`] the on-disk contract, [`controls`]
//! the value mapping and slider arithmetic, [`state`] the live resource and its
//! flush points, and [`window`] the created window's size and mode. This root
//! re-exports them, so every `settings::<name>` path callers already use stays
//! the same.
//!
//! [ADR 0006]: ../../docs/adr/0006-save-slots-on-disk-format-and-version-policy.md
//! [ADR 0007]: ../../docs/adr/0007-settings-file-beside-the-pack.md

mod controls;
mod file;
mod state;
mod window;

#[cfg(test)]
mod tests;

pub use controls::{
    amplitude_to_decibels, effective_amplitude, effective_text_speed, AudioControl, TrackControl,
    MAX_TEXT_SPEED, SILENCE_DECIBELS, TEXT_SPEED_LADDER,
};
pub use file::{
    load, save, settings_path_for, AudioSettings, LoadedSettings, PlaySettings, Settings,
    SettingsError, StartupSettings,
};
pub use state::{flush, SettingsState};
pub use window::{window_config, window_mode};

pub(crate) use controls::pointer_fraction;
#[cfg(test)]
pub(crate) use controls::track_fraction;
pub(crate) use state::{flush_on_exit, setup_settings};
