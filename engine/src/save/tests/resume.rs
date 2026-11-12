//! Staging snapshot, the RON round trip, and exact resume including every
//! refusal that must leave the destination runner untouched.

use super::*;

// --- StagingState ------------------------------------------------------

#[test]
fn staging_applies_bg_sprite_play() {
    let mut staging = StagingState::default();
    staging.apply(&StageCommand::Bg("school_gate".into()));
    staging.apply(&StageCommand::Sprite {
        npc_id: "ace".into(),
        pose: "neutral".into(),
    });
    staging.apply(&StageCommand::Play("music_morning".into()));

    assert_eq!(staging.background.as_deref(), Some("school_gate"));
    assert_eq!(
        staging.sprites.get("ace").map(String::as_str),
        Some("neutral")
    );
    assert_eq!(staging.bgm.as_deref(), Some("music_morning"));
}

#[test]
fn staging_play_replaces_the_current_track() {
    let mut staging = StagingState::default();
    staging.apply(&StageCommand::Play("music_morning".into()));
    staging.apply(&StageCommand::Play("music_evening".into()));
    assert_eq!(staging.bgm.as_deref(), Some("music_evening"));
}

#[test]
fn staging_stop_clears_only_the_played_track() {
    let mut staging = StagingState::default();
    staging.apply(&StageCommand::Play("music_morning".into()));

    staging.apply(&StageCommand::Stop("other".into()));
    assert_eq!(staging.bgm.as_deref(), Some("music_morning"));

    staging.apply(&StageCommand::Stop("music_morning".into()));
    assert_eq!(staging.bgm, None);
}

#[test]
fn staging_hide_removes_a_sprite() {
    let mut staging = StagingState::default();
    staging.apply(&StageCommand::Sprite {
        npc_id: "ace".into(),
        pose: "neutral".into(),
    });
    staging.apply(&StageCommand::Hide("ace".into()));
    assert!(staging.sprites.is_empty());
}

// --- RON round-trip ----------------------------------------------------

#[test]
fn save_round_trips_through_ron() {
    let mut runner = ScriptRunner::new(demo_pack());
    drive_to_choice(&mut runner);
    let mut staging = StagingState::default();
    staging.apply_all(&[
        StageCommand::Bg("school_gate".into()),
        StageCommand::Sprite {
            npc_id: "ace".into(),
            pose: "neutral".into(),
        },
        StageCommand::Play("music_morning".into()),
    ]);

    let save = SaveFile::capture(&runner, &staging);
    let text = save.to_ron().expect("encodes");
    let decoded = SaveFile::from_ron(&text).expect("decodes");
    assert_eq!(decoded, save);
}

#[test]
fn save_file_round_trips_through_disk() {
    let mut runner = ScriptRunner::new(demo_pack());
    drive_to_choice(&mut runner);
    let save = SaveFile::capture(&runner, &StagingState::default());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("demo.save.ron");
    write_save(&path, &save).expect("writes");
    let loaded = read_save(&path).expect("reads");
    assert_eq!(loaded, save);
}

// --- Exact resume ------------------------------------------------------

#[test]
fn restore_replays_a_say_wait() {
    let mut runner = ScriptRunner::new(demo_pack());
    let parked = drive_to_say(&mut runner);
    let save = SaveFile::capture(&runner, &StagingState::default());

    let mut fresh = ScriptRunner::new(demo_pack());
    save.restore(&mut fresh).expect("restores");

    assert_eq!(fresh.wait(), Some(&parked));
    // A subsequent run returns the same park rather than moving on.
    assert_eq!(fresh.run().wait, Some(parked));
}

#[test]
fn restore_replays_a_choice_wait() {
    let mut runner = ScriptRunner::new(demo_pack());
    let parked = drive_to_choice(&mut runner);
    let save = SaveFile::capture(&runner, &StagingState::default());

    let mut fresh = ScriptRunner::new(demo_pack());
    save.restore(&mut fresh).expect("restores");
    assert_eq!(fresh.wait(), Some(&parked));
}

#[test]
fn restore_returns_the_exact_saved_staging() {
    let mut runner = ScriptRunner::new(demo_pack());
    drive_to_say(&mut runner);
    let mut staging = StagingState::default();
    staging.apply_all(&[
        StageCommand::Bg("school_gate".into()),
        StageCommand::Sprite {
            npc_id: "ace".into(),
            pose: "neutral".into(),
        },
        StageCommand::Play("music_morning".into()),
    ]);
    let save = SaveFile::capture(&runner, &staging);
    let expected = save.staging.clone();

    let mut fresh = ScriptRunner::new(demo_pack());
    let restored = save.restore(&mut fresh).expect("restores");
    assert_eq!(restored, expected, "staging is returned verbatim");
}

#[test]
fn restore_accepts_zero_without_a_wait_and_the_scene_end() {
    let lines = demo_pack().scenes.get("first_meeting").unwrap().lines.len();

    // `pc == 0` with no parked wait is permitted, not treated as an end.
    let save = SaveFile::capture(&ScriptRunner::new(demo_pack()), &StagingState::default());
    assert_eq!((save.pc, save.wait.is_none()), (0, true));
    let mut fresh = ScriptRunner::new(demo_pack());
    save.restore(&mut fresh)
        .expect("zero without a wait restores");
    assert_eq!(fresh.pc(), 0);
    assert_eq!(fresh.wait(), None);

    // The position one past the last line (the scene end) is permitted when
    // no wait is parked there.
    let mut at_end = SaveFile::capture(&ScriptRunner::new(demo_pack()), &StagingState::default());
    at_end.pc = lines;
    at_end.wait = None;
    let mut fresh = ScriptRunner::new(demo_pack());
    at_end.restore(&mut fresh).expect("the scene end restores");
    assert_eq!(fresh.pc(), lines);
}

#[test]
fn restore_refires_a_decision_wait() {
    let mut runner = ScriptRunner::new(demo_pack());
    let parked = drive_to_decision(&mut runner);
    let save = SaveFile::capture(&runner, &StagingState::default());

    let mut fresh = ScriptRunner::new(demo_pack());
    save.restore(&mut fresh).expect("restores");
    assert_eq!(fresh.wait(), Some(&parked));

    // The caller re-fires the decision: landing on the action label works
    // from the reloaded state exactly as it would have live.
    fresh.resolve_decision("warm").expect("lands on reply.warm");
    let landed = fresh.run();
    let Wait::Say { speaker, text } = landed.wait.expect("parks on a say") else {
        panic!("expected a say after the decision");
    };
    assert_eq!(speaker, "Ace");
    assert!(text.contains("You came back!"));
}

#[test]
fn restore_carries_state_forward() {
    let runner = runner_in_reunion();
    let parked = runner.wait().cloned().expect("parked on the reunion say");
    let save = SaveFile::capture(&runner, &StagingState::default());

    assert_eq!(save.scene_id, "reunion");
    assert_eq!(
        save.state.get("affinity_level"),
        Some(&StateValue::Str("high".to_string()))
    );
    assert_eq!(save.state.get("met_ace"), Some(&StateValue::Bool(true)));

    let mut fresh = ScriptRunner::new(demo_pack());
    save.restore(&mut fresh).expect("restores");
    assert_eq!(fresh.scene_id(), "reunion");
    assert_eq!(
        fresh.state().get("affinity_level"),
        Some(&StateValue::Str("high".to_string()))
    );
    assert_eq!(fresh.wait(), Some(&parked));
}

#[test]
fn restore_resumes_inside_a_taken_conditional_arm() {
    let mut runner = ScriptRunner::new(conditional_pack());
    let parked = runner.run().wait.expect("parks on the arm's say");
    let save = SaveFile::capture(&runner, &StagingState::default());

    let mut fresh = ScriptRunner::new(conditional_pack());
    save.restore(&mut fresh).expect("restores");
    assert_eq!(fresh.wait(), Some(&parked));

    // Resuming after the arm's say skips the `else` marker and reaches the
    // say after the block without re-evaluating the condition.
    fresh.advance_say().unwrap();
    match fresh.run().wait {
        Some(Wait::Say { text, .. }) => assert_eq!(text, "After."),
        other => panic!("expected the say after the block, got {other:?}"),
    }
}

#[test]
fn restore_rejects_an_unknown_scene() {
    let runner = ScriptRunner::new(demo_pack());
    let mut save = SaveFile::capture(&runner, &StagingState::default());
    save.scene_id = "nowhere".to_string();

    // The destination is a live, non-entry session: the refusal must leave
    // its scene, pc, state and wait exactly as they were.
    let mut fresh = runner_in_reunion();
    let before = position(&fresh);
    let err = save.restore(&mut fresh).unwrap_err();
    assert!(matches!(
        err,
        SaveError::Runner(RunnerError::UnknownScene { .. })
    ));
    assert_eq!(position(&fresh), before, "the runner survives the refusal");
}

#[test]
fn restore_rejects_zero_and_newer_versions_and_preserves_the_runner() {
    for version in [0, SAVE_VERSION + 1] {
        let runner = ScriptRunner::new(demo_pack());
        let mut save = SaveFile::capture(&runner, &StagingState::default());
        save.version = version;

        let mut fresh = runner_in_reunion();
        let before = position(&fresh);
        let err = save.restore(&mut fresh).unwrap_err();
        assert!(
            matches!(err, SaveError::Version { found, .. } if found == version),
            "version {version} is refused as a version error: {err:?}"
        );
        assert_eq!(
            position(&fresh),
            before,
            "the runner survives version {version}"
        );
    }
}

#[test]
fn restore_refuses_each_semantic_defect_and_preserves_the_runner() {
    // Constructed directly from a valid parked save — not read from a slot —
    // so each case exercises `restore`'s own gate.
    let lines = demo_pack().scenes.get("first_meeting").unwrap().lines.len();
    let valid = parked_save();

    let mut past_end = valid.clone();
    past_end.pc = lines + 1;
    let mut wait_at_zero = valid.clone();
    wait_at_zero.pc = 0;
    let mut undeclared = valid.clone();
    undeclared
        .state
        .insert("surprise".to_string(), StateValue::Bool(true));
    let mut mismatched = valid.clone();
    mismatched.wait = Some(Wait::Choice {
        options: Vec::new(),
    });

    for (reason, save) in [
        ("past the end", past_end),
        ("position 0", wait_at_zero),
        ("is not declared", undeclared),
        ("does not match line", mismatched),
    ] {
        let mut fresh = runner_in_reunion();
        let before = position(&fresh);
        let err = save.restore(&mut fresh).unwrap_err();
        assert!(matches!(err, SaveError::Decode(_)), "{reason}: {err:?}");
        assert!(err.to_string().contains(reason), "got: {err}");
        assert_eq!(
            position(&fresh),
            before,
            "the runner survives a `{reason}` refusal"
        );
    }
}

#[test]
fn restore_rechecks_a_value_mutated_after_a_successful_read() {
    let (_dir, pack) = temp_pack();
    let save = parked_save();
    write_slot(&pack, Slot::Manual(1), &save).expect("writes");
    let mut readback = read_slot(&pack, Slot::Manual(1)).expect("the unchanged slot reads");

    // The file is still valid, but the value handed to `restore` was edited
    // after the read, so it is checked again rather than trusted.
    let lines = pack.scenes.get("first_meeting").unwrap().lines.len();
    readback.pc = lines + 1;

    let mut runner = ScriptRunner::new(demo_pack());
    let before = position(&runner);
    let err = readback.restore(&mut runner).unwrap_err();
    assert!(matches!(err, SaveError::Decode(_)));
    assert!(err.to_string().contains("past the end"), "got: {err}");
    assert_eq!(position(&runner), before);
    assert_eq!(
        read_slot(&pack, Slot::Manual(1)).unwrap(),
        save,
        "the on-disk slot is untouched by the refusal"
    );
}

#[test]
fn restore_refuses_a_value_only_valid_for_another_pack_with_the_same_scene_id() {
    let (_dir, pack_a) = temp_pack();
    let save = parked_save();
    write_slot(&pack_a, Slot::Manual(1), &save).expect("writes");
    let accepted = read_slot(&pack_a, Slot::Manual(1)).expect("pack A accepts it");

    // Pack B carries the same scene id with a different line where the wait
    // is parked, so A's acceptance does not transfer.
    let pack_b = pack_with_mismatched_wait_line(&accepted);
    let mut runner_b = ScriptRunner::new(pack_b);
    let before = position(&runner_b);

    let err = accepted.restore(&mut runner_b).unwrap_err();
    assert!(matches!(err, SaveError::Decode(_)));
    assert!(
        err.to_string().contains("does not match line"),
        "got: {err}"
    );
    assert_eq!(position(&runner_b), before, "pack B is unchanged");
}
