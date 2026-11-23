use super::*;

/// A keystroke that carries a real logical key (not `Unidentified`), so the
/// production `EditableText` widget's navigation/selection arms see it. The
/// shared `send_key_message` deliberately writes an unidentified logical key for
/// the app's own key-code routing; a caret/selection witness needs the widget's
/// own mapping instead.
fn send_logical_key(
    app: &mut App,
    window: Entity,
    logical: bevy::input::keyboard::Key,
    code: KeyCode,
    state: bevy::input::ButtonState,
) {
    use bevy::input::keyboard::KeyboardInput;
    app.world_mut().write_message(KeyboardInput {
        key_code: code,
        logical_key: logical,
        state,
        text: None,
        repeat: false,
        window,
    });
    app.update();
}

/// Extend the field's selection `count` characters left of its caret through the
/// **real** held-`Shift` + arrow path the production widget consumes.
fn select_left(app: &mut App, window: Entity, count: usize) {
    use bevy::input::keyboard::Key;
    app.world_mut()
        .resource_mut::<ButtonInput<Key>>()
        .press(Key::Shift);
    for _ in 0..count {
        send_logical_key(
            app,
            window,
            Key::ArrowLeft,
            KeyCode::ArrowLeft,
            bevy::input::ButtonState::Pressed,
        );
        send_logical_key(
            app,
            window,
            Key::ArrowLeft,
            KeyCode::ArrowLeft,
            bevy::input::ButtonState::Released,
        );
    }
    app.world_mut()
        .resource_mut::<ButtonInput<Key>>()
        .release(Key::Shift);
    app.update();
}

/// The field's `(caret byte index, selected byte range)` read from its own
/// `parley` editor, so a witness can pin a nontrivial caret and selection.
fn field_caret_and_selection(app: &App, field: Entity) -> (usize, std::ops::Range<usize>) {
    let editable = app
        .world()
        .entity(field)
        .get::<EditableText>()
        .expect("the field is an EditableText");
    let selection = editable.editor().raw_selection();
    (selection.focus().index(), selection.text_range())
}

// The F3 drawer's safety is proven through the production-shaped harnesses
// above wherever a key, pointer or text path is involved:
// `boot_widget_input_app` runs the real `InputPlugin`, focus dispatch,
// `EditableText` observers, clipboard port and propagated pointer observers over
// `RuntimeCorePlugin`; `boot_app_with_input` gives a held key a genuine
// multi-frame `ButtonInput` life. Projection, budget and layout checks use the
// real plugin systems over a deterministically seeded retained history.

fn drawer_claim_held(app: &App) -> bool {
    app.world().resource::<InputOwnership>().claimed_by() == Some(MONITOR_CLAIM)
}

fn drawer_roots(app: &mut App) -> usize {
    let world = app.world_mut();
    let mut query = world.query_filtered::<Entity, With<drawer::MonitorRoot>>();
    query.iter(world).count()
}

/// Pulse a key in a world without `bevy::input::InputPlugin` (the fixture apps
/// that drive `ButtonInput` directly). `just_pressed` is not cleared by the
/// engine there, so each frame clears it explicitly; production always has the
/// real input plugin, which the other harnesses use.
fn tap_key_simulated(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear_just_pressed(key);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(key);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear_just_pressed(key);
}

/// A row's `(background, left border)` marker colors, by admission sequence.
fn row_marker(app: &mut App, sequence: u64) -> Option<(Color, Color)> {
    let world = app.world_mut();
    let mut query = world.query::<(&MonitorRow, &BackgroundColor, &BorderColor)>();
    query
        .iter(world)
        .find(|(row, _, _)| row.0 == sequence)
        .map(|(_, background, border)| (background.0, border.left))
}

/// Boot dev mode off and prove no Monitor drawer state, quarantine or input
/// system was installed at all.
#[test]
fn dev_mode_off_installs_no_drawer_and_f3_does_nothing() {
    let pack = temp_pack();
    let mut app = boot_app_unstarted_dev(pack.path().join("pack"), None, false);
    app.update();
    assert!(
        app.world().get_resource::<drawer::MonitorPanel>().is_none(),
        "off installs no drawer state"
    );
    assert!(
        app.world().get_resource::<InputQuarantine>().is_none(),
        "off installs no held-input quarantine"
    );
    assert_eq!(drawer_roots(&mut app), 0);

    press_and_update(&mut app, KeyCode::F3);
    assert!(
        app.world().get_resource::<drawer::MonitorPanel>().is_none(),
        "F3 does no Monitor work when the drawer is not installed"
    );
    assert_eq!(drawer_roots(&mut app), 0, "no panel input or UI work");
}

/// Enabled launches start closed, and F3 reaches ordinary play.
#[test]
fn dev_enabled_starts_closed_and_f3_opens_over_play() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    assert!(!drawer_open(&app), "an enabled launch starts closed");

    enter_play_with_input(&mut app);
    let parked = wait(&app);
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app), "F3 opens the drawer over play");
    assert!(drawer_claim_held(&app), "the drawer owns app input");
    assert_eq!(drawer_roots(&mut app), 1, "one drawer tree");
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::ShowEverything,
        "the first open focuses Show everything"
    );
    assert_eq!(wait(&app), parked, "opening is not a runtime action");
}

/// F3 reaches the title, the overlay, the startup error screen and a defensive
/// hold — the drawer never depends on a loaded `Runtime` or a closed menu.
#[test]
fn f3_reaches_every_surface() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app), "F3 opens over the title");
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app), "F3 closes over the title");

    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "the overlay is up");
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app), "F3 opens over the overlay");
    tap_key(&mut app, KeyCode::Escape);
    assert!(
        !drawer_open(&app) && menu(&app).open,
        "Esc dismisses only Monitor"
    );

    let (mut broken, _window) = boot_widget_input_app(PathBuf::from("definitely/not/a/pack"));
    assert!(broken.world().get_resource::<StartupFailure>().is_some());
    tap_key(&mut broken, KeyCode::F3);
    assert!(
        drawer_open(&broken),
        "F3 opens over the startup error screen"
    );

    let mut held_app = app_with_missing_decide_npc(&FakeResolver::new(model_answer("warm")));
    held_app.update();
    assert!(on_hold(&held_app), "the runner is on a defensive hold");
    tap_key_simulated(&mut held_app, KeyCode::F3);
    assert!(drawer_open(&held_app), "F3 opens under a defensive hold");
}

/// Esc closes only Monitor: it does not open the overlay, advance a line or
/// quit the error screen. A fresh Esc afterwards still acts underneath.
#[test]
fn esc_dismisses_only_monitor_and_the_next_fresh_esc_acts() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    let parked = wait(&app);

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app), "Esc closes Monitor");
    assert!(!menu(&app).open, "Esc did not also open the overlay");
    assert_eq!(wait(&app), parked, "Esc did not also advance play");

    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "a fresh Esc opens the overlay");
}

/// F3 wins the overlap: closed F3+Esc opens once, open F3+Esc closes once.
#[test]
fn f3_and_esc_on_one_frame_toggle_once() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);

    press_key(&mut app, KeyCode::F3);
    press_key(&mut app, KeyCode::Escape);
    step_input(&mut app);
    assert!(drawer_open(&app), "closed F3+Esc opens once");
    release_key(&mut app, KeyCode::F3);
    release_key(&mut app, KeyCode::Escape);
    step_input(&mut app);
    assert!(
        drawer_open(&app),
        "the same-frame overlap did not close it again"
    );
    assert!(!menu(&app).open, "Esc did not leak into the overlay");

    press_key(&mut app, KeyCode::F3);
    press_key(&mut app, KeyCode::Escape);
    step_input(&mut app);
    assert!(!drawer_open(&app), "open F3+Esc closes once");
    release_key(&mut app, KeyCode::F3);
    release_key(&mut app, KeyCode::Escape);
    step_input(&mut app);
    assert!(!drawer_open(&app), "the closing overlap did not reopen it");
}

/// Opening with save/advance on the same frame cannot act underneath, and the
/// quarantined shortcut never fires after release.
#[test]
fn f3_opening_with_save_and_advance_does_not_act_underneath() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    let parked = wait(&app);
    assert!(!manual_path(&app).exists());

    press_key(&mut app, KeyCode::F3);
    press_key(&mut app, KeyCode::Space);
    press_key(&mut app, KeyCode::F5);
    step_input(&mut app);

    assert!(drawer_open(&app));
    assert_eq!(wait(&app), parked, "opening did not advance the say-line");
    assert!(!manual_path(&app).exists(), "opening did not save");
    release_key(&mut app, KeyCode::F3);
    release_key(&mut app, KeyCode::Space);
    release_key(&mut app, KeyCode::F5);
    step_input(&mut app);
    assert!(
        !manual_path(&app).exists(),
        "the quarantined F5 never writes after release"
    );
}

/// Closing with an activation on the same frame cannot act underneath.
#[test]
fn esc_closing_with_activation_does_not_act_underneath() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    let parked = wait(&app);

    tap_key(&mut app, KeyCode::F3);
    press_key(&mut app, KeyCode::Escape);
    press_key(&mut app, KeyCode::Enter);
    step_input(&mut app);
    assert!(!drawer_open(&app), "Esc+Enter closes Monitor");
    assert!(
        !menu(&app).open,
        "the closing frame did not open the overlay"
    );
    assert_eq!(wait(&app), parked, "the closing frame did not advance play");
    release_key(&mut app, KeyCode::Escape);
    release_key(&mut app, KeyCode::Enter);
    step_input(&mut app);
    assert_eq!(wait(&app), parked, "the quarantined Enter never advances");
}

/// Every app hotkey and play action is inert while the drawer owns the window,
/// and none of them is buffered into the release frame.
#[test]
fn underlying_hotkeys_are_inert_while_open() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    let parked = wait(&app);
    let settings_before = (
        app.world().resource::<SettingsState>().fullscreen(),
        app.world().resource::<SettingsState>().auto(),
    );
    assert!(!manual_path(&app).exists());

    tap_key(&mut app, KeyCode::F3);
    let focus = drawer_focus(&app);
    let skip_seen = app.world().resource::<crate::core::SkipState>().is_seen();
    for key in [
        KeyCode::Space,
        KeyCode::KeyA,
        KeyCode::F5,
        KeyCode::F9,
        KeyCode::F11,
        KeyCode::Digit1,
        KeyCode::ArrowLeft,
    ] {
        tap_key(&mut app, key);
    }

    assert!(drawer_open(&app), "nothing closed the drawer");
    assert_eq!(wait(&app), parked, "no play action leaked");
    assert!(!manual_path(&app).exists(), "F5 was inert");
    assert!(!menu(&app).open, "no navigation leaked");
    assert_eq!(
        drawer_focus(&app),
        focus,
        "unrelated keys move no Monitor control"
    );
    let settings_after = (
        app.world().resource::<SettingsState>().fullscreen(),
        app.world().resource::<SettingsState>().auto(),
    );
    assert_eq!(settings_before, settings_after, "no Settings field changed");
    assert_eq!(
        app.world().resource::<crate::core::SkipState>().is_seen(),
        skip_seen,
        "Monitor controls neither end seen skip nor toggle auto"
    );
}

/// A Decision completes and applies normally while Monitor is open over live
/// play: no new pause, no cancellation and no discarded answer.
#[test]
fn a_decision_completes_normally_while_monitor_is_open() {
    let fake = FakeResolver::new(model_answer("warm"));
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &dst);
    let mut app = boot_app_with_input(dst, Some(Arc::new(fake.clone())));
    enter_play_with_input(&mut app);
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    app.update();
    pump_until(&mut app, |_| fake.calls() == 1);

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    fake.release();
    pump_until(&mut app, |app| autosave_path(app).exists());

    assert!(drawer_open(&app), "the drawer stayed open");
    assert!(
        app.world().resource::<PendingDecision>().0.is_none(),
        "the request completed, not cancelled"
    );
    assert!(!held(&app), "live play applied the answer");
    let runtime = app.world().resource::<Runtime>();
    assert!(
        matches!(runtime.runner.wait(), Some(Wait::Say { .. })),
        "the runner parked past the decision: {:?}",
        runtime.runner.wait()
    );
}

/// A Decision completed over the overlay while Monitor is open stays held after
/// Monitor alone closes; the underlying resume applies it once.
#[test]
fn closing_monitor_alone_does_not_apply_a_held_answer() {
    let fake = FakeResolver::new(model_answer("warm"));
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &dst);
    let mut app = boot_app_with_input(dst, Some(Arc::new(fake.clone())));
    enter_play_with_input(&mut app);
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    app.update();
    pump_until(&mut app, |_| fake.calls() == 1);

    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    fake.release();
    pump_until(&mut app, held);

    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    assert!(held(&app), "closing Monitor alone left the answer held");
    assert!(menu(&app).open, "the overlay pause is intact");
    assert!(!autosave_path(&app).exists(), "no write while held");

    // The ordinary overlay resume applies it, exactly once.
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    assert!(!held(&app));
    pump_until(&mut app, |app| autosave_path(app).exists());
    assert!(
        matches!(
            app.world().resource::<Runtime>().runner.wait(),
            Some(Wait::Say { .. })
        ),
        "the held answer applied on resume"
    );
}

/// Opening and closing the drawer does not release a defensive hold.
#[test]
fn monitor_does_not_release_a_defensive_hold() {
    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = app_with_missing_decide_npc(&fake);
    app.update();
    assert!(on_hold(&app));

    tap_key_simulated(&mut app, KeyCode::F3);
    assert!(drawer_open(&app) && on_hold(&app));
    tap_key_simulated(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app) && on_hold(&app), "the hold survived");
}

/// A `Space`/`Enter` held across the drawer transition never types into or
/// commits the suspended draft, no matter how many OS repeats arrive after
/// close; release plus a fresh press restores ordinary editing.
#[test]
fn held_space_across_close_never_types_into_the_suspended_draft() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    assert_eq!(field_value(&app, field), "k");
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert!(menu(&app).key_editing, "the focused field is being edited");

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app), "Monitor owns the frame");

    // Hold Space with OS repeats while the drawer is open.
    press_key(&mut app, KeyCode::Space);
    for _ in 0..4 {
        send_key_message(
            &mut app,
            KeyCode::Space,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
    }
    assert_eq!(field_value(&app, field), "k", "held Space queued no text");

    // Close with Esc while Space is still physically down.
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    for _ in 0..4 {
        send_key_message(
            &mut app,
            KeyCode::Space,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
    }
    assert_eq!(
        field_value(&app, field),
        "k",
        "the held repeat never typed after close"
    );
    assert!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .expect("EditableText")
            .pending_edits
            .is_empty(),
        "the quarantined tail drained the queued inserts"
    );

    release_key(&mut app, KeyCode::Space);
    step_input(&mut app);
    assert!(
        !app.world()
            .get_resource::<InputQuarantine>()
            .expect("quarantine")
            .holds_key(KeyCode::Space),
        "the release ended the quarantine"
    );
    type_character(&mut app, window, 'k');
    assert_eq!(
        field_value(&app, field),
        "kk",
        "a fresh press types normally again"
    );
}

/// Held Enter across close neither commits the draft nor starts a key check.
#[test]
fn held_enter_across_close_does_not_commit_the_draft() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let checker = FakeKeyCheck::new();
    install_key_check(&mut app, &checker);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;

    tap_key(&mut app, KeyCode::F3);
    press_key(&mut app, KeyCode::Enter);
    for _ in 0..3 {
        send_key_message(
            &mut app,
            KeyCode::Enter,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
    }
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    for _ in 0..3 {
        send_key_message(
            &mut app,
            KeyCode::Enter,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
    }
    assert_eq!(field_value(&app, field), "k", "the draft survived");
    assert!(menu(&app).key_editing, "editing was not committed away");
    assert_eq!(checker.calls(), 0, "no credential check was started");

    release_key(&mut app, KeyCode::Enter);
    step_input(&mut app);
}

/// The drawer's whole safety bundle through the production widget path: a dummy
/// draft is typed, Monitor opens, every control is pointer-activated, more
/// typing is attempted, Monitor closes, and the draft is still there with caret
/// semantics intact for the next keystroke.
#[test]
fn dummy_draft_survives_open_control_activation_and_close() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert_eq!(field_value(&app, field), "k");

    tap_key(&mut app, KeyCode::F3);
    // Pointer-activate each control in turn (the Show everything toggle is
    // flipped back so the draft test ends on the default view).
    activate_control(&mut app, MonitorControl::ShowEverything);
    assert_eq!(drawer_view(&app), MonitorView::Everything);
    activate_control(&mut app, MonitorControl::FollowLatest);
    activate_control(&mut app, MonitorControl::ShowEverything);
    assert_eq!(drawer_view(&app), MonitorView::UnmeiOnly);
    // Typing while open is drained, not queued for later.
    type_character(&mut app, window, 'x');
    assert_eq!(
        field_value(&app, field),
        "k",
        "typing while open was blocked"
    );

    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    assert_eq!(field_value(&app, field), "k", "the draft is intact");
    assert!(menu(&app).key_editing, "editing mode survived");
    type_character(&mut app, window, 'q');
    assert_eq!(
        field_value(&app, field),
        "kq",
        "editing resumed with the draft and caret"
    );
}

/// The draft, a nontrivial caret and a selection survive real Monitor control
/// focus movement and dismissal; the restore waits for the held activation input
/// to release, and no credential check, write or diagnostic leakage is
/// Monitor-caused.
#[test]
fn draft_caret_and_selection_survive_monitor_focus_movement_and_close() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let checker = FakeKeyCheck::new();
    install_key_check(&mut app, &checker);
    let field = spawn_key_field(&mut app);
    let draft = "sk-dummy-draft";
    for character in draft.chars() {
        type_character(&mut app, window, character);
    }
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    select_left(&mut app, window, 3);
    let seeded = field_caret_and_selection(&app, field);
    assert_eq!(field_value(&app, field), draft);
    assert_eq!(
        seeded,
        (11, 11..14),
        "a nontrivial caret and non-empty tail selection are seeded"
    );
    let row_before = app.world().resource::<CredentialsState>().row.clone();

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    assert_eq!(drawer_focus(&app), MonitorControl::ShowEverything);
    tap_key(&mut app, KeyCode::Tab);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::Close);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::ShowEverything,
        "Tab wraps the three controls"
    );
    tap_key(&mut app, KeyCode::Tab);
    tap_key(&mut app, KeyCode::Enter);
    assert!(
        drawer_panel(&app).is_following(),
        "a real Enter activated the focused control"
    );

    // An engine-level focus loss under ownership drops InputFocus, then a held
    // Space keeps the restore quarantined through the closing frame.
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .clear();
    press_key(&mut app, KeyCode::Space);
    step_input(&mut app);
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        None,
        "the suspended focus is not restored while activation input is held"
    );
    assert_eq!(field_value(&app, field), draft, "the draft is untouched");

    release_key(&mut app, KeyCode::Space);
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(field),
        "the release restored the suspended field focus"
    );
    assert_eq!(field_value(&app, field), draft, "the draft is intact");
    assert_eq!(
        field_caret_and_selection(&app, field),
        seeded,
        "the caret and selection are unchanged by Monitor"
    );
    assert!(menu(&app).key_editing, "editing resumed");
    assert_eq!(checker.calls(), 0, "no credential check was started");
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        row_before,
        "Monitor did not commit, read or rewrite the credential row"
    );
    assert!(
        !app.world()
            .get_resource::<MonitorHistory>()
            .is_some_and(|history| history.text().contains(draft)),
        "the secret draft never reached the retained Monitor output"
    );
    assert!(
        all_texts(&app).iter().all(|text| !text.contains(draft)),
        "no Monitor-managed rendered text shows the secret draft"
    );
}

/// A removed or read-only field never resurrects editing or keeps the stale
/// secret draft; the current safe focus is used instead.
#[test]
fn invalidated_draft_is_cleared_not_restored() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert_eq!(field_value(&app, field), "k");

    tap_key(&mut app, KeyCode::F3);
    // A verdict lands while the drawer is open: the row is Checking (read-only).
    app.world_mut().resource_mut::<CredentialsState>().row = KeyRow::Checking;
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        field_value(&app, field),
        "",
        "the invalidated draft was cleared, not restored"
    );
    assert!(
        !menu(&app).key_editing,
        "editing did not resume on a read-only field"
    );
    assert_ne!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(field),
        "focus moved off the invalidated field"
    );
}

/// A departed field is never resurrected: its suspended draft is not restored
/// onto the live replacement field, and no editing is forced onto the missing
/// view.
#[test]
fn a_departed_field_is_not_resurrected_and_its_draft_is_discarded() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let checker = FakeKeyCheck::new();
    install_key_check(&mut app, &checker);
    let departed = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert_eq!(field_value(&app, departed), "k");

    tap_key(&mut app, KeyCode::F3);
    // The edited view departs while the drawer is open, and a replacement field
    // with its own draft takes the focus.
    app.world_mut().entity_mut(departed).despawn();
    let replacement = spawn_key_field(&mut app);
    app.world_mut()
        .entity_mut(replacement)
        .get_mut::<EditableText>()
        .expect("the replacement is an EditableText")
        .editor_mut()
        .set_text("b");
    tap_key(&mut app, KeyCode::Escape);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert!(!drawer_open(&app));
    assert_eq!(
        field_value(&app, replacement),
        "b",
        "the departed draft was not restored onto the live replacement"
    );
    assert_eq!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(replacement),
        "focus stayed on the live replacement, not the departed entity"
    );
    assert!(
        all_texts(&app).iter().all(|text| !text.contains('k')),
        "the departed draft never reappeared in any rendered text"
    );
    assert_eq!(checker.calls(), 0, "no credential check was started");
}

/// An environment-controlled row is read-only for the session: the suspended
/// secret draft is cleared and editing does not resume.
#[test]
fn an_environment_row_clears_the_suspended_draft_and_leaves_editing() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    assert_eq!(field_value(&app, field), "k");

    tap_key(&mut app, KeyCode::F3);
    // The environment variable wins while the drawer is open.
    app.world_mut().resource_mut::<CredentialsState>().row = KeyRow::FromEnvironment {
        fingerprint: "sk-dummy\u{2026}tail".to_string(),
    };
    tap_key(&mut app, KeyCode::Escape);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        field_value(&app, field),
        "",
        "the stale secret draft was cleared, not restored"
    );
    assert!(
        !menu(&app).key_editing,
        "editing did not resume on a read-only environment row"
    );
}

/// A changed but still-editable row is not an invalidation: the valid suspended
/// draft, caret and selection are restored, unlike the read-only rows.
#[test]
fn a_changed_editable_row_keeps_the_valid_draft() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    select_left(&mut app, window, 1);
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    let seeded = field_caret_and_selection(&app, field);

    tap_key(&mut app, KeyCode::F3);
    // The stored identity re-resolves to another editable verdict while the
    // draft is suspended.
    app.world_mut().resource_mut::<CredentialsState>().row = KeyRow::SetUnchecked {
        fingerprint: "sk-changed\u{2026}tail".to_string(),
    };
    tap_key(&mut app, KeyCode::Escape);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(field_value(&app, field), "k", "the valid draft is restored");
    assert_eq!(
        field_caret_and_selection(&app, field),
        seeded,
        "the caret and selection are unchanged"
    );
    assert!(
        menu(&app).key_editing,
        "editing resumed on the changed editable row"
    );
}

/// Opening cancels an active Settings drag at its last applied value, and the
/// held pointer cannot resume it after close.
#[test]
fn opening_cancels_an_active_settings_drag() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    // A fresh press on the TextSpeed track starts the drag through the real
    // pointer path; the first `drag.control()` is the start witness.
    app.world_mut()
        .spawn((MenuAction::TextSpeed, Interaction::Pressed));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        Some(crate::settings::TrackControl::TextSpeed),
        "the fresh press started the drag"
    );
    let before = (*app.world().resource::<SettingsState>()).text_speed();

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        None,
        "opening cancelled the drag"
    );
    assert!(
        app.world()
            .get_resource::<InputQuarantine>()
            .expect("quarantine")
            .holds_button(MouseButton::Left),
        "the held pointer is quarantined across the transition"
    );
    assert_eq!(
        app.world().resource::<SettingsState>().text_speed(),
        before,
        "the last applied value is preserved, with no rollback or flush"
    );

    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    app.world_mut()
        .spawn((MenuAction::TextSpeed, Interaction::Pressed));
    step_input(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::presentation::settings_view::SettingsDrag>()
            .control(),
        None,
        "held pointer state cannot restart the drag"
    );
}

/// First open focuses Show everything; Tab/Shift+Tab wrap the three controls;
/// a reopen remembers the last valid control.
#[test]
fn focus_cycle_wraps_and_reopen_remembers() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);

    tap_key(&mut app, KeyCode::F3);
    assert_eq!(drawer_focus(&app), MonitorControl::ShowEverything);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::FollowLatest);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::Close);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::ShowEverything,
        "Tab wraps"
    );

    press_key(&mut app, KeyCode::ShiftLeft);
    tap_key(&mut app, KeyCode::Tab);
    release_key(&mut app, KeyCode::ShiftLeft);
    step_input(&mut app);
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::Close,
        "Shift+Tab wraps backwards"
    );

    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    tap_key(&mut app, KeyCode::F3);
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::Close,
        "the reopen remembered the last control"
    );
}

/// Follow latest is a truthful idempotent state; Show everything toggles only
/// the display predicate.
#[test]
fn filter_control_toggles_and_follow_latest_is_idempotent() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F3);

    activate_control(&mut app, MonitorControl::FollowLatest);
    assert!(drawer_open(&app));
    assert!(drawer_panel(&app).is_following());
    assert_eq!(drawer_view(&app), MonitorView::UnmeiOnly);

    activate_control(&mut app, MonitorControl::ShowEverything);
    assert_eq!(drawer_view(&app), MonitorView::Everything);
    activate_control(&mut app, MonitorControl::ShowEverything);
    assert_eq!(drawer_view(&app), MonitorView::UnmeiOnly);

    activate_control(&mut app, MonitorControl::Close);
    assert!(!drawer_open(&app));
}

/// An OS repeat of F3 never oscillates the drawer; release plus a fresh press
/// is one action.
#[test]
fn held_and_repeated_f3_does_not_oscillate() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);

    press_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    assert!(drawer_open(&app));
    for _ in 0..5 {
        send_key_message(
            &mut app,
            KeyCode::F3,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
        assert!(drawer_open(&app), "a repeat never toggles");
    }

    release_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    assert!(drawer_open(&app), "the release alone does not close");
    press_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    assert!(!drawer_open(&app), "a fresh press closes once");
    release_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    assert!(!drawer_open(&app));
}

/// The already-armed pre-Monitor aggregate Ctrl hold keeps running while
/// physically down, its release mirrors under Monitor, and a Ctrl first pressed
/// inside never arms without a release and a fresh press.
#[test]
fn prearmed_ctrl_keeps_running_under_monitor_and_a_new_ctrl_never_arms() {
    let pack = pack_with_entry_says(&[
        "Narrator: One.",
        "Narrator: Two.",
        "Narrator: Three.",
        "Narrator: Four.",
        "Narrator: Five.",
    ]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);

    // Arm the aggregate hold in play, then open Monitor without releasing it.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "the hold armed in play");
    tap_key(&mut app, KeyCode::F3);
    assert!(
        drawer_open(&app) && skip_force(&app),
        "the hold survived opening"
    );

    let parked = wait(&app);
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert!(skip_force(&app), "the pre-armed hold stays active");
    assert_ne!(wait(&app), parked, "the hold kept advancing under Monitor");

    // The release mirror runs even with Monitor open.
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app), "the release cleared the latch");

    // A Ctrl first pressed inside cannot arm, here or after close.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app), "a press inside never arms");
    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    assert!(!skip_force(&app), "closing did not arm the held Ctrl");
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a fresh press in play arms again");
    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
}

/// Reveal and auto keep their ordinary clocks while Monitor is open: no new
/// pause, no catch-up freeze.
#[test]
fn reveal_and_auto_keep_running_under_monitor() {
    let pack = pack_with_entry_says(&["Narrator: A long enough line to reveal."]);
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app), "A turned auto on");

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    let parked = wait(&app);
    advance_time(&mut app, 2.5);
    step_input(&mut app);
    assert_ne!(
        wait(&app),
        parked,
        "auto advanced the line while Monitor was open"
    );
}

/// An already-engaged seen skip keeps its own step clock under Monitor:
/// opening the drawer neither pauses it nor ends it, and no input leaks.
#[test]
fn an_engaged_seen_skip_keeps_stepping_under_monitor() {
    let pack = pack_with_entry_says(&["Narrator: One.", "Narrator: Two.", "Narrator: Three."]);
    let mut app = app_with_known_entry(&pack);
    build_ui(&mut app);
    press_and_update(&mut app, KeyCode::Tab);
    assert!(skip_on(&app));
    let first = wait(&app).expect("parked on the first say");

    press_and_update(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    advance_time(&mut app, 0.05);
    tick_update(&mut app);
    assert!(skip_on(&app), "Monitor did not end the seen skip");
    assert_ne!(
        wait(&app),
        Some(first),
        "the seen skip kept its ordinary step clock under Monitor"
    );
}

/// A Notice keeps its ordinary countdown while Monitor owns input: no new
/// presentation pause and no input-driven dismissal.
#[test]
fn a_notice_keeps_its_countdown_under_monitor() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.world_mut()
        .resource_mut::<Notice>()
        .show("something failed");

    press_and_update(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    advance_time(&mut app, crate::failure::NOTICE_SECONDS + 0.1);
    tick_update(&mut app);

    assert!(drawer_open(&app), "the Notice clock did not close Monitor");
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "the Notice expired on its own clock under Monitor"
    );
}

/// A committed key's ping keeps its ordinary completion clock while Monitor is
/// open: the verdict lands, the resolver rebuilds and the check neither
/// cancels, refires nor pauses.
#[test]
fn a_key_check_completes_while_monitor_is_open() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-nano-{}", "k".repeat(60));
    commit_key_at(&mut rig.app, &key);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Checking,
        "the ping is in flight"
    );

    press_and_update(&mut rig.app, KeyCode::F3);
    assert!(drawer_open(&rig.app), "F3 opened Monitor over Settings");

    checker.release(KeyVerdict::Accepted);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Set {
                fingerprint: fingerprint.clone(),
            }
    });

    assert!(drawer_open(&rig.app), "the verdict left Monitor open");
    assert_eq!(
        rig.keys.lock().unwrap().as_slice(),
        std::slice::from_ref(&key),
        "the resolver rebuilt from the completed check under Monitor"
    );
    assert_eq!(checker.calls(), 1, "the check did not refire");
}

/// The default projection is the exact, case-sensitive `unmei::` prefix;
/// Show everything reveals the other surviving admitted targets from the same
/// history, with no rename, reset or second buffer.
#[test]
fn default_projection_is_the_exact_unmei_prefix_and_everything_shares_it() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        vec![
            test_monitor_line(0, "unmei::failure", log::Level::Info, "first"),
            test_monitor_line(1, "unmeiX", log::Level::Info, "lookalike"),
            test_monitor_line(2, "unmei", log::Level::Info, "no separator"),
            test_monitor_line(3, "dep::crate", log::Level::Info, "dependency"),
            test_monitor_line(4, "UNMEI::x", log::Level::Info, "wrong case"),
            test_monitor_line(5, "unmei::decision", log::Level::Warn, "explicit"),
        ],
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..4 {
        step_input(&mut app);
    }
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(!projection.is_updating(), "a six-line projection settles");
    assert_eq!(
        projection.targets(),
        vec!["unmei::failure", "unmei::decision"],
        "only the exact case-sensitive prefix matches by default"
    );

    let retained_before = app.world().resource::<MonitorHistory>().retained();
    activate_control(&mut app, MonitorControl::ShowEverything);
    for _ in 0..4 {
        step_input(&mut app);
    }
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert_eq!(
        projection.targets(),
        vec![
            "unmei::failure",
            "unmeiX",
            "unmei",
            "dep::crate",
            "UNMEI::x",
            "unmei::decision"
        ],
        "Show everything removes only the display predicate"
    );
    assert_eq!(
        app.world().resource::<MonitorHistory>().retained(),
        retained_before,
        "toggling views neither clears nor duplicates the shared history"
    );
    assert_eq!(
        drawer_rows(&mut app)
            .iter()
            .filter(|(_, part, _)| *part == MonitorRowPart::Target)
            .count(),
        6,
        "the display is the projection, not a second history"
    );
}

/// A genuine `log`-bridge record and an ordinary event carrying a bridge-named
/// field reach the same retained history. Only the real metadata target drives
/// the projection, so the ordinary event stays out of the default view and
/// appears under Show everything without losing its level or the shared
/// history.
#[test]
fn a_forged_bridge_field_does_not_enter_the_default_projection() {
    use crate::monitor::{capture_layer, MonitorCapture, SystemClock};
    use bevy::log::tracing_subscriber::prelude::*;

    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    let capture = MonitorCapture::install(Arc::new(SystemClock::new()));
    {
        let subscriber = bevy::log::tracing_subscriber::registry().with(capture_layer(&capture));
        bevy::log::tracing::subscriber::with_default(subscriber, || {
            // An ordinary event whose field is named like bridge metadata.
            bevy::log::tracing::event!(
                target: "dep::crate",
                bevy::log::tracing::Level::INFO,
                { "log.target" } = "unmei::forged",
                "the real body"
            );
            // A genuine bridged record, at its own level.
            let record = log::Record::builder()
                .args(format_args!("bridged body"))
                .level(log::Level::Warn)
                .target("unmei::bridged")
                .build();
            tracing_log::format_trace(&record).expect("the bridge dispatches");
        });
    }
    app.insert_resource(capture);
    app.world_mut().init_resource::<MonitorHistory>();
    app.world_mut()
        .run_system_once(crate::monitor::ingest_capture)
        .expect("the ingest system runs");

    tap_key(&mut app, KeyCode::F3);
    for _ in 0..4 {
        step_input(&mut app);
    }
    let default_rows = projected_rows(&mut app);
    assert_eq!(
        default_rows,
        vec![("unmei::bridged".to_string(), log::Level::Warn)],
        "only the genuine unmei:: target matches the default view"
    );

    activate_control(&mut app, MonitorControl::ShowEverything);
    for _ in 0..4 {
        step_input(&mut app);
    }
    assert_eq!(
        projected_rows(&mut app),
        vec![
            ("dep::crate".to_string(), log::Level::Info),
            ("unmei::bridged".to_string(), log::Level::Warn),
        ],
        "Show everything reveals both surviving events in admission order"
    );
    assert_eq!(
        app.world().resource::<MonitorHistory>().retained(),
        2,
        "filtering never drops the shared history"
    );
}

/// The projected `(target, level)` rows, oldest first.
fn projected_rows(app: &mut App) -> Vec<(String, log::Level)> {
    app.world()
        .resource::<crate::drawer::MonitorProjection>()
        .rows()
        .iter()
        .map(|row| (row.target.clone(), row.level))
        .collect()
}

/// A large projection is progressive: bounded per update, marked while
/// incomplete, and complete without a long-frame catch-up.
#[test]
fn progressive_projection_is_bounded_marked_and_completes() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        (0..1000)
            .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, &format!("event {i}")))
            .collect(),
    );
    tap_key(&mut app, KeyCode::F3);
    step_input(&mut app);

    let work = app.world().resource::<MonitorWork>();
    assert!(
        work.scanned <= PROJECTION_SCAN_BUDGET,
        "at most {PROJECTION_SCAN_BUDGET} entries per update: {}",
        work.scanned
    );
    assert!(work.text_ops <= TEXT_WORK_BUDGET);
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(projection.is_updating(), "the marked in-progress flag");
    // Three frames have run while open (the F3 press, its release and one
    // step), so the projection holds at most three update budgets.
    assert!(projection.shown() <= 3 * PROJECTION_SCAN_BUDGET);
    assert!(
        drawer_status(&mut app).contains("updating view"),
        "the footer marks the incomplete projection"
    );

    for _ in 0..16 {
        step_input(&mut app);
        if !app
            .world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating()
        {
            break;
        }
        assert!(
            app.world().resource::<MonitorWork>().scanned <= PROJECTION_SCAN_BUDGET,
            "no catch-up scan over the budget"
        );
        assert!(app.world().resource::<MonitorWork>().text_ops <= TEXT_WORK_BUDGET);
    }
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(!projection.is_updating(), "the pass finished");
    assert_eq!(projection.shown(), 1000, "all matching retained lines");
    assert_eq!(
        drawer_rows(&mut app)
            .iter()
            .filter(|(_, part, _)| *part == MonitorRowPart::Target)
            .count(),
        DISPLAY_ROWS,
        "only the newest display window is materialized"
    );

    // A settled frame does no projection or text work at all.
    step_input(&mut app);
    assert_eq!(app.world().resource::<MonitorWork>().scanned, 0);
    assert_eq!(app.world().resource::<MonitorWork>().text_ops, 0);
    assert!(
        !drawer_status(&mut app).contains("updating view"),
        "the marked flag cleared"
    );
}

/// A filter change cancels the obsolete pass: only current-predicate rows are
/// ever displayed, and the shared history is untouched.
#[test]
fn filter_change_cancels_obsolete_work_and_never_mixes_predicates() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    let mut lines = Vec::new();
    for i in 0..300u64 {
        lines.push(test_monitor_line(
            i * 2,
            "unmei::kept",
            log::Level::Info,
            "kept",
        ));
        lines.push(test_monitor_line(
            i * 2 + 1,
            "dep::noise",
            log::Level::Info,
            "noise",
        ));
    }
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    step_input(&mut app);

    // Mid-refresh, the old predicate's rows are already up and all match.
    assert!(app
        .world()
        .resource::<crate::drawer::MonitorProjection>()
        .is_updating());
    assert!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .targets()
            .iter()
            .all(|target| target == "unmei::kept"),
        "only the current predicate is displayed mid-refresh"
    );

    activate_control(&mut app, MonitorControl::ShowEverything);
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(projection.is_updating());
    let sequences = projection.sequences();
    assert!(
        sequences.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "one everything pass materializes a contiguous admission run; rows kept \
         from the unmei:: pass would leave gaps: {sequences:?}"
    );
    let targets = projection.targets();
    assert!(
        targets.iter().any(|target| target == "dep::noise"),
        "the everything predicate admits dependency rows mid-refresh: {targets:?}"
    );
    // Switching back cancels that obsolete pass: no dependency row from it may
    // survive into the `unmei::` view.
    activate_control(&mut app, MonitorControl::ShowEverything);
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(
        projection
            .targets()
            .iter()
            .all(|target| target == "unmei::kept"),
        "obsolete rows were cancelled, not mixed in: {:?}",
        projection.targets().first()
    );
    for _ in 0..16 {
        step_input(&mut app);
        if !app
            .world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating()
        {
            break;
        }
        assert!(
            app.world()
                .resource::<crate::drawer::MonitorProjection>()
                .targets()
                .iter()
                .all(|target| target == "unmei::kept"),
            "only current-predicate rows at any point of the refresh"
        );
    }
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(!projection.is_updating(), "the new pass finished");
    assert_eq!(projection.shown(), 300);
}

/// The newest marker follows the newest **retained** line, never the newest
/// matching one.
#[test]
fn newest_marker_follows_the_retained_tail_not_the_matching_tail() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        vec![
            test_monitor_line(0, "unmei::failure", log::Level::Info, "first"),
            test_monitor_line(1, "unmei::failure", log::Level::Info, "second"),
            test_monitor_line(2, "dep::noise", log::Level::Error, "newest retained"),
        ],
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        row_marker(&mut app, 2),
        None,
        "the filtered-out newest retained line is not shown, so nothing is marked"
    );

    activate_control(&mut app, MonitorControl::ShowEverything);
    for _ in 0..3 {
        step_input(&mut app);
    }
    let newest = row_marker(&mut app, 2).expect("the newest retained line is shown");
    assert_eq!(
        newest.0, NEWEST_WASH,
        "the newest retained row carries the wash"
    );
    assert_eq!(newest.1, NEWEST_INSET, "and the warm inset");
    for sequence in [0, 1] {
        let older = row_marker(&mut app, sequence).expect("older rows exist");
        assert_eq!(older.0, Color::NONE, "no other row is decorated");
        assert_eq!(older.1, Color::NONE);
    }
}

/// The empty projection has an empty state; later matching arrivals neither
/// resurrect anything nor silently enable a different reader state.
#[test]
fn empty_projection_shows_an_empty_state_and_later_matches_appear() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert!(
        has_component::<drawer::MonitorEmptyState>(&mut app),
        "the empty view has an empty state"
    );
    assert!(drawer_status(&mut app).contains("shown 0"));

    seed_monitor_history(
        &mut app,
        vec![test_monitor_line(
            0,
            "unmei::failure",
            log::Level::Info,
            "later",
        )],
    );
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert!(
        !has_component::<drawer::MonitorEmptyState>(&mut app),
        "the empty state cleared"
    );
    assert!(drawer_panel(&app).is_following());
    assert!(drawer_status(&mut app).contains("shown 1"));
}

/// Closed means no hidden projection, no hidden text work and no hidden tree.
#[test]
fn a_closed_drawer_does_no_projection_or_text_work() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        (0..200)
            .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, "x"))
            .collect(),
    );
    app.update();
    assert!(!drawer_open(&app));
    let work = app.world().resource::<MonitorWork>();
    assert_eq!(work.scanned, 0, "no hidden scans");
    assert_eq!(work.text_ops, 0, "no hidden text work");
    assert_eq!(drawer_roots(&mut app), 0, "no hidden tree");

    // Reopen resumes exactly where the pass stopped and sees the history.
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..4 {
        step_input(&mut app);
    }
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .shown(),
        200
    );
}

/// The footer reports the settled status vocabulary truthfully.
#[test]
fn status_footer_reports_view_counts_counters_and_following() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        vec![
            test_monitor_line(0, "unmei::failure", log::Level::Info, "a"),
            test_monitor_line(1, "dep::noise", log::Level::Warn, "b"),
        ],
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        drawer_status(&mut app),
        "showing unmei:: only · retained 2 · shown 1 · evicted 0 · lost 0 · truncated 0 · following latest · F3/Esc close · Tab cycles controls"
    );

    activate_control(&mut app, MonitorControl::ShowEverything);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        drawer_status(&mut app),
        "showing everything · retained 2 · shown 2 · evicted 0 · lost 0 · truncated 0 · following latest · F3/Esc close · Tab cycles controls"
    );
}

/// Launch-lifetime counters survive a display toggle and hide/reopen: the
/// projection and reader never touch `MonitorCapture`, so status keeps the same
/// evicted/lost/truncated values across ordinary Monitor transitions.
#[test]
fn counters_survive_toggling_and_hide_reopen() {
    use bevy::log::tracing;
    use bevy::log::tracing_subscriber;
    use tracing_subscriber::prelude::*;

    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..10, "unmei::seq");

    // Admit one oversized event through the real capture layer: a successful
    // admission counts as exactly one truncation.
    let capture =
        crate::monitor::MonitorCapture::install(Arc::new(crate::monitor::SystemClock::new()));
    let subscriber = tracing_subscriber::registry().with(crate::monitor::capture_layer(&capture));
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: "unmei::seq", "{}", "y".repeat(9000));
    });
    assert_eq!(
        capture.counters().truncated,
        1,
        "the oversized line was admitted truncated"
    );
    app.insert_resource(capture);
    app.world_mut().init_resource::<MonitorHistory>();

    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(drawer_status(&mut app).contains("truncated 1"));

    // A filter toggle and hide/reopen leave the shared history and counters as
    // they were.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    tap_key(&mut app, KeyCode::F3);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    let status = drawer_status(&mut app);
    assert!(
        status.contains("retained 10 · shown 10 · evicted 0 · lost 0 · truncated 1"),
        "{status}"
    );
}

/// Genuinely admitted events — not a seeded history — survive the live session
/// transitions: entering play, hide/reopen, the manual slot load and a real
/// player New Game. The launch-local history and the saturating counters are
/// unchanged throughout.
#[test]
fn captured_history_and_counters_survive_live_session_transitions() {
    use bevy::log::tracing;
    use bevy::log::tracing_subscriber;
    use tracing_subscriber::prelude::*;

    let pack = temp_pack();
    let capture =
        crate::monitor::MonitorCapture::install(Arc::new(crate::monitor::SystemClock::new()));
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    // The capture must exist before the ingest system is added; the headless
    // boot has no logger, so this is the only source of retained lines.
    app.insert_resource(capture.clone());
    app.world_mut()
        .init_resource::<crate::monitor::MonitorHistory>();
    app.add_systems(Update, crate::monitor::ingest_capture);

    // Admit real events through the production capture layer: 30 ordinary lines
    // and one oversized event that counts as exactly one truncation.
    let subscriber = tracing_subscriber::registry().with(crate::monitor::capture_layer(&capture));
    tracing::subscriber::with_default(subscriber, || {
        for index in 0..30 {
            tracing::info!(target: "unmei::seq", "captured event {index:04}");
        }
        tracing::info!(target: "unmei::seq", "{}", "y".repeat(9000));
    });
    step_input(&mut app);

    assert_eq!(
        app.world()
            .resource::<crate::monitor::MonitorHistory>()
            .retained(),
        31,
        "all 31 admitted events are retained"
    );
    assert_eq!(capture.counters().truncated, 1);

    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    let status = drawer_status(&mut app);
    assert!(status.contains("retained 31"), "{status}");
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));

    // The manual slot round trip and a real player New Game both perform their
    // own session resets while the launch-local Monitor state stays put.
    tap_key(&mut app, KeyCode::F5);
    assert!(manual_path(&app).exists(), "F5 wrote the manual slot 1");
    tap_key(&mut app, KeyCode::F9);
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    click_input(&mut app, MenuAction::NewGame);
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "New Game starts play");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    let history = app.world().resource::<crate::monitor::MonitorHistory>();
    assert_eq!(
        history.retained(),
        31,
        "the history survives the transitions"
    );
    assert!(history.text().contains("captured event 0000"));
    assert!(history.text().contains("captured event 0029"));
    assert!(history.text().contains("[truncated]"));
    let counters = capture.counters();
    assert_eq!(counters.truncated, 1, "the truncation counter survives");
    assert_eq!(counters.evicted, 0, "nothing was evicted");
    assert_eq!(counters.lost, 0, "nothing was lost");
}

/// A poisoned capture is reported as local unavailable status, never a
/// synthetic row, tracing event or Notice.
#[test]
fn capture_unavailable_is_local_status_only() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    // Install a capture and poison its completed queue through the same seam
    // the capture tests use.
    let capture =
        crate::monitor::MonitorCapture::install(Arc::new(crate::monitor::SystemClock::new()));
    app.insert_resource(capture);
    app.world_mut().init_resource::<MonitorHistory>();
    // Poison the mutex by locking and panicking on another thread.
    let shared = app.world().resource::<crate::monitor::MonitorCapture>();
    shared.poison_for_test();
    assert!(shared.unavailable());

    seed_monitor_history(
        &mut app,
        vec![test_monitor_line(0, "unmei::x", log::Level::Info, "a")],
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert!(drawer_status(&mut app).contains("capture unavailable"));
    assert!(
        !drawer_rows(&mut app)
            .iter()
            .any(|(_, _, text)| text.contains("unavailable")),
        "status is never a log row"
    );
    assert!(app.world().resource::<Notice>().text.is_none());
}

/// Retention eviction drops the evicted payload from the projection in the same
/// update, so no second history keeps it alive.
#[test]
fn evicted_rows_are_dropped_from_the_projection() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        (0..10)
            .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, "old"))
            .collect(),
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .sequences(),
        (0..10).collect::<Vec<_>>()
    );

    // The oldest five age out of retained history.
    seed_monitor_history(
        &mut app,
        (5..15)
            .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, "new"))
            .collect(),
    );
    for _ in 0..3 {
        step_input(&mut app);
    }
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert_eq!(
        projection.sequences(),
        (5..15).collect::<Vec<_>>(),
        "evicted rows are gone, not cached"
    );
}

/// A hidden drawer prunes evicted references without materializing or replacing
/// hidden text, so a previously full projection can never keep a second copy of
/// a payload retention already evicted, and reopening cannot resurrect it
/// (finding A02).
#[test]
fn a_hidden_drawer_prunes_evicted_payloads_without_text_work() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..1000, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .shown(),
        1000,
        "the whole retained history is projected"
    );

    // Close, then replace every retained entry while hidden.
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    seed_numbered_monitor(&mut app, 1000..2000, "unmei::seq");
    for _ in 0..3 {
        step_input(&mut app);
    }
    assert_eq!(
        drawer_roots(&mut app),
        0,
        "closed means no hidden tree, so no hidden row text can be materialized"
    );
    assert_eq!(
        app.world().resource::<MonitorWork>().text_ops,
        0,
        "the hidden update reports no visible text work"
    );
    assert_eq!(
        app.world().resource::<MonitorWork>().scanned,
        0,
        "the hidden update reports no metadata scan"
    );
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .shown(),
        0,
        "every evicted payload reference is dropped while hidden"
    );

    // Reopen: the replacement history projects from its own oldest entry and no
    // evicted text is resurrected.
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    let sequences = app
        .world()
        .resource::<crate::drawer::MonitorProjection>()
        .sequences();
    assert_eq!(sequences.first().copied(), Some(1000));
    assert!(
        sequences.iter().all(|sequence| *sequence >= 1000),
        "no evicted payload survives in the projection"
    );
    assert_eq!(sequences.len(), 1000, "the replacement history is complete");
}

/// A history replaced by a strictly older range (a reseeded fixture) must not
/// leave rows the retained history no longer owns looking like coverage: the
/// projection restarts and shows only the replacement's own identities.
#[test]
fn a_shrunk_history_reprojects_without_stale_rows() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .shown(),
        100
    );

    // The replacement's newest entry is below the examined band.
    seed_numbered_monitor(&mut app, 0..50, "unmei::seq");
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .sequences(),
        (0..50).collect::<Vec<_>>(),
        "no row survives from the replaced range"
    );
}

/// The settled drawer geometry at the reference size: fixed px insets, height,
/// bottom reservation, header/body/footer split and the row grid.
#[test]
fn drawer_geometry_is_the_settled_pixels() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    seed_monitor_history(
        &mut app,
        vec![test_monitor_line(
            0,
            "unmei::failure",
            log::Level::Warn,
            "row",
        )],
    );
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }

    let world = app.world_mut();
    let root = {
        let mut query = world.query::<(&Node, &GlobalZIndex)>();
        query
            .iter(world)
            .find_map(|(node, index)| {
                // The drawer root is the only absolute full-window node with a
                // global index.
                (index.0 == 100).then_some(node.clone())
            })
            .expect("the drawer root exists with its stacking index")
    };
    assert_eq!(root.position_type, PositionType::Absolute);
    assert_eq!(root.width, Val::Percent(100.0));
    assert_eq!(root.height, Val::Percent(100.0));

    let panel = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorPanelNode>>();
        query.iter(world).next().cloned().expect("the panel node")
    };
    assert_eq!(panel.left, Val::Px(24.0));
    assert_eq!(panel.right, Val::Px(24.0));
    assert_eq!(panel.bottom, Val::Px(148.0));
    assert_eq!(panel.height, Val::Px(360.0));
    assert_eq!(panel.border, UiRect::all(Val::Px(1.0)));
    assert_eq!(panel.flex_direction, FlexDirection::Column);

    let body = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorLogBody>>();
        query.iter(world).next().cloned().expect("the log body")
    };
    assert_eq!(body.flex_grow, 1.0);
    assert_eq!(body.flex_direction, FlexDirection::Row);
    assert!(!body.overflow.is_visible(), "the body clips the tail");

    // The flexible viewport carries the following bottom alignment and the
    // clipped body above it owns the clipping rect, so the reader's relative
    // offset can never move the clip with it.
    let viewport = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorLogViewport>>();
        query.iter(world).next().cloned().expect("the viewport")
    };
    assert_eq!(viewport.flex_grow, 1.0);
    assert_eq!(viewport.justify_content, JustifyContent::FlexEnd);
    assert_eq!(viewport.min_width, Val::Px(0.0));

    // The scrollbar is real chrome: a fixed-width track with a draggable thumb.
    let track = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorScrollbarTrack>>();
        query
            .iter(world)
            .next()
            .cloned()
            .expect("the scrollbar track")
    };
    assert_eq!(track.width, Val::Px(drawer::SCROLLBAR_WIDTH));
    assert_eq!(track.height, Val::Percent(100.0));
    let thumb = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorScrollbarThumb>>();
        query
            .iter(world)
            .next()
            .cloned()
            .expect("the scrollbar thumb")
    };
    assert_eq!(thumb.width, Val::Percent(100.0));
    assert_eq!(thumb.position_type, PositionType::Absolute);

    let status = {
        let mut query = world.query_filtered::<&Node, With<MonitorStatusText>>();
        query.iter(world).next().cloned().expect("the status node")
    };
    assert_eq!(status.height, Val::Px(13.0));

    let row = {
        let mut query = world.query::<(&MonitorRow, &Node)>();
        query
            .iter(world)
            .find(|(row, _)| row.0 == 0)
            .map(|(_, node)| node.clone())
            .expect("the row container")
    };
    assert_eq!(row.column_gap, Val::Px(10.0));
    assert_eq!(row.padding.top, Val::Px(1.0));
    assert_eq!(row.border.left, Val::Px(2.0));

    let parts: Vec<(
        MonitorRowPart,
        Val,
        TextFont,
        TextColor,
        LineHeight,
        f32,
        Val,
    )> = {
        let mut query =
            world.query::<(&MonitorRowPart, &Node, &TextFont, &TextColor, &LineHeight)>();
        query
            .iter(world)
            .map(|(part, node, font, color, height)| {
                let width = if node.width == Val::Auto {
                    Val::Auto
                } else {
                    node.width
                };
                (
                    *part,
                    width,
                    font.clone(),
                    *color,
                    *height,
                    node.flex_shrink,
                    node.min_width,
                )
            })
            .collect()
    };
    let part = |kind: MonitorRowPart| {
        parts
            .iter()
            .find(|(part, ..)| *part == kind)
            .expect("the row part exists")
    };
    assert_eq!(part(MonitorRowPart::Elapsed).1, Val::Px(72.0));
    assert_eq!(part(MonitorRowPart::Level).1, Val::Px(46.0));
    assert_eq!(part(MonitorRowPart::Target).1, Val::Px(224.0));
    assert_eq!(part(MonitorRowPart::Body).1, Val::Auto);
    // A fixed column holds its width; the message column takes the remaining
    // space and may shrink below its content width so long text wraps inside
    // the panel instead of overflowing the row.
    assert_eq!(part(MonitorRowPart::Target).5, 0.0);
    assert_eq!(part(MonitorRowPart::Body).5, 1.0);
    assert_eq!(part(MonitorRowPart::Body).6, Val::Px(0.0));
    for kind in [
        MonitorRowPart::Elapsed,
        MonitorRowPart::Level,
        MonitorRowPart::Target,
        MonitorRowPart::Body,
    ] {
        let (_, _, font, _, line_height, ..) = part(kind);
        assert_eq!(
            font.font_size,
            bevy::text::FontSize::Px(12.0),
            "{kind:?} uses Fira Mono Medium 12px"
        );
        assert_eq!(*line_height, LineHeight::RelativeToFont(1.45));
    }
    assert_eq!(
        part(MonitorRowPart::Elapsed).3 .0,
        crate::drawer::TONE_FAINT
    );
    assert_eq!(part(MonitorRowPart::Level).3 .0, crate::drawer::TONE_WARN);
    assert_eq!(part(MonitorRowPart::Target).3 .0, crate::drawer::TONE_INFO);
    assert_eq!(part(MonitorRowPart::Body).3 .0, crate::drawer::TONE_TEXT);

    // The header carries Monitor then the three controls in the settled order.
    let labels: Vec<String> = {
        let mut query = world.query::<(&drawer::MonitorControlTag, &Text)>();
        let mut controls: Vec<(MonitorControl, String)> = query
            .iter(world)
            .map(|(tag, text)| (tag.0, text.as_str().to_string()))
            .collect();
        controls.sort_by_key(|(control, _)| match control {
            MonitorControl::ShowEverything => 0,
            MonitorControl::FollowLatest => 1,
            MonitorControl::Close => 2,
        });
        controls.into_iter().map(|(_, label)| label).collect()
    };
    assert_eq!(
        labels,
        vec!["[ ] Show everything", "Follow latest", "Close"],
        "the settled control order and labels"
    );
}

/// Fullscreen keeps fixed px anchoring: no proportional inset, height or
/// reservation, so the drawer widens to the frame.
#[test]
fn drawer_fullscreen_anchoring_is_fixed_pixels() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..2 {
        step_input(&mut app);
    }
    let world = app.world_mut();
    let panel = {
        let mut query = world.query_filtered::<&Node, With<drawer::MonitorPanelNode>>();
        query.iter(world).next().cloned().expect("the panel node")
    };
    // The drawer is laid out from the window edges with a fixed px inset, so at
    // 1920x1080 its width is 1920 - 2*24 = 1872px with no width cap or scale.
    assert_eq!(panel.left, Val::Px(drawer::PANEL_SIDE_INSET));
    assert_eq!(panel.right, Val::Px(drawer::PANEL_SIDE_INSET));
    assert_eq!(panel.height, Val::Px(drawer::PANEL_HEIGHT));
    assert_eq!(panel.bottom, Val::Px(drawer::PANEL_BOTTOM_RESERVATION));
    assert!(
        matches!(panel.width, Val::Auto | Val::Percent(100.0)),
        "no width cap or proportional width"
    );
}

/// An oversized event collapses the grid into one warm-toned line with a
/// visible `[truncated]` suffix; an escaped event keeps one logical row.
#[test]
fn truncated_and_escaped_rows_render_as_one_line() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    let escaped = test_monitor_line(
        0,
        "unmei::failure",
        log::Level::Error,
        "line one\\nline two \\\\n literal",
    );
    let mut truncated = test_monitor_line(1, "unmei::failure", log::Level::Warn, "huge");
    truncated.text = format!(
        "0000000001ms WARN  unmei::failure {}{}",
        "x".repeat(100),
        " [truncated]"
    );
    truncated.body_start = truncated.text.len() - " [truncated]".len();
    truncated.truncated = true;
    seed_monitor_history(&mut app, vec![escaped, truncated]);
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }

    let world = app.world_mut();
    let rows: Vec<(u64, usize)> = {
        let mut query = world.query::<(&MonitorRow, &Children)>();
        query
            .iter(world)
            .map(|(row, children)| (row.0, children.len()))
            .collect()
    };
    assert!(rows.contains(&(0, 4)), "the ordinary grid has four columns");
    assert!(
        rows.contains(&(1, 1)),
        "the truncated event collapses to one line"
    );

    let truncated_row = {
        let mut query = world.query::<(&MonitorRow, Entity)>();
        query
            .iter(world)
            .find(|(row, _)| row.0 == 1)
            .map(|(_, entity)| entity)
            .expect("the truncated row")
    };
    let collapsed_child = world
        .get::<Children>(truncated_row)
        .expect("the collapsed row's one child")[0];
    let body_text = world
        .get::<Text>(collapsed_child)
        .expect("the collapsed line text")
        .as_str()
        .to_string();
    assert!(
        body_text.ends_with("[truncated]"),
        "the visible suffix: {body_text:?}"
    );
    assert!(!body_text.contains('\n'), "one logical row only");
    let warm = world
        .get::<TextColor>(collapsed_child)
        .expect("the collapsed line color")
        .0;
    assert_eq!(warm, crate::drawer::TONE_WARN, "the collapsed line is warm");
    assert_eq!(
        drawer_rows(&mut app)
            .iter()
            .find(|(sequence, part, _)| *sequence == 0 && *part == MonitorRowPart::Body)
            .map(|(_, _, text)| text.as_str()),
        Some("line one\\nline two \\\\n literal"),
        "real newline and literal backslash-n stay distinguishable in one row"
    );
}

/// A click outside the drawer neither closes it nor discards the suspended
/// draft; only the Monitor controls are inertly hit.
#[test]
fn outside_clicks_are_inert_while_open() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));

    let outside = app.world_mut().spawn_empty().id();
    outside_press(&mut app, outside);

    assert!(
        drawer_open(&app),
        "an outside click does not close the drawer"
    );
    assert_eq!(field_value(&app, field), "k", "the draft survived");
    assert!(menu(&app).key_editing, "editing stayed suspended");
}

/// Reader choices are launch-local: hiding and reopening, the overlay, a save
/// and a load never reset visibility, filter or the remembered control.
#[test]
fn reader_choices_survive_hide_reopen_and_session_transitions() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);

    tap_key(&mut app, KeyCode::F3);
    activate_control(&mut app, MonitorControl::ShowEverything);
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::FollowLatest);
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));

    tap_key(&mut app, KeyCode::F3);
    assert_eq!(
        drawer_view(&app),
        MonitorView::Everything,
        "filter survived"
    );
    assert_eq!(
        drawer_focus(&app),
        MonitorControl::FollowLatest,
        "focus survived"
    );
    tap_key(&mut app, KeyCode::F3);

    // The overlay and the player shortcuts are ordinary while Monitor is closed.
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    tap_key(&mut app, KeyCode::F5);
    tap_key(&mut app, KeyCode::F9);
    assert_eq!(
        drawer_view(&app),
        MonitorView::Everything,
        "a load or New Game does not reset the Monitor view"
    );
    assert_eq!(drawer_focus(&app), MonitorControl::FollowLatest);
}

/// A competing claimant is never displaced: the drawer still opens, but leaves
/// the other claim's protection in place and releases nothing on close.
#[test]
fn a_competing_claim_is_not_stolen() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    assert_eq!(
        app.world().resource::<InputOwnership>().claimed_by(),
        Some(InputClaim("seam-test")),
        "the drawer never steals another claimant's input"
    );
    tap_key(&mut app, KeyCode::F3);
    assert_eq!(
        app.world().resource::<InputOwnership>().claimed_by(),
        Some(InputClaim("seam-test")),
        "closing releases only the drawer's own claim"
    );
}

/// The reader commands address the log anywhere in the drawer and never leak
/// into the app underneath or move the three-control focus cycle.
#[test]
fn reader_commands_scroll_without_leaking_into_play() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    build_ui(&mut app);
    enter_play_with_input(&mut app);
    let parked = wait(&app);
    tap_key(&mut app, KeyCode::F3);
    let focus = drawer_focus(&app);
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_following());

    // Keyboard paging, Home and End address the log regardless of the focused
    // control.
    tap_key(&mut app, KeyCode::PageUp);
    assert!(
        !drawer_panel(&app).is_following(),
        "paging away disables following"
    );
    tap_key(&mut app, KeyCode::Home);
    assert_eq!(
        drawer_window(&mut app).first().copied(),
        Some(0),
        "Home selects the oldest displayed event"
    );
    tap_key(&mut app, KeyCode::End);
    assert!(
        drawer_panel(&app).is_following(),
        "End resumes following latest"
    );

    assert!(drawer_open(&app), "no reader command closes Monitor");
    assert_eq!(
        drawer_focus(&app),
        focus,
        "no reader command moves the control cycle"
    );
    assert_eq!(wait(&app), parked, "none of them leaked into play");
    assert!(!menu(&app).open, "none of them opened the overlay");

    // Unrelated keys still do nothing here or underneath.
    for key in [KeyCode::ArrowUp, KeyCode::ArrowDown] {
        tap_key(&mut app, key);
    }
    assert_eq!(drawer_focus(&app), focus);
    assert_eq!(wait(&app), parked);
    assert!(!menu(&app).open);

    // A wheel without a laid-out log body hit is swallowed, never leaked.
    app.world_mut()
        .write_message(bevy::input::mouse::MouseWheel {
            unit: bevy::input::mouse::MouseScrollUnit::Line,
            x: 0.0,
            y: -3.0,
            window: Entity::PLACEHOLDER,
            phase: bevy::input::touch::TouchPhase::Moved,
        });
    step_input(&mut app);
    assert!(drawer_panel(&app).is_following());
    assert_eq!(wait(&app), parked, "the wheel never leaked into play");
}

/// Keyboard activation is a fresh Enter/Space on the focused control; an OS
/// repeat or a held key activates once.
#[test]
fn controls_activate_by_fresh_keyboard_press_once() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F3);
    assert_eq!(drawer_focus(&app), MonitorControl::ShowEverything);

    // A held Enter with repeats toggles once; the repeats arrive while the key
    // is still down, exactly as the OS sends them.
    press_key(&mut app, KeyCode::Enter);
    step_input(&mut app);
    assert_eq!(drawer_view(&app), MonitorView::Everything);
    for _ in 0..4 {
        send_key_message(
            &mut app,
            KeyCode::Enter,
            bevy::input::ButtonState::Pressed,
            true,
        );
        step_input(&mut app);
        assert_eq!(drawer_view(&app), MonitorView::Everything, "one activation");
    }
    release_key(&mut app, KeyCode::Enter);
    step_input(&mut app);

    tap_key(&mut app, KeyCode::Space);
    assert_eq!(drawer_view(&app), MonitorView::UnmeiOnly);

    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::FollowLatest);
    tap_key(&mut app, KeyCode::Space);
    assert!(drawer_open(&app) && drawer_panel(&app).is_following());
}

/// Regaining window focus after a loss never replays activation, never quits
/// and adds no game pause.
#[test]
fn window_focus_regain_does_not_replay() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let parked = wait(&app);
    tap_key(&mut app, KeyCode::F3);
    let focus = drawer_focus(&app);

    let set_focus = |app: &mut App, focused: bool| {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.focused = focused;
    };
    set_focus(&mut app, false);
    step_input(&mut app);
    set_focus(&mut app, true);
    step_input(&mut app);

    assert!(
        drawer_open(&app),
        "the drawer survived the focus round trip"
    );
    assert_eq!(drawer_focus(&app), focus, "no activation was replayed");
    assert_eq!(wait(&app), parked, "no underlying action ran");
    assert_eq!(app.should_exit(), None, "no quit was manufactured");
}

/// Drive the runner to the demo pack's first player choice.
fn drive_runner_to_choice(runtime: &mut Runtime) {
    loop {
        let outcome = runtime.runner.run().unwrap();
        runtime.staging.apply_all(&outcome.staged);
        match outcome.wait {
            Some(Wait::Choice { .. }) => return,
            Some(Wait::Say { .. }) => runtime.runner.advance_say().unwrap(),
            other => panic!("expected a player choice, got {other:?}"),
        }
    }
}

/// F3 opens over a player-choice wait and a decision wait; a digit pressed
/// while open never picks the choice.
#[test]
fn f3_reaches_a_choice_wait_without_picking_it() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_choice(&mut runtime);
    }
    app.update();
    let parked = wait(&app);
    assert!(
        matches!(parked, Some(Wait::Choice { .. })),
        "parked on the choice: {parked:?}"
    );

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app), "F3 opens over a player choice");
    tap_key(&mut app, KeyCode::Digit1);
    assert_eq!(wait(&app), parked, "Monitor input never picks a choice");
    tap_key(&mut app, KeyCode::Escape);
    assert_eq!(wait(&app), parked, "closing never picks a choice either");
}

/// F3 opens over the overlay's Gallery/grid/still and Timeline views and changes
/// nothing about the underlying surface.
#[test]
fn f3_reaches_gallery_and_timeline_surfaces() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open, "the overlay is up");
    let focus_before = menu(&app).focus;

    for (view, still) in [
        (MenuView::Gallery, false),
        (MenuView::Gallery, true),
        (MenuView::Timeline, false),
    ] {
        app.world_mut().resource_mut::<MenuState>().view = view;
        app.world_mut().resource_mut::<MenuState>().gallery_still = still;
        step_input(&mut app);

        tap_key(&mut app, KeyCode::F3);
        assert!(drawer_open(&app), "F3 opens over {view:?} (still: {still})");
        tap_key(&mut app, KeyCode::Escape);
        assert!(
            !drawer_open(&app) && menu(&app).open,
            "Esc dismisses only Monitor"
        );
        assert_eq!(menu(&app).view, view, "the surface is unchanged");
        assert_eq!(menu(&app).gallery_still, still);
    }
    assert_eq!(menu(&app).focus, focus_before, "the selection is unchanged");
}

/// The grid's target column renders the escaped target; the original metadata
/// still drives filtering.
#[test]
fn the_target_column_renders_the_escaped_target() {
    let pack = temp_pack();
    let mut app = boot_app_with_input(pack.path().join("pack"), None);
    let mut bidi = test_monitor_line(0, "unmei::\u{202E}x", log::Level::Info, "bidi");
    bidi.target_display = "unmei::\\u{202e}x".to_string();
    seed_monitor_history(&mut app, vec![bidi]);
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..3 {
        step_input(&mut app);
    }

    let target = drawer_rows(&mut app)
        .into_iter()
        .find(|(sequence, part, _)| *sequence == 0 && *part == MonitorRowPart::Target)
        .map(|(_, _, text)| text)
        .expect("the rendered target column");
    assert_eq!(
        target, "unmei::\\u{202e}x",
        "the raw bidi control never renders"
    );
    assert!(
        !target.contains('\u{202E}'),
        "no raw bidi override reaches the UI"
    );
    // The original metadata still matches the exact-prefix filter.
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .targets(),
        vec!["unmei::\u{202E}x"]
    );
}

/// The queued-edit drain follows the ownership polarity exactly: while input is
/// blocked (claimed or quarantined) it consumes an edit the focus-change
/// producer queued, so the draft cannot mutate; once input is ready it is a
/// no-op, and the ordinary focus-loss discard then applies. Monitor never
/// *causes* a discard, but it does not hide a genuine focus loss after it stops
/// owning input.
#[test]
fn the_queued_edit_drain_follows_the_claim_polarity() {
    use bevy::input_focus::process_recorded_focus_changes;
    use bevy::text::apply_text_edits;

    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    assert_eq!(field_value(&app, field), "k");
    app.world_mut().resource_mut::<MenuState>().key_editing = true;

    // Queue the real focus-loss collapse edit through its real producer.
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .clear();
    app.world_mut()
        .run_system_once(process_recorded_focus_changes)
        .expect("the focus-change producer runs");
    assert!(
        !app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .pending_edits
            .is_empty(),
        "the focus-loss producer queued its collapse edit"
    );

    // Blocked: the drain consumes it, the draft survives, and apply is gated.
    app.world_mut()
        .resource_mut::<InputOwnership>()
        .claim(InputClaim("seam-test"));
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::discard_queued_key_edits_when_blocked)
        .expect("the drain runs");
    assert!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .pending_edits
            .is_empty(),
        "the blocked drain consumed the queued edit"
    );
    assert_eq!(field_value(&app, field), "k", "the draft is untouched");

    // Queue it again, then let the claim end: the drain no longer consumes, and
    // the ordinary apply performs the focus-loss discard.
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(field, bevy::input_focus::FocusCause::Navigated);
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .clear();
    app.world_mut()
        .run_system_once(process_recorded_focus_changes)
        .expect("the focus-change producer runs");
    app.world_mut().resource_mut::<InputOwnership>().release();
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::discard_queued_key_edits_when_blocked)
        .expect("the drain runs");
    assert!(
        !app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .pending_edits
            .is_empty(),
        "a ready drain is a no-op"
    );
    app.world_mut()
        .run_system_once(apply_text_edits)
        .expect("the apply runs");
    assert_eq!(
        field_value(&app, field),
        "k",
        "a collapse edit moves the caret; it never erases the draft"
    );
    assert!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .pending_edits
            .is_empty(),
        "the ordinary apply consumed the queued edit"
    );

    // With the claim gone and focus genuinely off the field, the ordinary
    // focused-editing sync discards the draft on the next full frame.
    app.update();
    assert_eq!(
        field_value(&app, field),
        "",
        "the ordinary focus-loss discard applied after the release"
    );
    assert!(!menu(&app).key_editing, "editing left with the focus");
}

/// A real clipboard paste while Monitor is open never reaches the suspended
/// draft, and it is not replayed after close; a fresh paste afterwards applies.
#[test]
fn a_paste_while_monitor_is_open_never_reaches_the_draft() {
    let pack = temp_pack();
    let (mut app, window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    let field = spawn_key_field(&mut app);
    type_character(&mut app, window, 'k');
    app.world_mut().resource_mut::<MenuState>().key_editing = true;
    *app.world_mut().resource_mut::<ClipboardPort>() =
        ClipboardPort::Memory(Some("sk-pasted\n".into()));

    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    paste(&mut app, window);
    assert_eq!(field_value(&app, field), "k", "the paste was blocked");

    tap_key(&mut app, KeyCode::Escape);
    assert!(!drawer_open(&app));
    step_input(&mut app);
    assert_eq!(
        field_value(&app, field),
        "k",
        "the claimed paste was never replayed"
    );

    paste(&mut app, window);
    assert_eq!(
        field_value(&app, field),
        "ksk-pasted",
        "a fresh paste applies after release"
    );
}
