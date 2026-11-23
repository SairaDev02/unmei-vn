use super::*;
use crate::presentation::settings_view::SettingsTrack;
use crate::settings::TrackControl;

#[test]
fn at_instant_the_say_line_is_whole_on_the_first_frame_and_a_press_advances() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    let (speaker, text) = say_parts(&app);
    assert_eq!(app.world().resource::<SettingsState>().text_speed(), 0.0);

    render_dialogue(&mut app);
    assert_eq!(dialogue_text(&app, node), format!("{speaker}: {text}"));
    assert!(
        app.world().resource::<TextReveal>().is_complete(0.0),
        "Instant positions the whole line with no partial state"
    );

    let before = app.world().resource::<Runtime>().runner.wait().cloned();
    press_and_update(&mut app, KeyCode::Space);
    assert_ne!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        before,
        "an Instant line has no reveal for a press to consume"
    );
}

#[test]
fn a_reveal_advances_with_simulated_time_and_keeps_the_prefix_whole() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);
    let (speaker, full) = say_parts(&app);
    assert_eq!(speaker, "Narrator", "the entry line is narration");
    let prefix = format!("{speaker}: ");

    render_dialogue(&mut app);
    assert_eq!(
        dialogue_text(&app, node),
        prefix,
        "the first frame shows the name plate and no text"
    );

    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    let partial = dialogue_text(&app, node);
    assert!(
        partial.starts_with(&prefix),
        "the prefix is whole: {partial}"
    );
    let body = partial.strip_prefix(&prefix).unwrap();
    assert!(!body.is_empty() && full.starts_with(body), "a char prefix");

    advance_time(&mut app, 2.0);
    render_dialogue(&mut app);
    assert_eq!(dialogue_text(&app, node), format!("{speaker}: {full}"));
}

#[test]
fn a_fifty_char_reveal_takes_about_a_second_and_never_splits_a_codepoint() {
    let mut reveal = TextReveal::default();
    let text = "é".repeat(50);
    let wait = Some(Wait::Say {
        speaker: "Narrator".to_string(),
        text: text.clone(),
    });
    reveal.arm(&wait, 0.02);
    let say = wait.clone().unwrap();

    reveal.advance(0.99, &say, 0.02);
    assert!(
        reveal.chars_shown(0.02) < 50,
        "not yet whole just under a second"
    );
    reveal.advance(0.02, &say, 0.02);
    assert!(reveal.is_complete(0.02), "whole at ~1 s of 0.02 s/char");

    // Re-run the reveal from zero, one step at a time: every intermediate
    // render is a real `chars()` prefix, so a multi-byte char never splits.
    let mut stepwise = TextReveal::default();
    stepwise.arm(&wait, 0.02);
    let mut previous = 0;
    for _ in 0..60 {
        stepwise.advance(0.02, &say, 0.02);
        let rendered = crate::presentation::play::rendered_line(&wait, &stepwise, 0.02);
        let body = rendered.strip_prefix("Narrator: ").expect("prefix whole");
        let count = body.chars().count();
        assert!(text.starts_with(body), "never a mid-codepoint slice");
        assert!(count >= previous, "the reveal never runs backwards");
        previous = count;
    }
    assert_eq!(previous, 50);

    // A pace change mid-line alters the rate going forward but never rolls
    // already-revealed characters back (the progress is stored in characters).
    let mut slowed = TextReveal::default();
    slowed.arm(&wait, 0.02);
    slowed.advance(0.5, &say, 0.02);
    assert_eq!(slowed.chars_shown(0.02), 25);
    slowed.advance(0.0, &say, 0.08);
    assert_eq!(slowed.chars_shown(0.08), 25, "slowing never retreats");
}

#[test]
fn a_press_mid_reveal_completes_the_line_and_is_consumed() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);

    let before = app.world().resource::<Runtime>().runner.wait().cloned();
    render_dialogue(&mut app);
    let (_, full) = say_parts(&app);
    assert!(!app.world().resource::<TextReveal>().is_complete(0.02));

    press_and_update(&mut app, KeyCode::Space);
    assert_eq!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        before,
        "the completing press is consumed, never an advance"
    );
    render_dialogue(&mut app);
    assert!(dialogue_text(&app, node).ends_with(&full));

    press_and_update(&mut app, KeyCode::Space);
    assert_ne!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        before,
        "the next press advances"
    );
}

#[test]
fn a_left_click_mid_reveal_completes_and_is_consumed_like_a_key() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);

    let before = app.world().resource::<Runtime>().runner.wait().cloned();
    render_dialogue(&mut app);
    let (_, full) = say_parts(&app);

    click_and_update(&mut app);
    assert_eq!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        before,
        "the click completes rather than advances"
    );
    render_dialogue(&mut app);
    assert!(dialogue_text(&app, node).ends_with(&full));

    click_and_update(&mut app);
    assert_ne!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        before,
        "the next click advances"
    );
}

#[test]
fn the_decision_placeholder_and_end_marker_never_reveal() {
    let reveal = TextReveal::default();
    let decision = Some(Wait::Decision {
        npc_id: "ace".to_string(),
        question: "greet".to_string(),
        prefix: "reply".to_string(),
    });
    assert_eq!(
        crate::presentation::play::rendered_line(&decision, &reveal, 0.02),
        "ace is deciding…"
    );
    assert_eq!(
        crate::presentation::play::rendered_line(&None, &reveal, 0.02),
        "— end of scene —"
    );
    let choice = Some(Wait::Choice {
        options: vec![("Ask".to_string(), "ask".to_string())],
    });
    assert_eq!(
        crate::presentation::play::rendered_line(&choice, &reveal, 0.02),
        ""
    );
}

#[test]
fn a_say_wait_restored_from_a_slot_re_reveals_from_the_first_character() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);

    press_and_update(&mut app, KeyCode::Space);
    render_dialogue(&mut app);
    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    let (speaker, full) = say_parts(&app);
    let partial = dialogue_text(&app, node);
    assert_ne!(partial, format!("{speaker}: {full}"), "mid-reveal");

    press_and_update(&mut app, KeyCode::F5);
    press_and_update(&mut app, KeyCode::F9);
    render_dialogue(&mut app);
    assert_eq!(
        dialogue_text(&app, node),
        format!("{speaker}: "),
        "a load re-arms the reveal at the first character"
    );
    assert!(!app.world().resource::<TextReveal>().is_complete(0.02));
}

#[test]
fn the_overlay_freezes_the_reveal_and_resumes_where_it_stopped() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let node = spawn_dialogue(&mut app);
    set_text_speed(&mut app, 0.02);

    render_dialogue(&mut app);
    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    let frozen_text = dialogue_text(&app, node);
    let shown_before = app.world().resource::<TextReveal>().chars_shown(0.02);
    assert!(shown_before > 0);

    esc(&mut app);
    assert!(menu(&app).open);
    advance_time(&mut app, 5.0);
    render_dialogue(&mut app);
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.02),
        shown_before,
        "the reveal clock is frozen while the overlay is open"
    );
    assert_eq!(dialogue_text(&app, node), frozen_text);

    esc(&mut app);
    assert!(!menu(&app).open);
    advance_time(&mut app, 0.5);
    render_dialogue(&mut app);
    assert!(
        app.world().resource::<TextReveal>().chars_shown(0.02) > shown_before,
        "closing resumes from where it stopped"
    );
}

#[test]
fn the_text_speed_control_walks_the_named_ladder_and_persists_on_leave() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());
    open_settings(&mut app);
    focus_down(&mut app, 5); // Master -> Text speed
    assert_eq!(menu(&app).focus, MenuFocus::TextSpeed);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
        "Instant"
    );

    for name in ["Fast", "Normal", "Slow", "Slowest"] {
        press_and_update(&mut app, KeyCode::ArrowRight);
        assert_eq!(
            settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
            name
        );
    }
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
        "Slowest",
        "clamped at the slowest step"
    );
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
        "Slow"
    );

    click(&mut app, MenuAction::TextSpeed);
    assert_eq!(menu(&app).focus, MenuFocus::TextSpeed);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
        "Slow",
        "the injected click does not press the mouse, so it changes no value"
    );

    assert!(
        !settings_path(&app).exists(),
        "no write while the view is open"
    );
    press_and_update(&mut app, KeyCode::Escape); // leave -> flush
    assert_eq!(settings::load(&pack_root).settings.play.text_speed, 0.04);
}

const TEXT_SPEED_TRACK_CENTRE: Vec2 = Vec2::new(640.0, 400.0);

/// The track's declared size, read from the node the row builder spawned.
fn declared_track_size(app: &App, track: Entity) -> Vec2 {
    let node = app
        .world()
        .entity(track)
        .get::<Node>()
        .expect("the text-speed track has a layout node");
    let axis = |value: Val, axis: &str| match value {
        Val::Px(px) => px,
        other => panic!("the track's {axis} is {other:?}, not a fixed size"),
    };
    Vec2::new(axis(node.width, "width"), axis(node.height, "height"))
}

/// The Settings row and track node for the text-speed control.
fn text_speed_entities(app: &mut App) -> (Entity, Entity) {
    let world = app.world_mut();
    let row = {
        let mut query = world.query::<(Entity, &MenuAction)>();
        query
            .iter(world)
            .find(|(_, action)| **action == MenuAction::TextSpeed)
            .map(|(entity, _)| entity)
            .expect("the text-speed row exists")
    };
    let track = {
        let mut query = world.query::<(Entity, &SettingsTrack)>();
        query
            .iter(world)
            .find(|(_, track)| track.control == TrackControl::TextSpeed)
            .map(|(entity, _)| entity)
            .expect("the text-speed track exists")
    };
    (row, track)
}

/// Press the text-speed track at `fraction` of its width, the pointer path a
/// player takes: the drag starts on the row's `Interaction::Pressed` and the
/// cursor's x on the laid-out track names the step.
///
/// The three layout components are what `settings_drag` reads, and a headless
/// app never computes them, so the test supplies them from the node's declared
/// size.
fn press_text_speed_track(
    app: &mut App,
    window: Entity,
    row: Entity,
    track: Entity,
    fraction: f32,
) {
    let size = declared_track_size(app, track);
    app.world_mut().entity_mut(track).insert((
        bevy::ui::ComputedNode { size, ..default() },
        bevy::ui::ComputedUiRenderTargetInfo::default(),
        bevy::ui::UiGlobalTransform::from_translation(TEXT_SPEED_TRACK_CENTRE),
    ));
    let offset = (fraction - 0.5) * size.x;
    app.world_mut()
        .entity_mut(window)
        .get_mut::<Window>()
        .expect("the primary window")
        .set_cursor_position(Some(TEXT_SPEED_TRACK_CENTRE + Vec2::new(offset, 0.0)));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut().entity_mut(row).insert(Interaction::Pressed);
    step_input(app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    step_input(app);
}

#[test]
fn a_press_on_the_text_speed_track_lands_on_the_step_under_the_pointer() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let (mut app, window) = boot_widget_input_app(pack_root.clone());
    app.init_resource::<crate::presentation::MenuBuilt>();
    app.add_systems(Update, crate::presentation::sync_menu_ui);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::Escape);
    click_input(&mut app, MenuAction::OpenSettings);
    step_input(&mut app);
    assert_eq!(menu(&app).view, MenuView::Settings);
    let (row, track) = text_speed_entities(&mut app);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
        "Instant"
    );

    for (fraction, step) in [(0.25, "Fast"), (0.75, "Slow"), (1.0, "Slowest")] {
        press_text_speed_track(&mut app, window, row, track, fraction);
        assert_eq!(
            settings_switch_text(&mut app, SettingsLabelKind::TextSpeed),
            step,
            "a press at {fraction} of the track lands on {step}"
        );
    }

    assert!(
        !settings_path(&app).exists(),
        "no write while the view is open"
    );
    tap_key(&mut app, KeyCode::Escape);
    assert_eq!(settings::load(&pack_root).settings.play.text_speed, 0.08);
}

#[test]
fn auto_advances_an_instant_line_two_seconds_after_it_parks() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    let start = wait(&app);
    assert!(start.is_some());

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(wait(&app), start, "1.9 s is short of the dwell");

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), start, "the say advances at 2.0 s");
}

#[test]
fn auto_waits_for_the_reveal_and_then_dwells_two_seconds() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    set_text_speed(&mut app, 0.02);
    render_dialogue(&mut app); // arm the reveal for the parked line
    enable_auto(&mut app);

    // A tick while the line is still revealing must not start the countdown. If
    // it did, the 1.9 s below would fire the say early.
    advance_time(&mut app, 0.5);
    tick_auto(&mut app);
    let line = wait(&app);
    assert!(line.is_some(), "the line is still parked");

    // This span finishes the reveal; the dwell begins exactly at its end.
    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        line,
        "the dwell starts only when the reveal ends"
    );

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        line,
        "the whole dwell runs from the reveal's end"
    );

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), line, "2.0 s after a completed reveal");
}

#[test]
fn a_manual_advance_onto_an_identical_line_restarts_the_dwell() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    // Two consecutive identical say-lines are indistinguishable by their
    // `Wait` (the recorded reveal quirk), but they are distinct parks with
    // distinct `pc`, so the second must get its own full dwell.
    std::fs::write(
        pack_root.join("scenes/first_meeting.unmei"),
        "bg school_gate\nplay music_morning\nAce: Same line.\nAce: Same line.\nNarrator: End.\n",
    )
    .unwrap();
    let mut app = app_for_pack(pack_root);
    enable_auto(&mut app);
    let first = wait(&app).expect("the first say parks");

    advance_time(&mut app, 1.5);
    tick_auto(&mut app); // ~0.5 s left on the first line
    press_and_update(&mut app, KeyCode::Space);
    let second = wait(&app).expect("the duplicate say parks");
    assert_eq!(
        second, first,
        "the two lines really are identical to the waiter"
    );

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        Some(second),
        "the duplicate line gets a fresh full 2.0 s"
    );
}

#[test]
fn a_manual_advance_during_the_dwell_restarts_the_full_two_seconds() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    let first = wait(&app).expect("parked on the entry say");

    advance_time(&mut app, 1.5);
    tick_auto(&mut app); // ~0.5 s left on the first line

    // The manual press lands on the next line; its dwell starts at full again.
    press_and_update(&mut app, KeyCode::Space);
    let second = wait(&app).expect("parked on the next say");
    assert_ne!(second, first);

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        Some(second.clone()),
        "no remainder carried into the new line"
    );

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), Some(second), "a full 2.0 s on the new line");
}

#[test]
fn auto_holds_at_a_choice_wait_and_never_chooses() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));

    let before = parked_position(&app);
    for _ in 0..3 {
        advance_time(&mut app, 5.0);
        tick_auto(&mut app);
    }
    assert_eq!(
        parked_position(&app),
        before,
        "auto never calls choose at a choice-wait"
    );
}

#[test]
fn auto_holds_at_a_decision_wait_and_never_touches_the_resolver() {
    let fake = FakeResolver::new(model_answer("cold"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    enable_auto(&mut app);
    assert!(auto_on(&app));
    assert_eq!(fake.calls(), 1, "the parked request was spawned once");

    let before = parked_position(&app);
    for _ in 0..3 {
        advance_time(&mut app, 5.0);
        tick_auto(&mut app);
    }
    assert_eq!(
        parked_position(&app),
        before,
        "auto never resolves or advances a decision-wait"
    );
    assert_eq!(fake.calls(), 1, "auto never starts another request");
}

#[test]
fn auto_holds_at_the_end_of_the_scene() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    play_to_end(&mut app);
    let before = parked_position(&app);
    assert!(before.1.is_none(), "the script ended");

    for _ in 0..3 {
        advance_time(&mut app, 5.0);
        tick_auto(&mut app);
    }
    assert_eq!(parked_position(&app), before, "nothing left to advance");
}

#[test]
fn the_overlay_freezes_the_auto_dwell_without_accumulating() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    tick_update(&mut app); // start the dwell
    advance_time(&mut app, 1.5);
    tick_update(&mut app); // ~0.5 s left
    let line = wait(&app);
    assert!(line.is_some());

    esc(&mut app);
    assert!(menu(&app).open);
    for _ in 0..3 {
        advance_time(&mut app, 2.0);
        tick_update(&mut app);
    }
    assert_eq!(wait(&app), line, "no dwell fires while the overlay is open");

    esc(&mut app);
    assert!(!menu(&app).open);
    advance_time(&mut app, 0.4);
    tick_update(&mut app);
    assert_eq!(wait(&app), line, "the dwell resumes from where it stopped");
    advance_time(&mut app, 0.3);
    tick_update(&mut app);
    assert_ne!(wait(&app), line, "the remaining ~0.5 s completes");
}

#[test]
fn the_auto_hotkey_and_settings_switch_stay_in_sync_and_flush_later() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());

    press_and_update(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app));
    assert!(app.world().resource::<SettingsState>().dirty);
    assert!(
        !settings_path(&app).exists(),
        "there is no write on key press"
    );

    open_settings(&mut app);
    assert_eq!(
        settings_switch_text(&mut app, SettingsLabelKind::Auto),
        "On",
        "the switch reflects the hotkey without a reopen"
    );

    // The Settings switch and the hotkey share one toggle.
    focus_down(&mut app, 6); // Master -> PlayAuto
    assert_eq!(menu(&app).focus, MenuFocus::PlayAuto);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!auto_on(&app), "the switch turns auto off");
    press_and_update(&mut app, KeyCode::Enter);
    assert!(auto_on(&app));

    // Leaving Settings is a flush point: the pending toggle lands.
    press_and_update(&mut app, KeyCode::Escape);
    assert!(!app.world().resource::<SettingsState>().dirty);
    assert!(settings::load(&pack_root).settings.play.auto);
}

#[test]
fn a_is_inert_at_a_choice_wait_and_ignored_while_the_overlay_is_open() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = app_for_pack(pack_root.clone());

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    press_and_update(&mut app, KeyCode::KeyA);
    assert!(!auto_on(&app), "A does nothing at a choice-wait");
    assert!(!app.world().resource::<SettingsState>().dirty);
    assert!(!settings_path(&app).exists());

    esc(&mut app);
    assert!(menu(&app).open);
    press_and_update(&mut app, KeyCode::KeyA);
    assert!(!auto_on(&app), "the overlay owns input while it is open");
    assert!(!app.world().resource::<SettingsState>().dirty);
}

#[test]
fn auto_survives_a_choice_pick_and_a_load() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    press_and_update(&mut app, KeyCode::Digit1);
    assert!(auto_on(&app), "auto survives a choice pick");

    press_and_update(&mut app, KeyCode::F5);
    assert!(auto_on(&app));
    press_and_update(&mut app, KeyCode::F9);
    assert!(auto_on(&app), "auto survives a load");
}

#[test]
fn the_auto_cue_flips_with_the_hotkey_and_the_help_line_carries_the_hotkeys() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    build_ui(&mut app);

    assert_eq!(auto_cue_text(&mut app), "", "the cue is absent by default");
    press_and_update(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app));
    assert_eq!(auto_cue_text(&mut app), "AUTO", "the cue appears with A");
    press_and_update(&mut app, KeyCode::KeyA);
    assert!(!auto_on(&app));
    assert_eq!(auto_cue_text(&mut app), "", "the cue disappears with A");

    let help = help_line(&mut app);
    assert!(help.contains("A auto"), "{help}");
    assert!(help.contains("F11 fullscreen"), "{help}");
    assert!(!help.contains("F5"), "{help}");
    assert!(!help.contains("F9"), "{help}");
}

#[test]
fn a_load_onto_the_parked_line_restarts_the_dwell() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    let line = wait(&app).expect("the entry say parks");

    advance_time(&mut app, 1.5);
    tick_auto(&mut app); // ~0.5 s left

    // Save then load the very line the dwell is counting on.
    press_and_update(&mut app, KeyCode::F5);
    press_and_update(&mut app, KeyCode::F9);
    assert_eq!(wait(&app), Some(line.clone()));
    assert!(auto_on(&app));

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        Some(line.clone()),
        "the loaded line gets its own full 2.0 s"
    );

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), Some(line));
}

#[test]
fn toggling_auto_off_and_on_restarts_the_dwell() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    let line = wait(&app).expect("the entry say parks");

    advance_time(&mut app, 1.5);
    tick_auto(&mut app); // ~0.5 s left

    press_and_update(&mut app, KeyCode::KeyA); // off: disarm
    assert!(!auto_on(&app));
    press_and_update(&mut app, KeyCode::KeyA); // on: a fresh park
    assert!(auto_on(&app));

    advance_time(&mut app, 1.9);
    tick_auto(&mut app);
    assert_eq!(
        wait(&app),
        Some(line.clone()),
        "turning auto off clears the elapsed time"
    );

    advance_time(&mut app, 0.2);
    tick_auto(&mut app);
    assert_ne!(wait(&app), Some(line));
}
