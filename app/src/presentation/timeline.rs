//! The Timeline: the surface, its cards and the focus scroll.

use super::focus_border;
use super::style::{TextFaces, BUTTON_BG, CARD_BG, DIM, FAINT, TEXT};
use crate::menu::{self, MenuAction, MenuFocus, MenuState, MenuView};
use crate::Runtime;
use bevy::prelude::*;
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::ScrollPosition;
use bevy::ui::UiSystems;
use bevy::ui_widgets::{ScrollArea, ScrollIntoView};

/// The Timeline's scroll container: only this body scrolls, and the focused card
/// is scrolled fully into view (`docs/title-and-menu.md` § Timeline `### Layout`
/// `#### Walk and scrolling`).
#[derive(Component)]
pub(crate) struct TimelineBody;
/// The Timeline's fixed two-column card grid.
#[derive(Component)]
pub(crate) struct TimelineGrid;

/// The Timeline surface: the header (the back control, the `Timeline` heading
/// and the right-aligned walk legend) over the explanatory line and either the
/// two-column internally scrolling card body or the single empty-state line.
/// Every declared scene renders once in order; a visited card shows its raw id
/// and selects, an unvisited card shows only `???` and is inert. Header,
/// explanation and legend stay outside the scroll body
/// (`docs/title-and-menu.md` § Timeline §2/§3, `### Layout`).
pub(super) fn timeline_surface(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    read_log: Option<&crate::read_log::ReadLogState>,
    scroll: Option<Vec2>,
) {
    let entries = runtime.runner.pack().timeline_entries();
    let empty = entries.is_empty();
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_grow: 1.0,
            min_height: Val::Px(0.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(16.0),
            ..default()
        })
        .with_children(|surface| {
            surface
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(14.0),
                    ..default()
                })
                .with_children(|header| {
                    header.spawn((
                        MenuAction::TimelineBack,
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                            border: UiRect::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(BUTTON_BG),
                        BorderColor::all(focus_border(menu, MenuAction::TimelineBack)),
                        Text::new(menu::TIMELINE_BACK),
                        faces.sans(16.0),
                        TextColor::from(TEXT),
                    ));
                    header.spawn((
                        Text::new(menu::TIMELINE_HEADING),
                        faces.sans_semibold(24.0),
                        TextColor::from(TEXT),
                    ));
                    header.spawn(Node {
                        flex_grow: 1.0,
                        ..default()
                    });
                    if !empty {
                        header.spawn((
                            Text::new(menu::TIMELINE_LEGEND),
                            faces.mono(12.0),
                            TextColor::from(FAINT),
                        ));
                    }
                });
            surface.spawn((
                Text::new(menu::TIMELINE_EXPLANATION),
                faces.sans(16.0),
                TextColor::from(DIM),
            ));

            if empty {
                // No order means no cards and no card focus; the exact empty
                // copy stands where the body would be
                // (`docs/title-and-menu.md` § Timeline §1/§3).
                surface
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        flex_grow: 1.0,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|body| {
                        body.spawn((
                            Text::new(menu::TIMELINE_EMPTY),
                            faces.sans(18.0),
                            TextColor::from(DIM),
                        ));
                    });
            } else {
                // Only the body scrolls; the header, explanation and legend stay
                // put (`docs/title-and-menu.md` § Timeline `### Layout`).
                surface
                    .spawn((
                        TimelineBody,
                        ScrollArea,
                        Node {
                            width: Val::Percent(100.0),
                            flex_grow: 1.0,
                            min_height: Val::Px(0.0),
                            flex_direction: FlexDirection::Column,
                            // The grid must size to its content rather than be
                            // stretched to the body's height, or the body sees
                            // no overflow and the focused-card scroll can never
                            // move (`docs/title-and-menu.md` § Timeline
                            // `### Layout`).
                            align_items: AlignItems::Start,
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                        // A confirm open/cancel rebuild restores the captured
                        // offset; every other rebuild starts at the top
                        // (`docs/title-and-menu.md` § Timeline §3/`### Layout`).
                        ScrollPosition(scroll.unwrap_or_default()),
                    ))
                    .with_children(|body| {
                        body.spawn((
                            TimelineGrid,
                            Node {
                                display: Display::Grid,
                                width: Val::Percent(100.0),
                                // The grid must keep its natural height inside
                                // the scrolling body; without this the flex
                                // column compresses it to the viewport, the
                                // body reports no overflow and the focused-card
                                // scroll never moves
                                // (`docs/title-and-menu.md` § Timeline `### Layout`).
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
                            for (index, id) in entries.iter().enumerate() {
                                let visited = read_log.is_some_and(|log| log.is_visited(id));
                                timeline_card(grid, faces, menu, index, id, visited);
                            }
                        });
                    });
            }
        });
}

/// The Timeline's card height, gutter and padding, at the approved roomier A
/// geometry (`docs/title-and-menu.md` § Timeline `### Layout`).
const TIMELINE_CARD_HEIGHT: f32 = 136.0;
const TIMELINE_GAP: f32 = 16.0;
const TIMELINE_CARD_PADDING: f32 = 18.0;
/// The locked label's faint tone (`rgba(204,204,217,.35)`).
pub(crate) const TIMELINE_LOCKED_TONE: Color =
    Color::srgba(204.0 / 255.0, 204.0 / 255.0, 217.0 / 255.0, 0.35);

/// One Timeline card: a fixed-height text card showing the raw visited id or
/// `???`. The visited and locked cards share one fill and the same reserved 2px
/// focus ring, with no size or fill change and no motion
/// (`docs/title-and-menu.md` § Timeline `### Layout`).
fn timeline_card(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    index: usize,
    scene_id: &str,
    visited: bool,
) {
    let action = MenuAction::TimelineCard(index);
    let label = if visited {
        scene_id.to_string()
    } else {
        menu::TIMELINE_LOCKED.to_string()
    };
    parent.spawn((
        action,
        Button,
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            height: Val::Px(TIMELINE_CARD_HEIGHT),
            padding: UiRect::all(Val::Px(TIMELINE_CARD_PADDING)),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(CARD_BG),
        BorderColor::all(focus_border(menu, action)),
        Text::new(label),
        faces.mono(18.0),
        TextColor::from(if visited { TEXT } else { TIMELINE_LOCKED_TONE }),
        // A scene id is one unbroken token, so the default word-boundary
        // break has no point to wrap at and the id would truncate; fall back
        // to character breaks inside a token so long ids wrap across lines
        // (`docs/title-and-menu.md` § Timeline `### Layout`).
        TextLayout {
            linebreak: LineBreak::WordOrCharacter,
            ..default()
        },
    ));
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
