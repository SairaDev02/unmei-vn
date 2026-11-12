//! The title door's gallery: the pack backdrop, the gallery-local scrim that
//! darkens it, and the shared surface over them.

use bevy::prelude::*;
use bevy::ui::{BackgroundGradient, ColorStop, RadialGradient, RadialGradientShape, UiPosition};

use super::super::assets::AssetWatch;
use super::super::style::TextFaces;
use super::super::title::{spawn_title_backdrop, title_gradient};
use super::super::MenuRoot;
use super::grid::gallery_surface;
use super::markers::GalleryScrim;
use crate::menu::MenuState;
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
pub(in crate::presentation) fn build_title_gallery(
    commands: &mut Commands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    asset_server: Option<&AssetServer>,
    mut watch: Option<&mut AssetWatch>,
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
            gallery_surface(root, faces, menu, runtime, asset_server, watch);
        });
}

/// The gallery's title-door scrim gradient: the prototype's radial from
/// [`GALLERY_SCRIM_INNER`] at the middle to [`GALLERY_SCRIM_OUTER`] at the
/// edges (`docs/title-and-menu.md` § Gallery `### Layout` §4/§7).
///
/// `UiPosition::CENTER` is the node's middle: a `UiPosition`'s `x`/`y` are
/// offsets from its anchor, so `CENTER.at_percent(50, 50)` would sit on the
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
