//! The Gallery: grid, cards, still, title-door scrim and its art,
//! column, fit and scroll syncs.
//!
//! This root composes the surface's own modules — the node markers, the grid's
//! card geometry, the grid and card spawn, the still view, the title door's
//! backdrop and scrim, and the sync systems — and re-exports their items, so
//! every `gallery::<Item>` path callers already use stays the same.

mod grid;
mod layout;
mod markers;
mod still;
mod sync;
mod title_door;

use crate::PresentationSet;
use bevy::prelude::*;
use bevy::ui::UiSystems;

pub(super) use grid::gallery_surface;
// The column count and the node markers are addressed by full path from the
// app's tests, the only readers outside this module, so both re-exports are
// test-only: a non-test build carries neither.
#[cfg(test)]
pub(crate) use layout::gallery_column_count;
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "the pre-split `gallery::<Marker>` paths stay valid; `GalleryStillCaption` has no reader outside `gallery`"
)]
pub(crate) use markers::{
    GalleryArt, GalleryArtBox, GalleryBody, GalleryGrid, GalleryScrim, GalleryStillArt,
    GalleryStillArtBox, GalleryStillCaption,
};
pub(crate) use still::{fit_gallery_still, sync_gallery_still};
pub(crate) use sync::{scroll_gallery_focus, sync_gallery_art, sync_gallery_columns};
pub(super) use title_door::build_title_gallery;

/// Register the gallery's syncs. The `Update` art/still pair keeps its place in
/// the menu-tree chain: the backdrop fit, then the cards, then the still. The
/// `PostUpdate` geometry group runs after UI layout and before `PostLayout`, so
/// the clip rects and a11y nodes observe this frame's fitted sizes.
pub(super) fn register(app: &mut App) {
    app.add_systems(
        Update,
        (
            sync_gallery_art.after(super::title::fit_title_backdrop),
            sync_gallery_still,
        )
            .chain()
            .in_set(PresentationSet),
    )
    .add_systems(
        PostUpdate,
        (
            sync_gallery_columns,
            scroll_gallery_focus,
            fit_gallery_still,
        )
            .chain()
            .after(UiSystems::Layout)
            .before(UiSystems::PostLayout),
    );
}
