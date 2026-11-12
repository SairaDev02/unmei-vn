//! The play surface: its applied caches, reveal, channels and chrome.
//!
//! Owns the staging draws, audio application, dialogue band and choice
//! buttons, the `AUTO`/`SKIP` cues, the Notice text node and the startup
//! error screen, and exposes the one concrete replay request the session
//! recipes call (ADR 0013).
//!
//! This root composes the surface's own modules — the components and applied
//! caches, the reveal, the chrome, the startup error screen and the sync
//! systems — and re-exports the items callers and the app's tests already use.

mod chrome;
mod error_screen;
mod reveal;
mod state;
mod sync;

#[cfg(test)]
mod tests;

use bevy::prelude::*;

use crate::PresentationSet;
use crate::Runtime;

pub(crate) use chrome::{setup_ui, sync_auto_cue, sync_notice, sync_skip_cue};
pub(crate) use error_screen::draw_error_screen;
pub(crate) use reveal::TextReveal;
pub(crate) use state::{request_replay, AppliedAudio, AppliedUi, AppliedVisuals, AppliedVolume};
pub(crate) use state::{Bgm, ChoiceButton, Sfx};
pub(crate) use sync::{
    advance_reveal, report_silent_degradations, sync_audio, sync_audio_volume,
    sync_staging_visuals, sync_wait_ui,
};

// The cue copy, the dialogue spawn and the node markers are addressed by full
// path from the app's tests, which is the only reader outside this module, so
// these two re-exports are test-only.
#[cfg(test)]
pub(crate) use chrome::{spawn_dialogue, AUTO_CUE_COLOR, SKIP_CUE_COLOR};
#[cfg(test)]
pub(crate) use reveal::rendered_line;
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "the pre-split `play::<Marker>` paths stay valid; `DialogueProse`, `ErrorScreen` and `NoticeText` have no reader outside `play`"
)]
pub(crate) use state::{
    AutoCue, BackgroundSprite, DialogueProse, DialogueText, ErrorScreen, HelpLine, NoticeText,
    NpcSprite, SkipCue,
};

/// Initialize the play surface's separately stored realization state: the
/// applied audio/volume/visual/UI caches and the reveal clock. The headless
/// [`RuntimeCorePlugin`](crate::RuntimeCorePlugin) keeps them through this
/// function, so its input and session tests exercise the real state.
pub(crate) fn init(app: &mut App) {
    app.init_resource::<AppliedAudio>()
        .init_resource::<AppliedVolume>()
        .init_resource::<AppliedVisuals>()
        .init_resource::<AppliedUi>()
        .init_resource::<TextReveal>();
}

/// Register the play surface's systems with their intrinsic edges: the Notice
/// text node and the startup error screen, the Runtime-gated play chain
/// (`staging visuals -> volume -> audio -> settle -> reveal -> dialogue`), and
/// the `AUTO`/`SKIP` cues.
pub(crate) fn register(app: &mut App) {
    app.add_systems(Startup, setup_ui)
        .add_systems(
            Update,
            (sync_notice, draw_error_screen).in_set(PresentationSet),
        )
        .add_systems(
            Update,
            (
                sync_staging_visuals,
                sync_audio_volume,
                sync_audio,
                report_silent_degradations,
                advance_reveal,
                sync_wait_ui,
            )
                .chain()
                .run_if(resource_exists::<Runtime>)
                .in_set(PresentationSet),
        )
        .add_systems(
            Update,
            (sync_auto_cue, sync_skip_cue).in_set(PresentationSet),
        );
}
