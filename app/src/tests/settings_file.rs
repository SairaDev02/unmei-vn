use super::*;

// ---------------------------------------------------------------------------
// Settings file + the Settings view audio group
// ---------------------------------------------------------------------------

/// The temporary sibling the atomic settings write uses.
fn settings_tmp_path(app: &App) -> PathBuf {
    settings_path(app).with_extension("ron.tmp")
}

/// Play one `←`/`→` step on the focused slider.
fn step_slider(app: &mut App, key: KeyCode) {
    assert!(
        menu::focus_control(menu(app).focus).is_some(),
        "a slider must be focused to step it"
    );
    press_and_update(app, key);
}

#[test]
fn settings_round_trip_persists_four_audio_values_and_no_play_key() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    open_settings(&mut app);

    // Master 100 -> 90, Music 100 -> 80, SFX 100 -> 95, Mute on.
    assert_eq!(menu(&app).focus, MenuFocus::AudioMaster);
    step_slider(&mut app, KeyCode::ArrowLeft);
    step_slider(&mut app, KeyCode::ArrowLeft);
    focus_down(&mut app, 1);
    for _ in 0..4 {
        step_slider(&mut app, KeyCode::ArrowLeft);
    }
    focus_down(&mut app, 1);
    step_slider(&mut app, KeyCode::ArrowLeft);
    focus_down(&mut app, 1);
    press_and_update(&mut app, KeyCode::Enter);

    assert_eq!(audio(&app).master, 0.90);
    assert_eq!(audio(&app).music, 0.80);
    assert_eq!(audio(&app).sfx, 0.95);
    assert!(audio(&app).muted);

    // Moving sliders writes nothing; the file appears only on leaving.
    assert!(
        !settings_path(&app).exists(),
        "no write while the view is open"
    );
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert!(settings_path(&app).exists(), "leaving the view flushes");

    let text = std::fs::read_to_string(settings_path(&app)).unwrap();
    assert!(text.contains("audio"), "{text}");
    assert!(!text.contains("play"), "an audio-only file has no play key");

    let reloaded = settings::load(&pack_root).settings;
    assert_eq!(reloaded.audio.master, 0.90);
    assert_eq!(reloaded.audio.music, 0.80);
    assert_eq!(reloaded.audio.sfx, 0.95);
    assert!(reloaded.audio.muted);

    // A fresh app over the same pack reads the same values.
    let fresh = app_for_pack(pack_root);
    assert_eq!(audio(&fresh), reloaded.audio);
}

#[test]
fn opening_settings_or_moving_sliders_creates_no_file() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    open_settings(&mut app);
    step_slider(&mut app, KeyCode::ArrowLeft);
    assert!(
        !settings_path(&app).exists(),
        "opening and adjusting writes nothing"
    );

    // Backing out to the grid with a pending change flushes once.
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert!(settings_path(&app).exists());
}

#[test]
fn a_preexisting_settings_file_is_byte_identical_until_a_real_change() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root);
    let path = settings_path(&app);
    let original =
        b"(audio: (master: 0.5, music: 0.5, sfx: 0.5, muted: false), future: 7)".to_vec();
    std::fs::write(&path, &original).unwrap();

    // Load and leave the overlay / save and load a slot: no settings change, no
    // rewrite. The unknown `future` key survives untouched.
    esc(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);
    press_and_update(&mut app, KeyCode::F9);
    esc(&mut app);
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn loading_a_slot_leaves_the_settings_file_byte_identical() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);

    // A settings file exists and holds a non-default mix.
    {
        let mut state = app.world_mut().resource_mut::<SettingsState>();
        state.set_fraction(AudioControl::Music, 0.25);
        let settings = state.settings.clone();
        settings::save(&state.pack_root, &settings).unwrap();
    }
    let before = std::fs::read(settings_path(&app)).unwrap();

    press_and_update(&mut app, KeyCode::F9);
    assert_eq!(std::fs::read(settings_path(&app)).unwrap(), before);
}

#[test]
fn mute_then_leave_then_unmute_restores_levels_and_only_muted_changes() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());

    open_settings(&mut app);
    step_slider(&mut app, KeyCode::ArrowLeft);
    step_slider(&mut app, KeyCode::ArrowLeft); // master 90
    focus_down(&mut app, 3); // Master -> Mute
    assert_eq!(menu(&app).focus, MenuFocus::AudioMute);
    press_and_update(&mut app, KeyCode::Enter); // mute on

    press_and_update(&mut app, KeyCode::Escape); // leave -> flush
    let muted_file = settings::load(&pack_root).settings;
    assert!(muted_file.audio.muted);
    assert_eq!(muted_file.audio.master, 0.90, "muting keeps the level");

    open_settings(&mut app);
    focus_down(&mut app, 3);
    press_and_update(&mut app, KeyCode::Enter); // unmute
    press_and_update(&mut app, KeyCode::Escape);

    let unmuted_file = settings::load(&pack_root).settings;
    assert!(!unmuted_file.audio.muted);
    assert_eq!(unmuted_file.audio.master, 0.90, "the level is restored");
    assert_eq!(unmuted_file.audio.music, 1.0);
    assert_eq!(unmuted_file.audio.sfx, 1.0);
}

#[test]
fn audio_reset_confirms_once_resets_only_audio_and_never_deletes_the_file() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = settings::settings_path_for(&pack_root);
    std::fs::write(
        &path,
        "(audio: (master: 0.5, music: 0.5, sfx: 0.5, muted: true), play: (auto: true))",
    )
    .unwrap();
    let mut app = app_for_pack(pack_root.clone());

    open_settings(&mut app);
    focus_down(&mut app, 4); // Master -> Reset
    assert_eq!(menu(&app).focus, MenuFocus::AudioReset);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ResetAudio));

    // Cancelling changes nothing.
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(audio(&app).master, 0.5);
    assert!(audio(&app).muted);
    assert_eq!(menu(&app).view, MenuView::Settings);

    // Accepting resets the audio section only.
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(audio(&app), settings::AudioSettings::default());
    assert!(
        app.world().resource::<SettingsState>().settings.play.auto,
        "reset never reaches into play"
    );

    press_and_update(&mut app, KeyCode::Escape); // leave -> flush
    assert!(path.exists(), "reset never deletes the file");
    let written = settings::load(&pack_root).settings;
    assert_eq!(written.audio, settings::AudioSettings::default());
    assert!(written.play.auto, "the pre-existing play key survives");
}

#[test]
fn fullscreen_boot_reads_the_play_section_into_the_created_window() {
    let dir = temp_pack();
    let pack = dir.path().join("pack");
    let path = settings::settings_path_for(&pack);

    // `fullscreen: true` -> borderless primary.
    std::fs::write(&path, "(play: (fullscreen: true))").unwrap();
    assert_eq!(
        settings::window_mode(settings::load(&pack).settings.play.fullscreen),
        WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
    );

    // `false` -> windowed.
    std::fs::write(&path, "(play: (fullscreen: false))").unwrap();
    assert_eq!(
        settings::window_mode(settings::load(&pack).settings.play.fullscreen),
        WindowMode::Windowed
    );

    // A missing key and an unreadable file -> windowed.
    let missing = temp_pack();
    assert_eq!(
        settings::window_mode(
            settings::load(&missing.path().join("pack"))
                .settings
                .play
                .fullscreen
        ),
        WindowMode::Windowed
    );
    let bad = temp_pack();
    let bad_pack = bad.path().join("pack");
    std::fs::write(settings::settings_path_for(&bad_pack), "not ron").unwrap();
    assert_eq!(
        settings::window_mode(settings::load(&bad_pack).settings.play.fullscreen),
        WindowMode::Windowed
    );
}

#[test]
fn the_live_window_mode_follows_the_fullscreen_field() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();

    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_window_mode)
        .unwrap();
    assert_eq!(
        app.world().get::<Window>(window).unwrap().mode,
        WindowMode::Windowed
    );

    app.world_mut()
        .resource_mut::<SettingsState>()
        .toggle_fullscreen();
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_window_mode)
        .unwrap();
    assert_eq!(
        app.world().get::<Window>(window).unwrap().mode,
        WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
        "an F11/switch toggle reaches the live window"
    );
}

#[test]
fn setup_settings_uses_the_preloaded_settings_without_rereading_the_file() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    // The file on disk says windowed; the preloaded value says fullscreen.
    std::fs::write(
        settings::settings_path_for(&pack_root),
        "(play: (fullscreen: false))",
    )
    .unwrap();
    let mut preloaded = settings::Settings::default();
    preloaded.play.fullscreen = true;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .insert_resource(settings::StartupSettings(settings::LoadedSettings {
            settings: preloaded,
            warning: None,
        }))
        .add_plugins(RuntimeCorePlugin);
    app.update();

    assert!(
        app.world().resource::<SettingsState>().fullscreen(),
        "the preloaded startup value wins over the file"
    );
}

#[test]
fn the_play_switches_toggle_auto_and_fullscreen_and_persist_on_leave() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    open_settings(&mut app);

    // Master -> PlayAuto is six rows down (Text speed precedes it).
    focus_down(&mut app, 6);
    assert_eq!(menu(&app).focus, MenuFocus::PlayAuto);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(app.world().resource::<SettingsState>().auto());
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::Auto),
        "On"
    );
    press_and_update(&mut app, KeyCode::Space);
    assert!(!app.world().resource::<SettingsState>().auto());
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::Auto),
        "Off"
    );

    focus_down(&mut app, 1);
    assert_eq!(menu(&app).focus, MenuFocus::PlayFullscreen);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(app.world().resource::<SettingsState>().fullscreen());
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::Fullscreen),
        "On"
    );
    assert!(
        !settings_path(&app).exists(),
        "no write while the view is open"
    );

    press_and_update(&mut app, KeyCode::Escape); // leave -> flush
    let written = settings::load(&pack_root).settings;
    assert!(written.play.fullscreen);
    assert!(!written.play.auto);
}

#[test]
fn f11_toggles_fullscreen_marks_dirty_and_the_switch_reflects_it() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    assert!(!app.world().resource::<SettingsState>().fullscreen());

    press_and_update(&mut app, KeyCode::F11);
    let state = app.world().resource::<SettingsState>();
    assert!(state.fullscreen(), "F11 flips the field");
    assert!(state.dirty, "and marks the settings dirty");
    assert!(
        !settings_path(&app).exists(),
        "there is no write on key press"
    );

    // Opening the view shows the same field: the switch reflects the hotkey
    // without the overlay having been reopened to pick it up.
    open_settings(&mut app);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::Fullscreen),
        "On"
    );
}

#[test]
fn f11_is_inert_at_a_choice_wait() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Choice { .. })
    ));

    press_and_update(&mut app, KeyCode::F11);
    let state = app.world().resource::<SettingsState>();
    assert!(!state.fullscreen(), "F11 does nothing at a choice-wait");
    assert!(!state.dirty);
    assert!(!settings_path(&app).exists());
}

#[test]
fn f11_is_ignored_while_the_overlay_is_open() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    esc(&mut app);
    assert!(menu(&app).open);

    press_and_update(&mut app, KeyCode::F11);
    let state = app.world().resource::<SettingsState>();
    assert!(
        !state.fullscreen(),
        "the overlay owns input while it is open"
    );
    assert!(!state.dirty);
}

#[test]
fn toggling_f11_twice_restores_the_file_bytes() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());

    // Seed a canonical file with a non-default play section.
    {
        let mut state = app.world_mut().resource_mut::<SettingsState>();
        state.settings.play.auto = true;
        settings::save(&state.pack_root, &state.settings).unwrap();
    }
    let before = std::fs::read(settings_path(&app)).unwrap();

    press_and_update(&mut app, KeyCode::F11);
    press_and_update(&mut app, KeyCode::F11);
    assert!(!app.world().resource::<SettingsState>().fullscreen());
    assert_eq!(
        std::fs::read(settings_path(&app)).unwrap(),
        before,
        "an unflushed pair of toggles touches nothing"
    );

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();
    assert_eq!(
        std::fs::read(settings_path(&app)).unwrap(),
        before,
        "toggling F11 twice restores the file bytes"
    );
}

#[test]
fn an_app_exit_flushes_a_pending_fullscreen_toggle() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());

    press_and_update(&mut app, KeyCode::F11);
    assert!(app.world().resource::<SettingsState>().dirty);
    assert!(
        !settings_path(&app).exists(),
        "no write until a flush point"
    );

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();

    assert!(!app.world().resource::<SettingsState>().dirty);
    assert!(settings::load(&pack_root).settings.play.fullscreen);
}

#[test]
fn play_reset_confirms_once_resets_only_play_and_persists_on_leave() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = settings::settings_path_for(&pack_root);
    let mut seeded = settings::Settings::default();
    seeded.audio.master = 0.5;
    seeded.audio.music = 0.5;
    seeded.audio.sfx = 0.5;
    seeded.audio.muted = true;
    seeded.play.text_speed = 0.04;
    seeded.play.auto = true;
    seeded.play.fullscreen = true;
    settings::save(&pack_root, &seeded).unwrap();
    let mut app = app_for_pack(pack_root.clone());

    open_settings(&mut app);
    focus_down(&mut app, 8); // Master -> PlayReset
    assert_eq!(menu(&app).focus, MenuFocus::PlayReset);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ResetPlay));

    // Cancelling changes nothing.
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(menu(&app).view, MenuView::Settings);
    let state = app.world().resource::<SettingsState>();
    assert!(state.auto());
    assert!(state.fullscreen());
    assert_eq!(state.settings.play.text_speed, 0.04);

    // Accepting resets the play section only.
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, None);
    let state = app.world().resource::<SettingsState>();
    assert_eq!(state.settings.play, settings::PlaySettings::default());
    assert_eq!(state.settings.audio, seeded.audio, "audio is untouched");

    // Persist on leave: the audio bytes are canonical and identical, and the
    // now-default play section is skipped entirely.
    press_and_update(&mut app, KeyCode::Escape);
    let expected = ron::ser::to_string_pretty(
        &settings::Settings {
            audio: seeded.audio.clone(),
            play: settings::PlaySettings::default(),
        },
        ron::ser::PrettyConfig::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    assert!(
        !std::fs::read_to_string(&path).unwrap().contains("play"),
        "an all-default play section carries no key"
    );
}

#[test]
fn a_failed_flush_keeps_values_notices_row_20_and_the_next_flush_retries() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    force_single_threaded(&mut app);

    open_settings(&mut app);
    step_slider(&mut app, KeyCode::ArrowLeft); // master 95, dirty
    std::fs::create_dir_all(settings_tmp_path(&app)).unwrap();

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    // Leaving the view attempts the one flush; it fails.
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).view, MenuView::Slots);
    let state = app.world().resource::<SettingsState>();
    assert!(state.dirty, "a failed flush keeps the change pending");
    assert_eq!(state.settings.audio.master, 0.95, "in-session value kept");
    assert!(app.world().resource::<Notice>().is_showing());
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("settings not saved")
    );
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert!(records[0].2.starts_with("settings not saved"));

    // Unblock and close the overlay: the next flush retries and succeeds.
    std::fs::remove_dir_all(settings_tmp_path(&app)).unwrap();
    press_and_update(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    assert!(!app.world().resource::<SettingsState>().dirty);
    assert_eq!(settings::load(&pack_root).settings.audio.master, 0.95);
}

#[test]
fn an_app_exit_flushes_a_pending_change() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    open_settings(&mut app);
    step_slider(&mut app, KeyCode::ArrowLeft);
    assert!(app.world().resource::<SettingsState>().dirty);
    assert!(!settings_path(&app).exists());

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();

    assert!(!app.world().resource::<SettingsState>().dirty);
    assert_eq!(settings::load(&pack_root).settings.audio.master, 0.95);
}

#[test]
fn the_settings_keyboard_model_moves_controls_and_steps_sliders() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);

    assert_eq!(menu(&app).focus, MenuFocus::AudioMaster);
    // Left/Right adjust without moving focus.
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::AudioMaster);
    assert_eq!(
        app.world()
            .resource::<SettingsState>()
            .percent(AudioControl::Master),
        95
    );
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        app.world()
            .resource::<SettingsState>()
            .percent(AudioControl::Master),
        100
    );

    // Down walks the controls, passing the rail's `Exit to title` and wrapping
    // to the first one.
    for expected in [
        MenuFocus::AudioMusic,
        MenuFocus::AudioSfx,
        MenuFocus::AudioMute,
        MenuFocus::AudioReset,
        MenuFocus::TextSpeed,
        MenuFocus::PlayAuto,
        MenuFocus::PlayFullscreen,
        MenuFocus::PlayReset,
        MenuFocus::KeyRow,
        MenuFocus::CopyKeyPath,
        MenuFocus::ClearKey,
        MenuFocus::ExitToTitle,
        MenuFocus::AudioMaster,
    ] {
        press_and_update(&mut app, KeyCode::ArrowDown);
        assert_eq!(menu(&app).focus, expected);
    }
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::ExitToTitle);

    // Tab cycles the top bar; Down returns to the group's first control.
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::LoadTab);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::AudioMaster);

    // Enter toggles mute while it is focused.
    focus_down(&mut app, 3);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(audio(&app).muted);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!audio(&app).muted);

    // Space is the same toggle.
    press_and_update(&mut app, KeyCode::Space);
    assert!(audio(&app).muted);
    assert_eq!(menu(&app).focus, MenuFocus::AudioMute);
}

#[test]
fn a_malformed_settings_file_is_defaults_warns_row_19_and_keeps_the_bytes() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = settings::settings_path_for(&pack_root);
    let original = b"not ron at all".to_vec();
    std::fs::write(&path, &original).unwrap();

    // Build the plugin (so `Notice`/`DefensiveHold` exist) but drive
    // `setup_settings` directly on this thread, so the row-19 line is captured
    // rather than emitted on a Bevy executor worker thread.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    captures::take();
    app.world_mut()
        .run_system_once(crate::settings::setup_settings)
        .unwrap();

    assert_eq!(
        app.world().resource::<SettingsState>().settings,
        settings::Settings::default(),
        "a malformed file is all defaults"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "row 19 is silent"
    );
    assert!(!app.world().resource::<DefensiveHold>().is_held());
    assert_eq!(std::fs::read(&path).unwrap(), original, "bytes untouched");

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
}
