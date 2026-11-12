//! The gallery's syncs: card art fitting, the laid-out column count the walk
//! steps by, and the focused-card scroll.

use bevy::prelude::*;
use bevy::ui_widgets::ScrollIntoView;

use super::super::style::{contain_size, FAINT};
use super::layout::{gallery_column_count, GALLERY_ART_RATIO};
use super::markers::{GalleryArt, GalleryArtBox, GalleryGrid};
use crate::menu::{MenuAction, MenuFocus, MenuState, MenuView};

/// Contain-fit the gallery's card art once its texture decodes: the drawn size
/// is the image's natural size times `min(box_w / image_w, box_h / image_h)`,
/// expressed as a percentage of the fixed 4:3 art box. The box centres the
/// image, so the free axis shows a letterbox of the card's own fill and no
/// pixel is cropped. Until a texture is resident — while loading and after a
/// failed load — the art node keeps its zero size and the box's faint outline
/// stands as the placeholder frame (`docs/title-and-menu.md` § Gallery
/// `### Layout` §2/§3).
pub(crate) fn sync_gallery_art(
    images: Res<Assets<Image>>,
    mut boxes: Query<(&Children, &mut Outline), With<GalleryArtBox>>,
    mut art: Query<(&ImageNode, &mut Node), With<GalleryArt>>,
) {
    for (children, mut outline) in &mut boxes {
        let mut loaded = false;
        for child in children.iter() {
            if let Ok((image_node, mut node)) = art.get_mut(child) {
                if let Some(image) = images.get(&image_node.image) {
                    let drawn = contain_size(image.size_f32(), GALLERY_ART_RATIO);
                    let width = Val::Percent(drawn.x / GALLERY_ART_RATIO.x * 100.0);
                    let height = Val::Percent(drawn.y / GALLERY_ART_RATIO.y * 100.0);
                    if node.width != width {
                        node.width = width;
                    }
                    if node.height != height {
                        node.height = height;
                    }
                    loaded = true;
                }
            }
        }
        let want = if loaded { Color::NONE } else { FAINT };
        if outline.color != want {
            outline.color = want;
        }
    }
}

/// Keep the gallery's walk in step with the grid's laid-out column count, so
/// `↑`/`↓` step by exactly one row at any window size. `ComputedNode` sizes are
/// physical pixels, so the logical width multiplies by the inverse scale
/// factor before [`gallery_column_count`].
pub(crate) fn sync_gallery_columns(
    grid: Query<&ComputedNode, With<GalleryGrid>>,
    mut menu: ResMut<MenuState>,
) {
    let Ok(node) = grid.single() else {
        return;
    };
    let width = node.size().x * node.inverse_scale_factor();
    if width <= 0.0 {
        // An unlaid-out grid reports no size; keep the last good count rather
        // than collapsing the walk's row step to one.
        return;
    }
    let columns = gallery_column_count(width);
    if menu.gallery_columns != columns {
        menu.gallery_columns = columns;
    }
}

/// Scroll the focused gallery card into view when the walk moves, and again
/// whenever a structural rebuild respawns the grid: the still closing and every
/// other signature change leave the focus where it was while the fresh body
/// starts at the top, so the unchanged focus must ask for its card again. The
/// card's own entity is the freshness signal — a real focus move and a rebuilt
/// tree both land on a different card entity — so an idle frame or a wheel
/// scroll never yanks the view back (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §4/§5, `### Layout` §4).
pub(crate) fn scroll_gallery_focus(
    menu: Res<MenuState>,
    cards: Query<(Entity, &MenuAction)>,
    mut last: Local<Option<Entity>>,
    mut commands: Commands,
) {
    let focus_index = match menu.view {
        MenuView::Gallery => match menu.focus {
            MenuFocus::GalleryCard(index) => Some(index),
            _ => None,
        },
        _ => None,
    };
    // No card means none is remembered: while the still is up the grid is gone,
    // so closing back to the same entry is a fresh card the body must reveal.
    let focused_card = focus_index.and_then(|index| {
        cards
            .iter()
            .find(|(_, action)| **action == MenuAction::GalleryCard(index))
            .map(|(entity, _)| entity)
    });
    if focused_card == *last {
        return;
    }
    *last = focused_card;
    let Some(entity) = focused_card else {
        return;
    };
    commands.trigger(ScrollIntoView { entity });
}
