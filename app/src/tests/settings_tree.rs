//! Ticket #117: the Settings view drawn from the plugin's `settings` surface.
//!
//! These pin the frame, the slots and the controls to the reference plugin's
//! tree: the body is that tree's root node, every control the surface requires is
//! one of its slots carrying its `MenuAction`, the contents page hides the tree's
//! back control and collapses the row around it while the title's door keeps its
//! own, and an edit to a pack's own tree changes what the view draws.

use super::*;

use std::path::Path;

use unmei_engine::ui::{Node as PluginNode, NodeKind, Padding};

use crate::presentation::render::{UiLook, UiStates};
use crate::presentation::style::TextFaces;
use crate::settings::AudioControl;

/// The thirteen controls the `settings` surface requires, with the action each
/// one carries. A Keiko control id is the surface's name for a control, so this
/// table is the contract between the tree and the menu's walk.
const CONTROLS: [(&str, MenuAction); 13] = [
    ("settings.back", MenuAction::BackToSlots),
    (
        "settings.master",
        MenuAction::AudioSlider(AudioControl::Master),
    ),
    (
        "settings.music",
        MenuAction::AudioSlider(AudioControl::Music),
    ),
    ("settings.sfx", MenuAction::AudioSlider(AudioControl::Sfx)),
    ("settings.mute", MenuAction::AudioMute),
    ("settings.reset-audio", MenuAction::AudioReset),
    ("settings.text-speed", MenuAction::TextSpeed),
    ("settings.auto", MenuAction::PlayAuto),
    ("settings.fullscreen", MenuAction::PlayFullscreen),
    ("settings.reset-play", MenuAction::PlayReset),
    ("settings.key", MenuAction::KeyRow),
    ("settings.copy-path", MenuAction::CopyKeyPath),
    ("settings.clear-key", MenuAction::ClearKey),
];

fn children_of(app: &App, entity: Entity) -> Vec<Entity> {
    app.world()
        .entity(entity)
        .get::<Children>()
        .map(|children| children.iter().collect())
        .unwrap_or_default()
}

fn parent_of(app: &App, entity: Entity) -> Entity {
    app.world()
        .entity(entity)
        .get::<ChildOf>()
        .map(ChildOf::parent)
        .expect("the entity has a parent")
}

fn is_within(app: &App, entity: Entity, root: Entity) -> bool {
    let mut current = Some(entity);
    while let Some(node) = current {
        if node == root {
            return true;
        }
        current = app.world().get::<ChildOf>(node).map(ChildOf::parent);
    }
    false
}

fn display(app: &App, entity: Entity) -> Display {
    app.world()
        .get::<Node>(entity)
        .expect("the node is laid out")
        .display
}

/// The Settings body the contents page or the title door mounted.
fn settings_body(app: &App) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::settings_view::SettingsBody>()
                .map(|_| entity.id())
        })
        .expect("the Settings body draws")
}

/// The one control that carries `action`.
fn control(app: &App, action: MenuAction) -> Entity {
    action_entity(app, action)
}

/// Every slot name the tree places, in spawn order.
fn slots_of(node: &PluginNode, out: &mut Vec<String>) {
    match &node.kind {
        NodeKind::Box(container) => {
            for child in &container.children {
                slots_of(child, out);
            }
        }
        NodeKind::Slot(slot) => {
            out.push(slot.slot.clone());
            for child in &slot.children {
                slots_of(child, out);
            }
        }
        NodeKind::List(list) => slots_of(&list.item, out),
        _ => {}
    }
}

/// The plugin's `settings` tree, as the look in force resolves it.
fn settings_tree(app: &App) -> PluginNode {
    app.world()
        .resource::<UiLook>()
        .surface("settings")
        .expect("the plugin draws the settings surface")
        .clone()
}

/// Open the contents page on its Settings section and build it, then run the
/// frame that follows the build: the tree's visibility pass is one of that
/// frame's systems, exactly as it is in the player.
fn settings_section() -> (App, tempfile::TempDir) {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);
    build_pause_tree(&mut app);
    app.add_systems(
        Update,
        crate::presentation::settings_view::sync_settings_visibility,
    );
    app.update();
    (app, pack)
}

#[test]
fn the_settings_section_draws_the_plugin_s_tree() {
    let (app, _pack) = settings_section();
    let body = settings_body(&app);
    let held = children_of(&app, body);
    assert_eq!(
        held.len(),
        1,
        "the body holds the surface's tree root and nothing else"
    );
    let root = held[0];

    let tree = settings_tree(&app);
    let NodeKind::Box(frame) = &tree.kind else {
        panic!("a tree's root is a box");
    };
    let drawn = children_of(&app, root);
    assert_eq!(
        drawn.len(),
        frame.children.len(),
        "the body's frame is the tree's root node, child for child"
    );

    let laid_out = app.world().get::<Node>(root).expect("the root is laid out");
    assert_eq!(
        laid_out.flex_direction,
        FlexDirection::Column,
        "the tree's column"
    );
    assert_eq!(laid_out.row_gap, Val::Px(frame.gap), "the tree's own gap");
    let Padding::Uniform(padding) = frame.padding else {
        panic!("the body's frame states one padding");
    };
    assert_eq!(
        laid_out.padding,
        UiRect::ZERO,
        "the seat pads the column, so the tree's own page padding ({padding}) is cleared"
    );

    let look = app.world().resource::<UiLook>();
    for (index, authored) in frame.children.iter().enumerate() {
        let NodeKind::Text(text) = &authored.kind else {
            continue;
        };
        let words = app
            .world()
            .get::<Text>(drawn[index])
            .map(|text| text.0.clone());
        assert_eq!(
            words.as_deref(),
            Some(look.copy(&text.text)),
            "the tree's `{}` node draws the plugin's copy",
            text.text
        );
    }

    let heading = text_entity(&app, "Settings");
    assert!(
        is_within(&app, heading, root),
        "the heading is the tree's own slot"
    );
    assert!(
        !held.contains(&heading),
        "no hand-built heading stands beside the tree"
    );
    for group in ["Audio", "Play", "Decisions"] {
        assert!(
            is_within(&app, text_entity(&app, group), root),
            "the {group} label is the tree's text node"
        );
    }
}

#[test]
fn every_settings_control_is_a_tree_slot_that_carries_its_action() {
    let (app, _pack) = settings_section();
    let root = children_of(&app, settings_body(&app))[0];
    let mut placed = Vec::new();
    slots_of(&settings_tree(&app), &mut placed);

    for (id, action) in CONTROLS {
        assert!(
            placed.iter().any(|slot| slot == id),
            "the tree places `{id}`"
        );
        let found: Vec<Entity> = app
            .world()
            .iter_entities()
            .filter(|entity| {
                entity.get::<MenuAction>() == Some(&action) && entity.get::<Button>().is_some()
            })
            .map(|entity| entity.id())
            .collect();
        assert!(!found.is_empty(), "`{id}` is spawned");
        for entity in &found {
            assert!(
                is_within(&app, *entity, root),
                "`{id}` is one of the tree's nodes"
            );
            assert!(
                app.world().get::<UiStates>(*entity).is_some(),
                "`{id}` is a rendered control, painted by the tree's states"
            );
        }
        let shown = found
            .iter()
            .filter(|entity| display(&app, **entity) != Display::None)
            .count();
        let expected = usize::from(action != MenuAction::BackToSlots);
        assert_eq!(
            shown, expected,
            "the contents page draws `{id}` {expected} time(s)"
        );
    }
}

#[test]
fn the_back_control_hides_on_the_contents_page_and_the_title_door_keeps_its_own() {
    let (app, _pack) = settings_section();
    assert_eq!(menu(&app).settings_origin, menu::MenuOrigin::Slots);
    let back = control(&app, MenuAction::BackToSlots);
    assert_eq!(
        display(&app, back),
        Display::None,
        "the contents page moves the way out to its own left column"
    );
    let button = parent_of(&app, back);
    assert_eq!(
        display(&app, button),
        Display::None,
        "the tree's button box collapses with the control"
    );
    assert_eq!(
        display(&app, parent_of(&app, button)),
        Display::None,
        "and the row that holds the box"
    );

    let mut titled = temp_title_app();
    titled.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut titled, KeyCode::Enter);
    build_pause_tree(&mut titled);
    let back = control(&titled, MenuAction::BackToSlots);
    assert_ne!(
        display(&titled, back),
        Display::None,
        "the title's door draws its own back control"
    );
    assert_eq!(
        flattened_text(&titled, back),
        unmei_engine::copy::text("settings.back.title"),
        "with the door's own label"
    );
}

/// The reference plugin's files, written into a temp pack's `ui/` folder, with
/// `settings.ron`'s heading face replaced so the tree the view draws differs from
/// the embedded one in one visible way.
fn write_settings_plugin(pack: &Path) {
    let ui = pack.join("ui");
    for &(path, text) in crate::reference_plugin::FILES {
        let target = ui.join(path);
        std::fs::create_dir_all(target.parent().expect("a plugin file has a folder")).unwrap();
        let text = if path == "surfaces/settings.ron" {
            let edited = text.replace(
                "typography: \"{typography.heading}\"",
                "typography: \"{typography.group}\"",
            );
            assert_ne!(edited, text, "the fixture edits the heading's face");
            edited
        } else {
            text.to_string()
        };
        std::fs::write(target, text).unwrap();
    }
}

#[test]
fn an_edited_settings_tree_in_the_pack_changes_what_the_view_draws() {
    let (reference, _reference_pack) = settings_section();
    let faces = reference.world().resource::<TextFaces>().clone();
    let reference_heading = text_entity(&reference, "Settings");
    assert_eq!(
        face_handle(&reference, reference_heading),
        faces.sans_semibold,
        "the reference plugin's heading is the SemiBold face"
    );

    let dir = temp_pack();
    let pack = dir.path().join("pack");
    write_settings_plugin(&pack);
    let mut app = app_for_pack(pack);
    app.insert_resource(TempPack(dir));
    open_settings(&mut app);
    build_pause_tree(&mut app);

    let heading = text_entity(&app, "Settings");
    assert_eq!(
        face_handle(&app, heading),
        faces.mono_medium,
        "the pack's edited tree draws the heading in the face its own file names"
    );
    assert_eq!(
        app.world().get::<TextFont>(heading).unwrap().font_size,
        FontSize::Px(18.0),
        "at the token's own size, not the embedded tree's"
    );
    assert_ne!(
        face_handle(&app, heading),
        face_handle(&reference, reference_heading),
        "the view follows the pack's tree, not a hard-coded copy of the reference one"
    );
}
