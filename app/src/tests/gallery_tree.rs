//! Ticket #117: the Gallery and the still view drawn from the plugin's
//! `gallery` and `still` trees.
//!
//! These pin the hand-off itself: the surface's root is the tree's root, mounted
//! in the contents page's right column; the heading, the legend and the empty
//! line are the tree's own nodes; the still's entry, art slot, label and legend
//! are the `still` tree's; and an edit to a pack's own tree changes what the view
//! draws. The frame the player used to build by hand — the header row with its
//! door-aware back control — is gone from the surface, because Default look §4
//! gives the way back to the page's left column.

use super::*;

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
        .expect("the entity has a parent")
}

/// The contents page's right column body: the node the section's own view is
/// mounted into (`docs/title-and-menu.md` § Default look §4).
fn page_body(app: &App) -> Entity {
    let menu_root = app
        .world()
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<crate::presentation::MenuRoot>()
                .map(|_| entity.id())
        })
        .expect("the page draws a root");
    let page = children_of(app, menu_root)
        .into_iter()
        .next()
        .expect("the root holds the page's tree");
    let column = *children_of(app, page)
        .last()
        .expect("the page has a right column");
    children_of(app, column)
        .get(1)
        .copied()
        .expect("the right column holds its heading and its body")
}

/// The scrolling card body the player mounts into the tree's `gallery.cards` list.
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

/// The gallery surface's root: the plugin tree's root.
fn gallery_root(app: &App) -> Entity {
    parent_of(app, gallery_body(app))
}

/// Open the overlay, then its `Gallery` entry. Assumes the overlay is closed.
fn open_overlay_gallery(app: &mut App) {
    esc(app);
    assert_eq!(menu(app).view, MenuView::Slots);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryButton;
    press_and_update(app, KeyCode::Enter);
    assert_eq!(menu(app).view, MenuView::Gallery);
}

/// The Gallery section draws the plugin's tree: its root is the tree's root in
/// the page's right column, the heading row, the empty line and the card body are
/// the tree's nodes, and the hand-built header row with its back control is gone
/// (`docs/title-and-menu.md` § Default look §4, § Gallery `### Layout` §1).
#[test]
fn the_gallery_draws_the_plugins_tree() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    let root = gallery_root(&app);
    assert_eq!(
        parent_of(&app, root),
        page_body(&app),
        "§4 seats the surface in the page's right column"
    );
    let root_node = app.world().get::<Node>(root).unwrap();
    assert_eq!(
        root_node.flex_direction,
        FlexDirection::Column,
        "the tree's root is a column"
    );
    assert_eq!(
        root_node.row_gap,
        Val::Px(16.0),
        "the tree's section gap separates its children"
    );
    assert_eq!(
        root_node.padding,
        UiRect::ZERO,
        "the seat pads, so the surface adds no second inset"
    );

    let children = children_of(&app, root);
    assert_eq!(
        children.len(),
        3,
        "the tree places a heading row, the empty line and the card list"
    );
    let heading_row = children[0];
    assert_eq!(
        app.world()
            .get::<Node>(heading_row)
            .unwrap()
            .justify_content,
        JustifyContent::SpaceBetween,
        "the tree's heading row holds the heading and the legend at its edges"
    );
    let words: Vec<String> = children_of(&app, heading_row)
        .into_iter()
        .filter_map(|child| app.world().get::<Text>(child).map(|text| text.0.clone()))
        .collect();
    assert_eq!(
        words,
        [menu::GALLERY_HEADING, menu::GALLERY_GRID_LEGEND],
        "the heading and the grid legend are the tree's text nodes"
    );
    assert_eq!(
        app.world().get::<Node>(children[1]).unwrap().display,
        Display::None,
        "the tree's empty line is the slot the pack's art hides"
    );
    assert_eq!(
        children[2],
        gallery_body(&app),
        "the tree's card list is the body the grid fills"
    );

    assert!(
        !has_action(&app, MenuAction::GalleryBack),
        "§4 gives the way back to the page's left column"
    );
    assert!(
        !all_texts(&app).iter().any(|text| text
            == menu::gallery_back_label(menu::MenuOrigin::Slots)
            || text == menu::gallery_back_label(menu::MenuOrigin::Title)),
        "the surface draws no back label of its own"
    );

    // The cards themselves are still one numbered item per entry, each with the
    // art box, the label and the control the format numbers for it.
    for (index, entry) in app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()
        .iter()
        .enumerate()
    {
        assert!(
            has_action(&app, MenuAction::GalleryCard(index)),
            "card {index} carries its control"
        );
        let panel = parent_of(&app, action_entity(&app, MenuAction::GalleryCard(index)));
        assert_eq!(
            flattened_text(&app, panel),
            entry.label(),
            "card {index} draws its entry's label"
        );
    }
}

/// The still draws its tree's entry: one `still.entry` item holding the art slot
/// and the label above the tree's legend, with the surface root as the one click
/// target (`docs/title-and-menu.md` § Gallery `### Layout` §6/§7,
/// `docs/ui-plugin-format.md` § `still`).
#[test]
fn the_still_draws_its_tree_entry_and_legend() {
    let pack = temp_pack();
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::GalleryCard(1);
    press_and_update(&mut app, KeyCode::Enter);
    assert!(menu(&app).gallery_still, "the still opened");
    build_pause_tree(&mut app);
    let label = app
        .world()
        .resource::<Runtime>()
        .runner
        .pack()
        .gallery_entries()[1]
        .label();

    let root = action_entity(&app, MenuAction::GalleryStillClose);
    let root_node = app.world().get::<Node>(root).unwrap();
    assert_eq!(
        root_node.flex_direction,
        FlexDirection::Column,
        "the tree's root is a column"
    );
    assert_eq!(
        root_node.row_gap,
        Val::Px(6.0),
        "the tree's caption gap separates its children"
    );
    assert_eq!(
        root_node.align_items,
        AlignItems::Center,
        "the tree centres the still"
    );
    assert_eq!(root_node.padding, UiRect::ZERO, "the seat pads");

    let children = children_of(&app, root);
    assert_eq!(
        children.len(),
        2,
        "the tree places the entry list and the still legend"
    );
    let list = children[0];
    let items = children_of(&app, list);
    assert_eq!(items.len(), 1, "the still shows one entry");
    let item = items[0];
    let parts = children_of(&app, item);
    assert_eq!(
        parts.len(),
        2,
        "the item places the art slot above the caption strip"
    );
    assert!(
        app.world()
            .get::<crate::presentation::gallery::GalleryStillArtBox>(parts[0])
            .is_some(),
        "the tree's `still.art` slot is the fitted art box"
    );
    let foot = parts[1];
    let foot_node = app.world().get::<Node>(foot).unwrap();
    assert_eq!(
        foot_node.flex_direction,
        FlexDirection::Column,
        "the caption strip is a column"
    );
    let label_slot = children_of(&app, foot)
        .into_iter()
        .next()
        .expect("the strip holds the label slot");
    let caption = children_of(&app, label_slot)
        .into_iter()
        .next()
        .expect("the label slot draws its words");
    assert!(
        app.world()
            .get::<crate::presentation::gallery::GalleryStillCaption>(caption)
            .is_some(),
        "the drawn `still.label` is the caption the browse step repaints"
    );
    assert_eq!(
        app.world().get::<Text>(caption).unwrap().0,
        label,
        "the caption is the entry's label, verbatim"
    );
    assert_eq!(
        app.world().get::<Text>(children[1]).unwrap().0,
        menu::GALLERY_STILL_LEGEND,
        "the still legend is the tree's text node"
    );
}

/// Install the reference look as the pack's own plugin with one edit to the
/// gallery tree: the card item's fill becomes the `panel` tone instead of the
/// `card` one.
fn install_gallery_tree_edit(pack: &Path) {
    let ui = pack.join("ui");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-plugin"),
        &ui,
    );
    let path = ui.join("surfaces/gallery.ron");
    let tree = std::fs::read_to_string(&path).unwrap();
    let edited = tree.replace(
        "component: \"card\",\n",
        "component: \"card\",\n                fill: \"{colors.panel}\",\n",
    );
    assert_ne!(edited, tree, "the fixture edits the card's node");
    std::fs::write(&path, edited).unwrap();
}

/// A pack's own tree decides what the view draws: editing the card's fill in the
/// pack's `gallery.ron` changes the drawn card, so the surface is the tree's and
/// not the player's (`docs/ui-plugin-spec.md` §11).
#[test]
fn a_tree_edit_in_the_pack_changes_what_the_gallery_draws() {
    let pack = temp_pack();
    install_gallery_tree_edit(&pack.path().join("pack"));
    let mut app = app_for_pack(pack.path().join("pack"));
    open_overlay_gallery(&mut app);
    build_pause_tree(&mut app);

    let panel = parent_of(&app, action_entity(&app, MenuAction::GalleryCard(0)));
    assert_eq!(
        app.world().get::<BackgroundColor>(panel).unwrap().0,
        Color::srgb(0.10, 0.11, 0.15),
        "the pack's card fill is the tree's, not the reference look's"
    );
}
