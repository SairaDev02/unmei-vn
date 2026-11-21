use super::*;

/// The temporary sibling `write_save` uses for one slot path.
fn tmp_path(slot: &Path) -> PathBuf {
    slot.with_extension("ron.tmp")
}

/// [`app_for_pack`] with an explicit dev-mode gate, so the disabled-shortcut
/// regressions start from real play without a shortcut-enabled fixture.
fn app_for_pack_dev(pack_root: PathBuf, dev_mode: bool) -> App {
    let mut app = boot_app_unstarted_dev(pack_root, None, dev_mode);
    app.update();
    enter_play(&mut app);
    app
}

#[test]
fn save_then_load_restores_position_through_the_app() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Mid-scene: advance once (entry say -> Ace say).
    press_and_update(&mut app, KeyCode::Space);
    let saved_wait = app.world().resource::<Runtime>().runner.wait().cloned();
    assert!(matches!(saved_wait, Some(Wait::Say { .. })));

    press_and_update(&mut app, KeyCode::F5);
    assert!(manual_path(&app).exists(), "F5 wrote the first manual slot");

    press_and_update(&mut app, KeyCode::Space);
    let moved_on = app.world().resource::<Runtime>().runner.wait().cloned();
    assert_ne!(moved_on, saved_wait);
    press_and_update(&mut app, KeyCode::F9);

    let restored = app.world().resource::<Runtime>().runner.wait().cloned();
    assert_eq!(restored, saved_wait);
}

#[test]
fn save_survives_an_app_restart() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");

    let (saved_wait, saved_staging) = {
        let mut app = app_for_pack(pack_root.clone());
        press_and_update(&mut app, KeyCode::Space);
        let runtime = app.world().resource::<Runtime>();
        let wait = runtime.runner.wait().cloned();
        let staging = runtime.staging.clone();
        press_and_update(&mut app, KeyCode::F5);
        (wait, staging)
    };

    let mut restarted = app_for_pack(pack_root);
    assert_eq!(
        restarted.world().resource::<Runtime>().runner.scene_id(),
        "first_meeting"
    );
    press_and_update(&mut restarted, KeyCode::F9);

    let runtime = restarted.world().resource::<Runtime>();
    assert_eq!(runtime.runner.wait().cloned(), saved_wait);
    assert_eq!(runtime.staging, saved_staging);
    assert_eq!(runtime.staging.background.as_deref(), Some("school_gate"));
    assert_eq!(runtime.staging.bgm.as_deref(), Some("music_morning"));
}

#[test]
fn load_without_a_save_file_is_a_clean_no_op() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let before = app.world().resource::<Runtime>().runner.wait().cloned();
    assert!(!manual_path(&app).exists());
    press_and_update(&mut app, KeyCode::F9);
    let after = app.world().resource::<Runtime>().runner.wait().cloned();
    assert_eq!(before, after);
}

#[test]
fn runtime_config_defaults_dev_mode_to_debug_assertions_with_an_override() {
    let config = RuntimeConfig::new(PathBuf::from("a-pack"), None);
    // The default is the compiler's debug-assertions setting, never the
    // optimization level or the presence of debug symbols.
    assert_eq!(config.dev_mode, cfg!(debug_assertions));
    assert!(
        RuntimeConfig::new(PathBuf::from("a-pack"), None)
            .with_dev_mode(true)
            .dev_mode
    );
    assert!(!config.with_dev_mode(false).dev_mode);
}

#[test]
fn dev_mode_off_makes_f5_and_f9_silent_no_ops() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");

    // Seed a valid manual slot from a shortcut-enabled run, so the dev-off F9
    // has something it would otherwise restore.
    {
        let mut seeded = app_for_pack(pack_root.clone());
        press_and_update(&mut seeded, KeyCode::Space);
        assert!(matches!(
            seeded.world().resource::<Runtime>().runner.wait(),
            Some(Wait::Say { .. })
        ));
        press_and_update(&mut seeded, KeyCode::F5);
        assert!(manual_path(&seeded).exists(), "the seed saved");
    }

    // The dev-off app starts real play and moves past the seeded park.
    let mut app = app_for_pack_dev(pack_root, false);
    force_single_threaded(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    let parked = app.world().resource::<Runtime>().runner.wait().cloned();
    assert!(matches!(parked, Some(Wait::Choice { .. })));
    let slot = manual_path(&app);
    let bytes_before = std::fs::read(&slot).expect("the seeded slot exists");

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    press_and_update(&mut app, KeyCode::F5);
    assert_eq!(
        std::fs::read(&slot).unwrap(),
        bytes_before,
        "dev-off F5 left the slot byte-identical"
    );
    assert!(
        captures::take().is_empty(),
        "dev-off F5 logged nothing at all"
    );

    press_and_update(&mut app, KeyCode::F9);
    assert_eq!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        parked,
        "dev-off F9 did not restore the slot"
    );
    assert!(captures::take().is_empty(), "dev-off F9 logged nothing");
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "dev-off F9 raised no Notice"
    );

    // Player persistence is not gated: resolving the choice still autosaves.
    press_and_update(&mut app, KeyCode::Digit1);
    assert!(
        autosave_path(&app).exists(),
        "dev mode off leaves autosave active"
    );
}

#[test]
fn a_boot_run_writes_no_slot() {
    let pack = temp_pack();
    let app = app_for_pack(pack.path().join("pack"));
    let runtime = app.world().resource::<Runtime>();
    for info in unmei_engine::list_slots(runtime.runner.pack()) {
        assert_eq!(
            info.status,
            SlotStatus::Empty,
            "a launch writes no slot ({:?})",
            info.slot
        );
    }
}

#[test]
fn a_choice_resolve_writes_the_autosave_once() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    assert!(!autosave_path(&app).exists());

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Choice { .. })
    ));
    press_and_update(&mut app, KeyCode::Digit1);

    // The autosave names the park the choice resolve reached, with the app
    // clock stamp and the staged snapshot the run applied.
    let runtime = app.world().resource::<Runtime>();
    let save = save::read_slot(runtime.runner.pack(), Slot::Autosave).expect("autosaved");
    assert!(save.saved_at.is_some(), "the app stamps saved_at");
    assert_eq!(save.wait, runtime.runner.wait().cloned());
    assert_eq!(save.staging, runtime.staging);

    // No further write without another event.
    let before = std::fs::read(autosave_path(&app)).unwrap();
    app.update();
    assert_eq!(std::fs::read(autosave_path(&app)).unwrap(), before);
    assert!(
        !app.world().resource::<crate::slots::PendingAutosave>().0,
        "the write disarmed the trigger"
    );
}

#[test]
fn a_scene_crossing_writes_the_autosave() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space); // Ace say
    press_and_update(&mut app, KeyCode::Space); // choice
    press_and_update(&mut app, KeyCode::Digit1); // ask_letter say
                                                 // The next advance runs `jump reunion`, a scene crossing.
    press_and_update(&mut app, KeyCode::Space);

    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), "reunion");
    let save = save::read_slot(runtime.runner.pack(), Slot::Autosave).expect("autosaved");
    assert_eq!(save.scene_id, "reunion");
    assert_eq!(save.wait, runtime.runner.wait().cloned());
    assert!(save.saved_at.is_some());

    // No further write without another event.
    let before = std::fs::read(autosave_path(&app)).unwrap();
    app.update();
    assert_eq!(std::fs::read(autosave_path(&app)).unwrap(), before);
    assert!(
        !app.world().resource::<crate::slots::PendingAutosave>().0,
        "the write disarmed the trigger"
    );
}

#[test]
fn a_load_writes_no_autosave() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space); // Ace say
    press_and_update(&mut app, KeyCode::F5);
    assert!(manual_path(&app).exists());

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F9);

    assert!(
        !autosave_path(&app).exists(),
        "loading never writes the autosave"
    );
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Say { .. })
    ));
}

#[test]
fn manual_save_failure_leaves_the_slot_byte_identical_and_notices_row_14() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);

    // A first save creates the slot, so "unchanged" has bytes to compare.
    press_and_update(&mut app, KeyCode::F5);
    let slot = manual_path(&app);
    let before = std::fs::read(&slot).unwrap();

    // Block the atomic write: the temp sibling is a directory, so `write`
    // fails before the slot could be touched.
    std::fs::create_dir(tmp_path(&slot)).expect("the temp path is free");

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    press_and_update(&mut app, KeyCode::F5);

    assert_eq!(
        std::fs::read(&slot).unwrap(),
        before,
        "a failed save leaves the slot byte-identical"
    );
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0]
            .2
            .starts_with("couldn't save — the slot is unchanged"),
        "{:?}",
        records[0].2
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't save — the slot is unchanged")
    );
}

#[test]
fn a_failed_autosave_notices_per_write_and_stays_silent_on_success() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);
    let tmp = tmp_path(&autosave_path(&app));
    std::fs::create_dir_all(&tmp).expect("the temp path is free");

    // First write point: a choice resolve.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    press_and_update(&mut app, KeyCode::Digit1);
    let first = failure_lines();
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0].0, log::Level::Error);
    assert!(
        first[0].2.starts_with("couldn't autosave"),
        "{:?}",
        first[0].2
    );

    // Second write point: advancing across the scene boundary.
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    press_and_update(&mut app, KeyCode::Space);
    let second = failure_lines();
    assert_eq!(second.len(), 1, "one line per failed write: {second:?}");
    assert!(
        second[0].2.starts_with("couldn't autosave"),
        "{:?}",
        second[0].2
    );
    assert!(app.world().resource::<Notice>().is_showing());

    // Clear the block: the next crossing's write lands and stays silent.
    std::fs::remove_dir(&tmp).unwrap();
    press_and_update(&mut app, KeyCode::Space); // decision fallback, parks
    captures::take();
    press_and_update(&mut app, KeyCode::Space); // epilogue crossing
    assert!(autosave_path(&app).exists(), "the successful write landed");
    let records = failure_lines();
    assert!(
        !records
            .iter()
            .any(|(_, _, line)| line.starts_with("couldn't autosave")),
        "a successful autosave stays silent: {records:?}"
    );
}

#[test]
fn an_unreadable_slot_refuses_to_load_with_its_reason() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);
    let slot = manual_path(&app);
    let garbage = b"definitely not a save";
    std::fs::write(&slot, garbage).unwrap();

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    press_and_update(&mut app, KeyCode::F9);

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("invalid save file"),
        "row 16 logs the slot's own reason: {:?}",
        records[0].2
    );
    assert!(
        records[0].2.contains(&slot.display().to_string()),
        "the reason is followed by the slot path: {:?}",
        records[0].2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "row 16 has no Notice — the slot card is its surface"
    );
    assert_eq!(std::fs::read(&slot).unwrap(), garbage);
    press_and_update(&mut app, KeyCode::F5);
    assert!(save::read_slot(
        app.world().resource::<Runtime>().runner.pack(),
        Slot::Manual(1)
    )
    .is_ok());
}

#[test]
fn a_load_refused_at_restore_time_notices_row_17() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // A save that names a scene the loaded pack lacks: `read_slot` would refuse
    // it while classifying, so this drives the restore-time site directly, the
    // "slot changed under the overlay" race row 17 names.
    let mut save = {
        let runtime = app.world().resource::<Runtime>();
        SaveFile::capture(&runtime.runner, &runtime.staging)
    };
    save.scene_id = "nowhere".to_string();
    let path = PathBuf::from("nowhere.slot1.save.ron");
    // A `Mutex<Option<_>>` keeps the system closure `FnMut` (a Bevy system) while
    // still handing `save` to the one call.
    let save = std::sync::Mutex::new(Some(save));

    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;
    let outcome = app
        .world_mut()
        .run_system_once(move |mut session: SessionInput| {
            let save = save
                .lock()
                .expect("the test closure lock is never poisoned")
                .take()
                .expect("the system runs once");
            session.transitions().restore_loaded(save, &path)
        })
        .expect("the one-shot restore system runs");
    assert_eq!(
        outcome,
        LoadOutcome::Refused,
        "the restore refusal reports its own outcome"
    );

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("couldn't load that save"),
        "{:?}",
        records[0].2
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save")
    );
}

/// Drive one typed load through the production session bridge, exactly as the
/// F9 shortcut and the overlay's load path do.
fn run_session_load(app: &mut App, slot: Slot, origin: LoadOrigin) -> LoadOutcome {
    app.world_mut()
        .run_system_once(move |mut session: SessionInput| session.transitions().load(slot, origin))
        .expect("the one-shot load system runs")
}

/// The typed origin alone selects the read-stage policy, and the outcome
/// vocabulary is what the menu's success-only closure branches on. Real engine
/// IO in the fixture's temporary pack directory: an absent dev slot, an
/// unreadable slot read by the shortcut, and the same unreadable slot read by a
/// menu origin.
#[test]
fn typed_load_origins_route_the_read_stages_to_their_own_outcomes_and_rows() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let slot = Slot::Manual(1);
    let path = manual_path(&app);

    // F9 with no file: the clean no-op — no log, no Notice.
    captures::take();
    assert_eq!(
        run_session_load(&mut app, slot, LoadOrigin::DevShortcut),
        LoadOutcome::Missing
    );
    assert!(
        failure_lines().is_empty(),
        "an absent dev slot logs nothing"
    );
    assert!(!app.world().resource::<Notice>().is_showing());

    // Present but unreadable: the dev origin routes row 16 with the engine's
    // reason and the slot path, Notice-less.
    std::fs::write(&path, b"not a save").unwrap();
    captures::take();
    assert_eq!(
        run_session_load(&mut app, slot, LoadOrigin::DevShortcut),
        LoadOutcome::Refused
    );
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("invalid save file"),
        "row 16 logs the slot's own reason: {:?}",
        records[0].2
    );
    assert!(
        records[0].2.contains(&path.display().to_string()),
        "the reason is followed by the slot path: {:?}",
        records[0].2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "row 16 has no Notice — the slot card is its surface"
    );

    // The same unreadable slot through a menu origin routes the existing row 17
    // with its Notice.
    app.world_mut().resource_mut::<Notice>().text = None;
    captures::take();
    assert_eq!(
        run_session_load(&mut app, slot, LoadOrigin::Menu),
        LoadOutcome::Refused
    );
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("couldn't load that save"),
        "{:?}",
        records[0].2
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save")
    );
}

fn legacy_path(app: &App) -> PathBuf {
    let runtime = app.world().resource::<Runtime>();
    save::legacy_save_path_for(runtime.runner.pack())
}

/// Write an importable legacy save: the app-captured `SaveFile` with the
/// legacy file's name and no stamp (the shape the slice's F5 wrote).
fn write_legacy(app: &App, saved_at: Option<u64>) {
    let runtime = app.world().resource::<Runtime>();
    let mut save = SaveFile::capture(&runtime.runner, &runtime.staging);
    save.saved_at = saved_at;
    save::write_save(&save::legacy_save_path_for(runtime.runner.pack()), &save).unwrap();
}

/// Open an app whose pack has an importable legacy save sitting beside it.
fn app_with_legacy() -> (App, tempfile::TempDir) {
    let pack = temp_pack();
    let app = app_for_pack(pack.path().join("pack"));
    write_legacy(&app, None);
    (app, pack)
}

#[test]
fn the_legacy_card_is_shown_only_in_save_mode_while_importable() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);
    assert!(!menu::legacy_visible(menu(&app)), "no file, no card");

    let (mut app, _pack) = app_with_legacy();
    esc(&mut app);
    assert!(menu::legacy_visible(menu(&app)));

    switch_mode(&mut app, MenuMode::Load);
    assert!(
        !menu::legacy_visible(menu(&app)),
        "load mode hides the card"
    );
    assert!(legacy_path(&app).exists());

    click(&mut app, MenuAction::OpenSettings);
    assert!(!menu::legacy_visible(menu(&app)), "settings hides the card");
    click(&mut app, MenuAction::BackToSlots);
    click(&mut app, MenuAction::SetMode(MenuMode::Save));
    assert!(
        menu::legacy_visible(menu(&app)),
        "back in save mode it returns"
    );

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::Card(2);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::LegacyCard);
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::Card(2));

    let (app, _pack) = app_with_legacy();
    let mut app = app;
    let before = b"not a save at all".to_vec();
    std::fs::write(legacy_path(&app), &before).unwrap();
    esc(&mut app);
    assert!(!menu::legacy_visible(menu(&app)), "unreadable is hidden");
    assert_eq!(std::fs::read(legacy_path(&app)).unwrap(), before);
}

#[test]
fn legacy_picker_labels_occupied_targets_as_overwrite() {
    let (mut app, _pack) = app_with_legacy();
    // Fill slot 1 (empty -> Save here) and slot 3 (unreadable).
    press_and_update(&mut app, KeyCode::F5);
    std::fs::write(slot_path(&app, Slot::Manual(3)), b"garbage").unwrap();
    esc(&mut app);

    click(&mut app, MenuAction::LegacyImport);
    assert_eq!(menu(&app).picker, Some(menu::LegacyPicker { target: 0 }));
    assert_eq!(menu(&app).focus, MenuFocus::LegacyTarget(0));

    let targets = menu::legacy_targets(menu(&app));
    assert_eq!(targets.len(), 3, "the three manual slots");
    assert!(targets[0].overwrite, "slot 1 is occupied");
    assert!(targets[0].display().contains("(overwrite)"));
    assert!(!targets[1].overwrite, "slot 2 is empty");
    assert!(!targets[1].display().contains("(overwrite)"));
    assert!(
        targets[2].overwrite,
        "slot 3 is unreadable, still overwritable"
    );

    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::LegacyTarget(1));
    esc(&mut app);
    assert_eq!(menu(&app).picker, None);
    assert!(menu(&app).open, "Esc cancelled the picker, not the overlay");
    assert_eq!(menu(&app).focus, MenuFocus::LegacyCard);

    let before = std::fs::read(legacy_path(&app)).unwrap();
    click(&mut app, MenuAction::LegacyImport);
    switch_mode(&mut app, MenuMode::Load);
    assert_eq!(menu(&app).picker, None, "load mode clears the picker");
    click(&mut app, MenuAction::LegacyTarget(Slot::Manual(2)));
    assert!(
        !slot_path(&app, Slot::Manual(2)).exists(),
        "no import in load mode"
    );
    assert_eq!(std::fs::read(legacy_path(&app)).unwrap(), before);
}

#[test]
fn importing_the_legacy_save_writes_the_target_and_retires_the_card() {
    let (mut app, _pack) = app_with_legacy();
    let legacy = legacy_path(&app);
    let original = std::fs::read(&legacy).unwrap();
    esc(&mut app);

    click(&mut app, MenuAction::LegacyImport);
    click(&mut app, MenuAction::LegacyTarget(Slot::Manual(2)));

    assert!(menu(&app).picker.is_none());
    assert!(!menu::legacy_visible(menu(&app)));
    let runtime = app.world().resource::<Runtime>();
    let imported = save::read_slot(runtime.runner.pack(), Slot::Manual(2)).unwrap();
    assert_eq!(imported.saved_at, None, "the import fabricates no time");
    assert_eq!(imported.wait, runtime.runner.wait().cloned());

    assert!(matches!(
        menu(&app).slots[1].status,
        SlotStatus::Ok { saved_at: None, .. }
    ));
    let model = menu::card_model(&menu(&app).slots[1], MenuMode::Save);
    assert_eq!(model.saved_at, "—", "an imported v1 save shows a dash");

    // The rename retires the entry without deleting the bytes.
    assert!(!legacy.exists());
    assert!(!menu::legacy_visible(menu(&app)));
    let imported_path = save::imported_legacy_path_for(runtime.runner.pack());
    assert_eq!(std::fs::read(&imported_path).unwrap(), original);

    esc(&mut app);
    esc(&mut app);
    esc(&mut app);
    assert!(!menu::legacy_visible(menu(&app)));
}

#[test]
fn a_failed_legacy_import_notices_row_18_and_leaves_the_card() {
    captures::install();
    let (mut app, _pack) = app_with_legacy();
    force_single_threaded(&mut app);
    let legacy = legacy_path(&app);
    let before = std::fs::read(&legacy).unwrap();
    esc(&mut app);

    // Block the target write: slot 1's atomic temp path is a directory.
    std::fs::create_dir_all(tmp_path(&slot_path(&app, Slot::Manual(1)))).unwrap();
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    click(&mut app, MenuAction::LegacyImport);
    click(&mut app, MenuAction::LegacyTarget(Slot::Manual(1)));

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("couldn't import the legacy save"),
        "{:?}",
        records[0].2
    );
    assert!(app.world().resource::<Notice>().is_showing());
    assert!(menu::legacy_visible(menu(&app)), "the card stays");
    assert!(menu(&app).picker.is_some(), "the picker stays for a retry");
    assert_eq!(std::fs::read(&legacy).unwrap(), before, "legacy untouched");
}
