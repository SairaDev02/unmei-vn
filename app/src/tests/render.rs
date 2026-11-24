//! The plugin renderer: the look a boot chooses, the nodes one surface's tree
//! becomes, and the facts a drawn confirm reports.
//!
//! The tests spawn a real boot over a temp pack and read the components a drawn
//! surface carries — a node's size and padding, a text's face and tone — never a
//! private renderer type or a node's name.

use std::path::Path;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::text::FontSource;
use unmei_engine::save::Slot;
use unmei_engine::ui;

use super::support::*;
use crate::menu::{self, MenuAction, MenuFocus, MenuState};
use crate::presentation::render::UiLook;
use crate::presentation::style::TextFaces;

#[test]
fn the_look_is_chosen_at_boot_and_holds_the_reference_confirm() {
    let dir = temp_pack();
    let app = app_for_pack(dir.path().join("pack"));
    let look = app
        .world()
        .get_resource::<UiLook>()
        .expect("a boot with a good pack chooses a look");
    assert!(look.surface("confirm").is_some());
}

/// The confirm keeps the overlay's own dialog look (`docs/title-and-menu.md`
/// § Timeline § Layout, the confirm row), and every kind draws from the
/// reference plugin's one tree.
#[test]
fn every_confirm_kind_is_drawn_from_the_reference_plugin() {
    let dir = temp_pack();
    let mut app = app_for_pack(dir.path().join("pack"));
    esc(&mut app);

    let kinds = [
        (
            menu::ConfirmKind::Overwrite(Slot::Manual(1)),
            "Overwrite".to_string(),
            format!("Overwrite {}? Its current save is replaced.", slot(1)),
            "Yes",
        ),
        (
            menu::ConfirmKind::Load(Slot::Manual(1)),
            "Load".to_string(),
            format!("Load {}? Unsaved progress is lost and any decision in flight is cancelled.", slot(1)),
            "Yes",
        ),
        (
            menu::ConfirmKind::ResetAudio,
            "Reset audio".to_string(),
            "Reset Master, Music and SFX to 100% and turn mute off? The file is not deleted."
                .to_string(),
            "Yes",
        ),
        (
            menu::ConfirmKind::ResetPlay,
            "Reset play".to_string(),
            "Reset text speed to Instant, turn auto off and return to windowed? The audio mix is untouched and the file is not deleted.".to_string(),
            "Yes",
        ),
        (
            menu::ConfirmKind::NewGame,
            "Start a new game?".to_string(),
            "The session in progress is discarded. Your saved slots and the autosave are untouched."
                .to_string(),
            "Start",
        ),
        (
            menu::ConfirmKind::Quit,
            "Quit?".to_string(),
            "Unsaved progress is lost and any decision in flight is cancelled.".to_string(),
            "Quit",
        ),
        (
            menu::ConfirmKind::ExitToTitle,
            "Exit to the title?".to_string(),
            "Your session is kept \u{2014} Continue resumes it where you left off.".to_string(),
            "Exit",
        ),
        (
            menu::ConfirmKind::ClearKey,
            "Clear key?".to_string(),
            String::new(),
            "Yes",
        ),
        (
            menu::ConfirmKind::TimelineJump(0),
            "Jump to ".to_string(),
            "Unsaved progress is lost and any decision in flight is cancelled. The scene restarts from the beginning. Current game state is retained.".to_string(),
            "Jump",
        ),
    ];

    for (kind, question, detail, accept) in kinds {
        app.world_mut().resource_mut::<MenuState>().confirm = Some(kind);
        build_pause_tree(&mut app);
        let texts = all_texts(&app);
        let asks = if question.ends_with('?') {
            texts.contains(&question)
        } else {
            // The Timeline jump names a scene the demo pack's own order would
            // supply; the prefix and the consequence below are what this tree
            // decides.
            texts.iter().any(|text| text.starts_with(&question))
        };
        assert!(asks, "{kind:?} draws its question: {texts:?}");
        if !detail.is_empty() {
            assert!(
                texts.contains(&detail),
                "{kind:?} draws its consequence: {texts:?}"
            );
        } else {
            assert!(
                texts
                    .iter()
                    .any(|text| text.starts_with("The stored key is deleted from ")),
                "{kind:?} names the key file: {texts:?}"
            );
        }
        assert!(
            texts.iter().any(|text| text == accept),
            "{kind:?} draws its accept label: {texts:?}"
        );
        assert!(
            texts.iter().any(|text| text == "Cancel"),
            "{kind:?} draws its cancel label: {texts:?}"
        );
        assert_eq!(
            app.world()
                .iter_entities()
                .filter(|entity| {
                    matches!(
                        entity.get::<MenuAction>(),
                        Some(MenuAction::ConfirmAccept | MenuAction::ConfirmCancel)
                    )
                })
                .count(),
            2,
            "{kind:?} places both controls once"
        );
    }
}

/// The dialog's geometry and faces, read from the nodes the tree drew.
#[test]
fn the_confirm_panel_keeps_the_overlay_dialog_geometry_and_faces() {
    let dir = temp_pack();
    let mut app = app_for_pack(dir.path().join("pack"));
    esc(&mut app);
    app.world_mut().resource_mut::<MenuState>().confirm =
        Some(menu::ConfirmKind::Overwrite(Slot::Manual(1)));
    build_pause_tree(&mut app);

    let panel = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            let node = entity.get::<bevy::ui::Node>()?;
            (node.width == Val::Px(420.0)).then_some((entity, node.clone()))
        })
        .map(|(_, node)| node)
        .expect("the dialog's 420px panel");
    assert_eq!(panel.padding, UiRect::all(Val::Px(20.0)));
    assert_eq!(panel.row_gap, Val::Px(12.0));
    assert_eq!(panel.border, UiRect::all(Val::Px(2.0)));
    assert_eq!(panel.flex_direction, FlexDirection::Column);

    let faces = app.world().resource::<TextFaces>();
    let question = text_entity(&app, "Overwrite");
    assert_eq!(
        face_handle(&app, question),
        embedded(&faces.sans_semibold(22.0)),
        "the question is 22px SemiBold"
    );
    assert_eq!(
        app.world().get::<TextFont>(question).unwrap().font_size,
        FontSize::Px(22.0)
    );
    let detail = text_entity(
        &app,
        &format!("Overwrite {}? Its current save is replaced.", slot(1)),
    );
    assert_eq!(
        face_handle(&app, detail),
        embedded(&faces.sans(15.0)),
        "the consequence is 15px sans"
    );
    assert_eq!(
        app.world().get::<TextColor>(detail).map(|color| color.0),
        Some(Color::srgba(0.80, 0.80, 0.85, 0.62)),
        "the consequence is the dialog's dim tone"
    );
}

/// A focused confirm control rings its border in the plugin's own focus tone, and
/// the ring is the tree's, not the coordinator's.
#[test]
fn a_focused_confirm_control_takes_the_the_trees_focus_ring() {
    let dir = temp_pack();
    let mut app = app_for_pack(dir.path().join("pack"));
    esc(&mut app);
    app.world_mut().resource_mut::<MenuState>().confirm = Some(menu::ConfirmKind::Quit);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::ConfirmYes;
    build_pause_tree(&mut app);
    app.world_mut()
        .run_system_once(crate::presentation::render::sync_paint_states)
        .expect("the state repaint runs");

    let yes = action_entity(&app, MenuAction::ConfirmAccept);
    assert_eq!(
        app.world().get::<BorderColor>(yes).map(|color| color.top),
        Some(Color::srgb(0.902, 0.62, 0.361)),
        "the plugin's FOCUS tone rings the accept"
    );
    let no = action_entity(&app, MenuAction::ConfirmCancel);
    assert_eq!(
        app.world().get::<BorderColor>(no).map(|color| color.top),
        Some(Color::NONE)
    );
}

/// A pack whose plugin restyles `confirm` draws its own tree, its own font and
/// its own image, with a copy override's placeholders filled, and its controls
/// still answer clicks.
#[test]
fn a_restyled_confirm_draws_the_packs_tree_font_image_and_copy() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    write_restyle_plugin(&pack);
    let mut app = app_for_pack(pack.clone());
    esc(&mut app);
    app.world_mut().resource_mut::<MenuState>().confirm =
        Some(menu::ConfirmKind::Overwrite(Slot::Manual(1)));
    build_pause_tree(&mut app);

    let overlay = format!("Slot {} is replaced. Proceed?", slot(1));
    let question = text_entity(&app, "Overwrite");
    assert_eq!(
        app.world().get::<TextFont>(question).unwrap().font_size,
        FontSize::Px(40.0),
        "the pack's own 40px face"
    );
    let detail = text_entity(&app, &overlay);
    assert!(
        app.world().get::<TextFont>(detail).is_some(),
        "the override shows, with its placeholder filled"
    );

    let look = app.world().resource::<UiLook>();
    assert_eq!(
        face_handle(&app, question),
        look.shipped_face("Fixture Face", 700)
            .expect("the pack's font is declared and loaded"),
        "the question draws in the pack's own face"
    );
    let (handle, size) = look
        .image("art/mark.png")
        .expect("the pack's image is declared and decoded");
    assert_eq!(size, Vec2::new(2.0, 2.0));
    assert!(
        app.world().iter_entities().any(|entity| entity
            .get::<ImageNode>()
            .is_some_and(|node| node.image == handle)),
        "the tree's image node draws the pack's image"
    );

    click(&mut app, MenuAction::ConfirmCancel);
    assert_eq!(app.world().resource::<MenuState>().confirm, None);
}

/// The handle a face was built with.
fn embedded(font: &TextFont) -> Handle<Font> {
    match &font.font {
        FontSource::Handle(handle) => handle.clone(),
        other => panic!("an embedded face is a handle: {other:?}"),
    }
}

fn slot(slot: u8) -> &'static str {
    menu::slot_label(Slot::Manual(slot))
}

/// The fixture plugin: a shipped face, a shipped image and one copy override,
/// all on the confirm surface.
fn write_restyle_plugin(pack: &Path) {
    let ui = pack.join("ui");
    std::fs::create_dir_all(ui.join("fonts")).unwrap();
    std::fs::create_dir_all(ui.join("art")).unwrap();
    std::fs::create_dir_all(ui.join("surfaces")).unwrap();
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
    std::fs::copy(
        assets.join("IBMPlexSans-Bold.ttf"),
        ui.join("fonts/fixture.ttf"),
    )
    .unwrap();
    std::fs::write(ui.join("fonts/OFL.txt"), "OFL-1.1").unwrap();
    let mut mark = image::RgbaImage::new(2, 2);
    mark.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    mark.save(ui.join("art/mark.png")).unwrap();
    std::fs::write(
        ui.join("plugin.ron"),
        r#"(
    format_version: 1,
    name: "Fixture",
    fonts: [
        (family: "Fixture Face", weight: 700, file: "fonts/fixture.ttf",
         license: "OFL-1.1", license_file: "fonts/OFL.txt"),
    ],
    images: [ (file: "art/mark.png") ],
    copy: { "confirm.overwrite.prompt": "Slot {slot} is replaced. Proceed?" },
)
"#,
    )
    .unwrap();
    std::fs::write(
        ui.join("tokens.ron"),
        r#"(
    colors: { "ink": (r: 0.9, g: 0.9, b: 0.9, a: 1.0) },
    typography: {
        "own": (face: "Fixture Face", weight: 700, size_px: 40.0, line_height_px: 44.0),
    },
    components: {
        "column": (
            typography: "{typography.own}",
            color: "{colors.ink}",
            direction: "column",
            gap: 8.0,
            padding: 12.0,
        ),
    },
)
"#,
    )
    .unwrap();
    std::fs::write(
        ui.join("surfaces/confirm.ron"),
        r#"(
    kind: "box",
    component: "column",
    width: "fill",
    height: "fill",
    children: [
        (kind: "image", file: "art/mark.png", fit: "stretch", width: 2.0, height: 2.0),
        (kind: "slot", slot: "confirm.prompt"),
        (kind: "slot", slot: "confirm.detail"),
        (kind: "box",
            direction: "row",
            gap: 8.0,
            children: [
                (kind: "slot", slot: "confirm.yes"),
                (kind: "slot", slot: "confirm.accept-label"),
                (kind: "slot", slot: "confirm.no"),
            ]),
    ],
)
"#,
    )
    .unwrap();
    let outcome = ui::load(pack);
    assert!(
        matches!(outcome, ui::PluginOutcome::Loaded(_)),
        "the fixture plugin loads: {:?}",
        outcome.findings()
    );
}
