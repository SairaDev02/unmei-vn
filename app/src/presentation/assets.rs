//! The one asset-load path: path producers, the watched-load set and the
//! one-per-path settle that feeds the failure posture.

use crate::failure::FailureRow;
use bevy::asset::{LoadState, RenderAssetUsages};
use bevy::image::ImageLoaderSettings;
use bevy::prelude::*;
use bevy_kira_audio::AudioSource;
use std::collections::HashSet;
use unmei_engine::GalleryEntry;

/// The assets whose loads the app is watching, so a missing or undecodable
/// background, sprite, track or select sound is reported as its inventory row
/// exactly once (rows 3–6). Each call site names its own row when it loads;
/// this resource only carries the watch to
/// [`report_silent_degradations`](super::play::report_silent_degradations).
#[derive(Resource, Default)]
pub(crate) struct AssetWatch {
    pending: Vec<WatchedAsset>,
    /// `(row, path)` pairs already logged, so a repeated staging command or a
    /// re-`load` of a failed path never warns twice.
    reported: HashSet<(FailureRow, String)>,
}

struct WatchedAsset {
    row: FailureRow,
    path: String,
    handle: WatchedHandle,
}

enum WatchedHandle {
    Image(Handle<Image>),
    Audio(Handle<AudioSource>),
}

impl WatchedHandle {
    fn id(&self) -> bevy::asset::UntypedAssetId {
        match self {
            WatchedHandle::Image(handle) => handle.id().untyped(),
            WatchedHandle::Audio(handle) => handle.id().untyped(),
        }
    }
}

impl AssetWatch {
    /// Load `path` as an image (row 3 or 4) and watch it. `None` means the path
    /// has already been reported, so the call site draws nothing and does not
    /// re-attempt it.
    ///
    /// Pack images load render-world-only, releasing the CPU pixel data once the
    /// texture is uploaded: expected and unmeasured. The size the cover- and
    /// contain-fits read (`Image::size_f32`) survives that release — Bevy 0.19.1
    /// extracts only `Image::data` and keeps the texture descriptor the size
    /// comes from.
    pub(crate) fn watch_image(
        &mut self,
        server: &AssetServer,
        row: FailureRow,
        path: impl Into<String>,
    ) -> Option<Handle<Image>> {
        self.watch(row, path.into(), WatchedHandle::Image, |path| {
            server
                .load_builder()
                .with_settings(|settings: &mut ImageLoaderSettings| {
                    settings.asset_usage = RenderAssetUsages::RENDER_WORLD;
                })
                .load(path)
        })
    }

    /// Load `path` as audio (row 5 or 6) and watch it; the same one-warn rule
    /// as [`AssetWatch::watch_image`].
    pub(crate) fn watch_audio(
        &mut self,
        server: &AssetServer,
        row: FailureRow,
        path: impl Into<String>,
    ) -> Option<Handle<AudioSource>> {
        self.watch(row, path.into(), WatchedHandle::Audio, |path| {
            server.load(path)
        })
    }

    /// The shared body of the two load-and-watch entry points; `tag` names the
    /// asset kind the call site is loading and `load` is its asset kind's own
    /// load call.
    fn watch<A: Asset>(
        &mut self,
        row: FailureRow,
        path: String,
        tag: impl FnOnce(Handle<A>) -> WatchedHandle,
        load: impl FnOnce(String) -> Handle<A>,
    ) -> Option<Handle<A>> {
        if self.reported.contains(&(row, path.clone())) {
            return None;
        }
        let handle = load(path.clone());
        self.pending.push(WatchedAsset {
            row,
            path,
            handle: tag(handle.clone()),
        });
        Some(handle)
    }

    /// Drain loads that have settled. A failed asset yields its row, path and
    /// error once; a loaded one is dropped silently; a loading one stays.
    pub(crate) fn settle(&mut self, server: &AssetServer) -> Vec<(FailureRow, String, String)> {
        let mut failed = Vec::new();
        let mut still_loading = Vec::new();
        for asset in std::mem::take(&mut self.pending) {
            match server.load_state(asset.handle.id()) {
                LoadState::Failed(error) => {
                    if self.reported.insert((asset.row, asset.path.clone())) {
                        failed.push((asset.row, asset.path, error.to_string()));
                    }
                }
                LoadState::NotLoaded | LoadState::Loading => still_loading.push(asset),
                LoadState::Loaded => {}
            }
        }
        self.pending = still_loading;
        failed
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// The asset path a `bg <id>` (and the manifest's `title_backdrop`) resolves to:
/// one producer of the rule, so the title backdrop stays "exactly like `bg`"
/// and the play path and the gallery share it
/// (`docs/title-and-menu.md` § Gallery § Entries §4).
pub(crate) fn background_asset_path(id: &str) -> String {
    format!("backgrounds/{id}.png")
}

/// The asset path a `sprite <npc_id> <pose>` resolves to: one producer of the
/// rule, so the play path and the gallery cannot fork a second inline copy
/// (`docs/title-and-menu.md` § Gallery § Entries §4).
pub(crate) fn sprite_asset_path(npc_id: &str, pose: &str) -> String {
    format!("sprites/{npc_id}/{pose}.png")
}

/// The asset path a `play <id>` resolves to: one producer of the rule, so the
/// title backdrop's `backgrounds/` sibling and the play path cannot drift.
pub(super) fn audio_asset_path(id: &str) -> String {
    format!("audio/{id}.ogg")
}

/// The failure row and asset path a gallery entry resolves to: one producer of
/// the kind-to-row/path mapping, so the card grid and the still cannot fork it
/// (`docs/title-and-menu.md` § Gallery § Entries §4).
pub(crate) fn gallery_entry_asset(entry: &GalleryEntry) -> (FailureRow, String) {
    match entry {
        GalleryEntry::Background { id } => (FailureRow::BackgroundAsset, background_asset_path(id)),
        GalleryEntry::Sprite { npc_id, pose } => {
            (FailureRow::SpriteAsset, sprite_asset_path(npc_id, pose))
        }
    }
}
