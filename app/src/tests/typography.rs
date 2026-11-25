use super::*;

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

    let ids = [
        faces.sans_regular.id(),
        faces.sans_semibold.id(),
        faces.sans_bold.id(),
        faces.mono_regular.id(),
        faces.mono_medium.id(),
    ];
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 5, "the five faces are distinct: {ids:?}");

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

/// The ledger row splits per Default look §6: the slot label and the saved-at
/// line are mono, and the scene or status and the action are sans, each with the
/// tone its state gives it.
#[test]
fn the_ledger_rows_split_their_faces_per_the_type_system() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    for mono in ["Slot 1", "Autosave", "no save yet"] {
        let entity = text_entity(&app, mono);
        assert_eq!(
            face_handle(&app, entity),
            faces.mono_medium,
            "{mono:?} is mono"
        );
        assert!(
            app.world().get::<TextColor>(entity).is_some(),
            "every machine line sets its own tone"
        );
    }
    for sans in ["—", "Save here"] {
        let entity = text_entity(&app, sans);
        assert_eq!(
            face_handle(&app, entity),
            faces.sans_regular,
            "{sans:?} is sans"
        );
        assert!(
            app.world().get::<TextColor>(entity).is_some(),
            "the scene line and the action set their own tone"
        );
    }
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
        errors: vec![unmei_engine::Finding::at_line(
            "pack.ron",
            3,
            unmei_engine::finding::rules::PACK_MANIFEST,
            "boom",
        )],
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
    let option_text = app
        .world()
        .iter_entities()
        .filter(|entity| {
            entity
                .get::<Text>()
                .is_some_and(|text| text.0.chars().count() > 3)
        })
        .map(|entity| entity.id())
        .find(|entity| {
            let mut parent = *entity;
            for _ in 0..4 {
                match app.world().get::<ChildOf>(parent) {
                    Some(child_of) => parent = child_of.parent(),
                    None => return false,
                }
                if parent == button {
                    return true;
                }
            }
            false
        })
        .expect("the option's words live inside its button");
    assert_eq!(
        face_handle(&app, option_text),
        faces.sans_regular,
        "the choice option text is sans"
    );
    assert_every_text_names_a_bundled_face(&app);
}
