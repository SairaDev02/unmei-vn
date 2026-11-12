//! The settings file contract: the typed sections, the sibling path, the
//! permissive read and the one atomic write.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::controls::{normalise_amplitude, normalise_text_speed};
use super::AudioControl;

/// The whole file: one section per settings group. The `play` section is
/// declared now (its fields are fixed by the comfort ticket) but skipped while
/// it is all-default, so an audio-only file carries no `play` key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub audio: AudioSettings,
    #[serde(skip_serializing_if = "PlaySettings::is_default")]
    pub play: PlaySettings,
}

impl Settings {
    /// Clamp every stored amplitude and the reveal pace, and replace
    /// non-finite ones with the field default. Applied on load only; the menu
    /// writes already-clamped values.
    fn normalise(&mut self) {
        let default = AudioSettings::default();
        self.audio.master = normalise_amplitude(self.audio.master, default.master);
        self.audio.music = normalise_amplitude(self.audio.music, default.music);
        self.audio.sfx = normalise_amplitude(self.audio.sfx, default.sfx);
        self.play.text_speed = normalise_text_speed(self.play.text_speed);
    }
}

/// The audio mix: linear amplitudes in `0.0..=1.0` and a mute flag. Defaults are
/// today's kira identity, so a first launch sounds exactly as the slice does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            master: 1.0,
            music: 1.0,
            sfx: 1.0,
            muted: false,
        }
    }
}

impl AudioSettings {
    /// The stored amplitude behind one control.
    pub fn amplitude(&self, control: AudioControl) -> f32 {
        match control {
            AudioControl::Master => self.master,
            AudioControl::Music => self.music,
            AudioControl::Sfx => self.sfx,
        }
    }

    pub(super) fn set_amplitude(&mut self, control: AudioControl, value: f32) {
        match control {
            AudioControl::Master => self.master = value,
            AudioControl::Music => self.music = value,
            AudioControl::Sfx => self.sfx = value,
        }
    }
}

/// The player-comfort section: the reveal pace, auto mode and fullscreen, stored
/// as physical values (`text_speed` in seconds per character, `0.0` = Instant).
/// Read at startup; the `play` key is skipped while every field holds its
/// default, so the first non-default comfort value starts emitting it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaySettings {
    pub text_speed: f32,
    pub auto: bool,
    pub fullscreen: bool,
}

impl Default for PlaySettings {
    fn default() -> Self {
        PlaySettings {
            text_speed: 0.0,
            auto: false,
            fullscreen: false,
        }
    }
}

impl PlaySettings {
    /// Whether every field still holds its default, so the whole section can be
    /// skipped on write.
    pub fn is_default(&self) -> bool {
        *self == PlaySettings::default()
    }
}

/// The pack's sibling settings path: `<pack-dir>.settings.ron`, named after the
/// pack exactly like the slot and log files.
pub fn settings_path_for(pack_root: &Path) -> PathBuf {
    unmei_engine::sidecar::sibling_path(pack_root, "settings.ron")
}

/// What a load yields: the settings to use in memory, plus the warning detail
/// (row 19) when the file existed but did not parse. `load` never writes.
#[derive(Debug, Clone)]
pub struct LoadedSettings {
    pub settings: Settings,
    pub warning: Option<String>,
}

/// The settings `main` loaded **before** the app was built, handed to the plugin
/// so the file is read exactly once. The window is constructed from
/// [`LoadedSettings::settings`] first, and [`super::setup_settings`] installs the
/// same values (and emits the carried warning, which happens on the plugin's
/// startup system because it runs after `LogPlugin` installs). A host that did
/// not preload — the headless tests, a future embedder — falls back to reading
/// the file in `setup_settings`.
#[derive(Resource, Debug, Clone)]
pub struct StartupSettings(pub LoadedSettings);

/// Read the settings file. A **missing** file is defaults with no warning; a
/// file that exists but cannot be read or does not parse is *all* defaults with
/// a warning carrying the path and the error. In every case the bytes on disk
/// are left exactly as they were.
pub fn load(pack_root: &Path) -> LoadedSettings {
    let path = settings_path_for(pack_root);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LoadedSettings {
                settings: Settings::default(),
                warning: None,
            }
        }
        Err(error) => {
            return LoadedSettings {
                settings: Settings::default(),
                warning: Some(format!("`{}`: {error}", path.display())),
            }
        }
    };
    match ron::from_str::<Settings>(&text) {
        Ok(mut settings) => {
            settings.normalise();
            LoadedSettings {
                settings,
                warning: None,
            }
        }
        Err(error) => LoadedSettings {
            settings: Settings::default(),
            warning: Some(format!("`{}`: {error}", path.display())),
        },
    }
}

/// One atomic write through the shared sidecar idiom
/// ([`unmei_engine::sidecar::write_atomic`]). The temp name derives from the
/// settings path, so it can never collide with a slot's.
pub fn save(pack_root: &Path, settings: &Settings) -> Result<(), SettingsError> {
    let path = settings_path_for(pack_root);
    let text = ron::ser::to_string_pretty(settings, ron::ser::PrettyConfig::default())
        .map_err(|error| SettingsError(format!("could not encode settings: {error}")))?;
    unmei_engine::sidecar::write_atomic(&path, &text)
        .map_err(|error| SettingsError(format!("`{}`: {error}", path.display())))
}

/// A settings write failure, rendered with the path the IO reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsError(String);

impl std::fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SettingsError {}
