//! Ticket #117: the legacy save picker, drawn from the plugin's `legacy-picker`
//! surface through the renderer.
//!
//! These pin what the tree owns — the dimmer, the 460px panel, the heading, the
//! prompt, one row per manual target with its label and status line, the three
//! target controls and Cancel — and that a pack's own token edit changes what the
//! picker draws.

use std::path::Path;

use unmei_engine::copy;

use super::*;
use crate::presentation::render::UiLook;

/// The reference look's dimmer and panel tones, as `tokens.ron` declares them.
const SHADE: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
const PANEL: Color = Color::srgba(0.10, 0.11, 0.15, 1.0);
/// The panel tone the pack's edited token names.
const RECOLOURED_PANEL: Color = Color::srgba(0.25, 0.50, 0.75, 1.0);

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

/// Every text inside `root`, in paint order.
fn subtree_words(app: &App, root: Entity) -> Vec<String> {
    subtree(app, root)
        .into_iter()
        .filter_map(|entity| app.world().get::<Text>(entity).map(|text| text.0.clone()))
        .collect()
}

/// Every entity inside `root`, `root` included, in spawn order.
fn subtree(app: &App, root: Entity) -> Vec<Entity> {
    let mut entities = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        entities.push(entity);
        stack.extend(children_of(app, entity));
    }
    entities
}

/// Whether `entity` sits inside `root`.
fn within(app: &App, entity: Entity, root: Entity) -> bool {
    let mut current = Some(entity);
    while let Some(node) = current {
        if node == root {
            return true;
        }
        current = app.world().get::<ChildOf>(node).map(ChildOf::parent);
    }
    false
}

fn fill(app: &App, entity: Entity) -> Color {
    app.world()
        .get::<BackgroundColor>(entity)
        .map(|fill| fill.0)
        .unwrap_or(Color::NONE)
}

/// The picker's root: the tree's dimmer, an absolute full-frame node filled with
/// the shade token, drawn into the contents page's own tree with no wrapper of
/// its own.
fn picker_root(app: &App) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            let node = entity.get::<Node>()?;
            let dimmer = entity
                .get::<BackgroundColor>()
                .is_some_and(|background| background.0 == SHADE)
                && node.position_type == PositionType::Absolute
                && node.width == Val::Percent(100.0)
                && node.height == Val::Percent(100.0);
            dimmer.then_some(entity.id())
        })
        .expect("the picker draws its dimmer")
}

/// The tree's 460px panel inside the dimmer.
fn panel(app: &App) -> Entity {
    children_of(app, picker_root(app))
        .into_iter()
        .find(|child| {
            app.world()
                .get::<Node>(*child)
                .is_some_and(|node| node.width == Val::Px(460.0))
        })
        .expect("the picker draws the 460px panel")
}

/// The one shown control carrying `action`: the item template places all three
/// target controls in every row, so a hidden sibling never stands for a row.
fn shown_control(app: &App, action: MenuAction) -> Entity {
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

/// Write an importable legacy save beside the pack: the captured session under
/// the legacy file's name, with no stamp.
fn write_legacy(app: &App) {
    let runtime = app.world().resource::<Runtime>();
    let mut save = SaveFile::capture(&runtime.runner, &runtime.staging);
    save.saved_at = None;
    save::write_save(&save::legacy_save_path_for(runtime.runner.pack()), &save).unwrap();
}

/// An app in play whose pack has an importable legacy save, with slot 1 filled
/// and the contents page open. Slot 3 is left empty.
fn app_with_legacy() -> (App, tempfile::TempDir) {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    write_legacy(&app);
    press_and_update(&mut app, KeyCode::F5);
    esc(&mut app);
    (app, pack)
}

/// The open picker, reached the way the player reaches it: the Legacy card.
fn open_picker(app: &mut App) {
    click(app, MenuAction::LegacyImport);
    assert_eq!(
        menu(app).picker,
        Some(menu::LegacyPicker { target: 0 }),
        "the picker is open"
    );
    build_pause_tree(app);
}

#[test]
fn opening_the_picker_draws_the_plugin_tree() {
    let (mut app, _pack) = app_with_legacy();
    open_picker(&mut app);

    let root = picker_root(&app);
    let tree = app
        .world()
        .resource::<UiLook>()
        .surface("legacy-picker")
        .cloned()
        .expect("the look holds the legacy-picker tree");
    assert_eq!(tree.style.width, unmei_engine::ui::Dimension::Fill);
    assert_eq!(tree.style.height, unmei_engine::ui::Dimension::Fill);
    assert_eq!(tree.style.position, unmei_engine::ui::Position::Absolute);
    let node = app.world().get::<Node>(root).expect("the dimmer is a node");
    assert_eq!(
        (node.width, node.height),
        (Val::Percent(100.0), Val::Percent(100.0))
    );
    assert_eq!(node.position_type, PositionType::Absolute);
    assert_eq!(fill(&app, root), SHADE, "the dimmer keeps the shade token");
    let page_root = parent_of(&app, root);
    assert!(
        app.world()
            .get::<crate::presentation::MenuRoot>(parent_of(&app, page_root))
            .is_some(),
        "the dimmer is the page's own surface root's child"
    );

    let drawn_panel = panel(&app);
    assert_eq!(
        parent_of(&app, drawn_panel),
        root,
        "the panel is the root's child"
    );
    assert_eq!(
        fill(&app, drawn_panel),
        PANEL,
        "the panel keeps the panel token"
    );

    let words = subtree_words(&app, root);
    for word in [
        copy::text("legacy-picker.heading"),
        copy::text("legacy-picker.prompt"),
        copy::text("legacy-picker.cancel"),
    ] {
        assert!(
            words.iter().any(|drawn| drawn.as_str() == word),
            "the picker draws {word:?}: {words:?}"
        );
    }

    assert_no_hand_built_nodes(&app, root);
}

/// The hand-built dialog is gone: its rows were one `Text` root with a `TextSpan`
/// status line, and its controls carried their words on themselves.
fn assert_no_hand_built_nodes(app: &App, root: Entity) {
    for entity in subtree(app, root) {
        assert!(
            app.world().get::<TextSpan>(entity).is_none(),
            "no hand-built status span is left"
        );
        if app.world().get::<MenuAction>(entity).is_some() {
            assert!(
                app.world().get::<Text>(entity).is_none(),
                "a control draws its words in a sibling slot, not on itself"
            );
        }
    }
}

#[test]
fn each_target_row_draws_its_label_detail_and_control() {
    let (mut app, _pack) = app_with_legacy();
    open_picker(&mut app);
    let root = picker_root(&app);
    let targets = menu::legacy_targets(menu(&app));
    assert_eq!(targets.len(), 3, "the three manual slots");

    for target in &targets {
        let action = MenuAction::LegacyTarget(target.slot);
        let control = shown_control(&app, action);
        assert!(within(&app, control, root), "{action:?} is the picker's");
        assert_eq!(
            app.world()
                .get::<crate::presentation::render::ControlLabel>(control)
                .map(|label| label.0.clone()),
            Some(target.display()),
            "{action:?} is labelled by its row's label slot"
        );
        let row = parent_of(&app, control);
        let words = subtree_words(&app, row);
        assert!(
            words.iter().any(|text| text.as_str() == target.display()),
            "{action:?} draws its label {:?}: {words:?}",
            target.display()
        );
        let detail = format!("{} - {}", target.status, target.detail);
        assert!(
            words.iter().any(|text| text.as_str() == detail),
            "{action:?} draws its status line {detail:?}: {words:?}"
        );
    }

    let (mut shown, mut hidden) = (0, 0);
    for entity in subtree(&app, root) {
        if !matches!(
            app.world().get::<MenuAction>(entity),
            Some(MenuAction::LegacyTarget(_))
        ) {
            continue;
        }
        let displays = app
            .world()
            .get::<Node>(entity)
            .is_some_and(|node| node.display != Display::None);
        if displays {
            shown += 1;
        } else {
            hidden += 1;
        }
    }
    assert_eq!(shown, 3, "one shown control per target row");
    assert_eq!(
        hidden, 6,
        "the two siblings the item template places in each row stay hidden"
    );

    let cancel = shown_control(&app, MenuAction::LegacyCancel);
    assert!(
        subtree_words(&app, cancel)
            .iter()
            .any(|text| text.as_str() == copy::text("legacy-picker.cancel")),
        "Cancel draws its words"
    );
}

/// Give the pack the reference look as its own plugin, with the panel token
/// recoloured: one tree, one token, and the picker must follow the token.
fn write_reference_plugin_with_panel(pack: &Path) {
    let ui = pack.join("ui");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-plugin"),
        &ui,
    );
    let tokens = ui.join("tokens.ron");
    let body = std::fs::read_to_string(&tokens).unwrap();
    let edited = body.replace(
        r#""panel": (r: 0.10, g: 0.11, b: 0.15, a: 1.0)"#,
        r#""panel": (r: 0.25, g: 0.50, b: 0.75, a: 1.0)"#,
    );
    assert_ne!(edited, body, "the panel token is where this test edits it");
    std::fs::write(&tokens, edited).unwrap();
}

#[test]
fn a_pack_token_edit_changes_what_the_picker_draws() {
    let (mut reference, _pack) = app_with_legacy();
    open_picker(&mut reference);
    assert_eq!(
        fill(&reference, panel(&reference)),
        PANEL,
        "the reference look's panel token"
    );

    let pack = temp_pack();
    let pack_root = pack.path().join("pack");
    write_reference_plugin_with_panel(&pack_root);
    let mut app = app_for_pack(pack_root);
    write_legacy(&app);
    esc(&mut app);
    open_picker(&mut app);
    assert_eq!(
        fill(&app, panel(&app)),
        RECOLOURED_PANEL,
        "the pack's own panel token is what the picker draws"
    );
}
