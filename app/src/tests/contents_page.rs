//! Ticket #116: Default look §4's contents page, drawn from the plugin's
//! `contents` surface.
//!
//! These pin the page's geometry — the opaque frame fill, the 420px column, the
//! hairline, the section entries and their current marker, the foot's Resume and
//! Exit to title, the right column's heading and the ledger's fields — and the
//! behaviour the ticket restates: the page opens on Save, `Esc` keeps its four
//! moments, Exit to title reaches every section, and no play frame is drawn
//! behind the page.

use super::*;

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

/// The page's tree root: [`MenuRoot`]'s first child.
fn page_root(app: &App) -> Entity {
    let menu_root = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::MenuRoot>()
                .map(|_| entity.id())
        })
        .expect("the page draws a root");
    children_of(app, menu_root)
        .into_iter()
        .next()
        .expect("the root holds the surface's tree")
}

/// The left column: the 420px box that holds the entries.
fn rail(app: &App) -> Entity {
    children_of(app, page_root(app))
        .into_iter()
        .find(|child| app.world().get::<Node>(*child).unwrap().width == Val::Px(420.0))
        .expect("the page draws the 420px column")
}

/// The right column's body: the tree puts the ledger list last in the column, and
/// the section's own view fills that same list when the page is not on Save or
/// Load.
fn body(app: &App) -> Entity {
    let columns = children_of(app, page_root(app));
    let column = *columns.last().expect("the page has a right column");
    let children = children_of(app, column);
    assert_eq!(
        children.len(),
        2,
        "the right column holds the heading and its body"
    );
    children[1]
}

/// The drawn control for `action`: the template places all five of a list's
/// controls in every item and the engine shows only the item's own, so a hidden
/// sibling never stands for the row.
fn shown_action_entity(app: &App, action: MenuAction) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            let drawn = entity.get::<MenuAction>() == Some(&action)
                && entity.get::<Button>().is_some()
                && entity
                    .get::<Node>()
                    .is_some_and(|node| node.display != Display::None);
            drawn.then_some(entity.id())
        })
        .unwrap_or_else(|| panic!("no drawn control {action:?}"))
}

fn fill(app: &App, entity: Entity) -> Color {
    app.world()
        .get::<BackgroundColor>(entity)
        .map(|fill| fill.0)
        .unwrap_or(Color::NONE)
}

/// Every word drawn inside `root`, in paint order.
fn subtree_words(app: &App, root: Entity) -> Vec<String> {
    let mut words = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(text) = app.world().get::<Text>(entity) {
            words.push(text.0.clone());
        }
        stack.extend(children_of(app, entity));
    }
    words.reverse();
    words
}

/// Whether `entity` sits inside `root`.
fn is_within(app: &App, entity: Entity, root: Entity) -> bool {
    let mut current = Some(entity);
    while let Some(node) = current {
        if node == root {
            return true;
        }
        current = app.world().get::<ChildOf>(node).map(ChildOf::parent);
    }
    false
}

/// Whether a drawn control carries `action`.
fn shows_action(app: &App, action: MenuAction) -> bool {
    app.world().iter_entities().any(|entity| {
        entity.get::<MenuAction>() == Some(&action)
            && entity.get::<Button>().is_some()
            && entity
                .get::<Node>()
                .is_some_and(|node| node.display != Display::None)
    })
}

/// §4's entries and the actions they carry, in order.
const ENTRIES: [(MenuAction, MenuFocus, &str); 5] = [
    (
        MenuAction::SetMode(MenuMode::Save),
        MenuFocus::SaveTab,
        "Save",
    ),
    (
        MenuAction::SetMode(MenuMode::Load),
        MenuFocus::LoadTab,
        "Load",
    ),
    (
        MenuAction::OpenTimeline,
        MenuFocus::TimelineButton,
        "Timeline",
    ),
    (MenuAction::OpenGallery, MenuFocus::GalleryButton, "Gallery"),
    (
        MenuAction::OpenSettings,
        MenuFocus::SettingsButton,
        "Settings",
    ),
];

fn open_page(app: &mut App) {
    esc(app);
    assert_eq!(menu(app).view, MenuView::Slots);
    assert_eq!(menu(app).mode, MenuMode::Save, "§7: Esc opens on Save");
    build_pause_tree(app);
}

/// §4's page: one opaque full-frame fill, a 420px column, a hairline, and the
/// five section entries under a small mono `CONTENTS` label.
#[test]
fn the_page_draws_the_opaque_frame_the_column_and_the_hairline() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_page(&mut app);

    let page = page_root(&app);
    assert_eq!(
        fill(&app, page),
        Color::srgb(0.051, 0.055, 0.071),
        "§4's page is the opaque #0d0e12 fill"
    );
    let root_node = app.world().get::<Node>(page).unwrap();
    assert_eq!(root_node.position_type, PositionType::Absolute);
    assert_eq!(root_node.width, Val::Percent(100.0));
    assert_eq!(root_node.height, Val::Percent(100.0));

    let columns = children_of(&app, page);
    assert_eq!(columns.len(), 3, "the rail, the hairline and the column");
    let divider = columns[1];
    let divider_node = app.world().get::<Node>(divider).unwrap();
    assert_eq!(divider_node.width, Val::Px(1.0), "a hairline");
    assert_eq!(divider_node.height, Val::Percent(100.0));
    assert_eq!(
        app.world()
            .get::<BackgroundColor>(divider)
            .unwrap()
            .0
            .alpha(),
        0.14,
        "the hairline draws the theme's hairline tone"
    );

    assert_eq!(
        app.world().get::<Node>(body(&app)).unwrap().min_height,
        Val::Px(0.0),
        "the player lets the scroll host shrink below its content"
    );
    let rail_node = app.world().get::<Node>(rail(&app)).unwrap();
    assert_eq!(rail_node.width, Val::Px(420.0), "§4's 420px column");
    assert_eq!(rail_node.flex_direction, FlexDirection::Column);

    let label = text_entity(&app, "CONTENTS");
    assert_eq!(
        face_handle(&app, label),
        app.world()
            .resource::<crate::presentation::style::TextFaces>()
            .mono_medium,
        "the CONTENTS label is mono"
    );
    assert_eq!(
        app.world().get::<TextFont>(label).unwrap().font_size,
        bevy::text::FontSize::Px(12.0),
        "and small"
    );
    assert_eq!(
        children_of(&app, rail(&app))[0],
        label,
        "it heads the column"
    );
}

/// §4's entries: `Save`, `Load`, `Timeline`, `Gallery`, `Settings` in that
/// order, faint until one is current, white with an orange `·` when it is.
#[test]
fn the_five_entries_stand_in_order_with_the_current_one_white_and_marked() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_page(&mut app);

    let mut drawn = Vec::new();
    for (action, _, word) in ENTRIES {
        let entry = shown_action_entity(&app, action);
        assert_eq!(
            app.world().get::<Node>(entry).unwrap().display,
            Display::Flex,
            "the {word:?} entry draws"
        );
        drawn.push(
            subtree_words(
                &app,
                parent_of(&app, parent_of(&app, text_entity(&app, word))),
            )
            .join(" "),
        );
    }
    assert_eq!(
        drawn,
        vec![
            "Save \u{00b7}".to_string(),
            "Load \u{00b7}".to_string(),
            "Timeline \u{00b7}".to_string(),
            "Gallery \u{00b7}".to_string(),
            "Settings \u{00b7}".to_string(),
        ],
        "§4's order, each entry with its marker"
    );

    app.update();
    let label = |word: &str| text_entity(&app, word);
    let tone = |entity: Entity| app.world().get::<TextColor>(entity).unwrap().0;
    assert_eq!(
        tone(label("Save")),
        Color::srgb(0.90, 0.90, 0.92),
        "the current section is white"
    );
    assert_eq!(
        tone(label("Load")),
        Color::srgba(0.80, 0.80, 0.85, 0.35),
        "the others are faint"
    );
    let marker = |word: &str| {
        let entry = parent_of(&app, parent_of(&app, label(word)));
        children_of(&app, entry)
            .into_iter()
            .find(|child| {
                app.world()
                    .get::<Text>(*child)
                    .is_some_and(|text| text.0 == "\u{00b7}")
            })
            .expect("the entry carries its marker")
    };
    assert_eq!(
        app.world().get::<TextColor>(marker("Save")).unwrap().0,
        Color::srgb(0.902, 0.62, 0.361),
        "the current section's marker is the FOCUS orange"
    );
    assert_eq!(
        app.world()
            .get::<TextColor>(marker("Load"))
            .unwrap()
            .0
            .alpha(),
        0.0,
        "and only the current section draws its marker"
    );

    app.world_mut().resource_mut::<MenuState>().mode = MenuMode::Load;
    build_pause_tree(&mut app);
    app.update();
    assert_eq!(
        app.world()
            .get::<TextColor>(text_entity(&app, "Load"))
            .unwrap()
            .0,
        Color::srgb(0.90, 0.90, 0.92),
        "Load is white once it is the open section"
    );
    assert_eq!(
        app.world()
            .get::<TextColor>(text_entity(&app, "Save"))
            .unwrap()
            .0,
        Color::srgba(0.80, 0.80, 0.85, 0.35),
        "and Save returns to the faint tone"
    );
}

/// §4's foot: `► Resume reading` in the FOCUS tone and `Exit to title` under the
/// entries, Resume's mark drawn in a face that carries it.
#[test]
fn the_foot_draws_resume_reading_in_the_focus_tone_and_exit_to_title() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_page(&mut app);

    let children = children_of(&app, rail(&app));
    let resume = text_entity(&app, "Resume reading");
    let index = |entity: Entity| {
        children
            .iter()
            .position(|child| *child == entity)
            .unwrap_or_else(|| panic!("{entity:?} is in the column"))
    };
    let resume_row = parent_of(&app, shown_action_entity(&app, MenuAction::Resume));
    assert_eq!(
        index(resume_row) + 1,
        index(shown_action_entity(&app, MenuAction::ExitToTitle)),
        "Resume sits directly above Exit to title at the foot"
    );
    assert_eq!(
        app.world().get::<TextColor>(resume).unwrap().0,
        Color::srgb(0.902, 0.62, 0.361),
        "Resume reading is in the FOCUS tone"
    );
    let mark = children_of(&app, resume_row)
        .into_iter()
        .find(|child| {
            app.world()
                .get::<Text>(*child)
                .is_some_and(|text| text.0 == "\u{25ba}")
        })
        .expect("the mark is the ► glyph");
    assert_eq!(
        face_handle(&app, mark),
        app.world()
            .resource::<crate::presentation::style::TextFaces>()
            .mono_medium,
        "no embedded sans face carries ►, so the mark is drawn in mono"
    );
    assert_eq!(
        app.world().get::<TextColor>(mark).unwrap().0,
        Color::srgb(0.902, 0.62, 0.361),
        "the mark shares Resume's tone"
    );
}

/// §4's heading: the pack's name over the section's own line naming the scene.
#[test]
fn the_heading_names_the_pack_and_the_scene_in_the_sections_words() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_page(&mut app);

    let scene = app
        .world()
        .resource::<Runtime>()
        .runner
        .scene_id()
        .to_string();
    let heading = text_entity(
        &app,
        &format!("Save your place \u{2014} you are in {scene}"),
    );
    assert_eq!(
        app.world().get::<Text>(heading).unwrap().0,
        format!("Save your place \u{2014} you are in {scene}"),
        "Save's heading is §4's line"
    );
    let pack_name = text_entity(&app, "unmei demo");
    assert_eq!(
        face_handle(&app, pack_name),
        app.world()
            .resource::<crate::presentation::style::TextFaces>()
            .sans_semibold,
        "the pack name heads the column in SemiBold"
    );
    assert_eq!(
        parent_of(&app, parent_of(&app, pack_name)),
        parent_of(&app, parent_of(&app, heading)),
        "the pack name and the scene line share one heading block"
    );

    app.world_mut().resource_mut::<MenuState>().mode = MenuMode::Load;
    build_pause_tree(&mut app);
    assert_eq!(
        app.world()
            .get::<Text>(text_entity(
                &app,
                &format!("Return to a page \u{2014} you are in {scene}")
            ))
            .unwrap()
            .0,
        format!("Return to a page \u{2014} you are in {scene}"),
        "Load's heading is §4's other line"
    );
}

/// §4's walk moves the FOCUS ring between the rows it draws: the ring is the
/// drawn row control's border, so a key and the page's paint meet here.
#[test]
fn the_walk_lights_the_row_it_draws() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_page(&mut app);
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(&mut app);
    app.update();

    let ring = |app: &App, row: usize| {
        app.world()
            .get::<BorderColor>(shown_action_entity(app, MenuAction::Card(row)))
            .map(|border| border.top)
            .expect("the row's ring")
    };
    assert_eq!(
        ring(&app, 0),
        Color::srgb(0.902, 0.62, 0.361),
        "the first row starts lit"
    );
    assert_eq!(ring(&app, 1), Color::NONE);

    press_and_update(&mut app, KeyCode::ArrowDown);
    app.update();
    assert_eq!(
        ring(&app, 1),
        Color::srgb(0.902, 0.62, 0.361),
        "↓ lights the second row"
    );
    assert_eq!(ring(&app, 0), Color::NONE, "and unlights the first");

    press_and_update(&mut app, KeyCode::Digit3);
    app.update();
    assert_eq!(
        ring(&app, 2),
        Color::srgb(0.902, 0.62, 0.361),
        "3 lights the third row"
    );
}

/// A readable slot and an unreadable one draw their own status, scene and
/// reason: §4's row holds the label, the status and the scene id or reason over
/// the saved-at line (`docs/runtime-hardening.md` § Menu overlay §2).
#[test]
fn an_occupied_row_and_an_unreadable_row_draw_their_own_words() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    {
        let runtime = app.world().resource::<Runtime>();
        let mut save = SaveFile::capture(&runtime.runner, &runtime.staging);
        save.saved_at = Some(1_700_000_000);
        save::write_slot(runtime.runner.pack(), Slot::Manual(1), &save).unwrap();
    }
    std::fs::write(slot_path(&app, Slot::Manual(3)), b"not a save at all").unwrap();
    open_page(&mut app);

    let row =
        |app: &App, index: usize| parent_of(app, shown_action_entity(app, MenuAction::Card(index)));
    let occupied = subtree_words(&app, row(&app, 0)).join(" ");
    assert!(occupied.contains("Slot 1"), "{occupied:?}");
    assert!(
        occupied.contains("manual"),
        "the row draws the slot's kind (§2's first field): {occupied:?}"
    );
    assert!(occupied.contains("Ok"), "the row draws its status");
    assert!(
        occupied.contains("first_meeting"),
        "the row draws the scene the save holds: {occupied:?}"
    );
    assert!(
        !occupied.contains("no save yet"),
        "and the time the save was written, not the empty mark"
    );

    let autosave = subtree_words(&app, row(&app, 3)).join(" ");
    assert!(
        autosave.contains("game-written"),
        "the autosave is marked game-written (§2): {autosave:?}"
    );

    let unreadable = subtree_words(&app, row(&app, 2)).join(" ");
    assert!(
        unreadable.contains("Unreadable"),
        "the row draws its status: {unreadable:?}"
    );
    assert!(
        unreadable.contains("manual"),
        "and the slot's kind: {unreadable:?}"
    );
    assert!(
        unreadable.to_lowercase().contains("invalid"),
        "and the reason it cannot be read: {unreadable:?}"
    );
}

/// §4's ledger: one row per slot in `Slot::ALL` order with the label, the scene
/// or status over the saved-at line, and the action at the row's right, muted
/// when it is disabled. The legacy entry is the last row in save mode only.
#[test]
fn the_ledger_shows_every_slot_and_the_legacy_row_in_save_mode_only() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_page(&mut app);

    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    for (row, (label, action)) in [
        ("Slot 1", "Save here"),
        ("Slot 2", "Save here"),
        ("Slot 3", "Save here"),
        ("Autosave", "Load-only"),
    ]
    .into_iter()
    .enumerate()
    {
        let control = shown_action_entity(&app, MenuAction::Card(row));
        let item = parent_of(&app, parent_of(&app, control));
        let text = flattened_text(&app, item);
        assert!(text.contains(label), "row {row} names {label:?}: {text:?}");
        assert!(
            text.contains(action),
            "row {row} offers {action:?}: {text:?}"
        );
        assert!(
            text.contains("no save yet"),
            "row {row} carries its saved-at line: {text:?}"
        );
        assert!(
            text.contains('\u{2014}'),
            "row {row} carries the empty mark for the scene: {text:?}"
        );
    }
    assert_eq!(
        face_handle(&app, text_entity(&app, "Slot 1")),
        faces.mono_medium,
        "the slot label is mono"
    );
    app.update();
    let muted = app
        .world()
        .get::<TextColor>(text_entity(&app, "Load-only"))
        .unwrap()
        .0;
    assert_eq!(
        muted,
        Color::srgba(0.80, 0.80, 0.85, 0.35),
        "a disabled action is the muted tone"
    );

    assert!(
        !shows_action(&app, MenuAction::LegacyImport),
        "no legacy file, no legacy row"
    );

    write_legacy_row(&app);
    {
        let pack = app.world().resource::<Runtime>().runner.pack().clone();
        app.world_mut().resource_mut::<MenuState>().refresh(&pack);
    }
    build_pause_tree(&mut app);
    let legacy = parent_of(&app, shown_action_entity(&app, MenuAction::LegacyImport));
    let last = parent_of(&app, shown_action_entity(&app, MenuAction::Card(3)));
    let rows = children_of(&app, body(&app));
    assert_eq!(
        rows.last(),
        Some(&legacy),
        "the legacy row is the ledger's last"
    );
    assert_eq!(
        rows.get(rows.len() - 2),
        Some(&last),
        "and it follows the autosave's row"
    );
    assert!(
        flattened_text(&app, legacy).contains("Legacy save"),
        "the row draws the legacy entry's words"
    );

    app.world_mut().resource_mut::<MenuState>().mode = MenuMode::Load;
    build_pause_tree(&mut app);
    assert!(
        !shows_action(&app, MenuAction::LegacyImport),
        "load mode shows no legacy row"
    );
    app.update();
    let autosave = parent_of(&app, shown_action_entity(&app, MenuAction::Card(3)));
    let action = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            let text = entity.get::<Text>()?;
            (text.0 == "Load" && is_within(&app, entity.id(), autosave))
                .then(|| entity.get::<TextColor>().map(|colour| colour.0))
                .flatten()
        })
        .expect("the autosave's action line");
    assert_eq!(
        action,
        Color::srgba(0.80, 0.80, 0.85, 0.35),
        "an empty autosave has nothing to load, so its action stays muted"
    );
}

/// Write a legacy save beside the pack, so the ledger's final row appears.
fn write_legacy_row(app: &App) {
    let pack = app.world().resource::<Runtime>().runner.pack();
    let path = unmei_engine::save::legacy_save_path_for(pack);
    let runtime = app.world().resource::<Runtime>();
    let save = SaveFile::capture(&runtime.runner, &runtime.staging);
    unmei_engine::save::write_save(&path, &save).expect("the legacy fixture writes");
}

/// §4 draws no play frame behind the page.
#[test]
fn the_play_frame_is_not_drawn_behind_the_page() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    set_text_speed(&mut app, 0.0);
    render_dialogue(&mut app);
    build_pause_tree(&mut app);
    assert!(
        app.world()
            .resource::<crate::presentation::play::PlayFrame>()
            .prose()
            .is_some(),
        "the frame draws while play is up"
    );

    open_page(&mut app);
    render_dialogue(&mut app);
    let frame = app
        .world()
        .resource::<crate::presentation::play::PlayFrame>();
    assert_eq!(
        frame.prose(),
        None,
        "the page replaces the frame instead of dimming it"
    );
    let root = frame.root().expect("the frame spawned");
    assert_eq!(
        app.world().get::<Node>(root).unwrap().display,
        Display::None,
        "the whole frame is hidden, not just its line"
    );

    esc(&mut app);
    build_pause_tree(&mut app);
    render_dialogue(&mut app);
    assert!(
        app.world()
            .resource::<crate::presentation::play::PlayFrame>()
            .prose()
            .is_some(),
        "closing the page brings the frame back"
    );
}

/// §4 moves `Exit to title` to the page's foot, so every section of the overlay's
/// page reaches it and the title's doors still draw none.
#[test]
fn exit_to_title_stands_at_the_foot_of_every_section() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_page(&mut app);
    assert!(shows_action(&app, MenuAction::ExitToTitle), "on the ledger");

    for (action, _, _) in ENTRIES.into_iter().skip(2) {
        click(&mut app, action);
        build_pause_tree(&mut app);
        assert!(
            shows_action(&app, MenuAction::ExitToTitle),
            "{action:?}'s section draws Exit to title"
        );
        if action != MenuAction::OpenSettings {
            click(&mut app, MenuAction::BackToSlots);
        }
    }

    click(&mut app, MenuAction::ExitToTitle);
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::ExitToTitle),
        "the control asks once, as § Title §5 says"
    );

    let mut title = temp_title_app();
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut title, KeyCode::Enter);
    build_pause_tree(&mut title);
    assert!(
        !shows_action(&title, MenuAction::ExitToTitle),
        "the title's Settings door has no live session to leave"
    );
}

/// §4's right column holds the ledger on Save and Load and the section's own view
/// in the same place for the other three, inside the height the heading leaves.
#[test]
fn the_section_s_own_view_draws_in_the_right_column() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_page(&mut app);

    let ledger_row = parent_of(&app, shown_action_entity(&app, MenuAction::Card(0)));
    assert_eq!(
        parent_of(&app, ledger_row),
        body(&app),
        "the ledger's rows fill the column's body"
    );

    click(&mut app, MenuAction::OpenSettings);
    build_pause_tree(&mut app);
    let settings_body = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::settings_view::SettingsBody>()
                .map(|_| entity.id())
        })
        .expect("the Settings body draws");
    assert_eq!(
        parent_of(&app, settings_body),
        body(&app),
        "Settings renders where the ledger was"
    );
    assert_eq!(
        app.world().get::<Node>(settings_body).unwrap().flex_grow,
        1.0,
        "the body fills the height the heading leaves, so it can scroll"
    );

    click(&mut app, MenuAction::BackToSlots);
    click(&mut app, MenuAction::OpenTimeline);
    build_pause_tree(&mut app);
    let timeline_heading = text_entity(&app, menu::TIMELINE_HEADING);
    assert!(
        is_within(&app, timeline_heading, body(&app)),
        "the Timeline renders in the column's body too"
    );
}

/// §7: the page opens on Save from `Esc` and on Load from the quick row's `Load`,
/// and `Esc` keeps its four moments — a confirm first, then the sub-view, then the
/// page, then nothing.
#[test]
fn the_page_opens_on_each_section_and_esc_keeps_its_moments() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));

    click(&mut app, MenuAction::QuickLoad);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert_eq!(menu(&app).mode, MenuMode::Load, "Load opens on Load");

    press_and_update(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open, "the first Esc resumes reading");

    press_and_update(&mut app, KeyCode::Escape);
    assert!(menu(&app).open);
    assert_eq!(menu(&app).mode, MenuMode::Save, "Esc opens on Save");

    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).view, MenuView::Settings);
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(
        menu(&app).view,
        MenuView::Slots,
        "the second moment backs out of the section"
    );
    assert!(menu(&app).open, "the page stands after a section backs out");

    // The first moment, from the ledger: a confirm takes the Esc, and the page
    // stays up with focus back on the control that asked.
    press_and_update(&mut app, KeyCode::ArrowDown);
    for _ in 0..4 {
        press_and_update(&mut app, KeyCode::ArrowDown);
    }
    assert_eq!(menu(&app).focus, MenuFocus::ExitToTitle);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).confirm, Some(ConfirmKind::ExitToTitle));
    assert_eq!(menu(&app).focus, MenuFocus::ConfirmYes);
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).confirm, None, "the first moment cancels");
    assert_eq!(menu(&app).focus, MenuFocus::ExitToTitle);
    assert!(menu(&app).open, "and leaves the page up");

    press_and_update(&mut app, KeyCode::Escape);
    assert!(!menu(&app).open, "the next closes the page");
}
