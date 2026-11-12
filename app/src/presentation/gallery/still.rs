//! The still view: one entry contain-fit above a foot caption strip, and the
//! two syncs that load and fit it.

use bevy::image::TRANSPARENT_IMAGE_HANDLE;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use super::super::assets::{gallery_entry_asset, AssetWatch};
use super::super::style::{contain_size, TextFaces, DIM, FAINT};
use super::markers::{GalleryStillArt, GalleryStillArtBox, GalleryStillCaption};
use crate::menu::{self, MenuAction, MenuFocus, MenuState, MenuView};
use crate::Runtime;

/// The still view: one entry alone over the door's backdrop and scrim, contain-
/// fit in the window above a foot caption strip, with the still legend under the
/// caption in the faint tone. The surface is one full-window button, so a click
/// anywhere closes back to the grid; its children carry [`FocusPolicy::Pass`] so
/// the art and text never swallow that click. The art node is spawned empty and
/// [`sync_gallery_still`] loads the focused entry into it, so stepping changes
/// only the image and the caption and never rebuilds the backdrop
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §5, `### Layout` §6/§7).
pub(super) fn gallery_still_view(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    label: &str,
) {
    parent
        .spawn((
            MenuAction::GalleryStillClose,
            Button,
            Node {
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            // The door's backdrop and scrim show through the still, never a
            // fill (`docs/title-and-menu.md` § Gallery `### Layout` §3/§7).
            BackgroundColor(Color::NONE),
            BorderColor::all(Color::NONE),
        ))
        .with_children(|still| {
            still
                .spawn((
                    GalleryStillArtBox,
                    Node {
                        width: Val::Percent(100.0),
                        flex_grow: 1.0,
                        min_height: Val::Px(0.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    // Transparent, so a sprite's unmarked pixels show the
                    // backdrop rather than a fill, and no checkerboard is drawn
                    // (`docs/title-and-menu.md` § Gallery `### Layout` §3).
                    BackgroundColor(Color::NONE),
                    // The placeholder frame. Bevy UI has no dashed border, so it
                    // is a faint 1px outline; an outline stays out of layout, so
                    // the contain-fit size resolves against the art box exactly.
                    // It shows while the file loads and forever after a failed
                    // load (`docs/title-and-menu.md` § Gallery `### Layout` §2,
                    // `### Behavior` §6).
                    Outline::new(Val::Px(1.0), Val::Px(0.0), FAINT),
                    FocusPolicy::Pass,
                ))
                .with_children(|art| {
                    art.spawn((
                        GalleryStillArt { shown: None },
                        // The image fills the node the fit system sizes; the
                        // texture is drawn verbatim, so a sprite's unmarked
                        // pixels stay transparent
                        // (`docs/title-and-menu.md` § Gallery `### Layout` §3).
                        ImageNode::default().with_mode(NodeImageMode::Stretch),
                        Node {
                            width: Val::Px(0.0),
                            height: Val::Px(0.0),
                            ..default()
                        },
                        FocusPolicy::Pass,
                    ));
                });
            // The foot strip: the caption, then the still legend, both centred
            // under the art on the backdrop
            // (`docs/title-and-menu.md` § Gallery `### Layout` §6).
            still
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        padding: UiRect::top(Val::Px(16.0)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(6.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                ))
                .with_children(|foot| {
                    foot.spawn((
                        GalleryStillCaption,
                        Text::new(label.to_string()),
                        faces.mono(16.0),
                        TextColor::from(DIM),
                        FocusPolicy::Pass,
                    ));
                    foot.spawn((
                        Text::new(menu::GALLERY_STILL_LEGEND),
                        faces.mono(12.0),
                        TextColor::from(FAINT),
                        FocusPolicy::Pass,
                    ));
                });
        });
}

/// Load the focused entry's art into the still and repaint its caption when the
/// browse step moves, so `←`/`→` change only the image and the caption. The
/// step changes no structural value, so the tree is not rebuilt and the door's
/// backdrop and scrim stand unchanged behind it
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §5/§6).
///
/// The load rides the shared `AssetWatch` funnel at rows 3/4 by kind, so a path
/// the card grid already reported logs no second line; a failed or unavailable
/// path leaves the art node on the transparent fallback and the placeholder
/// frame standing.
pub(crate) fn sync_gallery_still(
    menu: Res<MenuState>,
    runtime: Option<Res<Runtime>>,
    asset_server: Option<Res<AssetServer>>,
    mut watch: Option<ResMut<AssetWatch>>,
    mut captions: Query<&mut Text, With<GalleryStillCaption>>,
    mut art: Query<(&mut GalleryStillArt, &mut ImageNode)>,
) {
    let shown = match (menu.view, menu.gallery_still, menu.focus) {
        (MenuView::Gallery, true, MenuFocus::GalleryCard(index)) => Some(index),
        _ => None,
    };
    let Some(index) = shown else {
        return;
    };
    let Some(runtime) = runtime else {
        return;
    };
    let entries = runtime.runner.pack().gallery_entries();
    let Some(entry) = entries.get(index) else {
        return;
    };

    // The caption is cheap to keep in step; the image load happens once per
    // shown entry, so a rebuild reloads exactly once and a step reloads the
    // new entry.
    let label = entry.label();
    for mut caption in &mut captions {
        if caption.0 != label {
            caption.0 = label.clone();
        }
    }
    if !art.iter().any(|(node, _)| node.shown != shown) {
        return;
    }
    let (row, path) = gallery_entry_asset(entry);
    let handle = match (asset_server.as_deref(), watch.as_deref_mut()) {
        (Some(server), Some(watch)) => watch
            .watch_image(server, row, path)
            .unwrap_or(TRANSPARENT_IMAGE_HANDLE),
        _ => TRANSPARENT_IMAGE_HANDLE,
    };
    for (mut node, mut image) in &mut art {
        node.shown = shown;
        if image.image != handle {
            image.image = handle.clone();
        }
    }
}

/// Contain-fit the still's one image once its texture decodes: the drawn size
/// is the image's natural size times `min(box_w / image_w, box_h / image_h)`
/// against the art box above the foot strip, so both axes fit, the aspect is
/// preserved and the free axis letterboxes to the backdrop and scrim. The box
/// centres the image, so nothing is cropped and a transparent sprite's unmarked
/// pixels stay transparent. Until a texture is resident — while loading and
/// after a failed load — the art node keeps its zero size and the box's faint
/// outline stands as the placeholder frame (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §5, `### Layout` §3/§6).
pub(crate) fn fit_gallery_still(
    images: Res<Assets<Image>>,
    mut boxes: Query<(&Children, &ComputedNode, &mut Outline), With<GalleryStillArtBox>>,
    mut art: Query<(&ImageNode, &mut Node), With<GalleryStillArt>>,
) {
    for (children, box_node, mut outline) in &mut boxes {
        let box_size = box_node.size() * box_node.inverse_scale_factor();
        let mut loaded = false;
        for child in children.iter() {
            if let Ok((image_node, mut node)) = art.get_mut(child) {
                // The transparent fallback is a resident 1x1 image, so it must
                // not count as loaded: the placeholder frame stands and the node
                // keeps its zero size for absent or failed art
                // (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
                if image_node.image == TRANSPARENT_IMAGE_HANDLE {
                    continue;
                }
                if let Some(image) = images.get(&image_node.image) {
                    let drawn = contain_size(image.size_f32(), box_size);
                    let width = Val::Px(drawn.x);
                    let height = Val::Px(drawn.y);
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
