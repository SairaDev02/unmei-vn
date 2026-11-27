use super::*;

/// A gallery entry resolves to its kind's one path producer and failure row: a
/// background to `backgrounds/<id>.png` under `BackgroundAsset`, a sprite to
/// `sprites/<npc_id>/<pose>.png` under `SpriteAsset`, so the grid and the still
/// share the play path's two producers and never fork a second inline copy
/// (`docs/title-and-menu.md` § Gallery § Entries §4/§5).
#[test]
fn gallery_entries_resolve_through_the_shared_path_producers() {
    let background = unmei_engine::GalleryEntry::Background {
        id: "school_gate".to_string(),
    };
    assert_eq!(
        crate::presentation::assets::gallery_entry_asset(&background),
        (
            crate::failure::FailureRow::BackgroundAsset,
            "backgrounds/school_gate.png".to_string()
        )
    );
    assert_eq!(
        crate::presentation::assets::gallery_entry_asset(&background).1,
        crate::presentation::assets::background_asset_path("school_gate"),
        "the gallery reuses the play path's background producer"
    );

    let sprite = unmei_engine::GalleryEntry::Sprite {
        npc_id: "ace".to_string(),
        pose: "happy".to_string(),
    };
    assert_eq!(
        crate::presentation::assets::gallery_entry_asset(&sprite),
        (
            crate::failure::FailureRow::SpriteAsset,
            "sprites/ace/happy.png".to_string()
        )
    );
    assert_eq!(
        crate::presentation::assets::gallery_entry_asset(&sprite).1,
        crate::presentation::assets::sprite_asset_path("ace", "happy"),
        "the gallery reuses the play path's sprite producer"
    );
}

/// A copied demo pack whose scenes declare no `bg`/`sprite` and whose manifest
/// declares no `title_backdrop`, so its gallery has no entries
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
fn temp_pack_without_gallery_entries() -> tempfile::TempDir {
    let dir = temp_pack();
    let root = dir.path().join("pack");
    std::fs::write(
        root.join("pack.ron"),
        "(\n    name: \"unmei demo\",\n    entry: \"entry\",\n)\n",
    )
    .unwrap();
    for entry in std::fs::read_dir(root.join("scenes")).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    std::fs::write(root.join("scenes/entry.unmei"), "Narrator: Nothing here.\n").unwrap();
    dir
}

/// Repaint the still's caption and art for the current focus, as the real
/// presentation does every frame. The core test world registers no such system.
fn sync_still(app: &mut App) {
    app.world_mut()
        .run_system_once(crate::presentation::gallery::sync_gallery_still)
        .expect("sync_gallery_still runs");
}

/// Open the overlay, then its `Gallery` control: the overlay's door into the
/// gallery. Assumes the overlay is closed.
fn open_overlay_gallery(app: &mut App) {
    esc(app);
    assert!(menu(app).open, "the overlay opened");
    assert_eq!(menu(app).view, MenuView::Slots);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(app, KeyCode::Enter);
    assert_eq!(menu(app).view, MenuView::Gallery);
}

/// The title's `Gallery` row is enabled and caption-free
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1/§7).
#[test]
fn the_title_gallery_row_is_enabled_and_caption_free() {
    let mut app = temp_title_app();
    build_pause_tree(&mut app);

    assert_eq!(
        title_label(&app, MenuAction::Gallery).as_deref(),
        Some(menu::GALLERY_HEADING),
        "the row carries no caption"
    );
    let gallery = app
        .world()
        .get::<TextColor>(text_entity(&app, menu::GALLERY_HEADING))
        .expect("the row label is tinted")
        .0;
    let settings = app
        .world()
        .get::<TextColor>(text_entity(&app, "Settings"))
        .expect("the Settings row label is tinted")
        .0;
    assert_eq!(
        gallery, settings,
        "Gallery renders in the same enabled tone as Settings"
    );
}

/// `Tab` cycles Save → Load → Timeline → Gallery → Settings on the ledger, and
/// narrows to Save → Load in the gallery, Timeline and Settings views, where the
/// section entries stand but only those two are Tab stops
/// (`docs/title-and-menu.md` § Default look §4, § Gallery `### Behavior` §1).
#[test]
fn tab_cycles_the_sections_and_narrows_inside_the_sub_views() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::SaveTab;
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

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::SaveTab;
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::LoadTab, "gallery: Save → Load");
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab, "gallery: Load → Save");

    click(&mut app, MenuAction::GalleryBack);
    click(&mut app, MenuAction::OpenSettings);
    assert_eq!(menu(&app).view, MenuView::Settings);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::SaveTab;
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::LoadTab,
        "settings: Save → Load"
    );
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::SaveTab,
        "settings: Load → Save"
    );
}

/// From the title's door no top bar is drawn, so `Tab` cannot reach an
/// invisible Save/Load control and flip the mode
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1).
#[test]
fn tab_is_inert_on_the_titles_gallery_and_settings() {
    let mut title = temp_title_app();
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    assert_eq!(menu(&title).view, MenuView::Gallery);
    let focus = menu(&title).focus;
    press_and_update(&mut title, KeyCode::Tab);
    assert_eq!(menu(&title).focus, focus, "Tab is inert in the gallery");
    assert_eq!(menu(&title).mode, MenuMode::Save);

    let mut settings = temp_title_app();
    settings.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut settings, KeyCode::Enter);
    assert_eq!(menu(&settings).view, MenuView::Settings);
    let focus = menu(&settings).focus;
    press_and_update(&mut settings, KeyCode::Tab);
    assert_eq!(menu(&settings).focus, focus, "Tab is inert in Settings");
    assert_eq!(menu(&settings).mode, MenuMode::Save);
}

/// The section column stands in every section, so the Gallery's entry is drawn
/// on the ledger and inside the gallery itself, and a sub-view's interior carries
/// no door of its own (`docs/title-and-menu.md` § Default look §4).
#[test]
fn the_section_entries_stand_in_every_section() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);
    build_pause_tree(&mut app);
    assert!(has_action(&app, MenuAction::OpenGallery));
    assert!(has_action(&app, MenuAction::OpenSettings));

    click(&mut app, MenuAction::OpenGallery);
    build_pause_tree(&mut app);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert!(
        has_action(&app, MenuAction::OpenGallery),
        "the page's own entry stays drawn inside the gallery"
    );
    assert!(has_action(&app, MenuAction::OpenSettings));

    let entries = app
        .world()
        .iter_entities()
        .filter(|entity| {
            [
                MenuAction::SetMode(MenuMode::Save),
                MenuAction::SetMode(MenuMode::Load),
                MenuAction::OpenTimeline,
                MenuAction::OpenGallery,
                MenuAction::OpenSettings,
            ]
            .contains(
                &entity
                    .get::<MenuAction>()
                    .copied()
                    .unwrap_or(MenuAction::Resume),
            ) && entity.get::<Button>().is_some()
                && entity
                    .get::<Node>()
                    .is_some_and(|node| node.display != Display::None)
        })
        .count();
    assert_eq!(entries, 5, "the page's five section entries and no other");
}

/// The Gallery entry draws the spec's copy verbatim in the section column's face:
/// the ledger's entries are 24px SemiBold sans
/// (`docs/title-and-menu.md` § Default look §4, §6).
#[test]
fn the_gallery_section_entry_draws_the_copy_in_the_sections_face() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);
    build_pause_tree(&mut app);

    let label = text_entity(&app, "Gallery");
    assert_eq!(
        app.world().get::<Text>(label).unwrap().0,
        "Gallery",
        "the entry's copy is exact"
    );
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    assert_eq!(
        face_handle(&app, label),
        faces.sans_semibold,
        "the section entries are SemiBold sans"
    );
    assert_eq!(
        app.world().get::<TextFont>(label).unwrap().font_size,
        bevy::text::FontSize::Px(24.0),
        "§4's large entries are 24px"
    );
    for word in ["Save", "Load", "Timeline", "Settings"] {
        assert_eq!(
            app.world().get::<Text>(text_entity(&app, word)).unwrap().0,
            word,
            "every section entry draws its own copy"
        );
    }
}

/// The contents page's seat gives the gallery its way back, so the surface draws
/// no back control; the title's door draws its own, and either door returns to
/// where it came from (`docs/title-and-menu.md` § Default look §4, § Gallery
/// `### Behavior` §3/§7).
#[test]
fn the_gallery_back_control_is_door_aware() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    assert_eq!(menu(&app).gallery_origin, menu::MenuOrigin::Slots);
    build_pause_tree(&mut app);
    assert!(
        !has_action(&app, MenuAction::GalleryBack),
        "§4's left column is the grid's way back"
    );
    assert!(
        !all_texts(&app)
            .iter()
            .any(|text| text == menu::gallery_back_label(menu::MenuOrigin::Slots)),
        "the surface draws no back label"
    );
    click(&mut app, MenuAction::GalleryBack);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert!(menu(&app).open, "the overlay is still up");

    let mut title = temp_title_app();
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    assert_eq!(menu(&title).gallery_origin, menu::MenuOrigin::Title);
    build_pause_tree(&mut title);
    assert!(
        has_action(&title, MenuAction::GalleryBack),
        "the title's door draws its own way back"
    );
    assert!(all_texts(&title)
        .iter()
        .any(|text| text == "\u{2190} Back to title"));
    click(&mut title, MenuAction::GalleryBack);
    assert_eq!(menu(&title).view, MenuView::Title);
    assert!(menu(&title).title_up());
}

/// `Esc` unwinds exactly one level per press: the gallery to its door, the
/// overlay's grid to closed, and the title's door ends where `Esc` is inert
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §3).
#[test]
fn esc_unwinds_the_gallery_one_level_per_press() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);

    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert!(menu(&app).open, "the first Esc backed out to the grid");
    esc(&mut app);
    assert!(!menu(&app).open, "the second Esc closed the overlay");

    let mut title = temp_title_app();
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    esc(&mut title);
    assert_eq!(menu(&title).view, MenuView::Title);
    assert!(menu(&title).title_up());
    esc(&mut title);
    assert!(
        menu(&title).title_up(),
        "Esc is inert on the title, ending the chain"
    );
}

/// Settings offers no gallery control and the gallery offers no Settings
/// control, from either door (`docs/title-and-menu.md` § Gallery `### Behavior`
/// §1).
#[test]
fn neither_settings_nor_the_gallery_is_a_door_to_the_other() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);

    let entries = |app: &App, action: MenuAction| {
        app.world()
            .iter_entities()
            .filter(|entity| {
                entity.get::<MenuAction>() == Some(&action)
                    && entity.get::<Button>().is_some()
                    && entity
                        .get::<Node>()
                        .is_some_and(|node| node.display != Display::None)
            })
            .count()
    };
    click(&mut app, MenuAction::OpenSettings);
    build_pause_tree(&mut app);
    assert_eq!(
        entries(&app, MenuAction::OpenGallery),
        1,
        "Settings holds only the page's Gallery entry"
    );

    click(&mut app, MenuAction::BackToSlots);
    click(&mut app, MenuAction::OpenGallery);
    build_pause_tree(&mut app);
    assert_eq!(
        entries(&app, MenuAction::OpenSettings),
        1,
        "the gallery holds only the page's Settings entry"
    );

    let mut title = temp_title_app();
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    press_and_update(&mut title, KeyCode::Enter);
    build_pause_tree(&mut title);
    assert!(!has_action(&title, MenuAction::OpenGallery));
    click(&mut title, MenuAction::BackToSlots);
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    build_pause_tree(&mut title);
    assert!(!has_action(&title, MenuAction::OpenSettings));
}

/// The gallery opens at every parked wait and leaves the session unchanged
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §2).
#[test]
fn the_gallery_opens_at_every_parked_wait_leaving_the_session_unchanged() {
    let pack = temp_pack();
    let mut say = app_for_pack(pack.path().join("pack"));
    assert!(matches!(
        say.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Say { .. })
    ));
    let before = parked_position(&say);
    open_overlay_gallery(&mut say);
    assert_eq!(parked_position(&say), before);
    assert!(!autosave_path(&say).exists());

    let pack = temp_pack();
    let mut choice = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut choice, KeyCode::Space);
    press_and_update(&mut choice, KeyCode::Space);
    assert!(matches!(
        choice.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Choice { .. })
    ));
    let before = parked_position(&choice);
    open_overlay_gallery(&mut choice);
    assert_eq!(parked_position(&choice), before);

    let pack = temp_pack();
    let mut ended = app_for_pack(pack.path().join("pack"));
    play_to_end(&mut ended);
    assert!(ended.world().resource::<Runtime>().runner.wait().is_none());
    let before = parked_position(&ended);
    open_overlay_gallery(&mut ended);
    assert_eq!(parked_position(&ended), before);

    let fake = FakeResolver::new(model_answer("warm"));
    let (mut inflight, _dir) = app_parked_on_decision(&fake);
    let before = parked_position(&inflight);
    open_overlay_gallery(&mut inflight);
    assert_eq!(parked_position(&inflight), before);
    assert!(inflight.world().resource::<PendingDecision>().0.is_some());
    assert_eq!(fake.calls(), 1, "the gallery never re-fires or cancels");

    let fake = FakeResolver::new(model_answer("warm"));
    let (mut held_app, _dir) = app_parked_on_decision(&fake);
    esc(&mut held_app);
    fake.release();
    pump_until(&mut held_app, held);
    let before = parked_position(&held_app);
    held_app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut held_app, KeyCode::Enter);
    assert_eq!(menu(&held_app).view, MenuView::Gallery);
    assert!(held(&held_app), "the held answer stays held");
    assert_eq!(parked_position(&held_app), before);
}

/// Opening, walking and closing the gallery writes nothing and reads no slot
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1).
#[test]
fn opening_walking_and_closing_the_gallery_writes_nothing_and_reads_no_slot() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    esc(&mut app);
    let slots_before = menu(&app).slots.clone();

    // A slot appears on disk after the overlay read the list: the gallery must
    // not re-read it.
    {
        let runtime = app.world().resource::<Runtime>();
        let mut save = SaveFile::capture(&runtime.runner, &runtime.staging);
        save.saved_at = Some(1);
        save::write_slot(runtime.runner.pack(), Slot::Manual(2), &save).unwrap();
    }
    let settings = settings_path(&app);

    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert_eq!(menu(&app).slots, slots_before, "opening reads no slot");

    // Walk the top bar and the grid: moving focus writes nothing either.
    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab);
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(1));

    click(&mut app, MenuAction::GalleryBack);
    assert_eq!(menu(&app).view, MenuView::Slots);
    assert_eq!(menu(&app).slots, slots_before, "closing reads no slot");
    esc(&mut app);
    assert!(!menu(&app).open);

    assert!(!settings.exists(), "the gallery never flushes settings");
    assert!(
        !autosave_path(&app).exists(),
        "the gallery writes no autosave"
    );
}

/// A pack that declares no entries draws the tree's heading and its single
/// `No images in this pack` line, with no grid, no legend, no error or Notice,
/// from either door (`docs/title-and-menu.md` § Gallery `### Behavior` §6/§7,
/// `### Layout` §1).
#[test]
fn an_empty_pack_draws_the_header_and_the_single_empty_line() {
    let dir = temp_pack_without_gallery_entries();
    let mut app = app_for_pack(dir.path().join("pack"));
    esc(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    build_pause_tree(&mut app);

    let texts = all_texts(&app);
    assert!(
        texts.iter().any(|text| text == menu::GALLERY_EMPTY),
        "the empty line renders: {texts:?}"
    );
    assert!(texts.iter().any(|text| text == menu::GALLERY_HEADING));
    assert!(
        !has_action(&app, MenuAction::GalleryCard(0)),
        "no grid draws"
    );
    assert_eq!(
        app.world()
            .get::<Node>(gallery_text(&app, menu::GALLERY_GRID_LEGEND))
            .unwrap()
            .display,
        Display::None,
        "the legend is drawn only when the pack has art"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "the empty state raises no Notice"
    );
    let empty = gallery_text(&app, menu::GALLERY_EMPTY);
    let holder = app
        .world()
        .get::<ChildOf>(empty)
        .and_then(|slot| app.world().get::<ChildOf>(slot.parent()))
        .map(|parent| parent.parent())
        .expect("the empty line sits in the tree's own box");
    let node = app.world().get::<Node>(holder).unwrap();
    assert_eq!(
        (node.justify_content, node.align_items),
        (JustifyContent::Center, AlignItems::Center),
        "`No images in this pack` is centred where the grid would be"
    );
    assert_eq!(node.flex_grow, 1.0, "the box fills the body's height");

    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    assert_eq!(
        face_handle(&app, gallery_text(&app, menu::GALLERY_HEADING)),
        faces.sans_semibold,
        "the heading is sans SemiBold"
    );
    assert_eq!(
        face_handle(&app, gallery_text(&app, menu::GALLERY_EMPTY)),
        faces.sans_regular,
        "the empty-state line is sans prose"
    );

    let dir = temp_pack_without_gallery_entries();
    let mut title = boot_app(dir.path().join("pack"), None);
    title.insert_resource(TempPack(dir));
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    build_pause_tree(&mut title);
    let texts = all_texts(&title);
    assert!(
        texts.iter().any(|text| text == menu::GALLERY_EMPTY),
        "the empty line renders from the title's door: {texts:?}"
    );
    assert!(
        !has_action(&title, MenuAction::GalleryCard(0)),
        "no grid draws"
    );
}

/// The card a gallery arrow move lands on for each direction and boundary. The
/// walk does not wrap; `↑`/`↓` step one row and clamp to the last entry when
/// the destination row is short (`docs/title-and-menu.md` § Gallery
/// `### Layout` §5).
#[test]
fn gallery_move_steps_one_card_or_row_and_clamps_without_wrapping() {
    use menu::GalleryDir::{Down, Left, Right, Up};

    // A three-column grid of seven entries: rows 0-2 / 3-5 / 6.
    assert_eq!(menu::gallery_move(0, 7, 3, Right), Some(1));
    assert_eq!(menu::gallery_move(6, 7, 3, Left), Some(5));
    assert_eq!(
        menu::gallery_move(0, 7, 3, Left),
        None,
        "no wrap at the start"
    );
    assert_eq!(
        menu::gallery_move(6, 7, 3, Right),
        None,
        "no wrap at the end"
    );
    assert_eq!(menu::gallery_move(0, 7, 3, Down), Some(3), "one row down");
    assert_eq!(menu::gallery_move(3, 7, 3, Down), Some(6));
    assert_eq!(
        menu::gallery_move(4, 7, 3, Down),
        Some(6),
        "a short destination row clamps to the last entry"
    );
    assert_eq!(menu::gallery_move(2, 7, 3, Down), Some(5));
    assert_eq!(
        menu::gallery_move(6, 7, 3, Down),
        None,
        "the last card's down is a no-op"
    );
    assert_eq!(menu::gallery_move(3, 7, 3, Up), Some(0));
    assert_eq!(menu::gallery_move(6, 7, 3, Up), Some(3));
    assert_eq!(
        menu::gallery_move(0, 7, 3, Up),
        None,
        "the first card's up is a no-op"
    );

    // Degenerate inputs never panic or invent a card.
    assert_eq!(menu::gallery_move(0, 0, 3, Down), None);
    assert_eq!(
        menu::gallery_move(0, 7, 0, Down),
        Some(1),
        "a zero column count steps one"
    );
    assert_eq!(
        menu::gallery_move(9, 7, 3, Down),
        None,
        "an out-of-range focus is inert"
    );
}

/// The grid packs `minmax(240px, 1fr)` tracks with 20px gutters: three across the
/// contents page's 795px right column at 1280×720, and four across the wider
/// title door (`docs/title-and-menu.md` § Default look §4, § Gallery
/// `### Layout` §4).
#[test]
fn gallery_column_count_is_three_across_at_the_reference_window() {
    let page_seat = 1280.0 - 421.0 - 2.0 * 32.0;
    assert_eq!(
        crate::presentation::gallery::gallery_column_count(page_seat),
        3,
        "795px fits three 240px tracks and their two gutters"
    );
    // The title door's surface is the window less its 32px padding.
    assert_eq!(
        crate::presentation::gallery::gallery_column_count(1280.0 - 64.0),
        4,
        "the wider seat packs a fourth track"
    );
    // A fullscreen-width surface packs more columns.
    assert_eq!(
        crate::presentation::gallery::gallery_column_count(1856.0),
        7
    );
    assert_eq!(crate::presentation::gallery::gallery_column_count(0.0), 1);
}

/// The column count is read from the grid's laid-out width in logical pixels,
/// so a HiDPI scale factor cannot shift the walk's row step.
#[test]
fn sync_gallery_columns_reads_the_grids_logical_width() {
    let mut app = App::new();
    app.init_resource::<MenuState>();
    app.world_mut().spawn((
        crate::presentation::gallery::GalleryGrid,
        ComputedNode {
            size: Vec2::new(2432.0, 0.0),
            inverse_scale_factor: 0.5,
            ..default()
        },
    ));
    app.add_systems(Update, crate::presentation::gallery::sync_gallery_columns);
    app.update();
    assert_eq!(
        menu(&app).gallery_columns,
        4,
        "2432 physical at 2x is 1216 logical: four 240px tracks"
    );
}

/// The entity `entity` hangs under.
fn parent_of(app: &App, entity: Entity) -> Entity {
    app.world()
        .get::<ChildOf>(entity)
        .map(ChildOf::parent)
        .expect("the entity has a parent")
}

/// An entity's children, in paint order.
fn children_of(app: &App, entity: Entity) -> Vec<Entity> {
    app.world()
        .get::<Children>(entity)
        .map(|children| children.iter().collect())
        .unwrap_or_default()
}

/// The gallery surface's root: the plugin tree's root, which the seat mounts.
fn gallery_root(app: &App) -> Entity {
    parent_of(app, gallery_body(app))
}

/// The text node `words` inside the gallery surface's subtree, so a word the page
/// also draws — `Gallery` is its section entry too — is told apart from the
/// surface's own.
fn gallery_text(app: &App, words: &str) -> Entity {
    let mut stack = vec![gallery_root(app)];
    while let Some(entity) = stack.pop() {
        if app
            .world()
            .get::<Text>(entity)
            .is_some_and(|text| text.0 == words)
        {
            return entity;
        }
        stack.extend(children_of(app, entity));
    }
    panic!("no text {words:?} inside the gallery surface");
}

/// The card panel one card draws: the tree's item box, which holds the art box,
/// the label's slot and the control that covers them
/// (`docs/title-and-menu.md` § Gallery `### Layout` §2).
fn card_panel(app: &App, index: usize) -> Entity {
    parent_of(app, action_entity(app, MenuAction::GalleryCard(index)))
}

/// A gallery card's 4:3 art box.
fn gallery_art_box(app: &App, index: usize) -> Entity {
    children_of(app, card_panel(app, index))
        .into_iter()
        .find(|child| {
            app.world()
                .get::<crate::presentation::gallery::GalleryArtBox>(*child)
                .is_some()
        })
        .expect("the card carries an art box")
}

/// The built gallery body's scroll container.
fn gallery_body(app: &App) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::gallery::GalleryBody>()
                .map(|_| entity.id())
        })
        .expect("the gallery body draws")
}

/// The built gallery body's current scroll offset.
fn gallery_body_scroll(app: &App) -> Vec2 {
    app.world()
        .get::<ScrollPosition>(gallery_body(app))
        .expect("the gallery body carries a ScrollPosition")
        .0
}

/// The entry index of one gallery label for the running pack.
fn entry_index(app: &App, label: &str) -> usize {
    let runtime = app.world().resource::<Runtime>();
    runtime
        .runner
        .pack()
        .gallery_entries()
        .iter()
        .position(|entry| entry.label() == label)
        .unwrap_or_else(|| panic!("no gallery entry {label:?}"))
}

/// A demo-pack copy with extra declared backgrounds, so the gallery grid has
/// enough cards to scroll and walk multiple rows.
fn temp_pack_with_backgrounds(names: &[&str]) -> tempfile::TempDir {
    let dir = temp_pack();
    let path = dir.path().join("pack/scenes/first_meeting.unmei");
    let scene = std::fs::read_to_string(&path).unwrap();
    let mut text = String::new();
    for name in names {
        text.push_str(&format!("bg {name}\n"));
    }
    text.push_str(&scene);
    std::fs::write(&path, text).unwrap();
    dir
}

/// The gallery body is a scroll container over a `minmax(240px, 1fr)` grid with
/// a 20px gap and one framed card per entry, each with a 4:3 art box
/// (`docs/title-and-menu.md` § Default look §4, § Gallery `### Layout`
/// §1/§2/§4).
#[test]
fn the_gallery_grid_is_three_across_with_a_twenty_pixel_gap() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    let body = gallery_body(&app);
    let body_node = app.world().get::<Node>(body).unwrap();
    assert_eq!(body_node.overflow, Overflow::scroll_y());
    assert_eq!(body_node.min_height, Val::Px(0.0));
    // The body lets the grid keep its natural height: a cross-axis stretched or
    // shrunk grid hides its overflow, so the focused-card scroll can never move
    // (`docs/title-and-menu.md` § Gallery `### Layout` §1/§4).
    assert_eq!(body_node.flex_direction, FlexDirection::Column);
    assert_eq!(body_node.align_items, AlignItems::Start);
    assert!(
        app.world().get::<ScrollArea>(body).is_some(),
        "the body is a scroll area"
    );

    let grid = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::gallery::GalleryGrid>()
                .map(|_| entity.id())
        })
        .expect("the grid draws");
    let node = app.world().get::<Node>(grid).unwrap();
    assert_eq!(node.display, Display::Grid);
    assert_eq!(node.column_gap, Val::Px(20.0));
    assert_eq!(node.row_gap, Val::Px(20.0));
    let expected: Vec<RepeatedGridTrack> = RepeatedGridTrack::minmax(
        GridTrackRepetition::AutoFill,
        MinTrackSizingFunction::Px(240.0),
        MaxTrackSizingFunction::Fraction(1.0),
    );
    assert_eq!(node.grid_template_columns, expected);
    assert_eq!(
        node.flex_shrink, 0.0,
        "the grid keeps its natural height inside the scrolling body"
    );
    assert_eq!(
        crate::presentation::gallery::gallery_column_count(1280.0 - 421.0 - 2.0 * 32.0),
        3,
        "§4's 795px right column fits three tracks"
    );

    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    for (index, label) in ["school_gate", "ace \u{b7} happy", "ace \u{b7} neutral"]
        .iter()
        .enumerate()
    {
        assert!(has_action(&app, MenuAction::GalleryCard(index)));
        let art = gallery_art_box(&app, index);
        assert_eq!(
            app.world().get::<Node>(art).unwrap().aspect_ratio,
            Some(4.0 / 3.0)
        );
        assert!(
            all_texts(&app).iter().any(|text| text == label),
            "the {label:?} card label renders"
        );
        assert_eq!(
            face_handle(&app, text_entity(&app, label)),
            faces.mono_medium,
            "the {label:?} card label is mono"
        );
    }
}

/// The title's door draws the same `minmax(240px, 1fr)` auto-fill grid with a
/// 20px gap as the contents page's, on the wider seat its own root leaves: the
/// door changes the backdrop, the seat and the way back, never the grid
/// (`docs/title-and-menu.md` § Default look §4, § Gallery `### Layout` §1/§4).
#[test]
fn the_title_doors_grid_is_the_same_auto_fill_layout() {
    let pack = temp_pack();
    let mut title = boot_app(pack.path().join("pack"), None);
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    assert_eq!(menu(&title).view, MenuView::Gallery);
    build_pause_tree(&mut title);

    let grid = title
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::gallery::GalleryGrid>()
                .map(|_| entity.id())
        })
        .expect("the grid draws from the title's door");
    let node = title.world().get::<Node>(grid).unwrap();
    assert_eq!(node.display, Display::Grid);
    assert_eq!(node.column_gap, Val::Px(20.0));
    assert_eq!(node.row_gap, Val::Px(20.0));
    let expected: Vec<RepeatedGridTrack> = RepeatedGridTrack::minmax(
        GridTrackRepetition::AutoFill,
        MinTrackSizingFunction::Px(240.0),
        MaxTrackSizingFunction::Fraction(1.0),
    );
    assert_eq!(node.grid_template_columns, expected);
    assert_eq!(
        node.flex_shrink, 0.0,
        "the grid keeps its natural height inside the scrolling body"
    );
    let title_body = title.world().get::<Node>(gallery_body(&title)).unwrap();
    assert_eq!(
        title_body.overflow,
        Overflow::scroll_y(),
        "the body scrolls from the title's door too"
    );
    assert_eq!(title_body.flex_direction, FlexDirection::Column);
    assert_eq!(title_body.align_items, AlignItems::Start);

    assert_eq!(
        crate::presentation::gallery::gallery_column_count(1280.0 - 64.0),
        4
    );
    assert_eq!(
        crate::presentation::gallery::gallery_column_count(1280.0 - 421.0 - 2.0 * 32.0),
        3
    );
}

/// The gallery reuses the overlay's card vocabulary — the slot card's fill and
/// its 2px border — with the entry's label as a footer line under the art, and
/// the cards stand in the pack's entry order, so the walk follows it
/// (`docs/title-and-menu.md` § Gallery `### Layout` §2, § Entries §3,
/// § Menu overlay §1).
#[test]
fn gallery_cards_draw_the_card_panel_in_entry_order() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    let entries = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries();
    for (index, entry) in entries.iter().enumerate() {
        let card = card_panel(&app, index);
        assert_eq!(
            *app.world().get::<BackgroundColor>(card).unwrap(),
            BackgroundColor(Color::srgb(0.12, 0.13, 0.17)),
            "card {index} draws the card fill"
        );
        assert_eq!(
            app.world().get::<Node>(card).unwrap().border,
            UiRect::all(Val::Px(2.0)),
            "card {index} reserves its 2px focus ring"
        );
        let children = children_of(&app, card);
        let art = children
            .iter()
            .position(|child| {
                app.world()
                    .get::<crate::presentation::gallery::GalleryArtBox>(*child)
                    .is_some()
            })
            .expect("the card carries its 4:3 art box");
        let footer = children
            .iter()
            .position(|child| {
                children_of(&app, *child)
                    .iter()
                    .any(|line| app.world().get::<Text>(*line).is_some())
            })
            .expect("the card carries its footer label");
        assert!(
            art < footer,
            "card {index}'s label is a footer line under the art"
        );
        let label = flattened_text(&app, children[footer]);
        assert_eq!(
            label,
            entry.label(),
            "card {index} shows the entry it stands for"
        );
    }
}

/// `sync_gallery_art` sizes a loaded texture contain-fit inside the 4:3 box:
/// the free axis letterboxes and the node keeps the image's aspect
/// (`docs/title-and-menu.md` § Gallery `### Layout` §3).
#[test]
fn sync_gallery_art_contain_fits_a_loaded_texture() {
    let mut app = App::new();
    app.init_resource::<Assets<Image>>();
    let art_box = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryArtBox,
            Outline::new(Val::Px(1.0), Val::Px(0.0), Color::WHITE),
        ))
        .id();
    let background = add_test_image(&mut app, 1280, 720);
    let sprite = add_test_image(&mut app, 256, 512);

    // A 16:9 background fills the width and letterboxes to 75% height; a 1:2
    // sprite fills the height and pillarboxes to 37.5% width.
    let wide = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryArt,
            ImageNode::new(background),
            Node::default(),
            ChildOf(art_box),
        ))
        .id();
    let tall = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryArt,
            ImageNode::new(sprite),
            Node::default(),
            ChildOf(art_box),
        ))
        .id();
    app.add_systems(Update, crate::presentation::gallery::sync_gallery_art);
    app.update();

    assert_eq!(
        app.world().get::<Node>(wide).unwrap().width,
        Val::Percent(100.0)
    );
    assert_eq!(
        app.world().get::<Node>(wide).unwrap().height,
        Val::Percent(75.0)
    );
    assert_eq!(
        app.world().get::<Node>(tall).unwrap().width,
        Val::Percent(37.5)
    );
    assert_eq!(
        app.world().get::<Node>(tall).unwrap().height,
        Val::Percent(100.0)
    );
    assert_eq!(
        app.world().get::<Outline>(art_box).unwrap().color,
        Color::NONE,
        "once a texture decodes the placeholder frame clears"
    );
}

/// Insert an in-memory image of a given size into the app's `Assets<Image>`.
fn add_test_image(app: &mut App, width: u32, height: u32) -> Handle<Image> {
    let mut image = Image::default();
    image.texture_descriptor.size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    app.world_mut().resource_mut::<Assets<Image>>().add(image)
}

/// Focus is the overlay's 2px `FOCUS` border and nothing else: the focused
/// card's ring and an unfocused one share every other property, and focus starts
/// on the first card (`docs/title-and-menu.md` § Gallery `### Layout` §5).
#[test]
fn the_focused_gallery_card_differs_only_by_its_focus_border() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);
    app.update();

    let focused = action_entity(&app, MenuAction::GalleryCard(0));
    let plain = action_entity(&app, MenuAction::GalleryCard(1));
    let focus_color = app.world().get::<BorderColor>(focused).unwrap().top;
    assert_ne!(focus_color, Color::NONE, "the first card starts focused");
    assert_eq!(
        app.world().get::<BorderColor>(plain).unwrap().top,
        Color::NONE
    );
    let first = card_panel(&app, 0);
    let second = card_panel(&app, 1);
    assert_eq!(
        app.world().get::<Node>(first).unwrap(),
        app.world().get::<Node>(second).unwrap(),
        "focus changes no size or spacing"
    );
    assert_eq!(
        app.world().get::<BackgroundColor>(first).unwrap(),
        app.world().get::<BackgroundColor>(second).unwrap(),
        "focus changes no fill or brightness"
    );

    press_and_update(&mut app, KeyCode::ArrowRight);
    app.update();
    assert_eq!(
        app.world().get::<BorderColor>(plain).unwrap().top,
        focus_color,
        "the moved-to card takes the ring"
    );
    assert_eq!(
        app.world().get::<BorderColor>(focused).unwrap().top,
        Color::NONE,
        "the left card drops the ring"
    );
    assert_eq!(
        app.world().get::<Node>(first).unwrap(),
        app.world().get::<Node>(second).unwrap(),
        "the move still changes nothing but the border colour"
    );
}

/// The grid legend renders exactly the spec's string, right-aligned by the tree's
/// heading row and mono, and only when entries exist
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §7, `### Layout` §1).
#[test]
fn the_gallery_grid_legend_renders_right_aligned_and_mono() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    let legend = gallery_text(&app, menu::GALLERY_GRID_LEGEND);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    assert_eq!(face_handle(&app, legend), faces.mono_medium);
    assert_eq!(
        face_handle(&app, gallery_text(&app, menu::GALLERY_HEADING)),
        faces.sans_semibold,
        "the heading keeps the Settings panel title's face"
    );

    let header = parent_of(&app, legend);
    let children = children_of(&app, header);
    assert_eq!(
        children.last().copied(),
        Some(legend),
        "the legend is the row's last node"
    );
    assert_eq!(
        app.world().get::<Node>(header).unwrap().justify_content,
        JustifyContent::SpaceBetween,
        "the row right-aligns the legend"
    );

    // The header never scrolls: it is not inside the body.
    let body = gallery_body(&app);
    let mut ancestor = app.world().get::<ChildOf>(legend).map(ChildOf::parent);
    while let Some(entity) = ancestor {
        assert_ne!(entity, body, "the header sits outside the scroll body");
        ancestor = app.world().get::<ChildOf>(entity).map(ChildOf::parent);
    }

    let dir = temp_pack_without_gallery_entries();
    let mut empty = app_for_pack(dir.path().join("pack"));
    esc(&mut empty);
    empty.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut empty, KeyCode::Enter);
    build_pause_tree(&mut empty);
    assert_eq!(
        empty
            .world()
            .get::<Node>(gallery_text(&empty, menu::GALLERY_GRID_LEGEND))
            .unwrap()
            .display,
        Display::None,
        "the empty state shows no grid legend"
    );
}

/// Focus starts on the first card at every open, and the arrows walk the grid:
/// `←`/`→` step one card, `↑`/`↓` step one row and clamp on a short row, the
/// walk does not wrap and digits do nothing (`docs/title-and-menu.md`
/// § Gallery `### Behavior` §4, `### Layout` §5).
#[test]
fn the_gallery_walk_starts_on_the_first_card_and_does_not_wrap() {
    // Seven entries at three columns: rows 0-2 / 3-5 / 6.
    let dir = temp_pack_with_backgrounds(&["g1", "g2", "g3", "g4"]);
    let mut app = app_for_pack(dir.path().join("pack"));
    open_overlay_gallery(&mut app);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(0),
        "focus starts on the first card"
    );

    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(1));
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(0),
        "no wrap at the start"
    );

    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(3));
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(6));
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::Resume,
        "past the last card the page's foot follows"
    );
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(6));

    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(3));
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(0),
        "no wrap at the start"
    );

    // A short destination row clamps: 4 (row 1, col 1) down lands on 6, and
    // 2 (row 0, col 2) down lands on the row's 5.
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryCard(4);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(6));
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryCard(2);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(5));

    let before = menu(&app).focus;
    press_and_update(&mut app, KeyCode::Digit1);
    assert_eq!(menu(&app).focus, before, "a digit does nothing");

    click(&mut app, MenuAction::GalleryBack);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(0),
        "every open starts on the first card"
    );

    let dir = temp_pack_with_backgrounds(&["g1", "g2", "g3", "g4"]);
    let mut title = boot_app(dir.path().join("pack"), None);
    title.insert_resource(TempPack(dir));
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    assert_eq!(menu(&title).view, MenuView::Gallery);
    assert_eq!(menu(&title).focus, MenuFocus::GalleryCard(0));
}

/// The page's foot trails the grid: `↓` from the last card steps to Resume
/// reading and then Exit to title, `↑` walks back to that card, both ends clamp,
/// `←`/`→` stay inside the grid, and the foot's two stops do what they do
/// everywhere (`docs/title-and-menu.md` § Default look §4).
#[test]
fn the_walk_past_the_last_card_reaches_the_page_s_foot() {
    let dir = temp_pack_with_backgrounds(&["g1", "g2", "g3", "g4"]);
    let mut app = app_for_pack(dir.path().join("pack"));
    open_overlay_gallery(&mut app);
    let last = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()
        .len()
        - 1;
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryCard(last);

    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::Resume,
        "the foot comes after the last card"
    );
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(menu(&app).focus, MenuFocus::ExitToTitle);
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::ExitToTitle,
        "the foot's end clamps"
    );
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::ExitToTitle,
        "← stays out of the grid"
    );
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::ExitToTitle);

    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(menu(&app).focus, MenuFocus::Resume);
    press_and_update(&mut app, KeyCode::ArrowUp);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(last),
        "↑ returns to the last card"
    );

    press_and_update(&mut app, KeyCode::ArrowDown);
    press_and_update(&mut app, KeyCode::ArrowDown);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).confirm,
        Some(ConfirmKind::ExitToTitle),
        "the foot's second stop asks before leaving"
    );
    press_and_update(&mut app, KeyCode::Escape);
    assert_eq!(menu(&app).confirm, None);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::ExitToTitle,
        "cancelling returns focus to the foot"
    );
    assert_eq!(menu(&app).view, MenuView::Gallery, "the gallery stays up");

    press_and_update(&mut app, KeyCode::ArrowUp);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(!menu(&app).open, "Resume reading closes the page");
}

/// The label of the entry the still is showing, from the runtime pack's entry
/// order and the menu's focused card.
fn still_label(app: &App) -> String {
    let index = match menu(app).focus {
        MenuFocus::GalleryCard(index) => index,
        other => panic!("the still's focus is a gallery card, got {other:?}"),
    };
    app.world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()
        .get(index)
        .expect("the focused entry exists")
        .label()
}

/// Open the still from the grid on one entry index through `Enter`.
fn open_still(app: &mut App, index: usize) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryCard(index);
    press_and_update(app, KeyCode::Enter);
    assert!(menu(app).gallery_still, "the still opened");
}

/// One still browse step wraps at both ends; the gallery's entry order is the
/// still's order (`docs/title-and-menu.md` § Gallery `### Behavior` §5).
#[test]
fn gallery_browse_steps_and_wraps_at_both_ends() {
    assert_eq!(menu::gallery_browse(1, 3, 1), Some(2));
    assert_eq!(menu::gallery_browse(1, 3, -1), Some(0));
    assert_eq!(menu::gallery_browse(2, 3, 1), Some(0), "wraps forward");
    assert_eq!(menu::gallery_browse(0, 3, -1), Some(2), "wraps back");
    assert_eq!(menu::gallery_browse(0, 0, 1), None, "no entries, no step");
}

/// `Enter`, `Space` and a click each open the still on the focused entry, from
/// either door (`docs/title-and-menu.md` § Gallery `### Behavior` §4/§5/§8).
#[test]
fn enter_space_and_click_each_open_the_still_for_the_focused_entry() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);

    press_and_update(&mut app, KeyCode::Enter);
    assert!(menu(&app).gallery_still, "Enter opens the still");
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
    build_pause_tree(&mut app);
    let caption = still_label(&app);
    assert!(
        all_texts(&app).iter().any(|text| text == &caption),
        "the still caption renders: {caption:?}"
    );

    esc(&mut app);
    assert!(!menu(&app).gallery_still, "Esc closes the still");
    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(1));
    press_and_update(&mut app, KeyCode::Space);
    assert!(menu(&app).gallery_still, "Space opens the still");
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(1));

    esc(&mut app);
    click(&mut app, MenuAction::GalleryCard(2));
    assert!(menu(&app).gallery_still, "a click opens the still");
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(2));

    let mut title = boot_app(pack.path().join("pack"), None);
    title.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut title, KeyCode::Enter);
    assert_eq!(menu(&title).view, MenuView::Gallery);
    press_and_update(&mut title, KeyCode::Enter);
    assert!(
        menu(&title).gallery_still,
        "the title's door opens the still"
    );
    assert!(menu(&title).gallery_origin == menu::MenuOrigin::Title);
}

/// The still's caption is the entry label verbatim in the mono face, the still
/// legend renders exactly the spec's string, and the caption sits centred below
/// the art box in the dim tone with the legend in the faint tone, never over the
/// art (`docs/title-and-menu.md` § Gallery `### Behavior` §5/§7, `### Layout`
/// §6).
#[test]
fn the_still_caption_is_the_label_verbatim_mono_with_the_legend_under_it() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    // Capture the grid's own label for the entry the still will show, before
    // the still replaces the grid.
    build_pause_tree(&mut app);
    let card_label = flattened_text(&app, card_panel(&app, 1));
    open_still(&mut app, 1);
    build_pause_tree(&mut app);

    let label = still_label(&app);
    let caption = text_entity(&app, &label);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    assert_eq!(
        face_handle(&app, caption),
        faces.mono_medium,
        "the still caption is mono"
    );
    assert_eq!(
        card_label, label,
        "the still's caption is the card's label, verbatim"
    );
    let legend = text_entity(&app, menu::GALLERY_STILL_LEGEND);
    assert_eq!(
        face_handle(&app, legend),
        faces.mono_medium,
        "the still legend is mono"
    );

    // The foot strip holding the caption is a sibling after the art box in the
    // still column, so the caption is never drawn over the art.
    let foot = parent_of(&app, parent_of(&app, caption));
    let still = parent_of(&app, foot);
    let children = app.world().get::<Children>(still).unwrap();
    let art_index = children
        .iter()
        .position(|child| {
            app.world()
                .get::<crate::presentation::gallery::GalleryStillArtBox>(child)
                .is_some()
        })
        .expect("the still carries an art box");
    let foot_index = children
        .iter()
        .position(|child| child == foot)
        .expect("the foot strip is in the still column");
    assert!(art_index < foot_index, "the caption sits under the art");

    // The foot strip is a centred column: the caption and the legend are
    // centred under the image (`docs/title-and-menu.md` § Gallery `### Layout`
    // §6).
    let foot_node = app.world().get::<Node>(foot).unwrap();
    assert_eq!(foot_node.flex_direction, FlexDirection::Column);
    assert_eq!(foot_node.align_items, AlignItems::Center);

    // The caption is the dim tone and the legend the faint tone
    // (`docs/title-and-menu.md` § Gallery `### Layout` §6).
    assert_eq!(
        app.world().get::<TextColor>(caption).unwrap().0,
        Color::srgba(0.80, 0.80, 0.85, 0.62),
        "the still caption is the dim tone"
    );
    assert_eq!(
        app.world().get::<TextColor>(legend).unwrap().0,
        Color::srgba(0.80, 0.80, 0.85, 0.35),
        "the still legend is the faint tone"
    );
}

/// The still draws its art verbatim over the door's backdrop and scrim: the
/// surface and the art box carry no fill of their own, so a transparent sprite
/// stays transparent and no checkerboard is drawn
/// (`docs/title-and-menu.md` § Gallery `### Layout` §3/§6).
#[test]
fn the_still_surface_and_art_box_carry_no_fill() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    open_still(&mut app, 0);
    build_pause_tree(&mut app);

    let still = action_entity(&app, MenuAction::GalleryStillClose);
    assert_eq!(
        app.world().get::<BackgroundColor>(still).unwrap().0,
        Color::NONE,
        "the still surface lets the backdrop show through"
    );
    let box_entity = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::gallery::GalleryStillArtBox>()
                .map(|_| entity.id())
        })
        .expect("the still art box draws");
    assert_eq!(
        app.world().get::<BackgroundColor>(box_entity).unwrap().0,
        Color::NONE,
        "the letterbox shows the backdrop, not a fill"
    );
    let art = app
        .world()
        .get::<Children>(box_entity)
        .unwrap()
        .iter()
        .find(|child| {
            app.world()
                .get::<crate::presentation::gallery::GalleryStillArt>(*child)
                .is_some()
        })
        .expect("the art box carries an image node");
    assert_eq!(
        app.world().get::<ImageNode>(art).unwrap().color,
        Color::WHITE,
        "the art is drawn verbatim, with no tint"
    );
}

/// `fit_gallery_still` contain-fits the image into the art box above the foot
/// strip: both axes fit, the aspect is preserved, and the free axis letterboxes
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §8, `### Layout` §3/§6).
#[test]
fn fit_gallery_still_contain_fits_the_image_above_the_foot_strip() {
    let mut app = App::new();
    app.init_resource::<Assets<Image>>();
    let box_size = Vec2::new(800.0, 700.0);
    let art_box = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryStillArtBox,
            ComputedNode {
                size: box_size,
                ..default()
            },
            Outline::new(Val::Px(1.0), Val::Px(0.0), Color::WHITE),
        ))
        .id();
    let wide = add_test_image(&mut app, 1280, 720);
    let tall = add_test_image(&mut app, 256, 512);

    let wide_art = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryStillArt { shown: None },
            ImageNode::new(wide),
            Node::default(),
            ChildOf(art_box),
        ))
        .id();
    let tall_art = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryStillArt { shown: None },
            ImageNode::new(tall),
            Node::default(),
            ChildOf(art_box),
        ))
        .id();
    app.add_systems(Update, crate::presentation::gallery::fit_gallery_still);
    app.update();

    // A 1280x720 image in the 800x700 box: the width fits and the height
    // letterboxes to 800 * 720 / 1280 = 450.
    let wide_node = app.world().get::<Node>(wide_art).unwrap().clone();
    assert_eq!(wide_node.width, Val::Px(800.0));
    assert_eq!(wide_node.height, Val::Px(450.0));
    // A 256x512 sprite: the height fits and the width pillarboxes to 700 * 256
    // / 512 = 350.
    let tall_node = app.world().get::<Node>(tall_art).unwrap().clone();
    assert_eq!(tall_node.width, Val::Px(350.0));
    assert_eq!(tall_node.height, Val::Px(700.0));

    for (node, natural) in [
        (wide_node, Vec2::new(1280.0, 720.0)),
        (tall_node, Vec2::new(256.0, 512.0)),
    ] {
        let (Val::Px(width), Val::Px(height)) = (node.width, node.height) else {
            panic!("the still art is sized in pixels");
        };
        assert!(width <= box_size.x && height <= box_size.y, "both axes fit");
        assert_eq!(
            width / height,
            natural.x / natural.y,
            "the aspect is preserved"
        );
    }
    assert_eq!(
        app.world().get::<Outline>(art_box).unwrap().color,
        Color::NONE,
        "once a texture decodes the placeholder frame clears"
    );
}

/// The transparent fallback is a resident 1x1 image, so it must not be treated
/// as decoded art: a failed or absent entry keeps the placeholder frame and a
/// zero-sized node instead of stretching the 1x1 to the box
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
#[test]
fn fit_gallery_still_keeps_the_placeholder_for_the_transparent_fallback() {
    let mut app = App::new();
    app.init_resource::<Assets<Image>>();
    // The real app's `ImagePlugin` inserts the transparent handle as a decoded
    // asset; insert it here so the sentinel cannot pass as art.
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .insert(&bevy::image::TRANSPARENT_IMAGE_HANDLE, Image::transparent())
        .unwrap();
    let art_box = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryStillArtBox,
            ComputedNode {
                size: Vec2::new(800.0, 700.0),
                ..default()
            },
            Outline::new(Val::Px(1.0), Val::Px(0.0), Color::WHITE),
        ))
        .id();
    let art = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryStillArt { shown: Some(0) },
            ImageNode::new(bevy::image::TRANSPARENT_IMAGE_HANDLE),
            Node {
                width: Val::Px(0.0),
                height: Val::Px(0.0),
                ..default()
            },
            ChildOf(art_box),
        ))
        .id();
    app.add_systems(Update, crate::presentation::gallery::fit_gallery_still);
    app.update();

    assert_eq!(
        app.world().get::<Node>(art).unwrap().width,
        Val::Px(0.0),
        "the transparent fallback is not sized as art"
    );
    assert_ne!(
        app.world().get::<Outline>(art_box).unwrap().color,
        Color::NONE,
        "the placeholder frame stands for absent art"
    );
}

/// The still is modal: the page's controls stand but are inert, so a Save/Load
/// entry or Resume does nothing while the still is up — the mode, the focus and
/// the shown entry cannot drift apart
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §4/§5).
#[test]
fn the_page_s_controls_are_inert_while_the_still_is_up() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    open_still(&mut app, 1);
    let before = menu(&app).focus;

    click(&mut app, MenuAction::SetMode(MenuMode::Load));
    assert!(menu(&app).gallery_still, "a Save/Load tab does not leave");
    assert_eq!(menu(&app).mode, MenuMode::Save, "the mode does not switch");
    assert_eq!(menu(&app).focus, before, "the shown entry does not move");

    click(&mut app, MenuAction::Resume);
    assert!(menu(&app).gallery_still, "Resume does not close the still");
    assert!(menu(&app).open, "Resume does not close the overlay");
    assert_eq!(
        menu(&app).focus,
        before,
        "the shown entry still does not move"
    );

    click(&mut app, MenuAction::GalleryStillClose);
    assert!(!menu(&app).gallery_still, "the still's own click closes it");
}

/// `←`/`→` browse the entry order cyclically; the caption and the grid's focus
/// follow the step, so closing returns to the entry that was shown
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §4/§5).
#[test]
fn the_still_browses_cyclically_and_the_caption_and_grid_focus_follow() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    let count = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()
        .len();
    assert_eq!(count, 3, "the demo pack's three entries");
    open_overlay_gallery(&mut app);
    open_still(&mut app, 0);
    build_pause_tree(&mut app);
    sync_still(&mut app);
    let first = still_label(&app);

    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(count - 1));
    build_pause_tree(&mut app);
    sync_still(&mut app);
    let last = still_label(&app);
    assert_ne!(last, first);
    assert!(
        all_texts(&app).iter().any(|text| text == &last),
        "the caption follows the step: {last:?}"
    );

    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));

    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(1));
    esc(&mut app);
    assert!(!menu(&app).gallery_still, "closing returns to the grid");
    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(1),
        "the grid's focus follows the step"
    );
}

/// Assert the still closed back to the grid on one entry.
fn assert_still_closed_on(app: &App, index: usize, what: &str) {
    assert!(!menu(app).gallery_still, "{what} closes the still");
    assert_eq!(
        menu(app).view,
        MenuView::Gallery,
        "{what} returns to the grid"
    );
    assert_eq!(
        menu(app).focus,
        MenuFocus::GalleryCard(index),
        "{what} keeps the entry that was shown"
    );
}

/// `↑`, `↓`, `Esc` and a click each close the still back to the grid, focused on
/// the entry it was showing (`docs/title-and-menu.md` § Gallery `### Behavior`
/// §5/§8).
#[test]
fn up_down_esc_and_click_each_close_the_still_on_that_entry() {
    let pack = temp_pack();

    let mut up = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut up);
    open_still(&mut up, 1);
    press_and_update(&mut up, KeyCode::ArrowUp);
    assert_still_closed_on(&up, 1, "Up");

    let mut down = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut down);
    open_still(&mut down, 1);
    press_and_update(&mut down, KeyCode::ArrowDown);
    assert_still_closed_on(&down, 1, "Down");

    let mut escape = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut escape);
    open_still(&mut escape, 1);
    esc(&mut escape);
    assert_still_closed_on(&escape, 1, "Esc");

    let mut clicked = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut clicked);
    open_still(&mut clicked, 1);
    click(&mut clicked, MenuAction::GalleryStillClose);
    assert_still_closed_on(&clicked, 1, "a click");
}

/// The still is a hold, not a second menu: no input other than `←`/`→`/`↑`/`↓`,
/// `Esc` and a click does anything (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §5).
#[test]
fn no_other_input_moves_the_still() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    open_still(&mut app, 1);
    let before = menu(&app).focus;

    for key in [
        KeyCode::Tab,
        KeyCode::Digit1,
        KeyCode::Enter,
        KeyCode::Space,
    ] {
        press_and_update(&mut app, key);
        assert!(menu(&app).gallery_still, "{key:?} leaves the still up");
        assert_eq!(menu(&app).focus, before, "{key:?} moves nothing");
    }
    assert_eq!(
        menu(&app).mode,
        MenuMode::Save,
        "Tab does not flip the mode"
    );
    assert_eq!(menu(&app).view, MenuView::Gallery);
}

/// Stepping the still changes only the image and the caption: the door's
/// backdrop and scrim stand as entities behind it, and the still calls no
/// runner method and writes no slot, autosave or settings file
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §5/§6, `### Layout`
/// §6/§7).
#[test]
fn stepping_the_still_leaves_the_backdrop_scrim_and_session_unchanged() {
    let mut app = gallery_asset_app(&[]);
    open_title_gallery(&mut app);
    pump_until_watch_drains(&mut app);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(menu(&app).gallery_still, "the still opened");
    // One frame so the presentation builds the still tree (this harness adds no
    // ordering between the input and the UI systems) before the backdrop and
    // scrim entities are captured.
    app.update();

    let backdrop = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::title::TitleBackdropImage>()
                .map(|_| entity.id())
        })
        .expect("the declared backdrop draws");
    let scrim = gallery_scrim(&app).expect("the title door draws the scrim");
    let gradient = scrim_gradient(&app);
    let root = scrim_root(&app);
    let before = child_order(&app, root);
    let parked = parked_position(&app);

    // Step forward and back: the walk crosses both entries' art.
    press_and_update(&mut app, KeyCode::ArrowRight);
    press_and_update(&mut app, KeyCode::ArrowLeft);

    assert!(
        app.world()
            .get::<crate::presentation::title::TitleBackdropImage>(backdrop)
            .is_some(),
        "the same backdrop entity survives the step"
    );
    assert!(
        app.world()
            .get::<crate::presentation::gallery::GalleryScrim>(scrim)
            .is_some(),
        "the same scrim entity survives the step"
    );
    assert_eq!(child_order(&app, root), before, "the tree is not rebuilt");
    assert_eq!(scrim_gradient(&app), gradient, "the scrim is unchanged");
    assert_eq!(
        parked_position(&app),
        parked,
        "the still calls no runner method"
    );
    assert!(
        !autosave_path(&app).exists(),
        "the still writes no autosave"
    );
    assert!(
        !settings_path(&app).exists(),
        "the still never flushes settings"
    );
    assert!(
        !slot_path(&app, Slot::Manual(1)).exists(),
        "the still writes no slot file"
    );
}

/// The focused card's scroll-into-view fires on a real focus change only, so a
/// wheel scroll or an idle frame never yanks the view back
/// (`docs/title-and-menu.md` § Gallery `### Layout` §4).
#[derive(Resource, Default)]
struct GalleryScrolls(Vec<Entity>);

fn gallery_scrolls(app: &App) -> &[Entity] {
    app.world().resource::<GalleryScrolls>().0.as_slice()
}

#[test]
fn moving_the_gallery_focus_triggers_scroll_into_view() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);
    app.init_resource::<GalleryScrolls>();
    app.add_observer(
        |scroll: On<bevy::ui_widgets::ScrollIntoView>, mut seen: ResMut<GalleryScrolls>| {
            seen.0.push(scroll.entity);
        },
    );
    app.add_systems(
        Update,
        crate::presentation::gallery::scroll_gallery_focus.after(RuntimeCoreSet),
    );

    app.update();
    assert_eq!(
        gallery_scrolls(&app),
        [action_entity(&app, MenuAction::GalleryCard(0))]
    );
    app.update();
    assert_eq!(
        gallery_scrolls(&app).len(),
        1,
        "an idle frame triggers nothing"
    );

    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        gallery_scrolls(&app).len(),
        2,
        "the new focus reveals itself"
    );
    assert_eq!(
        gallery_scrolls(&app)[1],
        action_entity(&app, MenuAction::GalleryCard(1))
    );
}

/// In a 19-card grid every arrow move asks the body to reveal the card it lands
/// on, so the focused card is never scrolled off-screen
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §4, `### Layout` §4/§8).
#[test]
fn every_arrow_move_in_a_scrolling_grid_reveals_the_focused_card() {
    let extras: Vec<String> = (1..=16).map(|index| format!("g{index:02}")).collect();
    let extras: Vec<&str> = extras.iter().map(String::as_str).collect();
    let dir = temp_pack_with_backgrounds(&extras);
    let mut app = app_for_pack(dir.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);
    app.init_resource::<GalleryScrolls>();
    app.add_observer(
        |scroll: On<bevy::ui_widgets::ScrollIntoView>, mut seen: ResMut<GalleryScrolls>| {
            seen.0.push(scroll.entity);
        },
    );
    app.add_systems(
        Update,
        crate::presentation::gallery::scroll_gallery_focus.after(RuntimeCoreSet),
    );

    let count = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()
        .len();
    assert_eq!(count, 19);
    app.update(); // the initial focus reveals itself

    for key in [
        KeyCode::ArrowRight,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowUp,
    ] {
        let before = gallery_scrolls(&app).len();
        press_and_update(&mut app, key);
        let scrolls = gallery_scrolls(&app);
        assert_eq!(
            scrolls.len(),
            before + 1,
            "{key:?} reveals the newly focused card"
        );
        let focused = match menu(&app).focus {
            MenuFocus::GalleryCard(index) => action_entity(&app, MenuAction::GalleryCard(index)),
            other => panic!("focus left the grid: {other:?}"),
        };
        assert_eq!(
            scrolls[before], focused,
            "{key:?} reveals the card the walk landed on"
        );
    }
}

/// The contents page's grid frame at the reference window, measured from the
/// rendered acceptance screenshots and the documented grid
/// (`docs/title-and-menu.md` § Default look §4, § Gallery `### Layout` §4): a
/// 795x540 body — §4's right column less its padding, the page's heading and the
/// gallery's own heading row — over a three-across grid of 251x220 cards with
/// 20px gaps.
const RIG_VIEWPORT: Vec2 = Vec2::new(795.0, 540.0);

const RIG_CARD: Vec2 = Vec2::new(251.0, 220.0);

const RIG_GAP: f32 = 20.0;

const RIG_COLUMNS: usize = 3;

/// The 19-card grid's content height: seven card rows with a gap between each.
const RIG_CONTENT_HEIGHT: f32 = 7.0 * RIG_CARD.y + 6.0 * RIG_GAP;

/// The 19th card's (index 18) reveal: its bottom edge against the viewport's,
/// the minimal scroll `ScrollIntoView` asks for.
const RIG_CARD_18_REVEAL: f32 = RIG_CONTENT_HEIGHT - RIG_VIEWPORT.y;

/// Stand in for the UI layout pass a headless core app does not run: give the
/// built grid the real frame, so a `ScrollIntoView` on the spawned tree really
/// moves the body's `ScrollPosition` instead of computing against zero-sized
/// nodes.
fn rig_gallery_scroll(
    mut commands: Commands,
    mut bodies: Query<(Entity, &mut ComputedNode), With<crate::presentation::gallery::GalleryBody>>,
    mut cards: Query<
        (Entity, &MenuAction, &mut ComputedNode),
        Without<crate::presentation::gallery::GalleryBody>,
    >,
) {
    for (entity, mut node) in &mut bodies {
        node.size = RIG_VIEWPORT;
        node.content_size = Vec2::new(RIG_VIEWPORT.x, RIG_CONTENT_HEIGHT);
        commands
            .entity(entity)
            .insert(UiGlobalTransform::from_translation(RIG_VIEWPORT / 2.0));
    }
    for (entity, action, mut node) in &mut cards {
        let MenuAction::GalleryCard(index) = *action else {
            continue;
        };
        node.size = RIG_CARD;
        let column = (index % RIG_COLUMNS) as f32;
        let row = (index / RIG_COLUMNS) as f32;
        commands
            .entity(entity)
            .insert(UiGlobalTransform::from_translation(
                Vec2::new(column, row) * (RIG_CARD + Vec2::splat(RIG_GAP)) + RIG_CARD / 2.0,
            ));
    }
}

/// Closing the still returns to the grid focused on the entry the still was
/// showing, with that card revealed: the still's structural rebuild respawns
/// the body at the top, so the unchanged focus re-issues the card's reveal. Any
/// other rebuild that keeps the gallery up re-reveals it the same way, and an
/// idle frame never yanks the view back (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §4/§5, `### Layout` §4).
#[test]
fn closing_the_still_reveals_the_focused_card_again() {
    let extras: Vec<String> = (1..=16).map(|index| format!("g{index:02}")).collect();
    let extras: Vec<&str> = extras.iter().map(String::as_str).collect();
    let dir = temp_pack_with_backgrounds(&extras);
    let mut app = app_for_pack(dir.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);
    // The real scroll plugin and a layout stand-in, so the reveal is asserted
    // as the body's own `ScrollPosition`, not as a bare trigger.
    app.add_plugins(bevy::ui_widgets::ScrollAreaPlugin);
    app.add_systems(
        Update,
        (
            rig_gallery_scroll,
            crate::presentation::gallery::scroll_gallery_focus,
        )
            .chain()
            .after(RuntimeCoreSet),
    );

    // Walk down the grid to the 19th card: its reveal scrolls the body.
    for _ in 0..6 {
        press_and_update(&mut app, KeyCode::ArrowDown);
    }
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(18));
    let walked = gallery_body_scroll(&app);
    assert_eq!(
        walked,
        Vec2::new(0.0, RIG_CARD_18_REVEAL),
        "the walk reveals the 19th card at the viewport's foot"
    );

    // `Enter` opens the still: the grid gives way to the one entry.
    press_and_update(&mut app, KeyCode::Enter);
    build_pause_tree(&mut app);
    assert!(menu(&app).gallery_still, "Enter opens the still");
    assert!(
        has_action(&app, MenuAction::GalleryStillClose),
        "the still surface is up"
    );

    press_and_update(&mut app, KeyCode::ArrowDown);
    build_pause_tree(&mut app);
    assert!(!menu(&app).gallery_still, "↓ closes the still");
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(18));
    app.update();
    assert_eq!(
        gallery_body_scroll(&app),
        walked,
        "the returned grid reveals the still's entry"
    );

    app.update();
    assert_eq!(
        gallery_body_scroll(&app),
        walked,
        "an idle frame holds the view"
    );

    // The rebuild family is covered, not the still alone: a signature change
    // that keeps the card focus respawns the body the same way. It is written
    // directly because every reachable tab switch moves the focus to the tab.
    app.world_mut().resource_mut::<MenuState>().mode = MenuMode::Load;
    build_pause_tree(&mut app);
    app.update();
    assert_eq!(
        gallery_body_scroll(&app),
        walked,
        "a structural rebuild re-reveals the same card"
    );
}

/// The grid body really is a Bevy scroll area: a `ScrollIntoView` on a card
/// moves the body's `ScrollPosition` by the minimal amount that reveals it, so
/// the walk's focus stays visible (`docs/title-and-menu.md` § Gallery
/// `### Layout` §4).
#[test]
fn the_gallery_body_scrolls_the_focused_card_into_view() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::ui_widgets::ScrollAreaPlugin);
    let body = app
        .world_mut()
        .spawn((
            crate::presentation::gallery::GalleryBody,
            ScrollArea,
            Node {
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ComputedNode {
                size: Vec2::new(300.0, 100.0),
                content_size: Vec2::new(300.0, 400.0),
                ..default()
            },
            UiGlobalTransform::from_translation(Vec2::new(150.0, 50.0)),
        ))
        .id();
    // A card one-and-a-bit viewports down: its bottom sits at y 300.
    let card = app
        .world_mut()
        .spawn((
            Node::default(),
            ComputedNode {
                size: Vec2::new(300.0, 100.0),
                ..default()
            },
            UiGlobalTransform::from_translation(Vec2::new(150.0, 250.0)),
            ChildOf(body),
        ))
        .id();

    app.world_mut()
        .trigger(bevy::ui_widgets::ScrollIntoView { entity: card });

    let scroll = app.world().get::<ScrollPosition>(body).unwrap().0;
    assert_eq!(
        scroll.y, 200.0,
        "the body scrolls just enough to pull the card's bottom into view"
    );
}

/// `Tab` still reaches the overlay's narrowed top bar from a gallery card, and
/// an arrow returns to the grid; entering the top bar only changes mode
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §1, `### Layout` §5).
#[test]
fn tab_leaves_the_gallery_grid_and_arrows_return_to_it() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));

    press_and_update(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab, "Tab leaves the grid");
    press_and_update(&mut app, KeyCode::ArrowDown);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(0),
        "an arrow returns to the grid"
    );

    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(
        menu(&app).view,
        MenuView::Slots,
        "Save opens the ledger from the gallery"
    );
    assert_eq!(menu(&app).focus, MenuFocus::SaveTab);
    assert_eq!(menu(&app).mode, MenuMode::Save);
    press_and_update(&mut app, KeyCode::Tab);
    press_and_update(&mut app, KeyCode::Space);
    assert_eq!(menu(&app).mode, MenuMode::Load, "Space picks the Load tab");
    assert_eq!(menu(&app).focus, MenuFocus::LoadTab);
}

/// A title-door gallery app with the asset rig attached, so cards load through
/// the real watcher and the play path's staging shares that watcher too: a
/// present file decodes and a missing one fails. `remove` names pack-relative
/// asset paths deleted before the pack loads. The still sync is attached as
/// well; `fit_gallery_still` is deliberately not, because this minimal app has
/// no UI layout pass and its contain-fit maths is unit-tested directly.
fn gallery_asset_app(remove: &[&str]) -> App {
    let dir = temp_pack();
    let pack_root = dir.path().join("pack");
    for rel in remove {
        std::fs::remove_file(pack_root.join("assets").join(rel)).expect("remove the asset");
    }
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: pack_root.join("assets").to_string_lossy().into_owned(),
            ..default()
        })
        .init_asset::<Image>()
        .init_asset_loader::<PathImageLoader>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<AssetWatch>()
        .init_resource::<AppliedVisuals>()
        .init_resource::<crate::presentation::MenuBuilt>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    crate::presentation::render::register(&mut app);
    app.add_systems(
        Update,
        (
            crate::presentation::play::sync_staging_visuals,
            crate::presentation::sync_menu_ui,
            crate::presentation::gallery::sync_gallery_art,
            crate::presentation::gallery::sync_gallery_still,
            report_silent_degradations,
        )
            .chain(),
    );
    app.world_mut()
        .spawn((settings::window_config(), PrimaryWindow));
    attach_text_faces(&mut app);
    force_single_threaded(&mut app);
    app.insert_resource(TempPack(dir));
    app.update(); // Startup: load the pack, then show the title
    app
}

/// A missing card's art keeps the placeholder frame with its label, warns once
/// through rows 3/4, and raises no Notice (`docs/title-and-menu.md` § Gallery
/// `### Behavior` §6, `### Layout` §2).
#[test]
fn a_missing_art_card_draws_the_placeholder_frame_with_its_label() {
    captures::install();
    let mut app = gallery_asset_app(&["sprites/ace/happy.png"]);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    captures::take();
    pump_until_watch_drains(&mut app);

    let texts = all_texts(&app);
    for label in ["school_gate", "ace \u{b7} happy", "ace \u{b7} neutral"] {
        assert!(
            texts.iter().any(|text| text == label),
            "the {label:?} card label renders"
        );
    }

    // The decoded cards drop the placeholder frame; the failed card keeps it
    // and sizes no image, with no glyph or marker inside the art box.
    let gate = gallery_art_box(&app, entry_index(&app, "school_gate"));
    assert_eq!(
        app.world().get::<Outline>(gate).unwrap().color,
        Color::NONE,
        "a decoded card drops the placeholder frame"
    );
    let happy = gallery_art_box(&app, entry_index(&app, "ace \u{b7} happy"));
    assert_ne!(
        app.world().get::<Outline>(happy).unwrap().color,
        Color::NONE,
        "a failed card keeps the placeholder frame"
    );
    assert!(
        app.world()
            .get::<Children>(happy)
            .unwrap()
            .iter()
            .all(|child| app
                .world()
                .get::<crate::presentation::gallery::GalleryArt>(child)
                .is_some()),
        "the placeholder art box holds no glyph or marker"
    );

    // A decoded card draws its texture verbatim — white, untinted — over the
    // card's own fill, so a sprite's transparency shows the fill and there is
    // no checkerboard or veil.
    let gate_art = app
        .world()
        .get::<Children>(gate)
        .unwrap()
        .iter()
        .find(|child| {
            app.world()
                .get::<crate::presentation::gallery::GalleryArt>(*child)
                .is_some()
        })
        .expect("the decoded card carries an image node");
    assert_eq!(
        app.world().get::<ImageNode>(gate_art).unwrap().color,
        Color::WHITE,
        "the art is drawn verbatim"
    );
    let card = card_panel(&app, entry_index(&app, "school_gate"));
    assert_eq!(
        app.world().get::<BackgroundColor>(gate).unwrap().0,
        app.world().get::<BackgroundColor>(card).unwrap().0,
        "the letterbox is the card's own fill"
    );

    // Exactly one warn for the one absent path, under row 4, with no Notice.
    let records = failure_lines();
    assert_eq!(records.len(), 1, "one warn per failed path: {records:?}");
    assert_eq!(records[0].0, log::Level::Warn);
    assert!(
        records[0].2.starts_with("couldn't load a sprite")
            && records[0].2.contains("sprites/ace/happy.png"),
        "{:?}",
        records[0].2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "failed card art never notices"
    );
}

/// The still's one art node.
fn still_art(app: &App) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::gallery::GalleryStillArt>()
                .map(|_| entity.id())
        })
        .expect("the still carries an art node")
}

/// Whether a card's art box carries an image node. A card whose path the
/// watcher refused to attempt (already reported) has none; a present path has
/// one.
fn art_box_has_image(app: &App, art_box: Entity) -> bool {
    app.world()
        .get::<Children>(art_box)
        .map(|children| {
            children.iter().any(|child| {
                app.world()
                    .get::<crate::presentation::gallery::GalleryArt>(child)
                    .is_some()
            })
        })
        .unwrap_or(false)
}

/// Activating a failed card opens the still on the same placeholder frame with
/// the entry's label as its caption, and the whole open/walk/close path logs
/// no second line for a path the grid already reported
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §5/§6, `### Layout` §2).
#[test]
fn a_failed_cards_still_keeps_the_placeholder_and_the_label() {
    captures::install();
    let mut app = gallery_asset_app(&["sprites/ace/happy.png"]);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(&mut app, KeyCode::Enter);
    assert_eq!(menu(&app).view, MenuView::Gallery);
    captures::take();
    pump_until_watch_drains(&mut app);

    let grid_warns = failure_lines();
    assert_eq!(grid_warns.len(), 1, "one warn for the path: {grid_warns:?}");

    let failed = entry_index(&app, "ace \u{b7} happy");
    open_still(&mut app, failed);
    pump_until_watch_drains(&mut app);
    app.update();

    // The caption is the card's label verbatim, so the placeholder still draws
    // the entry's identity, and the art node holds the transparent fallback
    // that `fit_gallery_still` keeps unsized, so the frame stands.
    assert_eq!(
        still_label(&app),
        "ace \u{b7} happy",
        "the still shows the failed card's label"
    );
    assert!(
        all_texts(&app)
            .iter()
            .any(|text| text == "ace \u{b7} happy"),
        "the placeholder still draws the entry's label"
    );
    assert_eq!(
        app.world().get::<ImageNode>(still_art(&app)).unwrap().image,
        bevy::image::TRANSPARENT_IMAGE_HANDLE,
        "absent art leaves the transparent fallback, not a stretched sentinel"
    );

    assert!(
        failure_lines().is_empty(),
        "the still must not warn again for a reported path"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a placeholder still never notices"
    );

    // A decoded entry still loads into the still: stepping to its neighbour
    // replaces the transparent fallback with the real image, so the fallback
    // above is a placeholder and not a rig where nothing ever loads.
    press_and_update(&mut app, KeyCode::ArrowLeft);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(entry_index(&app, "school_gate")),
        "the still stepped to the previous entry"
    );
    pump_until_watch_drains(&mut app);
    app.update();
    assert_eq!(
        still_label(&app),
        "school_gate",
        "the caption follows the step"
    );
    assert_ne!(
        app.world().get::<ImageNode>(still_art(&app)).unwrap().image,
        bevy::image::TRANSPARENT_IMAGE_HANDLE,
        "a decoded entry loads its real image into the still"
    );
}

/// The shared image load asks for render-world-only usage. This harness has no
/// render world, so nothing extracts or unloads the image here: the witness is
/// the usage the load requested, not a released texture.
#[test]
fn a_pack_image_load_is_render_world_only() {
    let dir = temp_pack();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: dir
                .path()
                .join("pack/assets")
                .to_string_lossy()
                .into_owned(),
            ..default()
        })
        // The real decoder: `ImageLoader` comes from `bevy_image::ImagePlugin`
        // in `DefaultPlugins`, which `MinimalPlugins` omits — not from
        // `RenderPlugin`, which the headless composition disables. A
        // test-double loader builds its own `Image` and so could not show what
        // the shared image load asked the loader for.
        .init_asset::<Image>()
        .register_asset_loader(bevy::image::ImageLoader::new(
            bevy::image::CompressedImageFormats::NONE,
        ))
        .init_resource::<AssetWatch>();

    let server = app.world().resource::<AssetServer>().clone();
    let handle = app
        .world_mut()
        .resource_mut::<AssetWatch>()
        .watch_image(
            &server,
            crate::failure::FailureRow::BackgroundAsset,
            "backgrounds/school_gate.png",
        )
        .expect("the first load of a path is watched");

    // No render world and no settle system in this app, so
    // `pump_until_watch_drains` has nothing to drain: this waits on the load
    // state directly, keeping that helper's 10 s deadline and periodic 1 ms
    // sleep so a busy task pool cannot make the loop spin.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut spins = 0u32;
    while matches!(
        server.load_state(handle.id()),
        LoadState::NotLoaded | LoadState::Loading
    ) {
        assert!(
            Instant::now() < deadline,
            "the background load never settled"
        );
        app.update();
        spins += 1;
        if spins.is_multiple_of(64) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    let images = app.world().resource::<Assets<Image>>();
    let image = images.get(&handle).expect("the demo background decodes");
    assert_eq!(
        image.asset_usage,
        RenderAssetUsages::RENDER_WORLD,
        "the shared image load asks for render-world-only usage"
    );
}

/// A path the play path already reported is not reported a second time by the
/// gallery: the card draws its placeholder silently and the log stays at one
/// line (`docs/title-and-menu.md` § Gallery `### Behavior` §6/§8).
#[test]
fn a_path_the_play_path_reported_is_not_reported_again_by_the_gallery() {
    captures::install();
    let mut app = gallery_asset_app(&["backgrounds/school_gate.png"]);
    captures::take();

    // The play path stages the entry scene's background and reports row 3 once.
    enter_play(&mut app);
    app.update();
    pump_until_watch_drains(&mut app);
    let reported = failure_lines();
    let background_warns: Vec<_> = reported
        .iter()
        .filter(|(_, _, line)| line.starts_with("couldn't load a background"))
        .collect();
    assert_eq!(
        background_warns.len(),
        1,
        "the play path reports the missing background once: {reported:?}"
    );
    assert!(
        background_warns[0]
            .2
            .contains("backgrounds/school_gate.png"),
        "{:?}",
        background_warns[0].2
    );

    // The gallery's `school_gate` card loads the same path under the same row;
    // the one-per-path rule means no second line.
    open_overlay_gallery(&mut app);
    pump_until_watch_drains(&mut app);
    assert!(
        failure_lines().is_empty(),
        "a path already reported by the play path logs no second line"
    );

    let gate = gallery_art_box(&app, entry_index(&app, "school_gate"));
    assert_ne!(
        app.world().get::<Outline>(gate).unwrap().color,
        Color::NONE,
        "the already-reported card keeps the placeholder frame"
    );
    // The card consulted the watcher and the one-per-path rule refused the
    // load: no image node is spawned at all, so the frame stands and no second
    // warn is queued. A present sibling still spawns its art node, proving the
    // build really ran `watch_image` for these cards.
    assert!(
        !art_box_has_image(&app, gate),
        "an already-reported path spawns no art node"
    );
    assert!(
        art_box_has_image(
            &app,
            gallery_art_box(&app, entry_index(&app, "ace \u{b7} neutral"))
        ),
        "a present path still spawns its art node"
    );
    assert!(
        all_texts(&app).iter().any(|text| text == "school_gate"),
        "the placeholder card still carries its label"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a re-reported path never notices"
    );
}

/// A 19-entry pack scrolls its body, and every arrow move keeps focus on a real
/// card (`docs/title-and-menu.md` § Gallery `### Layout` §4, `### Behavior`
/// §4).
#[test]
fn a_nineteen_entry_grid_scrolls_with_the_focus_in_range() {
    let extras: Vec<String> = (1..=16).map(|index| format!("g{index:02}")).collect();
    let extras: Vec<&str> = extras.iter().map(String::as_str).collect();
    let dir = temp_pack_with_backgrounds(&extras);
    let mut app = app_for_pack(dir.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    let count = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()
        .len();
    assert_eq!(count, 19);
    for index in 0..count {
        assert!(has_action(&app, MenuAction::GalleryCard(index)));
    }
    assert_eq!(
        app.world()
            .get::<Node>(gallery_body(&app))
            .unwrap()
            .overflow,
        Overflow::scroll_y(),
        "the body scrolls"
    );

    for _ in 0..(count + 4) {
        press_and_update(&mut app, KeyCode::ArrowRight);
        assert_in_grid(&app, count);
    }
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(count - 1));
    for _ in 0..(count + 4) {
        press_and_update(&mut app, KeyCode::ArrowLeft);
        assert_in_grid(&app, count);
    }
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryCard(0);
    let rows = count.div_ceil(menu(&app).gallery_columns);
    for _ in 1..rows {
        press_and_update(&mut app, KeyCode::ArrowDown);
        assert_in_grid(&app, count);
    }
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(count - 1));
    for _ in 1..rows {
        press_and_update(&mut app, KeyCode::ArrowUp);
        assert_in_grid(&app, count);
    }
    assert_eq!(menu(&app).focus, MenuFocus::GalleryCard(0));
}

fn assert_in_grid(app: &App, count: usize) {
    match menu(app).focus {
        MenuFocus::GalleryCard(index) => {
            assert!(index < count, "focus {index} left the {count}-card grid")
        }
        other => panic!("focus left the grid: {other:?}"),
    }
}

/// The built gallery's title-door scrim node, if the title door drew one.
fn gallery_scrim(app: &App) -> Option<Entity> {
    app.world().iter_entities().find_map(|entity| {
        entity
            .get::<crate::presentation::gallery::GalleryScrim>()
            .map(|_| entity.id())
    })
}

/// The scrim's one radial gradient, so its stops can be pinned.
fn scrim_gradient(app: &App) -> RadialGradient {
    let scrim = gallery_scrim(app).expect("the title door draws the scrim");
    single_radial(app, scrim)
}

/// The parent of the title-door scrim — the gallery's root column, whose paint
/// order and gradient underlay the sibling tests read.
fn scrim_root(app: &App) -> Entity {
    let scrim = gallery_scrim(app).expect("the title door draws the scrim");
    app.world().get::<ChildOf>(scrim).unwrap().parent()
}

/// A node's children in paint order.
fn child_order(app: &App, parent: Entity) -> Vec<Entity> {
    app.world()
        .get::<Children>(parent)
        .map(|children| children.iter().collect())
        .unwrap_or_default()
}

/// Open the gallery from the title's `Gallery` row: the title door.
fn open_title_gallery(app: &mut App) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleGallery;
    press_and_update(app, KeyCode::Enter);
    assert_eq!(menu(app).view, MenuView::Gallery);
    assert_eq!(menu(app).gallery_origin, menu::MenuOrigin::Title);
}

/// From the title's door the gallery stands on the pack's backdrop and draws
/// the gallery-local radial scrim between that backdrop and the surface, so it
/// darkens the backdrop only and never a card's art
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6, `### Layout` §4/§7).
#[test]
fn the_title_gallery_draws_the_backdrop_under_the_scrim_under_the_surface() {
    let mut app = gallery_asset_app(&[]);
    open_title_gallery(&mut app);
    pump_until_watch_drains(&mut app);

    // The declared `school_gate` backdrop draws, unmodified, over the neutral
    // gradient underlay.
    let (image, _) = title_backdrop_node(&app).expect("the declared backdrop draws");
    assert_eq!(image.color, Color::WHITE, "the art is drawn verbatim");

    // The scrim is the prototype's radial: 0.34 at the middle, 0.74 at the
    // edges, centred on the window.
    let radial = scrim_gradient(&app);
    assert_eq!(radial.position, UiPosition::CENTER, "the scrim is centred");
    // `resolve` yields node-centre-relative coordinates, so the middle is the
    // origin; a `CENTER.at_percent(50, 50)` offset would land on the
    // bottom-right corner instead.
    assert_eq!(
        radial
            .position
            .resolve(1.0, Vec2::new(1280.0, 720.0), Vec2::new(1280.0, 720.0)),
        Vec2::ZERO,
        "the scrim's centre is the middle of the window"
    );
    assert_eq!(radial.shape, RadialGradientShape::FarthestCorner);
    assert_eq!(radial.stops.len(), 2, "two stops");
    assert_eq!(radial.stops[0].point, Val::Percent(0.0));
    assert_eq!(radial.stops[1].point, Val::Percent(100.0));
    assert_eq!(
        radial.stops[0].color,
        Color::srgba(5.0 / 255.0, 7.0 / 255.0, 10.0 / 255.0, 0.34),
        "the middle is the spec's 0.34 tone"
    );
    assert_eq!(
        radial.stops[1].color,
        Color::srgba(5.0 / 255.0, 7.0 / 255.0, 10.0 / 255.0, 0.74),
        "the edges are the spec's 0.74 tone"
    );

    // The scrim is full-window, and its paint order is backdrop → scrim →
    // surface, so no card's art is ever covered.
    let scrim = gallery_scrim(&app).unwrap();
    let node = app.world().get::<Node>(scrim).unwrap();
    assert_eq!(node.position_type, PositionType::Absolute);
    assert_eq!(node.width, Val::Percent(100.0));
    assert_eq!(node.height, Val::Percent(100.0));

    let root = scrim_root(&app);
    let siblings = child_order(&app, root);
    let scrim_index = siblings.iter().position(|entity| *entity == scrim).unwrap();
    let backdrop_index = siblings
        .iter()
        .position(|entity| {
            app.world()
                .get::<crate::presentation::title::TitleBackdropImage>(*entity)
                .is_some()
        })
        .expect("the backdrop is a sibling of the scrim");
    let surface = app
        .world()
        .get::<ChildOf>(gallery_body(&app))
        .unwrap()
        .parent();
    let surface_index = siblings
        .iter()
        .position(|entity| *entity == surface)
        .unwrap();
    assert!(
        backdrop_index < scrim_index && scrim_index < surface_index,
        "the scrim paints between the backdrop and the surface"
    );
    assert!(
        app.world().get::<BackgroundGradient>(root).is_some(),
        "the neutral gradient underlay stays beneath the art"
    );
}

/// A pack that declares no `title_backdrop` draws the neutral gradient behind
/// the title-door gallery, with the scrim still over it
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
#[test]
fn a_title_gallery_without_a_backdrop_keeps_the_gradient_and_the_scrim() {
    let manifest = "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n)\n";
    let mut app = title_asset_app(Some(manifest));
    pump_until_watch_drains(&mut app);
    {
        let mut state = app.world_mut().resource_mut::<MenuState>();
        state.view = MenuView::Gallery;
        state.gallery_origin = menu::MenuOrigin::Title;
    }
    pump_until_watch_drains(&mut app);

    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert!(
        title_backdrop_node(&app).is_none(),
        "no backdrop image node is drawn"
    );
    let root = scrim_root(&app);
    assert!(
        app.world().get::<BackgroundGradient>(root).is_some(),
        "the gradient is the whole backdrop"
    );
}

/// From the overlay's door the gallery keeps the overlay's dimmed freeze of the
/// play frame and draws no scrim and no gradient
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6, `### Layout` §7).
#[test]
fn the_overlay_gallery_draws_no_scrim() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    assert!(
        gallery_scrim(&app).is_none(),
        "the overlay's door adds no scrim"
    );
    let root = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::MenuRoot>()
                .map(|_| entity.id())
        })
        .expect("the overlay root draws");
    assert_eq!(
        app.world().get::<BackgroundColor>(root).unwrap().0.alpha(),
        0.0,
        "the page's fill is the tree's, not a dimmer over the play frame"
    );
    let page = app
        .world()
        .get::<Children>(root)
        .expect("the page root holds the surface")
        .iter()
        .find(|child| {
            app.world()
                .get::<BackgroundColor>(*child)
                .is_some_and(|fill| fill.0.alpha() > 0.0)
        })
        .expect("the tree's page node fills the frame");
    assert_eq!(
        app.world().get::<BackgroundColor>(page).unwrap().0,
        Color::srgb(0.051, 0.055, 0.071),
        "§4's page is the opaque #0d0e12 fill"
    );
    assert!(
        app.world().get::<BackgroundGradient>(page).is_none(),
        "§4's page draws one flat fill, not a gradient"
    );
}

/// A declared but missing title backdrop degrades silently behind the
/// title-door gallery: the gradient and the scrim stand, each load of the path
/// logs once as a warning and no Notice appears
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6).
#[test]
fn a_missing_title_backdrop_behind_the_gallery_degrades_silently() {
    captures::install();
    let manifest = "(\n    name: \"unmei demo\",\n    entry: \"first_meeting\",\n    title_backdrop: \"missing_art\",\n)\n";
    let mut app = title_asset_app(Some(manifest));
    force_single_threaded(&mut app);
    captures::take();
    pump_until_watch_drains(&mut app);
    {
        let mut state = app.world_mut().resource_mut::<MenuState>();
        state.view = MenuView::Gallery;
        state.gallery_origin = menu::MenuOrigin::Title;
    }
    pump_until_watch_drains(&mut app);

    assert_eq!(menu(&app).view, MenuView::Gallery);
    assert!(
        title_backdrop_node(&app).is_none(),
        "a failed backdrop draws nothing"
    );
    let root = scrim_root(&app);
    assert!(
        app.world().get::<BackgroundGradient>(root).is_some(),
        "the gradient stands behind the gallery"
    );

    // Row 25 is the title backdrop's one row, and the gallery reuses the
    // title's watch, so opening it adds no second title-backdrop line. The
    // gallery's "row 3" wording is deliberately not taken for the backdrop: row 3
    // is the play path's `bg` row (`couldn't load a background`), while the
    // `title_backdrop` asset's documented row is 25. The one row-3 line here
    // is the gallery *card* for the same id — a distinct load with its own
    // funnel, not the backdrop's.
    let records = failure_lines();
    assert!(
        records
            .iter()
            .all(|(level, _, _)| *level == log::Level::Warn),
        "every gallery degradation is silent: {records:?}"
    );
    assert_eq!(records.len(), 2, "one line per load: {records:?}");
    let count_of = |prefix: &str| {
        records
            .iter()
            .filter(|(_, _, line)| line.starts_with(prefix))
            .count()
    };
    assert_eq!(
        count_of("couldn't load the title backdrop"),
        1,
        "the backdrop logs exactly once: {records:?}"
    );
    assert_eq!(
        count_of("couldn't load a background"),
        1,
        "the card logs exactly once: {records:?}"
    );
    let backdrop_line = records
        .iter()
        .find(|(_, _, line)| line.starts_with("couldn't load the title backdrop"))
        .expect("the backdrop logs");
    assert!(
        backdrop_line.2.contains("backgrounds/missing_art.png"),
        "the line names the path: {:?}",
        backdrop_line.2
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "a silent degradation never notices"
    );
}

/// Opening and walking the title-door gallery leaves the backdrop and its scrim
/// exactly as they were — the surface is not rebuilt behind the walk — and
/// backing out to the title keeps the same backdrop
/// (`docs/title-and-menu.md` § Gallery `### Behavior` §6, `### Layout` §6).
#[test]
fn walking_the_title_gallery_leaves_the_backdrop_and_scrim_unchanged() {
    let mut app = gallery_asset_app(&[]);
    open_title_gallery(&mut app);
    pump_until_watch_drains(&mut app);

    let backdrop = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::title::TitleBackdropImage>()
                .map(|_| entity.id())
        })
        .expect("the declared backdrop draws");
    let scrim = gallery_scrim(&app).unwrap();
    let gradient = scrim_gradient(&app);
    let root = scrim_root(&app);
    let before = child_order(&app, root);

    press_and_update(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        menu(&app).focus,
        MenuFocus::GalleryCard(1),
        "the walk moved"
    );

    assert!(
        app.world()
            .get::<crate::presentation::title::TitleBackdropImage>(backdrop)
            .is_some(),
        "the same backdrop entity survives the walk"
    );
    assert!(
        app.world()
            .get::<crate::presentation::gallery::GalleryScrim>(scrim)
            .is_some(),
        "the same scrim entity survives the walk"
    );
    assert_eq!(child_order(&app, root), before, "the tree is not rebuilt");
    assert_eq!(scrim_gradient(&app), gradient, "the scrim is unchanged");

    esc(&mut app);
    assert_eq!(menu(&app).view, MenuView::Title);
    pump_until_watch_drains(&mut app);
    assert!(
        title_backdrop_node(&app).is_some(),
        "the title's backdrop is unchanged by the gallery round trip"
    );
}
