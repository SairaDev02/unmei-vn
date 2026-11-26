//! The title surface drawn from the plugin's `title` tree: the wordmark, the
//! tagline, the five rows, Continue's reason line, the legend and the veil
//! (Default look §3, spec §11).
//!
//! The tree decides the look; this module decides what it says and what the
//! format cannot. The app supplies the pack's name, tagline and reason line, the
//! rows as five items of the one template in `Title` §2's order and the pack's
//! art in the `title.backdrop` slot, loaded through the shared watch, so a
//! missing or undecodable file is row 25's silent degradation. The look is the
//! tree's alone: the neutral gradient under the art, the veil and the tagline's
//! tracking are the reference plugin's, so another plugin replaces them (spec
//! §11, rules 41 and 42). Behaviour — the walk, the confirms, Continue's
//! precedence — stays the menu's.
//!
//! A door that still builds its own root borrows [`spawn_title_backdrop`],
//! [`title_gradient`] and [`veil_gradient`], so Settings from the title opens
//! over the same backdrop and veil.

use super::assets::{background_asset_path, AssetWatch};
use super::overlay::confirm_dialog;
use super::render::tree::{Bind, ControlState, Surface, Words};
use super::render::UiLook;
use super::style::{cover_size, TextFaces};
use super::MenuRoot;
use crate::failure::FailureRow;
use crate::menu::{MenuAction, MenuState};
use crate::PresentationSet;
use crate::Runtime;
use bevy::prelude::*;
use bevy::ui::{
    BackgroundGradient, ColorStop, Gradient, LinearGradient, RadialGradient, RadialGradientShape,
    UiPosition,
};
use bevy::window::{PrimaryWindow, Window};

/// The surface's id, as the plugin folder names it.
const SURFACE: &str = "title";

/// The five rows in `Title` §2's order: the copy key its label draws and the
/// control id the row is.
const ROWS: [(&str, &str); 5] = [
    ("title.row.continue", "title.continue"),
    ("title.row.new-game", "title.new-game"),
    ("title.row.settings", "title.settings"),
    ("title.row.gallery", "title.gallery"),
    ("title.row.quit", "title.quit"),
];

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

/// The veil's three stops, from Default look §3: dark at the left edge, through
/// 38%, clear by 62%.
const VEIL_STOPS: [(f32, [f32; 4]); 3] = [
    (0.0, [8.0 / 255.0, 9.0 / 255.0, 13.0 / 255.0, 0.86]),
    (0.38, [8.0 / 255.0, 9.0 / 255.0, 13.0 / 255.0, 0.55]),
    (0.62, [8.0 / 255.0, 9.0 / 255.0, 13.0 / 255.0, 0.0]),
];

/// The title door's Settings panel width: the panel stays hand-built and the body
/// inside it draws the plugin's Settings tree (#117).
pub(super) const TITLE_SETTINGS_WIDTH: f32 = 720.0;

/// Build the title from the plugin's tree: the pack's backdrop under the veil,
/// the wordmark, the tagline, the five rows and the reason line in the left
/// column, and the legend at the frame's foot. Nothing of the play path is drawn
/// behind it.
pub(super) fn build_title(
    commands: &mut Commands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    asset_server: Option<&AssetServer>,
    watch: Option<&mut AssetWatch>,
    look: Option<&UiLook>,
) {
    let Some(look) = look else {
        return;
    };
    let manifest = &runtime.runner.pack().manifest;
    let bind = Title {
        name: manifest.name.clone(),
        tagline: manifest.tagline.clone(),
        reason: menu.continue_line.clone(),
    };
    let bedrock = manifest.title_backdrop.clone();
    let surface = Surface {
        look,
        faces,
        bind: &bind,
    };
    commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ))
        .with_children(|root| {
            let Some(bound) = surface.spawn(SURFACE, root) else {
                return;
            };
            let (Some(list), Some(item)) = (
                bound.lists.get("title.rows").copied(),
                bound.items.get("title.rows").cloned(),
            ) else {
                return;
            };
            root.commands().entity(list).with_children(|rows| {
                for (label, control) in ROWS {
                    let enabled = control != "title.continue" || menu.continue_enabled;
                    let bind = Row {
                        label,
                        control,
                        action: row_action(control),
                        enabled,
                    };
                    surface.spawn_item(&item, rows, &bind, Some(control));
                }
            });
            if let (Some(slot), Some(asset_server)) =
                (bound.art.get("title.backdrop").copied(), asset_server)
            {
                root.commands().entity(slot).with_children(|parent| {
                    spawn_title_backdrop(parent, bedrock.as_deref(), Some(asset_server), watch);
                });
            }
            if let Some(confirm) = menu.confirm {
                confirm_dialog(root, Some(look), faces, confirm, None, runtime);
            }
        });
}

/// The action one row's control carries.
fn row_action(control: &str) -> MenuAction {
    match control {
        "title.continue" => MenuAction::Continue,
        "title.new-game" => MenuAction::NewGame,
        "title.settings" => MenuAction::OpenSettings,
        "title.gallery" => MenuAction::Gallery,
        _ => MenuAction::Quit,
    }
}

/// The title's own answer: the pack's name, tagline and Continue's reason line.
struct Title {
    name: String,
    tagline: Option<String>,
    reason: String,
}

impl Bind for Title {
    fn words(&self, name: &str) -> Option<Words> {
        match name {
            "pack.name" => Some(Words::text(self.name.clone())),
            "title.tagline" => self
                .tagline
                .as_ref()
                .map(|tagline| Words::text(tagline.to_uppercase())),
            "title.continue-reason" => Some(Words::text(self.reason.clone())),
            _ => None,
        }
    }

    fn shows(&self, name: &str) -> bool {
        name != "title.tagline" || self.tagline.is_some()
    }
}

/// One row of the list: its label, the control it is, and whether it is enabled.
/// The label sits beside the control, so the row's own state answers for the
/// label's tone too — every node in the item follows the item's control.
struct Row {
    label: &'static str,
    control: &'static str,
    action: MenuAction,
    enabled: bool,
}

impl Bind for Row {
    fn words(&self, name: &str) -> Option<Words> {
        (name == "title.row.label" || name == self.control).then(|| Words::new(self.label))
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        (name == self.control).then_some(self.action)
    }

    fn state(&self, _name: &str) -> ControlState {
        ControlState {
            disabled: !self.enabled,
            ..default()
        }
    }
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

/// Default look §3's veil, left to right at 0°: the same stops the reference
/// plugin's `veil` component authors, for a door whose root is still built by
/// hand. The format's 0° runs left to right and Bevy's runs bottom to top,
/// clockwise, so 0° is a quarter turn.
pub(super) fn veil_gradient() -> BackgroundGradient {
    BackgroundGradient(vec![Gradient::Linear(LinearGradient {
        angle: std::f32::consts::FRAC_PI_2,
        stops: VEIL_STOPS
            .iter()
            .map(|(at, [r, g, b, a])| {
                ColorStop::new(Color::srgba(*r, *g, *b, *a), Val::Percent(at * 100.0))
            })
            .collect(),
        ..default()
    })])
}

/// Spawn the pack's title backdrop, resolved exactly like `bg`
/// (`assets/backgrounds/<id>.png`), as an absolutely positioned image node.
/// It paints over the gradient underlay and under the veil;
/// [`fit_title_backdrop`] sizes it to cover once the texture has loaded. No
/// asset server (a headless test world) or no declared backdrop draws nothing
/// and leaves the gradient standing.
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
