//! The still view drawn from the plugin's `still` tree: one entry contain-fit in
//! the seat, with its label under the art and the still legend beneath, and the
//! two syncs that load and fit it.
//!
//! The tree decides the look; this module decides what the format cannot. The
//! format has no per-item image binding and no live text, so the player mounts
//! the `still.art` slot's image and keeps `still.label`'s words in step with the
//! browsed entry, and the click that closes the still stays its own: the surface
//! has no Keiko control id, so the tree places no control and the player makes
//! the surface root the one click target (`docs/ui-plugin-format.md` § `still`,
//! § Players §3).

use bevy::image::TRANSPARENT_IMAGE_HANDLE;
use bevy::prelude::*;

use super::super::assets::{gallery_entry_asset, AssetWatch};
use super::super::render::tree::Rendered;
use super::super::render::{Bind, Surface, UiLook, Words};
use super::super::style::{contain_size, TextFaces, FAINT};
use super::markers::{GalleryStillArt, GalleryStillArtBox, GalleryStillCaption};
use crate::menu::{MenuAction, MenuFocus, MenuState, MenuView};
use crate::Runtime;
use unmei_engine::GalleryEntry;

/// The surface's id, as the plugin folder names it.
const SURFACE: &str = "still";

/// The still view: the plugin's tree, its one entry's art contain-fit in the
/// `still.art` slot and the entry's label in `still.label`, over the door's
/// backdrop and scrim. The surface carries no fill, so a transparent sprite's
/// unmarked pixels stay transparent, and its root is the one button, so a click
/// anywhere closes back to the grid (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §5, `### Layout` §3/§6/§7).
pub(super) fn gallery_still_view(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    entries: &[GalleryEntry],
    look: &UiLook,
) {
    let label = match menu.focus {
        MenuFocus::GalleryCard(index) => entries.get(index).map(GalleryEntry::label),
        _ => None,
    }
    .unwrap_or_default();
    let bind = Entry { label: &label };
    let surface = Surface {
        look,
        faces,
        bind: &bind,
    };
    let Some(bound) = surface.spawn(SURFACE, parent) else {
        return;
    };
    let Some(root) = bound.root else {
        return;
    };
    let mut commands = parent.commands();
    super::grid::seat(&mut commands, root);
    commands.entity(root).insert((
        MenuAction::GalleryStillClose,
        Button,
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
    ));

    let (Some(list), Some(item)) = (
        bound.lists.get("still.entry").copied(),
        bound.items.get("still.entry").cloned(),
    ) else {
        return;
    };
    commands
        .entity(list)
        .entry::<Node>()
        .and_modify(|mut node| {
            node.height = Val::Auto;
            node.flex_grow = 1.0;
            node.min_height = Val::Px(0.0);
        });
    commands.entity(list).with_children(|list| {
        let drawn = surface.spawn_item(&item, list, &bind, None);
        grow(list, drawn.root);
        if let Some(caption) = drawn.slots.get("still.label").copied() {
            list.commands().entity(caption).insert(GalleryStillCaption);
        }
        mount_still_art(list, &drawn);
    });
}

/// Let a node take the height its seat leaves: the format has no grow, so the
/// still's column grows into what the legend leaves instead of claiming all of
/// it and pushing the legend off the surface.
fn grow(parent: &mut ChildSpawnerCommands, node: Option<Entity>) {
    let Some(node) = node else {
        return;
    };
    parent
        .commands()
        .entity(node)
        .entry::<Node>()
        .and_modify(|mut node| {
            node.height = Val::Auto;
            node.flex_grow = 1.0;
            node.min_height = Val::Px(0.0);
        });
}

/// The still's one image: the tree's `still.art` slot becomes the fitted art box
/// with its placeholder frame, transparent so the letterbox shows the backdrop
/// and no checkerboard is drawn, and the image stays the player's because the
/// format binds no art to a slot (`docs/title-and-menu.md` § Gallery
/// `### Layout` §3/§6).
fn mount_still_art(parent: &mut ChildSpawnerCommands, drawn: &Rendered) {
    let Some(art_box) = drawn.art.get("still.art").copied() else {
        return;
    };
    parent.commands().entity(art_box).insert((
        GalleryStillArtBox,
        BackgroundColor(Color::NONE),
        Outline::new(Val::Px(1.0), Val::Px(0.0), FAINT),
    ));
    parent
        .commands()
        .entity(art_box)
        .entry::<Node>()
        .and_modify(|mut node| {
            node.width = Val::Percent(100.0);
            node.height = Val::Auto;
            node.flex_grow = 1.0;
            node.min_height = Val::Px(0.0);
            node.justify_content = JustifyContent::Center;
            node.align_items = AlignItems::Center;
            node.overflow = Overflow::clip();
        });
    parent.commands().entity(art_box).with_children(|art| {
        art.spawn((
            GalleryStillArt { shown: None },
            ImageNode::default().with_mode(NodeImageMode::Stretch),
            Node {
                width: Val::Px(0.0),
                height: Val::Px(0.0),
                ..default()
            },
        ));
    });
}

/// The entry the still is showing: its label, verbatim.
struct Entry<'a> {
    label: &'a str,
}

impl Bind for Entry<'_> {
    fn words(&self, name: &str) -> Option<Words> {
        (name == "still.label").then(|| Words::text(self.label.to_string()))
    }
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
