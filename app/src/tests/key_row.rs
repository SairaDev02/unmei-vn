use super::*;

/// The Decisions group's key row state line, if the group is built.
fn key_row_text(app: &App) -> Option<String> {
    app.world().iter_entities().find_map(|entity| {
        entity.get::<crate::presentation::settings_view::KeyRowText>()?;
        entity.get::<Text>().map(|text| text.0.clone())
    })
}

/// The credential records captured under the failure target.
fn credential_lines() -> Vec<(log::Level, String, String)> {
    captures::take()
        .into_iter()
        .filter(|(_, target, _)| target == crate::failure::LOG_TARGET)
        .collect()
}

#[test]
fn the_decisions_group_renders_last_with_the_read_only_key_row_and_path_line() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let key = format!("sk-nano-{}", "a".repeat(60));
    let fingerprint = credentials::fingerprint(&key);

    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);
    app.init_resource::<crate::presentation::MenuBuilt>();
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::Set {
            fingerprint: fingerprint.clone(),
        },
    });
    build_menu_ui(&mut app);

    let texts = all_texts(&app);
    for group in ["Audio", "Play", "Decisions"] {
        assert!(texts.iter().any(|text| text == group), "missing {group}");
    }
    let expected = format!("Set \u{2014} {fingerprint}");
    assert_eq!(
        key_row_text(&app).as_deref(),
        Some(expected.as_str()),
        "{texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text == &format!("Stored in {}", path.display())),
        "the path line renders: {texts:?}"
    );
    assert!(
        texts.iter().all(|text| !text.contains(&key)),
        "the raw key is never rendered"
    );

    let mut titled = temp_title_app();
    titled.init_resource::<crate::presentation::MenuBuilt>();
    titled.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut titled, KeyCode::Enter);
    titled.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::Set { fingerprint },
    });
    build_menu_ui(&mut titled);
    assert!(
        all_texts(&titled).iter().any(|text| text == "Decisions"),
        "the Decisions group is shared by both doors"
    );
    let titled_expected = format!("Set \u{2014} sk-nano-\u{2026}{}", "a".repeat(4));
    assert_eq!(
        key_row_text(&titled).as_deref(),
        Some(titled_expected.as_str())
    );
}

#[test]
fn the_key_row_renders_the_environment_state_with_the_env_note() {
    let pack = temp_pack();
    let env = "sk-nano-0123456789abcdef";
    let mut app = app_for_pack(pack.path().join("pack"));
    app.init_resource::<crate::presentation::MenuBuilt>();
    app.insert_resource(CredentialsState {
        config_dir: None,
        path: None,
        row: KeyRow::FromEnvironment {
            fingerprint: credentials::fingerprint(env),
        },
    });
    open_settings(&mut app);
    build_menu_ui(&mut app);

    let text = key_row_text(&app).expect("the key row renders");
    assert!(
        text.starts_with(&format!(
            "From environment \u{2014} {}",
            credentials::fingerprint(env)
        )),
        "{text}"
    );
    assert!(text.contains(credentials::ENV_NOTE), "{text}");
    assert!(
        all_texts(&app)
            .iter()
            .any(|text| text == "Stored in \u{2014}"),
        "no derivable directory names a dash"
    );
}

#[test]
fn the_key_row_renders_the_invalid_state_for_a_shape_failure() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);
    app.init_resource::<crate::presentation::MenuBuilt>();
    app.insert_resource(CredentialsState {
        config_dir: None,
        path: None,
        row: KeyRow::Invalid,
    });
    build_menu_ui(&mut app);

    assert_eq!(
        key_row_text(&app).as_deref(),
        Some("Invalid \u{2014} does not look like a key")
    );
}

#[test]
fn a_default_credentials_resource_renders_not_set_without_a_path() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.init_resource::<crate::presentation::MenuBuilt>();
    open_settings(&mut app);
    build_menu_ui(&mut app);

    assert_eq!(key_row_text(&app).as_deref(), Some("Not set"));
    assert!(
        all_texts(&app)
            .iter()
            .any(|text| text == "Stored in \u{2014}"),
        "a host with no preloaded read renders a dash, not a path"
    );
}

#[test]
fn the_decisions_group_is_the_last_group_and_the_rail_control_follows_it() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_settings(&mut app);

    focus_down(&mut app, 8); // Master -> PlayReset, the play group's last control
    assert_eq!(menu(&app).focus, MenuFocus::PlayReset);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::KeyRow,
        "the Decisions group follows the play group"
    );
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::CopyKeyPath);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::ClearKey,
        "the Decisions group's last control is the last group's last stop"
    );
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::ExitToTitle,
        "the rail's session-scoped Exit to title is the walk's one stop past it"
    );
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::AudioMaster,
        "the walk wraps to the first control"
    );
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::ExitToTitle);
}

/// The Decisions group is the last walk stop from the title door too
/// (`docs/title-and-menu.md` § Decisions settings §6).
#[test]
fn the_decisions_group_is_the_last_walk_stop_from_the_title_door() {
    let mut app = temp_title_app();
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Settings);
    assert_eq!(menu(&app).settings_origin, menu::MenuOrigin::Title);

    focus_down(&mut app, 11); // AudioMaster -> ClearKey
    assert_eq!(
        menu(&app).focus,
        MenuFocus::ClearKey,
        "the Decisions group is the title door's last stop"
    );
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::AudioMaster,
        "the walk wraps to the first control"
    );
}

/// One Settings door for the headless scroll regressions. Both doors mount the
/// same body inside a bounded `SettingsBody` viewport; only the surrounding
/// chrome differs.
#[derive(Clone, Copy, Debug)]
enum SettingsDoor {
    Title,
    Overlay,
}

impl SettingsDoor {
    /// The door's app with its Settings view open and its tree built. The temp
    /// pack is kept alive by a `TempPack` resource.
    fn app(self) -> App {
        let dir = temp_pack();
        let pack_root = dir.path().join("pack");
        let mut app = match self {
            SettingsDoor::Title => boot_app(pack_root, None),
            SettingsDoor::Overlay => app_for_pack(pack_root),
        };
        app.insert_resource(TempPack(dir));
        match self {
            SettingsDoor::Title => {
                app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
                press_and_update(&mut app, KeyCode::Enter);
            }
            SettingsDoor::Overlay => open_settings(&mut app),
        }
        assert_eq!(menu(&app).view, MenuView::Settings, "{self:?} door open");
        build_pause_tree(&mut app);
        app
    }
}

/// A synthetic Settings frame for the headless scroll tests: a bounded viewport
/// with the twelve walk stops — and the door-aware back control, when the door
/// draws one — laid out a row at a time, standing in for the layout pass a core
/// app does not run. The content keeps its natural height, a row step per row
/// drawn, so the body has a real scroll range.
const SETTINGS_VIEWPORT: Vec2 = Vec2::new(680.0, 300.0);

const SETTINGS_ROW: Vec2 = Vec2::new(440.0, 40.0);

const SETTINGS_ROW_STEP: f32 = 60.0;

/// Lay the built Settings body and its controls out with the synthetic frame.
/// The contents page hides the tree's way out and collapses its row, so the
/// overlay's frame has one row fewer than the title's.
fn rig_settings_frame(
    mut commands: Commands,
    mut bodies: Query<
        (Entity, &mut ComputedNode),
        With<crate::presentation::settings_view::SettingsBody>,
    >,
    mut controls: Query<
        (Entity, &MenuAction, &Node, &mut ComputedNode),
        Without<crate::presentation::settings_view::SettingsBody>,
    >,
) {
    let order: Vec<MenuAction> = crate::menu::SETTINGS_ORDER
        .iter()
        .map(|focus| crate::menu::action_for_focus(*focus))
        .collect();
    let mut rows = order.len();
    for (entity, action, node, mut computed) in &mut controls {
        let index = if *action == MenuAction::BackToSlots {
            if node.display == Display::None {
                continue;
            }
            rows += 1;
            order.len()
        } else {
            match order.iter().position(|candidate| *candidate == *action) {
                Some(index) => index,
                None => continue,
            }
        };
        computed.size = SETTINGS_ROW;
        let y = index as f32 * SETTINGS_ROW_STEP;
        commands
            .entity(entity)
            .insert(UiGlobalTransform::from_translation(Vec2::new(
                SETTINGS_ROW.x / 2.0,
                y + SETTINGS_ROW.y / 2.0,
            )));
    }
    for (entity, mut node) in &mut bodies {
        node.size = SETTINGS_VIEWPORT;
        node.content_size = Vec2::new(SETTINGS_VIEWPORT.x, rows as f32 * SETTINGS_ROW_STEP);
        commands
            .entity(entity)
            .insert(UiGlobalTransform::from_translation(SETTINGS_VIEWPORT / 2.0));
    }
}

/// Register the real scroll systems and the synthetic frame over a built door.
fn attach_settings_scroll_rig(app: &mut App) {
    app.add_plugins(bevy::ui_widgets::ScrollAreaPlugin)
        .add_message::<bevy::input::mouse::MouseWheel>()
        .add_systems(
            Update,
            (
                rig_settings_frame,
                crate::presentation::settings_view::scroll_settings_focus,
                crate::presentation::settings_view::scroll_settings_wheel,
            )
                .chain()
                .after(RuntimeCoreSet),
        );
    app.world_mut().spawn((Window::default(), PrimaryWindow));
}

fn settings_body(app: &App) -> Entity {
    entity_with::<crate::presentation::settings_view::SettingsBody>(app)
}

fn children_of(app: &App, entity: Entity) -> Vec<Entity> {
    app.world()
        .entity(entity)
        .get::<Children>()
        .map(|children| children.iter().collect())
        .unwrap_or_default()
}

/// The synthetic frame's scroll range: the rows the door draws past the
/// viewport.
fn settings_max_scroll(app: &App) -> f32 {
    let content = app
        .world()
        .get::<ComputedNode>(settings_body(app))
        .expect("the rig sized the body")
        .content_size;
    (content.y - SETTINGS_VIEWPORT.y).max(0.0)
}

fn settings_scroll(app: &App) -> f32 {
    app.world()
        .get::<ScrollPosition>(settings_body(app))
        .expect("the Settings body carries a ScrollPosition")
        .y
}

/// The focused control's top and bottom edges in the synthetic frame.
fn settings_control_extent(app: &App, action: MenuAction) -> (f32, f32) {
    let entity = action_entity(app, action);
    let node = app.world().get::<ComputedNode>(entity).unwrap();
    let transform = app.world().get::<UiGlobalTransform>(entity).unwrap();
    let bottom = transform.translation.y + node.size.y / 2.0;
    (bottom - node.size.y, bottom)
}

fn set_settings_cursor(app: &mut App, position: Vec2) {
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>();
    query
        .single_mut(app.world_mut())
        .expect("the Settings window")
        .set_cursor_position(Some(position));
}

fn send_settings_wheel(app: &mut App, y: f32) {
    app.world_mut()
        .write_message(bevy::input::mouse::MouseWheel {
            unit: bevy::input::mouse::MouseScrollUnit::Line,
            x: 0.0,
            y,
            window: Entity::PLACEHOLDER,
            phase: bevy::input::touch::TouchPhase::Moved,
        });
}

/// Both Settings doors mount a bounded, scrollable viewport with the groups at
/// their natural height, so the lower controls the 1280×720 clipping hid keep
/// their reach (`docs/title-and-menu.md` § Title layout §5). The viewport's one
/// child is the plugin's tree root, at the height its own content needs: a root
/// that filled the viewport would leave the body nothing to scroll.
#[test]
fn both_settings_doors_have_a_bounded_scroll_body() {
    for door in [SettingsDoor::Title, SettingsDoor::Overlay] {
        let mut app = door.app();
        attach_settings_scroll_rig(&mut app);
        app.update();
        let body = settings_body(&app);
        assert!(
            app.world()
                .get::<bevy::ui_widgets::ScrollArea>(body)
                .is_some(),
            "{door:?}: the body is a Bevy scroll area"
        );
        let node = app.world().get::<Node>(body).unwrap();
        assert_eq!(
            node.overflow.y,
            bevy::ui::OverflowAxis::Scroll,
            "{door:?}: the body scrolls vertically"
        );
        // A bounded viewport with the automatic minimum size released, so the
        // content keeps its natural height and overflows into a real scroll
        // range instead of running past the window edge.
        assert_eq!(
            node.min_height,
            bevy::ui::Val::Px(0.0),
            "{door:?}: shrinkable"
        );
        assert!(
            node.max_height != bevy::ui::Val::Auto || node.flex_grow >= 1.0,
            "{door:?}: the viewport is bounded"
        );
        let held = children_of(&app, body);
        assert_eq!(
            held.len(),
            1,
            "{door:?}: the body holds the surface's tree root"
        );
        assert_eq!(
            app.world().get::<Node>(held[0]).unwrap().height,
            bevy::ui::Val::Auto,
            "{door:?}: the tree keeps its natural height"
        );
    }
}

/// A wheel over either Settings body changes what is visible, and reaching the
/// bottom brings the last walk stop into the viewport; the title's door reveals
/// its own back control there too, while the contents page's body has none — its
/// way out is the page's left column, so the row around the tree's back control
/// collapses. A wheel off the body is ignored (`docs/title-and-menu.md` § Title
/// layout §5/§7).
#[test]
fn the_settings_wheel_scrolls_the_lower_controls_and_back_control_into_view() {
    for door in [SettingsDoor::Title, SettingsDoor::Overlay] {
        let mut app = door.app();
        attach_settings_scroll_rig(&mut app);
        app.update();
        assert_eq!(settings_scroll(&app), 0.0, "{door:?}: starts at the top");

        set_settings_cursor(&mut app, SETTINGS_VIEWPORT + Vec2::splat(50.0));
        send_settings_wheel(&mut app, -1.0);
        app.update();
        assert_eq!(
            settings_scroll(&app),
            0.0,
            "{door:?}: a wheel off the body does not scroll it"
        );

        // A wheel over the body moves it by the widget's line convention.
        set_settings_cursor(&mut app, SETTINGS_VIEWPORT / 2.0);
        send_settings_wheel(&mut app, -1.0);
        app.update();
        assert_eq!(
            settings_scroll(&app),
            bevy::input::mouse::MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR,
            "{door:?}: the wheel moved the panel one line"
        );

        for _ in 0..=crate::menu::SETTINGS_ORDER.len() {
            send_settings_wheel(&mut app, -1.0);
            app.update();
        }
        let scroll = settings_scroll(&app);
        assert_eq!(
            scroll,
            settings_max_scroll(&app),
            "{door:?}: the wheel clamps at the last group"
        );
        let (clear_top, clear_bottom) = settings_control_extent(&app, MenuAction::ClearKey);
        assert!(
            clear_top >= scroll && clear_bottom <= scroll + SETTINGS_VIEWPORT.y,
            "{door:?}: the last walk stop is fully inside the viewport"
        );

        let back = action_entity(&app, MenuAction::BackToSlots);
        let drawn = app
            .world()
            .get::<Node>(back)
            .is_some_and(|node| node.display != Display::None);
        assert_eq!(
            drawn,
            matches!(door, SettingsDoor::Title),
            "{door:?}: only the title's door draws the back control"
        );
    }
}

/// An open confirmation freezes the Settings panel, exactly as it freezes the
/// rest of the menu: a wheel over the backdrop never scrolls the content
/// underneath (`docs/runtime-hardening.md` § Menu overlay §6).
#[test]
fn the_settings_wheel_is_frozen_under_an_open_confirmation() {
    for door in [SettingsDoor::Title, SettingsDoor::Overlay] {
        let mut app = door.app();
        attach_settings_scroll_rig(&mut app);
        app.update();

        // Raise a confirmation from a mid-panel control. The structural rebuild
        // respawns the body at the top.
        app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::AudioReset;
        press_and_update(&mut app, KeyCode::Enter);
        assert_eq!(
            menu(&app).confirm,
            Some(ConfirmKind::ResetAudio),
            "{door:?}: the confirmation is up"
        );
        build_pause_tree(&mut app);
        app.update();
        assert_eq!(settings_scroll(&app), 0.0, "{door:?}: rebuilt at the top");

        set_settings_cursor(&mut app, SETTINGS_VIEWPORT / 2.0);
        send_settings_wheel(&mut app, -1.0);
        app.update();
        assert_eq!(
            settings_scroll(&app),
            0.0,
            "{door:?}: the confirmation freezes the panel"
        );
    }
}

/// Walking the Settings focus to every control, including the last stop
/// `Clear key`, scrolls the focused control fully into view on both doors
/// (`docs/title-and-menu.md` § Decisions settings §6).
#[test]
fn the_settings_walk_reveals_every_focused_control() {
    for door in [SettingsDoor::Title, SettingsDoor::Overlay] {
        let mut app = door.app();
        attach_settings_scroll_rig(&mut app);
        app.update();
        assert_eq!(
            menu(&app).focus,
            MenuFocus::AudioMaster,
            "{door:?}: first stop"
        );

        let stops = crate::menu::SETTINGS_ORDER.len();
        for step in 0..stops {
            let focus = menu(&app).focus;
            let action = crate::menu::action_for_focus(focus);
            let (top, bottom) = settings_control_extent(&app, action);
            let scroll = settings_scroll(&app);
            assert!(
                top >= scroll - 0.5 && bottom <= scroll + SETTINGS_VIEWPORT.y + 0.5,
                "{door:?}: {focus:?} is fully visible (top {top}, bottom {bottom}, scroll {scroll})"
            );
            if step + 1 < stops {
                press_and_update(&mut app, KeyCode::ArrowDown);
            }
        }
        assert_eq!(
            menu(&app).focus,
            MenuFocus::ClearKey,
            "{door:?}: the walk ends on the last stop"
        );
        assert!(
            settings_scroll(&app) > 0.0,
            "{door:?}: reaching the lower controls scrolled the body"
        );
    }
}

#[test]
fn opening_the_settings_view_creates_no_credential_file() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(credentials::credentials_path_in(config_dir.path())),
        row: KeyRow::NotSet,
    });

    open_settings(&mut app);

    assert!(
        !credentials::credentials_path_in(config_dir.path()).exists(),
        "opening the view writes nothing"
    );
    assert!(
        !config_dir.path().join("credentials.tmp").exists(),
        "and leaves no temp sibling"
    );
}

#[test]
fn a_boot_without_preloaded_credentials_stays_not_set_and_silent() {
    captures::install();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<CredentialsState>()
        .init_resource::<Notice>()
        .init_resource::<DefensiveHold>();

    // Drive the startup system directly: no preload means no read at all.
    captures::take();
    app.world_mut()
        .run_system_once(credentials::setup_credentials)
        .unwrap();

    assert!(
        credential_lines().is_empty(),
        "the hermetic default logs no credential line"
    );
    let state = app.world().resource::<CredentialsState>();
    assert_eq!(state.row, KeyRow::NotSet);
    assert!(state.path.is_none(), "no directory is derived in tests");
    assert!(state.config_dir.is_none());
}

#[test]
fn a_shape_failure_logs_once_without_quoting_the_value_or_its_fingerprint() {
    captures::install();
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("unmei");
    std::fs::create_dir_all(&config_dir).unwrap();
    let value = "sk-nano-not-a-key-because of the space";
    std::fs::write(credentials::credentials_path_in(&config_dir), value).unwrap();

    let launch = credentials::resolve_launch(Some(&config_dir), None, |_| {
        panic!("a shape-invalid stored value must not build a resolver")
    });
    assert_eq!(launch.state.row, KeyRow::Invalid);
    assert!(launch.resolver.is_none());
    let warning = launch.warning.expect("a shape failure warns once");

    let pack = temp_pack();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(pack.path().join("pack"), None).with_dev_mode(true))
        .insert_resource(credentials::StartupCredentials {
            state: launch.state,
            warning: Some(warning),
        });
    app.add_plugins(RuntimeCorePlugin);

    // Drive the startup system on this thread, so the line is captured rather
    // than emitted on a Bevy executor worker thread.
    captures::take();
    app.world_mut()
        .run_system_once(credentials::setup_credentials)
        .unwrap();

    let records = credential_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(!records[0].2.contains(value), "{:?}", records[0].2);
    assert!(
        !records[0].2.contains(&credentials::fingerprint(value)),
        "{:?}",
        records[0].2
    );
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::Invalid,
        "the preloaded read lands on the resource"
    );
}

/// The field's current buffer.
fn field_text(app: &App) -> String {
    let entity = key_field_entity(app).expect("the key field is built");
    app.world()
        .entity(entity)
        .get::<EditableText>()
        .expect("the field is an EditableText")
        .value()
        .to_string()
}

/// The placeholder's current `Visibility`.
fn placeholder_visibility(app: &App) -> Option<Visibility> {
    app.world().iter_entities().find_map(|entity| {
        entity
            .get::<crate::presentation::settings_view::KeyPlaceholder>()
            .and_then(|_| entity.get::<Visibility>().copied())
    })
}

#[test]
fn the_key_field_starts_empty_hides_the_key_and_shows_the_placeholder_only_on_not_set() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let key = format!("sk-nano-{}", "a".repeat(60));
    credentials::write(config_dir.path(), &key).unwrap();

    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(credentials::credentials_path_in(config_dir.path())),
        row: KeyRow::NotSet,
    });
    open_settings_with_ui(&mut app);

    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::Set {
            fingerprint: credentials::fingerprint(&key)
        },
        "the open read lands on the stored key"
    );
    assert_eq!(
        field_text(&app),
        "",
        "the stored key never enters the field"
    );
    assert!(
        all_texts(&app).iter().all(|text| !text.contains(&key)),
        "the raw key is never rendered"
    );
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_key_row)
        .unwrap();
    assert_eq!(
        placeholder_visibility(&app),
        Some(Visibility::Hidden),
        "a Set row shows no placeholder"
    );

    app.world_mut().resource_mut::<CredentialsState>().row = KeyRow::NotSet;
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_key_row)
        .unwrap();
    assert_eq!(placeholder_visibility(&app), Some(Visibility::Inherited));
    assert!(
        all_texts(&app)
            .iter()
            .any(|text| text == credentials::KEY_PLACEHOLDER),
        "the placeholder copy is fixed"
    );
}

#[test]
fn the_from_environment_row_has_no_editable_field() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: None,
        path: None,
        row: KeyRow::FromEnvironment {
            fingerprint: "sk-nano-\u{2026}abcd".to_string(),
        },
    });
    open_settings_with_ui(&mut app);

    assert!(
        key_field_entity(&app).is_none(),
        "the environment row is read-only"
    );
}

#[test]
fn the_key_field_focus_model_walks_edits_commits_and_discards() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::NotSet,
    });
    open_settings_with_ui(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::KeyRow;

    press_and_update(&mut app, KeyCode::Enter);
    assert!(menu(&app).key_editing);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::KeyRow,
        "arrows do not walk while editing"
    );

    set_field_text(&mut app, "short");
    press_and_update(&mut app, KeyCode::Enter);
    assert!(
        !menu(&app).key_editing,
        "Enter commits and returns to the walk"
    );
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::Invalid
    );
    assert!(!path.exists(), "a shape failure writes nothing");
    assert_eq!(field_text(&app), "short", "the typed text is kept");

    // A valid key lands the file, shows `Checking…`, and becomes `Set`. Every
    // test world keeps the hermetic `None` key-check default, which trusts the
    // key without a network call.
    press_and_update(&mut app, KeyCode::Enter);
    assert!(menu(&app).key_editing);
    let key = format!("sk-nano-{}", "b".repeat(60));
    set_field_text(&mut app, &key);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).key_editing);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        key,
        "the key is on disk before the ping"
    );
    pump_until(&mut app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Set {
                fingerprint: credentials::fingerprint(&key),
            }
    });
    assert_eq!(
        field_text(&app),
        "",
        "a committed key never stays in the field"
    );

    press_and_update(&mut app, KeyCode::Enter);
    set_field_text(&mut app, "another-value-here");
    press_and_update(&mut app, KeyCode::Escape);
    assert!(!menu(&app).key_editing);
    assert_eq!(menu(&app).view, MenuView::Settings);
    assert_eq!(field_text(&app), "", "Esc discards the typed text");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), key);
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).view, MenuView::Slots, "the second Esc backs out");
}

#[test]
fn copy_path_writes_the_path_to_the_in_process_clipboard_and_returns_the_label() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::NotSet,
    });
    open_settings_with_ui(&mut app);

    click(&mut app, MenuAction::CopyKeyPath);
    assert_eq!(
        app.world().resource::<ClipboardPort>(),
        &ClipboardPort::Memory(Some(path.display().to_string())),
        "the path lands in the in-process buffer"
    );
    assert!(app.world().resource::<CopyPathLabel>().remaining > 0.0);

    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_copy_path_label)
        .unwrap();
    assert!(
        all_texts(&app).iter().any(|text| text == "Copied"),
        "the label flips to Copied"
    );

    app.world_mut().resource_mut::<CopyPathLabel>().remaining = 0.0;
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_copy_path_label)
        .unwrap();
    assert!(
        all_texts(&app).iter().any(|text| text == "Copy path"),
        "the label returns to Copy path"
    );
}

#[test]
fn clear_key_confirms_once_deletes_the_file_and_returns_the_resolver_to_none() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let key = format!("sk-nano-{}", "c".repeat(60));
    credentials::write(config_dir.path(), &key).unwrap();

    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::Set {
            fingerprint: credentials::fingerprint(&key),
        },
    });
    app.world_mut()
        .resource_mut::<crate::DecisionChannel>()
        .resolver = Some(Arc::new(FakeResolver::new(ResolvedDecision {
        action: "cold".to_string(),
        model: None,
        confidence: None,
        fallback_reason: None,
    })));

    open_settings_with_ui(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::ClearKey;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ClearKey));

    click(&mut app, MenuAction::ConfirmCancel);
    assert!(path.exists());
    assert!(app
        .world()
        .resource::<crate::DecisionChannel>()
        .resolver
        .is_some());

    // Confirming deletes the file and drops the resolver the play path reads.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::ClearKey;
    press_and_update(&mut app, KeyCode::Enter);
    click(&mut app, MenuAction::ConfirmAccept);
    assert!(!path.exists(), "the file is deleted");
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::NotSet
    );
    assert!(
        app.world()
            .resource::<crate::DecisionChannel>()
            .resolver
            .is_none(),
        "Clear returns the live resolver to None"
    );
}

#[test]
fn clear_key_is_disabled_while_the_environment_variable_wins() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let key = format!("sk-nano-{}", "d".repeat(60));
    credentials::write(config_dir.path(), &key).unwrap();

    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::FromEnvironment {
            fingerprint: credentials::fingerprint(&key),
        },
    });
    open_settings_with_ui(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::ClearKey;
    press_and_update(&mut app, KeyCode::Enter);

    assert_eq!(
        menu(&app).confirm,
        None,
        "Clear is a no-op under the env key"
    );
    assert!(path.exists(), "the stored bytes are untouched");
}

#[test]
fn opening_settings_picks_up_a_hand_edit_without_rewriting_the_file() {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let first = format!("sk-nano-{}", "e".repeat(60));
    credentials::write(config_dir.path(), &first).unwrap();
    let bytes = std::fs::read(&path).unwrap();

    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::NotSet,
    });

    open_settings(&mut app);
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::Set {
            fingerprint: credentials::fingerprint(&first)
        }
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "a read rewrites nothing"
    );

    // A hand-edit behind the app is picked up on the next open.
    let second = format!("sk-nano-{}", "f".repeat(60));
    std::fs::write(&path, &second).unwrap();
    open_settings(&mut app);
    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::Set {
            fingerprint: credentials::fingerprint(&second)
        },
        "the hand-edit is picked up on open"
    );
}

#[test]
fn the_editable_field_receives_keyboard_input_and_a_trimmed_in_process_paste() {
    use bevy::clipboard::ClipboardPlugin;
    use bevy::input::keyboard::{Key, KeyboardInput};
    use bevy::input::{ButtonState, InputPlugin};
    use bevy::input_focus::{FocusCause, InputDispatchPlugin, InputFocus, InputFocusPlugin};
    use bevy::picking::events::{Pointer, Release};
    use bevy::text::{apply_text_edits, EditableTextFilter, FontCx, LayoutCx};
    use bevy::ui::UiScale;
    use bevy::ui_widgets::EditableTextInputPlugin;
    use bevy::window::Ime;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(InputPlugin)
        .add_plugins(InputFocusPlugin)
        .add_plugins(InputDispatchPlugin)
        .add_plugins(ClipboardPlugin)
        .add_plugins(EditableTextInputPlugin)
        .init_resource::<FontCx>()
        .init_resource::<LayoutCx>()
        .init_resource::<UiScale>()
        .init_resource::<ClipboardPort>()
        .add_message::<Ime>()
        .add_message::<Pointer<Release>>()
        .add_systems(
            PostUpdate,
            crate::presentation::settings_view::trim_pasted_text
                .before(bevy::text::EditableTextSystems),
        )
        .add_systems(
            PostUpdate,
            apply_text_edits.in_set(bevy::text::EditableTextSystems),
        );

    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    let field = app
        .world_mut()
        .spawn((
            KeyField,
            EditableText::new(""),
            Node::default(),
            TextFont::default(),
            TextColor::default(),
            EditableTextFilter::new(|character| character != '\n' && character != '\r'),
        ))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(field, FocusCause::Navigated);

    // Typing 'a' through the real keyboard chain.
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::KeyA,
        logical_key: Key::Character("a".into()),
        state: ButtonState::Pressed,
        text: Some("a".into()),
        repeat: false,
        window,
    });
    app.update();
    assert_eq!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .value()
            .to_string(),
        "a"
    );

    // Ctrl+V through the in-process clipboard: the trailing newline is trimmed
    // before the all-or-nothing filter, so the paste is not dropped whole.
    *app.world_mut().resource_mut::<ClipboardPort>() =
        ClipboardPort::Memory(Some("sk-pasted\n".into()));
    app.world_mut()
        .resource_mut::<ButtonInput<Key>>()
        .press(Key::Control);
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::KeyV,
        logical_key: Key::Character("v".into()),
        state: ButtonState::Pressed,
        text: Some("v".into()),
        repeat: false,
        window,
    });
    app.update();
    assert_eq!(
        app.world()
            .entity(field)
            .get::<EditableText>()
            .unwrap()
            .value()
            .to_string(),
        "ask-pasted",
        "the paste is trimmed and appended, not dropped whole"
    );
}

#[test]
fn a_shape_failure_writes_nothing_and_never_pings() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    commit_key_at(&mut rig.app, "short");
    rig.app.update();

    assert_eq!(rig.checker.calls(), 0, "a shape failure never pings");
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Invalid
    );
    assert!(!rig.path.exists(), "nothing is written");
    assert_eq!(field_text(&rig.app), "short", "the typed text is kept");
    assert!(rig.keys.lock().unwrap().is_empty(), "no resolver is built");
}

/// A write that fails reports row 23, leaves the row and the buffer as they
/// were, and runs no ping (`docs/title-and-menu.md` § Decisions settings §4.2,
/// §6).
#[test]
fn a_failed_write_runs_no_ping_and_keeps_the_row_and_buffer() {
    captures::install();
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    force_single_threaded(&mut rig.app);
    // Block the atomic write: a directory where the temp sibling should go makes
    // `create_new` fail before any byte lands.
    std::fs::create_dir(rig.path.with_extension("tmp")).unwrap();

    let key = format!("sk-nano-{}", "w".repeat(60));
    captures::take();
    commit_key_at(&mut rig.app, &key);
    rig.app.update();

    assert_eq!(rig.checker.calls(), 0, "a failed write never pings");
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::NotSet,
        "the row is left as it was"
    );
    assert_eq!(
        field_text(&rig.app),
        key,
        "the buffer is kept so the commit can be retried"
    );
    assert!(!rig.path.exists(), "no credential file lands");
    assert!(rig.keys.lock().unwrap().is_empty(), "no resolver is built");

    let notice = rig.app.world().resource::<Notice>();
    assert!(notice.is_showing(), "a failed write notices");
    assert_eq!(notice.text.as_deref(), Some("Could not save the key"));

    let records = credential_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Error, "row 23 logs error!");
    assert!(
        records[0].2.starts_with("Could not save the key"),
        "{records:?}"
    );
    assert!(!records[0].2.contains(&key), "the value is never logged");
}

#[test]
fn a_committed_key_is_on_disk_before_the_ping_and_shows_accepted() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-nano-{}", "a".repeat(60));
    commit_key_at(&mut rig.app, &key);

    assert!(rig.path.exists(), "the write precedes the ping");
    assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), key);
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Checking,
        "the row shows Checking while the ping runs"
    );

    rig.app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::KeyRow;
    press_and_update(&mut rig.app, KeyCode::Enter);
    assert!(!menu(&rig.app).key_editing, "Checking… disables the input");

    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Accepted);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Set {
                fingerprint: fingerprint.clone(),
            }
    });
    assert_eq!(
        rig.keys.lock().unwrap().as_slice(),
        std::slice::from_ref(&key),
        "the resolver was rebuilt from the committed key"
    );
}

#[test]
fn an_unreachable_ping_shows_not_checked_and_rebuilds_the_resolver() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-nano-{}", "b".repeat(60));
    commit_key_at(&mut rig.app, &key);

    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Unreachable);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::SetUnchecked {
                fingerprint: fingerprint.clone(),
            }
    });
    assert_eq!(
        rig.keys.lock().unwrap().as_slice(),
        std::slice::from_ref(&key),
        "an unreachable ping still rebuilds"
    );
    assert!(rig.path.exists(), "the key stays on disk");
}

#[test]
fn a_rejected_ping_keeps_the_file_and_leaves_the_resolver_alone() {
    let original: Arc<dyn DecisionResolver> = Arc::new(FakeResolver::new(model_answer("cold")));
    let mut rig = key_check_rig(original.clone());
    rig.app
        .world_mut()
        .resource_mut::<DecisionChannel>()
        .resolver = Some(original.clone());
    let checker = rig.checker.clone();
    let key = format!("sk-nano-{}", "c".repeat(60));
    commit_key_at(&mut rig.app, &key);

    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Rejected);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Rejected {
                fingerprint: fingerprint.clone(),
            }
    });

    assert_eq!(
        std::fs::read_to_string(&rig.path).unwrap(),
        key,
        "the rejected key stays on disk"
    );
    let current = rig
        .app
        .world()
        .resource::<DecisionChannel>()
        .resolver
        .clone()
        .expect("a resolver is still configured");
    assert!(Arc::ptr_eq(&current, &original), "no swap happened");
    assert!(
        rig.keys.lock().unwrap().is_empty(),
        "a rejection never rebuilds"
    );
}

#[test]
fn a_ping_that_never_answers_still_leaves_the_key_on_disk() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-nano-{}", "d".repeat(60));
    commit_key_at(&mut rig.app, &key);

    pump_until(&mut rig.app, |_| checker.calls() == 1);
    // Never release: the ping stays in flight across several frames.
    for _ in 0..4 {
        rig.app.update();
    }
    assert_eq!(
        std::fs::read_to_string(&rig.path).unwrap(),
        key,
        "the key is on disk before the ping answers"
    );
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Checking
    );
}

#[test]
fn there_is_no_standing_ping_at_launch_or_on_view_open() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    for _ in 0..4 {
        rig.app.update();
    }
    assert_eq!(rig.checker.calls(), 0, "boot and view-open ping nothing");

    press_and_update(&mut rig.app, KeyCode::Escape);
    click(&mut rig.app, MenuAction::OpenSettings);
    rig.app.update();
    assert_eq!(rig.checker.calls(), 0, "re-opening pings nothing");
}

#[test]
fn a_rejected_verdict_is_live_not_persisted() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-nano-{}", "e".repeat(60));
    commit_key_at(&mut rig.app, &key);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Rejected);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Rejected {
                fingerprint: fingerprint.clone(),
            }
    });

    // A relaunch reads only the file: shape-valid, so it shows `Set`.
    let read = credentials::read(Some(rig.config_dir.path()), None);
    assert_eq!(
        read.state.row,
        KeyRow::Set {
            fingerprint: credentials::fingerprint(&key),
        },
        "the verdict is live, not persisted"
    );
}

#[test]
fn an_accepted_key_applies_in_session_to_a_decision_without_a_relaunch() {
    let resolver = FakeResolver::new(model_answer("warm"));
    let mut rig = key_check_rig(Arc::new(resolver.clone()));
    let checker = rig.checker.clone();

    let key = format!("sk-nano-{}", "f".repeat(60));
    commit_key_at(&mut rig.app, &key);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Accepted);
    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Set {
                fingerprint: fingerprint.clone(),
            }
    });
    assert_eq!(
        rig.keys.lock().unwrap().as_slice(),
        std::slice::from_ref(&key)
    );

    // Leave Settings and the overlay, then run a decision with no relaunch.
    press_and_update(&mut rig.app, KeyCode::Escape);
    press_and_update(&mut rig.app, KeyCode::Escape);
    assert!(!menu(&rig.app).open, "back in play");
    {
        let mut runtime = rig.app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    pump_until(&mut rig.app, |_| resolver.calls() == 1);
    assert_eq!(
        resolver.calls(),
        1,
        "the rebuilt resolver is the one called"
    );
    resolver.release();
    pump_until(&mut rig.app, parked_on_say);
    let runtime = rig.app.world().resource::<Runtime>();
    let Some(Wait::Say { text, .. }) = runtime.runner.wait() else {
        panic!(
            "expected the model branch's say, got {:?}",
            runtime.runner.wait()
        );
    };
    assert!(text.contains("You came back!"), "got {text:?}");
}

#[test]
fn a_shape_failure_commit_never_quotes_the_value_in_a_log() {
    captures::install();
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    force_single_threaded(&mut rig.app);
    let secret = "sk-nano-not-a-key-because of the space";
    captures::take();

    commit_key_at(&mut rig.app, secret);
    rig.app.update();

    let records = captures::take();
    assert!(
        records
            .iter()
            .all(|(_, target, _)| target != crate::failure::LOG_TARGET),
        "a commit shape failure logs nothing under the failure target: {records:?}"
    );
    for (_, _, line) in records {
        assert!(!line.contains(secret), "the value leaked: {line}");
        assert!(
            !line.contains("because of the space"),
            "the value leaked: {line}"
        );
        assert!(
            !line.contains(&credentials::fingerprint(secret)),
            "more than the four-character fingerprint leaked: {line}"
        );
    }
    assert_eq!(rig.checker.calls(), 0);
}

#[test]
fn a_commit_without_a_checker_is_trusted_without_a_ping() {
    // The hermetic default: no checker installed, so the key is trusted with no
    // network call and no failure log. Production always installs
    // `HttpKeyCheck`; the offline default only exists for hostless worlds.
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    rig.app.insert_resource(crate::KeyCheckChannel::default());
    let key = format!("sk-nano-{}", "g".repeat(60));
    commit_key_at(&mut rig.app, &key);

    let fingerprint = credentials::fingerprint(&key);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Set {
                fingerprint: fingerprint.clone(),
            }
    });
    assert!(rig.path.exists(), "the key is written");
    assert_eq!(
        rig.keys.lock().unwrap().as_slice(),
        std::slice::from_ref(&key),
        "the trusted key still builds a resolver"
    );
}

#[test]
fn clearing_while_checking_cancels_the_ping_and_never_lands_a_stale_verdict() {
    let original: Arc<dyn DecisionResolver> = Arc::new(FakeResolver::new(model_answer("cold")));
    let mut rig = key_check_rig(original.clone());
    rig.app
        .world_mut()
        .resource_mut::<DecisionChannel>()
        .resolver = Some(original.clone());
    let checker = rig.checker.clone();
    let key = format!("sk-nano-{}", "h".repeat(60));
    commit_key_at(&mut rig.app, &key);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Checking
    );

    // Clear confirms once; the file goes, the resolver goes, the task is
    // dropped.
    rig.app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::ClearKey;
    press_and_update(&mut rig.app, KeyCode::Enter);
    assert_eq!(menu(&rig.app).confirm, Some(ConfirmKind::ClearKey));
    click(&mut rig.app, MenuAction::ConfirmAccept);
    assert!(!rig.path.exists(), "the file is deleted");
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::NotSet
    );
    assert!(
        rig.app
            .world()
            .resource::<DecisionChannel>()
            .resolver
            .is_none(),
        "Clear returns the resolver to None"
    );
    assert!(
        rig.app.world().resource::<PendingKeyCheck>().task.is_none(),
        "the in-flight ping is cancelled"
    );

    // A late answer from the dropped future never lands.
    checker.release(KeyVerdict::Accepted);
    for _ in 0..4 {
        rig.app.update();
    }
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::NotSet,
        "a stale verdict never lands"
    );
    assert!(
        rig.keys.lock().unwrap().is_empty(),
        "a cleared key never rebuilds"
    );
}

#[test]
fn a_ping_verdict_never_quotes_the_key_in_a_log() {
    captures::install();
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    force_single_threaded(&mut rig.app);
    let checker = rig.checker.clone();
    let unreachable = format!("sk-nano-{}", "i".repeat(60));

    // Unreachable: the warn names the outcome, never the key.
    captures::take();
    commit_key_at(&mut rig.app, &unreachable);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::Unreachable);
    let unreachable_fp = credentials::fingerprint(&unreachable);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::SetUnchecked {
                fingerprint: unreachable_fp.clone(),
            }
    });
    let records = captures::take();
    assert!(
        !records.is_empty(),
        "the unreachable warn was captured on this thread"
    );
    for (_, _, line) in records {
        assert!(!line.contains(&unreachable), "the key leaked: {line}");
        assert!(
            !line.contains(&unreachable_fp),
            "the fingerprint leaked: {line}"
        );
    }

    // Rejected: same rule.
    checker.reset();
    let rejected = format!("sk-nano-{}", "j".repeat(60));
    captures::take();
    commit_key_at(&mut rig.app, &rejected);
    pump_until(&mut rig.app, |_| checker.calls() == 2);
    checker.release(KeyVerdict::Rejected);
    let rejected_fp = credentials::fingerprint(&rejected);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row
            == KeyRow::Rejected {
                fingerprint: rejected_fp.clone(),
            }
    });
    let records = captures::take();
    assert!(
        !records.is_empty(),
        "the rejected warn was captured on this thread"
    );
    for (_, _, line) in records {
        assert!(!line.contains(&rejected), "the key leaked: {line}");
        assert!(
            !line.contains(&rejected_fp),
            "the fingerprint leaked: {line}"
        );
    }
}

// OpenRouter keys are never sent (docs/nanogpt.md N10, N11)

#[test]
fn a_typed_openrouter_key_is_refused_before_any_write_ping_or_resolver_change() {
    let original: Arc<dyn DecisionResolver> = Arc::new(FakeResolver::new(model_answer("warm")));
    let mut rig = key_check_rig(original.clone());
    rig.app
        .world_mut()
        .resource_mut::<DecisionChannel>()
        .resolver = Some(original.clone());
    let typed = format!("sk-or-v1-{}", "t".repeat(60));

    commit_key_at(&mut rig.app, &format!("  {typed}  "));
    for _ in 0..3 {
        rig.app.update();
    }

    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::OpenRouterKey
    );
    assert_eq!(field_text(&rig.app), "", "the field is cleared");
    assert!(!rig.path.exists(), "nothing is written");
    assert_eq!(rig.checker.calls(), 0, "no ping");
    assert!(rig.keys.lock().unwrap().is_empty(), "no resolver is built");
    let channel = rig.app.world().resource::<DecisionChannel>();
    assert!(
        Arc::ptr_eq(channel.resolver.as_ref().unwrap(), &original),
        "the resolver is untouched"
    );
    rig.app
        .world_mut()
        .run_system_once(crate::presentation::settings_view::sync_key_row)
        .unwrap();
    assert_eq!(
        key_row_text(&rig.app).as_deref(),
        Some("Invalid \u{2014} an OpenRouter key; enter a NanoGPT key")
    );
}

#[test]
fn a_shape_failing_openrouter_prefix_still_reports_the_shape_failure_first() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    commit_key_at(&mut rig.app, "sk-or-short");
    assert_eq!(
        rig.app.world().resource::<CredentialsState>().row,
        KeyRow::Invalid,
        "trim, then shape, then the OpenRouter check"
    );
}

#[test]
fn a_stored_openrouter_key_on_view_open_builds_nothing_keeps_the_file_and_logs_once() {
    captures::install();
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let stored = format!("sk-or-v1-{}", "s".repeat(60));
    credentials::write(config_dir.path(), &stored).unwrap();

    let (keys, factory) = recording_factory(Arc::new(FakeResolver::new(model_answer("warm"))));
    let mut app = app_for_pack(pack.path().join("pack"));
    force_single_threaded(&mut app);
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::NotSet,
    });
    app.insert_resource(factory);
    captures::take();
    open_settings_with_ui(&mut app);

    assert_eq!(
        app.world().resource::<CredentialsState>().row,
        KeyRow::OpenRouterKey
    );
    assert!(keys.lock().unwrap().is_empty(), "no resolver is built");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), stored);
    let records = credential_lines();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(records[0].2.contains("credential file"), "{records:?}");
    assert!(records[0].2.contains("only NanoGPT keys"), "{records:?}");
    assert!(!records[0].2.contains("sk-or"), "{records:?}");
    assert!(!records[0].2.contains(&stored[8..16]), "{records:?}");
    assert_eq!(
        key_row_text(&app).as_deref(),
        Some("Invalid \u{2014} an OpenRouter key; enter a NanoGPT key")
    );
    assert!(
        key_field_entity(&app).is_some(),
        "the field stays editable so a NanoGPT key can be entered"
    );
}

#[test]
fn an_openrouter_key_in_the_variable_row_is_read_only_with_the_env_note() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: None,
        path: None,
        row: KeyRow::FromEnvironmentOpenRouter,
    });
    open_settings_with_ui(&mut app);

    assert!(key_field_entity(&app).is_none(), "not editable");
    let text = key_row_text(&app).expect("the key row renders");
    assert!(
        text.starts_with("From environment \u{2014} Invalid, an OpenRouter key"),
        "{text}"
    );
    assert!(text.contains(credentials::ENV_NOTE), "{text}");
}

#[test]
fn a_key_check_verdict_of_openrouter_key_builds_no_resolver() {
    let mut rig = key_check_rig(Arc::new(FakeResolver::new(model_answer("warm"))));
    let checker = rig.checker.clone();
    let key = format!("sk-nano-{}", "z".repeat(60));
    commit_key_at(&mut rig.app, &key);
    pump_until(&mut rig.app, |_| checker.calls() == 1);
    checker.release(KeyVerdict::OpenRouterKey);
    pump_until(&mut rig.app, |app| {
        app.world().resource::<CredentialsState>().row == KeyRow::OpenRouterKey
    });
    assert!(rig.keys.lock().unwrap().is_empty());
}
