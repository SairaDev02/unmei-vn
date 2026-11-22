use super::*;

fn reader_anchor(app: &App) -> Option<drawer::MonitorAnchor> {
    drawer_panel(app).anchor()
}

fn scroll_drag_active(app: &App) -> bool {
    app.world()
        .resource::<drawer::MonitorScrollbarDrag>()
        .active
}

fn reader_node(app: &mut App, part: drawer::MonitorReaderNode) -> Entity {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &drawer::MonitorReaderNode)>();
    query
        .iter(world)
        .find(|(_, node)| **node == part)
        .map(|(entity, _)| entity)
        .expect("the reader node exists")
}

fn log_body_entity(app: &mut App) -> Entity {
    let world = app.world_mut();
    let mut query = world.query_filtered::<Entity, With<drawer::MonitorLogBody>>();
    query.iter(world).next().expect("the log body exists")
}

fn scrollbar_track_entity(app: &mut App) -> Entity {
    let world = app.world_mut();
    let mut query = world.query_filtered::<Entity, With<drawer::MonitorScrollbarTrack>>();
    query
        .iter(world)
        .next()
        .expect("the scrollbar track exists")
}

/// Place a drawer node's laid-out rect in the headless world exactly as the
/// production `ComputedNode`/`UiGlobalTransform` would, so cursor hit-testing
/// runs the same arithmetic.
fn place_reader_rect(app: &mut App, entity: Entity, center: Vec2, size: Vec2) {
    app.world_mut().entity_mut(entity).insert((
        bevy::ui::ComputedNode { size, ..default() },
        bevy::ui::UiGlobalTransform::from_translation(center),
    ));
}

fn set_reader_cursor(app: &mut App, position: Vec2) {
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>();
    let mut window = query
        .single_mut(app.world_mut())
        .expect("the reader window");
    window.set_cursor_position(Some(position));
}

fn send_reader_wheel(app: &mut App, y: f32, unit: bevy::input::mouse::MouseScrollUnit) {
    app.world_mut()
        .write_message(bevy::input::mouse::MouseWheel {
            unit,
            x: 0.0,
            y,
            window: Entity::PLACEHOLDER,
            phase: bevy::input::touch::TouchPhase::Moved,
        });
}

/// Drag the reader's scrollbar to a track fraction (`0.0` = top of the track,
/// `1.0` = bottom), the production navigation that establishes a deliberate
/// reading position. The track is laid out exactly as the 1280×720 drawer
/// draws it (y 212–505); a fresh press on the thumb starts the drag, the
/// cursor's landing place is the position, and the release ends it.
fn drag_reader_to(app: &mut App, track_fraction: f32) {
    let track = scrollbar_track_entity(app);
    place_reader_rect(app, track, Vec2::new(1251.0, 358.5), Vec2::new(10.0, 293.0));
    let thumb = reader_node(app, drawer::MonitorReaderNode::Thumb);
    set_reader_cursor(app, Vec2::new(1251.0, 212.0 + track_fraction * 293.0));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .entity_mut(thumb)
        .insert(Interaction::Pressed);
    step_input(app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    step_input(app);
    app.world_mut().entity_mut(thumb).insert(Interaction::None);
    step_input(app);
}

/// A rendered row's imposed height, by admission sequence.
fn row_node_height(app: &mut App, sequence: u64) -> Option<Val> {
    let world = app.world_mut();
    let mut query = world.query::<(&MonitorRow, &Node)>();
    query
        .iter(world)
        .find(|(row, _)| row.0 == sequence)
        .map(|(_, node)| node.height)
}

#[test]
fn wheel_scrolls_only_when_hit_tested_to_the_log_body() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(
        drawer_panel(&app).is_following(),
        "an enabled launch follows latest"
    );

    // The log body's rect at 1280x720: x 24-1256, y 212-505.
    let body = log_body_entity(&mut app);
    place_reader_rect(
        &mut app,
        body,
        Vec2::new(640.0, 358.5),
        Vec2::new(1232.0, 293.0),
    );

    set_reader_cursor(&mut app, Vec2::new(40.0, 40.0));
    send_reader_wheel(&mut app, 1.0, bevy::input::mouse::MouseScrollUnit::Line);
    step_input(&mut app);
    assert!(
        drawer_panel(&app).is_following(),
        "outside-panel wheel never scrolls the log"
    );
    assert_eq!(drawer_window(&mut app).last().copied(), Some(99));

    // Hit-tested to the body, one notch moves three display lines older and
    // disables following; the anchor is a retained admission identity.
    set_reader_cursor(&mut app, Vec2::new(640.0, 358.5));
    send_reader_wheel(&mut app, 1.0, bevy::input::mouse::MouseScrollUnit::Line);
    step_input(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "scrolling away disables following"
    );
    let anchor = reader_anchor(&app).expect("the reader anchored");
    assert_eq!(anchor.sequence, 82, "the 15-line tail minus a 3-line notch");
    assert_eq!(
        drawer_window(&mut app).first().copied(),
        Some(82),
        "the anchor is the top visible event"
    );
    assert!(drawer_status(&mut app).contains("reading older lines"));
    assert!(!drawer_panel(&app).is_pending());

    // Arrivals never pull the older-lines reader to the tail.
    seed_numbered_monitor(&mut app, 0..105, "unmei::seq");
    settle_reader(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "new arrivals never enable follow"
    );
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(82),
        "the same event stays anchored"
    );
    assert_eq!(drawer_window(&mut app).first().copied(), Some(82));

    for _ in 0..3 {
        send_reader_wheel(&mut app, -1.0, bevy::input::mouse::MouseScrollUnit::Line);
        step_input(&mut app);
    }
    assert!(
        drawer_panel(&app).is_following(),
        "a deliberate bottom arrival resumes follow"
    );
    assert!(drawer_status(&mut app).contains("following latest"));
    assert_eq!(drawer_window(&mut app).last().copied(), Some(104));
}

#[test]
fn page_home_end_and_conflicting_navigation_follow_the_contract() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..200, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    // Focus is irrelevant: paging always addresses the log.
    tap_key(&mut app, KeyCode::Tab);
    assert_eq!(drawer_focus(&app), MonitorControl::FollowLatest);

    press_key(&mut app, KeyCode::PageUp);
    step_input(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(170),
        "200 events minus a 15-line bottom viewport, one page older"
    );
    assert!(!drawer_panel(&app).is_following());

    advance_time(&mut app, 0.05);
    step_input(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(170),
        "no repeat before the paging cadence"
    );
    advance_time(&mut app, 0.12);
    step_input(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(155),
        "the held key repeats"
    );
    // A repeat moves the reader like any other change: the drawn window starts
    // at the reader's own anchor, so a slow-frame repeat shows its own page, never
    // an older one (the stale-drawing class once mistaken for a drifting anchor).
    assert_eq!(
        drawer_window(&mut app).first().copied(),
        reader_anchor(&app).map(|anchor| anchor.sequence),
        "the drawn top row is the repeat's own anchor"
    );
    release_key(&mut app, KeyCode::PageUp);
    step_input(&mut app);

    // PgDn moves newer; reaching the bottom resumes following.
    tap_key(&mut app, KeyCode::PageDown);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(170));
    assert!(!drawer_panel(&app).is_following());

    tap_key(&mut app, KeyCode::Home);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(0));
    assert_eq!(drawer_window(&mut app).first().copied(), Some(0));
    assert!(!drawer_panel(&app).is_following());
    tap_key(&mut app, KeyCode::End);
    assert!(drawer_panel(&app).is_following());

    // A repeat is not a fresh press: End's fresh press runs once, and holding
    // it while Home reads older never re-enables follow.
    press_key(&mut app, KeyCode::End);
    step_input(&mut app);
    assert!(drawer_panel(&app).is_following());
    press_key(&mut app, KeyCode::Home);
    step_input(&mut app);
    assert!(!drawer_panel(&app).is_following());
    send_key_message(
        &mut app,
        KeyCode::End,
        bevy::input::ButtonState::Pressed,
        true,
    );
    step_input(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "an OS repeat is not a fresh End press"
    );
    release_key(&mut app, KeyCode::End);
    release_key(&mut app, KeyCode::Home);
    step_input(&mut app);

    let anchor = reader_anchor(&app);
    press_key(&mut app, KeyCode::PageUp);
    press_key(&mut app, KeyCode::Home);
    step_input(&mut app);
    assert_eq!(
        reader_anchor(&app),
        anchor,
        "PgUp+Home conflicted and neither applied"
    );
    release_key(&mut app, KeyCode::PageUp);
    release_key(&mut app, KeyCode::Home);
    step_input(&mut app);

    // Esc beats scroll for the frame: a closing frame never scrolls away.
    tap_key(&mut app, KeyCode::End);
    assert!(drawer_panel(&app).is_following());
    press_key(&mut app, KeyCode::Escape);
    press_key(&mut app, KeyCode::PageUp);
    step_input(&mut app);
    assert!(!drawer_open(&app), "Esc closed Monitor");
    release_key(&mut app, KeyCode::Escape);
    release_key(&mut app, KeyCode::PageUp);
    step_input(&mut app);
    tap_key(&mut app, KeyCode::F3);
    assert!(
        drawer_panel(&app).is_following(),
        "the closing frame's PgUp never scrolled"
    );
}

#[test]
fn scrollbar_drag_positions_the_reader_and_cancels_safely() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    // The track's rect at 1280x720: x 1246-1256, y 212-505.
    let track = scrollbar_track_entity(&mut app);
    place_reader_rect(
        &mut app,
        track,
        Vec2::new(1251.0, 358.5),
        Vec2::new(10.0, 293.0),
    );
    let thumb = reader_node(&mut app, drawer::MonitorReaderNode::Thumb);

    // A fresh press on the thumb starts a drag; the thumb centre follows the
    // cursor to the track's middle: event 42 with half a line read into it.
    set_reader_cursor(&mut app, Vec2::new(1251.0, 358.5));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .entity_mut(thumb)
        .insert(Interaction::Pressed);
    step_input(&mut app);
    let anchor = reader_anchor(&app).expect("the drag anchored the reader");
    assert_eq!(anchor.sequence, 42);
    assert!(
        (anchor.offset_lines - 0.5).abs() < 0.01,
        "the within-row offset is half the anchor row: {}",
        anchor.offset_lines
    );
    assert!(!drawer_panel(&app).is_following());
    assert!(scroll_drag_active(&app));

    // Dragging to the bottom of the travel is a deliberate bottom arrival.
    set_reader_cursor(&mut app, Vec2::new(1251.0, 505.0));
    step_input(&mut app);
    assert!(
        drawer_panel(&app).is_following(),
        "the bottom of the track resumes following"
    );
    assert!(scroll_drag_active(&app), "the button is still held");

    // The release ends the drag; reader state is preserved.
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    step_input(&mut app);
    assert!(!scroll_drag_active(&app));
    app.world_mut().entity_mut(thumb).insert(Interaction::None);
    step_input(&mut app);

    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .entity_mut(thumb)
        .insert(Interaction::Pressed);
    step_input(&mut app);
    assert!(scroll_drag_active(&app));
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    assert!(
        !scroll_drag_active(&app),
        "close cancels the scrollbar drag"
    );
    tap_key(&mut app, KeyCode::F3);
    assert!(drawer_open(&app));
    assert!(
        !scroll_drag_active(&app),
        "a held pointer cannot resume the drag on reopen"
    );
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    step_input(&mut app);

    tap_key(&mut app, KeyCode::Home);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(0));
    let thumb = reader_node(&mut app, drawer::MonitorReaderNode::Thumb);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .entity_mut(thumb)
        .insert(Interaction::Pressed);
    step_input(&mut app);
    assert!(scroll_drag_active(&app));
    {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.focused = false;
    }
    step_input(&mut app);
    assert!(
        !scroll_drag_active(&app),
        "focus loss cancels the scrollbar drag"
    );
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(0),
        "focus loss preserves the reader position"
    );
    {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.focused = true;
    }
    step_input(&mut app);
    assert!(!scroll_drag_active(&app), "regain does not resume the drag");
}

/// Home chooses the oldest **currently displayed** matching event during an
/// incomplete refresh and keeps that event as older matches are prepended; it
/// does not silently retarget to the whole retained history.
#[test]
fn home_keeps_the_oldest_currently_displayed_event_through_the_refresh() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..300, "unmei::seq");

    // The opening frame examines exactly the newest `PROJECTION_SCAN_BUDGET`
    // entries, so the oldest displayed event is derived from the seeded history
    // and the documented budget, not read back from the projection.
    press_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    let oldest_displayed = 300 - crate::drawer::PROJECTION_SCAN_BUDGET as u64;
    assert_eq!(
        oldest_displayed, 172,
        "the opening pass examined 128 newest"
    );
    assert!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating(),
        "the refresh is still incomplete"
    );

    tap_key(&mut app, KeyCode::Home);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(oldest_displayed),
        "Home selected the oldest currently displayed event"
    );

    // As the pass prepends older matches the selected event is preserved, not
    // retargeted to the newly arrived oldest retained event.
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(oldest_displayed),
        "the selected event survives older matches being prepended"
    );
    assert_eq!(
        drawer_window(&mut app).first().copied(),
        Some(oldest_displayed)
    );
    assert!(!drawer_panel(&app).is_following());
}

#[test]
fn a_reading_anchor_survives_hide_reopen_and_clamps_to_its_rewrap() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    let mut lines: Vec<MonitorLine> = (0..100)
        .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, &format!("event {i}")))
        .collect();
    // Event 42 is a long wrapped event, so it can be read part-way down.
    lines[42].text = format!("0000000042ms INFO  unmei::seq {}", "x".repeat(400));
    lines[42].body_start = lines[42].text.len() - 400;
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    // A production scrollbar drag into the wrapped event establishes a
    // fractional reading position: the mid travel over 100 single-line rows
    // lands on event 42, and the four-line row admits an offset past one line.
    drag_reader_to(&mut app, 0.5);
    let anchor = reader_anchor(&app).expect("the drag anchored the reader");
    assert_eq!(anchor.sequence, 42);
    assert!(
        anchor.offset_lines > 1.0,
        "the within-row offset is fractional: {}",
        anchor.offset_lines
    );
    assert_eq!(drawer_window(&mut app).first().copied(), Some(42));

    let offset = anchor.offset_lines;
    tap_key(&mut app, KeyCode::F3);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    let anchor = reader_anchor(&app).expect("the anchor survived the reopen");
    assert_eq!(anchor.sequence, 42);
    assert!((anchor.offset_lines - offset).abs() < 0.01);
    assert_eq!(drawer_window(&mut app).first().copied(), Some(42));

    // A fullscreen-like resize rewraps the same event to one line, clamping
    // the offset inside its new extent instead of scrolling past it.
    {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.resolution = bevy::window::WindowResolution::new(4000, 720);
    }
    settle_reader(&mut app);
    let anchor = reader_anchor(&app).expect("the anchor survived the rewrap");
    assert_eq!(anchor.sequence, 42, "the event is kept across a rewrap");
    assert!(
        anchor.offset_lines < 1.0,
        "the offset clamped into the one-line row: {}",
        anchor.offset_lines
    );
    assert_eq!(
        drawer_window(&mut app).first().copied(),
        Some(42),
        "the drawn top event is the clamped anchor"
    );

    // When the whole remaining tail fits the viewport, the offset drops to
    // zero rather than shifting the short content up.
    seed_numbered_monitor(&mut app, 42..45, "unmei::seq");
    settle_reader(&mut app);
    let anchor = reader_anchor(&app).expect("the anchor survived");
    assert_eq!(anchor.sequence, 42);
    assert_eq!(anchor.offset_lines, 0.0, "a fitting tail clamps to zero");
    assert_eq!(drawer_window(&mut app).first().copied(), Some(42));
}

#[test]
fn a_surviving_rows_imposed_height_follows_a_rewrap() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    // Event 5 is four lines at the 1280px column and one line at 4000px.
    let mut lines: Vec<MonitorLine> = (0..20)
        .map(|i| test_monitor_line(i, "unmei::seq", log::Level::Info, "short"))
        .collect();
    lines[5].text = format!("0000000005ms INFO  unmei::seq {}", "x".repeat(400));
    lines[5].body_start = lines[5].text.len() - 400;
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    let before = row_node_height(&mut app, 5).expect("event 5 is rendered");
    {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.resolution = bevy::window::WindowResolution::new(4000, 720);
    }
    settle_reader(&mut app);
    let after = row_node_height(&mut app, 5).expect("event 5 survived the rewrap");

    // The surviving row keeps its text but its imposed height is recomputed for
    // the wider column, so the shift/spacer/clamp cannot disagree with the row.
    assert!(
        matches!(after, Val::Px(value) if value < 20.0),
        "the rewrapped row collapsed to one line: {after:?}"
    );
    assert!(
        matches!(before, Val::Px(value) if value > 60.0),
        "the row was four lines before the rewrap: {before:?}"
    );
}

#[test]
fn an_evicted_anchor_clamps_to_the_oldest_match_and_acknowledges_expiry() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    tap_key(&mut app, KeyCode::PageUp);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(70));

    // Retention evicts the anchored event.
    seed_numbered_monitor(&mut app, 80..180, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired(), "eviction is acknowledged");
    assert!(
        !drawer_panel(&app).is_following(),
        "expiry keeps following off"
    );
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(80),
        "the anchor clamps to the oldest surviving matching event"
    );
    assert_eq!(drawer_window(&mut app).first().copied(), Some(80));
    assert!(drawer_status(&mut app).contains("older lines expired"));

    // The acknowledgement is panel-local, never a row, tracing event or Notice.
    assert!(
        !drawer_rows(&mut app)
            .iter()
            .any(|(_, _, text)| text.contains("expired")),
        "the expiry status is not a retained row"
    );
    assert!(app.world().resource::<Notice>().text.is_none());

    tap_key(&mut app, KeyCode::F3);
    tap_key(&mut app, KeyCode::F3);
    assert!(
        drawer_panel(&app).is_expired(),
        "hide/reopen preserves the acknowledgement"
    );
    seed_numbered_monitor(&mut app, 200..300, "unmei::seq");
    settle_reader(&mut app);
    assert!(
        drawer_panel(&app).is_expired(),
        "a repeated eviction keeps the acknowledgement set"
    );

    tap_key(&mut app, KeyCode::PageUp);
    assert!(
        !drawer_panel(&app).is_expired(),
        "deliberate navigation acknowledges the loss"
    );

    // A filter change clears it too.
    seed_numbered_monitor(&mut app, 400..500, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired());
    activate_control(&mut app, MonitorControl::ShowEverything);
    assert!(!drawer_panel(&app).is_expired());

    seed_numbered_monitor(&mut app, 500..600, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired());
    tap_key(&mut app, KeyCode::End);
    assert!(drawer_panel(&app).is_following());
    assert!(
        !drawer_panel(&app).is_expired(),
        "End acknowledges the loss"
    );

    // And Follow latest resumes following and clears it.
    tap_key(&mut app, KeyCode::Home);
    seed_numbered_monitor(&mut app, 700..800, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired());
    assert!(!drawer_panel(&app).is_following());
    activate_control(&mut app, MonitorControl::FollowLatest);
    assert!(drawer_panel(&app).is_following());
    assert!(!drawer_panel(&app).is_expired());
}

#[test]
fn filter_changes_keep_or_fall_back_to_matching_anchors() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    // Even sequences below 84 are the default view's matches; odd sequences
    // and everything at or above 84 are noise, so a non-matching anchor above
    // the last match exercises the nearest-earlier fallback.
    let lines: Vec<MonitorLine> = (0..100u64)
        .map(|i| {
            if i % 2 == 0 && i < 84 {
                test_monitor_line(i, "unmei::kept", log::Level::Info, &format!("kept {i}"))
            } else {
                test_monitor_line(i, "dep::noise", log::Level::Info, &format!("noise {i}"))
            }
        })
        .collect();
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    tap_key(&mut app, KeyCode::PageUp);
    let anchored = reader_anchor(&app).expect("anchored").sequence;
    assert_eq!(anchored % 2, 0, "the anchor is a matching event");

    // A matching anchor survives the filter change, and an old-predicate row
    // never leaks into the new view.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(anchored),
        "a matching anchor is kept across the filter change"
    );
    assert_eq!(drawer_window(&mut app).first().copied(), Some(anchored));
    assert!(
        !drawer_panel(&app).is_following(),
        "a filter change preserves follow intent"
    );
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(anchored)
    );
    assert!(
        drawer_rows(&mut app)
            .iter()
            .filter(|(_, part, _)| *part == MonitorRowPart::Target)
            .all(|(_, _, target)| target == "unmei::kept"),
        "only current-predicate rows are displayed"
    );

    // A non-matching anchor falls back to the first surviving matching newer
    // event. The drag lands on odd event 65: 100 rows give the mid-lower track
    // a position just inside 65 across the 85-line bottom range.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    drag_reader_to(&mut app, 0.726);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(65));
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(66),
        "the first surviving matching newer event"
    );
    assert!(!drawer_panel(&app).is_following());

    // ...else the nearest preceding match. Event 83 is past the last match at
    // 82, so no newer match survives.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    drag_reader_to(&mut app, 0.91);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(83));
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(82),
        "the nearest preceding match when no newer one survives"
    );
    assert!(!drawer_panel(&app).is_following());
}

#[test]
fn an_empty_filter_view_stays_empty_and_later_matches_never_enable_follow() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_monitor_history(
        &mut app,
        (0..100)
            .map(|i| test_monitor_line(i, "dep::noise", log::Level::Info, &format!("noise {i}")))
            .collect(),
    );
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    // Anchor a dependency event under Show everything, then switch the display
    // predicate to `unmei::` only: there is no match at all. The drag's mid
    // travel over the 100 rows lands on event 42.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    drag_reader_to(&mut app, 0.5);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(42));
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(reader_anchor(&app), None, "no matches, no anchor");
    assert!(!drawer_panel(&app).is_following());
    assert!(
        has_component::<drawer::MonitorEmptyState>(&mut app),
        "the empty view is shown, not a filter reset"
    );

    // A later matching arrival anchors at the oldest new match and never
    // silently enables follow.
    seed_monitor_history(
        &mut app,
        vec![test_monitor_line(
            100,
            "unmei::kept",
            log::Level::Info,
            "later",
        )],
    );
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(100),
        "the first later match anchors the empty view"
    );
    assert!(
        !drawer_panel(&app).is_following(),
        "a later arrival never silently enables follow"
    );
    assert_eq!(drawer_window(&mut app).first().copied(), Some(100));

    // Further arrivals accumulate below the anchored event, not at the tail.
    seed_monitor_history(
        &mut app,
        (100..105)
            .map(|i| test_monitor_line(i, "unmei::kept", log::Level::Info, "later"))
            .collect(),
    );
    settle_reader(&mut app);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(100));
    assert!(!drawer_panel(&app).is_following());
    assert!(drawer_status(&mut app).contains("reading older lines"));
}

/// A retained anchor that the restarted pass has not reached yet must not be
/// presented as a "no matches" empty view: the body stays blank under the
/// truthful `updating view` until the anchor is located.
#[test]
fn a_filter_refresh_never_shows_a_false_empty_state_while_locating_the_anchor() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    let mut lines: Vec<MonitorLine> = (0..100)
        .map(|i| test_monitor_line(i, "unmei::kept", log::Level::Info, &format!("kept {i}")))
        .collect();
    lines.extend(
        (100..228)
            .map(|i| test_monitor_line(i, "dep::noise", log::Level::Info, &format!("noise {i}"))),
    );
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    tap_key(&mut app, KeyCode::PageUp);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(70));

    // Show everything keeps the matching anchor.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(70));

    // Back to `unmei::` only: the restart's first pass examines the newest 128
    // entries — all dependency noise — so there are zero current-predicate rows
    // while the retained anchor (70) is still unscanned.
    let control = drawer_control_entity(&mut app, MonitorControl::ShowEverything);
    app.world_mut()
        .entity_mut(control)
        .insert(Interaction::Pressed);
    step_input(&mut app);
    assert!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating(),
        "the restarted pass is still running"
    );
    assert!(
        drawer_panel(&app).is_pending(),
        "the retained anchor is still being located"
    );
    assert!(
        !has_component::<drawer::MonitorEmptyState>(&mut app),
        "no false empty state while the anchor is unscanned"
    );
    assert!(drawer_status(&mut app).contains("updating view"));

    app.world_mut()
        .entity_mut(control)
        .insert(Interaction::None);
    step_input(&mut app);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(70),
        "the anchor survived the refresh"
    );
    assert!(!drawer_panel(&app).is_pending());
}

/// A restarted pass that already holds positive current-predicate rows must not
/// present the matching tail as the older non-following reader's location. The
/// retained anchor is older than everything the first pass examined, so the
/// body stays on the locating placeholder until the anchor is reached, then
/// jumps straight to it — never to the tail and back.
#[test]
fn a_pending_refresh_never_presents_the_matching_tail_as_the_olders_location() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    // Even sequences match the default view; odd sequences are dependency noise.
    let lines: Vec<MonitorLine> = (0..1000u64)
        .map(|i| {
            if i % 2 == 0 {
                test_monitor_line(i, "unmei::kept", log::Level::Info, &format!("kept {i}"))
            } else {
                test_monitor_line(i, "dep::noise", log::Level::Info, &format!("noise {i}"))
            }
        })
        .collect();
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    // The drag's travel over the 500 matching rows is 485 lines deep, so a
    // track fraction of 0.2155 lands just inside the 100th match — retained
    // event 200.
    drag_reader_to(&mut app, 0.2155);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(200));

    // Toggle the view to restart the pass: its first window is the newest 128
    // retained entries (872..999), which hold positive `unmei::` matches, while
    // the retained anchor 200 is still unscanned.
    activate_control(&mut app, MonitorControl::ShowEverything);
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert!(projection.is_updating(), "the restarted pass is incomplete");
    assert!(
        projection.shown() > 0,
        "the incomplete first pass already holds positive current-predicate rows"
    );
    assert!(
        drawer_panel(&app).is_pending(),
        "the retained anchor is still being located"
    );
    assert!(
        drawer_window(&mut app).is_empty(),
        "the matching tail is not shown as the older reader's location"
    );
    assert!(
        has_component::<drawer::MonitorLocatingState>(&mut app),
        "the unresolved position is truthfully represented"
    );
    assert!(drawer_status(&mut app).contains("updating view"));
    assert!(drawer_status(&mut app).contains("reading older lines"));

    settle_reader(&mut app);
    assert!(!drawer_panel(&app).is_pending());
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(200));
    assert_eq!(drawer_window(&mut app).first().copied(), Some(200));
}

#[test]
fn automatic_clamps_never_enable_follow() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    // Rows below 70 are `unmei::` matches; everything above is noise, so a
    // noise anchor sits past the last match.
    let lines: Vec<MonitorLine> = (0..100u64)
        .map(|i| {
            if i < 70 {
                test_monitor_line(i, "unmei::kept", log::Level::Info, "kept")
            } else {
                test_monitor_line(i, "dep::noise", log::Level::Info, "noise")
            }
        })
        .collect();
    seed_monitor_history(&mut app, lines);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    // Anchor a noise event past every match under Show everything (the drag
    // lands near event 80), then switch back: the nearest preceding match is
    // the projection's newest row — the automatic fallback must not enable
    // follow.
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    drag_reader_to(&mut app, 0.88);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(80));
    activate_control(&mut app, MonitorControl::ShowEverything);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(69),
        "the nearest preceding match"
    );
    assert!(
        !drawer_panel(&app).is_following(),
        "an automatic bottom fallback never enables follow"
    );

    // The same holds for an eviction clamp to a single surviving match at the
    // tail of the projection.
    let tail: Vec<MonitorLine> = (100..120u64)
        .map(|i| {
            if i == 119 {
                test_monitor_line(i, "unmei::kept", log::Level::Info, "kept")
            } else {
                test_monitor_line(i, "dep::noise", log::Level::Info, "noise")
            }
        })
        .collect();
    seed_monitor_history(&mut app, tail);
    settle_reader(&mut app);
    assert_eq!(
        reader_anchor(&app).map(|anchor| anchor.sequence),
        Some(119),
        "the only surviving matching event"
    );
    assert!(
        !drawer_panel(&app).is_following(),
        "the eviction clamp never enables follow"
    );
    assert!(drawer_panel(&app).is_expired());
}

#[test]
fn reader_scrolling_and_filter_refresh_stay_within_the_settled_budgets() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..1000, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    for _ in 0..20 {
        step_input(&mut app);
        let work = app.world().resource::<MonitorWork>();
        assert!(work.scanned <= PROJECTION_SCAN_BUDGET);
        assert!(work.text_ops <= TEXT_WORK_BUDGET);
        if !app
            .world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating()
        {
            break;
        }
    }
    assert!(!app
        .world()
        .resource::<crate::drawer::MonitorProjection>()
        .is_updating());

    // Paging a deep history never scans or materializes beyond the budget.
    for _ in 0..4 {
        tap_key(&mut app, KeyCode::PageUp);
        let work = app.world().resource::<MonitorWork>();
        assert!(work.scanned <= PROJECTION_SCAN_BUDGET);
        assert!(work.text_ops <= TEXT_WORK_BUDGET);
    }
    // A settled reading frame does no metadata scan and no text work.
    step_input(&mut app);
    assert_eq!(app.world().resource::<MonitorWork>().scanned, 0);
    assert_eq!(app.world().resource::<MonitorWork>().text_ops, 0);

    // A filter refresh cancels obsolete work, stays inside the budgets and
    // shows only current-predicate rows at every point.
    activate_control(&mut app, MonitorControl::ShowEverything);
    for _ in 0..20 {
        step_input(&mut app);
        let work = app.world().resource::<MonitorWork>();
        assert!(work.scanned <= PROJECTION_SCAN_BUDGET);
        assert!(work.text_ops <= TEXT_WORK_BUDGET);
        if !app
            .world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating()
        {
            break;
        }
    }
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .shown(),
        1000
    );
    assert_eq!(
        app.world().resource::<MonitorHistory>().retained(),
        1000,
        "the reader never retains a second history"
    );
}

#[test]
fn reader_position_and_expiry_survive_session_transitions() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    tap_key(&mut app, KeyCode::PageUp);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(70));
    seed_numbered_monitor(&mut app, 80..180, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired());
    let anchor = reader_anchor(&app);

    tap_key(&mut app, KeyCode::F3);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(!drawer_panel(&app).is_following());
    assert_eq!(
        reader_anchor(&app),
        anchor,
        "the anchor survives hide/reopen"
    );
    assert!(drawer_panel(&app).is_expired());

    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    tap_key(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open);
    tap_key(&mut app, KeyCode::F5);
    tap_key(&mut app, KeyCode::F9);
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "a load does not reset the reader choice"
    );
    assert_eq!(reader_anchor(&app), anchor, "the anchor survives a load");
    assert!(
        drawer_panel(&app).is_expired(),
        "the expiry acknowledgement survives a load"
    );

    // A real player-facing New Game transition (confirm once) preserves the
    // reader state while performing its own session reset.
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    click_input(&mut app, MenuAction::NewGame);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "accepting New Game starts play");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "a New Game does not reset the reader choice"
    );
    assert_eq!(
        reader_anchor(&app),
        anchor,
        "the anchor survives a New Game"
    );
    assert!(
        drawer_panel(&app).is_expired(),
        "the expiry acknowledgement survives a New Game"
    );
}

/// A real Timeline jump is a position replacement; the reader's anchor, follow
/// choice and expiry acknowledgement survive it while the underlying session
/// performs its own reset. This needs a pack with declared timeline scenes, so
/// it runs on a synthetic one rather than the auto-discovered demo pack.
#[test]
fn reader_position_and_expiry_survive_a_timeline_jump() {
    let pack = temp_custom_pack(
        &[("a", "A: entry.\n"), ("b", "B: b.\n")],
        Some(&["a", "b"]),
        "a",
    );
    let root = pack.path().join("pack");
    seed_visited(&root, &["a"]);
    let mut app = boot_reader_app(root);
    enter_play_with_input(&mut app);
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    tap_key(&mut app, KeyCode::PageUp);
    assert_eq!(reader_anchor(&app).map(|anchor| anchor.sequence), Some(70));
    seed_numbered_monitor(&mut app, 80..180, "unmei::seq");
    settle_reader(&mut app);
    assert!(drawer_panel(&app).is_expired());
    let anchor = reader_anchor(&app);

    // The jump goes through the real overlay, Timeline view, card press and
    // confirmation while the drawer is closed.
    tap_key(&mut app, KeyCode::F3);
    assert!(!drawer_open(&app));
    tap_key(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    click_input(&mut app, MenuAction::OpenTimeline);
    assert_eq!(menu(&app).view, MenuView::Timeline);
    click_input(&mut app, MenuAction::TimelineCard(0));
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::TimelineJump(0)),
        "the visited entry scene asks to confirm the jump"
    );
    click_input(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "accepting jumps to the timeline scene");

    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);
    assert!(
        !drawer_panel(&app).is_following(),
        "a Timeline jump does not reset the reader choice"
    );
    assert_eq!(
        reader_anchor(&app),
        anchor,
        "the anchor survives a Timeline jump"
    );
    assert!(
        drawer_panel(&app).is_expired(),
        "the expiry acknowledgement survives a Timeline jump"
    );
}

/// Regression for the projection's arrivals band: once a batch of arrivals has
/// established the new block, the next batch above it must still be examined.
/// The following view is the observable outcome — the newest matching arrival
/// is emitted, not silently skipped.
#[test]
fn later_arrivals_above_an_established_band_are_projected() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    seed_numbered_monitor(&mut app, 0..105, "unmei::seq");
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .sequences()
            .last()
            .copied(),
        Some(104),
        "the first arrival batch is projected"
    );
    let _ = drawer_window(&mut app);

    // A second batch above the established band is new work, not something an
    // earlier pass already reviewed.
    seed_numbered_monitor(&mut app, 0..110, "unmei::seq");
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .sequences()
            .last()
            .copied(),
        Some(109),
        "arrivals above the established band are examined"
    );
    assert_eq!(drawer_window(&mut app).last().copied(), Some(109));
    assert!(drawer_panel(&app).is_following());
}

/// A third batch, arriving above the band a *previous* pass established, is
/// examined whole. The pass must resume the arrivals band from the last
/// examined entry rather than restart at the newest arrival: treating the
/// newest as reviewed would otherwise mark every older, still-unexamined
/// arrival in the same batch as already reviewed and drop it from the
/// projection forever. The rendered acceptance walk caught the
/// drawer advancing one row per update while the rest of every batch was
/// silently lost.
#[test]
fn a_third_arrival_batch_above_an_established_band_is_projected() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..100, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    settle_reader(&mut app);

    // Establish the arrivals band above the old block.
    seed_numbered_monitor(&mut app, 0..105, "unmei::seq");
    settle_reader(&mut app);
    assert_eq!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .sequences()
            .last()
            .copied(),
        Some(104),
        "the first arrival batch established the band"
    );

    // A whole second arrival batch above that band, in one pass.
    seed_numbered_monitor(&mut app, 0..130, "unmei::seq");
    settle_reader(&mut app);
    let projection = app.world().resource::<crate::drawer::MonitorProjection>();
    assert_eq!(projection.shown(), 130, "every arrival is projected");
    assert_eq!(
        projection.sequences().last().copied(),
        Some(129),
        "the newest arrival is projected"
    );
    assert_eq!(
        drawer_window(&mut app).last().copied(),
        Some(129),
        "the newest arrival is rendered"
    );
}

/// Successive 128-event arrival batches under an incomplete refresh leave no
/// unexamined gap: the projection extends its examined band contiguously from
/// the entry it last stopped on, so the settled projection equals the
/// independently selected current matching retained identities.
/// The fixture slides the retained window as the batches arrive, so the pass
/// also prunes entries the window has moved past.
#[test]
fn arrival_batches_under_an_incomplete_refresh_settle_complete() {
    let pack = temp_pack();
    let mut app = boot_reader_app(pack.path().join("pack"));
    seed_numbered_monitor(&mut app, 0..1000, "unmei::seq");
    tap_key(&mut app, KeyCode::F3);
    step_input(&mut app);
    assert!(
        app.world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating(),
        "the opening pass left the older block incomplete"
    );

    for end in [1128u64, 1256, 1384] {
        seed_numbered_monitor(&mut app, (end - 1000)..end, "unmei::seq");
        step_input(&mut app);
    }
    settle_reader(&mut app);

    // Independently select the current matching retained identities; do not
    // mirror the projection's own interval arithmetic.
    let expected: Vec<u64> = app
        .world()
        .resource::<MonitorHistory>()
        .lines()
        .iter()
        .filter(|line| line.target.starts_with("unmei::"))
        .map(|line| line.sequence)
        .collect();
    let projected = app
        .world()
        .resource::<crate::drawer::MonitorProjection>()
        .sequences();
    assert_eq!(
        projected, expected,
        "every retained matching identity is projected"
    );
    assert_eq!(projected, (384..1384).collect::<Vec<_>>());

    step_input(&mut app);
    let work = app.world().resource::<MonitorWork>();
    assert_eq!(work.scanned, 0, "settled frames scan nothing");
    assert_eq!(work.text_ops, 0, "settled frames materialize nothing");
}
