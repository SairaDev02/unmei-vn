use super::*;

/// Open, then close with no action: the position is untouched either way.
fn assert_open_close_leaves_position(app: &mut App) {
    let before = parked_position(app);
    esc(app);
    assert!(menu(app).open, "Esc opened the overlay");
    assert_eq!(parked_position(app), before, "opening changes nothing");
    esc(app);
    assert!(!menu(app).open, "Esc closed the overlay");
    assert_eq!(parked_position(app), before, "closing changes nothing");
}

#[test]
fn esc_opens_and_closes_from_every_parked_state_unchanged() {
    let pack = temp_pack();
    let mut say = app_for_pack(pack.path().join("pack"));
    assert!(matches!(
        say.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Say { .. })
    ));
    assert_open_close_leaves_position(&mut say);
    assert!(
        !autosave_path(&say).exists(),
        "opening then closing writes no autosave"
    );

    let pack = temp_pack();
    let mut choice = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut choice, KeyCode::Space);
    press_and_update(&mut choice, KeyCode::Space);
    assert!(matches!(
        choice.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Choice { .. })
    ));
    assert_open_close_leaves_position(&mut choice);

    let pack = temp_pack();
    let mut ended = app_for_pack(pack.path().join("pack"));
    play_to_end(&mut ended);
    assert!(ended.world().resource::<Runtime>().runner.wait().is_none());
    assert_open_close_leaves_position(&mut ended);

    let fake = FakeResolver::new(model_answer("warm"));
    let (mut inflight, _dir) = app_parked_on_decision(&fake);
    assert!(inflight.world().resource::<PendingDecision>().0.is_some());
    assert_open_close_leaves_position(&mut inflight);
    assert_eq!(fake.calls(), 1, "the overlay never re-fires or cancels");
    assert!(
        inflight.world().resource::<PendingDecision>().0.is_some(),
        "the in-flight request is untouched by open/close"
    );
}

#[test]
fn menu_cards_render_exactly_what_list_slots_yields() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // A stamped manual save (slot 1) at the second say.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);

    // A v1-style save with no `saved_at` (slot 2): the card shows `—`, never a
    // fabricated time.
    {
        let runtime = app.world().resource::<Runtime>();
        let mut save = SaveFile::capture(&runtime.runner, &runtime.staging);
        save.saved_at = None;
        save::write_slot(runtime.runner.pack(), Slot::Manual(2), &save).unwrap();
    }

    esc(&mut app);

    let runtime = app.world().resource::<Runtime>();
    let infos = save::list_slots(runtime.runner.pack());
    let state = menu(&app);
    assert_eq!(
        state.slots,
        infos.to_vec(),
        "the cards cache list_slots verbatim"
    );

    let models = menu::card_models(state);
    assert_eq!(models.len(), 4, "four cards in Slot::ALL order");

    assert_eq!(models[0].label, "Slot 1");
    assert_eq!(models[0].kind, "manual");
    assert_eq!(models[0].status, "Ok");
    assert_eq!(models[0].detail, "first_meeting");
    assert_ne!(models[0].saved_at, "—", "the app stamp is formatted");
    assert_eq!(models[0].action.label, "Overwrite");
    assert_eq!(models[0].action.action, Some(SlotAction::Write));
    assert!(models[0].action.danger);

    assert_eq!(models[1].status, "Ok");
    assert_eq!(models[1].saved_at, "—", "an unstamped v1 save renders `—`");
    assert_eq!(models[1].action.label, "Overwrite");

    assert_eq!(models[2].status, "Empty");
    assert_eq!(models[2].detail, "—");
    assert_eq!(models[2].saved_at, "no save yet");
    assert_eq!(models[2].action.label, "Save here");
    assert_eq!(models[2].action.action, Some(SlotAction::Write));

    assert_eq!(models[3].label, "Autosave");
    assert_eq!(models[3].kind, "game-written");
    assert_eq!(models[3].action.label, "Load-only");
    assert!(!models[3].action.enabled);
    assert_eq!(models[3].action.action, None);
}

#[test]
fn autosave_offers_no_save_action_and_unreadable_keeps_overwrite() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // Resolve the choice: the autosave is written.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    // An unreadable manual slot beside it.
    std::fs::write(manual_path(&app), b"garbage bytes").unwrap();

    esc(&mut app);
    let models = menu::card_models(menu(&app));
    assert_eq!(models[3].status, "Ok");
    assert_eq!(models[3].action.label, "Load-only");
    assert!(!models[3].action.enabled, "the autosave offers no save");
    assert_eq!(models[3].action.action, None);

    assert_eq!(models[0].status, "Unreadable");
    assert!(
        models[0].detail.contains("invalid"),
        "{:?}",
        models[0].detail
    );
    assert_eq!(models[0].action.label, "Overwrite");
    assert_eq!(models[0].action.action, Some(SlotAction::Write));

    switch_mode(&mut app, MenuMode::Load);
    let models = menu::card_models(menu(&app));
    assert_eq!(models[3].action.label, "Load");
    assert_eq!(models[3].action.action, Some(SlotAction::Load));
    assert_eq!(models[0].action.label, "Overwrite", "still overwrite-only");
    assert_eq!(models[0].action.action, Some(SlotAction::Write));
    // An empty manual slot still offers `Save here`: `docs/runtime-hardening.md`
    // § Menu overlay §3 qualifies only the occupied and autosave rows by mode.
    assert_eq!(models[1].status, "Empty");
    assert_eq!(models[1].action.label, "Save here");
    assert_eq!(models[1].action.action, Some(SlotAction::Write));
}

#[test]
fn save_over_empty_is_immediate_and_overwrite_confirms_once() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    esc(&mut app);
    assert_eq!(menu(&app).focus, MenuFocus::Card(0));
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, None, "save-over-empty asks nothing");
    assert!(manual_path(&app).exists(), "the empty slot was written");
    assert!(matches!(menu(&app).slots[0].status, SlotStatus::Ok { .. }));

    // Move on, then reopen: the occupied card confirms once.
    esc(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    let slot = manual_path(&app);
    let before = std::fs::read(&slot).unwrap();
    esc(&mut app);

    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::Overwrite(Slot::Manual(1)))
    );
    esc(&mut app);
    assert_eq!(menu(&app).confirm, None, "Esc cancelled the confirm");
    assert!(
        menu(&app).open,
        "Esc cancelled the confirm, not the overlay"
    );
    assert_eq!(
        std::fs::read(&slot).unwrap(),
        before,
        "cancelling leaves the file byte-identical"
    );

    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::Overwrite(Slot::Manual(1)))
    );
    press_and_update(&mut app, KeyCode::Enter);
    let runtime = app.world().resource::<Runtime>();
    let reloaded = save::read_slot(runtime.runner.pack(), Slot::Manual(1)).unwrap();
    assert_eq!(
        reloaded.wait,
        runtime.runner.wait().cloned(),
        "the overwrite captured the current park"
    );
}

#[test]
fn load_confirms_once_and_cancel_leaves_position_and_file_identical() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);
    let saved_wait = app.world().resource::<Runtime>().runner.wait().cloned();
    press_and_update(&mut app, KeyCode::Space);
    let moved_wait = app.world().resource::<Runtime>().runner.wait().cloned();
    assert_ne!(saved_wait, moved_wait);
    let slot = manual_path(&app);
    let before = std::fs::read(&slot).unwrap();

    esc(&mut app);
    switch_mode(&mut app, MenuMode::Load);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::Card(0);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::Load(Slot::Manual(1))));
    esc(&mut app);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(menu(&app).mode, MenuMode::Load);
    assert_eq!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        moved_wait,
        "a cancelled load does not move the runner"
    );
    assert_eq!(std::fs::read(&slot).unwrap(), before);

    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "a load closes the overlay");
    assert_eq!(
        app.world().resource::<Runtime>().runner.wait().cloned(),
        saved_wait
    );
}

#[test]
fn a_load_refused_under_the_overlay_notices_row_17() {
    captures::install();
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);

    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);
    esc(&mut app);
    switch_mode(&mut app, MenuMode::Load);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::Card(0);

    // The slot changes under the overlay, after its card was classified.
    std::fs::write(manual_path(&app), b"definitely not a save").unwrap();
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    press_and_update(&mut app, KeyCode::Enter); // confirm
    press_and_update(&mut app, KeyCode::Enter); // accept

    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert!(
        records[0].2.starts_with("couldn't load that save"),
        "{:?}",
        records[0].2
    );
    assert!(menu(&app).open, "a refused load leaves the overlay open");
    assert!(app.world().resource::<Notice>().is_showing());
}

#[test]
fn manual_save_succeeds_from_every_parked_state_including_in_flight() {
    let pack = temp_pack();
    let mut say = app_for_pack(pack.path().join("pack"));
    esc(&mut say);
    press_and_update(&mut say, KeyCode::Enter);
    assert_eq!(menu(&say).confirm, None);
    assert!(manual_path(&say).exists());

    let pack = temp_pack();
    let mut choice = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut choice, KeyCode::Space);
    press_and_update(&mut choice, KeyCode::Space);
    esc(&mut choice);
    press_and_update(&mut choice, KeyCode::Enter);
    assert!(manual_path(&choice).exists());

    let pack = temp_pack();
    let mut ended = app_for_pack(pack.path().join("pack"));
    play_to_end(&mut ended);
    esc(&mut ended);
    press_and_update(&mut ended, KeyCode::Enter);
    assert!(manual_path(&ended).exists());

    let fake = FakeResolver::new(model_answer("warm"));
    let (mut inflight, _dir) = app_parked_on_decision(&fake);
    esc(&mut inflight);
    assert!(matches!(
        inflight.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));
    press_and_update(&mut inflight, KeyCode::Enter);
    assert!(manual_path(&inflight).exists());
    assert!(
        inflight.world().resource::<PendingDecision>().0.is_some(),
        "a manual save never cancels the request"
    );
    let save = save::read_slot(
        inflight.world().resource::<Runtime>().runner.pack(),
        Slot::Manual(1),
    )
    .unwrap();
    assert!(matches!(save.wait, Some(Wait::Decision { .. })));
}

#[test]
fn keyboard_focus_walks_the_grid_and_cycles_the_top_bar() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);

    assert_eq!(menu(&app).focus, MenuFocus::Card(0));
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::Card(1));
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::Card(3));
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::Card(1));
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::Card(0));

    // Digits focus a manual slot (1–3) without activating it.
    press_and_update(&mut app, KeyCode::Digit2);
    assert_eq!(menu(&app).focus, MenuFocus::Card(1));

    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::LoadTab);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineButton);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryButton);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SettingsButton);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab);

    // Left from the grid's left column reaches the rail's Resume, which is
    // keyboard-activatable like every other control.
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::Card(0));
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::Resume);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Resume is keyboard-activatable");
}

#[test]
fn every_control_activates_by_click() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);
    esc(&mut app);

    click(&mut app, MenuAction::SetMode(MenuMode::Load));
    assert_eq!(menu(&app).mode, MenuMode::Load);

    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).view, MenuView::Settings);

    click(&mut app, MenuAction::BackToSlots);
    assert_eq!(menu(&app).view, MenuView::Slots);

    click(&mut app, MenuAction::Card(0));
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::Load(Slot::Manual(1))));

    click(&mut app, MenuAction::ConfirmCancel);
    assert_eq!(menu(&app).confirm, None);

    click(&mut app, MenuAction::Resume);
    assert!(!menu(&app).open);
}

#[test]
fn settings_route_is_reachable_and_esc_backs_out_to_the_grid() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);

    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SettingsButton);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Settings);
    assert_eq!(menu(&app).focus, MenuFocus::AudioMaster);

    // Up wraps to the rail's session-scoped `Exit to title`, the walk's one
    // stop past the Decisions group; Up again reaches the group's last control.
    // The back control is a click/`Esc` affordance, not an arrow stop.
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::ExitToTitle);
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::ClearKey);
    click(&mut app, MenuAction::BackToSlots);
    assert_eq!(menu(&app).view, MenuView::Slots);

    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).view, MenuView::Settings);
    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert!(
        menu(&app).open,
        "Esc backed out to the grid, not the overlay"
    );
    esc(&mut app);
    assert!(!menu(&app).open);
}

#[test]
fn the_focused_control_is_highlighted_in_every_state() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);

    let spawn = |app: &mut App, action: MenuAction| {
        app.world_mut()
            .spawn((action, BorderColor::all(Color::NONE)))
            .id()
    };
    let resume = spawn(&mut app, MenuAction::Resume);
    let save_tab = spawn(&mut app, MenuAction::SetMode(MenuMode::Save));
    let load_tab = spawn(&mut app, MenuAction::SetMode(MenuMode::Load));
    let gallery_button = spawn(&mut app, MenuAction::OpenGallery);
    let settings = spawn(&mut app, MenuAction::OpenSettings);
    let back = spawn(&mut app, MenuAction::BackToSlots);
    let card0 = spawn(&mut app, MenuAction::Card(0));
    let legacy_card = spawn(&mut app, MenuAction::LegacyImport);
    let legacy_target0 = spawn(&mut app, MenuAction::LegacyTarget(Slot::Manual(1)));
    let legacy_cancel = spawn(&mut app, MenuAction::LegacyCancel);
    let controls = [
        resume,
        save_tab,
        load_tab,
        gallery_button,
        settings,
        back,
        card0,
        legacy_card,
        legacy_target0,
        legacy_cancel,
    ];

    let highlighted = |app: &App, entity: Entity| {
        app.world().get::<BorderColor>(entity).unwrap().top != Color::NONE
    };

    for (focus, expected) in [
        (MenuFocus::Card(0), card0),
        (MenuFocus::Resume, resume),
        (MenuFocus::SaveTab, save_tab),
        (MenuFocus::LoadTab, load_tab),
        (MenuFocus::GalleryButton, gallery_button),
        (MenuFocus::SettingsButton, settings),
        (MenuFocus::SettingsBack, back),
        (MenuFocus::LegacyCard, legacy_card),
        (MenuFocus::LegacyTarget(0), legacy_target0),
        (MenuFocus::LegacyCancel, legacy_cancel),
    ] {
        app.world_mut().resource_mut::<MenuState>().focus = focus;
        app.world_mut()
            .run_system_once(crate::presentation::sync_menu_focus)
            .expect("the focus system runs");
        assert!(highlighted(&app, expected), "{focus:?} is highlighted");
        for entity in controls {
            if entity != expected {
                assert!(!highlighted(&app, entity), "{focus:?} leaves others plain");
            }
        }
    }
}
