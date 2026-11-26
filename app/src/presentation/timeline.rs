//! The Timeline drawn from the plugin's `timeline` surface: the heading and the
//! walk legend over the explanatory line, one card per declared scene in the
//! measured two-column grid, and the empty line for a pack that declares none
//! (Default look §4, spec §11).
//!
//! The tree decides the look; this module decides what it says and what the
//! format cannot. It binds the explanation, the empty line and each card — the
//! raw scene id on a card the read log has visited, `timeline.locked` on one it
//! has not, whose control is disabled so the label draws faint — and spawns one
//! copy of the tree's card template per declared scene, in order.
//!
//! The tree authors the frame and no way out: the contents page's left column
//! owns that, and `MenuAction::TimelineBack` stays the menu's click twin of `Esc`,
//! keyed by control id rather than by a node. Three things the format has no
//! property for are mounted into the tree's own nodes instead: the card list is
//! the scrolling body, the two-column grid is the node its cards are laid in, and
//! a card's label wraps inside a long scene id
//! (`docs/title-and-menu.md` § Timeline `### Layout`).

use super::render::tree::{Bind, ControlState, Surface, Words};
use super::render::UiLook;
use super::style::TextFaces;
use crate::menu::{MenuAction, MenuFocus, MenuState, MenuView};
use crate::Runtime;
use bevy::prelude::*;
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::ScrollPosition;
use bevy::ui::UiSystems;
use bevy::ui_widgets::{ScrollArea, ScrollIntoView};

/// The surface's id, as the plugin folder names it.
const SURFACE: &str = "timeline";

/// The cards' list: the tree's node the engine mounts the scroll body and the
/// grid in.
const CARDS: &str = "timeline.cards";

/// One card's label, the slot its words draw in.
const CARD_LABEL: &str = "timeline.card.label";

/// The control one card is. The template places one member of the indexed family
/// `timeline.card.<n>` and the engine numbers it per item, 1-based, so every item
/// is spawned from that one member and carries its own
/// [`MenuAction::TimelineCard`].
const CARD_CONTROL: &str = "timeline.card.1";

/// The measured card grid's gutter (`docs/title-and-menu.md` § Timeline
/// `### Layout`).
const TIMELINE_GAP: f32 = 16.0;

/// The Timeline's scroll container: only the card body scrolls, and the focused
/// card is scrolled fully into view (`docs/title-and-menu.md` § Timeline `### Layout`
/// `#### Walk and scrolling`). It is the tree's card list, because the format has
/// no scroll property.
#[derive(Component)]
pub(crate) struct TimelineBody;

/// The Timeline's fixed two-column card grid, mounted inside [`TimelineBody`]
/// because the format has no grid and its list stacks items in one column.
#[derive(Component)]
pub(crate) struct TimelineGrid;

/// Build the Timeline from the plugin's tree: the heading row with its walk
/// legend, the explanatory line, the empty line when the pack declares no
/// scenes, and the cards in the grid the engine mounts into the tree's list.
///
/// The three mounts carry what the format has no property for, each keeping the
/// geometry the approved layout measured. The tree's card list is the scroll
/// body, so a confirm open or cancel restores the offset it captured while every
/// other rebuild starts at the top. The grid keeps its natural height, because a
/// stretched or shrunk grid hides its overflow and the focused-card scroll could
/// never move. Each label falls back to character breaks, because a scene id is
/// one unbroken token the default word boundary has no point to break at
/// (`docs/title-and-menu.md` § Timeline `### Layout`).
pub(super) fn timeline_surface(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    _menu: &MenuState,
    runtime: &Runtime,
    read_log: Option<&crate::read_log::ReadLogState>,
    scroll: Option<Vec2>,
    look: Option<&UiLook>,
) {
    let Some(look) = look else {
        return;
    };
    let entries = runtime.runner.pack().timeline_entries();
    let bind = Timeline {
        empty: entries.is_empty(),
    };
    let surface = Surface {
        look,
        faces,
        bind: &bind,
    };
    let Some(bound) = surface.spawn(SURFACE, parent) else {
        return;
    };
    if let Some(root) = bound.root {
        parent
            .commands()
            .entity(root)
            .entry::<Node>()
            .and_modify(|mut node| {
                node.padding = UiRect::ZERO;
            });
    }
    if entries.is_empty() {
        return;
    }
    let (Some(body), Some(item)) = (
        bound.lists.get(CARDS).copied(),
        bound.items.get(CARDS).cloned(),
    ) else {
        return;
    };

    let mut commands = parent.commands();
    commands
        .entity(body)
        .entry::<Node>()
        .and_modify(|mut node| {
            node.flex_grow = 1.0;
            node.min_height = Val::Px(0.0);
            node.align_items = AlignItems::Start;
            node.overflow = Overflow::scroll_y();
        });
    commands.entity(body).insert((
        TimelineBody,
        ScrollArea,
        ScrollPosition(scroll.unwrap_or_default()),
    ));

    let mut labels = Vec::new();
    commands.entity(body).with_children(|list| {
        list.spawn((
            TimelineGrid,
            Node {
                display: Display::Grid,
                width: Val::Percent(100.0),
                flex_shrink: 0.0,
                grid_template_columns: RepeatedGridTrack::flex(2, 1.0),
                grid_auto_flow: GridAutoFlow::Row,
                column_gap: Val::Px(TIMELINE_GAP),
                row_gap: Val::Px(TIMELINE_GAP),
                align_items: AlignItems::Start,
                ..default()
            },
        ))
        .with_children(|grid| {
            for (index, scene_id) in entries.iter().enumerate() {
                let card = Card {
                    index,
                    scene_id,
                    visited: read_log.is_some_and(|log| log.is_visited(scene_id)),
                };
                let rendered = surface.spawn_item(&item, grid, &card, Some(CARD_CONTROL));
                if let Some(label) = rendered.slots.get(CARD_LABEL) {
                    labels.push(*label);
                }
            }
        });
    });

    for label in labels {
        commands.entity(label).insert(TextLayout {
            linebreak: LineBreak::WordOrCharacter,
            ..default()
        });
    }
}

/// What the surface itself says outside its cards: the explanatory line in every
/// state, and the empty line only when the pack declares no scenes
/// (`docs/title-and-menu.md` § Timeline §1/§3).
struct Timeline {
    empty: bool,
}

impl Bind for Timeline {
    fn words(&self, name: &str) -> Option<Words> {
        match name {
            "timeline.explanation" => Some(Words::new("timeline.explanation")),
            "timeline.empty" => Some(Words::new("timeline.empty")),
            _ => None,
        }
    }

    fn shows(&self, name: &str) -> bool {
        name != "timeline.empty" || self.empty
    }
}

/// One card of the list: its slot's words, its own control and the state that
/// control is in. A visited card shows its raw scene id and selects; a locked
/// card shows `???` and is disabled, which draws its label faint and leaves
/// activation inert (`docs/title-and-menu.md` § Timeline §2/§3).
struct Card<'a> {
    index: usize,
    scene_id: &'a str,
    visited: bool,
}

impl Bind for Card<'_> {
    fn words(&self, name: &str) -> Option<Words> {
        if name != CARD_LABEL && name != CARD_CONTROL {
            return None;
        }
        Some(if self.visited {
            Words::text(self.scene_id.to_string())
        } else {
            Words::new("timeline.locked")
        })
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        (name == CARD_CONTROL).then_some(MenuAction::TimelineCard(self.index))
    }

    fn state(&self, _name: &str) -> ControlState {
        ControlState {
            disabled: !self.visited,
            ..default()
        }
    }
}

/// Scroll the focused Timeline card fully into view when the walk moves. The
/// trigger fires only on a real focus change, and a confirm-cancel returning to
/// the same card re-triggers it so the target scroll is restored
/// (`docs/title-and-menu.md` § Timeline `### Layout`).
pub(crate) fn scroll_timeline_focus(
    menu: Res<MenuState>,
    cards: Query<(Entity, &MenuAction)>,
    mut last: Local<Option<usize>>,
    mut commands: Commands,
) {
    let current = match menu.view {
        MenuView::Timeline => match menu.focus {
            MenuFocus::TimelineCard(index) => Some(index),
            _ => None,
        },
        _ => None,
    };
    if current == *last {
        return;
    }
    *last = current;
    let Some(index) = current else {
        return;
    };
    if let Some((entity, _)) = cards
        .iter()
        .find(|(_, action)| **action == MenuAction::TimelineCard(index))
    {
        commands.trigger(ScrollIntoView { entity });
    }
}

/// Register the Timeline's focus scroll. It is last in the `PostUpdate`
/// geometry group, after the gallery's still fit as it always has been, and
/// before `PostLayout`.
pub(super) fn register(app: &mut App) {
    app.add_systems(
        PostUpdate,
        scroll_timeline_focus
            .after(UiSystems::Layout)
            .after(super::gallery::fit_gallery_still)
            .before(UiSystems::PostLayout),
    );
}
