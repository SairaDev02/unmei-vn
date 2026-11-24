//! The play surface: its applied caches, reveal, channels, frame and chrome.
//!
//! Owns the staging draws, audio application, the plugin-drawn play frame, the
//! Notice text node and the startup error screen, and exposes the one concrete
//! replay request the session recipes call (ADR 0013).
//!
//! This root composes the surface's own modules — the components and applied
//! caches, the reveal, the frame drawn from the plugin's `play` surface, the
//! chrome that is not a surface, the startup error screen and the sync systems
//! — and re-exports the items callers and the app's tests already use.

mod chrome;
mod error_screen;
mod frame;
mod reveal;
mod state;
mod sync;

#[cfg(test)]
mod tests;

use bevy::prelude::*;

use crate::PresentationSet;
use crate::Runtime;

pub(crate) use chrome::{setup_ui, sync_notice};
pub(crate) use error_screen::draw_error_screen;
pub(crate) use frame::PlayFrame;
pub(crate) use frame::{read_play_frame, spawn_play_frame, sync_play_frame, sync_play_options};
pub(crate) use reveal::TextReveal;
pub(crate) use state::{request_replay, AppliedAudio, AppliedUi, AppliedVisuals, AppliedVolume};
pub(crate) use state::{Bgm, ChoiceButton, Sfx};
pub(crate) use sync::{
    advance_reveal, report_silent_degradations, sync_audio, sync_audio_volume, sync_staging_visuals,
};

// The line the reveal draws and the node markers are addressed by full path
// from the app's tests, which is the only reader outside this module, so these
// re-exports are test-only.
#[cfg(test)]
pub(crate) use reveal::rendered_line;
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "the pre-split `play::<Marker>` paths stay valid; `ErrorScreen` and `NoticeText` have no reader outside `play`"
)]
pub(crate) use state::{BackgroundSprite, ErrorScreen, NoticeText, NpcSprite};

/// Initialize the play surface's separately stored realization state: the
/// applied audio/volume/visual/UI caches, the frame the renderer fills and the
/// reveal clock. The headless
/// [`RuntimeCorePlugin`](crate::RuntimeCorePlugin) keeps them through this
/// function, so its input and session tests exercise the real state.
pub(crate) fn init(app: &mut App) {
    app.init_resource::<AppliedAudio>()
        .init_resource::<AppliedVolume>()
        .init_resource::<AppliedVisuals>()
        .init_resource::<AppliedUi>()
        .init_resource::<PlayFrame>()
        .init_resource::<TextReveal>();
}

/// Register the play surface's systems with their intrinsic edges: the Notice
/// text node and the startup error screen; the Runtime-gated play chain
/// (`staging visuals -> volume -> audio -> settle -> reveal`); and the frame,
/// which reads the reveal after it aged and writes the nodes before the
/// renderer's own state repaints.
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
            )
                .chain()
                .run_if(resource_exists::<Runtime>)
                .in_set(PresentationSet),
        )
        .add_systems(
            Update,
            (
                spawn_play_frame,
                read_play_frame,
                sync_play_options,
                sync_play_frame,
            )
                .chain()
                .after(advance_reveal)
                .in_set(PresentationSet)
                .before(crate::presentation::render::Repaint),
        );
}
