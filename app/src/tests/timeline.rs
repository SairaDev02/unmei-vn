use super::*;

/// A copied demo pack whose manifest declares the given scene order. `None`
/// omits the field (no declared order); `Some(&[])` writes an explicit empty
/// list. The demo scenes stay as authored.
fn temp_pack_with_order(order: Option<&[&str]>) -> tempfile::TempDir {
    let dir = temp_pack();
    let root = dir.path().join("pack");
    let scenes = match order {
        None => String::new(),
        Some(list) => format!(
            "    scenes: [{}],\n",
            list.iter()
                .map(|id| format!("\"{id}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    std::fs::write(
        root.join("pack.ron"),
        format!("(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n{scenes})\n"),
    )
    .unwrap();
    dir
}

/// Open the overlay (a live session must be entered) and its Timeline door.
fn open_timeline(app: &mut App) {
    esc(app);
    assert!(menu(app).open, "the overlay opened");
    assert_eq!(menu(app).view, MenuView::Slots);
    click(app, MenuAction::OpenTimeline);
    assert_eq!(menu(app).view, MenuView::Timeline);
    build_pause_tree(app);
}

/// The built Timeline cards' labels in declared order. Builds the pause tree
/// first.
fn timeline_labels(app: &mut App) -> Vec<String> {
    build_pause_tree(app);
    let mut cards: Vec<(usize, String)> = app
        .world()
        .iter_entities()
        .filter_map(|entity| {
            let MenuAction::TimelineCard(index) = entity.get::<MenuAction>()? else {
                return None;
            };
            let text = entity.get::<Text>()?;
            Some((*index, text.0.clone()))
        })
        .collect();
    cards.sort_by_key(|(index, _)| *index);
    cards.into_iter().map(|(_, label)| label).collect()
}

/// An app already in play over a demo pack declaring `order`.
fn timeline_app(order: &[&str]) -> (App, tempfile::TempDir) {
    let pack = temp_pack_with_order(Some(order));
    let root = pack.path().join("pack");
    let app = app_for_pack(root);
    (app, pack)
}

#[test]
fn timeline_replaces_absent_scenes_with_the_exact_empty_copy_and_keeps_the_door() {
    for order in [None, Some(&[][..])] {
        let pack = temp_pack_with_order(order);
        let mut app = app_for_pack(pack.path().join("pack"));
        esc(&mut app);
        build_pause_tree(&mut app);
        assert!(
            has_action(&app, MenuAction::OpenTimeline),
            "the door is enabled without an order"
        );
        click(&mut app, MenuAction::OpenTimeline);
        assert_eq!(menu(&app).view, MenuView::Timeline);
        build_pause_tree(&mut app);
        assert!(
            !has_action(&app, MenuAction::TimelineCard(0)),
            "no cards render"
        );
        let texts = all_texts(&app);
        assert!(
            texts.iter().any(|t| t == menu::TIMELINE_EMPTY),
            "the exact empty copy renders: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == menu::TIMELINE_EXPLANATION),
            "the explanation renders"
        );

        click(&mut app, MenuAction::TimelineBack);
        assert_eq!(menu(&app).view, MenuView::Slots);
        click(&mut app, MenuAction::Resume);
        assert!(!menu(&app).open);
    }
}

#[test]
fn every_declared_scene_renders_once_in_order_and_unlisted_scenes_do_not() {
    // `reunion` is loaded but unlisted, so it never renders.
    let (mut app, _pack) = timeline_app(&["first_meeting", "epilogue"]);
    open_timeline(&mut app);
    assert_eq!(timeline_labels(&mut app), vec!["first_meeting", "???"]);
    assert!(!has_action(&app, MenuAction::TimelineCard(2)));
}

#[test]
fn focus_starts_at_the_first_visited_declared_card_otherwise_the_first_declared() {
    // Entry `a` is unlisted and the only scene play visits; `c` is seeded
    // visited, so the first selectable declared card is `c`.
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: b.\n"), ("c", "C: c.\n")],
        Some(&["b", "c"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["c"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(1));
    assert_eq!(timeline_labels(&mut app), vec!["???", "c"]);

    // All declared cards locked: focus falls to the first declared card, which
    // is still focusable though activation is inert.
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: b.\n"), ("c", "C: c.\n")],
        Some(&["b", "c"]),
        "a",
    );
    let mut app = app_for_pack(pack.path().join("pack"));
    open_timeline(&mut app);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(0));
    assert_eq!(timeline_labels(&mut app), vec!["???", "???"]);
}

#[test]
fn reopening_forgets_the_previous_focus() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(2);
    click(&mut app, MenuAction::TimelineBack);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert_eq!(menu(&app).focus, MenuFocus::Card(0));
    click(&mut app, MenuAction::OpenTimeline);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::TimelineCard(0),
        "focus starts afresh at the first selectable card"
    );
}

#[test]
fn timeline_move_steps_plus_minus_one_and_two_and_clamps_without_wrapping() {
    assert_eq!(menu::timeline_move(3, 5, menu::GalleryDir::Left), Some(2));
    assert_eq!(menu::timeline_move(3, 5, menu::GalleryDir::Right), Some(4));
    assert_eq!(menu::timeline_move(3, 5, menu::GalleryDir::Up), Some(1));
    assert_eq!(menu::timeline_move(1, 5, menu::GalleryDir::Down), Some(3));
    assert_eq!(menu::timeline_move(0, 5, menu::GalleryDir::Left), None);
    assert_eq!(menu::timeline_move(0, 5, menu::GalleryDir::Up), None);
    assert_eq!(menu::timeline_move(4, 5, menu::GalleryDir::Right), None);
    assert_eq!(menu::timeline_move(4, 5, menu::GalleryDir::Down), None);
    // Diagonal end clamping is intentional: index 1 up lands on 0.
    assert_eq!(menu::timeline_move(1, 5, menu::GalleryDir::Up), Some(0));
    assert_eq!(menu::timeline_move(0, 0, menu::GalleryDir::Left), None);
}

#[test]
fn the_walk_includes_locked_cards_and_activation_of_a_locked_card_is_inert() {
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: b.\n"), ("c", "C: c.\n")],
        Some(&["b", "c"]),
        "a",
    );
    let mut app = app_for_pack(pack.path().join("pack"));
    open_timeline(&mut app);
    let before = parked_position(&app);

    // Both cards are locked; the walk still reaches the next one.
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(1));

    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, None);
    press_and_update(&mut app, KeyCode::Space);
    assert_eq!(menu(&app).confirm, None);
    click(&mut app, MenuAction::TimelineCard(1));
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(parked_position(&app), before, "the session is untouched");
    assert_eq!(menu(&app).view, MenuView::Timeline);
}

#[test]
fn the_confirm_copy_is_exact_and_uses_jump_and_cancel() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    open_timeline(&mut app);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::TimelineJump(0)),
        "the visited current card confirms"
    );
    build_pause_tree(&mut app);
    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|t| t == "Jump to first_meeting?"),
        "the title names the raw target: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == menu::TIMELINE_CONFIRM_DETAIL),
        "the detail is verbatim"
    );
    assert!(
        texts.iter().any(|t| t == "Jump"),
        "the accept label is Jump"
    );
    assert!(texts.iter().any(|t| t == "Cancel"));

    // A raw scene id is one unbroken token; the title opts into character
    // fallback so a long id wraps inside the 420px panel instead of
    // overflowing it (`docs/title-and-menu.md` § Timeline §3).
    let title = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            let text = entity.get::<Text>()?;
            (text.0 == "Jump to first_meeting?").then_some(entity.id())
        })
        .expect("the confirm title node");
    assert_eq!(
        app.world().get::<TextLayout>(title).map(|l| l.linebreak),
        Some(LineBreak::WordOrCharacter)
    );
}

#[test]
fn cancelling_the_confirm_restores_the_target_focus_and_touches_nothing() {
    // Seed `epilogue` visited so the confirm targets a non-default third card,
    // proving the cancel restores the target rather than the first card.
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion", "epilogue"]));
    let root = pack.path().join("pack");
    seed_visited(&root, &["epilogue"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);
    let before = parked_position(&app);
    let log_before = std::fs::read(read_log_path(&app)).unwrap_or_default();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(2);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::TimelineJump(2)));
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(2));
    assert_eq!(menu(&app).view, MenuView::Timeline);
    assert_eq!(parked_position(&app), before);
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap_or_default(),
        log_before,
        "cancelling writes no file"
    );
}

#[test]
fn cancelling_the_confirm_restores_the_scrolled_body_offset() {
    let pack = temp_pack_with_many_entries(30, false);
    let mut app = app_for_pack(pack.path().join("pack"));
    open_timeline(&mut app);
    build_pause_tree(&mut app);

    // Scroll the body far down, as the arrow walk to a late card would.
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    let scrolled = ScrollPosition(Vec2::new(0.0, 860.0));
    app.world_mut().entity_mut(body).insert(scrolled);

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(29);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::TimelineJump(29)));
    build_pause_tree(&mut app);
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    assert_eq!(
        app.world()
            .get::<ScrollPosition>(body)
            .map(|position| position.0),
        Some(Vec2::new(0.0, 860.0)),
        "the card behind the confirm keeps its offset"
    );

    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(29));
    build_pause_tree(&mut app);
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    assert_eq!(
        app.world()
            .get::<ScrollPosition>(body)
            .map(|position| position.0),
        Some(Vec2::new(0.0, 860.0)),
        "Cancel restores the scroll the walk had"
    );

    // The carry is confirm-only: a fresh open still starts at the top.
    click(&mut app, MenuAction::TimelineBack);
    build_pause_tree(&mut app);
    click(&mut app, MenuAction::OpenTimeline);
    build_pause_tree(&mut app);
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    assert_eq!(
        app.world()
            .get::<ScrollPosition>(body)
            .map(|position| position.0),
        Some(Vec2::ZERO),
        "reopening forgets the previous scroll"
    );
}

/// The coordinator's rebuild boundary (ADR 0013). A focus move or a credential
/// label change repaints in place and never rebuilds the tree; a structural
/// change rebuilds it; and only a confirmation change carries the Timeline
/// body's scroll offset across that rebuild.
#[test]
fn the_coordinator_rebuilds_only_on_structure_and_carries_scroll_on_confirm() {
    let pack = temp_pack_with_many_entries(30, false);
    let mut app = app_for_pack(pack.path().join("pack"));
    app.init_resource::<CredentialsState>();
    app.world_mut().resource_mut::<MenuState>().open = true;
    app.world_mut().resource_mut::<MenuState>().view = MenuView::Settings;
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::AudioMaster;
    build_pause_tree(&mut app);
    let settings_root = entity_with::<crate::presentation::MenuRoot>(&app);

    // Focus is not part of the signature: the border repaints, the tree stays.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::KeyRow;
    build_pause_tree(&mut app);
    assert_eq!(
        entity_with::<crate::presentation::MenuRoot>(&app),
        settings_root,
        "a focus move repaints in place"
    );

    // A credential label change is realized by the view's own in-place sync, so
    // the coordinator does not rebuild for it.
    app.world_mut().resource_mut::<CredentialsState>().row = KeyRow::Set {
        fingerprint: "9F2A".to_string(),
    };
    build_pause_tree(&mut app);
    assert_eq!(
        entity_with::<crate::presentation::MenuRoot>(&app),
        settings_root,
        "a credential-label change repaints in place"
    );
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_key_row)
        .expect("sync_key_row runs");
    assert!(
        app.world().iter_entities().any(|entity| {
            entity
                .get::<Text>()
                .is_some_and(|text| text.0 == "Set \u{2014} 9F2A")
        }),
        "the row repaints the new label in place"
    );

    app.world_mut().resource_mut::<MenuState>().view = MenuView::Timeline;
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(29);
    build_pause_tree(&mut app);
    let timeline_root = entity_with::<crate::presentation::MenuRoot>(&app);
    assert_ne!(
        timeline_root, settings_root,
        "a structural change rebuilds the tree"
    );

    // Only a confirmation change carries the body's offset across the rebuild:
    // a walk inside the Timeline leaves both the tree and the offset alone.
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    app.world_mut()
        .entity_mut(body)
        .insert(ScrollPosition(Vec2::new(0.0, 860.0)));
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(28);
    build_pause_tree(&mut app);
    assert_eq!(
        entity_with::<crate::presentation::MenuRoot>(&app),
        timeline_root,
        "the walk repaints without a rebuild"
    );

    app.world_mut().resource_mut::<MenuState>().confirm = Some(ConfirmKind::TimelineJump(28));
    build_pause_tree(&mut app);
    assert_ne!(
        entity_with::<crate::presentation::MenuRoot>(&app),
        timeline_root,
        "the confirmation rebuilds the tree"
    );
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    assert_eq!(
        app.world()
            .get::<ScrollPosition>(body)
            .map(|position| position.0),
        Some(Vec2::new(0.0, 860.0)),
        "the confirmation carries the body's scroll offset"
    );
}

#[test]
fn esc_winds_the_timeline_back_to_slots_then_resumes() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion"]);
    open_timeline(&mut app);
    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert_eq!(menu(&app).focus, MenuFocus::Card(0));
    assert!(menu(&app).open);
    esc(&mut app);
    assert!(!menu(&app).open);
}

#[test]
fn a_jump_restores_the_target_from_pc_zero_with_carried_staging_and_state() {
    let pack = temp_custom_pack(
        &[
            (
                "a",
                "set affinity_level = \"high\"\nbg gate\nplay music_a\nA: one.\n",
            ),
            ("b", "A: two.\n"),
        ],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_for_pack(root);
    // State and staging from scene `a` are carried into the jump.
    {
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.runner.scene_id(), "a");
        assert_eq!(runtime.staging.background.as_deref(), Some("gate"));
        assert_eq!(runtime.staging.bgm.as_deref(), Some("music_a"));
    }

    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    // The jump is a scene entry, so it announces the scene it lands in; a
    // single-threaded `Update` keeps the log record on this test's thread.
    captures::install();
    force_single_threaded(&mut app);
    captures::take();
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    let records = captures::take();
    assert!(
        records
            .iter()
            .any(|(_, _, message)| message == "unmei: entered scene b"),
        "the confirmed jump is a scene entry: {records:?}"
    );
    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), "b");
    let Some(Wait::Say { text, .. }) = runtime.runner.wait() else {
        panic!(
            "expected a say on the landing, got {:?}",
            runtime.runner.wait()
        );
    };
    assert_eq!(text, "two.");
    assert_eq!(
        runtime.staging.background.as_deref(),
        Some("gate"),
        "the target's absent bg keeps the carried staging"
    );
    assert_eq!(runtime.staging.bgm.as_deref(), Some("music_a"));
    assert_eq!(
        runtime.runner.state().get("affinity_level"),
        Some(&StateValue::Str("high".to_string())),
        "the state map is carried"
    );
    assert!(!menu(&app).open, "the overlay closes on the landing");
}

#[test]
fn a_jump_cancels_a_pending_request_and_discards_a_held_answer() {
    // Pending: the request is in flight when the jump is confirmed.
    let fake = FakeResolver::new(model_answer("warm"));
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let dst = pack.path().join("pack");
    let mut app = app_with_decisions_at(dst, Some(Arc::new(fake.clone())));
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    app.update();
    pump_until(&mut app, |_| fake.calls() == 1);
    assert!(app.world().resource::<PendingDecision>().0.is_some());

    open_timeline(&mut app);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept
    assert!(app.world().resource::<PendingDecision>().0.is_none());
    fake.wait_until_dropped();
    assert!(app.world().resource::<HeldDecision>().0.is_none());

    // Held: the request answered behind the overlay, then the jump discards it.
    let fake = FakeResolver::new(model_answer("warm"));
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let dst = pack.path().join("pack");
    let mut app = app_with_decisions_at(dst, Some(Arc::new(fake.clone())));
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    app.update();
    pump_until(&mut app, |_| fake.calls() == 1);
    esc(&mut app);
    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app));

    click(&mut app, MenuAction::OpenTimeline);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept
    assert!(!held(&app), "the held answer is discarded");
    assert!(
        !autosave_path(&app).exists(),
        "an old answer never captures"
    );
}

#[test]
fn a_held_ctrl_through_a_timeline_jump_disarms_and_needs_a_fresh_press() {
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let root = pack.path().join("pack");
    seed_visited(&root, &["reunion"]);
    let mut app = boot_app_with_input(root, None);
    enter_play_with_input(&mut app);

    // A genuine physical hold, armed in play.
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app));
    app.world_mut().resource_mut::<Notice>().show("something");

    // Keep Ctrl down through the overlay, the Timeline door, the confirm and
    // the jump — the real keyboard path, not a `reset_all` simulation.
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    assert!(skip_force(&app), "a hold survives the overlay pause");
    click_input(&mut app, MenuAction::OpenTimeline);
    assert_eq!(menu(&app).view, MenuView::Timeline);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    click_input(&mut app, MenuAction::TimelineCard(1));
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::TimelineJump(1)));
    click_input(&mut app, MenuAction::ConfirmAccept);

    // The landing disarms both modes: the still-physically-down Ctrl cannot
    // skip the replacement position, even after further frames.
    assert!(!menu(&app).open, "the jump closed the overlay");
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "reunion"
    );
    assert!(!skip_force(&app), "the held Ctrl is disarmed by the jump");
    assert!(!skip_on(&app));
    assert!(
        app.world().resource::<Notice>().text.is_none(),
        "the old Notice is cleared"
    );
    let landed = wait(&app).expect("the landing parked on a say");
    for _ in 0..3 {
        advance_time(&mut app, 0.05);
        step_input(&mut app);
    }
    assert_eq!(
        wait(&app),
        Some(landed.clone()),
        "no skip runs while Ctrl is still down"
    );

    release_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(!skip_force(&app));
    press_key(&mut app, KeyCode::ControlLeft);
    step_input(&mut app);
    assert!(skip_force(&app), "a fresh press in play re-arms");
    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_ne!(
        wait(&app),
        Some(landed),
        "the fresh hold skips the landing line"
    );
}

#[test]
fn a_same_scene_restart_recomputes_arrival_and_writes_nothing_when_clean() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    // The entry line was marked and flushed on play entry, so the log is clean.
    let before = std::fs::read(read_log_path(&app)).unwrap();
    open_timeline(&mut app);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept, restart first_meeting

    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), "first_meeting");
    assert!(matches!(runtime.runner.wait(), Some(Wait::Say { .. })));
    assert_eq!(
        read_state(&app).arrival_read(),
        Some(true),
        "the landing line was read before this visit"
    );
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap(),
        before,
        "a clean same-scene restart writes no new knowledge"
    );
}

#[test]
fn a_jump_lands_only_on_its_displayed_line_and_leaves_the_rest_unread() {
    // Scene `b` is visited but never read, so its landing marks only its first
    // displayed line.
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: first.\nB: second.\n")],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    let state = read_state(&app);
    assert_eq!(state.arrival_read(), Some(false));
    assert_eq!(
        state.log.read.get("b"),
        // Line 1 is the first authored line of scene `b`; no directive precedes
        // it, so the landing say-line's authored source line is 1.
        Some(&std::collections::BTreeSet::from([1])),
        "only the displayed landing line is marked"
    );
    assert!(state.is_visited("b"));
}

#[test]
fn a_restore_refusal_stays_on_the_timeline_and_preserves_the_session() {
    captures::install();
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);
    open_timeline(&mut app);

    // Simulate a pack whose declared order names a scene it does not contain:
    // the loader validates this away in normal play, so install the mutated
    // pack directly to reach the defensive restore path.
    let mut mutated = app.world().resource::<Runtime>().runner.pack().clone();
    mutated.manifest.scenes.insert(0, "ghost".to_string());
    app.world_mut().resource_mut::<Runtime>().runner = ScriptRunner::new(mutated);
    app.world_mut()
        .resource_mut::<crate::read_log::ReadLogState>()
        .log
        .mark_visited("ghost");

    // Re-open so the confirm targets the ghost card.
    click(&mut app, MenuAction::TimelineBack);
    click(&mut app, MenuAction::OpenTimeline);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(0));
    let before = parked_position(&app);
    captures::take();
    app.world_mut().resource_mut::<Notice>().text = None;

    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept the refused jump

    assert_eq!(menu(&app).view, MenuView::Timeline, "Timeline stays open");
    assert_eq!(
        parked_position(&app),
        before,
        "the old runner position is preserved"
    );
    let records = failure_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert!(
        records[0].2.starts_with("the story can't continue"),
        "the defensive funnel's existing string: {:?}",
        records[0].2
    );
    assert!(
        app.world().resource::<DefensiveHold>().is_held(),
        "a refused restore engages the defensive hold"
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("the story can't continue — Esc for the menu")
    );
}

#[test]
fn a_jump_selection_writes_no_autosave_even_with_a_stale_pending_flag() {
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: two.\n")],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_for_pack(root);

    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    press_and_update(&mut app, KeyCode::Enter); // open the confirm
                                                // `write_pending_autosave` consumes an armed flag on the next frame, so
                                                // arming it before the jump would legitimately write. Arming it in the same
                                                // frame as the accept proves the jump's disarm runs before that writer.
    app.world_mut()
        .resource_mut::<crate::slots::PendingAutosave>()
        .arm();
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(
        !autosave_path(&app).exists(),
        "the selection itself writes no autosave"
    );
}

#[test]
fn a_jump_leaves_a_pre_jump_autosave_untouched_then_a_real_crossing_writes() {
    // Play the demo to the reunion crossing so an autosave exists.
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion", "epilogue"]));
    let root = pack.path().join("pack");
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // cross into reunion
    let autosave_before = std::fs::read(autosave_path(&app)).expect("a crossing autosaved");

    // Jump back to the visited entry scene; the autosave is untouched.
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(0);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        autosave_before,
        "the selection writes no autosave"
    );

    // A later normal crossing still writes: remove the pre-jump file and prove
    // the crossing recreates it (its bytes may coincide within the same second).
    std::fs::remove_file(autosave_path(&app)).unwrap();
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // jump to reunion
    app.update();
    assert!(
        autosave_path(&app).exists(),
        "a normal authored crossing still autosaves"
    );
    let after = save::read_slot(
        app.world().resource::<Runtime>().runner.pack(),
        Slot::Autosave,
    )
    .expect("a real autosave content");
    assert_eq!(after.scene_id, "reunion");
}

#[test]
fn a_same_scene_restart_flushes_pending_knowledge_to_the_sibling_file() {
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let mut app = app_for_pack(pack.path().join("pack"));

    // The play-entry flush wrote the first line; the next park's mark is dirty
    // because an in-scene advance is not a flush moment.
    press_and_update(&mut app, KeyCode::Space);
    assert!(read_state(&app).is_read("first_meeting", 10));
    assert!(read_state(&app).is_dirty());
    assert!(
        !read_log_on_disk(&app).unwrap().is_read("first_meeting", 10),
        "the second line's mark is not on disk yet"
    );

    // A same-scene restart is a scene-entry flush opportunity of its own.
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(0);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "first_meeting"
    );
    assert_eq!(read_state(&app).arrival_read(), Some(true));
    assert!(!read_state(&app).is_dirty(), "the landing flushed");
    assert!(
        read_log_on_disk(&app).unwrap().is_read("first_meeting", 10),
        "the same-scene landing wrote the pending knowledge"
    );
}

#[test]
fn a_different_scene_landing_flushes_and_a_failed_flush_retries() {
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let root = pack.path().join("pack");
    seed_visited(&root, &["reunion"]);
    let mut app = app_for_pack(root);

    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);

    // Block the flush: the temp sibling's path is a directory, so the write
    // fails after the landing's mark.
    let tmp = read_log_path(&app).with_extension("ron.tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "reunion"
    );
    assert!(
        read_state(&app).is_read("reunion", 7),
        "the landing marked its displayed line"
    );
    assert!(read_state(&app).is_dirty(), "a failed flush keeps the flag");
    assert!(
        !read_log_on_disk(&app).unwrap().is_read("reunion", 7),
        "the previous bytes stay"
    );

    // Unblock and reach the next flush moment (app exit): the retry succeeds.
    std::fs::remove_dir_all(&tmp).unwrap();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();
    assert!(!read_state(&app).is_dirty());
    assert!(
        read_log_on_disk(&app).unwrap().is_read("reunion", 7),
        "the retry wrote the landing knowledge"
    );
}

#[test]
fn a_jump_then_quit_confirms_continue_loads_the_pre_jump_autosave() {
    // Play the demo to the reunion crossing so an autosave exists.
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion", "epilogue"]));
    let root = pack.path().join("pack");
    let mut app = app_for_pack(root.clone());
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Digit1);
    press_and_update(&mut app, KeyCode::Space); // cross into reunion
    let autosave_before = std::fs::read(autosave_path(&app)).expect("a crossing autosaved");

    // Jump back to the entry scene; the selection writes nothing.
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(0);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "first_meeting"
    );
    assert_eq!(std::fs::read(autosave_path(&app)).unwrap(), autosave_before);

    // Quit, relaunch over the same pack and pick Continue: the pre-jump
    // autosave — not the Timeline landing — is what resumes.
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AppExit>>()
        .write(AppExit::Success);
    app.update();
    drop(app);

    let mut relaunched = boot_app(root, None);
    assert!(menu(&relaunched).continue_enabled, "the autosave loads");
    relaunched.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut relaunched, KeyCode::Enter);
    assert!(!menu(&relaunched).open, "Continue entered play");
    assert_eq!(
        relaunched.world().resource::<Runtime>().runner.scene_id(),
        "reunion",
        "Continue resumes the pre-jump autosave, not the Timeline landing"
    );
}

#[test]
fn opening_walking_and_backing_out_of_the_timeline_writes_nothing() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    let before = parked_position(&app);
    let log_before = std::fs::read(read_log_path(&app)).unwrap_or_default();
    assert!(!autosave_path(&app).exists());

    open_timeline(&mut app);
    press_and_update(&mut app, KeyCode::ArrowDown);
    press_and_update(&mut app, KeyCode::ArrowRight);
    click(&mut app, MenuAction::TimelineBack);
    esc(&mut app);
    assert!(!menu(&app).open);
    assert_eq!(parked_position(&app), before);
    assert_eq!(
        std::fs::read(read_log_path(&app)).unwrap_or_default(),
        log_before
    );
    assert!(!autosave_path(&app).exists());
}

#[test]
fn the_timeline_opens_at_a_choice_wait_and_freezes_the_request() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    let before = parked_position(&app);
    open_timeline(&mut app);
    let focus_before = menu(&app).focus;
    assert_eq!(
        parked_position(&app),
        before,
        "the choice park is untouched"
    );
    // A digit is inert in the Timeline view: neither the park nor the focus
    // moves.
    press_and_update(&mut app, KeyCode::Digit1);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    assert_eq!(menu(&app).focus, focus_before);
}

#[test]
fn timeline_cards_use_the_approved_geometry_and_identical_focus_ring() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    open_timeline(&mut app);
    build_pause_tree(&mut app);

    let cards: Vec<Entity> = app
        .world()
        .iter_entities()
        .filter_map(|entity| {
            matches!(
                entity.get::<MenuAction>(),
                Some(MenuAction::TimelineCard(_))
            )
            .then_some(entity.id())
        })
        .collect();
    assert_eq!(cards.len(), 3);
    for card in &cards {
        let node = app.world().get::<Node>(*card).expect("card node");
        assert_eq!(node.height, Val::Px(136.0));
        assert_eq!(node.padding, UiRect::all(Val::Px(18.0)));
        assert_eq!(node.border, UiRect::all(Val::Px(2.0)));
        // A raw scene id is one unbroken token: every label opts into the
        // character fallback so a long id wraps rather than truncating
        // (`docs/title-and-menu.md` § Timeline `### Layout`).
        assert_eq!(
            app.world().get::<TextLayout>(*card).map(|l| l.linebreak),
            Some(LineBreak::WordOrCharacter)
        );
    }

    // The focused card carries the reserved orange ring; an unfocused card has
    // none, and locked and visited share one fill.
    let focused_border = app
        .world()
        .get::<BorderColor>(cards[0])
        .expect("card border")
        .top;
    assert_eq!(focused_border, crate::presentation::style::FOCUS);
    let locked_border = app
        .world()
        .get::<BorderColor>(cards[1])
        .expect("card border")
        .top;
    assert_eq!(locked_border, Color::NONE, "an unfocused card has no ring");
    assert_eq!(
        app.world().get::<BackgroundColor>(cards[0]).unwrap().0,
        app.world().get::<BackgroundColor>(cards[1]).unwrap().0,
        "locked and visited share one fill"
    );

    // The grid is two equal columns with 16px gaps; the body scrolls.
    let grid = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::timeline::TimelineGrid>()
                .map(|_| entity.id())
        })
        .expect("the Timeline grid");
    let grid_node = app.world().get::<Node>(grid).unwrap();
    assert_eq!(grid_node.column_gap, Val::Px(16.0));
    assert_eq!(grid_node.row_gap, Val::Px(16.0));
    assert_eq!(
        grid_node.grid_template_columns,
        vec![RepeatedGridTrack::flex(2, 1.0)]
    );

    // The body lets the grid keep its natural height: a stretched or shrunk
    // grid hides its overflow, so the focused-card scroll can never move
    // (`docs/title-and-menu.md` § Timeline `### Layout`).
    let body = entity_with::<crate::presentation::timeline::TimelineBody>(&app);
    let body_node = app.world().get::<Node>(body).unwrap();
    assert_eq!(body_node.flex_direction, FlexDirection::Column);
    assert_eq!(body_node.align_items, AlignItems::Start);
    assert_eq!(body_node.overflow.y, bevy::ui::OverflowAxis::Scroll);
    assert_eq!(grid_node.flex_shrink, 0.0);
    let body = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::timeline::TimelineBody>()
                .map(|_| entity.id())
        })
        .expect("the Timeline body");
    let body_node = app.world().get::<Node>(body).unwrap();
    assert_eq!(body_node.overflow, Overflow::scroll_y());

    // Heading / explanation / legend hold the contract's type roles and sizes.
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    let font_size = |app: &App, want: &str| -> bevy::text::FontSize {
        app.world()
            .iter_entities()
            .find_map(|entity| {
                (entity.get::<Text>().map(|text| text.0.as_str()) == Some(want))
                    .then(|| entity.get::<TextFont>().map(|font| font.font_size))
                    .flatten()
            })
            .unwrap_or_else(|| panic!("no text node {want:?}"))
    };
    assert_eq!(
        font_size(&app, menu::TIMELINE_HEADING),
        faces.sans_semibold(24.0).font_size
    );
    assert_eq!(
        font_size(&app, menu::TIMELINE_EXPLANATION),
        faces.sans(16.0).font_size
    );
    assert_eq!(
        font_size(&app, menu::TIMELINE_LEGEND),
        faces.mono(12.0).font_size
    );
}

#[test]
fn timeline_and_gallery_doors_render_only_in_the_slot_grid() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion"]);
    esc(&mut app);
    build_pause_tree(&mut app);
    assert!(has_action(&app, MenuAction::OpenTimeline));
    assert!(has_action(&app, MenuAction::OpenGallery));

    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).view, MenuView::Settings);
    build_pause_tree(&mut app);
    assert!(!has_action(&app, MenuAction::OpenTimeline));
    assert!(!has_action(&app, MenuAction::OpenGallery));

    click(&mut app, MenuAction::BackToSlots);
    click(&mut app, MenuAction::OpenGallery);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    build_pause_tree(&mut app);
    assert!(!has_action(&app, MenuAction::OpenTimeline));
}

#[test]
fn a_jump_applies_the_targets_leading_staging_changes() {
    let pack = temp_custom_pack(
        &[
            ("a", "bg a\nplay music_a\nA: one.\n"),
            ("b", "bg b\nplay music_b\nA: two.\n"),
        ],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    let staging = &app.world().resource::<Runtime>().staging;
    assert_eq!(staging.background.as_deref(), Some("b"));
    assert_eq!(staging.bgm.as_deref(), Some("music_b"));
}

#[test]
fn a_jump_to_a_scene_that_ends_immediately_parks_at_the_end() {
    let pack = temp_custom_pack(
        &[("a", "bg a\nA: one.\n"), ("b", "stop music_a\n")],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    assert!(
        wait(&app).is_none(),
        "the target ended immediately; no invented wait"
    );
    assert!(!menu(&app).open, "the overlay closed on the landing");
}

#[test]
fn a_jump_begins_only_the_targets_fresh_decision_request() {
    let fake = FakeResolver::new(model_answer("warm"));
    let pack = temp_custom_pack(
        &[
            ("a", "A: entry.\n"),
            (
                "b",
                "decide ace.greet -> reply\nlabel reply.warm\nA: warm.\nlabel reply.cold\nA: cold.\nlabel reply.none\nA: none.\n",
            ),
        ],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["b"]);
    let mut app = app_with_decisions_at(root, Some(Arc::new(fake.clone())));
    assert_eq!(fake.calls(), 0, "no request before the jump");

    open_timeline(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(1);
    press_and_update(&mut app, KeyCode::Enter);
    press_and_update(&mut app, KeyCode::Enter); // accept

    pump_until(&mut app, |_| fake.calls() == 1);
    assert!(matches!(wait(&app), Some(Wait::Decision { .. })));
    fake.release();
    pump_until(&mut app, parked_on_say);
    let Some(Wait::Say { text, .. }) = wait(&app) else {
        panic!(
            "expected the target's say after its decision, got {:?}",
            wait(&app)
        );
    };
    assert_eq!(text, "warm.");
}

/// A copied demo pack with `count` generated scenes; the first id is long when
/// `long_first` is set. All ids are declared in order and the first is the
/// entry, so play visits it.
fn temp_pack_with_many_entries(count: usize, long_first: bool) -> tempfile::TempDir {
    const LONG: &str = "scene_with_a_deliberately_long_representative_identifier_00";
    let dir = temp_pack();
    let root = dir.path().join("pack");
    for scene in std::fs::read_dir(root.join("scenes")).unwrap() {
        std::fs::remove_file(scene.unwrap().path()).unwrap();
    }
    let mut ids = Vec::new();
    for index in 0..count {
        let id = if index == 0 && long_first {
            LONG.to_string()
        } else {
            format!("tl_scene_{index:02}")
        };
        std::fs::write(root.join(format!("scenes/{id}.unmei")), "A: line.\n").unwrap();
        ids.push(id);
    }
    std::fs::write(
        root.join("pack.ron"),
        format!(
            "(\n    name: \"timeline many\",\n    entry: \"{}\",\n    scenes: [{}],\n)\n",
            ids[0],
            ids.iter()
                .map(|id| format!("\"{id}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )
    .unwrap();
    let visited: Vec<&str> = ids.iter().map(String::as_str).collect();
    seed_visited(&root, &visited);
    dir
}

#[test]
fn a_thirty_entry_timeline_renders_every_card_with_long_ids_verbatim() {
    let pack = temp_pack_with_many_entries(30, true);
    let mut app = app_for_pack(pack.path().join("pack"));
    open_timeline(&mut app);
    let labels = timeline_labels(&mut app);
    assert_eq!(labels.len(), 30, "every declared scene renders once");
    assert_eq!(
        labels[0], "scene_with_a_deliberately_long_representative_identifier_00",
        "the long raw id is rendered verbatim"
    );
    assert_eq!(labels[29], "tl_scene_29");
    assert!(has_action(&app, MenuAction::TimelineCard(29)));
    assert!(
        !has_action(&app, MenuAction::TimelineCard(30)),
        "no card past the declared list"
    );

    // The walk clamps on the last card rather than wrapping.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TimelineCard(29);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(29));
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::TimelineCard(29));
}

#[test]
fn the_timeline_opens_at_the_end_of_the_scene_unchanged() {
    let (mut app, _pack) = timeline_app(&["first_meeting", "reunion", "epilogue"]);
    play_to_end(&mut app);
    assert!(wait(&app).is_none(), "the demo runs to its end");
    esc(&mut app);
    assert!(menu(&app).open, "the overlay opens at scene end");
    click(&mut app, MenuAction::OpenTimeline);
    assert_eq!(menu(&app).view, MenuView::Timeline);
    assert!(wait(&app).is_none(), "the end park is untouched");
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "epilogue"
    );
}
