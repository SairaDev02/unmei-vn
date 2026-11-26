//! The gallery grid's card geometry: the track minimum, the gutter, the art box
//! ratio, and the column count those three imply for a laid-out width.

use bevy::prelude::*;

/// The grid's card minimum and gutter: auto-fill `minmax(240px, 1fr)` with a
/// 20px row and column gap. Default look §4 seats the grid in the contents
/// page's right column, 795px wide at the 1280×720 reference, where three 240px
/// cards and their two 20px gutters (760px) fit; the title door's own seat is
/// the whole window less its padding, so the same tracks pack four there
/// (`docs/title-and-menu.md` § Default look §4, § Gallery `### Layout` §4).
pub(super) const GALLERY_CARD_MIN: f32 = 240.0;
pub(super) const GALLERY_GAP: f32 = 20.0;
/// The card art box's 4:3 aspect (`docs/title-and-menu.md` § Gallery
/// `### Layout` §3/§4).
pub(super) const GALLERY_ART_RATIO: Vec2 = Vec2::new(4.0, 3.0);

/// The grid's column count for an available width: how many
/// `minmax(GALLERY_CARD_MIN, 1fr)` tracks with `GALLERY_GAP` gutters fit. Three
/// in the 795px right column at the 1280×720 reference window, four from the
/// title door's wider seat (`docs/title-and-menu.md` § Gallery `### Layout` §4).
pub(crate) fn gallery_column_count(width: f32) -> usize {
    if width <= 0.0 {
        return 1;
    }
    (((width + GALLERY_GAP) / (GALLERY_CARD_MIN + GALLERY_GAP)).floor() as usize).max(1)
}
