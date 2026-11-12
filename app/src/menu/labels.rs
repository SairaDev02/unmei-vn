//! The overlay's copy: the walk orders, the headings, the legends and the
//! door-aware labels the owner pins.

use crate::settings::{AudioControl, TrackControl};

use super::state::{MenuFocus, MenuOrigin};

/// The five title rows, top to bottom (`docs/title-and-menu.md` § Title §2).
/// Disabled rows stay in the walk, so the list is every row, not the enabled
/// ones.
pub const TITLE_ORDER: [MenuFocus; 5] = [
    MenuFocus::TitleContinue,
    MenuFocus::TitleNewGame,
    MenuFocus::TitleSettings,
    MenuFocus::TitleGallery,
    MenuFocus::TitleQuit,
];

/// The title's fixed foot-row legend (`docs/title-and-menu.md` § Title §6).
pub const TITLE_LEGEND: &str =
    "\u{2191}/\u{2193} move \u{00b7} Enter select \u{00b7} click to choose";

/// The gallery's heading (`docs/title-and-menu.md` § Gallery `### Behavior`
/// §7).
pub const GALLERY_HEADING: &str = "Gallery";

/// The line an empty pack draws in place of the grid
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
pub const GALLERY_EMPTY: &str = "No images in this pack";

/// The gallery grid's legend, right-aligned in the header
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §7).
pub const GALLERY_GRID_LEGEND: &str =
    "\u{2191}/\u{2193}/\u{2190}/\u{2192} move \u{00b7} Enter view \u{00b7} click to open";

/// The still view's legend, under the caption in the faint tone
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §7).
pub const GALLERY_STILL_LEGEND: &str =
    "\u{2190}/\u{2192} browse \u{00b7} \u{2191}/\u{2193} or Esc or click to close";

/// The gallery's door-aware back label (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §3/§7).
pub fn gallery_back_label(origin: MenuOrigin) -> &'static str {
    match origin {
        MenuOrigin::Slots => "\u{2190} Back to slots",
        MenuOrigin::Title => "\u{2190} Back to title",
    }
}

/// The Timeline's heading (`docs/title-and-menu.md` § Timeline §3).
pub const TIMELINE_HEADING: &str = "Timeline";
/// The Timeline's door-aware back label. Timeline's only door is the overlay's
/// slot grid, so the label is fixed (`docs/title-and-menu.md` § Timeline §1).
pub const TIMELINE_BACK: &str = "\u{2190} Back to slots";
/// The Timeline's explanatory line, under the heading in every non-card state.
pub const TIMELINE_EXPLANATION: &str =
    "Choose a visited scene to start it from the beginning with your current state.";
/// The Timeline's empty state (`docs/title-and-menu.md` § Timeline §3).
pub const TIMELINE_EMPTY: &str = "This pack has no Timeline.";
/// The label an unvisited card exposes in place of the scene id.
pub const TIMELINE_LOCKED: &str = "???";
/// The Timeline's walk legend, right-aligned in the header (`docs/title-and-menu.md`
/// § Timeline §3).
pub const TIMELINE_LEGEND: &str =
    "\u{2191}/\u{2193}/\u{2190}/\u{2192} move \u{00b7} Enter/Space select \u{00b7} Esc back \u{00b7} click to choose";
/// The Timeline confirm's detail sentence, verbatim (`docs/title-and-menu.md`
/// § Timeline §3).
pub const TIMELINE_CONFIRM_DETAIL: &str = "Unsaved progress is lost and any decision in flight is cancelled. The scene restarts from the beginning. Current game state is retained.";

/// The Settings view's controls, top to bottom. The keyboard walks this order
/// with `↑`/`↓`; the card grid's `←`/`→` focus rule deliberately does not apply
/// here. The door-aware back control is a click/`Esc` affordance and is **not**
/// an arrow stop, so the Decisions group's `Clear key` is the walk's last stop
/// (`docs/title-and-menu.md` § Title §7, § Decisions settings §4).
pub const SETTINGS_ORDER: [MenuFocus; 12] = [
    MenuFocus::AudioMaster,
    MenuFocus::AudioMusic,
    MenuFocus::AudioSfx,
    MenuFocus::AudioMute,
    MenuFocus::AudioReset,
    MenuFocus::TextSpeed,
    MenuFocus::PlayAuto,
    MenuFocus::PlayFullscreen,
    MenuFocus::PlayReset,
    MenuFocus::KeyRow,
    MenuFocus::CopyKeyPath,
    MenuFocus::ClearKey,
];

/// The track a focus points at — an audio slider or the stepped Text-speed
/// control — or `None` for a row that is not a track.
pub fn focus_track(focus: MenuFocus) -> Option<TrackControl> {
    match focus {
        MenuFocus::AudioMaster => Some(TrackControl::Audio(AudioControl::Master)),
        MenuFocus::AudioMusic => Some(TrackControl::Audio(AudioControl::Music)),
        MenuFocus::AudioSfx => Some(TrackControl::Audio(AudioControl::Sfx)),
        MenuFocus::TextSpeed => Some(TrackControl::TextSpeed),
        _ => None,
    }
}

/// The audio control a slider focus points at, or `None` for other controls.
pub fn focus_control(focus: MenuFocus) -> Option<AudioControl> {
    match focus_track(focus) {
        Some(TrackControl::Audio(control)) => Some(control),
        _ => None,
    }
}

/// The focus that points at one slider.
pub fn control_focus(control: AudioControl) -> MenuFocus {
    match control {
        AudioControl::Master => MenuFocus::AudioMaster,
        AudioControl::Music => MenuFocus::AudioMusic,
        AudioControl::Sfx => MenuFocus::AudioSfx,
    }
}
