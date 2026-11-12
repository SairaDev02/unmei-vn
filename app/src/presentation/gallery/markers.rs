//! The gallery's shared node markers: the title-door scrim, the scrolling body,
//! the auto-fill grid, a card's art box and image, and the still view's art
//! box, image and caption.
//!
//! Only the markers live here. The card geometry they are laid out with is
//! [`super::layout`]'s.

use bevy::prelude::*;

/// The gallery's title-door scrim: a full-window radial that darkens only the
/// door's backdrop, so the heading, labels and legends hold contrast over pack
/// art. It is spawned after the backdrop and before the surface, and no other
/// door draws one (`docs/title-and-menu.md` § Gallery `### Layout` §4/§7).
#[derive(Component)]
pub(crate) struct GalleryScrim;

/// The gallery's scroll container: only this body scrolls, and the focused card
/// is scrolled into view (`docs/title-and-menu.md` § Gallery `### Layout`
/// §1/§4).
#[derive(Component)]
pub(crate) struct GalleryBody;
/// The gallery's auto-fill card grid, whose laid-out width fixes the current
/// column count (`### Layout` §4/§5).
#[derive(Component)]
pub(crate) struct GalleryGrid;

/// A card's 4:3 art box. Its outline is the dashed placeholder frame, shown
/// while the art loads and after a failed load, and hidden once it decodes.
#[derive(Component)]
pub(crate) struct GalleryArtBox;
/// One card's image node, contain-fit inside its 4:3 art box.
#[derive(Component)]
pub(crate) struct GalleryArt;
/// The still view's art box: the window above the foot caption strip, where the
/// one entry is contain-fit over the door's backdrop and scrim. Its outline is
/// the placeholder frame, shown while the art loads and after a failed load
/// (`docs/title-and-menu.md` § Gallery `### Layout` §6).
#[derive(Component)]
pub(crate) struct GalleryStillArtBox;
/// The still view's one image node, contain-fit inside its art box. It carries
/// the entry index currently loaded (`None` before the first sync), so a browse
/// step or a rebuilt tree reloads exactly once.
#[derive(Component)]
pub(crate) struct GalleryStillArt {
    pub(crate) shown: Option<usize>,
}
/// The still view's caption: the focused entry's label, verbatim, centred under
/// the art in the mono face (`docs/title-and-menu.md` § Gallery `### Behavior`
/// §5/§7).
#[derive(Component)]
pub(crate) struct GalleryStillCaption;
