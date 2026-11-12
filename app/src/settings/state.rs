//! The live settings resource and its two flush points.

use std::path::PathBuf;

use bevy::prelude::*;

use super::controls::{
    nearest_text_speed_step, snap_percent, snap_text_speed_step, text_speed_percent, AudioControl,
    TEXT_SPEED_LADDER,
};
use super::file::{load, save, AudioSettings, PlaySettings, Settings, StartupSettings};
use crate::failure::{self, DefensiveHold, Failure, FailureRow, Notice};
use crate::RuntimeConfig;

/// The in-memory settings and whether they differ from what is on disk.
///
/// `dirty` is the persistence guard: it is set by every control change and
/// cleared only by a successful flush, so a failed write keeps the in-session
/// values and the next flush point retries.
#[derive(Resource, Debug, Clone)]
pub struct SettingsState {
    pub settings: Settings,
    pub dirty: bool,
    pub pack_root: PathBuf,
}

impl SettingsState {
    /// A fresh state over `settings`, with nothing pending.
    pub fn new(pack_root: PathBuf, settings: Settings) -> Self {
        SettingsState {
            settings,
            dirty: false,
            pack_root,
        }
    }

    /// The stored amplitude of one control.
    pub fn amplitude(&self, control: AudioControl) -> f32 {
        self.settings.audio.amplitude(control)
    }

    /// The control's stored amplitude as a 0–100 percentage.
    pub fn percent(&self, control: AudioControl) -> u32 {
        (self.amplitude(control) * 100.0).round().clamp(0.0, 100.0) as u32
    }

    /// Whether the mix is muted.
    pub fn muted(&self) -> bool {
        self.settings.audio.muted
    }

    /// Whether auto mode is on.
    pub fn auto(&self) -> bool {
        self.settings.play.auto
    }

    /// Whether the window should be borderless-primary fullscreen.
    pub fn fullscreen(&self) -> bool {
        self.settings.play.fullscreen
    }

    /// The stored reveal pace, in seconds per character.
    pub fn text_speed(&self) -> f32 {
        self.settings.play.text_speed
    }

    /// The ladder index the stored pace is nearest to.
    pub fn text_speed_step(&self) -> usize {
        nearest_text_speed_step(self.settings.play.text_speed)
    }

    /// The ladder name the focused Text-speed row shows.
    pub fn text_speed_name(&self) -> &'static str {
        TEXT_SPEED_LADDER[self.text_speed_step()].0
    }

    /// The current ladder position as the track fill's 0–100 percentage.
    pub fn text_speed_percent(&self) -> u32 {
        text_speed_percent(self.text_speed_step())
    }

    /// Move the reveal pace `steps` ladder positions, clamped to the ends.
    pub fn step_text_speed(&mut self, steps: i32) {
        let last = TEXT_SPEED_LADDER.len() as i32 - 1;
        let index = (self.text_speed_step() as i32 + steps).clamp(0, last) as usize;
        self.set_text_speed(TEXT_SPEED_LADDER[index].1);
    }

    /// Set the reveal pace from a `0.0..=1.0` fraction across the ladder,
    /// snapped to the nearest step (the click/drag path).
    pub fn set_text_speed_fraction(&mut self, fraction: f32) {
        let index = snap_text_speed_step(fraction);
        self.set_text_speed(TEXT_SPEED_LADDER[index].1);
    }

    fn set_text_speed(&mut self, value: f32) {
        self.settings.play.text_speed = value;
        self.dirty = true;
    }

    /// Set a slider from a `0.0..=1.0` fraction, snapped to the 5% grid.
    pub fn set_fraction(&mut self, control: AudioControl, fraction: f32) {
        let value = snap_percent(fraction) as f32 / 100.0;
        self.set_amplitude_value(control, value);
    }

    /// Move a slider one `steps`-many 5% steps, clamped to `0..=100`.
    pub fn step(&mut self, control: AudioControl, steps: i32) {
        let percent = (self.percent(control) as i32 + steps * 5).clamp(0, 100);
        self.set_amplitude_value(control, percent as f32 / 100.0);
    }

    fn set_amplitude_value(&mut self, control: AudioControl, value: f32) {
        self.settings.audio.set_amplitude(control, value);
        self.dirty = true;
    }

    /// Flip mute. The stored levels are untouched, so unmuting restores them.
    pub fn toggle_mute(&mut self) {
        self.settings.audio.muted = !self.settings.audio.muted;
        self.dirty = true;
    }

    /// Flip auto mode. The Settings switch and the in-play `A` hotkey share
    /// this one toggle so the two can never drift; it sets `dirty` so the
    /// change rides the flush points rather than writing on key press.
    pub fn toggle_auto(&mut self) {
        self.settings.play.auto = !self.settings.play.auto;
        self.dirty = true;
    }

    /// Flip fullscreen. The Settings switch and the `F11` hotkey share this one
    /// toggle; the window itself is synced from this field by the presentation.
    pub fn toggle_fullscreen(&mut self) {
        self.settings.play.fullscreen = !self.settings.play.fullscreen;
        self.dirty = true;
    }

    /// Reset the `audio` section only: `master`/`music`/`sfx` → `1.0`, `muted`
    /// → `false`. `play` is untouched, and the file is never deleted.
    pub fn reset_audio(&mut self) {
        self.settings.audio = AudioSettings::default();
        self.dirty = true;
    }

    /// Reset the `play` section only: `text_speed` → `0.0`, `auto` → `false`,
    /// `fullscreen` → `false`. `audio` is untouched, and the file is never
    /// deleted.
    pub fn reset_play(&mut self) {
        self.settings.play = PlaySettings::default();
        self.dirty = true;
    }
}

/// The single flush: one atomic write when the state is dirty. A success clears
/// `dirty`; a failure keeps the in-session values, leaves `dirty` set for the
/// next flush to retry, and notices `settings not saved` (row 20).
pub fn flush(state: &mut SettingsState, notice: &mut Notice, hold: &mut DefensiveHold) {
    if !state.dirty {
        return;
    }
    match save(&state.pack_root, &state.settings) {
        Ok(()) => state.dirty = false,
        Err(error) => failure::report(
            Failure::new(FailureRow::SettingsFlush, error.to_string()),
            notice,
            hold,
        ),
    }
}

/// Install the live [`SettingsState`]: from the preloaded [`StartupSettings`]
/// when the entry point read the file before the app existed, otherwise by
/// reading it now (the headless/embedded case). A malformed file reports row 19.
/// A missing file is silently the defaults.
pub(crate) fn setup_settings(
    mut commands: Commands,
    config: Res<RuntimeConfig>,
    preloaded: Option<Res<StartupSettings>>,
    mut notice: ResMut<Notice>,
    mut hold: ResMut<DefensiveHold>,
) {
    let loaded = match preloaded {
        Some(preloaded) => preloaded.0.clone(),
        None => load(&config.pack_root),
    };
    if let Some(detail) = loaded.warning {
        failure::report(
            Failure::new(FailureRow::SettingsMalformed, detail),
            &mut notice,
            &mut hold,
        );
    }
    commands.insert_resource(SettingsState::new(
        config.pack_root.clone(),
        loaded.settings,
    ));
}

/// The app-exit flush: a change not left behind by a menu close still lands.
/// Runs in `Last` so it reads an `AppExit` written earlier in the frame.
pub(crate) fn flush_on_exit(
    mut exits: MessageReader<AppExit>,
    state: Option<ResMut<SettingsState>>,
    mut notice: ResMut<Notice>,
    mut hold: ResMut<DefensiveHold>,
) {
    if exits.read().next().is_none() {
        return;
    }
    let Some(mut state) = state else {
        return;
    };
    flush(&mut state, &mut notice, &mut hold);
}
