use super::*;

/// [`boot_app`], but with the `Startup` schedule pinned to one thread so a
/// `Startup`-time failure log lands on the test thread's capture buffer.
fn boot_app_capturing(pack_root: PathBuf) -> App {
    let mut app = boot_app_unstarted(pack_root, None);
    app.edit_schedule(Startup, |schedule| {
        schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    });
    app.update();
    app
}

#[test]
fn the_boot_park_behind_the_title_marks_nothing_and_writes_nothing() {
    let pack = temp_pack();
    let mut app = boot_app(pack.path().join("pack"), None);

    let runtime = app.world().resource::<Runtime>();
    assert!(matches!(runtime.runner.wait(), Some(Wait::Say { .. })));
    assert_eq!(runtime.runner.scene_id(), "first_meeting");

    let state = read_state(&app);
    assert!(
        !state.is_visited("first_meeting"),
        "the boot park is not play"
    );
    assert!(!state.is_read("first_meeting", 7));
    assert!(!state.is_dirty());
    assert!(
        !read_log_path(&app).exists(),
        "looking at the title gains no file"
    );

    enter_play(&mut app);
    let state = read_state(&app);
    assert!(state.is_visited("first_meeting"));
    assert!(state.is_read("first_meeting", 7));
    assert_eq!(state.arrival_read(), Some(false));
    assert!(read_log_path(&app).exists(), "play entry flushed the marks");

    let on_disk = read_log_on_disk(&app).unwrap();
    assert!(on_disk.is_visited("first_meeting"));
    assert!(on_disk.is_read("first_meeting", 7));
}

#[test]
fn a_second_visit_to_a_line_reports_arrival_read() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");

    let first = app_for_pack(pack_root.clone());
    assert_eq!(read_state(&first).arrival_read(), Some(false));
    assert!(read_state(&first).is_read("first_meeting", 7));
    drop(first);

    // Second launch: the same line is already known, so it reads as read.
    let second = app_for_pack(pack_root);
    assert_eq!(read_state(&second).arrival_read(), Some(true));
}

#[test]
fn a_first_visit_marks_and_reports_arrival_unread_in_the_same_step() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Entry say -> the Ace say is the next park.
    press_and_update(&mut app, KeyCode::Space);
    let state = read_state(&app);
    assert!(state.is_read("first_meeting", 10));
    assert_eq!(
        state.arrival_read(),
        Some(false),
        "the line is marked but arrival was captured before the mark"
    );

    // Back to the entry line with a New Game: now it is arrival-read.
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
}

#[test]
fn non_say_parks_mark_nothing_and_arrival_is_absent() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert_eq!(read_state(&app).arrival_read(), None);
    let before = read_state(&app).log.clone();

    // The choice is a flush moment, but a choice-wait itself marks nothing.
    press_and_update(&mut app, KeyCode::Digit1);
    let after = &read_state(&app).log;
    assert!(
        after.is_read("first_meeting", 21),
        "the line the choice lands on is marked"
    );
    assert_eq!(
        before.read.get("first_meeting"),
        Some(&std::collections::BTreeSet::from([7, 10])),
        "the choice-wait added no say mark"
    );
}

#[test]
fn a_decision_wait_marks_nothing_and_a_model_landing_marks_its_line() {
    let pack = temp_pack();
    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = boot_app(pack.path().join("pack"), Some(Arc::new(fake.clone())));
    enter_play(&mut app);

    // Play to the reunion decision point through the real input path.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // crossing into reunion
    let before_decision = read_state(&app).log.clone();
    press_and_update(&mut app, KeyCode::Space); // reunion say -> decision
    assert!(matches!(wait(&app), Some(Wait::Decision { .. })));
    pump_until(&mut app, |_| fake.calls() == 1);
    assert_eq!(read_state(&app).arrival_read(), None);
    assert_eq!(
        read_state(&app).log,
        before_decision,
        "the decision-wait itself marks nothing"
    );

    // The model answer lands `warm` on reunion line 14 and flushes the mark.
    fake.release();
    pump_until(&mut app, parked_on_say);
    let state = read_state(&app);
    assert!(state.is_read("reunion", 14));
    assert!(state.is_visited("reunion"));
    assert_eq!(state.arrival_read(), Some(false));
    assert!(
        read_log_on_disk(&app).unwrap().is_read("reunion", 14),
        "a resolved decision is a read-log flush moment"
    );
}

#[test]
fn a_same_scene_label_jump_marks_no_scene() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Entry -> Ace -> choice; choosing `ask_letter` is a same-scene label jump.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    let state = read_state(&app);
    assert_eq!(
        state.log.visited,
        std::collections::BTreeSet::from(["first_meeting".to_string()]),
        "a local label jump is not a crossing"
    );
    assert!(state.is_read("first_meeting", 21));
}

#[test]
fn a_crossing_marks_the_target_scene_visited() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // ask_letter say -> jump reunion
    assert!(read_state(&app).is_visited("reunion"));
    assert!(read_state(&app).is_read("reunion", 7));
}

#[test]
fn a_load_marks_the_restored_scene_visited_even_when_it_was_never_played() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root);
    assert!(!read_state(&app).is_visited("reunion"));

    // A save copied in beside the pack, pointing at reunion's first say.
    let pack_res = app.world().resource::<Runtime>().runner.pack().clone();
    let save = SaveFile {
        version: save::SAVE_VERSION,
        state: pack_res.state.clone(),
        scene_id: "reunion".to_string(),
        pc: 3,
        wait: Some(Wait::Say {
            speaker: "Narrator".to_string(),
            text: "The gate again, hours later. She's still there.".to_string(),
        }),
        staging: StagingState::default(),
        saved_at: None,
    };
    save::write_slot(&pack_res, Slot::Manual(1), &save).unwrap();

    press_and_update(&mut app, KeyCode::F9);
    let state = read_state(&app);
    assert_eq!(
        state.log.visited,
        std::collections::BTreeSet::from(["first_meeting".to_string(), "reunion".to_string()])
    );
    assert!(
        state.is_read("reunion", 7),
        "the restored say-line is marked"
    );
}

#[test]
fn marks_wait_for_a_flush_moment_and_overlay_browsing_writes_nothing() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let after_entry = read_log_on_disk(&app).unwrap();

    // A plain advance marks but is not a flush moment.
    press_and_update(&mut app, KeyCode::Space);
    assert!(read_state(&app).is_read("first_meeting", 10));
    assert_eq!(read_log_on_disk(&app).unwrap(), after_entry, "no flush yet");

    // Opening and closing the overlay adds no flush opportunity.
    press_and_update(&mut app, KeyCode::Escape);
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(
        read_log_on_disk(&app).unwrap(),
        after_entry,
        "overlay browsing writes nothing"
    );

    // The choice resolve is the next flush moment, and it carries the deferred
    // line-10 mark.
    press_and_update(&mut app, KeyCode::Space); // choice park
    press_and_update(&mut app, KeyCode::Digit1);
    let flushed = read_log_on_disk(&app).unwrap();
    assert!(flushed.is_read("first_meeting", 10));
    assert!(flushed.is_read("first_meeting", 21));
}

#[test]
fn a_fallback_decision_flushes_the_read_log_but_leaves_the_autosave_untouched() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // crossing arms + writes the autosave
    assert!(autosave_path(&app).exists());
    let autosave_before = std::fs::read(autosave_path(&app)).unwrap();
    let read_before = read_log_on_disk(&app).unwrap();

    press_and_update(&mut app, KeyCode::Space); // reunion say -> decision
    app.update(); // the author fallback resolves and lands

    let read_after = read_log_on_disk(&app).unwrap();
    assert!(
        read_after.is_read("reunion", 19),
        "the fallback flush carried the landing mark"
    );
    assert_ne!(read_after, read_before, "the read log was flushed");
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        autosave_before,
        "a fallback answer never churns the autosave"
    );
}

#[test]
fn app_exit_flushes_pending_read_marks() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Mark a line without a flush moment.
    press_and_update(&mut app, KeyCode::Space);
    assert!(!read_log_on_disk(&app).unwrap().is_read("first_meeting", 10));

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();

    assert!(read_log_on_disk(&app).unwrap().is_read("first_meeting", 10));
    assert!(!read_state(&app).is_dirty());
}

#[test]
fn a_missing_file_is_empty_knowledge_with_no_failure() {
    captures::install();
    let pack = temp_pack();
    let app = boot_app_capturing(pack.path().join("pack"));
    assert!(
        failure_lines().is_empty(),
        "a missing file is first-run state"
    );
    assert!(!read_state(&app).is_dirty());
}

#[test]
fn an_undecodable_file_yields_empty_knowledge_and_one_warn_only_report() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    std::fs::write(&path, "this is not ron {").unwrap();
    let before = std::fs::read(&path).unwrap();

    let app = boot_app_capturing(pack_root);
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't use the read log"),
        "{:?}",
        records[0].2
    );
    assert!(!app.world().resource::<Notice>().is_showing());
    assert!(!app.world().resource::<DefensiveHold>().is_held());
    assert_eq!(std::fs::read(&path).unwrap(), before, "bytes untouched");
    assert!(read_state(&app).log.read.is_empty());
}

#[test]
fn a_wrong_version_file_yields_empty_knowledge_and_one_warn_only_report() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    std::fs::write(
        &path,
        "(version: 2, read: {\"reunion\": [7]}, visited: [\"reunion\"])",
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();

    let app = boot_app_capturing(pack_root);
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(!read_state(&app).is_read("reunion", 7));
    assert!(!app.world().resource::<Notice>().is_showing());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "the file is not rewritten"
    );

    // The next ordinary dirty write replaces the wrong version in place.
    let mut app = app;
    enter_play(&mut app);
    let on_disk = read_log_on_disk(&app).unwrap();
    assert_eq!(on_disk.version, unmei_engine::read_log::READ_LOG_VERSION);
    assert!(on_disk.is_read("first_meeting", 7));
}

#[test]
fn launch_filtering_drops_unusable_entries_without_writing() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut seeded = unmei_engine::read_log::ReadLog::default();
    // A loaded scene with a valid say line, a line 0, an out-of-range line, a
    // command's source line (first_meeting line 4 is `bg`), and an unknown scene.
    seeded.mark_read("first_meeting", 7);
    seeded.mark_read("first_meeting", 0);
    seeded.mark_read("first_meeting", 9999);
    seeded.mark_read("first_meeting", 4);
    seeded.mark_read("ghost_scene", 3);
    seeded.mark_visited("first_meeting");
    seeded.mark_visited("ghost_scene");
    seed_read_log(&pack_root, &seeded);
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    let before = std::fs::read(&path).unwrap();

    let app = boot_app(pack_root, None);
    let state = read_state(&app);
    assert!(state.is_read("first_meeting", 7));
    assert!(!state.is_read("first_meeting", 0));
    assert!(!state.is_read("first_meeting", 9999));
    assert!(
        !state.is_read("first_meeting", 4),
        "a command line is not a say-line"
    );
    assert!(!state.is_read("ghost_scene", 3));
    assert!(!state.is_visited("ghost_scene"));
    assert!(!state.is_dirty(), "filtering alone does not dirty the log");
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "filtering alone writes nothing"
    );

    // The invalid entries disappear on the next ordinary dirty write.
    let mut app = app;
    enter_play(&mut app);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "the clean entry wrote nothing"
    );
    press_and_update(&mut app, KeyCode::Space); // mark line 10 (dirty)
    press_and_update(&mut app, KeyCode::Space); // choice park
    press_and_update(&mut app, KeyCode::Digit1); // flush moment
    let written = read_log_on_disk(&app).unwrap();
    assert_eq!(
        written.read.get("first_meeting"),
        Some(&std::collections::BTreeSet::from([7, 10, 21])),
        "only valid say-lines survive the write"
    );
    assert!(!written.read.contains_key("ghost_scene"));
    assert_eq!(
        written.visited,
        std::collections::BTreeSet::from(["first_meeting".to_string()])
    );
}

#[test]
fn the_log_is_read_once_per_launch_and_never_again() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut seeded = unmei_engine::read_log::ReadLog::default();
    seeded.mark_read("reunion", 7);
    seed_read_log(&pack_root, &seeded);

    let mut app = boot_app(pack_root.clone(), None);
    assert!(read_state(&app).is_read("reunion", 7));

    // Overwrite the file mid-session; the in-memory log must not change.
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    let mut replacement = unmei_engine::read_log::ReadLog::default();
    replacement.mark_read("first_meeting", 10);
    unmei_engine::read_log::write_read_log(&path, &replacement).unwrap();

    enter_play(&mut app);
    assert!(
        read_state(&app).is_read("reunion", 7),
        "the launch read is the only read"
    );
    assert!(!read_state(&app).is_read("first_meeting", 10));
}

#[test]
fn a_broken_pack_neither_reads_nor_writes_the_read_log() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut seeded = unmei_engine::read_log::ReadLog::default();
    seeded.mark_read("reunion", 7);
    seed_read_log(&pack_root, &seeded);
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    let before = std::fs::read(&path).unwrap();

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(
            RuntimeConfig::new(PathBuf::from("definitely/not/a/pack"), None).with_dev_mode(true),
        )
        .add_plugins(RuntimeCorePlugin);
    app.update();
    app.update();

    assert!(app.world().get_resource::<Runtime>().is_none());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "no write on a rejected load"
    );
}

#[test]
fn a_failed_flush_logs_once_preserves_bytes_and_retries_at_the_next_moment() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);
    let path = read_log_path(&app);
    let after_entry = std::fs::read(&path).unwrap();

    // Block the atomic write by putting a directory where the temp sibling goes.
    let tmp = path.with_extension("ron.tmp");
    std::fs::create_dir(&tmp).unwrap();
    captures::take();

    // Mark a line, then reach a flush moment (the choice resolve).
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);

    let records = failure_lines();
    assert_eq!(
        records.len(),
        1,
        "one warn per failed occurrence: {records:?}"
    );
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(records[0].2.starts_with("couldn't use the read log"));
    assert!(!app.world().resource::<Notice>().is_showing());
    assert!(!app.world().resource::<DefensiveHold>().is_held());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        after_entry,
        "bytes preserved"
    );
    assert!(read_state(&app).is_dirty(), "the dirty flag survives");

    // Unblock and reach the next flush moment: the retry succeeds.
    std::fs::remove_dir(&tmp).unwrap();
    press_and_update(&mut app, KeyCode::Space); // crossing into reunion
    assert!(!read_state(&app).is_dirty());
    assert!(read_log_on_disk(&app).unwrap().is_read("first_meeting", 10));
}

#[test]
fn new_game_retains_knowledge_and_only_a_dirty_entry_writes() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root);

    // A pending dirty mark: the entry flush wrote [7]; advancing marks 10 with
    // no flush moment in between.
    press_and_update(&mut app, KeyCode::Space);
    assert!(read_state(&app).is_dirty());
    let dirty_bytes = std::fs::read(read_log_path(&app)).unwrap();
    assert!(!read_log_on_disk(&app).unwrap().is_read("first_meeting", 10));

    // Exit to title adds no flush opportunity: the pending mark waits.
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap(),
        dirty_bytes,
        "Exit to title writes nothing"
    );

    // Start a New Game: the pending knowledge survives and is flushed with the
    // entry marks.
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    let flushed = read_log_on_disk(&app).unwrap();
    assert!(
        flushed.is_read("first_meeting", 7),
        "existing knowledge retained"
    );
    assert!(
        flushed.is_read("first_meeting", 10),
        "pending knowledge flushed"
    );

    // Now the log is clean and the entry is already known: a second New Game
    // adds nothing and leaves the bytes identical.
    let clean = std::fs::read(read_log_path(&app)).unwrap();
    assert!(!read_state(&app).is_dirty());
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap(),
        clean,
        "a clean, already-known entry writes nothing"
    );
}

#[test]
fn an_overlay_pause_preserves_the_arrival_status() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Back onto the known entry line: arrival-read.
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(read_state(&app).arrival_read(), Some(true));

    // A presentation pause is not a discontinuity: opening and closing the
    // overlay leaves the park and its arrival status alone.
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
}

#[test]
fn a_load_derives_arrival_from_the_log_not_the_park_it_replaced() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root);
    // Land back on the known entry line so the current park reads arrival-true.
    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(read_state(&app).arrival_read(), Some(true));

    // A save copied in beside the pack, pointing at reunion's first say, which
    // this session has never read.
    let pack_res = app.world().resource::<Runtime>().runner.pack().clone();
    let save = SaveFile {
        version: save::SAVE_VERSION,
        state: pack_res.state.clone(),
        scene_id: "reunion".to_string(),
        pc: 3,
        wait: Some(Wait::Say {
            speaker: "Narrator".to_string(),
            text: "The gate again, hours later. She's still there.".to_string(),
        }),
        staging: StagingState::default(),
        saved_at: None,
    };
    save::write_slot(&pack_res, Slot::Manual(1), &save).unwrap();

    press_and_update(&mut app, KeyCode::F9);
    assert_eq!(
        read_state(&app).arrival_read(),
        Some(false),
        "arrival is recomputed from the log, never carried across the load"
    );
    assert!(
        read_state(&app).is_read("reunion", 7),
        "the restored line marks"
    );
}

#[test]
fn continue_resuming_a_clean_session_writes_nothing() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root);

    // Flush a clean stretch so the log is settled.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    let clean = std::fs::read(read_log_path(&app)).unwrap();
    assert!(!read_state(&app).is_dirty());

    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::OpenSettings);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(live_session(&app));
    click(&mut app, MenuAction::Continue);
    assert!(!menu(&app).open, "Continue resumed the live session");
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap(),
        clean,
        "a clean Continue-resume writes nothing"
    );
}

#[test]
fn empty_knowledge_can_become_arrival_read_later_in_the_same_launch() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // First arrival to the entry line is unread.
    assert_eq!(read_state(&app).arrival_read(), Some(false));

    press_and_update(&mut app, KeyCode::Escape);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(
        read_state(&app).arrival_read(),
        Some(true),
        "read-once does not freeze knowledge for the whole launch"
    );
}

#[test]
fn an_io_unreadable_read_log_yields_empty_knowledge_and_one_warn_only_report() {
    captures::install();
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = pack_root.parent().unwrap().join("pack.read.ron");
    // A directory at the log path is IO-unreadable: `read_to_string` fails.
    std::fs::create_dir(&path).unwrap();

    let app = boot_app_capturing(pack_root);
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(records[0].2.starts_with("couldn't use the read log"));
    assert!(!app.world().resource::<Notice>().is_showing());
    assert!(read_state(&app).log.read.is_empty());
    assert!(
        path.is_dir(),
        "the unreadable target is not deleted or renamed"
    );
}

#[test]
fn the_end_of_scene_park_marks_no_say_line() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    play_to_end(&mut app);

    assert!(wait(&app).is_none(), "the script ended");
    assert_eq!(
        read_state(&app).arrival_read(),
        None,
        "the end-of-scene marker is not a say-park"
    );
}

#[test]
fn a_held_fallback_answer_flushes_the_read_log_on_resume() {
    let pack = temp_pack();
    let fake = FakeResolver::new(fallback_answer("cold"));
    let mut app = boot_app(pack.path().join("pack"), Some(Arc::new(fake.clone())));
    enter_play(&mut app);

    // Play to the reunion decision point; the crossing wrote the autosave.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // crossing into reunion
    press_and_update(&mut app, KeyCode::Space); // reunion say -> decision
    pump_until(&mut app, |_| fake.calls() == 1);
    let autosave = std::fs::read(autosave_path(&app)).unwrap();
    let read_before = read_log_on_disk(&app).unwrap();

    // Open the overlay, then let the author fallback answer: it is held, not
    // applied, so neither log changes yet.
    press_and_update(&mut app, KeyCode::Escape);
    fake.release();
    pump_until(&mut app, |app| {
        app.world().resource::<HeldDecision>().0.is_some()
    });
    assert_eq!(read_log_on_disk(&app).unwrap(), read_before);
    assert_eq!(std::fs::read(autosave_path(&app)).unwrap(), autosave);

    // Resume: the held fallback applies, marks its landing line and flushes the
    // read log — while still leaving the autosave alone.
    press_and_update(&mut app, KeyCode::Escape);
    pump_until(&mut app, parked_on_say);
    assert!(
        read_log_on_disk(&app).unwrap().is_read("reunion", 19),
        "the held fallback flushed its knowledge"
    );
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        autosave,
        "a fallback answer never churns the autosave"
    );
}

/// Seed the read log with exactly the listed `(scene, source line)` say-lines,
/// marking each named scene visited.
fn seed_read_lines(pack_root: &Path, entries: &[(&str, &[usize])]) {
    let mut log = unmei_engine::read_log::ReadLog::default();
    for (scene_id, lines) in entries {
        log.mark_visited(scene_id);
        for line in *lines {
            log.mark_read(scene_id, *line);
        }
    }
    seed_read_log(pack_root, &log);
}

/// An entry scene whose two say-lines are separated by a staging command.
fn pack_with_staged_commands() -> tempfile::TempDir {
    let pack = temp_pack();
    std::fs::write(
        pack.path().join("pack/scenes/first_meeting.unmei"),
        "bg school_gate\nsprite ace neutral\nNarrator: One.\nsprite ace happy\nNarrator: Two.\n",
    )
    .unwrap();
    pack
}

/// An entry scene that jumps into a second scene.
fn pack_with_crossing() -> tempfile::TempDir {
    let pack = temp_pack();
    std::fs::write(
        pack.path().join("pack/scenes/first_meeting.unmei"),
        "bg school_gate\nNarrator: One.\nNarrator: Two.\njump second_scene\n",
    )
    .unwrap();
    std::fs::write(
        pack.path().join("pack/scenes/second_scene.unmei"),
        "Narrator: Three.\nNarrator: Four.\n",
    )
    .unwrap();
    pack
}

#[test]
fn the_skip_dwell_is_fifty_ms_and_unscaled() {
    assert_eq!(crate::core::SKIP_DWELL_SECONDS, 0.05);
}

#[test]
fn tab_engages_only_on_an_arrival_read_say_park() {
    let pack = temp_pack();
    let root = pack.path().join("pack");
    let mut app = app_for_pack(root.clone());
    build_ui(&mut app);
    assert_eq!(read_state(&app).arrival_read(), Some(false));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(!skip_on(&app), "an unread arrival cannot engage");

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(!skip_on(&app), "a choice-wait cannot engage");

    let known = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut known_app = app_with_known_entry(&known);
    build_ui(&mut known_app);
    assert_eq!(read_state(&known_app).arrival_read(), Some(true));
    press_and_update(&mut known_app, KeyCode::Tab);
    assert!(skip_on(&known_app));
    press_and_update(&mut known_app, KeyCode::Tab);
    assert!(!skip_on(&known_app));
}

#[test]
fn tab_engages_mid_reveal() {
    let pack = pack_with_entry_says(&[
        "Narrator: One two three four five six seven eight.",
        "Narrator: Two.",
    ]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = app_for_pack(root);
    build_ui(&mut app);
    set_text_speed(&mut app, 0.02);
    render_dialogue(&mut app);
    advance_time(&mut app, 0.1);
    render_dialogue(&mut app);
    assert!(!app.world().resource::<TextReveal>().is_complete(0.02));

    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app), "mid-reveal counts for the gate");
}

#[test]
fn an_instant_line_advances_one_step_per_fifty_ms() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).expect("parked on the first say");
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    assert_eq!(wait(&app), Some(first.clone()));

    advance_time(&mut app, 0.04);
    tick_update(&mut app);
    assert_eq!(wait(&app), Some(first.clone()), "0.04 s is short of a step");

    advance_time(&mut app, 0.02);
    tick_update(&mut app);
    assert_ne!(wait(&app), Some(first), "one advance at 0.05 s");
}

#[test]
fn the_engagement_frame_never_advances_even_under_a_hitch() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();

    // Engage and step in one frame whose delta is five seconds. The completed
    // line must still be shown for its dwell's opening frame, not advanced
    // away before it ever renders.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Tab);
    advance_time(&mut app, 5.0);
    tick_update(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();

    assert!(skip_on(&app));
    assert_eq!(
        wait(&app),
        Some(first.clone()),
        "the engage frame completes the line but never advances it"
    );

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(wait(&app), Some(first), "then one step advances");
}

#[test]
fn a_one_second_reveal_is_completed_and_advanced_in_a_single_step() {
    let pack = pack_with_entry_says(&[
        "Narrator: One two three four five six seven.",
        "Narrator: Eight.",
    ]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = app_for_pack(root);
    set_text_speed(&mut app, 0.02);
    render_dialogue(&mut app);
    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    assert!(!app.world().resource::<TextReveal>().is_complete(0.02));
    let line = wait(&app).unwrap();

    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    assert_eq!(wait(&app), Some(line.clone()), "engaging never advances");
    assert!(
        app.world().resource::<TextReveal>().is_complete(0.02),
        "the step completes the reveal"
    );

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(wait(&app), Some(line), "then advances once");
}

#[test]
fn a_hitch_advances_once_and_never_catches_up() {
    let pack = pack_with_entry_says(&[
        "Narrator: One.",
        "Narrator: Two.",
        "Narrator: Three.",
        "Narrator: Four.",
    ]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();
    press_and_update(&mut app, KeyCode::Tab);

    advance_time(&mut app, 5.0);
    tick_update(&mut app);
    let second = wait(&app).unwrap();
    assert_ne!(second, first, "the hitch advanced exactly once");

    advance_time(&mut app, 0.0);
    tick_update(&mut app);
    assert_eq!(wait(&app), Some(second.clone()), "no banked catch-up");

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(wait(&app), Some(second), "a fresh step resumes normally");
}

#[test]
fn commands_between_parks_execute_inside_the_step() {
    let pack = pack_with_staged_commands();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[3, 5])]);
    let mut app = app_for_pack(root);
    assert_eq!(
        app.world()
            .resource::<Runtime>()
            .staging
            .sprites
            .get("ace")
            .map(String::as_str),
        Some("neutral")
    );
    press_and_update(&mut app, KeyCode::Tab);
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_eq!(
        app.world()
            .resource::<Runtime>()
            .staging
            .sprites
            .get("ace")
            .map(String::as_str),
        Some("happy"),
        "the staging command between the two skipped says landed in the step"
    );
}

#[test]
fn a_jump_crosses_inside_the_step_and_known_text_continues() {
    let pack = pack_with_crossing();
    let root = pack.path().join("pack");
    seed_read_lines(
        &root,
        &[("first_meeting", &[2, 3]), ("second_scene", &[1, 2])],
    );
    let mut app = app_for_pack(root);
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "first_meeting"
    );
    press_and_update(&mut app, KeyCode::Tab);

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "first_meeting"
    );
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "second_scene",
        "the jump crossed inside the skipped step"
    );
    assert!(skip_on(&app), "known text continues across the crossing");
    assert!(
        read_state(&app).is_visited("second_scene"),
        "the crossing marked the new scene visited"
    );
}

#[test]
fn a_crossing_stops_on_the_first_arrival_unread_line() {
    let pack = pack_with_crossing();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2, 3])]);
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Tab);

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "second_scene"
    );
    assert!(
        !skip_on(&app),
        "the first unread line ends skip on the landing frame"
    );

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(!skip_on(&app), "the first unread line stopped skip");
    assert!(
        read_state(&app).is_read("second_scene", 1),
        "the stop line is marked by its ordinary park"
    );
    assert_eq!(read_state(&app).arrival_read(), Some(false));
}

#[test]
fn skip_ends_at_a_choice_wait_without_choosing() {
    let pack = temp_pack();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[7, 10])]);
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    for _ in 0..4 {
        advance_time(&mut app, 0.05);
        tick_update(&mut app);
        if matches!(wait(&app), Some(Wait::Choice { .. })) {
            break;
        }
    }
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert!(
        !skip_on(&app),
        "a choice-wait ends skip on the frame it lands"
    );
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(!skip_on(&app), "skip holds at a choice-wait");
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
}

#[test]
fn a_space_press_ends_skip_and_advances_once() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    press_and_update(&mut app, KeyCode::Space);
    assert!(!skip_on(&app), "Space ends the toggle");
    assert_ne!(
        wait(&app),
        Some(first),
        "and performs exactly its own one advance"
    );
}

#[test]
fn a_space_mid_reveal_completes_and_does_not_step() {
    let pack = pack_with_entry_says(&["Narrator: One two three.", "Narrator: Four."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = app_for_pack(root);
    set_text_speed(&mut app, 0.02);
    press_and_update(&mut app, KeyCode::Tab);
    // Advance the skip onto the second line, then arm its reveal so it is
    // mid-reveal while the toggle is still running.
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    let line = wait(&app).unwrap();
    render_dialogue(&mut app);
    assert!(!app.world().resource::<TextReveal>().is_complete(0.02));

    press_and_update(&mut app, KeyCode::Space);
    assert!(!skip_on(&app), "the press ends the toggle");
    assert_eq!(wait(&app), Some(line), "it only completes, never advances");
    assert!(app.world().resource::<TextReveal>().is_complete(0.02));
}

#[test]
fn esc_ends_skip_before_the_overlay_and_closing_does_not_resume() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    build_ui(&mut app);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    esc(&mut app);
    assert!(menu(&app).open, "Esc opened the overlay");
    assert!(!skip_on(&app), "the toggle ended on the opening frame");

    esc(&mut app);
    assert!(!menu(&app).open);
    assert!(!skip_on(&app), "closing does not resurrect the toggle");
}

#[test]
fn f11_and_f5_leave_skip_running() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    press_and_update(&mut app, KeyCode::F11);
    assert!(skip_on(&app), "F11 does not cancel");
    press_and_update(&mut app, KeyCode::F5);
    assert!(skip_on(&app), "F5 does not cancel");
}

#[test]
fn a_ends_skip_and_toggles_auto() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_with_known_entry(&pack);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    assert!(!auto_on(&app));
    press_and_update(&mut app, KeyCode::KeyA);
    assert!(!skip_on(&app));
    assert!(auto_on(&app), "A is the explicit handoff");
}

#[test]
fn a_load_disarms_skip() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    press_and_update(&mut app, KeyCode::F5);
    assert!(skip_on(&app), "saving is not a discontinuity");
    press_and_update(&mut app, KeyCode::F9);
    assert!(!skip_on(&app), "a load disarms the toggle");
}

#[test]
fn an_empty_log_can_become_arrival_read_later_in_the_same_launch() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_for_pack(pack.path().join("pack"));
    assert_eq!(read_state(&app).arrival_read(), Some(false));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(!skip_on(&app), "first arrival cannot engage");

    // New Game returns to the same entry park, now arrival-read.
    esc(&mut app);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app), "the ordinary revisit is eligible");
}

#[test]
fn the_overlay_gate_freezes_skip_timing() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    press_and_update(&mut app, KeyCode::Tab);
    let line = wait(&app).unwrap();

    // Open without Esc (a defence-in-depth check of the shared gate).
    app.world_mut().resource_mut::<MenuState>().open = true;
    for _ in 0..3 {
        advance_time(&mut app, 1.0);
        tick_update(&mut app);
    }
    assert_eq!(wait(&app), Some(line.clone()), "no step under the overlay");

    app.world_mut().resource_mut::<MenuState>().open = false;
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(wait(&app), Some(line), "closing resumes the step");
}

#[test]
fn skip_suppresses_auto_without_changing_the_preference() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    build_ui(&mut app);
    enable_auto(&mut app);
    assert!(auto_on(&app));

    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    assert!(auto_on(&app), "the preference is untouched");

    let first = wait(&app).unwrap();
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(
        wait(&app),
        Some(first),
        "the line advances on skip's 0.05 s, not auto's 2.0 s"
    );
}

#[test]
fn ending_skip_restarts_a_full_auto_dwell() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    enable_auto(&mut app);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    // Space cancels skip and advances; auto is still on and must start fresh.
    press_and_update(&mut app, KeyCode::Space);
    assert!(!skip_on(&app));
    let line = wait(&app).unwrap();

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(wait(&app), Some(line.clone()), "no remainder carried in");

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), Some(line), "a full 2.0 s after skip ends");
}

#[test]
fn a_new_game_disarms_skip() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_with_known_entry(&pack);
    // Raise the title directly, then engage so the New Game reset is observable.
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Title;
    app.world_mut()
        .resource_mut::<crate::core::SkipState>()
        .engage();
    assert!(skip_on(&app));

    click(&mut app, MenuAction::NewGame);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!skip_on(&app), "New Game disarms the toggle");
}

#[test]
fn a_choice_pick_performs_only_its_own_action() {
    let pack = temp_pack();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[7, 10])]);
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert!(!skip_on(&app), "skip never runs at a choice-wait");
    // Arm the toggle defensively at the choice, so the digit's cancel path is
    // exercised: the pick must end it and do only its own action.
    app.world_mut()
        .resource_mut::<crate::core::SkipState>()
        .engage();
    assert!(skip_on(&app));

    let before = parked_position(&app);
    press_and_update(&mut app, KeyCode::Digit1);
    assert!(!skip_on(&app), "the digit ended the armed toggle");
    assert_ne!(
        parked_position(&app),
        before,
        "the digit performed exactly its own pick"
    );
}

/// An entry scene whose second line is a `decide` point, so a resolvable
/// decision-wait can be parked in flight.
fn pack_with_decision() -> tempfile::TempDir {
    let pack = temp_pack();
    std::fs::write(
        pack.path().join("pack/scenes/first_meeting.unmei"),
        "bg school_gate\nNarrator: One.\ndecide ace.greet -> reply\n\
         label reply.warm\nNarrator: Two.\nlabel reply.cold\nNarrator: Three.\n\
         label reply.none\nNarrator: Four.\n",
    )
    .unwrap();
    pack
}

#[test]
fn skip_ends_at_a_decision_wait_without_resolving() {
    let pack = pack_with_decision();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = app_with_decisions_at(root, Some(Arc::new(fake.clone())));
    build_ui(&mut app);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Decision { .. })));
    assert!(
        !skip_on(&app),
        "the decision-wait ends skip on the frame it lands"
    );

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Decision { .. })));

    pump_until(&mut app, |_| fake.calls() == 1);
    assert!(
        app.world().resource::<PendingDecision>().0.is_some(),
        "skip neither resolved nor cancelled the request"
    );
}

#[test]
fn seen_skip_stops_at_a_resolverless_decision_even_when_its_landing_is_read() {
    let pack = pack_with_decision();
    let root = pack.path().join("pack");
    // Both the engaged line and the author fallback's landing line are known,
    // so only the decision stop can end the toggle: under the old ordering the
    // immediate fallback ran to its read landing in the same frame and skip
    // continued straight through the decision.
    seed_read_lines(&root, &[("first_meeting", &[2, 7])]);
    let mut app = app_for_pack(root);
    build_ui(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Say { text, .. }) if text == "One."));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    // One step advances into the decision; the resolver-less fallback lands on
    // `reply.cold`'s read line in the same frame.
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(
        matches!(wait(&app), Some(Wait::Say { text, .. }) if text == "Three."),
        "the immediate fallback still lands: {:?}",
        wait(&app)
    );
    assert!(
        !skip_on(&app),
        "the decision stop is not bypassed by its immediate fallback"
    );
}

#[test]
fn skip_ends_at_the_end_of_the_scene() {
    let pack = pack_with_entry_says(&["Narrator: One."]);
    let mut app = app_with_known_entry(&pack);
    build_ui(&mut app);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(wait(&app).is_none(), "the script ended");
    assert!(!skip_on(&app), "the end of the scene ends skip");
}

#[test]
fn a_running_skip_never_dirties_the_read_log() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = app_for_pack(root);
    let before = std::fs::read(read_log_path(&app)).expect("the seeded log exists");
    assert!(!read_state(&app).is_dirty());

    press_and_update(&mut app, KeyCode::Tab);
    for _ in 0..2 {
        advance_time(&mut app, 0.05);
        tick_update(&mut app);
    }
    assert!(!read_state(&app).is_dirty(), "seen skip adds no marks");
    assert_eq!(std::fs::read(read_log_path(&app)).unwrap(), before);
}

#[test]
fn a_successor_line_renders_whole_for_its_full_dwell() {
    let pack = pack_with_entry_says(&[
        "Narrator: One two three four five six seven eight.",
        "Narrator: Nine ten eleven twelve thirteen fourteen.",
        "Narrator: Later.",
    ]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = app_for_pack(root);
    build_ui(&mut app);
    set_text_speed(&mut app, 0.02);
    render_dialogue(&mut app);

    // Engage on the first (revealing) line: the completion frame only
    // completes it. One 0.05 s step then lands on the second line, still
    // revealing.
    press_and_update(&mut app, KeyCode::Tab);
    let first = say_parts(&app).1;
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    render_dialogue(&mut app);
    let second = say_parts(&app).1;
    assert_ne!(second, first, "the step landed on the successor");
    assert!(
        !app.world().resource::<TextReveal>().is_complete(0.02),
        "the successor is still revealing when it lands"
    );

    // The completion frame spends no dwell: a full 0.05 s frame completes the
    // line and still shows it whole instead of advancing it away unrendered.
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    render_dialogue(&mut app);
    assert_eq!(
        say_parts(&app).1,
        second,
        "the successor is not advanced away on its completion frame"
    );
    assert!(app.world().resource::<TextReveal>().is_complete(0.02));
    assert_eq!(
        drawn_prose(&app),
        second,
        "the frame rendered the whole successor line"
    );

    // The next 0.05 s frame is the dwell's first and last: it advances once.
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert_ne!(
        say_parts(&app).1,
        second,
        "the full dwell then advances once"
    );
}

#[test]
fn a_digit_ends_seen_skip_without_picking_or_stepping() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    // Arm the step to fire this frame, then press a digit: the cancelling
    // input wins before the skip step and performs no pick at a say-park.
    advance_time(&mut app, 0.05);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Digit5);
    tick_update(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    assert!(!skip_on(&app), "the digit ended the toggle");
    assert_eq!(wait(&app), Some(first), "the digit spent no skip step");
}

#[test]
fn enter_and_left_click_each_end_skip_and_do_only_their_action() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!skip_on(&app), "Enter ends the toggle");
    assert_ne!(wait(&app), Some(first), "and advances exactly once");

    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    let first = wait(&app).unwrap();
    press_and_update(&mut app, KeyCode::Tab);
    click_and_update(&mut app);
    assert!(!skip_on(&app), "a left-click ends the toggle");
    assert_ne!(wait(&app), Some(first), "and advances exactly once");
}

#[test]
fn a_choice_button_performs_only_its_own_action() {
    let pack = temp_pack();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[7, 10])]);
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert!(!skip_on(&app));
    app.world_mut()
        .resource_mut::<crate::core::SkipState>()
        .engage();
    assert!(skip_on(&app));

    let before = parked_position(&app);
    app.world_mut().spawn((
        crate::presentation::play::ChoiceButton { index: 0 },
        Interaction::Pressed,
    ));
    app.update();
    assert!(!skip_on(&app), "the button ended the armed toggle");
    assert_ne!(
        parked_position(&app),
        before,
        "the choice button performed exactly its own pick"
    );
}

#[test]
fn esc_still_clears_skip_under_a_defensive_hold() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));

    app.world_mut().resource_mut::<DefensiveHold>().engage();
    esc(&mut app);
    assert!(menu(&app).open, "Esc still opens the overlay under a hold");
    assert!(
        !skip_on(&app),
        "Esc clears the toggle even while a hold freezes the play path"
    );
}

/// Step the force hold until it reaches a choice-wait.
fn run_until_choice(app: &mut App) {
    for _ in 0..8 {
        advance_time(app, 0.05);
        step_input(app);
        if matches!(wait(app), Some(Wait::Choice { .. })) {
            return;
        }
    }
    panic!("the hold never reached a choice");
}

#[test]
fn a_held_ctrl_skips_unread_lines_and_release_stops() {
    let pack = pack_with_entry_says(&[
        "Narrator: One.",
        "Narrator: Two.",
        "Narrator: Three.",
        "Narrator: Four.",
    ]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    let first = wait(&app).unwrap();
    assert_eq!(
        read_state(&app).arrival_read(),
        Some(false),
        "unread arrival"
    );

    // Press once and keep Ctrl down: force arms on an unread line.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_on(&app), "a held Ctrl skips an unread line");
    assert!(skip_force(&app));
    assert_eq!(wait(&app), Some(first.clone()), "the arm frame holds");

    // The hold advances on the shared 0.05 s step, across frames.
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    let second = wait(&app).unwrap();
    assert_ne!(second, first, "one step advances");
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(
        wait(&app),
        Some(second),
        "the hold continues without a fresh press"
    );

    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_on(&app), "release ends the force hold");
}

#[test]
fn tab_is_inert_during_a_force_hold_and_release_restores_seen() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);

    // The seen toggle is on first, so its survival through the hold is visible.
    tap_key(&mut app, KeyCode::Tab);
    assert!(app.world().resource::<crate::core::SkipState>().is_seen());

    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    // Tab is inert under the hold: the suspended seen toggle is unchanged.
    tap_key(&mut app, KeyCode::Tab);
    assert!(skip_force(&app), "the hold continues");
    assert!(
        app.world().resource::<crate::core::SkipState>().is_seen(),
        "the suspended seen toggle is untouched"
    );

    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
    assert!(skip_on(&app), "an eligible seen toggle resumes on release");
}

#[test]
fn force_holds_at_a_choice_and_resumes_after_the_pick() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    run_until_choice(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert!(skip_force(&app), "the hold waits at the choice");

    // A digit still chooses while Ctrl is held; the hold continues.
    tap_key(&mut app, KeyCode::Digit1);
    assert!(skip_force(&app), "the pick does not end the hold");
    let line = wait(&app).unwrap();
    assert!(
        matches!(line, Wait::Say { .. }),
        "the choice landed on a say"
    );

    // Force resumes on the next say-park while the key is still down.
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(wait(&app), Some(line), "force resumes after the park");
}

#[test]
fn a_choice_button_under_force_picks_and_the_hold_continues() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    run_until_choice(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert!(skip_force(&app));

    let before = parked_position(&app);
    app.world_mut().spawn((
        crate::presentation::play::ChoiceButton { index: 0 },
        Interaction::Pressed,
    ));
    step_input(&mut app);
    assert!(skip_force(&app), "the button does not end the hold");
    assert_ne!(
        parked_position(&app),
        before,
        "the button performed exactly its own pick"
    );
}

#[test]
fn a_manual_advance_under_force_marks_its_arrival_and_never_double_steps() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    let first = wait(&app).unwrap();

    // Let the first line's dwell run partway, then advance manually: the press
    // performs exactly its own one advance, marks the line it lands on, and the
    // hold resumes fresh at the new park rather than spending the old dwell.
    advance_time(&mut app, 0.04);
    step_input(&mut app);
    assert_eq!(wait(&app), Some(first.clone()), "0.04 s is short of a step");
    tap_key(&mut app, KeyCode::Space);
    assert!(skip_force(&app), "the press does not end the hold");
    assert_eq!(
        parked_position(&app).0,
        3,
        "the press advanced exactly once, to line 3"
    );
    assert!(
        read_state(&app).is_read("first_meeting", 3),
        "the manual arrival marks"
    );
}

#[test]
fn force_holds_at_a_decision_and_never_resolves_for_the_player() {
    let pack = pack_with_decision();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = boot_app_with_input(root, Some(Arc::new(fake.clone())));
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(matches!(wait(&app), Some(Wait::Decision { .. })));
    assert!(skip_force(&app), "the hold waits at the decision");
    assert!(
        app.world().resource::<PendingDecision>().0.is_some(),
        "force neither resolved nor cancelled the request"
    );

    // The request still answers; force resumes on the say the answer lands on.
    fake.release();
    pump_until(&mut app, |_| fake.calls() == 1);
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(skip_force(&app), "force resumes after the answer");
}

#[test]
fn a_force_advance_landing_on_a_decision_does_not_exempt_the_fallback_say() {
    let pack = pack_with_decision();
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    // No resolver: the decision resolves to its author fallback (`cold`) in the
    // same frame as the force advance.
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    advance_time(&mut app, 0.05);
    step_input(&mut app);
    // The advance landed on the decision, not a say, so its exemption is void:
    // the fallback's say (line 7) is an ordinary park and marks normally.
    assert!(
        read_state(&app).is_read("first_meeting", 7),
        "the fallback landing marks"
    );
    assert!(
        !read_state(&app).is_read("first_meeting", 3),
        "the decision line is no say and is never marked"
    );
}

#[test]
fn force_holds_at_the_end_of_the_scene() {
    let pack = pack_with_entry_says(&["Narrator: One."]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(wait(&app).is_none(), "the script ended");
    assert!(skip_force(&app), "force holds at the end of the scene");

    advance_time(&mut app, 1.0);
    step_input(&mut app);
    assert!(skip_force(&app));
    assert!(wait(&app).is_none());
}

#[test]
fn a_force_crossing_leaves_crossed_lines_unread_but_visits_the_scene() {
    let pack = pack_with_crossing();
    let root = pack.path().join("pack");
    // Only the entry line is known; the forced lines stay unread.
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    for _ in 0..8 {
        advance_time(&mut app, 0.05);
        step_input(&mut app);
        if wait(&app).is_none() {
            break;
        }
    }
    assert!(wait(&app).is_none(), "the script ended under the hold");

    let state = read_state(&app);
    assert!(state.is_visited("first_meeting"));
    assert!(state.is_visited("second_scene"), "the crossing is visited");
    assert!(
        state.is_read("first_meeting", 2),
        "the entry line was read before the hold"
    );
    assert!(
        !state.is_read("first_meeting", 3),
        "the forced line stays unread"
    );
    assert!(
        !state.is_read("second_scene", 1),
        "the forced crossing line stays unread"
    );
    assert!(
        !state.is_read("second_scene", 2),
        "the forced crossing line stays unread"
    );

    // The sibling bytes agree: the crossing's visited mark flushed, the forced
    // read marks did not.
    let disk = read_log_on_disk(&app).expect("the crossing flushed the log");
    assert!(disk.is_visited("second_scene"));
    assert!(!disk.is_read("first_meeting", 3));
    assert!(!disk.is_read("second_scene", 1));
}

#[test]
fn a_later_ordinary_park_marks_a_force_skipped_line() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_read_lines(&root, &[("first_meeting", &[2])]);
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);

    // Force one step onto line 3, then release.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
    assert!(
        !read_state(&app).is_read("first_meeting", 3),
        "the forced arrival is unread"
    );
    assert_eq!(
        read_state(&app).arrival_read(),
        Some(false),
        "the forced park still evaluates its arrival status"
    );

    // Save on the forced line, advance manually, then load back: the load's
    // re-park is a later ordinary park and marks it.
    tap_key(&mut app, KeyCode::F5);
    tap_key(&mut app, KeyCode::Space);
    assert!(
        read_state(&app).is_read("first_meeting", 4),
        "the manual park marks"
    );
    assert!(
        !read_state(&app).is_read("first_meeting", 3),
        "the forced line stays unread until a later park"
    );
    tap_key(&mut app, KeyCode::F9);
    assert!(
        read_state(&app).is_read("first_meeting", 3),
        "the load's re-park marks the forced line"
    );
}

#[test]
fn an_ordinary_advance_that_loops_back_is_a_fresh_arrival() {
    // The label sits immediately before the say and the jump immediately
    // after, so an advance loops back to the exact same `(scene, pc)`.
    let pack = temp_custom_pack(
        &[(
            "a",
            "bg school_gate\nlabel again\nNarrator: One.\njump again\n",
        )],
        None,
        "a",
    );
    let mut app = app_for_pack(pack.path().join("pack"));
    build_ui(&mut app);
    assert_eq!(read_state(&app).arrival_read(), Some(false));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(!skip_on(&app), "an unread arrival cannot engage");

    // The advance departs and re-enters the same coordinates; the second
    // arrival must recompute rather than inherit the first visit's status.
    press_and_update(&mut app, KeyCode::Space);
    assert_eq!(read_state(&app).arrival_read(), Some(true));
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app), "Tab engages on the re-entered, read line");
}

#[test]
fn a_forced_loop_landing_consumes_its_advance_exemption() {
    // `a` crosses into `b`; `b` jumps back to its own top, so a forced advance
    // inside `b` lands on the exact same coordinates.
    let pack = temp_custom_pack(
        &[
            ("a", "Narrator: A one.\njump b\n"),
            ("b", "Narrator: One.\njump b\n"),
        ],
        None,
        "a",
    );
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    // The forced crossing into `b` is exempt: its landing line stays unread.
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_eq!(app.world().resource::<Runtime>().runner.scene_id(), "b");
    assert_eq!(read_state(&app).arrival_read(), Some(false));
    assert!(!read_state(&app).is_read("b", 1));
    assert!(read_state(&app).is_visited("b"));

    // One more forced step loops back to the same coordinates; the landing
    // must consume that step's exemption even though the park key repeats.
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(skip_force(&app));
    assert!(!read_state(&app).is_read("b", 1));

    // Release and park ordinarily on that line: a stale exemption would
    // suppress this legitimate mark.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    tap_key(&mut app, KeyCode::Space);
    assert!(
        read_state(&app).is_read("b", 1),
        "the ordinary re-park marks the line once no exemption is pending"
    );
}

#[test]
fn a_force_hold_adds_no_flush_and_writes_nothing_within_one_scene() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    let before = std::fs::read(read_log_path(&app)).expect("play entry flushed the log");
    assert!(!read_state(&app).is_dirty());

    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    for _ in 0..3 {
        advance_time(&mut app, 0.05);
        step_input(&mut app);
    }
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);

    assert!(!read_state(&app).is_dirty(), "force adds no read marks");
    assert_eq!(std::fs::read(read_log_path(&app)).unwrap(), before);
}

#[test]
fn an_armed_hold_survives_the_overlay_pause_without_catch_up() {
    let pack = pack_with_entry_says(&[
        "Narrator: One.",
        "Narrator: Two.",
        "Narrator: Three.",
        "Narrator: Four.",
    ]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    let line = wait(&app).unwrap();

    // Esc opens the overlay; the hold survives and the clock freezes.
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "Esc opened the overlay");
    assert!(skip_force(&app), "the hold survives the pause");
    for _ in 0..3 {
        advance_time(&mut app, 1.0);
        step_input(&mut app);
    }
    assert_eq!(wait(&app), Some(line.clone()), "no step under the overlay");

    // Closing with the key still held resumes, without catch-up.
    advance_time(&mut app, 0.0);
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    assert!(skip_force(&app), "closing resumes the hold");
    assert_eq!(
        wait(&app),
        Some(line.clone()),
        "the frozen time is not caught up"
    );

    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(wait(&app), Some(line), "a fresh step advances");
}

#[test]
fn esc_under_force_ends_seen_but_the_hold_survives() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::Tab);
    assert!(app.world().resource::<crate::core::SkipState>().is_seen());
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    // Esc ends the seen toggle (never latched behind the pause) while the
    // physical hold survives.
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "Esc opened the overlay");
    assert!(!app.world().resource::<crate::core::SkipState>().is_seen());
    assert!(skip_force(&app), "the physical hold survives the pause");

    // Closing does not resurrect the toggle; the hold resumes.
    advance_time(&mut app, 0.0);
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    assert!(!app.world().resource::<crate::core::SkipState>().is_seen());
    assert!(skip_force(&app));
}

#[test]
fn force_suppresses_auto_and_release_restarts_a_full_auto_dwell() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app));

    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    let first = wait(&app).unwrap();
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(
        wait(&app),
        Some(first),
        "the 0.05 s skip dwell beats auto's 2.0 s"
    );

    // Release: auto's dwell restarts full, not with skip's remainder.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
    let line = wait(&app).unwrap();
    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(wait(&app), Some(line.clone()), "no remainder carried in");
    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), Some(line), "a full 2.0 s after release");
}

#[test]
fn a_defensive_hold_blocks_arming_and_freezes_the_force_clock() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    let line = wait(&app).unwrap();

    // A Ctrl press under a defensive hold is not armed, and the held key still
    // does not arm when the hold releases.
    app.world_mut().resource_mut::<DefensiveHold>().engage();
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(
        !skip_force(&app),
        "a press under a defensive hold is not armed"
    );
    app.world_mut().resource_mut::<DefensiveHold>().release();
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(
        !skip_force(&app),
        "the gate release does not arm a held key"
    );
    assert_eq!(wait(&app), Some(line.clone()));
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);

    // Armed in play, a defensive hold freezes the force clock.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    app.world_mut().resource_mut::<DefensiveHold>().engage();
    for _ in 0..3 {
        advance_time(&mut app, 1.0);
        step_input(&mut app);
    }
    assert_eq!(wait(&app), Some(line.clone()), "no step under the hold");
    app.world_mut().resource_mut::<DefensiveHold>().release();
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(wait(&app), Some(line), "the force resumes after the hold");
}

#[test]
fn a_ctrl_press_under_the_overlay_is_not_armed() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);

    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app), "a press under the overlay is not armed");

    // Closing with the key still held must not start a skip.
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    assert!(
        !skip_force(&app),
        "play resume does not arm an overlay press"
    );
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
}

#[test]
fn a_ctrl_press_on_the_title_is_not_armed() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);

    // Title is up: press and hold Ctrl there.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));

    enter_play_with_input(&mut app);
    assert!(!skip_force(&app), "a title press does not arm play skip");

    // Release and re-press in play arms it.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a fresh press in play arms");
}

#[test]
fn a_load_disarms_the_force_hold_and_needs_a_fresh_press() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F5);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    // F9 load disarms the hold while the key is still down.
    tap_key(&mut app, KeyCode::F9);
    assert!(!skip_force(&app), "a load disarms the hold");
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(!skip_force(&app), "a held but disarmed key stays stopped");

    // Release and re-press arms again.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
}

#[test]
fn an_overlay_load_disarms_the_force_hold() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F5);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    click_input(&mut app, MenuAction::SetMode(MenuMode::Load));
    click_input(&mut app, MenuAction::Card(0));
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::Load(Slot::Manual(1))));
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "the load closed the overlay");
    assert!(!skip_force(&app), "an overlay load disarms the hold");
}

#[test]
fn a_new_game_disarms_the_force_hold() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    tap_key(&mut app, KeyCode::Escape);
    click_input(&mut app, MenuAction::OpenSettings);
    click_input(&mut app, MenuAction::ExitToTitle);
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(menu(&app).title_surface_up());
    click_input(&mut app, MenuAction::NewGame);
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "New Game starts play");
    assert!(!skip_force(&app), "New Game disarms the hold");
}

#[test]
fn title_to_play_disarms_the_force_hold() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    tap_key(&mut app, KeyCode::Escape);
    click_input(&mut app, MenuAction::OpenSettings);
    click_input(&mut app, MenuAction::ExitToTitle);
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(menu(&app).title_surface_up());
    click_input(&mut app, MenuAction::Continue);
    assert!(!menu(&app).open, "Continue resumes play");
    assert!(!skip_force(&app), "title→play disarms the hold");

    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a fresh press in play re-arms");
}

#[test]
fn focus_loss_stops_the_force_hold() {
    use bevy::input::keyboard::KeyboardFocusLost;

    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));

    // The engine's forced release, not a new focus gate.
    app.world_mut().write_message(KeyboardFocusLost);
    step_input(&mut app);
    assert!(!skip_force(&app), "focus loss stops the hold");

    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a fresh press re-arms");
}

#[test]
fn a_toggles_auto_without_ending_the_force_hold() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    assert!(!auto_on(&app));

    tap_key(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app), "A toggles the Auto preference");
    assert!(skip_force(&app), "the hold continues");
}

#[test]
fn the_shared_skip_cue_follows_the_effective_state() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let root = pack.path().join("pack");
    seed_scene_read(&root, "first_meeting");
    let mut app = boot_app_with_input(root, None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);

    tap_key(&mut app, KeyCode::Tab);
    tap_key(&mut app, KeyCode::Tab);

    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);

    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);

    tap_key(&mut app, KeyCode::KeyA);
}
