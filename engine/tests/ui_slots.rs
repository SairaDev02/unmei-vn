//! Ticket #107's static rules: the per-surface interface tables, the slot and
//! control rules a tree obeys against them, the copy overrides, the copy cap and
//! `numbering` (UI plugin spec §5, §15 rules 6 and 7).
//!
//! Every case loads through [`ui::load_from`] with a memory source, so the tests
//! run the same passes the check, the boot and the export run. The fixtures are
//! synthetic: no case depends on the demo pack or on a shipped plugin.
//!
//! Two families of case live here. The table cases derive a tree from
//! [`ui::SURFACES`] itself, which proves the tables are satisfiable and that a
//! tree placing what they allow is accepted. The rule cases break one thing in
//! such a tree, so each finding is attributable to the rule under test.

use std::collections::BTreeSet;

use unmei_engine::copy;
use unmei_engine::ui::{self, ListSpec, Numbering, PluginOutcome, SurfaceSpec};

/// The smallest manifest: the format version is the only required field.
const MANIFEST: &str = "(format_version: 1)";

/// One typography token, so a `text` node has a face to draw with.
const TOKENS: &str = r#"(
    typography: {
        "body": (face: "IBM Plex Sans", weight: 400, size_px: 25.0, line_height_px: 38.0),
    },
)"#;

/// One plugin: the tokens file, then each `(id, tree)` as `surfaces/<id>.ron`.
fn load(manifest: &str, surfaces: &[(String, String)]) -> PluginOutcome {
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("plugin.ron".to_string(), manifest.as_bytes().to_vec()),
        ("tokens.ron".to_string(), TOKENS.as_bytes().to_vec()),
    ];
    for (id, tree) in surfaces {
        files.push((format!("surfaces/{id}.ron"), tree.as_bytes().to_vec()));
    }
    ui::load_from(&ui::MemorySource::new(files))
}

/// One surface's tree, named `ui/surfaces/<id>.ron` in its findings.
fn load_one(manifest: &str, id: &str, tree: &str) -> PluginOutcome {
    load(manifest, &[(id.to_string(), tree.to_string())])
}

/// `(file, path, rule)` for each finding: the shape rule 12 fixes.
fn reported(outcome: &PluginOutcome) -> Vec<(String, String, String)> {
    outcome
        .findings()
        .iter()
        .map(|finding| {
            (
                finding.file.clone(),
                finding.path.to_string(),
                finding.rule.clone(),
            )
        })
        .collect()
}

fn messages(outcome: &PluginOutcome) -> String {
    outcome
        .findings()
        .iter()
        .map(|finding| finding.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn plugin(outcome: &PluginOutcome) -> &ui::UiPlugin {
    outcome
        .plugin()
        .unwrap_or_else(|| panic!("the plugin loads: {}", messages(outcome)))
}

fn spec(id: &str) -> &'static SurfaceSpec {
    ui::surface(id).unwrap_or_else(|| panic!("the table has a `{id}` surface"))
}

// --- the trees the tables describe -----------------------------------------

fn root(children: &[String]) -> String {
    format!("(kind: \"box\", children: [{}])", children.join(", "))
}

fn slot(name: &str) -> String {
    format!("(kind: \"slot\", slot: \"{name}\")")
}

/// The member of an indexed family a tree names: `gallery.card.<n>` is placed as
/// `gallery.card.1`, and the engine shows the row's own member.
fn member(declared: &str) -> String {
    match declared.strip_suffix("<n>") {
        Some(family) => format!("{family}1"),
        None => declared.to_string(),
    }
}

/// One list node whose item template places `extra` after the listed slots.
fn list_node(list: &ListSpec, extra: &[String]) -> String {
    let mut children: Vec<String> = list.item_slots.iter().map(|name| slot(name)).collect();
    children.extend(list.item_controls.iter().map(|name| slot(&member(name))));
    children.extend(extra.iter().cloned());
    format!(
        "(kind: \"list\", list: \"{}\", item: {})",
        list.slot,
        root(&children)
    )
}

/// The root children that place exactly what `spec` requires, plus every
/// optional slot when `optional`.
fn spec_children(spec: &SurfaceSpec, optional: bool) -> Vec<String> {
    let mut children: Vec<String> = Vec::new();
    for name in spec.slots {
        match spec.lists.iter().find(|list| list.slot == *name) {
            Some(list) => children.push(list_node(list, &[])),
            None => children.push(slot(name)),
        }
    }
    if optional {
        children.extend(spec.optional_slots.iter().map(|name| slot(name)));
    }
    for control in spec.controls {
        // A control a placed list's item template carries is placed there, and
        // one that is also a required slot is already placed above.
        let in_list = spec
            .lists
            .iter()
            .any(|list| list.item_controls.contains(control));
        if !in_list && !spec.slots.contains(control) {
            children.push(slot(control));
        }
    }
    children
}

fn spec_tree(spec: &SurfaceSpec, optional: bool, extra: &[String]) -> String {
    let mut children = spec_children(spec, optional);
    children.extend(extra.iter().cloned());
    root(&children)
}

/// The tree that places exactly what `id`'s surface requires, with `extras`
/// appended at the root, and the root index of the first extra.
fn tree_with(id: &str, extras: &[String]) -> (String, usize) {
    let spec = spec(id);
    let mut children = spec_children(spec, false);
    let index = children.len();
    children.extend(extras.iter().cloned());
    (root(&children), index)
}

// --- the tables are satisfiable and complete -------------------------------

/// The tables are satisfiable: ten trees, each placing exactly what its surface
/// requires, load with no finding.
#[test]
fn a_tree_placing_the_required_interface_loads() {
    let surfaces: Vec<(String, String)> = ui::SURFACES
        .iter()
        .map(|spec| (spec.id.to_string(), spec_tree(spec, false, &[])))
        .collect();
    let outcome = load(MANIFEST, &surfaces);
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
}

/// An optional slot is allowed wherever the table lists one.
#[test]
fn a_tree_placing_every_optional_slot_loads() {
    let surfaces: Vec<(String, String)> = ui::SURFACES
        .iter()
        .map(|spec| (spec.id.to_string(), spec_tree(spec, true, &[])))
        .collect();
    let outcome = load(MANIFEST, &surfaces);
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
}

/// One table per surface, in the surface id order the tree pass already holds.
#[test]
fn every_surface_has_one_table_in_the_surface_order() {
    let ids: Vec<&str> = ui::SURFACES.iter().map(|spec| spec.id).collect();
    assert_eq!(ids, ui::SURFACE_IDS.to_vec());
}

/// Every control the tables name is a Keiko id, and every list's slot is a slot
/// its surface requires.
#[test]
fn the_tables_name_keiko_controls_and_required_slots() {
    for spec in ui::SURFACES {
        for control in spec.controls {
            assert!(
                unmei_engine::keiko::is_control_id(control),
                "{} names the control `{control}`, which Keiko does not have",
                spec.id
            );
        }
        for list in spec.lists {
            assert!(
                spec.slots.contains(&list.slot),
                "{} has a list at `{}`, which is not a required slot",
                spec.id,
                list.slot
            );
            for control in list.item_controls {
                let placed = member(control);
                assert!(
                    unmei_engine::keiko::is_control_id(&placed),
                    "{}'s list `{}` names the control `{control}`, which Keiko does not have",
                    spec.id,
                    list.slot
                );
            }
            assert!(
                !list.item_slots.is_empty() || !list.item_controls.is_empty(),
                "{}'s list `{}` places nothing per item",
                spec.id,
                list.slot
            );
        }
    }
}

/// Every copy key the table holds is owned by the surface its id names, and the
/// one key family two surfaces share is `settings.*`, which the `title-settings`
/// door draws from the `settings` body.
#[test]
fn every_copy_key_belongs_to_its_surface() {
    for key in copy::KEYS {
        let owners: Vec<&str> = ui::SURFACES
            .iter()
            .filter(|spec| spec.copy_keys.contains(&key.id))
            .map(|spec| spec.id)
            .collect();
        assert!(!owners.is_empty(), "no surface may use `{}`", key.id);
        let prefix = key.id.split('.').next().expect("a dotted id");
        if prefix == "settings" {
            assert_eq!(owners, ["title-settings", "settings"], "`{}`", key.id);
        } else {
            assert_eq!(owners, [prefix], "`{}` is owned by one surface", key.id);
        }
    }
}

/// No surface lists one key, one slot or one control twice.
#[test]
fn no_table_lists_one_name_twice() {
    for spec in ui::SURFACES {
        for names in [
            spec.slots,
            spec.optional_slots,
            spec.controls,
            spec.copy_keys,
        ] {
            let mut seen = BTreeSet::new();
            for name in names {
                assert!(seen.insert(*name), "{} lists `{name}` twice", spec.id);
            }
        }
    }
}

/// The two ornament slots are the play frame's markers, and each one is a slot
/// the `play` surface requires.
#[test]
fn the_ornament_slots_are_the_play_markers() {
    assert_eq!(ui::ORNAMENT_SLOTS, ["advance-marker", "deciding-marker"]);
    let play = spec("play");
    for marker in ui::ORNAMENT_SLOTS {
        assert!(play.slots.contains(&marker), "`{marker}` is a play slot");
    }
}

// --- rule 6: slots and controls --------------------------------------------

/// A slot no table names fails with `ui.slot.unknown`, at the node's own path.
#[test]
fn an_unknown_slot_fails() {
    let (tree, index) = tree_with("play", &[slot("proze")]);
    let outcome = load_one(MANIFEST, "play", &tree);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            format!("root.children[{index}].slot"),
            "ui.slot.unknown".to_string()
        )]
    );
    assert!(
        messages(&outcome).contains("proze"),
        "the message names the slot: {}",
        messages(&outcome)
    );
}

/// A slot named for another surface's control is equally unknown: a plugin may
/// only place the controls its own surface has.
#[test]
fn another_surfaces_control_is_an_unknown_slot() {
    let (tree, index) = tree_with("play", &[slot("settings.mute")]);
    let outcome = load_one(MANIFEST, "play", &tree);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            format!("root.children[{index}].slot"),
            "ui.slot.unknown".to_string()
        )]
    );
}

/// A required slot left out fails with `ui.slot.missing`, and the finding is the
/// tree's own: no node carries it.
#[test]
fn a_missing_required_slot_fails() {
    let mut children = spec_children(spec("play"), false);
    children.retain(|child| child != &slot("prose"));
    let outcome = load_one(MANIFEST, "play", &root(&children));
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            String::new(),
            "ui.slot.missing".to_string()
        )]
    );
    assert!(
        messages(&outcome).contains("prose"),
        "the message names the slot: {}",
        messages(&outcome)
    );
}

/// A required control left out fails with `ui.slot.missing`.
#[test]
fn a_missing_required_control_fails() {
    let spec = spec("play");
    let mut children: Vec<String> = spec
        .slots
        .iter()
        .map(
            |name| match spec.lists.iter().find(|list| list.slot == *name) {
                Some(list) => list_node(list, &[]),
                None => slot(name),
            },
        )
        .collect();
    children.extend(
        spec.controls
            .iter()
            .filter(|name| **name != "play.skip")
            .map(|name| slot(name)),
    );
    let outcome = load_one(MANIFEST, "play", &root(&children));
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            String::new(),
            "ui.slot.missing".to_string()
        )]
    );
    assert!(
        messages(&outcome).contains("play.skip"),
        "the message names the control: {}",
        messages(&outcome)
    );
}

/// A required control is placed by the list that declares it as an item control,
/// so its surface does not also place it at the top level.
#[test]
fn a_list_s_item_control_places_a_required_control() {
    let spec = spec("title");
    let tree = spec_tree(spec, false, &[]);
    assert!(
        !tree.contains("slot: \"title.new-game\"")
            || tree.matches("slot: \"title.new-game\"").count() == 1,
        "the generated tree places the row control once"
    );
    let outcome = load_one(MANIFEST, "title", &tree);
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
}

/// The same control placed twice fails with `ui.slot.duplicate`.
#[test]
fn a_control_placed_twice_fails() {
    let (tree, index) = tree_with("play", &[slot("play.save")]);
    let outcome = load_one(MANIFEST, "play", &tree);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            format!("root.children[{index}].slot"),
            "ui.slot.duplicate".to_string()
        )]
    );
}

/// The same slot placed twice fails with `ui.slot.duplicate`.
#[test]
fn a_slot_placed_twice_fails() {
    let (tree, _) = tree_with("play", &[slot("speaker")]);
    let outcome = load_one(MANIFEST, "play", &tree);
    assert_eq!(
        reported(&outcome).len(),
        0,
        "an optional slot may be placed"
    );

    let mut children = spec_children(spec("play"), false);
    children.retain(|child| child != &slot("prose"));
    let index = children.len();
    children.push(slot("prose"));
    children.push(slot("prose"));
    let outcome = load_one(MANIFEST, "play", &root(&children));
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            format!("root.children[{}].slot", index + 1),
            "ui.slot.duplicate".to_string()
        )],
        "{}",
        messages(&outcome)
    );
}

/// Every declared item slot and item control is required in the item template.
#[test]
fn an_item_template_that_omits_an_item_slot_fails() {
    let gallery = spec("gallery");
    let list = &gallery.lists[0];
    let tree = root(&[format!(
        "(kind: \"list\", list: \"{}\", item: {})",
        list.slot,
        root(&[slot("gallery.card.art"), slot("gallery.card.1")])
    )]);
    let outcome = load_one(MANIFEST, "gallery", &tree);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/gallery.ron".to_string(),
            String::new(),
            "ui.slot.missing".to_string()
        )]
    );
    assert!(
        messages(&outcome).contains("gallery.card.label"),
        "the message names the item slot: {}",
        messages(&outcome)
    );
}

/// A list's item control is as required as its item slots: a gallery item that
/// places both of its slots and no card control fails.
#[test]
fn an_item_template_that_omits_an_item_control_fails() {
    let gallery = spec("gallery");
    let list = &gallery.lists[0];
    let tree = root(&[format!(
        "(kind: \"list\", list: \"{}\", item: {})",
        list.slot,
        root(&[slot("gallery.card.label"), slot("gallery.card.art")])
    )]);
    let outcome = load_one(MANIFEST, "gallery", &tree);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/gallery.ron".to_string(),
            String::new(),
            "ui.slot.missing".to_string()
        )]
    );
    assert!(
        messages(&outcome).contains("gallery.card.<n>"),
        "the message names the item control: {}",
        messages(&outcome)
    );
}

/// A list the surface does not have fails with `ui.slot.unknown`.
#[test]
fn an_unknown_list_fails() {
    let (tree, index) = tree_with(
        "play",
        &[r#"(kind: "list", list: "title.rows", item: (kind: "box"))"#.to_string()],
    );
    let outcome = load_one(MANIFEST, "play", &tree);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            format!("root.children[{index}].list"),
            "ui.slot.unknown".to_string()
        )]
    );
}

/// A control a list's item template places is one control on the surface, so a
/// tree that also places it at the top level places it twice.
#[test]
fn a_control_placed_at_the_top_level_and_in_a_list_fails() {
    let mut children = spec_children(spec("title"), false);
    let index = children.len();
    children.push(slot("title.continue"));
    let outcome = load_one(MANIFEST, "title", &root(&children));
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/title.ron".to_string(),
            format!("root.children[{index}].slot"),
            "ui.slot.duplicate".to_string()
        )],
        "{}",
        messages(&outcome)
    );
}

/// An item slot the list does not declare fails with `ui.slot.unknown`, at the
/// node inside the item template.
#[test]
fn an_unknown_item_slot_fails() {
    let gallery = spec("gallery");
    let list = &gallery.lists[0];
    let tree = root(&[list_node(list, &[slot("gallery.card.nope")])]);
    let outcome = load_one(MANIFEST, "gallery", &tree);
    let reported = reported(&outcome);
    assert_eq!(
        reported
            .iter()
            .map(|(_, path, rule)| (path.as_str(), rule.as_str()))
            .collect::<Vec<_>>(),
        [("root.children[0].item.children[3].slot", "ui.slot.unknown")]
    );
}

/// An indexed family is one control per item: a template placing two members
/// places the item's control twice.
#[test]
fn two_members_of_one_family_are_a_duplicate() {
    let gallery = spec("gallery");
    let list = &gallery.lists[0];
    let tree = root(&[list_node(list, &[slot("gallery.card.2")])]);
    let outcome = load_one(MANIFEST, "gallery", &tree);
    assert_eq!(
        reported(&outcome).len(),
        1,
        "one finding: {}",
        messages(&outcome)
    );
    assert_eq!(reported(&outcome)[0].2, "ui.slot.duplicate");
}

// --- rule 7: copy keys -----------------------------------------------------

/// A `text` node names a copy key the copy table holds and the surface may use.
#[test]
fn a_text_node_naming_its_surface_s_copy_key_loads() {
    let (tree, _) = tree_with(
        "play",
        &["(kind: \"text\", text: \"play.help\", typography: \"{typography.body}\")".to_string()],
    );
    let outcome = load_one(MANIFEST, "play", &tree);
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
}

/// A `text` node naming a key no table holds fails with `ui.copy.unknown`.
#[test]
fn a_text_node_naming_an_unknown_copy_key_fails() {
    let (tree, index) = tree_with(
        "play",
        &["(kind: \"text\", text: \"play.nope\", typography: \"{typography.body}\")".to_string()],
    );
    let outcome = load_one(MANIFEST, "play", &tree);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            format!("root.children[{index}].text"),
            "ui.copy.unknown".to_string()
        )]
    );
}

/// A `text` node naming another surface's key is the same failure: a surface
/// may only show its own copy.
#[test]
fn a_text_node_naming_another_surface_s_key_fails() {
    let (tree, index) = tree_with(
        "play",
        &[
            "(kind: \"text\", text: \"confirm.quit.title\", typography: \"{typography.body}\")"
                .to_string(),
        ],
    );
    let outcome = load_one(MANIFEST, "play", &tree);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            format!("root.children[{index}].text"),
            "ui.copy.unknown".to_string()
        )]
    );
}

fn manifest_with_copy(entries: &str) -> String {
    format!("(format_version: 1, copy: {{ {entries} }})")
}

/// An override of a key the table holds loads, and it is the plugin's text.
#[test]
fn an_override_of_a_held_key_loads() {
    let manifest = manifest_with_copy("\"play.deciding\": \"{npc} ponders.\"");
    let outcome = load_one(&manifest, "play", &spec_tree(spec("play"), false, &[]));
    let plugin = plugin(&outcome);
    assert_eq!(
        plugin
            .manifest
            .copy
            .get("play.deciding")
            .map(String::as_str),
        Some("{npc} ponders.")
    );
}

/// An unknown override key fails with `ui.copy.unknown`, at the manifest entry.
#[test]
fn an_override_with_an_unknown_key_fails() {
    let manifest = manifest_with_copy("\"play.nope\": \"text\"");
    let outcome = load_one(&manifest, "play", &spec_tree(spec("play"), false, &[]));
    assert_eq!(
        reported(&outcome),
        [(
            "ui/plugin.ron".to_string(),
            "copy[\"play.nope\"]".to_string(),
            "ui.copy.unknown".to_string()
        )]
    );
}

/// An override that drops a placeholder the key declares fails with
/// `ui.copy.placeholder`.
#[test]
fn an_override_that_drops_a_placeholder_fails() {
    let manifest = manifest_with_copy("\"confirm.overwrite.prompt\": \"Overwrite it?\"");
    let outcome = load_one(
        &manifest,
        "confirm",
        &spec_tree(spec("confirm"), false, &[]),
    );
    assert_eq!(
        reported(&outcome),
        [(
            "ui/plugin.ron".to_string(),
            "copy[\"confirm.overwrite.prompt\"]".to_string(),
            "ui.copy.placeholder".to_string()
        )]
    );
    assert!(
        messages(&outcome).contains("{slot}"),
        "the message names the placeholder: {}",
        messages(&outcome)
    );
}

/// An override that names a placeholder the key does not declare fails with
/// `ui.copy.placeholder`.
#[test]
fn an_override_that_adds_a_placeholder_fails() {
    let manifest = manifest_with_copy("\"play.deciding\": \"{npc} and {who} confer\"");
    let outcome = load_one(&manifest, "play", &spec_tree(spec("play"), false, &[]));
    assert_eq!(
        reported(&outcome),
        [(
            "ui/plugin.ron".to_string(),
            "copy[\"play.deciding\"]".to_string(),
            "ui.copy.placeholder".to_string()
        )]
    );
    assert!(
        messages(&outcome).contains("{who}"),
        "the message names the placeholder: {}",
        messages(&outcome)
    );
}

/// The placeholders are the key's, not their order: an override that moves one
/// keeps the contract and loads.
#[test]
fn an_override_may_reorder_the_placeholders() {
    let manifest = manifest_with_copy("\"title.continue.autosave\": \"{saved_at}, in {scene}\"");
    let outcome = load_one(&manifest, "title", &spec_tree(spec("title"), false, &[]));
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
}

/// An override at the cap passes and one character more fails with `ui.cap`,
/// which names the cap and the value.
#[test]
fn an_override_over_the_copy_cap_fails() {
    let at_cap = manifest_with_copy(&format!("\"play.end-of-scene\": \"{}\"", "x".repeat(512)));
    let outcome = load_one(&at_cap, "play", &spec_tree(spec("play"), false, &[]));
    assert_eq!(reported(&outcome), [], "512 characters pass the cap");

    let over = manifest_with_copy(&format!("\"play.end-of-scene\": \"{}\"", "x".repeat(513)));
    let outcome = load_one(&over, "play", &spec_tree(spec("play"), false, &[]));
    assert_eq!(
        reported(&outcome),
        [(
            "ui/plugin.ron".to_string(),
            "copy[\"play.end-of-scene\"]".to_string(),
            "ui.cap".to_string()
        )]
    );
    let message = messages(&outcome);
    assert!(
        message.contains("513") && message.contains("512"),
        "the message names the value and the cap: {message}"
    );
}

// --- numbering -------------------------------------------------------------

/// Each of the five authored styles resolves to its own value.
#[test]
fn every_numbering_style_resolves() {
    for (authored, expected) in [
        ("arabic", Numbering::Arabic),
        ("roman-upper", Numbering::RomanUpper),
        ("roman-lower", Numbering::RomanLower),
        ("alpha-upper", Numbering::AlphaUpper),
        ("alpha-lower", Numbering::AlphaLower),
    ] {
        let manifest = format!("(format_version: 1, numbering: \"{authored}\")");
        let outcome = load_one(&manifest, "play", &spec_tree(spec("play"), false, &[]));
        let plugin = plugin(&outcome);
        assert_eq!(plugin.manifest.numbering, expected, "`{authored}`");
    }
}

/// The manifest's `numbering` is optional and defaults to Arabic.
#[test]
fn a_manifest_without_numbering_is_arabic() {
    let outcome = load_one(MANIFEST, "play", &spec_tree(spec("play"), false, &[]));
    assert_eq!(plugin(&outcome).manifest.numbering, Numbering::Arabic);
}

/// Any other value is `ui.parse` at the manifest's own field, and the message
/// names the five styles.
#[test]
fn an_unknown_numbering_style_fails() {
    let outcome = load_one(
        "(format_version: 1, numbering: \"roman\")",
        "play",
        &spec_tree(spec("play"), false, &[]),
    );
    assert_eq!(
        reported(&outcome),
        [(
            "ui/plugin.ron".to_string(),
            "numbering".to_string(),
            "ui.parse".to_string()
        )]
    );
    let message = messages(&outcome);
    for style in Numbering::STYLES {
        assert!(
            message.contains(style),
            "the message names `{style}`: {message}"
        );
    }
}

/// The authored spelling is the kebab-case string. A RON identifier is not a
/// style, so the typed spelling fails where the string spelling passes.
#[test]
fn a_numbering_written_as_a_ron_ident_fails() {
    let outcome = load_one(
        "(format_version: 1, numbering: Arabian)",
        "play",
        &spec_tree(spec("play"), false, &[]),
    );
    assert_eq!(
        reported(&outcome),
        [(
            "ui/plugin.ron".to_string(),
            "numbering".to_string(),
            "ui.parse".to_string()
        )]
    );
}

// --- slot content ----------------------------------------------------------

/// The two ornament slots carry the plugin's own nodes, so a marker holding one
/// loads.
#[test]
fn an_ornament_slot_may_hold_content() {
    let mut children = spec_children(spec("play"), false);
    children.retain(|child| child != &slot("advance-marker"));
    children.push(
        "(kind: \"slot\", slot: \"advance-marker\", children: [(kind: \"box\")])".to_string(),
    );
    let outcome = load_one(MANIFEST, "play", &root(&children));
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
}

/// Every other slot holds engine content, so content inside one is a slot the
/// surface cannot take it in, reported where the content sits.
#[test]
fn content_in_an_engine_slot_fails() {
    let mut children = spec_children(spec("play"), false);
    children.retain(|child| child != &slot("prose"));
    let index = children.len();
    children.push("(kind: \"slot\", slot: \"prose\", children: [(kind: \"box\")])".to_string());
    let outcome = load_one(MANIFEST, "play", &root(&children));
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            format!("root.children[{index}].children"),
            "ui.slot.unknown".to_string()
        )],
        "{}",
        messages(&outcome)
    );
    assert!(
        messages(&outcome).contains("advance-marker"),
        "the message names the slots that do take content: {}",
        messages(&outcome)
    );
}
