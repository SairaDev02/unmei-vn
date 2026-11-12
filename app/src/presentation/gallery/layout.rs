//! The gallery grid's card geometry: the track minimum, the gutter, the art box
//! ratio, and the column count those three imply for a laid-out width.

use bevy::prelude::*;

/// The grid's card minimum and gutter: auto-fill `minmax(300px, 1fr)` with a
/// 20px row and column gap, three across at 1280×720 on both doors
/// (`docs/title-and-menu.md` § Gallery `### Layout` §4).
pub(super) const GALLERY_CARD_MIN: f32 = 300.0;
pub(super) const GALLERY_GAP: f32 = 20.0;
/// The card art box's 4:3 aspect (`docs/title-and-menu.md` § Gallery
/// `### Layout` §3/§4).
pub(super) const GALLERY_ART_RATIO: Vec2 = Vec2::new(4.0, 3.0);

/// The grid's column count for an available width: how many
/// `minmax(GALLERY_CARD_MIN, 1fr)` tracks with `GALLERY_GAP` gutters fit. Three
/// at the 1280×720 reference window from both doors
/// (`docs/title-and-menu.md` § Gallery `### Layout` §4).
pub(crate) fn gallery_column_count(width: f32) -> usize {
    if width <= 0.0 {
        return 1;
    }
    (((width + GALLERY_GAP) / (GALLERY_CARD_MIN + GALLERY_GAP)).floor() as usize).max(1)
}
