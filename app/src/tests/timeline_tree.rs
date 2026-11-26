//! Ticket #117: the Timeline section drawn from the plugin's `timeline` surface.
//!
//! These pin the move into the renderer: the section's root, heading row,
//! explanation, empty line and card list are the tree's nodes, no hand-built
//! header row or back control draws any more, each declared scene is one copy of
//! the tree's card template with its own control and locked state, and a pack's
//! own plugin changes what the section draws (Default look §4, spec §11).

use std::path::Path;

use super::timeline::{
    card_box, card_control, card_fill, card_label, open_timeline, temp_pack_with_order,
};
use super::*;
use unmei_engine::ui::{NodeKind, Padding};

/// The children of `entity`, in spawn order.
fn children_of(app: &App, entity: Entity) -> Vec<Entity> {
    app.world()
        .get::<Children>(entity)
        .map(|children| children.iter().collect())
        .unwrap_or_default()
}

fn parent_of(app: &App, entity: Entity) -> Entity {
    app.world()
        .get::<ChildOf>(entity)
        .map(ChildOf::parent)
        .expect("the node has a parent")
}

/// The page's tree root: [`MenuRoot`]'s first child.
fn page_root(app: &App) -> Entity {
    let menu_root = entity_with::<crate::presentation::MenuRoot>(app);
    *children_of(app, menu_root)
        .first()
        .expect("the root holds the page's tree")
}

/// The Timeline's own view: the section mounted in the contents page's right
/// column body, which is the tree's ledger list.
fn timeline_root(app: &App) -> Entity {
    let column = *children_of(app, page_root(app))
        .last()
        .expect("the page draws a right column");
    let body = *children_of(app, column)
        .last()
        .expect("the right column holds the section's body");
    *children_of(app, body)
        .first()
        .expect("the section draws its own view")
}

/// The one text entity inside `root` whose words are exactly `words`.
fn words_within(app: &App, root: Entity, words: &str) -> Entity {
    let mut stack = vec![root];
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
    panic!("the Timeline draws no {words:?}")
}

/// The drawn Timeline is the plugin's: the root's own column, padding, gap and
/// child count are the tree root's, the heading and walk legend draw in the
/// tree's heading row, the explanation and empty lines are its slots, and the
/// hand-built header row with its `← Back to slots` control is gone because the
/// contents page's left column is the way out (Default look §4, spec §11).
#[test]
fn the_timeline_draws_the_plugins_tree() {
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion", "epilogue"]));
    let mut app = app_for_pack(pack.path().join("pack"));
    open_timeline(&mut app);

    let tree = app
        .world()
        .resource::<crate::presentation::render::UiLook>()
        .surface("timeline")
        .cloned()
        .expect("the reference plugin authors the timeline surface");
    let NodeKind::Box(root) = &tree.kind else {
        panic!("the timeline tree's root is a box");
    };
    let Padding::Uniform(padding) = root.padding else {
        panic!("the timeline root pads all sides");
    };

    let drawn = timeline_root(&app);
    let node = app
        .world()
        .get::<Node>(drawn)
        .expect("the timeline root node");
    assert_eq!(node.flex_direction, FlexDirection::Column);
    assert_eq!(node.width, Val::Percent(100.0));
    assert_eq!(node.height, Val::Percent(100.0));
    assert_eq!(node.row_gap, Val::Px(root.gap));
    assert_eq!(
        node.padding,
        UiRect::ZERO,
        "the seat pads the column, so the tree's own {padding}px page padding is cleared"
    );
    assert_eq!(
        children_of(&app, drawn).len(),
        root.children.len(),
        "every child of the tree root draws"
    );

    let heading = words_within(&app, drawn, menu::TIMELINE_HEADING);
    let legend = words_within(&app, drawn, menu::TIMELINE_LEGEND);
    let row = parent_of(&app, heading);
    let NodeKind::Box(heading_row) = &root.children[0].kind else {
        panic!("the tree root's first child is the heading row");
    };
    assert_eq!(parent_of(&app, legend), row, "the legend shares the row");
    assert_eq!(parent_of(&app, row), drawn);
    let row_node = app.world().get::<Node>(row).unwrap();
    assert_eq!(row_node.flex_direction, FlexDirection::Row);
    assert_eq!(row_node.column_gap, Val::Px(heading_row.gap));
    assert_eq!(row_node.align_items, AlignItems::Center);

    let explanation = words_within(&app, drawn, menu::TIMELINE_EXPLANATION);
    assert_eq!(
        parent_of(&app, parent_of(&app, explanation)),
        drawn,
        "the explanation is the tree's own slot"
    );
    let empty = words_within(&app, drawn, menu::TIMELINE_EMPTY);
    let empty_slot = parent_of(&app, empty);
    assert_eq!(parent_of(&app, empty_slot), drawn);
    assert_eq!(
        app.world().get::<Node>(empty_slot).unwrap().display,
        Display::None,
        "the empty line hides while the pack declares scenes"
    );

    let cards = *children_of(&app, drawn).last().expect("the card list");
    assert_eq!(
        cards,
        entity_with::<crate::presentation::timeline::TimelineBody>(&app),
        "the tree's card list is the scroll body the engine mounts"
    );

    assert!(
        !has_action(&app, MenuAction::TimelineBack),
        "the hand-built back control is gone"
    );
    assert!(
        !all_texts(&app)
            .iter()
            .any(|text| text == menu::TIMELINE_BACK),
        "the removed back label does not draw"
    );
}

/// The tree's card list carries one control per declared scene, each the tree's
/// own control for that card, and the locked card's control is disabled with the
/// exact `timeline.locked` label (Default look §4, spec §11).
#[test]
fn the_trees_card_list_carries_one_control_per_card_and_the_locked_state() {
    let pack = temp_pack_with_order(Some(&["first_meeting", "reunion", "epilogue"]));
    let root = pack.path().join("pack");
    seed_visited(&root, &["epilogue"]);
    let mut app = app_for_pack(root);
    open_timeline(&mut app);

    let grid = entity_with::<crate::presentation::timeline::TimelineGrid>(&app);
    let controls: Vec<(usize, Entity)> = app
        .world()
        .iter_entities()
        .filter_map(|entity| {
            let MenuAction::TimelineCard(index) = entity.get::<MenuAction>()? else {
                return None;
            };
            entity.get::<Button>()?;
            Some((*index, entity.id()))
        })
        .collect();
    assert_eq!(controls.len(), 3, "one control per declared scene");
    for (index, control) in &controls {
        assert_eq!(
            *control,
            card_control(&app, *index),
            "each control is its own card's"
        );
        assert_eq!(parent_of(&app, card_box(&app, *index)), grid);
    }

    for (index, drawn) in [(0, "first_meeting"), (1, "???"), (2, "epilogue")] {
        assert_eq!(
            app.world()
                .get::<Text>(card_label(&app, index))
                .map(|text| text.0.as_str()),
            Some(drawn),
            "the tree's label slot draws the card's words"
        );
    }

    let disabled = |index: usize| {
        app.world()
            .get::<crate::presentation::render::UiStates>(card_control(&app, index))
            .is_some_and(|states| states.disabled)
    };
    assert!(disabled(1), "the locked card's control is disabled");
    assert!(!disabled(0), "a visited card's control is enabled");
    assert!(!disabled(2));
    assert_eq!(
        crate::keiko::controls::id_for_focus(MenuFocus::TimelineCard(1)),
        Some("timeline.card.2".to_string()),
        "the disabled card keeps its Keiko id"
    );
}

/// A pack's own plugin changes what the section draws: the same view over a
/// copied plugin whose card token is magenta paints magenta cards, not the
/// reference look's (Default look §4, spec §11).
#[test]
fn a_packs_own_plugin_changes_what_the_timeline_draws() {
    let dir = temp_pack_with_order(Some(&["first_meeting", "reunion"]));
    let pack_root = dir.path().join("pack");
    let ui = pack_root.join("ui");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-plugin"),
        &ui,
    );
    let tokens = ui.join("tokens.ron");
    let reference = std::fs::read_to_string(&tokens).expect("the reference tokens");
    let edited = reference.replace(
        "\"card\": (r: 0.12, g: 0.13, b: 0.17, a: 1.0),",
        "\"card\": (r: 1.0, g: 0.0, b: 1.0, a: 1.0),",
    );
    assert_ne!(
        reference, edited,
        "the reference card token is the one edited"
    );
    std::fs::write(&tokens, edited).expect("the edited tokens");

    let mut app = boot_app(pack_root, None);
    app.insert_resource(TempPack(dir));
    enter_play(&mut app);
    open_timeline(&mut app);

    assert_eq!(
        card_fill(&app, 0),
        Color::srgba(1.0, 0.0, 1.0, 1.0),
        "the pack's own card token draws"
    );
    assert_ne!(
        card_fill(&app, 0),
        Color::srgba(0.12, 0.13, 0.17, 1.0),
        "the reference look's card fill is replaced"
    );
}
