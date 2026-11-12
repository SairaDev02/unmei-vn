//! The title surface: wordmark, rows, tagline, gradient and backdrop.

use super::assets::{background_asset_path, AssetWatch};
use super::overlay::confirm_dialog;
use super::style::{cover_size, TextFaces, BUTTON_BG, DIM, FAINT, FOCUS, PANEL_BG, TEXT};
use super::{focus_border, MenuRoot};
use crate::failure::FailureRow;
use crate::menu::{self, MenuAction, MenuState};
use crate::PresentationSet;
use crate::Runtime;
use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::ui::{BackgroundGradient, ColorStop, RadialGradient, RadialGradientShape, UiPosition};
use bevy::window::{PrimaryWindow, Window};

/// The title's pack-backdrop image node, sized to cover the window once its
/// texture is loaded (`docs/title-and-menu.md` § Title layout §2). Distinct from
/// [`BackgroundSprite`](super::play::BackgroundSprite), which belongs to the play
/// frame.
#[derive(Component)]
pub(crate) struct TitleBackdropImage;

/// The title's neutral gradient endpoints: the flat default backdrop the pack's
/// optional [`TitleBackdropImage`] art replaces (`docs/title-and-menu.md`
/// § Title layout §2).
const TITLE_GRADIENT_INNER: Color = Color::srgb(0.106, 0.133, 0.188);
const TITLE_GRADIENT_OUTER: Color = Color::srgb(0.043, 0.051, 0.071);

/// The warm tone Continue's reason line takes while it explains a disabled
/// Continue.
const TITLE_WARN: Color = Color::srgb(0.86, 0.63, 0.40);
/// The title panel's fixed width and its Settings panel's width.
const TITLE_PANEL_WIDTH: f32 = 520.0;
pub(super) const TITLE_SETTINGS_WIDTH: f32 = 720.0;
/// The tagline's ~0.34em tracking at its 11px size (`docs/title-and-menu.md`
/// § Title layout §7).
pub(crate) const TAGLINE_TRACKING_PX: f32 = 3.7;

/// Build the title: the neutral gradient, the optional pack backdrop, the
/// pack's wordmark, the five-row panel, and the foot row. Nothing of the play
/// path is drawn behind it.
pub(super) fn build_title(
    commands: &mut Commands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    asset_server: Option<&AssetServer>,
    watch: Option<&mut AssetWatch>,
) {
    let manifest = &runtime.runner.pack().manifest;
    let pack_name = manifest.name.clone();
    let tagline = manifest.tagline.clone();

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
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(28.0),
                ..default()
            },
            title_gradient(),
        ))
        .with_children(|root| {
            // The pack backdrop draws first, under everything else; the radial
            // gradient on the root stays visible beneath it.
            spawn_title_backdrop(
                root,
                manifest.title_backdrop.as_deref(),
                asset_server,
                watch,
            );

            // The wordmark block: the name, with the tagline directly under it
            // when the pack declares one. Wrapping both keeps the column's gap
            // above the panel identical whether or not a tagline is present
            // (`docs/title-and-menu.md` § Title layout §3/§8).
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.0),
                ..default()
            })
            .with_children(|wordmark| {
                wordmark.spawn((
                    Text::new(pack_name),
                    faces.sans_bold(46.0),
                    TextColor::from(TEXT),
                ));
                if let Some(tagline) = &tagline {
                    wordmark.spawn((
                        Text::new(tagline.to_uppercase()),
                        faces.mono(11.0),
                        LetterSpacing::Px(TAGLINE_TRACKING_PX),
                        TextColor::from(FOCUS),
                    ));
                }
            });

            // The five-row panel, reusing the overlay's fill, hairline border
            // and corner vocabulary.
            root.spawn((
                Node {
                    width: Val::Px(TITLE_PANEL_WIDTH),
                    max_width: Val::Percent(92.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(10.0)),
                    row_gap: Val::Px(2.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
                BorderColor::all(FAINT),
            ))
            .with_children(|panel| {
                title_row(
                    panel,
                    faces,
                    menu,
                    MenuAction::Continue,
                    "Continue",
                    menu.continue_enabled,
                    None,
                );
                title_row(
                    panel,
                    faces,
                    menu,
                    MenuAction::NewGame,
                    "New Game",
                    true,
                    None,
                );
                title_row(
                    panel,
                    faces,
                    menu,
                    MenuAction::OpenSettings,
                    "Settings",
                    true,
                    None,
                );
                // Gallery is an ordinary enabled, caption-free row
                // (`docs/title-and-menu.md` § Gallery `### Behavior` §1).
                title_row(
                    panel,
                    faces,
                    menu,
                    MenuAction::Gallery,
                    menu::GALLERY_HEADING,
                    true,
                    None,
                );
                // A hairline rule separates Quit from the other four.
                panel.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(1.0),
                        margin: UiRect::axes(Val::Px(0.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(FAINT),
                ));
                title_row(panel, faces, menu, MenuAction::Quit, "Quit", true, None);
            });

            // The foot row: Continue's reason line left, the legend right.
            root.spawn(Node {
                width: Val::Px(TITLE_PANEL_WIDTH),
                max_width: Val::Percent(92.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|foot| {
                foot.spawn((
                    Text::new(menu.continue_line.clone()),
                    faces.mono(12.0),
                    TextColor::from(if menu.continue_enabled {
                        DIM
                    } else {
                        TITLE_WARN
                    }),
                ));
                foot.spawn((
                    Text::new(menu::TITLE_LEGEND),
                    faces.mono(12.0),
                    TextColor::from(FAINT),
                ));
            });

            if let Some(confirm) = menu.confirm {
                confirm_dialog(root, faces, menu, confirm, None, runtime);
            }
        });
}

/// The title's neutral radial gradient — the default backdrop drawn when a pack
/// declares no `title_backdrop` (and the underlay beneath its art).
///
/// `TOP_LEFT` is deliberate: a `UiPosition`'s `x`/`y` are offsets from the
/// anchor, so `CENTER.at_percent(50, 28)` lands on the right edge. This resolves
/// to the node-centre-relative `(0, -0.22·H)` — the upper middle the spec asks
/// for (`docs/title-and-menu.md` § Title layout §2).
pub(super) fn title_gradient() -> BackgroundGradient {
    BackgroundGradient::from(RadialGradient::new(
        UiPosition::TOP_LEFT.at_percent(50.0, 28.0),
        RadialGradientShape::FarthestCorner,
        vec![
            ColorStop::new(TITLE_GRADIENT_INNER, Val::Percent(0.0)),
            ColorStop::new(TITLE_GRADIENT_OUTER, Val::Percent(100.0)),
        ],
    ))
}

/// Spawn the pack's title backdrop, resolved exactly like `bg`
/// (`assets/backgrounds/<id>.png`), as an absolutely positioned image node.
/// It is the root's first child, so it paints over the gradient underlay and
/// under the wordmark and panel; [`fit_title_backdrop`] sizes it to cover once
/// the texture has loaded. No asset server (a headless test world) or no
/// declared backdrop draws nothing and leaves the gradient standing.
pub(super) fn spawn_title_backdrop(
    root: &mut ChildSpawnerCommands,
    backdrop: Option<&str>,
    asset_server: Option<&AssetServer>,
    watch: Option<&mut AssetWatch>,
) {
    let Some(id) = backdrop else {
        return;
    };
    let (Some(asset_server), Some(watch)) = (asset_server, watch) else {
        return;
    };
    // Row 25: a missing or undecodable file is a silent degradation — the
    // gradient stands and the path warns once through the shared funnel.
    let Some(handle) = watch.watch_image(
        asset_server,
        FailureRow::TitleBackdrop,
        background_asset_path(id),
    ) else {
        return;
    };
    root.spawn((
        TitleBackdropImage,
        // The node's size is computed from the texture's aspect ratio; the
        // image fills exactly that node, so it is never distorted.
        ImageNode::new(handle).with_mode(NodeImageMode::Stretch),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
    ));
}

/// Size and centre the title backdrop art so it covers the window. Bevy 0.19's
/// UI image node has no cover mode (`NodeImageMode` is Auto/Stretch/Sliced/
/// Tiled), so the node is computed from the loaded texture's aspect ratio; the
/// window edges do the cropping, and the texture's aspect is preserved.
pub(crate) fn fit_title_backdrop(
    windows: Query<&Window, With<PrimaryWindow>>,
    images: Res<Assets<Image>>,
    mut nodes: Query<(&ImageNode, &mut Node), With<TitleBackdropImage>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let target = Vec2::new(window.width(), window.height());
    for (image_node, mut node) in &mut nodes {
        let Some(image) = images.get(&image_node.image) else {
            continue;
        };
        let cover = cover_size(image.size_f32(), target);
        node.width = Val::Px(cover.x);
        node.height = Val::Px(cover.y);
        node.left = Val::Px((target.x - cover.x) / 2.0);
        node.top = Val::Px((target.y - cover.y) / 2.0);
    }
}

/// One title row. Disabled rows stay walkable and focusable; their label and
/// optional second-line caption render in the faint tone. The label is the sans
/// root text and a present caption is a mono span on the same node
/// (`docs/title-and-menu.md` § Type system §4).
fn title_row(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    action: MenuAction,
    label: &str,
    enabled: bool,
    caption: Option<&str>,
) {
    let mut row = parent.spawn((
        action,
        Button,
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::axes(Val::Px(14.0), Val::Px(12.0)),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(if enabled { BUTTON_BG } else { PANEL_BG }),
        BorderColor::all(focus_border(menu, action)),
        Text::new(label.to_string()),
        faces.sans(16.0),
        TextColor::from(if enabled { TEXT } else { FAINT }),
    ));
    if let Some(caption) = caption {
        row.with_children(|row| {
            row.spawn((
                TextSpan::new(format!("\n{caption}")),
                faces.mono(16.0),
                TextColor::from(FAINT),
            ));
        });
    }
}

/// Register the title backdrop's fit. It runs in `Update` after the menu-tree
/// chain's focus repaint and before the gallery's art syncs, the position it
/// has always held.
pub(super) fn register(app: &mut App) {
    app.add_systems(
        Update,
        fit_title_backdrop
            .after(super::sync_menu_focus)
            .in_set(PresentationSet),
    );
}
