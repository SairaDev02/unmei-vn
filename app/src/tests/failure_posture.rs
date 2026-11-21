use super::*;

#[test]
fn an_active_notice_does_not_consume_input() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.world_mut()
        .resource_mut::<Notice>()
        .show("something failed");

    let before = app.world().resource::<Runtime>().runner.wait().cloned();
    press_and_update(&mut app, KeyCode::Space);
    let after = app.world().resource::<Runtime>().runner.wait().cloned();

    assert_ne!(
        before, after,
        "the Notice must not consume the advance press"
    );
    assert!(
        app.world().resource::<Notice>().is_showing(),
        "a press must not dismiss the Notice"
    );
}

#[test]
fn a_broken_pack_records_the_failure_and_never_opens_play() {
    let app = app_for_broken_pack();
    let failure = app
        .world()
        .get_resource::<StartupFailure>()
        .expect("the failed load is recorded");
    let StartupFailure::Pack {
        errors, defaulted, ..
    } = failure
    else {
        panic!("a broken pack is a pack failure: {failure:?}");
    };
    assert!(!errors.is_empty(), "the error list is kept whole");
    assert!(!defaulted, "the test app gives an explicit pack path");
    assert!(
        app.world().get_resource::<Runtime>().is_none(),
        "the play path must stay shut without a pack"
    );
}

/// The origin the entry point resolved reaches the screen: a defaulted pack that
/// is missing explains the default rule with the hint line, and the same failure
/// from an explicit path does not.
#[test]
fn the_resolved_pack_origin_reaches_the_error_screen_hint() {
    let broken = |origin| {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(
                RuntimeConfig::new(PathBuf::from("definitely/not/a/demo-pack"), None)
                    .with_dev_mode(true)
                    .with_pack_origin(origin),
            )
            .add_plugins(RuntimeCorePlugin);
        app.update();
        app
    };

    let defaulted = broken(crate::launch::PackOrigin::Defaulted);
    let failure = defaulted
        .world()
        .get_resource::<StartupFailure>()
        .expect("the missing default pack is recorded");
    assert!(
        failure
            .screen_lines()
            .iter()
            .any(|line| line == &crate::failure::default_pack_hint()),
        "the defaulted path explains itself: {:?}",
        failure.screen_lines()
    );

    let explicit = broken(crate::launch::PackOrigin::Explicit);
    let failure = explicit
        .world()
        .get_resource::<StartupFailure>()
        .expect("the missing explicit pack is recorded");
    assert!(
        !failure
            .screen_lines()
            .iter()
            .any(|line| line.contains("No PACK_DIR given")),
        "an explicit pack shows no hint: {:?}",
        failure.screen_lines()
    );
}

#[test]
fn the_error_screen_quits_nonzero_on_a_key_or_a_click() {
    // Each fresh app gets one quit; `should_exit` drains the message.
    for quit in [KeyCode::Escape, KeyCode::Enter, KeyCode::Space] {
        let mut app = app_for_broken_pack();
        assert_eq!(app.should_exit(), None);
        press_and_update(&mut app, quit);
        assert!(
            matches!(app.should_exit(), Some(AppExit::Error(_))),
            "{quit:?} must quit non-zero"
        );
    }

    let mut app = app_for_broken_pack();
    assert_eq!(app.should_exit(), None);
    click_and_update(&mut app);
    assert!(matches!(app.should_exit(), Some(AppExit::Error(_))));
}

#[test]
fn a_healthy_pack_never_records_a_startup_failure() {
    let app = headless_app();
    assert!(app.world().get_resource::<StartupFailure>().is_none());
    assert!(app.world().get_resource::<Runtime>().is_some());
}

#[test]
fn a_defensive_failure_holds_the_runner_and_shows_the_notice() {
    // The fake answers with an action the loaded pack has no label for, which
    // the loader's validation makes unreachable — but the hold is specified.
    captures::install();
    let fake = FakeResolver::new(model_answer("no_such_label"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    force_single_threaded(&mut app);
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    fake.release();
    pump_until(&mut app, on_hold);

    let records = failure_lines();
    assert_eq!(records.len(), 1, "row 12 logs once: {records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("the story can't continue"),
        "{:?}",
        records[0].2
    );

    let runtime = app.world().resource::<Runtime>();
    assert!(
        matches!(runtime.runner.wait(), Some(Wait::Decision { .. })),
        "a defensive failure must leave the runner parked"
    );
    let notice = app.world().resource::<Notice>();
    assert!(notice.is_showing(), "the hold must be visible");
    assert_eq!(
        notice.text.as_deref(),
        Some("the story can't continue — Esc for the menu")
    );
    assert!(on_hold(&app), "rows 11–13 set the defensive hold");

    let parked = parked_position(&app);
    esc(&mut app);
    assert!(menu(&app).open, "Esc opens the overlay under a hold");
    esc(&mut app);
    app.update();
    assert_eq!(
        parked_position(&app),
        parked,
        "a held runner never advances"
    );
    assert_eq!(fake.calls(), 1, "the missing label never re-fires");
}

#[test]
fn a_decide_point_with_no_matching_npc_holds_the_runner() {
    captures::install();
    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = app_with_missing_decide_npc(&fake);
    force_single_threaded(&mut app);
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    app.update();

    let records = failure_lines();
    assert_eq!(records.len(), 1, "row 11 logs once: {records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("the story can't continue"),
        "{:?}",
        records[0].2
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("the story can't continue — Esc for the menu")
    );
    assert!(on_hold(&app));
    assert_eq!(
        fake.calls(),
        0,
        "no resolver call for a missing decide point"
    );
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));

    let parked = parked_position(&app);
    esc(&mut app);
    assert!(menu(&app).open, "Esc opens the overlay under the hold");
    esc(&mut app);
    app.update();
    assert_eq!(parked_position(&app), parked);
}

#[test]
fn a_digit_past_the_choice_options_is_ignored_not_a_defensive_hold() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);

    // Park on first_meeting's two-option choice, then press a digit past it.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    let parked = parked_position(&app);
    assert!(matches!(parked.1, Some(Wait::Choice { .. })));

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    press_and_update(&mut app, KeyCode::Digit9);

    // A mispress is not a row-13 RunnerError: nothing logs, nothing holds, and
    // the choice is still live, so the next valid pick advances normally.
    assert!(failure_lines().is_empty(), "a mispress is not a failure");
    assert!(!on_hold(&app), "a mispress must not freeze the game");
    assert_eq!(parked_position(&app), parked);
    assert!(!app.world().resource::<Notice>().is_showing());

    press_and_update(&mut app, KeyCode::Digit1);
    assert!(
        !matches!(
            app.world().resource::<Runtime>().runner.wait(),
            Some(Wait::Choice { .. })
        ),
        "a valid pick after a mispress still advances"
    );
}

#[test]
fn a_pack_warning_is_logged_but_never_builds_an_error_screen() {
    // A `choice` config with no `"none"` criterion is the canonical warning.
    let dir = tempfile::tempdir().unwrap();
    let pack_root = dir.path().join("warn-pack");
    std::fs::create_dir_all(pack_root.join("npcs")).unwrap();
    std::fs::create_dir_all(pack_root.join("scenes")).unwrap();
    std::fs::write(
        pack_root.join("pack.ron"),
        "(name: \"warn\", entry: \"scene\")",
    )
    .unwrap();
    std::fs::write(pack_root.join("state.ron"), "(met_ace: false)").unwrap();
    std::fs::write(
        pack_root.join("scenes/scene.unmei"),
        "Narrator: hi\n\ndecide ace.greet -> reply\n\nlabel reply.warm\nAce: warm\n\nlabel reply.cold\nAce: cold\n",
    )
    .unwrap();
    std::fs::write(
        pack_root.join("npcs/ace.ron"),
        "(\n    id: \"ace\",\n    name: \"Ace\",\n    decisions: {\n        \"greet\": (\n            instructions: \"Greet?\",\n            state_keys: [\"met_ace\"],\n            criteria: {\n                \"warm\": \"warm\",\n                \"cold\": \"cold\",\n            },\n            fallback: \"cold\",\n        ),\n    },\n)\n",
    )
    .unwrap();

    let loaded = unmei_engine::load_pack(&pack_root).expect("the warning pack still loads");
    assert!(
        !loaded.warnings.is_empty(),
        "the fixture must actually warn, or the test proves nothing"
    );

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    app.update();

    assert!(
        app.world().get_resource::<Runtime>().is_some(),
        "the pack plays despite the warning"
    );
    assert!(
        app.world().get_resource::<StartupFailure>().is_none(),
        "a warning never reaches the error screen"
    );
}

#[test]
fn an_unwritable_log_reports_without_a_notice_and_the_pack_still_plays() {
    let pack = temp_pack();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(LogOpenError {
            path: PathBuf::from("/read-only/pack.log"),
            message: "permission denied".into(),
        })
        .insert_resource(RuntimeConfig::new(pack.path().join("pack"), None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    app.update();

    assert!(
        app.world().get_resource::<Runtime>().is_some(),
        "play continues despite the log failure"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "row 21 must never show a Notice"
    );
}

// ---------------------------------------------------------------------------
// Silent asset degradation (rows 3–6)
// ---------------------------------------------------------------------------

/// A presentation-only app rooted at `asset_root`, with the demo pack's runner
/// held but undriven, so the real load sites and the watcher run with no
/// window, renderer or audio device.
fn asset_watch_app(asset_root: PathBuf) -> App {
    let loaded = load_pack(&demo_pack_root()).expect("the demo pack loads");
    let runtime = Runtime {
        runner: ScriptRunner::new(loaded.pack),
        staging: StagingState::default(),
        pack_root: demo_pack_root(),
        asset_root: asset_root.clone(),
    };

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: asset_root.to_string_lossy().into_owned(),
            ..default()
        })
        // The real app registers these asset types through its render and audio
        // plugins. The image loader here is a test double that reads the file
        // and either rejects it or returns a plain `Image`, so one sibling can
        // load while an unreadable one fails in the loader — the same
        // `LoadState::Failed` path an undecodable file takes.
        .init_asset::<Image>()
        .init_asset_loader::<PathImageLoader>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .insert_resource(runtime)
        .init_resource::<Notice>()
        .init_resource::<DefensiveHold>()
        .init_resource::<AssetWatch>()
        .init_resource::<AppliedVisuals>()
        .init_resource::<AppliedAudio>()
        .init_resource::<AppliedUi>()
        .insert_resource(AudioChannel::<Bgm>::default())
        .add_systems(
            Update,
            (sync_staging_visuals, sync_audio, report_silent_degradations).chain(),
        );
    force_single_threaded(&mut app);
    app
}

#[test]
fn missing_background_sprite_and_music_each_warn_once_and_play_continues() {
    captures::install();
    let dir = tempfile::tempdir().expect("temp dir");
    let mut app = asset_watch_app(dir.path().to_path_buf());
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        runtime.staging = StagingState {
            background: Some("missing_bg".into()),
            sprites: [("ace".to_string(), "missing_pose".to_string())].into(),
            bgm: Some("missing_track".into()),
        };
    }

    captures::take();
    pump_until_watch_drains(&mut app);
    let records = failure_lines();

    assert_eq!(records.len(), 3, "one warn per failed asset: {records:?}");
    assert!(records
        .iter()
        .all(|(level, _, _)| *level == log::Level::Warn));
    for (prefix, path) in [
        ("couldn't load a background", "backgrounds/missing_bg.png"),
        ("couldn't load a sprite", "sprites/ace/missing_pose.png"),
        ("couldn't load the music", "audio/missing_track.ogg"),
    ] {
        assert!(
            records
                .iter()
                .any(|(_, _, line)| line.starts_with(prefix) && line.contains(path)),
            "missing row for {prefix:?} ({path}): {records:?}"
        );
    }
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "silent degradations never notice"
    );

    // Re-applying the same staging must not warn again: the paths were already
    // reported, so the watch is deduplicated. `request_replay` is the
    // production way to force that re-application.
    app.world_mut()
        .run_system_once(
            |mut audio: ResMut<AppliedAudio>,
             mut visuals: ResMut<AppliedVisuals>,
             mut ui: ResMut<AppliedUi>| {
                crate::presentation::play::request_replay(&mut audio, &mut visuals, &mut ui);
            },
        )
        .unwrap();
    captures::take();
    app.update();
    pump_until_watch_drains(&mut app);
    assert!(
        failure_lines().is_empty(),
        "a repeated staging command must not warn again"
    );

    let mut runtime = app.world_mut().resource_mut::<Runtime>();
    let outcome = runtime.runner.run().unwrap();
    runtime.staging.apply_all(&outcome.staged);
    assert!(
        outcome.wait.is_some(),
        "a missing asset never stops the runner"
    );
}

#[test]
fn a_missing_choice_sound_reports_row_6_once() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    std::fs::remove_file(pack_root.join("assets/audio/sfx_select.ogg")).expect("remove the sfx");

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: pack_root.join("assets").to_string_lossy().into_owned(),
            ..default()
        })
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<Notice>()
        .init_resource::<AssetWatch>()
        .insert_resource(AudioChannel::<Sfx>::default())
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin)
        .add_systems(Update, report_silent_degradations);
    force_single_threaded(&mut app);
    app.update(); // load the pack, then show the title
    enter_play(&mut app);

    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        loop {
            let outcome = runtime.runner.run().unwrap();
            runtime.staging.apply_all(&outcome.staged);
            match outcome.wait {
                Some(Wait::Say { .. }) => runtime.runner.advance_say().expect("advance"),
                Some(Wait::Choice { .. }) => break,
                Some(Wait::Decision { .. }) => {
                    runtime.runner.resolve_decision("cold").expect("resolve");
                }
                None => panic!("the script ended before a choice"),
            }
        }
    }

    captures::take();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Digit1);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    pump_until_watch_drains(&mut app);

    let records = failure_lines();
    assert_eq!(
        records.len(),
        1,
        "one warn for the missing sfx: {records:?}"
    );
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't load the choice sound"),
        "{:?}",
        records[0].2
    );
    assert!(records[0].2.contains("audio/sfx_select.ogg"));
    assert!(!app.world().resource::<Notice>().is_showing());
}

#[test]
fn an_undecodable_sprite_warns_while_the_valid_background_still_loads() {
    captures::install();
    let dir = tempfile::tempdir().expect("temp dir");
    let assets = dir.path().join("assets");
    std::fs::create_dir_all(assets.join("backgrounds")).expect("backgrounds dir");
    std::fs::create_dir_all(assets.join("sprites/ace")).expect("sprites dir");
    // A readable sibling and an unreadable one; the loader rejects empty bytes.
    std::fs::write(assets.join("backgrounds/school_gate.png"), b"a background")
        .expect("valid background");
    std::fs::write(assets.join("sprites/ace/happy.png"), b"").expect("empty sprite");

    let mut app = asset_watch_app(assets);
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        runtime.staging = StagingState {
            background: Some("school_gate".into()),
            sprites: [("ace".to_string(), "happy".to_string())].into(),
            bgm: None,
        };
    }

    captures::take();
    pump_until_watch_drains(&mut app);

    let records = failure_lines();
    assert_eq!(
        records.len(),
        1,
        "only the undecodable sprite warns: {records:?}"
    );
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't load a sprite"),
        "{:?}",
        records[0].2
    );
    assert!(!app.world().resource::<Notice>().is_showing());

    let server = app.world().resource::<bevy::asset::AssetServer>().clone();
    let handle = server
        .get_handle::<Image>("backgrounds/school_gate.png")
        .expect("the background handle exists");
    assert!(
        app.world()
            .resource::<bevy::asset::Assets<Image>>()
            .contains(handle.id()),
        "a failed sibling must not clear the loaded background"
    );
}

/// The no-executable startup records its reason before the app exists, and the
/// pack load is skipped so a second failure cannot overwrite it.
#[test]
fn a_recorded_no_executable_failure_survives_startup_without_a_pack_load() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(
            RuntimeConfig::new(PathBuf::from("definitely/not/a/pack"), None).with_dev_mode(true),
        )
        .insert_resource(StartupFailure::NoExecutable)
        .add_plugins(RuntimeCorePlugin);
    app.update();

    assert!(
        matches!(
            app.world().get_resource::<StartupFailure>(),
            Some(StartupFailure::NoExecutable)
        ),
        "the recorded reason must not be replaced by a pack load"
    );
    assert!(app.world().get_resource::<Runtime>().is_none());
}

/// The no-executable screen draws the one shared message and no pack path, so the
/// player sees the same text a terminal does.
#[test]
fn the_no_executable_screen_draws_the_shared_message_without_a_path() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    attach_text_faces(&mut app);
    app.insert_resource(StartupFailure::NoExecutable);
    app.world_mut()
        .run_system_once(crate::presentation::play::draw_error_screen)
        .expect("draw_error_screen runs");

    let mut texts: Vec<String> = app
        .world()
        .iter_entities()
        .filter_map(|entity| entity.get::<Text>().map(|text| text.0.clone()))
        .collect();
    texts.sort();
    assert!(
        texts.contains(&"unmei-play could not start:".to_string()),
        "the intro is drawn: {texts:?}"
    );
    let body = texts
        .iter()
        .find(|text| text.contains(crate::launch::NO_EXECUTABLE_MESSAGE))
        .expect("the shared message reaches the screen");
    assert!(!body.contains(".log"), "no log pointer: {body:?}");
    assert!(
        !body.contains("unmei could not load the pack:"),
        "no pack path or pack wording: {body:?}"
    );
}

/// A scene whose loop parks only while `met_ace` holds: with the declared
/// `false` it spins, a cycle pack load cannot see (it passes through an `if`).
const SPINNING_LOOP: &str = "label top\nif met_ace\nA: met.\nendif\njump top\n";

#[test]
fn a_wait_free_cycle_after_a_say_lands_on_row_13_with_the_hold() {
    captures::install();
    let pack = temp_custom_pack(&[("a", &format!("A: entry.\n{SPINNING_LOOP}"))], None, "a");
    let mut app = app_with_decisions_at(pack.path().join("pack"), None);
    force_single_threaded(&mut app);
    assert!(parked_on_say(&app), "the entry say-line parks first");
    captures::take();

    press_and_update(&mut app, KeyCode::Space);

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("the story can't continue")
            && records[0].2.contains("`jump top` in scene `a`"),
        "row 13's string with the runner's detail: {:?}",
        records[0].2
    );
    assert!(app.world().resource::<DefensiveHold>().is_held());
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("the story can't continue — Esc for the menu")
    );
}

#[test]
fn new_game_on_an_entry_scene_that_never_parks_lands_on_row_13_with_the_hold() {
    captures::install();
    let pack = temp_custom_pack(&[("a", SPINNING_LOOP)], None, "a");
    let mut app = boot_app(pack.path().join("pack"), None);
    force_single_threaded(&mut app);
    captures::take();

    enter_play(&mut app);

    let records = failure_lines();
    assert!(
        records
            .iter()
            .any(|(_, _, line)| line.contains("closes a cycle that reaches no say-line")),
        "{records:?}"
    );
    assert!(
        app.world().resource::<DefensiveHold>().is_held(),
        "New Game's reset must not release the hold it reports"
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("the story can't continue — Esc for the menu")
    );
}
