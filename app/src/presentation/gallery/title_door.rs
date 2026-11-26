//! The title door's gallery: the pack backdrop, the gallery-local scrim that
//! darkens it, the door's own way back, and the shared surface under them.

use bevy::prelude::*;
use bevy::ui::{BackgroundGradient, ColorStop, RadialGradient, RadialGradientShape, UiPosition};

use super::super::assets::AssetWatch;
use super::super::focus_border;
use super::super::render::UiLook;
use super::super::style::{TextFaces, BUTTON_BG, TEXT};
use super::super::title::{spawn_title_backdrop, title_gradient};
use super::super::MenuRoot;
use super::grid::gallery_surface;
use super::markers::GalleryScrim;
use crate::menu::{self, MenuAction, MenuState};
use crate::Runtime;

/// The gallery's title-door scrim, as the prototype stub pinned it
/// (`prototype/86-gallery`, `app/prototype/gallery.html`): a radial from
/// `rgba(5, 7, 10, 0.34)` at the middle to `rgba(5, 7, 10, 0.74)` at the edges
/// (`docs/title-and-menu.md` § Gallery `### Layout` §4/§7).
const GALLERY_SCRIM_INNER: Color = Color::srgba(5.0 / 255.0, 7.0 / 255.0, 10.0 / 255.0, 0.34);
const GALLERY_SCRIM_OUTER: Color = Color::srgba(5.0 / 255.0, 7.0 / 255.0, 10.0 / 255.0, 0.74);

/// The title door's gallery: the shared gallery surface spanning the window
/// over the title's backdrop, exactly as Settings does. Between the backdrop
/// and the surface sits the gallery-local radial scrim, which darkens the
/// backdrop only and never the card art
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6, `### Layout` §7).
///
/// The surface has no back control — §4 gives the way back to the seat that
/// holds it — so this door draws `← Back to title` itself, above the surface
/// and outside its tree (`docs/title-and-menu.md` § Default look §4,
/// § Gallery `### Behavior` §3).
pub(in crate::presentation) fn build_title_gallery(
    commands: &mut Commands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    asset_server: Option<&AssetServer>,
    mut watch: Option<&mut AssetWatch>,
    look: Option<&UiLook>,
) {
    let backdrop = runtime.runner.pack().manifest.title_backdrop.clone();
    commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(32.0)),
                ..default()
            },
            title_gradient(),
        ))
        .with_children(|root| {
            // The same pack backdrop stands behind the gallery opened from the
            // title (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
            spawn_title_backdrop(
                root,
                backdrop.as_deref(),
                asset_server,
                watch.as_deref_mut(),
            );
            spawn_title_gallery_scrim(root);
            spawn_door_back(root, faces, menu);
            gallery_surface(root, faces, menu, runtime, asset_server, watch, look);
        });
}

/// The title door's own way back, above the surface: `← Back to title`, the
/// door-aware label the gallery's one copy key names
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §3/§7).
fn spawn_door_back(parent: &mut ChildSpawnerCommands, faces: &TextFaces, menu: &MenuState) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            margin: UiRect::bottom(Val::Px(16.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
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
        });
}

/// The gallery's title-door scrim gradient: the prototype's radial from
/// [`GALLERY_SCRIM_INNER`] at the middle to [`GALLERY_SCRIM_OUTER`] at the
/// edges (`docs/title-and-menu.md` § Gallery `### Layout` §4/§7).
///
/// `UiPosition::CENTER` is the node's middle: a `UiPosition`'s `x`/`y` are
/// offsets from the anchor, so `CENTER.at_percent(50, 50)` would sit on the
/// bottom-right corner.
fn gallery_scrim_gradient() -> BackgroundGradient {
    BackgroundGradient::from(RadialGradient::new(
        UiPosition::CENTER,
        RadialGradientShape::FarthestCorner,
        vec![
            ColorStop::new(GALLERY_SCRIM_INNER, Val::Percent(0.0)),
            ColorStop::new(GALLERY_SCRIM_OUTER, Val::Percent(100.0)),
        ],
    ))
}

/// Spawn the title door's scrim between the backdrop and the gallery surface.
/// It is an absolutely positioned full-window sibling spawned after
/// [`spawn_title_backdrop`] and before [`gallery_surface`], so it darkens the
/// backdrop only and never a card's art; the overlay's door draws none
/// (`docs/title-and-menu.md` § Gallery `### Layout` §7).
fn spawn_title_gallery_scrim(root: &mut ChildSpawnerCommands) {
    root.spawn((
        GalleryScrim,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        gallery_scrim_gradient(),
    ));
}
