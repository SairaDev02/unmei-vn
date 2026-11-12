//! The gallery surface both doors share: the header, the empty line, the
//! scrolling auto-fill grid and one framed card.

use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use unmei_engine::GalleryEntry;

use super::super::assets::{gallery_entry_asset, AssetWatch};
use super::super::focus_border;
use super::super::style::{TextFaces, BUTTON_BG, CARD_BG, DIM, FAINT, TEXT};
use super::layout::{GALLERY_ART_RATIO, GALLERY_CARD_MIN, GALLERY_GAP};
use super::markers::{GalleryArt, GalleryArtBox, GalleryBody, GalleryGrid};
use super::still::gallery_still_view;
use crate::menu::{self, MenuAction, MenuFocus, MenuState};
use crate::Runtime;

/// The gallery surface both doors share: the header (the door-aware back
/// control and the `Gallery` heading, with the grid legend right-aligned) over
/// the body. With no entries the body is the single `No images in this pack`
/// line and the legend is not drawn; otherwise the body is the auto-fill card
/// grid (`docs/title-and-menu.md` § Gallery `### Layout` §1/§4).
pub(in crate::presentation) fn gallery_surface(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    asset_server: Option<&AssetServer>,
    mut watch: Option<&mut AssetWatch>,
) {
    let entries = runtime.runner.pack().gallery_entries();
    let empty = entries.is_empty();
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_grow: 1.0,
            // A growing column whose child scrolls must be allowed to shrink
            // below its content, or the surface expands instead of scrolling.
            min_height: Val::Px(0.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(16.0),
            ..default()
        })
        .with_children(|surface| {
            // The still replaces the grid and the header inside the same surface
            // (`docs/title-and-menu.md` § Gallery `### Layout` §1/§6).
            if menu.gallery_still {
                let label = match menu.focus {
                    MenuFocus::GalleryCard(index) => entries.get(index).map(GalleryEntry::label),
                    _ => None,
                }
                .unwrap_or_default();
                gallery_still_view(surface, faces, &label);
                return;
            }
            // The header carries the back control leftmost, the heading beside
            // it, and the grid legend right-aligned in the same row
            // (`docs/title-and-menu.md` § Gallery `### Layout` §1).
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
                        MenuAction::GalleryBack,
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                            border: UiRect::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(BUTTON_BG),
                        BorderColor::all(focus_border(menu, MenuAction::GalleryBack)),
                        Text::new(menu::gallery_back_label(menu.gallery_origin)),
                        faces.sans(16.0),
                        TextColor::from(TEXT),
                    ));
                    header.spawn((
                        Text::new(menu::GALLERY_HEADING),
                        faces.sans_semibold(26.0),
                        TextColor::from(TEXT),
                    ));
                    header.spawn(Node {
                        flex_grow: 1.0,
                        ..default()
                    });
                    if !empty {
                        header.spawn((
                            Text::new(menu::GALLERY_GRID_LEGEND),
                            faces.mono(12.0),
                            TextColor::from(FAINT),
                        ));
                    }
                });

            if empty {
                // No grid, no legend, no error: one quiet line where the grid
                // would be (`docs/title-and-menu.md` § Gallery `### Behavior`
                // §6).
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
                            Text::new(menu::GALLERY_EMPTY),
                            faces.sans(18.0),
                            TextColor::from(DIM),
                        ));
                    });
            } else {
                // Only the body scrolls; the header stays put
                // (`docs/title-and-menu.md` § Gallery `### Layout` §1).
                surface
                    .spawn((
                        GalleryBody,
                        ScrollArea,
                        Node {
                            width: Val::Percent(100.0),
                            flex_grow: 1.0,
                            min_height: Val::Px(0.0),
                            flex_direction: FlexDirection::Column,
                            // The grid must size to its content rather than be
                            // cross-axis stretched to the body's height, or the
                            // body sees no overflow and the focused-card scroll
                            // can never move (`docs/title-and-menu.md` § Gallery
                            // `### Layout` §1/§4).
                            align_items: AlignItems::Start,
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                    ))
                    .with_children(|body| {
                        body.spawn((
                            GalleryGrid,
                            Node {
                                display: Display::Grid,
                                width: Val::Percent(100.0),
                                // The grid must keep its natural height inside
                                // the scrolling body; without this the flex
                                // column compresses it to the viewport, the
                                // body reports no overflow and the focused-card
                                // scroll never moves (`docs/title-and-menu.md`
                                // § Gallery `### Layout` §1/§4).
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
                                gallery_card(
                                    grid,
                                    faces,
                                    menu,
                                    index,
                                    entry,
                                    asset_server,
                                    watch.as_deref_mut(),
                                );
                            }
                        });
                    });
            }
        });
}

/// One framed card: the overlay's panel treatment, a 4:3 art box and the
/// entry's label in a mono footer, left-aligned. Focus is the `FOCUS` 2px
/// border and nothing else, so the label tone never changes with focus
/// (`docs/title-and-menu.md` § Gallery `### Layout` §2/§3/§5).
fn gallery_card(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    index: usize,
    entry: &GalleryEntry,
    asset_server: Option<&AssetServer>,
    watch: Option<&mut AssetWatch>,
) {
    let action = MenuAction::GalleryCard(index);
    parent
        .spawn((
            action,
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(10.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(CARD_BG),
            BorderColor::all(focus_border(menu, action)),
        ))
        .with_children(|card| {
            card.spawn((
                GalleryArtBox,
                Node {
                    width: Val::Percent(100.0),
                    aspect_ratio: Some(GALLERY_ART_RATIO.x / GALLERY_ART_RATIO.y),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(CARD_BG),
                // The placeholder frame. Bevy UI has no dashed border, so
                // the faint frame is a 1px outline; an outline stays out of
                // layout, so the contain-fit percentages resolve against the
                // art box exactly. It shows while the file loads and forever
                // after a failed load, with no marker or glyph
                // (`docs/title-and-menu.md` § Gallery `### Layout` §2).
                Outline::new(Val::Px(1.0), Val::Px(0.0), FAINT),
            ))
            .with_children(|art| {
                let (row, path) = gallery_entry_asset(entry);
                if let (Some(server), Some(watch)) = (asset_server, watch) {
                    if let Some(handle) = watch.watch_image(server, row, path) {
                        art.spawn((
                            GalleryArt,
                            // The image fills the node the fit system sizes; the
                            // texture is drawn verbatim, so a sprite's unmarked
                            // pixels stay transparent — no tint, veil or
                            // checkerboard (`docs/title-and-menu.md` § Gallery
                            // `### Layout` §3).
                            ImageNode::new(handle).with_mode(NodeImageMode::Stretch),
                            Node {
                                width: Val::Px(0.0),
                                height: Val::Px(0.0),
                                ..default()
                            },
                        ));
                    }
                }
            });
            card.spawn((
                Text::new(entry.label()),
                faces.mono(12.5),
                TextColor::from(DIM),
            ));
        });
}
