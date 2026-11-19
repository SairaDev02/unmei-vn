use super::*;
use crate::credentials::{self, ClipboardPort, CopyPathLabel, CredentialsState, KeyField, KeyRow};
use crate::failure::{captures, LogOpenError, Notice, StartupFailure};
use crate::input::{InputClaim, InputOwnership};
use crate::menu::{
    self, ConfirmKind, LoadSurface, MenuAction, MenuFocus, MenuMode, MenuState, MenuView,
    SlotAction,
};
use crate::presentation::assets::AssetWatch;
use crate::presentation::play::{
    report_silent_degradations, sync_audio, sync_staging_visuals, AppliedAudio, AppliedUi,
    AppliedVisuals, Bgm, DialogueText, HelpLine, Sfx, TextReveal,
};
use crate::presentation::settings_view::{SettingsLabel, SettingsLabelKind};
use crate::session::{LoadOrigin, LoadOutcome, ReadOutcome, SessionInput};
use crate::settings::{self, AudioControl, SettingsState};
use bevy::app::AppExit;
use bevy::asset::{
    io::Reader, AssetApp, AssetLoader, AssetPlugin, LoadContext, LoadState, RenderAssetUsages,
};
use bevy::ecs::system::RunSystemOnce;
use bevy::ecs::world::FromWorld;
use bevy::reflect::TypePath;
use bevy::render::render_resource::Extent3d;
use bevy::text::{EditableText, LineBreak, LineHeight, TextLayout};
use bevy::ui::widget::NodeImageMode;
use bevy::ui_widgets::ScrollArea;
use bevy::window::{MonitorSelection, PrimaryWindow, Window, WindowMode};
use bevy::MinimalPlugins;
use bevy_kira_audio::AudioChannel;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use unmei_engine::decision::{DecisionError, DecisionResolver, ResolvedDecision};
use unmei_engine::runner::Wait;
use unmei_engine::save::{self, SaveFile, Slot, SlotStatus, StagingState};
use unmei_engine::{load_pack, DecisionConfig, KeyCheck, KeyVerdict, ScriptRunner, StateValue};

use crate::drawer::{
    self, MonitorControl, MonitorRow, MonitorRowPart, MonitorStatusText, MonitorView, MonitorWork,
    DISPLAY_ROWS, MONITOR_CLAIM, NEWEST_INSET, NEWEST_WASH, PROJECTION_SCAN_BUDGET,
    TEXT_WORK_BUDGET,
};
use crate::input::InputQuarantine;
use crate::monitor::{MonitorHistory, MonitorLine};

use support::*;
pub(crate) use support::{
    activate_control, app_for_pack, boot_widget_input_app, drawer_open, drawer_view,
    enter_play_with_input, press_and_update, press_key, step_input, tap_key, temp_pack,
};

mod asset_root;
mod comfort;
mod decisions;
mod failure_posture;
mod gallery;
mod input_ownership;
mod key_row;
mod monitor_drawer;
mod monitor_reading;
mod overlay;
mod play_path;
mod reading;
mod save_slots;
mod settings_file;
mod support;
mod timeline;
mod title;

// ---------------------------------------------------------------------------
// The embedded interface typefaces
// ---------------------------------------------------------------------------

/// The five interface faces are embedded in the binary and resolve with no
/// pack asset root: the asset root here points at a directory that does not
/// exist, so nothing can come from disk (`docs/title-and-menu.md`
/// § Type system §1).
#[test]
fn the_five_interface_faces_are_embedded_and_load_without_a_pack_asset_root() {
    use crate::presentation::style::{
        embed_text_faces, TextFaces, FIRAMONO_MEDIUM, FIRAMONO_REGULAR, IBMPLEXSANS_BOLD,
        IBMPLEXSANS_REGULAR, IBMPLEXSANS_SEMIBOLD,
    };
    use bevy::asset::Assets;
    use bevy::text::{Font, FontCx, TextPlugin};

    let dir = tempfile::tempdir().unwrap();
    let asset_root = dir.path().join("no-such-pack-assets");

    // The real delivery path: `TextPlugin` owns `Assets<Font>` (as it does in
    // `DefaultPlugins`), then the presentation plugin's `embed_text_faces`
    // installs the five embedded faces into it. The asset root does not exist,
    // so nothing can load from disk.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: asset_root.to_string_lossy().into_owned(),
            ..default()
        })
        .add_plugins(TextPlugin);
    embed_text_faces(&mut app);
    app.update();

    let faces = app.world().resource::<TextFaces>().clone();

    // One distinct asset per face.
    let ids = [
        faces.sans_regular.id(),
        faces.sans_semibold.id(),
        faces.sans_bold.id(),
        faces.mono_regular.id(),
        faces.mono_medium.id(),
    ];
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 5, "the five faces are distinct: {ids:?}");

    // Each handle is the embedded file for its named face, so
    // `mono_medium` is Fira Mono Medium and `mono_regular` Fira Mono Regular.
    let font_bytes = |handle: &Handle<Font>| {
        app.world()
            .resource::<Assets<Font>>()
            .get(handle)
            .unwrap_or_else(|| panic!("no asset for {handle:?}"))
            .data
            .data()
            .to_vec()
    };
    assert_eq!(font_bytes(&faces.sans_regular), IBMPLEXSANS_REGULAR);
    assert_eq!(font_bytes(&faces.sans_semibold), IBMPLEXSANS_SEMIBOLD);
    assert_eq!(font_bytes(&faces.sans_bold), IBMPLEXSANS_BOLD);
    assert_eq!(font_bytes(&faces.mono_regular), FIRAMONO_REGULAR);
    assert_eq!(font_bytes(&faces.mono_medium), FIRAMONO_MEDIUM);

    // Loadable: `TextPlugin`'s font-collection system registered both families
    // from the embedded bytes, so the handles resolve to real faces.
    let mut font_cx = app.world_mut().resource_mut::<FontCx>();
    for family in ["IBM Plex Sans", "Fira Mono"] {
        assert!(
            font_cx.collection.family_id(family).is_some(),
            "the embedded faces register the {family} family"
        );
    }
}

// ---------------------------------------------------------------------------
// The sans/mono type split
// ---------------------------------------------------------------------------

/// The five face handles, for membership checks.
fn face_handles(app: &App) -> Vec<Handle<bevy::text::Font>> {
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>();
    vec![
        faces.sans_regular.clone(),
        faces.sans_semibold.clone(),
        faces.sans_bold.clone(),
        faces.mono_regular.clone(),
        faces.mono_medium.clone(),
    ]
}

/// Every text entity in the world names one of the five bundled faces and never
/// Bevy's default handle (`docs/title-and-menu.md` § Type system §5).
fn assert_every_text_names_a_bundled_face(app: &App) {
    let bundled = face_handles(app);
    let mut checked = 0;
    for entity in app.world().iter_entities() {
        let is_text = entity.get::<Text>().is_some() || entity.get::<TextSpan>().is_some();
        if !is_text {
            continue;
        }
        let font = entity
            .get::<TextFont>()
            .unwrap_or_else(|| panic!("{:?} is text but has no TextFont", entity.id()));
        let handle = match &font.font {
            bevy::text::FontSource::Handle(handle) => handle.clone(),
            other => panic!("{:?} names a non-bundled source: {other:?}", entity.id()),
        };
        assert!(
            bundled.contains(&handle),
            "{:?} uses an unknown face: {handle:?}",
            entity.id()
        );
        checked += 1;
    }
    assert!(checked > 0, "the app built some text");
}

/// The faces of the title's wordmark, tagline, rows, reason line and legend
/// (`docs/title-and-menu.md` § Type system §2/§3/§4).
#[test]
fn the_titles_faces_follow_the_type_system() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    let name = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .manifest
        .name
        .clone();
    assert_eq!(
        face_handle(&app, text_entity(&app, &name)),
        faces.sans_bold,
        "the wordmark is Plex Sans Bold"
    );
    assert_eq!(
        face_handle(&app, text_entity(&app, "A DEMO PACK")),
        faces.mono_medium,
        "the tagline is Fira Mono Medium"
    );
    for label in ["Continue", "New Game", "Settings", "Gallery", "Quit"] {
        assert_eq!(
            face_handle(&app, text_entity(&app, label)),
            faces.sans_regular,
            "the {label} row label is sans"
        );
    }
    assert_eq!(
        face_handle(&app, text_entity(&app, "No autosave yet")),
        faces.mono_medium,
        "Continue's reason line is mono"
    );
    assert_eq!(
        face_handle(&app, text_entity(&app, menu::TITLE_LEGEND)),
        faces.mono_medium,
        "the legend is mono"
    );
    assert_every_text_names_a_bundled_face(&app);
}

/// The say-line's root is the mono speaker prefix and its child span the sans
/// prose, and the reveal writes the span only
/// (`docs/title-and-menu.md` § Type system §4/§5).
#[test]
fn the_say_line_splits_into_a_mono_prefix_and_a_sans_prose_span() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    let (speaker, full) = say_parts(&app);
    let prefix = format!("{speaker}: ");

    render_dialogue(&mut app);
    let prose = app
        .world()
        .get::<Children>(node)
        .and_then(|children| children.first().copied())
        .expect("the dialogue node has one prose span");
    let root_text = |app: &App| app.world().get::<Text>(node).expect("root").0.clone();
    let prose_text = |app: &App| app.world().get::<TextSpan>(prose).expect("span").0.clone();

    assert_eq!(root_text(&app), prefix, "the root is the whole prefix");
    assert_eq!(prose_text(&app), "", "the span starts empty");
    assert_eq!(
        face_handle(&app, node),
        faces.mono_medium,
        "the speaker prefix is mono"
    );
    assert_eq!(
        face_handle(&app, prose),
        faces.sans_regular,
        "the prose span is sans"
    );
    assert_ne!(
        face_handle(&app, node),
        face_handle(&app, prose),
        "the two halves use different faces"
    );

    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    let shown = prose_text(&app);
    assert!(
        !shown.is_empty() && full.starts_with(&shown),
        "the reveal fills the prose span: {shown:?}"
    );
    assert_eq!(
        root_text(&app),
        prefix,
        "the reveal never rewrites the mono prefix"
    );
}

/// A headless UI world with the embedded faces and no pack asset root renders
/// the dialogue split (`docs/title-and-menu.md` § Type system §5).
#[test]
fn a_headless_ui_world_renders_the_dialogue_split_without_a_pack_asset_root() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    attach_text_faces(&mut app);
    app.world_mut()
        .run_system_once(crate::presentation::play::setup_ui)
        .expect("setup_ui runs");

    let root = app
        .world()
        .iter_entities()
        .find_map(|entity| entity.get::<DialogueText>().map(|_| entity.id()))
        .expect("the dialogue node exists");
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    assert_eq!(face_handle(&app, root), faces.mono_medium);
    let prose = app
        .world()
        .get::<Children>(root)
        .and_then(|children| children.first().copied())
        .expect("the dialogue node has a prose span");
    assert_eq!(face_handle(&app, prose), faces.sans_regular);
    assert_ne!(face_handle(&app, root), face_handle(&app, prose));
}

/// The slot card splits into a sans label root, mono machine spans and a sans
/// action-label span, each with its own colour
/// (`docs/title-and-menu.md` § Type system §4/§5).
#[test]
fn the_slot_cards_split_their_faces_per_the_type_system() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app); // the overlay's slots grid
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    let card = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            (entity.get::<MenuAction>() == Some(&MenuAction::Card(0))).then_some(entity.id())
        })
        .expect("card 0 is built");
    assert_eq!(
        face_handle(&app, card),
        faces.sans_regular,
        "the card label is sans"
    );
    assert!(
        app.world().get::<TextColor>(card).is_some(),
        "the label sets its own colour"
    );

    let children: Vec<Entity> = app
        .world()
        .get::<Children>(card)
        .expect("the card has spans")
        .iter()
        .collect();
    assert_eq!(
        children.len(),
        5,
        "four machine lines then the action label"
    );
    for machine in &children[..4] {
        assert_eq!(
            face_handle(&app, *machine),
            faces.mono_medium,
            "kind/status/detail/saved-at are mono"
        );
        assert!(
            app.world().get::<TextColor>(*machine).is_some(),
            "every machine span sets its own colour"
        );
    }
    let action = children[4];
    assert_eq!(
        face_handle(&app, action),
        faces.sans_regular,
        "the action label is sans"
    );
    assert!(
        app.world().get::<TextColor>(action).is_some(),
        "the action span sets its own colour"
    );
}

/// The Settings panel title is SemiBold, group headers and control values mono,
/// and row labels sans (`docs/title-and-menu.md` § Type system §2/§3/§5).
#[test]
fn the_settings_faces_follow_the_type_system() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    assert_eq!(
        face_handle(&app, text_entity(&app, "Settings")),
        faces.sans_semibold,
        "the panel title is SemiBold"
    );
    for header in ["Audio", "Play", "Decisions"] {
        assert_eq!(
            face_handle(&app, text_entity(&app, header)),
            faces.mono_medium,
            "the {header} group header is mono"
        );
    }
    for label in [
        "Master",
        "Music",
        "SFX",
        "Mute",
        "Text speed",
        "Auto",
        "Fullscreen",
    ] {
        assert_eq!(
            face_handle(&app, text_entity(&app, label)),
            faces.sans_regular,
            "the {label} row label is sans"
        );
    }
    for value in ["100%", "Instant", "Off", "Not set", "Stored in \u{2014}"] {
        assert_eq!(
            face_handle(&app, text_entity(&app, value)),
            faces.mono_medium,
            "the {value} control value is mono"
        );
    }
    assert_every_text_names_a_bundled_face(&app);
}

/// The startup error screen's intro is sans while its pack path, error lines and
/// quit footer are mono (`docs/title-and-menu.md` § Type system §2/§5).
#[test]
fn the_error_screen_splits_its_sans_intro_and_mono_machine_lines() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    attach_text_faces(&mut app);
    app.insert_resource(StartupFailure::Pack {
        pack_root: PathBuf::from("/games/pack"),
        errors: vec![unmei_engine::LoadError::new("pack.ron", Some(3), "boom")],
        defaulted: false,
    });
    app.world_mut()
        .run_system_once(crate::presentation::play::draw_error_screen)
        .expect("draw_error_screen runs");
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    assert_eq!(
        face_handle(&app, text_entity(&app, "unmei could not load the pack:")),
        faces.sans_regular,
        "the intro is sans"
    );
    let machine = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            let text = entity.get::<Text>()?;
            text.0.contains("/games/pack").then_some(entity.id())
        })
        .expect("the machine block renders");
    assert_eq!(
        face_handle(&app, machine),
        faces.mono_medium,
        "the path, error lines and footer are mono"
    );
    let body = app
        .world()
        .get::<Text>(machine)
        .expect("machine text")
        .0
        .clone();
    assert!(body.contains("pack.ron:3: boom"), "{body}");
    assert!(
        body.contains("Esc · Enter · Space · click to quit"),
        "the quit footer is mono: {body}"
    );
    assert_every_text_names_a_bundled_face(&app);
}

/// Every surface's text names a bundled face, never Bevy's default handle
/// (`docs/title-and-menu.md` § Type system §5).
#[test]
fn every_app_text_entity_names_a_bundled_face() {
    // The title and its Settings door.
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    assert_every_text_names_a_bundled_face(&app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut app, KeyCode::Enter);
    build_menu_ui(&mut app);
    assert_every_text_names_a_bundled_face(&app);
    app.world_mut()
        .run_system_once(crate::presentation::play::setup_ui)
        .expect("setup_ui runs");
    assert_every_text_names_a_bundled_face(&app);

    // The Esc overlay's slots grid and its Settings door.
    let pack = temp_pack();
    let mut play = app_for_pack(pack.path().join("pack"));
    esc(&mut play);
    play.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut play);
    assert_every_text_names_a_bundled_face(&play);
    open_settings(&mut play);
    build_menu_ui(&mut play);
    assert_every_text_names_a_bundled_face(&play);

    // The startup error screen.
    let mut error = App::new();
    error.add_plugins(MinimalPlugins);
    attach_text_faces(&mut error);
    error.insert_resource(StartupFailure::Pack {
        pack_root: PathBuf::from("/games/pack"),
        errors: Vec::new(),
        defaulted: false,
    });
    error
        .world_mut()
        .run_system_once(crate::presentation::play::draw_error_screen)
        .expect("draw_error_screen runs");
    assert_every_text_names_a_bundled_face(&error);
}

/// The choice options are sans (`docs/title-and-menu.md` § Type system §2).
#[test]
fn the_choice_options_are_sans() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    // The entry say -> Ace say -> the choice wait.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    render_dialogue(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    let button = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::play::ChoiceButton>()
                .map(|_| entity.id())
        })
        .expect("a choice button exists");
    assert_eq!(
        face_handle(&app, button),
        faces.sans_regular,
        "the choice option text is sans"
    );
    assert_every_text_names_a_bundled_face(&app);
}
