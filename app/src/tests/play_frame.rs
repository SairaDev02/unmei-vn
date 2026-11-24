//! The play frame through the plugin renderer: Default look §1–§2's geometry and
//! faces, the choice-wait's kept line and its empty box after a load, and the
//! manifest's choice numbering (spec §5, rule 22).

use super::*;
use crate::core::SkipState;
use bevy::text::{FontSource, LineHeight};
use bevy::ui::{BoxShadow, Display};

/// Draw the play frame over the current park.
fn frame(app: &mut App) {
    render_dialogue(app);
}

/// Whether `entity` or one of its ancestors is hidden from layout, which is how
/// the engine draws a slot it has nothing for.
fn hidden(app: &App, entity: Entity) -> bool {
    let mut at = entity;
    loop {
        if app
            .world()
            .get::<Node>(at)
            .is_some_and(|node| node.display == Display::None)
        {
            return true;
        }
        match app.world().get::<ChildOf>(at) {
            Some(child_of) => at = child_of.parent(),
            None => return false,
        }
    }
}

fn node_where(app: &App, filter: impl Fn(Entity, &App) -> bool) -> Option<Entity> {
    app.world()
        .iter_entities()
        .map(|entity| entity.id())
        .find(|entity| filter(*entity, app))
}

fn text_of(app: &App, entity: Entity) -> String {
    app.world()
        .get::<Text>(entity)
        .map(|text| text.0.clone())
        .unwrap_or_default()
}

fn font_of(app: &App, entity: Entity) -> TextFont {
    app.world()
        .get::<TextFont>(entity)
        .cloned()
        .expect("a drawn face")
}

/// The drawn node whose words are `words`, outside the title and overlay chrome.
fn node_with_words(app: &App, words: &str) -> Entity {
    node_where(app, |entity, app| text_of(app, entity) == words)
        .unwrap_or_else(|| panic!("the frame draws {words:?}"))
}

#[test]
fn the_dialogue_box_and_prose_keep_default_looks_geometry() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    frame(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();

    // §1: the box spans the frame less a 72px inset, sits 28px above the bottom
    // edge, and is 178px tall with a 1px hairline, a 14px radius and a shadow.
    let box_node = node_where(&app, |entity, app| {
        let Some(node) = app.world().get::<Node>(entity) else {
            return false;
        };
        node.left == Val::Px(72.0)
            && node.right == Val::Px(72.0)
            && node.bottom == Val::Px(28.0)
            && node.height == Val::Px(178.0)
    })
    .expect("the dialogue box exists");
    let drawn = app.world().get::<Node>(box_node).expect("box node").clone();
    assert_eq!(drawn.border, UiRect::all(Val::Px(1.0)), "a 1px hairline");
    assert_eq!(
        drawn.border_radius,
        BorderRadius::all(Val::Px(14.0)),
        "a 14px radius"
    );
    assert_eq!(drawn.position_type, PositionType::Absolute);
    assert!(
        app.world().get::<BackgroundGradient>(box_node).is_some(),
        "the fill is the vertical gradient"
    );
    assert!(
        app.world().get::<BoxShadow>(box_node).is_some(),
        "the box carries a soft drop shadow"
    );

    // §1: the prose is 25px at a 1.5 line height, sans, wrapping on words across
    // the box's width.
    let (_, prose_text) = say_parts(&app);
    let prose = node_with_words(&app, &prose_text);
    let prose_font = font_of(&app, prose);
    assert_eq!(prose_font.font_size, faces.sans(25.0).font_size);
    assert_eq!(
        prose_font.font,
        FontSource::Handle(faces.sans_regular.clone()),
        "the prose is sans"
    );
    assert_eq!(
        app.world().get::<LineHeight>(prose).cloned(),
        Some(LineHeight::Px(38.0)),
        "25px at a 1.5 line height"
    );
    assert_eq!(
        app.world()
            .get::<TextLayout>(prose)
            .map(|layout| layout.linebreak),
        Some(LineBreak::WordOrCharacter),
        "long prose wraps on words"
    );
    let prose_slot = app
        .world()
        .get::<ChildOf>(prose)
        .map(|child_of| child_of.parent())
        .expect("the prose slot");
    assert_eq!(
        app.world().get::<Node>(prose_slot).map(|node| node.width),
        Some(Val::Percent(100.0)),
        "the prose fills the box's width, so it wraps there"
    );

    // §1: the advance mark is the plugin's own ▼ at the box's right, and an
    // Instant line is whole, so it shows.
    let marker = node_with_words(&app, "▼");
    assert_eq!(
        font_of(&app, marker).font,
        FontSource::Handle(faces.mono_medium.clone()),
        "the marker's face carries the glyph"
    );
    assert_eq!(font_of(&app, marker).font_size, faces.mono(18.0).font_size);
    assert_eq!(
        app.world()
            .get::<BackgroundColor>(marker)
            .map(|fill| fill.0.alpha()),
        Some(0.0),
        "a glyph's fill is its tone, not a background behind the mark"
    );
    assert_eq!(
        app.world().get::<TextColor>(marker).map(|tone| tone.0),
        Some(Color::srgba(0.80, 0.80, 0.85, 0.62)),
        "the mark draws in the plugin's dim tone"
    );
    assert!(!hidden(&app, marker), "a whole line shows its advance mark");
}

#[test]
fn the_advance_mark_waits_for_the_line_and_narration_draws_no_tab() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    set_text_speed(&mut app, 0.02);
    frame(&mut app);

    // §1: narration has no tab, and the mark waits until the line is whole.
    let tab = node_where(&app, |entity, app| {
        app.world()
            .get::<Node>(entity)
            .is_some_and(|node| node.left == Val::Px(28.0) && node.top == Val::Px(-20.0))
    })
    .expect("the tab box exists in the tree");
    assert!(hidden(&app, tab), "narration draws no name tab");
    let marker = node_with_words(&app, "▼");
    assert!(hidden(&app, marker), "the mark waits for the reveal");

    let (_, full) = say_parts(&app);
    let needed = full.chars().count() as f32 * 0.02 + 0.01;
    advance_time(&mut app, needed);
    frame(&mut app);
    assert_eq!(drawn_prose(&app), full);
    assert!(!hidden(&app, marker), "a whole line shows the mark");

    // A spoken line draws its tab: the speaker in the plugin's own face.
    press_and_update(&mut app, KeyCode::Space);
    set_text_speed(&mut app, 0.0);
    frame(&mut app);
    let faces = app
        .world()
        .resource::<crate::presentation::style::TextFaces>()
        .clone();
    let (speaker, _) = say_parts(&app);
    assert_ne!(speaker, "Narrator", "the second line is spoken");
    let name = node_with_words(&app, &speaker);
    assert!(!hidden(&app, tab), "a spoken line draws the tab");
    let name_font = font_of(&app, name);
    assert_eq!(
        name_font.font,
        FontSource::Handle(faces.sans_semibold.clone())
    );
    assert_eq!(name_font.font_size, faces.sans_semibold(16.0).font_size);
}

#[test]
fn a_choice_wait_keeps_the_line_fully_revealed_and_without_the_mark() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space);
    frame(&mut app);
    let (speaker, line) = say_parts(&app);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));

    frame(&mut app);
    assert_eq!(
        drawn_prose(&app),
        line,
        "§2: the box keeps the say-line the player just read"
    );
    assert_eq!(drawn_speaker(&app), Some(speaker), "and keeps its name tab");
    let marker = node_with_words(&app, "▼");
    assert!(hidden(&app, marker), "a choice shows no advance mark");
    let option = node_with_words(&app, "Ask about the letter");
    assert!(!hidden(&app, option), "the options are drawn");
}

#[test]
fn a_load_onto_a_choice_wait_leaves_the_box_empty() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    press_and_update(&mut app, KeyCode::F5);
    assert!(manual_path(&app).exists(), "the save holds the choice-wait");

    press_and_update(&mut app, KeyCode::F9);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    frame(&mut app);
    assert_eq!(
        drawn_prose(&app),
        "",
        "§2: a load forgets the line it was showing"
    );
    assert_eq!(drawn_speaker(&app), None, "and draws no tab");
    let option = node_with_words(&app, "Ask about the letter");
    assert!(!hidden(&app, option), "the options still draw");
}

/// A pack whose plugin declares a numbering style and otherwise draws the
/// reference look: the play tree and its tokens, unchanged.
fn write_numbering_plugin(pack: &std::path::Path, numbering: &str) {
    let reference = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-plugin");
    let ui = pack.join("ui");
    std::fs::create_dir_all(ui.join("surfaces")).unwrap();
    std::fs::write(
        ui.join("plugin.ron"),
        format!(
            r#"(
    format_version: 1,
    name: "Fixture",
    numbering: "{numbering}",
)
"#
        ),
    )
    .unwrap();
    std::fs::copy(reference.join("tokens.ron"), ui.join("tokens.ron")).unwrap();
    std::fs::copy(
        reference.join("surfaces/play.ron"),
        ui.join("surfaces/play.ron"),
    )
    .unwrap();
}

/// Add a third option to the demo pack's first choice, so a numbering style is
/// tested on the option count rule 22 states.
fn add_third_option(pack: &std::path::Path) {
    let scene = pack.join("scenes/first_meeting.unmei");
    let text = std::fs::read_to_string(&scene).unwrap();
    let text = text.replace(
        "  \"Stay silent\" -> silence
end",
        "  \"Stay silent\" -> silence
  \"Check the trees\" -> silence
end",
    );
    std::fs::write(&scene, text).unwrap();
}

#[test]
fn the_plugins_numbering_numbers_the_options_and_the_keys_still_pick() {
    let pack = temp_pack();
    let root = pack.path().join("pack");
    write_numbering_plugin(&root, "roman-upper");
    add_third_option(&root);
    let mut app = app_for_pack(root);
    press_and_update(&mut app, KeyCode::Space);
    press_and_update(&mut app, KeyCode::Space);
    assert!(matches!(wait(&app), Some(Wait::Choice { .. })));
    frame(&mut app);

    // §5 and rule 22: the badges read the manifest's numbering style...
    let options = match wait(&app) {
        Some(Wait::Choice { options }) => options,
        other => panic!("a choice-wait, got {other:?}"),
    };
    let badges = ["I", "II", "III"];
    assert_eq!(options.len(), badges.len());
    for badge in badges {
        let node = node_with_words(&app, badge);
        assert!(!hidden(&app, node), "{badge} is drawn");
    }

    // ...while the keys stay 1–9: `2` picks the second option, whose branch
    // speaks before anything else.
    let before = parked_position(&app);
    press_and_update(&mut app, KeyCode::Digit2);
    assert_ne!(
        parked_position(&app),
        before,
        "the key picked the second option"
    );
    assert_eq!(
        wait(&app),
        Some(Wait::Say {
            speaker: "Ace".to_string(),
            text: "...Fine, don't talk. See if I care.".to_string(),
        }),
        "the second option's branch is the one taken"
    );
}

/// The advance mark's own blink: a 2000 ms loop through 1.0, 0.3 and back, on
/// real time, dimming the tone it starts with and never compounding it
/// (Default look §1, spec §6).
#[test]
fn the_advance_mark_blinks_on_its_period() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    frame(&mut app);
    let marker = node_with_words(&app, "▼");
    let base = app
        .world()
        .get::<TextColor>(marker)
        .map(|tone| tone.0.alpha())
        .expect("the mark is toned");
    assert!(base > 0.3, "the mark starts at its authored tone: {base}");

    // One third of the period is the loop's dimmest keyframe.
    advance_time(&mut app, 2.0 / 3.0);
    tick_update(&mut app);
    let dim = app
        .world()
        .get::<TextColor>(marker)
        .map(|tone| tone.0.alpha())
        .expect("the mark is toned");
    assert!(
        (dim - base * 0.3).abs() < 0.02,
        "a third of the period is the 0.3 keyframe: {dim} against {base}"
    );

    // Another third brings it back to the tone it started with.
    advance_time(&mut app, 2.0 / 3.0);
    tick_update(&mut app);
    let again = app
        .world()
        .get::<TextColor>(marker)
        .map(|tone| tone.0.alpha())
        .expect("the mark is toned");
    assert!(
        (again - base).abs() < 0.02,
        "the loop returns to the authored tone, never compounding: {again}"
    );
}

/// The rendered quick-row control whose action is `action`.
fn row_control(app: &App, action: MenuAction) -> Entity {
    node_where(app, |entity, app| {
        app.world().get::<MenuAction>(entity).copied() == Some(action)
    })
    .unwrap_or_else(|| panic!("the row draws a control for {action:?}"))
}

fn border_width(app: &App, entity: Entity) -> f32 {
    match app.world().get::<Node>(entity).map(|node| node.border.left) {
        Some(Val::Px(width)) => width,
        _ => 0.0,
    }
}

/// The tone a control's drawn words carry.
fn words_tone(app: &App, control: Entity) -> Option<Color> {
    let children = app.world().get::<Children>(control)?;
    children
        .iter()
        .find_map(|child| app.world().get::<TextColor>(child).map(|tone| tone.0))
}

/// One click on a quick-row control, settled the way one `Update` settles it:
/// the row's own input, then the frame's read, then the renderer's repaint.
fn click_row(app: &mut App, action: MenuAction) {
    let control = row_control(app, action);
    app.world_mut()
        .entity_mut(control)
        .insert(Interaction::Pressed);
    tick_update(app);
    frame(app);
    tick_update(app);
    app.world_mut()
        .entity_mut(control)
        .insert(Interaction::Hovered);
}

/// The row's five labels are the copy table's, drawn from the plugin's tree
/// (Default look §1, rule 40).
#[test]
fn the_quick_row_draws_its_five_labels() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    frame(&mut app);
    for words in ["Save", "Load", "Auto A", "Skip Tab", "Menu Esc"] {
        let label = node_with_words(&app, words);
        assert!(!hidden(&app, label), "the row draws {words:?}");
    }
}

/// `Auto A` and `Skip Tab` light and unlight in the frame their mode changes,
/// with `Auto A` unlit while skip runs (Default look §1, rule 40).
#[test]
fn the_row_lights_in_the_frame_its_mode_changes() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    frame(&mut app);
    let auto = row_control(&app, MenuAction::QuickAuto);
    let skip = row_control(&app, MenuAction::QuickSkip);
    assert_eq!(border_width(&app, auto), 0.0, "auto starts unlit");
    assert_eq!(border_width(&app, skip), 0.0, "skip starts unlit");

    click_row(&mut app, MenuAction::QuickAuto);
    assert!(auto_on(&app), "the click turned auto on");
    assert_eq!(border_width(&app, auto), 1.0, "auto lights on its click");
    assert_eq!(
        words_tone(&app, auto),
        Some(Color::srgb(0.902, 0.62, 0.361)),
        "a lit label draws in the plugin's focus tone"
    );

    app.world_mut().resource_mut::<SkipState>().engage();
    frame(&mut app);
    tick_update(&mut app);
    assert_eq!(border_width(&app, skip), 1.0, "skip lights when it runs");
    assert_eq!(
        border_width(&app, auto),
        0.0,
        "auto is unlit while skip runs"
    );

    app.world_mut().resource_mut::<SkipState>().end();
    app.world_mut().resource_mut::<SkipState>().arm_force();
    app.world_mut()
        .resource_mut::<SkipState>()
        .set_force_active(true);
    frame(&mut app);
    tick_update(&mut app);
    assert_eq!(border_width(&app, skip), 1.0, "a held Ctrl lights skip");

    app.world_mut()
        .resource_mut::<SkipState>()
        .set_force_active(false);
    app.world_mut().resource_mut::<SkipState>().end();
    frame(&mut app);
    tick_update(&mut app);
    assert_eq!(border_width(&app, skip), 0.0, "skip unlights");
    assert_eq!(border_width(&app, auto), 1.0, "auto relights");
    assert_eq!(
        words_tone(&app, skip),
        Some(Color::srgba(0.80, 0.80, 0.85, 0.35)),
        "an unlit label returns to the plugin's faint tone"
    );
}

/// `Skip Tab` is the `Tab` key's toggle: a click engages the seen skip on a
/// re-read line and a second click ends it (Default look §1, rule 40).
#[test]
fn a_click_on_skip_tab_toggles_the_seen_skip() {
    let pack = temp_pack();
    let mut app = app_with_known_entry(&pack);
    frame(&mut app);

    click_row(&mut app, MenuAction::QuickSkip);
    assert!(
        app.world().resource::<SkipState>().is_seen(),
        "the click engaged Tab's own toggle"
    );
    click_row(&mut app, MenuAction::QuickSkip);
    assert!(
        !app.world().resource::<SkipState>().is_seen(),
        "a second click ends it"
    );
}

/// A click that lands on the row is the row's, never the line's (Default look
/// §1).
#[test]
fn a_click_on_the_row_never_advances_the_line() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    frame(&mut app);
    advance_time(&mut app, 10.0);
    let before = wait(&app);
    assert!(matches!(before, Some(Wait::Say { .. })));

    let auto = row_control(&app, MenuAction::QuickAuto);
    app.world_mut()
        .entity_mut(auto)
        .insert(Interaction::Pressed);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .run_system_once(crate::input::advance_say_input)
        .expect("the advance input runs");
    assert_eq!(
        wait(&app),
        before,
        "the row's click never advances the line"
    );

    app.world_mut()
        .entity_mut(auto)
        .insert(Interaction::Hovered);
    app.world_mut()
        .run_system_once(crate::input::advance_say_input)
        .expect("the advance input runs");
    assert_ne!(wait(&app), before, "a click on the box still advances");
}

/// `Save`, `Load` and `Menu Esc` open the contents page, the first two on their
/// own section (Default look §1).
#[test]
fn save_and_load_open_the_contents_page_at_their_section() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    frame(&mut app);

    click_row(&mut app, MenuAction::QuickSave);
    let menu = app.world().resource::<MenuState>();
    assert!(menu.open, "Save opens the contents page");
    assert_eq!(menu.view, MenuView::Slots);
    assert_eq!(menu.mode, MenuMode::Save, "on the Save section");

    app.world_mut().resource_mut::<MenuState>().open = false;
    click_row(&mut app, MenuAction::QuickLoad);
    let menu = app.world().resource::<MenuState>();
    assert!(menu.open);
    assert_eq!(menu.mode, MenuMode::Load, "Load opens on its own section");

    app.world_mut().resource_mut::<MenuState>().open = false;
    click_row(&mut app, MenuAction::QuickMenu);
    assert!(
        app.world().resource::<MenuState>().open,
        "Menu Esc opens the contents page"
    );
}

/// A pack that draws the reference look with one copy override: rule 40's row
/// shows the plugin's words, and `A` still toggles auto.
#[test]
fn a_copy_override_of_a_quick_label_shows_and_the_a_key_still_toggles_auto() {
    let pack = temp_pack();
    write_plugin_with_copy_override(
        &pack.path().join("pack"),
        "play.quick.auto",
        "Auto (custom)",
    );
    let mut app = app_for_pack(pack.path().join("pack"));
    frame(&mut app);

    let auto = row_control(&app, MenuAction::QuickAuto);
    assert_eq!(
        app.world().get::<Children>(auto).map(|children| {
            children
                .iter()
                .find_map(|child| app.world().get::<Text>(child).map(|text| text.0.clone()))
        }),
        Some(Some("Auto (custom)".to_string())),
        "the override's words are the row's label"
    );
    assert!(
        node_where(&app, |entity, app| text_of(app, entity) == "Auto A").is_none(),
        "the table's words are not drawn"
    );

    press_and_update(&mut app, KeyCode::KeyA);
    assert!(auto_on(&app), "A still toggles auto");
}

/// Install the reference look as the pack's own plugin with one copy override.
fn write_plugin_with_copy_override(pack: &std::path::Path, key: &str, words: &str) {
    let ui = pack.join("ui");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-plugin"),
        &ui,
    );
    let path = ui.join("plugin.ron");
    let body = std::fs::read_to_string(&path).unwrap();
    let at = body.find("format_version").expect("the manifest's fields");
    let open = body[..at].rfind('(').expect("the manifest's tuple");
    let rebuilt = format!(
        "{}(format_version: 1, name: \"Reference\", numbering: \"arabic\", copy: {{ \"{key}\": \"{words}\" }})\n",
        &body[..open]
    );
    std::fs::write(&path, rebuilt).unwrap();
}
