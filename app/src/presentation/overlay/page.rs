//! The contents page drawn from the plugin's `contents` surface: the section
//! column with Resume and Exit to title at its foot, the right column's heading,
//! and the Save and Load ledger (Default look §4, spec §11).
//!
//! The tree decides the look; this module decides what it says. It binds the
//! pack name, the section the page is on and the scene the player is in, spawns
//! one item per section entry and per ledger row, and mounts the section's own
//! view where the tree put the ledger when the page is not on Save or Load. The
//! tree's ledger list is the right column's body, so a sub-view fills the height
//! the heading leaves and scrolls inside it.
//!
//! Focus, keys, walks and confirms stay the input code's, keyed by control id
//! (spec §11): every control here carries the same [`MenuAction`] its Keiko id
//! names, so a click and a key reach one dispatch.

use bevy::prelude::*;
use bevy::ui::ScrollPosition;
use bevy::ui_widgets::ScrollArea;
use unmei_engine::copy;

use super::super::assets::AssetWatch;
use super::super::gallery::gallery_surface;
use super::super::render::{Bind, ControlState, Surface, UiLook, Words};
use super::super::settings_view::{settings_body, SettingsBody};
use super::super::style::TextFaces;
use super::super::timeline::timeline_surface;
use super::super::MenuRoot;
use super::confirm::confirm_dialog;
use super::dialogs::legacy_picker;
use crate::credentials::CredentialsState;
use crate::menu::{self, MenuAction, MenuMode, MenuState, MenuView};
use crate::settings::SettingsState;
use crate::Runtime;

/// The surface's id, as the plugin folder names it.
const SURFACE: &str = "contents";

/// Default look §4's five section entries, top to bottom, with the control each
/// carries and the copy key its label draws.
const SECTIONS: [(&str, MenuAction, &str); 5] = [
    (
        "overlay.save-tab",
        MenuAction::SetMode(MenuMode::Save),
        "contents.tab.save",
    ),
    (
        "overlay.load-tab",
        MenuAction::SetMode(MenuMode::Load),
        "contents.tab.load",
    ),
    (
        "overlay.timeline",
        MenuAction::OpenTimeline,
        "contents.tab.timeline",
    ),
    (
        "overlay.gallery",
        MenuAction::OpenGallery,
        "contents.tab.gallery",
    ),
    (
        "overlay.settings",
        MenuAction::OpenSettings,
        "contents.tab.settings",
    ),
];

/// The action every control of the page carries, by its slot name: the one map
/// the two lists' items and the rail's foot share, so a hidden sibling control
/// still knows it is a control and the row's own is the one the engine shows.
fn page_action(name: &str) -> Option<MenuAction> {
    for (control, action, _) in SECTIONS {
        if name == control {
            return Some(action);
        }
    }
    match name {
        "overlay.resume" => Some(MenuAction::Resume),
        "overlay.exit-to-title" => Some(MenuAction::ExitToTitle),
        "overlay.slot.1" => Some(MenuAction::Card(0)),
        "overlay.slot.2" => Some(MenuAction::Card(1)),
        "overlay.slot.3" => Some(MenuAction::Card(2)),
        "overlay.slot.autosave" => Some(MenuAction::Card(3)),
        "overlay.legacy" => Some(MenuAction::LegacyImport),
        _ => None,
    }
}

/// The section the page is showing, as the control that names it.
fn current_section(menu: &MenuState) -> Option<&'static str> {
    match menu.view {
        MenuView::Slots => Some(match menu.mode {
            MenuMode::Save => "overlay.save-tab",
            MenuMode::Load => "overlay.load-tab",
        }),
        MenuView::Timeline => Some("overlay.timeline"),
        MenuView::Gallery => Some("overlay.gallery"),
        MenuView::Settings => Some("overlay.settings"),
        MenuView::Title => None,
    }
}

/// What one list item draws: the words its own slots show, and the state its own
/// control is in. Every node inside the item follows that state (spec §4).
struct Item {
    words: Vec<(&'static str, Words)>,
    state: ControlState,
}

impl Item {
    /// A section entry: its label, and `current` when its section is the open
    /// one, which turns the label white and shows §4's `·`.
    fn section(index: usize, menu: &MenuState) -> Self {
        let (control, _, key) = SECTIONS[index];
        Item {
            words: vec![
                ("contents.section.label", Words::new(key)),
                (control, Words::new(key)),
            ],
            state: ControlState {
                current: current_section(menu) == Some(control),
                ..default()
            },
        }
    }

    /// A ledger row: the slot's label, the scene or status over the saved-at
    /// line, and the contextual action, muted when the row cannot be activated
    /// ([`docs/runtime-hardening.md` § Menu overlay §2/§3]).
    ///
    /// [`docs/runtime-hardening.md` § Menu overlay §2/§3]: ../../../docs/runtime-hardening.md
    fn row(model: &menu::CardModel, control: &'static str) -> Self {
        Item {
            words: vec![
                (
                    "contents.ledger.label",
                    Words::text(model.label.to_string()),
                ),
                ("contents.ledger.detail", Words::text(model.detail.clone())),
                (
                    "contents.ledger.saved-at",
                    Words::text(model.saved_at.clone()),
                ),
                (
                    "contents.ledger.action",
                    Words::text(model.action.label.to_string()),
                ),
                (control, Words::text(model.label.to_string())),
            ],
            state: ControlState {
                disabled: model.action.action.is_none(),
                ..default()
            },
        }
    }

    /// The one-time legacy entry, as the ledger's final row in save mode: its
    /// own copy key already names what activating it does, so it draws no
    /// detail, saved-at or action.
    fn legacy() -> Self {
        Item {
            words: vec![
                (
                    "contents.ledger.label",
                    Words::new("contents.card.legacy-import"),
                ),
                ("overlay.legacy", Words::new("contents.card.legacy-import")),
            ],
            state: ControlState::default(),
        }
    }
}

impl Bind for Item {
    fn words(&self, name: &str) -> Option<Words> {
        self.words
            .iter()
            .find(|(slot, _)| *slot == name)
            .map(|(_, words)| words.clone())
    }

    fn shows(&self, name: &str) -> bool {
        self.words(name).is_some() || !name.starts_with("contents.ledger.")
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        page_action(name)
    }

    fn state(&self, _name: &str) -> ControlState {
        self.state
    }
}

/// What the page itself draws outside its list items: the heading, and the two
/// controls at the column's foot.
struct Page<'a> {
    menu: &'a MenuState,
    pack_name: &'a str,
    scene: &'a str,
}

impl Bind for Page<'_> {
    fn words(&self, name: &str) -> Option<Words> {
        match name {
            "pack.name" => Some(Words::text(self.pack_name.to_string())),
            "contents.scene" => Some(Words::with(
                match self.menu.mode {
                    MenuMode::Save => "contents.heading.save",
                    MenuMode::Load => "contents.heading.load",
                },
                &[("scene", self.scene)],
            )),
            "overlay.resume" => Some(Words::new("contents.rail.resume")),
            "overlay.exit-to-title" => Some(Words::new("contents.rail.exit-to-title")),
            _ => None,
        }
    }

    fn shows(&self, name: &str) -> bool {
        match name {
            "overlay.exit-to-title" => menu::exit_to_title_shown(self.menu),
            _ => true,
        }
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        page_action(name)
    }

    fn state(&self, name: &str) -> ControlState {
        ControlState {
            current: current_section(self.menu) == Some(name),
            ..default()
        }
    }
}

/// Build the contents page: one full-frame root holding the surface tree, the
/// section and ledger items, the section's own view and any open confirm or
/// picker. The root carries [`MenuRoot`], so the coordinator's one rebuild
/// clears all of it.
///
/// The list the tree puts the ledger in is the right column's body and its scroll
/// host, so the player lets it shrink below its content: flexbox's default
/// `min-height: auto` would grow the list past the frame instead of scrolling,
/// and the format has no property for it. Without a look there is no tree and so
/// no page; the reference plugin is embedded, so that is a no-assets rig's path
/// only.
#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn build_page(
    commands: &mut Commands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    settings: Option<&SettingsState>,
    credentials: Option<&CredentialsState>,
    asset_server: Option<&AssetServer>,
    watch: Option<&mut AssetWatch>,
    read_log: Option<&crate::read_log::ReadLogState>,
    look: Option<&UiLook>,
    timeline_scroll: Option<Vec2>,
) {
    let Some(look) = look else {
        return;
    };
    let pack_name = runtime.runner.pack().manifest.name.clone();
    let scene = runtime.runner.scene_id().to_string();
    let page = Page {
        menu,
        pack_name: &pack_name,
        scene: &scene,
    };
    let surface = Surface {
        look,
        faces,
        bind: &page,
    };
    let ledger = if menu.view == MenuView::Slots {
        menu::card_models(menu)
    } else {
        Vec::new()
    };
    let legacy = menu::legacy_visible(menu);

    let mut bound = None;
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
            let Some(rendered) = surface.spawn(SURFACE, root) else {
                return;
            };
            if let Some(sections) = rendered.lists.get("contents.sections") {
                let items = rendered.items.get("contents.sections").cloned();
                root.commands().entity(*sections).with_children(|list| {
                    let Some(item) = items else {
                        return;
                    };
                    for (index, (control, _, _)) in SECTIONS.into_iter().enumerate() {
                        surface.spawn_item(&item, list, &Item::section(index, menu), Some(control));
                    }
                });
            }
            if let (Some(ledger_list), Some(item)) = (
                rendered.lists.get("contents.ledger").copied(),
                rendered.items.get("contents.ledger").cloned(),
            ) {
                let models = ledger.clone();
                root.commands().entity(ledger_list).with_children(|list| {
                    for model in &models {
                        let control = ledger_control(model.slot);
                        surface.spawn_item(&item, list, &Item::row(model, control), Some(control));
                    }
                    if legacy {
                        surface.spawn_item(&item, list, &Item::legacy(), Some("overlay.legacy"));
                    }
                });
            }
            bound = Some(rendered);
        });

    let Some(bound) = bound else {
        return;
    };
    if let Some(body) = bound.lists.get("contents.ledger").copied() {
        commands
            .entity(body)
            .entry::<Node>()
            .and_modify(|mut node| {
                node.min_height = Val::Px(0.0);
            });
        commands.entity(body).with_children(|parent| {
            section_body(
                parent,
                faces,
                menu,
                runtime,
                settings,
                credentials,
                asset_server,
                watch,
                read_log,
                timeline_scroll,
            );
        });
    }
    if menu.confirm.is_some() || menu.picker.is_some() {
        if let Some(root) = bound.root {
            commands.entity(root).with_children(|parent| {
                if let Some(confirm) = menu.confirm {
                    confirm_dialog(parent, Some(look), faces, confirm, credentials, runtime);
                }
                if menu.picker.is_some() {
                    legacy_picker(parent, faces, menu);
                }
            });
        }
    }
}

/// The Keiko control id one ledger row carries: the slot's own, in `Slot::ALL`
/// order.
fn ledger_control(slot: unmei_engine::save::Slot) -> &'static str {
    match slot {
        unmei_engine::save::Slot::Manual(1) => "overlay.slot.1",
        unmei_engine::save::Slot::Manual(2) => "overlay.slot.2",
        unmei_engine::save::Slot::Manual(3) => "overlay.slot.3",
        unmei_engine::save::Slot::Autosave => "overlay.slot.autosave",
        unmei_engine::save::Slot::Manual(_) => "overlay.slot.1",
    }
}

/// The right column's body for the open section: §4's ledger rows fill it on Save
/// and Load, and the other three sections render their own view in the same
/// place, each inside the height the heading leaves (`docs/title-and-menu.md`
/// § Default look §4 — their interiors are their own until they are drawn from
/// trees).
#[allow(clippy::too_many_arguments)]
fn section_body(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    settings: Option<&SettingsState>,
    credentials: Option<&CredentialsState>,
    asset_server: Option<&AssetServer>,
    watch: Option<&mut AssetWatch>,
    read_log: Option<&crate::read_log::ReadLogState>,
    timeline_scroll: Option<Vec2>,
) {
    match menu.view {
        MenuView::Gallery => {
            gallery_surface(parent, faces, menu, runtime, asset_server, watch);
        }
        MenuView::Timeline => {
            timeline_surface(parent, faces, menu, runtime, read_log, timeline_scroll);
        }
        MenuView::Settings => {
            parent
                .spawn((
                    SettingsBody,
                    ScrollArea,
                    ScrollPosition::default(),
                    Node {
                        width: Val::Percent(100.0),
                        flex_grow: 1.0,
                        min_height: Val::Px(0.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(16.0),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                ))
                .with_children(|body| {
                    settings_body(
                        body,
                        faces,
                        menu,
                        settings,
                        credentials,
                        copy::text("settings.back.slots"),
                    );
                });
        }
        MenuView::Slots | MenuView::Title => {}
    }
}
