//! The gallery surface both doors share: the plugin's `gallery` tree with the
//! measured card grid mounted into it, or — while the still is up — the plugin's
//! `still` tree with the one contain-fit entry mounted into its art slot.
//!
//! The tree decides the frame, the heading, the legends and the empty line; this
//! module decides what they say and what the format cannot draw. The format has
//! no grid, no wrap, no scroll and no per-item image, so the player turns the
//! `gallery.cards` body into the scrolling auto-fill grid, spawns one numbered
//! item per entry with the entry's art and label in its slots, and keeps the
//! still's fitted art. Focus, the walk and the Keiko ids stay the menu's: each
//! card's control is `gallery.card.<n>` and carries the [`MenuAction`] that id
//! names (`docs/title-and-menu.md` § Gallery `### Layout` §1–§7).

use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use unmei_engine::ui::{Node as PluginNode, NodeKind};
use unmei_engine::GalleryEntry;

use super::super::assets::{gallery_entry_asset, AssetWatch};
use super::super::render::tree::Rendered;
use super::super::render::{Bind, Surface, UiLook, Words};
use super::super::style::{TextFaces, CARD_BG, FAINT};
use super::layout::{GALLERY_ART_RATIO, GALLERY_CARD_MIN, GALLERY_GAP};
use super::markers::{GalleryArt, GalleryArtBox, GalleryBody, GalleryGrid};
use super::still::gallery_still_view;
use crate::menu::{MenuAction, MenuState};
use crate::Runtime;

/// The surface's id, as the plugin folder names it.
pub(super) const SURFACE: &str = "gallery";

/// The gallery surface both doors share: the plugin's grid, or the still while
/// the player is viewing one entry (`docs/title-and-menu.md` § Gallery
/// `### Layout` §1/§6).
pub(in crate::presentation) fn gallery_surface(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    asset_server: Option<&AssetServer>,
    mut watch: Option<&mut AssetWatch>,
    look: Option<&UiLook>,
) {
    let Some(look) = look else {
        return;
    };
    let entries = runtime.runner.pack().gallery_entries();
    if menu.gallery_still {
        gallery_still_view(parent, faces, menu, &entries, look);
        return;
    }

    let frame = Frame {
        empty: entries.is_empty(),
    };
    let surface = Surface {
        look,
        faces,
        bind: &frame,
    };
    let Some(bound) = surface.spawn(SURFACE, parent) else {
        return;
    };
    let mut commands = parent.commands();
    if let Some(root) = bound.root {
        seat(&mut commands, root);
    }
    if entries.is_empty() {
        hide_copy_text(&mut commands, &bound, "gallery.legend");
    }

    let (Some(body), Some(item)) = (
        bound.lists.get("gallery.cards").copied(),
        bound.items.get("gallery.cards").cloned(),
    ) else {
        return;
    };
    commands.entity(body).insert((GalleryBody, ScrollArea));
    commands
        .entity(body)
        .entry::<Node>()
        .and_modify(|mut node| {
            node.height = Val::Auto;
            node.flex_grow = 1.0;
            node.min_height = Val::Px(0.0);
            node.align_items = AlignItems::Start;
            node.overflow = Overflow::scroll_y();
        });
    if entries.is_empty() {
        return;
    }
    commands.entity(body).with_children(|body| {
        body.spawn((
            GalleryGrid,
            Node {
                display: Display::Grid,
                width: Val::Percent(100.0),
                flex_shrink: 0.0,
                grid_template_columns: RepeatedGridTrack::minmax(
                    GridTrackRepetition::AutoFill,
                    MinTrackSizingFunction::Px(GALLERY_CARD_MIN),
                    MaxTrackSizingFunction::Fraction(1.0),
                ),
                grid_auto_flow: GridAutoFlow::Row,
                column_gap: Val::Px(GALLERY_GAP),
                row_gap: Val::Px(GALLERY_GAP),
                align_items: AlignItems::Start,
                ..default()
            },
        ))
        .with_children(|grid| {
            for (index, entry) in entries.iter().enumerate() {
                let control = card_control(index);
                let card = Card {
                    label: entry.label(),
                    index,
                    control: &control,
                };
                let template = numbered_item(&item, &control);
                let drawn = surface.spawn_item(&template, grid, &card, Some(&control));
                mount_card(grid, &drawn, entry, asset_server, watch.as_deref_mut());
            }
        });
    });
}

/// Mount a surface root in the seat that holds it. The seat owns the inset —
/// §4's page column pads its body by the page padding and the title door's own
/// root does the same — and the format has neither a grow nor a "fill what is
/// left", so the root drops the padding the tree authors and grows into the
/// height the seat leaves (`docs/title-and-menu.md` § Default look §4,
/// § Gallery `### Layout` §1).
pub(super) fn seat(commands: &mut Commands, root: Entity) {
    commands
        .entity(root)
        .entry::<Node>()
        .and_modify(|mut node| {
            node.padding = UiRect::ZERO;
            node.height = Val::Auto;
            node.flex_grow = 1.0;
            node.min_height = Val::Px(0.0);
        });
}

/// The one card template with its placed family member numbered for this card:
/// the format numbers `gallery.card.<n>` per item, 1-based, so entry `index`
/// places `gallery.card.<index + 1>` (`docs/ui-plugin-format.md` § `gallery`).
fn numbered_item(item: &PluginNode, control: &str) -> PluginNode {
    let mut numbered = item.clone();
    renumber(&mut numbered, control);
    numbered
}

fn renumber(node: &mut PluginNode, control: &str) {
    match &mut node.kind {
        NodeKind::Box(kind) => kind
            .children
            .iter_mut()
            .for_each(|child| renumber(child, control)),
        NodeKind::Slot(kind) => {
            if card_family_member(&kind.slot) {
                kind.slot = control.to_string();
            }
            kind.children
                .iter_mut()
                .for_each(|child| renumber(child, control));
        }
        _ => {}
    }
}

/// Whether `name` is a placed member of the `gallery.card.<n>` family, rather
/// than one of its `gallery.card.art` and `gallery.card.label` slots.
fn card_family_member(name: &str) -> bool {
    name.strip_prefix("gallery.card.")
        .is_some_and(|number| number.parse::<usize>().is_ok())
}

/// The Keiko id one card is: the family member the format numbers per item,
/// 1-based in the pack's entry order (`docs/keiko-spec.md` § Controls).
fn card_control(index: usize) -> String {
    crate::keiko::controls::gallery_card_id(index)
}

/// One card's art box and image: the 4:3 box with its placeholder frame, and the
/// entry's art contain-fit once its texture decodes. The art box is the tree's
/// `gallery.card.art` slot, which the format gives no ratio and no image to draw
/// with (`docs/title-and-menu.md` § Gallery `### Layout` §2–§4).
fn mount_card(
    parent: &mut ChildSpawnerCommands,
    drawn: &Rendered,
    entry: &GalleryEntry,
    asset_server: Option<&AssetServer>,
    watch: Option<&mut AssetWatch>,
) {
    let Some(art) = drawn.art.get("gallery.card.art").copied() else {
        return;
    };
    parent.commands().entity(art).insert((
        GalleryArtBox,
        BackgroundColor(CARD_BG),
        Outline::new(Val::Px(1.0), Val::Px(0.0), FAINT),
    ));
    parent
        .commands()
        .entity(art)
        .entry::<Node>()
        .and_modify(|mut node| {
            node.width = Val::Percent(100.0);
            node.aspect_ratio = Some(GALLERY_ART_RATIO.x / GALLERY_ART_RATIO.y);
            node.justify_content = JustifyContent::Center;
            node.align_items = AlignItems::Center;
            node.overflow = Overflow::clip();
        });
    let (Some(server), Some(watch)) = (asset_server, watch) else {
        return;
    };
    let (row, path) = gallery_entry_asset(entry);
    let Some(handle) = watch.watch_image(server, row, path) else {
        return;
    };
    parent.commands().entity(art).with_children(|art| {
        art.spawn((
            GalleryArt,
            ImageNode::new(handle).with_mode(NodeImageMode::Stretch),
            Node {
                width: Val::Px(0.0),
                height: Val::Px(0.0),
                ..default()
            },
        ));
    });
}

/// Hide the tree's `text` node that names the copy key `key`. The format has no
/// conditional text node and a bind answers `shows` only for slots, so the player
/// hides the node it asked the renderer to spawn: the grid legend is drawn only
/// when the pack declares art (`docs/title-and-menu.md` § Gallery `### Layout`
/// §1, `### Behavior` §6).
fn hide_copy_text(commands: &mut Commands, bound: &Rendered, key: &str) {
    let Some(entity) = copy_text_node(bound, key) else {
        return;
    };
    commands
        .entity(entity)
        .entry::<Node>()
        .and_modify(|mut node| node.display = Display::None);
}

/// The entity the renderer spawned for the tree's one `text` node naming `key`:
/// the tree and its nodes are recorded in the same pre-order walk, so the two
/// stay aligned (`docs/ui-plugin-format.md` § Nodes).
fn copy_text_node(bound: &Rendered, key: &str) -> Option<Entity> {
    let tree = bound.tree.as_ref()?;
    let mut index = 0;
    find_copy_text(tree, key, &bound.nodes, &mut index)
}

fn find_copy_text(
    node: &PluginNode,
    key: &str,
    nodes: &[Entity],
    index: &mut usize,
) -> Option<Entity> {
    let entity = nodes.get(*index).copied();
    *index += 1;
    if matches!(&node.kind, NodeKind::Text(text) if text.text == key) {
        return entity;
    }
    match &node.kind {
        NodeKind::Box(kind) => kind
            .children
            .iter()
            .find_map(|child| find_copy_text(child, key, nodes, index)),
        NodeKind::Slot(kind) => kind
            .children
            .iter()
            .find_map(|child| find_copy_text(child, key, nodes, index)),
        _ => None,
    }
}

/// What the gallery itself draws outside the cards: the empty line, which the
/// bind shows only when the pack declares no art (`docs/title-and-menu.md`
/// § Gallery `### Behavior` §6).
struct Frame {
    empty: bool,
}

impl Bind for Frame {
    fn words(&self, name: &str) -> Option<Words> {
        (name == "gallery.empty").then(|| Words::new("gallery.empty"))
    }

    fn shows(&self, name: &str) -> bool {
        name != "gallery.empty" || self.empty
    }
}

/// One card's binding: its entry's label, and the control the format numbers for
/// it. The label sits in the footer slot and is the control's recorded label too,
/// so a reader that asks what a card is finds the entry's words.
struct Card<'a> {
    label: String,
    index: usize,
    control: &'a str,
}

impl Bind for Card<'_> {
    fn words(&self, name: &str) -> Option<Words> {
        (name == "gallery.card.label" || name == self.control)
            .then(|| Words::text(self.label.clone()))
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        (name == self.control).then_some(MenuAction::GalleryCard(self.index))
    }
}
