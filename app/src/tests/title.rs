use super::*;

/// The built title's one radial gradient — the neutral underlay §2 draws under
/// the backdrop art and the veil — so its resolved centre can be pinned without
/// re-reading the whole entity tree.
fn title_gradient_radial(app: &App) -> RadialGradient {
    let root = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            let gradient = entity.get::<BackgroundGradient>()?;
            gradient
                .0
                .iter()
                .any(|gradient| matches!(gradient, bevy::ui::Gradient::Radial(_)))
                .then_some(entity.id())
        })
        .expect("the title draws the neutral radial gradient");
    single_radial(app, root)
}

/// The built title's left column: Default look §3 places it 104px in, 150px
/// down and 420px wide.
fn title_column(app: &App) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            let node = entity.get::<Node>()?;
            (node.left == Val::Px(104.0)
                && node.top == Val::Px(150.0)
                && node.width == Val::Px(420.0))
            .then_some(entity.id())
        })
        .expect("the title's left column")
}

/// The node that draws §3's veil: the one linear gradient whose stops are the
/// veil's three.
fn veil_node(app: &App) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            let gradient = entity.get::<BackgroundGradient>()?;
            gradient
                .0
                .iter()
                .any(|gradient| match gradient {
                    bevy::ui::Gradient::Linear(linear) => {
                        linear.stops.len() == 3 && linear.stops[2].color.to_srgba().alpha == 0.0
                    }
                    _ => false,
                })
                .then_some(entity.id())
        })
        .expect("the title draws the veil")
}

/// The item of the list that holds the control `action` carries.
fn row_item(app: &App, action: MenuAction) -> Entity {
    let control = app
        .world()
        .iter_entities()
        .find_map(|entity| (entity.get::<MenuAction>() == Some(&action)).then_some(entity.id()))
        .expect("the row's control");
    parent_of(app, control)
}

/// The item's focus bar: the 12x2 box beside its label.
fn focus_bar(app: &App, item: Entity) -> Entity {
    children_of(app, item)
        .into_iter()
        .find(|child| {
            app.world()
                .get::<Node>(*child)
                .is_some_and(|node| node.width == Val::Px(12.0) && node.height == Val::Px(2.0))
        })
        .expect("the row's focus bar")
}

/// The item's label text: the one child without a control whose box holds a
/// text node.
fn row_label(app: &App, item: Entity) -> Entity {
    children_of(app, item)
        .into_iter()
        .filter(|child| app.world().entity(*child).get::<MenuAction>().is_none())
        .find_map(|child| children_of(app, child).into_iter().next())
        .expect("the row's label")
}

fn children_of(app: &App, entity: Entity) -> Vec<Entity> {
    app.world()
        .entity(entity)
        .get::<Children>()
        .map(|children| children.iter().collect())
        .unwrap_or_default()
}

fn parent_of(app: &App, entity: Entity) -> Entity {
    app.world()
        .entity(entity)
        .get::<ChildOf>()
        .map(ChildOf::parent)
        .expect("the entity has a parent")
}

fn alpha_of(app: &App, entity: Entity) -> f32 {
    app.world()
        .get::<BackgroundColor>(entity)
        .map(|fill| fill.0.to_srgba().alpha)
        .unwrap_or(0.0)
}

fn tone_of(app: &App, entity: Entity) -> Color {
    app.world()
        .get::<TextColor>(entity)
        .map(|tone| tone.0)
        .expect("the text draws a tone")
}

/// The reference look's tones, as the tokens declare them.
const TEXT: Color = Color::srgba(0.90, 0.90, 0.92, 1.0);
const DIM: Color = Color::srgba(0.80, 0.80, 0.85, 0.62);
const FAINT: Color = Color::srgba(0.80, 0.80, 0.85, 0.35);

/// The tone of Continue's reason line in the built title.
fn title_reason_color(app: &App) -> Color {
    let line = menu(app).continue_line.clone();
    app.world()
        .iter_entities()
        .find_map(|entity| {
            let text = entity.get::<Text>()?;
            if text.0 != line {
                return None;
            }
            entity.get::<TextColor>().map(|color| color.0)
        })
        .expect("the reason line renders")
}

/// Assert the built title lays out in Default look §3's left column and opens
/// no scroll container anywhere it draws.
fn assert_title_never_scrolls(app: &mut App) {
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(app);
    for entity in app.world().iter_entities() {
        if let Some(node) = entity.get::<Node>() {
            assert_eq!(
                node.overflow,
                Overflow::default(),
                "the title opens no scroll container"
            );
        }
    }
    title_column(app);
}

#[test]
fn boot_leaves_the_title_up_over_the_entry_scenes_opening_frame() {
    let app = temp_title_app();
    assert!(
        menu(&app).title_up(),
        "the first frame after a good load is the title"
    );

    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), "first_meeting");
    assert!(matches!(runtime.runner.wait(), Some(Wait::Say { .. })));
    assert_eq!(runtime.staging.background.as_deref(), Some("school_gate"));
    assert_eq!(runtime.staging.bgm.as_deref(), Some("music_morning"));

    for info in unmei_engine::list_slots(runtime.runner.pack()) {
        assert_eq!(info.status, SlotStatus::Empty, "a launch writes no slot");
    }
}

#[test]
fn the_title_freezes_the_play_path_byte_identical() {
    let mut app = temp_title_app();
    let before = parked_position(&app);

    for key in [
        KeyCode::Space,
        KeyCode::KeyA,
        KeyCode::F11,
        KeyCode::F5,
        KeyCode::F9,
        KeyCode::Escape,
        KeyCode::Digit1,
        KeyCode::Tab,
    ] {
        press_and_update(&mut app, key);
        assert_eq!(parked_position(&app), before, "{key:?} moved the runner");
        assert!(menu(&app).title_up(), "{key:?} left the title");
    }
    assert!(
        !autosave_path(&app).exists(),
        "the title writes no autosave"
    );
}

#[test]
fn enter_play_presses_a_real_new_game() {
    let mut app = temp_title_app();
    assert!(menu(&app).title_up());
    enter_play(&mut app);
    assert!(!menu(&app).open, "New Game entered play");
    assert!(live_session(&app), "entering play sets the live session");
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Say { .. })
    ));
}

#[test]
fn new_game_at_boot_starts_play_with_no_confirm() {
    let pack = temp_pack();
    let mut app = boot_app(pack.path().join("pack"), None);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(!menu(&app).open, "New Game is immediate at boot");
    assert!(menu(&app).confirm.is_none(), "no confirm at boot");
    assert!(live_session(&app));
    assert!(!app.world().resource::<crate::slots::PendingAutosave>().0);
    assert!(!autosave_path(&app).exists());
    assert!(!manual_path(&app).exists());
}

#[test]
fn quit_at_boot_exits_with_no_confirm() {
    let mut app = temp_title_app();
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleQuit;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(
        menu(&app).confirm.is_none(),
        "no confirm with no live session"
    );
    assert!(matches!(app.should_exit(), Some(AppExit::Success)));
}

#[test]
fn continue_is_disabled_with_no_autosave_and_focus_falls_to_new_game() {
    let pack = temp_pack();
    let app = boot_app(pack.path().join("pack"), None);
    let state = menu(&app);
    assert!(!state.continue_enabled, "no autosave disables Continue");
    assert_eq!(state.continue_line, "No autosave yet");
    assert_eq!(
        state.focus,
        MenuFocus::TitleNewGame,
        "focus falls to New Game"
    );
}

/// Re-enter the title with the current live session, without holding a borrow
/// of `Runtime` while mutating `MenuState`.
fn reenter_title(app: &mut App) {
    let (pack, scene) = {
        let runtime = app.world().resource::<Runtime>();
        (
            runtime.runner.pack().clone(),
            runtime.runner.scene_id().to_string(),
        )
    };
    app.world_mut()
        .resource_mut::<MenuState>()
        .enter_title(&pack, &scene);
}

#[test]
fn continue_reads_the_autosave_once_the_title_is_entered_again() {
    let pack = temp_pack();
    let mut app = boot_app(pack.path().join("pack"), None);
    enter_play(&mut app);
    play_to_end(&mut app);
    reenter_title(&mut app);
    let state = menu(&app);
    assert!(
        state.continue_enabled,
        "a loadable autosave enables Continue"
    );
    assert!(
        state.continue_line.contains("reunion") || state.continue_line.contains("epilogue"),
        "Continue names the autosave's scene: {:?}",
        state.continue_line
    );
}

#[test]
fn title_settings_opens_the_shared_view_with_the_title_door_and_backs_out() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut app, KeyCode::Enter);

    assert_eq!(menu(&app).view, MenuView::Settings);
    assert_eq!(menu(&app).settings_origin, menu::MenuOrigin::Title);

    build_menu_ui(&mut app);
    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == "\u{2190} Back to title"),
        "the title door's back control reads the title label: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("Resume")),
        "the title's Settings has no Resume"
    );
    assert!(
        !texts.iter().any(|text| text == "Exit to title"),
        "the title's Settings has no Exit to title"
    );

    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Title);
    assert!(menu(&app).title_up());
    esc(&mut app);
    assert!(menu(&app).title_up(), "Esc on the title does nothing");
}

#[test]
fn exit_to_title_keeps_the_live_session_and_continue_resumes_it() {
    let mut app = temp_title_app();
    enter_play(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    let before = parked_position(&app);

    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).settings_origin, menu::MenuOrigin::Slots);
    click(&mut app, MenuAction::ExitToTitle);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ExitToTitle));
    click(&mut app, MenuAction::ConfirmAccept);

    assert!(menu(&app).title_up(), "Exit to title shows the title");
    assert!(live_session(&app), "the session is kept");
    assert_eq!(parked_position(&app), before, "position is untouched");
    assert!(
        !autosave_path(&app).exists(),
        "Exit to title writes nothing"
    );

    // A slot that turns unreadable behind the title cannot affect a
    // live-session Continue: it reads no slot.
    std::fs::write(autosave_path(&app), b"not a save").unwrap();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue entered play");
    assert_eq!(parked_position(&app), before, "Continue resumes verbatim");
}

#[test]
fn new_game_with_a_live_session_confirms_once_and_resets() {
    let mut app = temp_title_app();
    enter_play(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    // The fresh-boot park is the entry scene's first say.
    let boot_wait = {
        let pack = app.world().resource::<Runtime>().runner.pack().clone();
        ScriptRunner::new(pack).run().unwrap().wait
    };
    reenter_title(&mut app);

    app.world_mut().resource_mut::<Notice>().show("stale");
    app.world_mut()
        .resource_mut::<crate::slots::PendingAutosave>()
        .arm();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click(&mut app, MenuAction::ConfirmAccept);

    assert!(!menu(&app).open, "accepting starts play");
    assert!(live_session(&app));
    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), "first_meeting");
    assert_eq!(
        runtime.runner.wait().cloned(),
        boot_wait,
        "back at the fresh-boot park"
    );
    assert_eq!(runtime.staging.background.as_deref(), Some("school_gate"));
    assert!(!app.world().resource::<Notice>().is_showing());
    assert!(!app.world().resource::<crate::slots::PendingAutosave>().0);
    assert!(
        app.world().resource::<AppliedAudio>().is_forced(),
        "audio replays"
    );
    assert!(
        app.world().resource::<AppliedVisuals>().is_forced(),
        "visuals replay"
    );
    assert!(
        !app.world().resource::<AppliedUi>().is_ready(),
        "the reveal is re-armed"
    );
}

#[test]
fn new_game_cancels_an_in_flight_decision() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    assert!(app.world().resource::<PendingDecision>().0.is_some());

    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open);
    assert!(app.world().resource::<PendingDecision>().0.is_none());
    assert!(app.world().resource::<HeldDecision>().0.is_none());
    fake.wait_until_dropped();
}

/// New Game writes no slot and no settings change; the autosave only lands on
/// the first scene crossing after it (`docs/title-and-menu.md` § Title §3, §7).
#[test]
fn new_game_writes_no_slot_and_the_first_crossing_autosaves() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    // A manual slot and a dirty settings file exist to prove New Game leaves
    // both alone.
    press_and_update(&mut app, KeyCode::F5);
    assert!(manual_path(&app).exists(), "the manual save landed");
    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut app, KeyCode::Enter);
    click(&mut app, MenuAction::AudioMute);
    esc(&mut app); // back to the title, flushing the dirty settings file
    assert!(menu(&app).title_up());
    assert!(settings_path(&app).exists(), "the flush landed");

    let manual_before = std::fs::read(manual_path(&app)).unwrap();
    let settings_before = std::fs::read(settings_path(&app)).unwrap();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "New Game entered play");

    assert!(!autosave_path(&app).exists(), "New Game writes no autosave");
    assert_eq!(
        std::fs::read(manual_path(&app)).unwrap(),
        manual_before,
        "the manual slot is byte-identical"
    );
    assert_eq!(
        std::fs::read(settings_path(&app)).unwrap(),
        settings_before,
        "the settings file is byte-identical"
    );

    // The first scene crossing after New Game is what writes the autosave.
    press_and_update(&mut app, KeyCode::Space); // Ace's say
    press_and_update(&mut app, KeyCode::Space); // the choice
    press_and_update(&mut app, KeyCode::Digit1); // ask_letter's say
    press_and_update(&mut app, KeyCode::Space); // jump reunion
    assert_eq!(
        app.world().resource::<Runtime>().runner.scene_id(),
        "reunion"
    );
    let save = save::read_slot(
        app.world().resource::<Runtime>().runner.pack(),
        Slot::Autosave,
    )
    .expect("the crossing autosaved");
    assert_eq!(save.scene_id, "reunion");
}

/// Manual slots and the credential file are byte-identical after a New Game
/// (`docs/title-and-menu.md` § Title §3, §7).
#[test]
fn manual_slots_and_the_credential_file_survive_a_new_game() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let credential = credentials::credentials_path_in(config_dir.path());
    let key = format!("sk-nano-{}", "m".repeat(60));
    credentials::write(config_dir.path(), &key).unwrap();
    let credential_before = std::fs::read(&credential).unwrap();

    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(credential.clone()),
        row: KeyRow::NotSet,
    });
    press_and_update(&mut app, KeyCode::F5);
    let manual_before = std::fs::read(manual_path(&app)).unwrap();

    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click(&mut app, MenuAction::ConfirmAccept);

    assert_eq!(
        std::fs::read(manual_path(&app)).unwrap(),
        manual_before,
        "the manual slot is untouched"
    );
    assert_eq!(
        std::fs::read(&credential).unwrap(),
        credential_before,
        "the credential file is untouched"
    );
}

/// After New Game + Quit + relaunch, Continue loads the pre-New-Game autosave:
/// New Game never clobbers the checkpoint (`docs/title-and-menu.md` § Title §3's
/// accepted consequence, §7).
#[test]
fn the_old_autosave_survives_new_game_quit_and_relaunch() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");

    // Playthrough 1 crosses into `reunion`, writing the autosave.
    let mut app = app_for_pack(pack_root.clone());
    press_and_update(&mut app, KeyCode::Space); // Ace's say
    press_and_update(&mut app, KeyCode::Space); // the choice
    press_and_update(&mut app, KeyCode::Digit1); // ask_letter's say
    press_and_update(&mut app, KeyCode::Space); // jump reunion
    let (saved_scene, saved_wait) = {
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.runner.scene_id(), "reunion");
        (
            runtime.runner.scene_id().to_string(),
            runtime.runner.wait().cloned(),
        )
    };

    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "New Game started the new session");

    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleQuit;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::Quit));
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(matches!(app.should_exit(), Some(AppExit::Success)));

    let mut relaunched = boot_app(pack_root, None);
    assert!(menu(&relaunched).continue_enabled);
    relaunched.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut relaunched, KeyCode::Enter);
    assert!(!menu(&relaunched).open, "Continue entered play");
    let runtime = relaunched.world().resource::<Runtime>();
    assert_eq!(runtime.runner.scene_id(), saved_scene.as_str());
    assert_eq!(runtime.runner.wait().cloned(), saved_wait);
}

#[test]
fn the_title_renders_the_wordmark_rows_caption_and_legend() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let name = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .manifest
        .name
        .clone();
    let texts = all_texts(&app);
    assert!(
        texts.contains(&name),
        "the wordmark is the manifest name: {texts:?}"
    );
    for label in ["Continue", "New Game", "Settings", "Gallery", "Quit"] {
        assert!(
            texts.iter().any(|text| text.starts_with(label)),
            "missing {label}"
        );
    }
    assert_eq!(
        title_label(&app, MenuAction::Gallery).as_deref(),
        Some("Gallery"),
        "Gallery's row is caption-free"
    );
    let legend_count = texts
        .iter()
        .filter(|text| *text == menu::TITLE_LEGEND)
        .count();
    assert_eq!(
        legend_count, 1,
        "the legend renders exactly once: {texts:?}"
    );
    for chip in ["AUTOSAVE PRESENT", "LIVE FROZEN SESSION"] {
        assert!(
            texts.iter().all(|text| !text.contains(chip)),
            "the prototype's state chip never renders: {chip}"
        );
    }
    assert!(
        texts.iter().any(|text| text == "No autosave yet"),
        "Continue's reason line renders"
    );
}

/// Default look §3: the title's left column sits 104px in and 150px down, 420px
/// wide, with the veil over the backdrop, the reason line 26px under the rows
/// and the legend at the frame's bottom right.
#[test]
fn the_title_draws_the_left_column_and_the_veil() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let column = title_column(&app);
    let node = app
        .world()
        .get::<Node>(column)
        .expect("the column's node")
        .clone();
    assert_eq!(node.position_type, PositionType::Absolute);
    assert_eq!(node.flex_direction, FlexDirection::Column);
    assert_eq!(
        node.row_gap,
        Val::Px(46.0),
        "the rows start 46px under the tagline"
    );

    let veil = veil_node(&app);
    let root = parent_of(&app, veil);
    let order = children_of(&app, root);
    assert_eq!(
        order.get(1),
        Some(&veil),
        "the veil follows the backdrop slot"
    );
    assert_eq!(
        order.get(2),
        Some(&column),
        "the wordmark and rows draw over the veil"
    );
    let BackgroundGradient(gradients) = app
        .world()
        .get::<BackgroundGradient>(veil)
        .expect("the veil draws a gradient");
    let [bevy::ui::Gradient::Linear(linear)] = gradients.as_slice() else {
        panic!("the veil is one linear gradient: {gradients:?}");
    };
    let stops: Vec<(Val, f32)> = linear
        .stops
        .iter()
        .map(|stop| (stop.point, stop.color.to_srgba().alpha))
        .collect();
    assert_eq!(
        stops,
        vec![
            (Val::Percent(0.0), 0.86),
            (Val::Percent(38.0), 0.55),
            (Val::Percent(62.0), 0.0)
        ],
        "§3's veil is dark at the left edge, through 38%, clear by 62%"
    );

    let reason = text_parent(&app, &menu(&app).continue_line);
    let rows_box = parent_of(&app, reason);
    assert_eq!(
        app.world()
            .get::<Node>(rows_box)
            .expect("the rows box")
            .row_gap,
        Val::Px(26.0)
    );

    let legend = text_entity(&app, menu::TITLE_LEGEND);
    let node = app.world().get::<Node>(legend).expect("the legend's node");
    assert_eq!(node.position_type, PositionType::Absolute);
    assert_eq!((node.right, node.bottom), (Val::Px(40.0), Val::Px(26.0)));
}

/// Default look §3's focused row: white, with the 12x2 FOCUS bar lit beside it
/// and no ring on the control, and a disabled row dimmed but still in the walk.
#[test]
fn the_focused_row_is_white_with_the_focus_bar() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    app.update();

    assert_eq!(
        menu(&app).focus,
        MenuFocus::TitleNewGame,
        "the temp pack has no autosave, so focus starts on New Game"
    );
    let focused = row_item(&app, MenuAction::NewGame);
    assert_eq!(alpha_of(&app, focus_bar(&app, focused)), 1.0);
    assert_eq!(tone_of(&app, row_label(&app, focused)), TEXT);

    let unfocused = row_item(&app, MenuAction::Quit);
    assert_eq!(alpha_of(&app, focus_bar(&app, unfocused)), 0.0);
    assert_eq!(tone_of(&app, row_label(&app, unfocused)), DIM);

    let disabled = row_item(&app, MenuAction::Continue);
    assert_eq!(
        tone_of(&app, row_label(&app, disabled)),
        FAINT,
        "a disabled row is dimmed and stays in the walk"
    );

    let control = children_of(&app, focused)
        .into_iter()
        .find(|child| app.world().get::<MenuAction>(*child) == Some(&MenuAction::NewGame))
        .expect("the row's control");
    assert_eq!(
        app.world()
            .get::<Node>(control)
            .expect("the control's node")
            .border,
        UiRect::ZERO,
        "a focused row is the bar and the white label, not the overlay's ring"
    );

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    app.update();
    assert_eq!(
        alpha_of(&app, focus_bar(&app, row_item(&app, MenuAction::Gallery))),
        1.0,
        "moving the focus lights the bar on the same frame"
    );
    assert_eq!(
        alpha_of(&app, focus_bar(&app, focused)),
        0.0,
        "the row the focus left goes dark"
    );
    assert_eq!(tone_of(&app, row_label(&app, focused)), DIM);
}

/// Default look §3 spaces the five rows evenly: the maintainer's decision leaves
/// `Quit`'s 14px gap out, because one item template cannot space its fifth item
/// differently, and the format gains no property for it.
#[test]
fn the_five_rows_share_one_template_and_one_gap() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let items = [
        MenuAction::Continue,
        MenuAction::NewGame,
        MenuAction::OpenSettings,
        MenuAction::Gallery,
        MenuAction::Quit,
    ]
    .map(|action| row_item(&app, action));
    let list = parent_of(&app, items[0]);
    assert_eq!(
        children_of(&app, list),
        items.to_vec(),
        "one list holds the five rows in Title §2's order, with no separator"
    );
    let first = app.world().get::<Node>(items[0]).expect("the item's node");
    for item in &items[1..] {
        assert_eq!(
            app.world().get::<Node>(*item),
            Some(first),
            "every row is the one item template"
        );
    }
    assert_eq!(
        app.world()
            .get::<Node>(list)
            .expect("the list's node")
            .row_gap,
        Val::Px(0.0),
        "the rows are evenly spaced, with no per-item offset"
    );
}

/// The neutral title gradient's bright tone sits at the upper middle — the
/// node's horizontal centre, ~28% down — not on the right edge, where
/// `CENTER.at_percent(50, 28)` lands: a `UiPosition`'s `x`/`y` are offsets from
/// the anchor, and `resolve` yields node-centre-relative coordinates
/// (`docs/title-and-menu.md` § Title layout §2).
#[test]
fn the_title_gradient_centres_on_the_upper_middle() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let radial = title_gradient_radial(&app);
    assert_eq!(radial.shape, RadialGradientShape::FarthestCorner);
    assert_eq!(
        radial.stops.len(),
        2,
        "the neutral backdrop is a two-stop radial"
    );
    assert_eq!(radial.stops[0].point, Val::Percent(0.0));
    assert_eq!(radial.stops[1].point, Val::Percent(100.0));
    assert_eq!(radial.stops[0].color, Color::srgb(0.106, 0.133, 0.188));
    assert_eq!(radial.stops[1].color, Color::srgb(0.043, 0.051, 0.071));

    // Resolve against a 1280x720 node. `resolve` returns node-centre-relative
    // coordinates — the middle is the origin and the corners are ±half the
    // node — so the upper middle is `(0, -0.22*H)`. The title root is
    // full-window, so the node and its render target are the same size.
    let size = Vec2::new(1280.0, 720.0);
    let centre = radial.position.resolve(1.0, size, size);
    assert_eq!(centre.x, 0.0, "the bright tone is horizontally centred");
    let from_top = centre.y + size.y / 2.0;
    assert!(
        (from_top - size.y * 0.28).abs() < 0.5,
        "the bright tone is ~28% down from the top, got {from_top}"
    );
}

/// The text of `parent`'s direct children, in child order.
fn child_texts(app: &App, parent: Entity) -> Vec<String> {
    let Some(children) = app.world().entity(parent).get::<Children>() else {
        return Vec::new();
    };
    children
        .iter()
        .filter_map(|child| app.world().entity(child).get::<Text>().map(|t| t.0.clone()))
        .collect()
}

/// The parent of the single text node holding `text`.
fn text_parent(app: &App, text: &str) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            let matches = entity.get::<Text>().is_some_and(|t| t.0 == text);
            matches
                .then(|| entity.get::<ChildOf>().map(ChildOf::parent))
                .flatten()
        })
        .unwrap_or_else(|| panic!("no text node {text:?} with a parent"))
}

/// The demo pack's tagline renders uppercased directly under the wordmark
/// (`docs/title-and-menu.md` § Title layout §3/§8).
#[test]
fn the_title_renders_the_tagline_uppercased_under_the_wordmark() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let name = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .manifest
        .name
        .clone();
    let wordmark_block = parent_of(&app, text_parent(&app, &name));
    let drawn: Vec<String> = children_of(&app, wordmark_block)
        .into_iter()
        .filter_map(|slot| child_texts(&app, slot).into_iter().next())
        .collect();
    assert_eq!(
        drawn,
        vec![name, "A DEMO PACK".to_string()],
        "the tagline is uppercased and sits directly under the wordmark"
    );
    let tracking = app.world().iter_entities().find_map(|entity| {
        let tagline = entity.get::<Text>().is_some_and(|t| t.0 == "A DEMO PACK");
        tagline
            .then(|| entity.get::<bevy::text::LetterSpacing>().copied())
            .flatten()
    });
    assert_eq!(
        tracking,
        Some(bevy::text::LetterSpacing::Px(
            crate::presentation::title::TAGLINE_TRACKING_PX
        )),
        "the tagline carries the spec's tracking"
    );
}

/// A 48-character tagline renders its full uppercased line (the warning boundary
/// in `docs/title-and-menu.md` § Title layout §8).
#[test]
fn a_forty_eight_character_tagline_renders_in_full() {
    let tagline = "x".repeat(48);
    let manifest = format!(
        "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n    tagline: \"{tagline}\",\n)\n"
    );
    let mut app = temp_title_app_with_manifest(Some(&manifest));
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == &tagline.to_uppercase()),
        "the full 48-char line renders: {texts:?}"
    );
}

/// A pack with no tagline renders exactly today's title: the wordmark alone,
/// with no tagline line and no spacing shift (`docs/title-and-menu.md` § Title
/// layout §3/§8).
#[test]
fn a_pack_without_a_tagline_renders_no_tagline_line() {
    let mut app = temp_title_app_with_manifest(Some(
        "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n)\n",
    ));
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let texts = all_texts(&app);
    assert!(
        texts.iter().all(|text| !text.contains("DEMO PACK")),
        "no tagline line renders: {texts:?}"
    );
    let name = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .manifest
        .name
        .clone();
    let wordmark_block = parent_of(&app, text_parent(&app, &name));
    let visible: Vec<String> = children_of(&app, wordmark_block)
        .into_iter()
        .filter(|slot| {
            app.world()
                .get::<Node>(*slot)
                .is_some_and(|node| node.display != Display::None)
        })
        .flat_map(|slot| child_texts(&app, slot))
        .collect();
    assert_eq!(visible, vec![name], "the wordmark sits alone");
}

/// `{var}` interpolation is deliberately not performed on the tagline: the copy
/// renders literally, uppercased (`docs/content-format.md` §6).
#[test]
fn the_tagline_is_never_interpolated() {
    let mut app = temp_title_app_with_manifest(Some(
        "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n    tagline: \"hello {player}\",\n)\n",
    ));
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);

    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == "HELLO {PLAYER}"),
        "the tagline renders literally: {texts:?}"
    );
}

/// `cover_size` fills both axes, preserves the texture's aspect, and lets the
/// overflowing axis crop; a degenerate dimension falls back to the window.
#[test]
fn cover_size_fills_both_axes_and_preserves_aspect() {
    // A 4:3 image in a 16:9 window: width fills, height overflows.
    let cover =
        crate::presentation::style::cover_size(Vec2::new(1024.0, 768.0), Vec2::new(1280.0, 720.0));
    assert_eq!(cover, Vec2::new(1280.0, 960.0));
    assert!((cover.x / cover.y - 4.0 / 3.0).abs() < 1e-4);

    // A 16:9 image in a 4:3 window: height fills, width overflows.
    let cover =
        crate::presentation::style::cover_size(Vec2::new(1280.0, 720.0), Vec2::new(1024.0, 768.0));
    assert!((cover.x - 1365.3333).abs() < 0.01, "{cover:?}");
    assert!((cover.y - 768.0).abs() < 1e-4, "{cover:?}");
    assert!((cover.x / cover.y - 16.0 / 9.0).abs() < 1e-4);

    // A degenerate image or window never divides by zero.
    assert_eq!(
        crate::presentation::style::cover_size(Vec2::ZERO, Vec2::new(1280.0, 720.0)),
        Vec2::new(1280.0, 720.0)
    );
    assert_eq!(
        crate::presentation::style::cover_size(Vec2::new(1280.0, 720.0), Vec2::ZERO),
        Vec2::ZERO
    );
}

/// `contain_size` is `cover_size`'s inverse: both axes fit inside the box,
/// aspect is preserved and the free axis letterboxes; a degenerate dimension
/// falls back to the box unchanged.
#[test]
fn contain_size_fits_both_axes_and_preserves_aspect() {
    // A 16:9 image in a 4:3 box: width fits, height letterboxes.
    let box_size = Vec2::new(360.0, 270.0);
    let contain = crate::presentation::style::contain_size(Vec2::new(1280.0, 720.0), box_size);
    assert_eq!(contain, Vec2::new(360.0, 202.5));
    assert!((contain.x / contain.y - 16.0 / 9.0).abs() < 1e-4);
    assert!(
        contain.x <= box_size.x && contain.y <= box_size.y,
        "{contain:?}"
    );

    // A 1:2 sprite in the same box: height fits, width pillarboxes.
    let contain = crate::presentation::style::contain_size(Vec2::new(256.0, 512.0), box_size);
    assert_eq!(contain, Vec2::new(135.0, 270.0));
    assert!((contain.x / contain.y - 256.0 / 512.0).abs() < 1e-4);
    assert!(
        contain.x <= box_size.x && contain.y <= box_size.y,
        "{contain:?}"
    );

    // A degenerate image or box never divides by zero.
    assert_eq!(
        crate::presentation::style::contain_size(Vec2::ZERO, box_size),
        box_size
    );
    assert_eq!(
        crate::presentation::style::contain_size(Vec2::new(1280.0, 720.0), Vec2::ZERO),
        Vec2::ZERO
    );
}

/// `title_backdrop: "<id>"` draws `assets/backgrounds/<id>.png` full-window,
/// unmodified over the gradient underlay, with Default look §3's veil as a
/// sibling over it (`docs/title-and-menu.md` § Title layout §2, Default look §3).
#[test]
fn the_title_draws_the_declared_backdrop_covering_the_window() {
    // The demo pack declares `title_backdrop: "school_gate"`.
    let mut app = title_asset_app(None);
    pump_until_watch_drains(&mut app);

    let (image, node) = title_backdrop_node(&app).expect("the declared backdrop draws");
    assert_eq!(
        image.color,
        Color::WHITE,
        "the art is drawn verbatim — no veil, tint or overlay"
    );
    assert_eq!(image.image_mode, NodeImageMode::Stretch);
    // The stand-in loads as the loader's 1x1 image, so covering a 16:9 window
    // needs a 1280x1280 node cropped to the window's height; the general aspect
    // maths is pinned by `cover_size` above.
    assert_eq!(node.width, Val::Px(1280.0));
    assert_eq!(node.height, Val::Px(1280.0));
    assert_eq!(node.left, Val::Px(0.0));
    assert_eq!(node.top, Val::Px(-280.0));
    assert!(
        app.world()
            .iter_entities()
            .any(|entity| entity.get::<BackgroundGradient>().is_some()),
        "the neutral gradient underlay stays beneath the art"
    );
    let art = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::title::TitleBackdropImage>()
                .map(|_| entity.id())
        })
        .expect("the art's node");
    let slot = parent_of(&app, art);
    let order = children_of(&app, parent_of(&app, slot));
    assert_eq!(order.first(), Some(&slot), "the art draws first");
    assert_eq!(
        order.get(1),
        Some(&veil_node(&app)),
        "§3's veil draws over the art"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a loaded backdrop never notices"
    );

    // Re-entering the title rebuilds the tree and draws the same backdrop, once.
    // `continue_line` is part of `MenuBuilt`'s signature, so the change forces a
    // real rebuild; the rendered line proves it, and the handle proves the same
    // `backgrounds/<id>.png` resolved again.
    let before_handle = image.image.clone();
    app.world_mut().resource_mut::<MenuState>().continue_line = "re-entered".to_string();
    app.update();
    pump_until_watch_drains(&mut app);
    assert!(
        all_texts(&app).iter().any(|text| text == "re-entered"),
        "the title tree rebuilt"
    );
    let backdrops = app
        .world()
        .iter_entities()
        .filter(|entity| {
            entity
                .get::<crate::presentation::title::TitleBackdropImage>()
                .is_some()
        })
        .count();
    assert_eq!(backdrops, 1, "re-entry draws exactly one backdrop");
    let (after, _) = title_backdrop_node(&app).expect("the backdrop survives re-entry");
    assert_eq!(
        after.color,
        Color::WHITE,
        "the rebuilt art is still unmodified"
    );
    assert_eq!(
        after.image, before_handle,
        "re-entry resolves the same backdrop asset"
    );
}

/// A pack with neither `tagline` nor `title_backdrop` renders exactly today's
/// title: the gradient and no backdrop node
/// (`docs/title-and-menu.md` § Title layout §8).
#[test]
fn a_pack_without_a_title_backdrop_draws_only_the_gradient() {
    let mut app = title_asset_app(Some(
        "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n)\n",
    ));
    pump_until_watch_drains(&mut app);

    assert!(
        title_backdrop_node(&app).is_none(),
        "no backdrop image node is drawn"
    );
    assert!(
        app.world()
            .iter_entities()
            .any(|entity| entity.get::<BackgroundGradient>().is_some()),
        "the gradient remains the whole backdrop"
    );
}

/// Settings opened from the title keeps the same backdrop
/// (`docs/title-and-menu.md` § Title layout §5).
#[test]
fn title_settings_keeps_the_same_backdrop() {
    let mut app = title_asset_app(None);
    pump_until_watch_drains(&mut app);
    {
        let mut state = app.world_mut().resource_mut::<MenuState>();
        state.view = MenuView::Settings;
        state.settings_origin = menu::MenuOrigin::Title;
    }
    app.world_mut()
        .run_system_once(crate::presentation::sync_menu_ui)
        .expect("sync_menu_ui runs");

    assert!(
        title_backdrop_node(&app).is_some(),
        "the title door's Settings keeps the backdrop"
    );
}

/// A declared backdrop whose file is present but undecodable is the same silent
/// degradation as a missing one: one `warn!`, the gradient stands, no Notice
/// (`docs/runtime-hardening.md` §2 row 25).
#[test]
fn a_declared_but_undecodable_backdrop_warns_once_and_draws_the_gradient() {
    captures::install();
    // The loader reads the file and rejects empty bytes, the decoder-failure
    // shape of the same `LoadState::Failed` arm a corrupt PNG takes.
    let mut app = title_asset_app_with_assets(
        Some(
            "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n    title_backdrop: \"broken_art\",\n)\n",
        ),
        &[("backgrounds/broken_art.png", b"")],
    );
    force_single_threaded(&mut app);
    captures::take();
    pump_until_watch_drains(&mut app);

    let records = failure_lines();
    assert_eq!(
        records.len(),
        1,
        "one warn for the undecodable path: {records:?}"
    );
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't load the title backdrop")
            && records[0].2.contains("backgrounds/broken_art.png"),
        "row 25 names the undecodable path: {:?}",
        records[0].2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a silent degradation never notices"
    );
    assert!(
        app.world()
            .iter_entities()
            .any(|entity| entity.get::<BackgroundGradient>().is_some()),
        "the gradient is the fallback"
    );
}

/// A declared backdrop whose file is missing is a silent degradation: one
/// `warn!`, the gradient stands, no Notice, and no duplicate on re-entry
/// (`docs/runtime-hardening.md` §2 row 25).
#[test]
fn a_declared_but_missing_backdrop_warns_once_and_draws_the_gradient() {
    captures::install();
    let mut app = title_asset_app(Some(
        "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n    title_backdrop: \"missing_art\",\n)\n",
    ));
    force_single_threaded(&mut app);
    captures::take();
    pump_until_watch_drains(&mut app);

    let records = failure_lines();
    assert_eq!(
        records.len(),
        1,
        "one warn for the failed path: {records:?}"
    );
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't load the title backdrop"),
        "row 25's short string prefixes the line: {:?}",
        records[0].2
    );
    assert!(
        records[0].2.contains("backgrounds/missing_art.png"),
        "the line names the path: {:?}",
        records[0].2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a silent degradation never notices"
    );
    assert!(
        app.world()
            .iter_entities()
            .any(|entity| entity.get::<BackgroundGradient>().is_some()),
        "the gradient is the fallback"
    );

    // Re-entry rebuilds the title; the path is already reported, so no second
    // warn (`AssetWatch`'s one-per-path rule).
    captures::take();
    app.world_mut().resource_mut::<MenuState>().continue_line = "a changed reason line".to_string();
    app.update();
    pump_until_watch_drains(&mut app);
    assert!(
        failure_lines().is_empty(),
        "a re-entered title must not warn twice"
    );
}

/// Continue's reason line draws the tree's own tone in both states: Default look
/// §3 places the line 26px under the rows and states no tone for it, so the
/// reference plugin's `faint` stands, and a disabled Continue reads from its
/// dimmed row instead. § Title layout §4's warm-warn line is one of the rules
/// §3 supersedes.
#[test]
fn the_reason_line_draws_the_trees_tone_in_both_states() {
    let mut disabled = temp_title_app();
    disabled.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut disabled);
    assert!(!menu(&disabled).continue_enabled);
    assert_eq!(
        title_reason_color(&disabled),
        FAINT,
        "a disabled Continue's reason line keeps the tree's tone"
    );

    let pack = pack_with_a_stamped_autosave();
    let mut enabled = boot_app(pack.path().join("pack"), None);
    assert!(menu(&enabled).continue_enabled);
    enabled.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut enabled);
    assert_eq!(
        title_reason_color(&enabled),
        FAINT,
        "an enabled Continue's reason line is the same tone"
    );
}

/// The four title states lay out with no scroll container and the spec's column
/// width. The project ships no font, so the fit bullet
/// (`docs/title-and-menu.md` § Title layout §1, §8) is asserted structurally:
/// nothing in the title can scroll.
#[test]
fn the_title_fits_without_a_scroll_container_in_every_state() {
    assert_title_never_scrolls(&mut temp_title_app());

    let pack = pack_with_a_stamped_autosave();
    let mut loadable = boot_app(pack.path().join("pack"), None);
    assert!(menu(&loadable).continue_enabled);
    assert_title_never_scrolls(&mut loadable);

    let mut live = temp_title_app();
    enter_play(&mut live);
    reenter_title(&mut live);
    assert!(menu(&live).continue_enabled);
    assert_title_never_scrolls(&mut live);

    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = {
        let loaded = load_pack(&pack_root).expect("pack loads");
        save::save_path_for(&loaded.pack, Slot::Autosave)
    };
    std::fs::write(&path, b"not a save").unwrap();
    let mut unreadable = boot_app(pack_root, None);
    assert!(!menu(&unreadable).continue_enabled);
    assert_title_never_scrolls(&mut unreadable);
}

#[test]
fn the_title_walk_keeps_its_five_rows() {
    let mut app = temp_title_app();
    assert_eq!(menu(&app).focus, MenuFocus::TitleNewGame);

    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TitleSettings);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TitleGallery);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TitleQuit);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TitleContinue);
}

/// The title's `Gallery` row is enabled and caption-free; activating it opens
/// the gallery with the title as origin, moving nothing
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1/§3).
#[test]
fn the_titles_gallery_row_opens_the_gallery_with_the_title_as_origin() {
    let mut app = temp_title_app();
    press_and_update(&mut app, KeyCode::ArrowDown);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::TitleGallery);
    let before = parked_position(&app);

    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert_eq!(menu(&app).gallery_origin, menu::MenuOrigin::Title);
    assert_eq!(parked_position(&app), before, "opening changes nothing");

    click(&mut app, MenuAction::GalleryBack);
    assert_eq!(menu(&app).view, MenuView::Title);
    assert!(menu(&app).title_up());
    assert_eq!(parked_position(&app), before);
}

/// A minimal presentation app over the demo pack with the audio/visual sync
/// systems, so the title's silence and suppression can be driven headless.
fn chrome_app(staging: StagingState) -> App {
    // The interface faces are embedded, so `sync_wait_ui` can build the
    // dialogue spans; see `attach_text_faces`.
    let loaded = load_pack(&demo_pack_root()).expect("demo pack loads");
    let runtime = Runtime {
        runner: ScriptRunner::new(loaded.pack),
        staging,
        pack_root: demo_pack_root(),
        asset_root: demo_pack_root().join("assets"),
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: demo_pack_root()
                .join("assets")
                .to_string_lossy()
                .into_owned(),
            ..default()
        })
        .init_asset::<Image>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .insert_resource(runtime)
        .init_resource::<MenuState>()
        .init_resource::<Notice>()
        .init_resource::<DefensiveHold>()
        .init_resource::<AssetWatch>()
        .init_resource::<AppliedVisuals>()
        .init_resource::<AppliedAudio>()
        .init_resource::<AppliedUi>()
        .init_resource::<TextReveal>()
        .insert_resource(AudioChannel::<Bgm>::default());
    attach_text_faces(&mut app);
    app
}

fn raise_title(app: &mut App) {
    let mut menu_state = app.world_mut().resource_mut::<MenuState>();
    menu_state.open = true;
    menu_state.view = MenuView::Title;
}

fn lower_title(app: &mut App) {
    let mut menu_state = app.world_mut().resource_mut::<MenuState>();
    menu_state.open = false;
    menu_state.view = MenuView::Slots;
}

#[test]
fn the_title_is_silent_until_play_is_entered() {
    let mut app = chrome_app(StagingState {
        bgm: Some("music_morning".into()),
        ..default()
    });
    raise_title(&mut app);
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_audio)
        .expect("sync_audio runs");
    assert!(
        app.world()
            .resource::<AppliedAudio>()
            .current_bgm()
            .is_none(),
        "the title stages music but never sounds it"
    );

    lower_title(&mut app);
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_audio)
        .expect("sync_audio runs");
    assert_eq!(
        app.world().resource::<AppliedAudio>().current_bgm(),
        Some("music_morning"),
        "entering play starts the entry scene's track from the top"
    );
}

/// The title's Settings and gallery doors keep the title's silence: the play
/// path stays silent behind them too (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §6).
#[test]
fn the_titles_sub_views_keep_the_title_silent() {
    let mut app = chrome_app(StagingState {
        bgm: Some("music_morning".into()),
        ..default()
    });
    raise_title(&mut app);

    for (view, settings_origin, gallery_origin) in [
        (
            MenuView::Settings,
            menu::MenuOrigin::Title,
            menu::MenuOrigin::Slots,
        ),
        (
            MenuView::Gallery,
            menu::MenuOrigin::Slots,
            menu::MenuOrigin::Title,
        ),
    ] {
        {
            let mut menu_state = app.world_mut().resource_mut::<MenuState>();
            menu_state.view = view;
            menu_state.settings_origin = settings_origin;
            menu_state.gallery_origin = gallery_origin;
        }
        app.world_mut()
            .run_system_once(crate::presentation::play::sync_audio)
            .expect("sync_audio runs");
        assert!(
            app.world()
                .resource::<AppliedAudio>()
                .current_bgm()
                .is_none(),
            "{view:?} from the title stays silent"
        );
    }

    // The overlay's own door sounds the staged track as before.
    {
        let mut menu_state = app.world_mut().resource_mut::<MenuState>();
        menu_state.view = MenuView::Slots;
        menu_state.settings_origin = menu::MenuOrigin::Slots;
    }
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_audio)
        .expect("sync_audio runs");
    assert_eq!(
        app.world().resource::<AppliedAudio>().current_bgm(),
        Some("music_morning"),
        "the overlay's door sounds the staged track"
    );
}

#[test]
fn no_play_sprites_are_drawn_while_the_title_is_up() {
    let mut app = chrome_app(StagingState {
        background: Some("school_gate".into()),
        sprites: [("ace".to_string(), "neutral".to_string())].into(),
        bgm: Some("music_morning".into()),
    });

    app.world_mut()
        .run_system_once(crate::presentation::play::sync_staging_visuals)
        .unwrap();
    let sprites = |app: &App| {
        app.world()
            .iter_entities()
            .filter(|entity| {
                entity
                    .get::<crate::presentation::play::BackgroundSprite>()
                    .is_some()
                    || entity
                        .get::<crate::presentation::play::NpcSprite>()
                        .is_some()
            })
            .count()
    };
    assert!(sprites(&app) > 0, "play draws its sprites");

    raise_title(&mut app);
    app.world_mut()
        .run_system_once(crate::presentation::play::sync_staging_visuals)
        .unwrap();
    assert_eq!(sprites(&app), 0, "no sprite is drawn behind the title");
}

#[test]
fn a_broken_pack_never_shows_the_title() {
    let app = app_for_broken_pack();
    assert!(
        !menu(&app).open,
        "the title never appears for a broken pack"
    );
    assert!(!menu(&app).title_up());
    assert!(app.world().get_resource::<Runtime>().is_none());
}

#[test]
fn pointer_and_keyboard_agree_on_the_title_rows() {
    let mut app = temp_title_app();
    click(&mut app, MenuAction::Gallery);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    click(&mut app, MenuAction::GalleryBack);
    assert_eq!(menu(&app).view, MenuView::Title);
    click(&mut app, MenuAction::NewGame);
    assert!(!menu(&app).open, "New Game's click starts play");
}

#[test]
fn the_title_freezes_the_reveal_and_the_auto_dwell() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    set_text_speed(&mut app, 0.04);
    enable_auto(&mut app);
    render_dialogue(&mut app);

    {
        let mut menu_state = app.world_mut().resource_mut::<MenuState>();
        menu_state.open = true;
        menu_state.view = MenuView::Title;
    }
    let chars = app.world().resource::<TextReveal>().chars_shown(0.04);
    let parked = wait(&app);
    advance_time(&mut app, 5.0);
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.04),
        chars,
        "the reveal is frozen while the title is up"
    );
    tick_update(&mut app);
    assert_eq!(
        wait(&app),
        parked,
        "the auto dwell never accumulates on the title"
    );
}

#[test]
fn leaving_the_titles_settings_flushes_a_dirty_file() {
    let pack = temp_pack();
    let mut app = boot_app(pack.path().join("pack"), None);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Settings);

    click(&mut app, MenuAction::AudioMute);
    assert!(app.world().resource::<SettingsState>().muted());
    assert!(!settings_path(&app).exists(), "opening writes nothing");

    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Title);
    assert!(settings_path(&app).exists(), "leaving flushes the file");
    assert!(audio(&app).muted);
}

#[test]
fn continue_loads_the_autosave_through_the_existing_load_path() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let (saved_wait, saved_staging) = {
        let mut app = app_for_pack(pack_root.clone());
        press_and_update(&mut app, KeyCode::Space);
        let runtime = app.world().resource::<Runtime>();
        let wait = runtime.runner.wait().cloned();
        let staging = runtime.staging.clone();
        crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
        (wait, staging)
    };

    let mut app = boot_app(pack_root, None);
    assert!(menu(&app).continue_enabled, "the autosave enables Continue");
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(!menu(&app).open, "Continue entered play");
    assert!(live_session(&app));
    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.runner.wait().cloned(), saved_wait);
    assert_eq!(runtime.staging, saved_staging);
}

#[test]
fn an_answer_arriving_while_the_title_is_up_is_held_until_continue() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    reenter_title(&mut app);

    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app), "the answer is held while the title is up");

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue resumed the live session");
    assert!(!held(&app), "the held answer landed on resume");
    assert!(parked_on_say(&app));
}

#[test]
fn continue_resumes_a_mid_reveal_line_where_it_stopped() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    set_text_speed(&mut app, 0.04);
    render_dialogue(&mut app);
    advance_time(&mut app, 0.5);
    let mid = app.world().resource::<TextReveal>().chars_shown(0.04);
    assert!(mid > 0, "the line is mid-reveal");
    assert!(!app.world().resource::<TextReveal>().is_complete(0.04));

    reenter_title(&mut app);
    assert!(menu(&app).title_up());
    advance_time(&mut app, 5.0);
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.04),
        mid,
        "the time on the title leaves the reveal clock alone"
    );
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue resumed the live session");

    render_dialogue(&mut app);
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.04),
        mid,
        "the reveal clock resumes where it stopped"
    );
}

/// A live Continue is the Esc resume one level up: the auto dwell resumes with
/// exactly the time it had left, and skip is disarmed on entry so preserving
/// the dwell cannot become an unexpected fast-forward. The
/// remainder is anchored to real ticks, so a reset to the full `2.0 s` fails
/// this test — the defect this repair removes.
#[test]
fn live_continue_resumes_the_remaining_auto_dwell_and_disarms_skip() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    // Arm a real (non-instant) reveal and let it finish, so the reveal clock is
    // a live, observable value the resumed Continue has to preserve.
    set_text_speed(&mut app, 0.04);
    render_dialogue(&mut app);
    enable_auto(&mut app);
    advance_time(&mut app, 3.0); // finish the reveal
    tick_update(&mut app); // start the dwell
    assert!(
        app.world().resource::<TextReveal>().is_complete(0.04),
        "the entry reveal is finished before the dwell"
    );
    let reveal = app.world().resource::<TextReveal>().chars_shown(0.04);
    assert!(reveal > 0, "the reveal clock is armed");
    advance_time(&mut app, 1.5);
    tick_update(&mut app); // ~0.5 s left
    let line = wait(&app);
    assert!(line.is_some(), "the say is parked");

    // A skip mode engaged in play, then the title: it must not survive into the
    // resumed session, or the preserved dwell would be skipped over instead.
    app.world_mut()
        .resource_mut::<crate::core::SkipState>()
        .engage();
    assert!(skip_on(&app));

    reenter_title(&mut app);
    assert!(menu(&app).title_up());
    for _ in 0..3 {
        advance_time(&mut app, 2.0);
        tick_update(&mut app);
    }
    assert_eq!(wait(&app), line, "no dwell fires while the title is up");

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue resumed the live session");
    assert!(!skip_on(&app), "title→play disarms skip");
    assert_eq!(wait(&app), line, "the park is unchanged by Continue");
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.04),
        reveal,
        "the reveal clock is preserved by Continue"
    );

    advance_time(&mut app, 0.4);
    tick_update(&mut app);
    assert_eq!(
        wait(&app),
        line,
        "the dwell resumes from the remaining ~0.5 s, not a fresh 2.0 s"
    );
    advance_time(&mut app, 0.3);
    tick_update(&mut app);
    assert_ne!(wait(&app), line, "the remaining dwell completes");
}

/// New Game is a position replacement, so it resets the auto dwell even when it
/// lands back on the very park the dwell was counting on: the entry line gets
/// the full `2.0 s`, not the remainder the discarded session had left — New
/// Game's discontinuity-reset protection.
#[test]
fn new_game_resets_the_auto_dwell_on_the_same_park() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    enable_auto(&mut app);
    tick_update(&mut app); // start the dwell on the entry say
    advance_time(&mut app, 1.5);
    tick_update(&mut app); // ~0.5 s left
    let entry = wait(&app);
    assert!(entry.is_some(), "the entry say is parked");

    reenter_title(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter); // confirm: a live session is lost
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!menu(&app).open, "New Game starts play");
    assert_eq!(wait(&app), entry, "New Game lands on the same entry park");

    advance_time(&mut app, 1.9);
    tick_update(&mut app);
    assert_eq!(
        wait(&app),
        entry,
        "the discarded dwell's remainder does not carry into New Game"
    );
    advance_time(&mut app, 0.2);
    tick_update(&mut app);
    assert_ne!(wait(&app), entry, "the entry line gets its full 2.0 s");
}

/// Force the documented stale-acceptance case through ordinary production
/// operations: a real `read` of `slot`, then a public-field edit the read
/// cannot see, then the menu's own completion. The save is genuinely refused at
/// restoration, not at read, so the branch under test is the real completion
/// decision.
fn force_restore_refusal(
    app: &mut App,
    slot: Slot,
    origin: LoadOrigin,
    surface: LoadSurface,
) -> LoadOutcome {
    app.world_mut()
        .run_system_once(
            move |mut menu: ResMut<MenuState>, mut session: SessionInput| {
                let ReadOutcome::Save { mut save, path } = session.transitions().read(slot, origin)
                else {
                    panic!("the fixture slot must read successfully, so the refusal is at restore");
                };
                save.scene_id = "nowhere".to_string();
                crate::menu::complete_load(
                    &mut menu,
                    &mut session,
                    ReadOutcome::Save { save, path },
                    surface,
                )
            },
        )
        .expect("the one-shot load-completion system runs")
}

/// A fresh boot has no live session, so this is the launch-entry marker the
/// refusal must not set: a load genuinely refused at restoration leaves the
/// title up and does not enter play. The read succeeds on a
/// real autosave; only the already-read value's restore is refused.
#[test]
fn a_forced_restore_refusal_at_the_title_leaves_no_launch_entry_marker() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    {
        let mut app = app_for_pack(pack_root.clone());
        press_and_update(&mut app, KeyCode::Space);
        let runtime = app.world().resource::<Runtime>();
        crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
    }

    let mut app = boot_app(pack_root, None);
    assert!(!live_session(&app), "a fresh boot has no live session");
    assert!(menu(&app).continue_enabled, "the autosave is loadable");
    let before = parked_position(&app);
    let staging = app.world().resource::<Runtime>().staging.clone();
    let replay_before = (
        app.world().resource::<AppliedAudio>().is_forced(),
        app.world().resource::<AppliedVisuals>().is_forced(),
    );
    assert_eq!(
        replay_before,
        (false, false),
        "the fixture starts with no pending replay, so the comparison is meaningful"
    );

    let outcome = force_restore_refusal(
        &mut app,
        Slot::Autosave,
        LoadOrigin::Menu,
        LoadSurface::Title,
    );

    assert_eq!(
        outcome,
        LoadOutcome::Refused,
        "the restoration refused, not the read"
    );
    assert!(menu(&app).title_up(), "the title stays up");
    assert!(
        !live_session(&app),
        "a refused load cannot acquire the live-session entry marker"
    );
    assert_eq!(parked_position(&app), before, "the boot position survives");
    assert_eq!(
        app.world().resource::<Runtime>().staging,
        staging,
        "the boot staging survives"
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save"),
        "the existing load Notice fires"
    );
    assert!(
        !on_hold(&app),
        "row 17 is continue-with-message, not the defensive hold"
    );
    assert_eq!(
        (
            app.world().resource::<AppliedAudio>().is_forced(),
            app.world().resource::<AppliedVisuals>().is_forced(),
        ),
        replay_before,
        "a refusal requests no presentation replay"
    );
}

/// The restore-stage refusal is defensive — the read already ran the semantic
/// checks — and unreachable through file IO by construction, so this forces it
/// at the production seam: a real read, an edit the read cannot see, then the
/// menu's own completion. It witnesses the refusal outcome, its row-17
/// reporting and the no-mutation contract: the old runner, staging and
/// pending/held session survive intact, the title stays up, and no
/// successful-replacement cleanup runs.
#[test]
fn a_forced_restore_refusal_cannot_close_the_menu_or_enter_play() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    reenter_title(&mut app);

    // The in-flight answer lands while the title is up: it is held, not
    // applied, so a refused load owes it back untouched.
    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app), "the answer is held behind the title");

    // A real, loadable autosave so the read accepts the slot.
    {
        let runtime = app.world().resource::<Runtime>();
        crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
    }

    let before = parked_position(&app);
    let staging = app.world().resource::<Runtime>().staging.clone();

    let outcome = force_restore_refusal(
        &mut app,
        Slot::Autosave,
        LoadOrigin::Menu,
        LoadSurface::Title,
    );

    assert_eq!(outcome, LoadOutcome::Refused, "the refusal is reported");
    assert!(menu(&app).title_up(), "the refusal keeps the title up");
    assert!(live_session(&app), "the live session is untouched");
    assert_eq!(parked_position(&app), before, "the old runner survives");
    assert_eq!(
        app.world().resource::<Runtime>().staging,
        staging,
        "the old staging survives"
    );
    assert!(
        app.world().resource::<PendingDecision>().0.is_none(),
        "the pending slot is exactly as it was"
    );
    assert!(held(&app), "the held answer survives");
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save"),
        "the existing load Notice fires"
    );
    assert!(
        !on_hold(&app),
        "row 17 is continue-with-message, not the defensive hold"
    );
}

/// The overlay's own completion: a restore refused under the load grid keeps
/// the overlay on that surface and the old runner/staging intact.
#[test]
fn a_forced_restore_refusal_keeps_the_overlay_open() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);

    // A real manual slot, so the grid's card is actionable and the read accepts
    // it; only the already-read restore is refused.
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::F5);
    esc(&mut app);
    switch_mode(&mut app, MenuMode::Load);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::Card(0);
    assert!(menu(&app).open, "the overlay is open on the load grid");

    let before = parked_position(&app);
    let staging = app.world().resource::<Runtime>().staging.clone();

    let outcome = force_restore_refusal(
        &mut app,
        Slot::Manual(1),
        LoadOrigin::Menu,
        LoadSurface::Overlay,
    );

    assert_eq!(outcome, LoadOutcome::Refused, "the refusal is reported");
    assert!(menu(&app).open, "a refused restore leaves the overlay open");
    assert_eq!(
        menu(&app).view,
        MenuView::Slots,
        "the overlay stays on the slot grid"
    );
    assert_eq!(parked_position(&app), before, "the old runner survives");
    assert_eq!(
        app.world().resource::<Runtime>().staging,
        staging,
        "the old staging survives"
    );
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save"),
        "the existing load Notice fires"
    );
}

#[test]
fn a_refused_continue_load_refreshes_the_titles_metadata() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    {
        let mut app = app_for_pack(pack_root.clone());
        press_and_update(&mut app, KeyCode::Space);
        let runtime = app.world().resource::<Runtime>();
        crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
    }

    let mut app = boot_app(pack_root, None);
    assert!(menu(&app).continue_enabled);
    // The autosave turns unreadable after the metadata read.
    let path = save::save_path_for(
        app.world().resource::<Runtime>().runner.pack(),
        Slot::Autosave,
    );
    std::fs::write(&path, b"not a save").unwrap();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(menu(&app).title_up(), "a refused load stays on the title");
    assert_eq!(
        app.world().resource::<Notice>().text.as_deref(),
        Some("couldn't load that save"),
        "the refusal raises the existing load Notice"
    );
    assert!(
        !menu(&app).continue_enabled,
        "the refreshed metadata flips Continue to disabled"
    );
    assert_ne!(menu(&app).continue_line, "No autosave yet");
}

/// A temp pack carrying a loadable, app-stamped autosave at the second park,
/// so every Continue-autosave test starts from the same fixture.
fn pack_with_a_stamped_autosave() -> tempfile::TempDir {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space);
    let runtime = app.world().resource::<Runtime>();
    crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
    pack
}

/// An unreadable autosave disables Continue with the engine's own
/// `SlotInfo::Unreadable { reason }` string, verbatim — one source of truth with
/// the overlay's card.
#[test]
fn continue_disables_with_the_engines_unreadable_reason_verbatim() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let path = {
        let loaded = load_pack(&pack_root).expect("pack loads");
        save::save_path_for(&loaded.pack, Slot::Autosave)
    };
    std::fs::write(&path, b"not a save").unwrap();

    let app = boot_app(pack_root, None);
    let state = menu(&app);
    let info = state
        .slots
        .get(menu::slot_index(Slot::Autosave))
        .expect("the autosave is listed");
    let SlotStatus::Unreadable { reason } = &info.status else {
        panic!("expected an unreadable autosave, got {:?}", info.status);
    };
    assert!(
        !state.continue_enabled,
        "an unreadable autosave disables Continue"
    );
    assert_eq!(
        &state.continue_line, reason,
        "the engine's reason, verbatim"
    );
    assert_eq!(
        &menu::card_model(info, MenuMode::Load).detail,
        reason,
        "identical to the overlay's card"
    );
    assert_eq!(
        state.focus,
        MenuFocus::TitleNewGame,
        "focus falls to New Game"
    );
}

/// A live session has no `saved_at`, so the line is the runner's current scene
/// id alone — even when a loadable autosave sits behind it.
#[test]
fn continue_line_names_the_live_sessions_scene_alone() {
    let mut app = temp_title_app();
    enter_play(&mut app);
    {
        let runtime = app.world().resource::<Runtime>();
        crate::slots::write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave).unwrap();
    }
    reenter_title(&mut app);

    let scene = app
        .world()
        .resource::<Runtime>()
        .runner
        .scene_id()
        .to_string();
    assert!(menu(&app).continue_enabled);
    assert_eq!(
        menu(&app).continue_line,
        scene,
        "a live session names the scene id alone"
    );
    assert!(
        !menu(&app).continue_line.contains('\u{00b7}'),
        "a live session has no saved_at, so there is no separator"
    );
}

/// A loadable autosave renders the scene id plus the app-formatted local time
/// from `SlotInfo::Ok`, matching the overlay's card; `saved_at: None` renders
/// the dash rather than a fabricated time.
#[test]
fn continue_line_appends_the_autosaves_formatted_time_and_a_dash_when_unstamped() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let (scene_id, captured) = {
        let mut app = app_for_pack(pack_root.clone());
        press_and_update(&mut app, KeyCode::Space);
        let runtime = app.world().resource::<Runtime>();
        (
            runtime.runner.scene_id().to_string(),
            SaveFile::capture(&runtime.runner, &runtime.staging),
        )
    };

    // The engine's `capture` never stamps, so this autosave has no `saved_at`.
    {
        let loaded = load_pack(&pack_root).expect("pack loads");
        save::write_slot(&loaded.pack, Slot::Autosave, &captured).unwrap();
    }
    let app = boot_app(pack_root.clone(), None);
    assert_eq!(
        menu(&app).continue_line,
        format!("{scene_id} \u{00b7} \u{2014}"),
        "no saved_at renders the dash"
    );
    assert!(menu(&app).continue_enabled);

    let stamped_at = 1_760_000_000;
    {
        let loaded = load_pack(&pack_root).expect("pack loads");
        let mut stamped = captured.clone();
        stamped.saved_at = Some(stamped_at);
        save::write_slot(&loaded.pack, Slot::Autosave, &stamped).unwrap();
    }
    let app = boot_app(pack_root, None);
    let formatted = menu::format_saved_at(Some(stamped_at));
    assert_eq!(
        menu(&app).continue_line,
        format!("{scene_id} \u{00b7} {formatted}"),
        "the scene id plus the app-formatted local time"
    );
    let info = &menu(&app).slots[menu::slot_index(Slot::Autosave)];
    assert_eq!(
        menu::card_model(info, MenuMode::Load).saved_at,
        formatted,
        "Continue and the card format the time identically"
    );
}

/// Continue's autosave path onto a decision-wait re-fires the `decide` line.
#[test]
fn continue_onto_a_decision_wait_refires_the_decide_line() {
    let dir = tempfile::tempdir().unwrap();
    let pack_root = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &pack_root);

    // Park on the decision in a throwaway run and capture the autosave there.
    {
        let mut app = app_with_decisions_at(pack_root.clone(), None);
        {
            let mut runtime = app.world_mut().resource_mut::<Runtime>();
            drive_runner_to_decision(&mut runtime);
        }
        let runtime = app.world().resource::<Runtime>();
        let save = SaveFile::capture(&runtime.runner, &runtime.staging);
        save::write_slot(runtime.runner.pack(), Slot::Autosave, &save).unwrap();
    }

    let fake = FakeResolver::new(model_answer("warm"));
    let mut app = boot_app(pack_root, Some(Arc::new(fake.clone())));
    assert!(
        menu(&app).continue_enabled,
        "the decision-wait autosave enables Continue"
    );
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(!menu(&app).open, "Continue entered play");
    assert!(live_session(&app));
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));
    pump_until(&mut app, |_| fake.calls() == 1);
    assert_eq!(fake.calls(), 1, "the restored decision re-fired");
}

/// Slot metadata is read once per title entry: a slot written behind the app
/// after the title is up does not change the line until the title is re-entered.
#[test]
fn the_title_reads_slot_metadata_once_per_entry() {
    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    let mut app = boot_app(pack_root.clone(), None);
    assert_eq!(menu(&app).continue_line, "No autosave yet");
    assert!(!menu(&app).continue_enabled);

    // A loadable autosave appears behind the app after the title is up.
    let mut save = {
        let loaded = load_pack(&pack_root).expect("pack loads");
        let mut runner = ScriptRunner::new(loaded.pack);
        runner.run().unwrap();
        SaveFile::capture(&runner, &StagingState::default())
    };
    save.saved_at = Some(1_760_000_000);
    {
        let loaded = load_pack(&pack_root).expect("pack loads");
        save::write_slot(&loaded.pack, Slot::Autosave, &save).unwrap();
    }

    app.update();
    assert_eq!(
        menu(&app).continue_line,
        "No autosave yet",
        "the line is frozen until the title is re-entered"
    );
    assert!(!menu(&app).continue_enabled);

    reenter_title(&mut app);
    assert!(
        menu(&app).continue_enabled,
        "re-entry re-reads the metadata"
    );
    assert!(
        menu(&app).continue_line.starts_with(&save.scene_id),
        "the re-read names the new autosave: {:?}",
        menu(&app).continue_line
    );
}

/// Continue rewrites nothing: a load writes no autosave, and nothing
/// outstanding after it does either.
#[test]
fn continue_writes_no_autosave() {
    let pack = pack_with_a_stamped_autosave();
    let mut app = boot_app(pack.path().join("pack"), None);
    assert!(menu(&app).continue_enabled);
    let before = std::fs::read(autosave_path(&app)).unwrap();

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue entered play");
    assert!(live_session(&app), "Continue sets the live-session flag");
    assert!(
        !app.world().resource::<crate::slots::PendingAutosave>().0,
        "a load arms no autosave write"
    );
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        before,
        "Continue rewrites nothing"
    );
    app.update();
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        before,
        "nothing outstanding writes no autosave"
    );
}

/// A disabled Continue (nothing outstanding) is a no-op that writes nothing.
#[test]
fn a_disabled_continue_is_a_no_op_that_writes_nothing() {
    let mut app = temp_title_app();
    let before = parked_position(&app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(menu(&app).title_up(), "a disabled Continue is a no-op");
    assert_eq!(parked_position(&app), before, "position is untouched");
    assert!(
        !autosave_path(&app).exists(),
        "nothing outstanding writes no autosave"
    );
}

/// No key state gates play: with no key, a malformed key, a present key or the
/// environment key, Continue stays enabled and enters play, and New Game still
/// starts play.
#[test]
fn no_key_state_gates_continue_or_new_game() {
    let pack = pack_with_a_stamped_autosave();
    let pack_root = pack.path().join("pack");
    let rows = [
        KeyRow::NotSet,
        KeyRow::Invalid,
        KeyRow::Set {
            fingerprint: "sk-nano-\u{2026}a1b2".to_string(),
        },
        KeyRow::FromEnvironment {
            fingerprint: "sk-nano-\u{2026}env".to_string(),
        },
    ];

    for row in &rows {
        let mut app = boot_app(pack_root.clone(), None);
        app.insert_resource(CredentialsState {
            config_dir: None,
            path: None,
            row: row.clone(),
        });
        assert!(
            menu(&app).continue_enabled,
            "Continue stays enabled with {row:?}"
        );
        app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
        press_and_update(&mut app, KeyCode::Enter);
        assert!(!menu(&app).open, "Continue is not gated by {row:?}");
        assert!(live_session(&app), "{row:?} does not block play");

        let mut app = boot_app(pack_root.clone(), None);
        app.insert_resource(CredentialsState {
            config_dir: None,
            path: None,
            row: row.clone(),
        });
        app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
        press_and_update(&mut app, KeyCode::Enter);
        assert!(!menu(&app).open, "New Game is not gated by {row:?}");
        assert!(live_session(&app), "{row:?} does not block New Game");
    }
}

/// The app's one `saved_at` formatter: `DD Mon YYYY HH:MM`, or the dash when
/// unstamped. The concrete day/hour depend on the machine's timezone, so this
/// pins the shape; Continue and the overlay's card share the function.
#[test]
fn format_saved_at_pins_the_app_time_shape() {
    assert_eq!(menu::format_saved_at(None), "\u{2014}");
    let formatted = menu::format_saved_at(Some(1_760_000_000));
    assert_eq!(formatted.len(), 17, "DD Mon YYYY HH:MM: {formatted:?}");
    assert_eq!(
        &formatted[2..3],
        " ",
        "a space after the day: {formatted:?}"
    );
    assert_eq!(
        &formatted[6..7],
        " ",
        "a space after the month: {formatted:?}"
    );
    assert_eq!(
        &formatted[14..15],
        ":",
        "a colon in the time: {formatted:?}"
    );
}

#[test]
fn exit_to_title_renders_from_the_overlay_rail_with_its_confirm_copy() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    enter_play(&mut app);
    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).settings_origin, menu::MenuOrigin::Slots);

    build_menu_ui(&mut app);
    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == "Exit to title"),
        "the overlay rail renders Exit to title: {texts:?}"
    );

    click(&mut app, MenuAction::ExitToTitle);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ExitToTitle));
    build_menu_ui(&mut app);
    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == "Exit to the title?"),
        "the exit confirm title renders: {texts:?}"
    );
    assert!(texts.iter().any(
        |text| text == "Your session is kept \u{2014} Continue resumes it where you left off."
    ));
    assert!(texts.iter().any(|text| text == "Exit"), "the accept button");
    assert!(
        texts.iter().any(|text| text == "Cancel"),
        "the cancel button"
    );
    assert!(
        !texts
            .iter()
            .any(|text| text.contains("Unsaved progress is lost")),
        "Exit never renders the Load card's sentence: {texts:?}"
    );
}

#[test]
fn cancelling_exit_to_title_changes_nothing() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    set_text_speed(&mut app, 0.04);
    render_dialogue(&mut app);
    advance_time(&mut app, 0.5);
    app.world_mut().resource_mut::<Notice>().show("stale");

    let position = parked_position(&app);
    let chars = app.world().resource::<TextReveal>().chars_shown(0.04);
    assert!(chars > 0, "the line is mid-reveal");

    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    click(&mut app, MenuAction::ExitToTitle);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ExitToTitle));
    click(&mut app, MenuAction::ConfirmCancel);

    assert_eq!(menu(&app).confirm, None, "cancel drops the confirm");
    assert_eq!(
        menu(&app).view,
        MenuView::Settings,
        "cancel stays in Settings"
    );
    assert_eq!(
        parked_position(&app),
        position,
        "cancel leaves position alone"
    );
    assert_eq!(
        app.world().resource::<TextReveal>().chars_shown(0.04),
        chars,
        "cancel leaves the reveal clock alone"
    );
    assert!(app.world().resource::<Notice>().is_showing());
    assert!(!autosave_path(&app).exists(), "cancel writes nothing");
}

#[test]
fn exit_to_title_then_continue_applies_a_held_answer_without_a_second_decide() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    assert_eq!(fake.calls(), 1);

    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(menu(&app).title_up(), "Exit to title shows the title");
    assert!(live_session(&app), "the session is kept");

    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app), "the answer is held while the title is up");
    assert!(!autosave_path(&app).exists(), "held means no write yet");

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue resumed the live session");
    assert!(parked_on_say(&app), "the held answer landed on resume");
    assert_eq!(fake.calls(), 1, "resuming fires no second decide call");

    let runtime = app.world().resource::<Runtime>();
    let save = save::read_slot(runtime.runner.pack(), Slot::Autosave).expect("autosaved once");
    assert_eq!(save.wait, runtime.runner.wait().cloned());
    let before = std::fs::read(autosave_path(&app)).unwrap();
    app.update();
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        before,
        "a held model answer writes the autosave exactly once"
    );
}

#[test]
fn a_held_fallback_answer_across_the_title_writes_no_autosave() {
    let fake = FakeResolver::new(fallback_answer("cold"));
    let (mut app, _dir) = app_parked_on_decision(&fake);

    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(menu(&app).title_up());

    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app), "the fallback is held while the title is up");
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);

    assert!(parked_on_say(&app), "the held fallback landed on resume");
    assert!(
        !autosave_path(&app).exists(),
        "a held fallback writes no autosave"
    );
}

#[test]
fn new_game_with_a_live_session_renders_its_confirm_and_cancel_changes_nothing() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    enter_play(&mut app);
    press_and_update(&mut app, KeyCode::Space);
    reenter_title(&mut app);
    let position = parked_position(&app);

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::NewGame));

    build_menu_ui(&mut app);
    let texts = all_texts(&app);
    assert!(texts.iter().any(|text| text == "Start a new game?"));
    assert!(texts.iter().any(|text| {
        text
        == "The session in progress is discarded. Your saved slots and the autosave are untouched."
    }));
    assert!(texts.iter().any(|text| text == "Start"));
    assert!(texts.iter().any(|text| text == "Cancel"));

    click(&mut app, MenuAction::ConfirmCancel);
    assert_eq!(menu(&app).confirm, None);
    assert!(menu(&app).title_up(), "cancel stays on the title");
    assert!(live_session(&app), "cancel keeps the live session");
    assert_eq!(parked_position(&app), position, "cancel changes nothing");
}

#[test]
fn quit_with_a_live_session_renders_its_confirm_and_cancel_changes_nothing() {
    let mut app = temp_title_app();
    app.init_resource::<crate::presentation::MenuBuilt>();
    enter_play(&mut app);
    reenter_title(&mut app);

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleQuit;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::Quit),
        "a live session confirms once"
    );

    build_menu_ui(&mut app);
    let texts = all_texts(&app);
    assert!(texts.iter().any(|text| text == "Quit?"));
    assert!(texts
        .iter()
        .any(|text| text == "Unsaved progress is lost and any decision in flight is cancelled."));
    assert!(texts.iter().any(|text| text == "Quit"));
    assert!(texts.iter().any(|text| text == "Cancel"));

    click(&mut app, MenuAction::ConfirmCancel);
    assert_eq!(menu(&app).confirm, None);
    assert!(menu(&app).title_up(), "cancel stays on the title");
    assert!(live_session(&app), "cancel keeps the live session");
    assert!(app.should_exit().is_none(), "cancel does not exit");
}

#[test]
fn quitting_with_a_held_answer_writes_no_autosave() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    reenter_title(&mut app);

    fake.release();
    pump_until(&mut app, held);
    assert!(held(&app), "the answer is held while the title is up");

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleQuit;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::Quit));
    click(&mut app, MenuAction::ConfirmAccept);

    assert!(matches!(app.should_exit(), Some(AppExit::Success)));
    assert!(
        held(&app),
        "quitting drops rather than applies the held answer"
    );
    assert!(
        !autosave_path(&app).exists(),
        "quitting with a held answer writes no autosave"
    );
}

/// A core app over the demo pack with the presentation's audio rig attached, so
/// the real menu actions can drive `sync_audio` end to end.
fn core_audio_app() -> App {
    let mut app = boot_app(demo_pack_root(), None);
    app.add_plugins(AssetPlugin {
        file_path: demo_pack_root()
            .join("assets")
            .to_string_lossy()
            .into_owned(),
        ..default()
    })
    .init_asset::<Image>()
    .init_asset::<bevy_kira_audio::AudioSource>()
    .init_resource::<AssetWatch>()
    .insert_resource(AudioChannel::<Bgm>::default());
    app
}

#[test]
fn bgm_stops_on_exit_to_title_and_restarts_on_continue() {
    let mut app = core_audio_app();
    enter_play(&mut app);
    // In play, the entry scene's track sounds.
    app.world_mut()
        .run_system_once(sync_audio)
        .expect("sync_audio runs");
    assert_eq!(
        app.world().resource::<AppliedAudio>().current_bgm(),
        Some("music_morning")
    );

    // The real `Exit to title`: overlay Settings rail, confirm, accept.
    esc(&mut app);
    click(&mut app, MenuAction::OpenSettings);
    click(&mut app, MenuAction::ExitToTitle);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(menu(&app).title_up());
    app.world_mut()
        .run_system_once(sync_audio)
        .expect("sync_audio runs");
    assert!(
        app.world()
            .resource::<AppliedAudio>()
            .current_bgm()
            .is_none(),
        "Exit to title stops the music"
    );

    // The real `Continue` (live session): the title closes and the track replays.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleContinue;
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Continue entered play");
    app.world_mut()
        .run_system_once(sync_audio)
        .expect("sync_audio runs");
    assert_eq!(
        app.world().resource::<AppliedAudio>().current_bgm(),
        Some("music_morning"),
        "Continue restarts the music"
    );
}
