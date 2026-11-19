use super::*;

// ---------------------------------------------------------------------------
// App-input ownership seam
// ---------------------------------------------------------------------------

#[test]
fn a_claimed_frame_suppresses_play_shortcuts_and_queued_text() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    let parked = wait(&app);
    assert!(parked.is_some(), "in play on a say-line");
    assert!(!manual_path(&app).exists());

    // Claim: the play path, the F5 shortcut and the focused-text path are all
    // frozen for the frame.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    type_character(&mut app, window, 'k');
    tap_key(&mut app, KeyCode::F5);
    tap_key(&mut app, KeyCode::Space);

    assert_eq!(field_value(&app, field), "", "claimed typing did not apply");
    assert_eq!(wait(&app), parked, "claimed Space did not advance");
    assert!(!manual_path(&app).exists(), "claimed F5 did not save");

    // Release: ordinary input works again, and the claimed keystroke does not
    // replay — one character, not two.
    app.world_mut().resource_mut::<InputOwnership>().release();
    type_character(&mut app, window, 'k');
    assert_eq!(
        field_value(&app, field),
        "k",
        "release restores ordinary editing without replaying the claimed press"
    );
    tap_key(&mut app, KeyCode::Space);
    assert_ne!(wait(&app), parked, "release restores the advance");
}

#[test]
fn a_claimed_frame_freezes_menu_navigation() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "the overlay opens unclaimed");
    let focused = menu(&app).focus;

    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    tap_key(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        menu(&app).focus,
        focused,
        "claimed navigation did not move focus"
    );

    app.world_mut().resource_mut::<InputOwnership>().release();
    tap_key(&mut app, KeyCode::ArrowRight);
    assert_ne!(
        menu(&app).focus,
        focused,
        "release restores menu navigation"
    );
}

#[test]
fn a_claimed_frame_blocks_the_outside_press_discard() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    assert_eq!(field_value(&app, field), "k");
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert!(menu(&app).key_editing, "the focused field is being edited");
    let outside = app.world_mut().spawn_empty().id();

    // A claimed outside press leaves the draft and edit mode alone.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    outside_press(&mut app, outside);
    assert_eq!(
        field_value(&app, field),
        "k",
        "claimed press did not discard"
    );
    assert!(
        menu(&app).key_editing,
        "claimed press did not leave the field"
    );

    // Release: the ordinary outside press discards the draft.
    app.world_mut().resource_mut::<InputOwnership>().release();
    outside_press(&mut app, outside);
    assert_eq!(field_value(&app, field), "", "release restores the discard");
    assert!(!menu(&app).key_editing);
}

#[test]
fn a_claimed_frame_freezes_the_settings_drag_start() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);

    // Claimed: a fresh pressed interaction starts no drag.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    app.world_mut()
        .spawn((MenuAction::TextSpeed, Interaction::Pressed));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        None,
        "claimed press started no Settings drag"
    );

    // Released: a button held across the claim transition is still
    // quarantined, so it cannot resume the drag by itself.
    app.world_mut().resource_mut::<InputOwnership>().release();
    app.world_mut()
        .spawn((MenuAction::TextSpeed, Interaction::Pressed));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        None,
        "a held pointer cannot resume the Settings drag after release"
    );

    // Releasing and pressing again is the fresh pointer the contract requires.
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    step_input(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .spawn((MenuAction::TextSpeed, Interaction::Pressed));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        Some(crate::settings::TrackControl::TextSpeed),
        "release and a fresh press restore the Settings drag start"
    );
}

#[test]
fn a_claimed_frame_keeps_a_prearmed_force_hold_live_and_mirrors_its_release() {
    let pack = pack_with_entry_says(&[
        "Narrator: One.",
        "Narrator: Two.",
        "Narrator: Three.",
        "Narrator: Four.",
    ]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);

    // Arm the force hold while the app owns input.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a held Ctrl armed the force hold");

    // A claim must not freeze the already-armed hold: it keeps stepping.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    let parked = wait(&app);
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(skip_force(&app), "the pre-armed hold stays active");
    assert_ne!(
        wait(&app),
        parked,
        "a claimed frame still steps an armed force hold"
    );

    // Releasing the physical key clears the latch even while claimed.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app), "the release mirror ran under the claim");

    // A Ctrl first pressed while claimed cannot arm.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app), "a claimed press never arms");
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
}

/// A test-only request that the resolution-point system take app input.
#[derive(Resource, Default)]
struct ClaimOnResolve(bool);

fn claim_on_resolve(flag: Res<ClaimOnResolve>, mut ownership: ResMut<InputOwnership>) {
    if flag.0 {
        ownership.claim(InputClaim("seam-resolve"));
    }
}

#[test]
fn a_claim_resolved_in_preupdate_covers_the_same_frame() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    // A claimant scheduled at the resolution point takes app input before the
    // frame's physical tracking and app actions run.
    app.insert_resource(ClaimOnResolve(true));
    app.add_systems(
        PreUpdate,
        claim_on_resolve.in_set(crate::input::InputOwnershipSet),
    );

    tap_key(&mut app, KeyCode::F5);
    assert!(
        !manual_path(&app).exists(),
        "a claim resolved in PreUpdate suppressed the same frame's F5"
    );
}

#[test]
fn a_claimed_frame_suppresses_a_queued_paste() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    *app.world_mut().resource_mut::<ClipboardPort>() =
        ClipboardPort::Memory(Some("sk-pasted\n".into()));

    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    paste(&mut app, window);
    assert_eq!(field_value(&app, field), "", "claimed paste did not apply");

    app.world_mut().resource_mut::<InputOwnership>().release();
    paste(&mut app, window);
    assert_eq!(
        field_value(&app, field),
        "sk-pasted",
        "release restores the trimmed paste without replaying the claimed one"
    );
}

#[test]
fn a_claimed_frame_blocks_the_startup_error_quit() {
    let (mut app, _window) = boot_widget_input_app(PathBuf::from("definitely/not/a/pack"));
    assert!(
        app.world().get_resource::<StartupFailure>().is_some(),
        "the broken pack is recorded"
    );
    assert_eq!(app.should_exit(), None);

    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    tap_key(&mut app, KeyCode::Enter);
    assert_eq!(
        app.should_exit(),
        None,
        "a claimed Enter did not quit the error screen"
    );

    app.world_mut().resource_mut::<InputOwnership>().release();
    tap_key(&mut app, KeyCode::Enter);
    assert!(
        matches!(app.should_exit(), Some(AppExit::Error(_))),
        "release restores the startup-error quit"
    );
}

#[test]
fn a_claimed_frame_ignores_an_os_repeat_and_needs_a_fresh_press() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);

    // A held F5 arriving as an OS repeat is not a fresh action.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    send_key_message(
        &mut app,
        KeyCode::F5,
        bevy::input::ButtonState::Pressed,
        true,
    );
    step_input(&mut app);
    assert!(
        !manual_path(&app).exists(),
        "a repeat press did not save while claimed"
    );

    // Release the claim with the key still physically down: the held repeat is
    // not a fresh action.
    app.world_mut().resource_mut::<InputOwnership>().release();
    send_key_message(
        &mut app,
        KeyCode::F5,
        bevy::input::ButtonState::Pressed,
        true,
    );
    step_input(&mut app);
    assert!(
        !manual_path(&app).exists(),
        "a held repeat did not save after release"
    );

    // Release and a fresh press does act.
    release_key(&mut app, KeyCode::F5);
    step_input(&mut app);
    press_key(&mut app, KeyCode::F5);
    step_input(&mut app);
    assert!(manual_path(&app).exists(), "a fresh press saves");
}

#[test]
fn a_claimed_frame_keeps_the_draft_through_focus_loss() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    assert_eq!(field_value(&app, field), "k");
    assert!(menu(&app).key_editing, "the focused field is being edited");

    // Claim, then move focus off the field: engine-level focus dispatch is not
    // gated, so the focused-editing sync is frozen and the drain must consume
    // the focus-loss collapse edit the observer queues in PostUpdate. That
    // producer runs after the drain in `bevy_ui`'s Content/Layout order, so the
    // queued edit is consumed on the next blocked frame, before `apply_text_edits`
    // could ever run it.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .clear();
    app.update();
    assert_eq!(
        field_value(&app, field),
        "k",
        "a claimed focus loss kept the draft"
    );
    app.update();
    assert!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .expect("EditableText")
            .pending_edits
            .is_empty(),
        "the claim consumed the queued collapse edit"
    );
    assert!(menu(&app).key_editing);
}
