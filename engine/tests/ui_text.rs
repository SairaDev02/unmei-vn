//! The text style a node inherits and the one every text show must resolve
//! (UI plugin spec §4, "Engine text"; §15 rules 35, 36 and 37's parsing).
//!
//! Every case loads through [`ui::load_from`] with a memory source, so it runs
//! the pass the check, the boot and the export run. The fixtures use `still`,
//! whose interface is one list and two item slots, so a tree is small enough
//! that the text style is the only thing under test.

use unmei_engine::ui::{self, Color, Node, NodeKind, PluginOutcome};

const MANIFEST: &str = "(format_version: 1, name: \"Test\")";

const TOKENS: &str = r#"(
    colors: {
        "ink": (r: 0.06, g: 0.06, b: 0.08, a: 1.0),
        "focus": (r: 0.9, g: 0.8, b: 0.5, a: 1.0),
    },
    typography: {
        "body": (face: "IBM Plex Sans", weight: 400, size_px: 25.0, line_height_px: 38.0),
        "mono": (face: "Fira Mono", weight: 500, size_px: 12.0, line_height_px: 16.0),
    },
)"#;

/// A `still` tree whose item places `label` and the required art slot, inside
/// the root box whose own properties `root` writes.
fn still(root: &str, label: &str) -> String {
    format!(
        "(kind: \"box\", {root} children: [
            (kind: \"list\", list: \"still.entry\", item: (kind: \"box\", children: [
                {label},
                (kind: \"slot\", slot: \"still.art\"),
            ])),
        ])"
    )
}

fn load(tree: &str) -> PluginOutcome {
    let files = vec![
        ("plugin.ron".to_string(), MANIFEST.as_bytes().to_vec()),
        ("tokens.ron".to_string(), TOKENS.as_bytes().to_vec()),
        ("surfaces/still.ron".to_string(), tree.as_bytes().to_vec()),
    ];
    ui::load_from(&ui::MemorySource::new(files))
}

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

/// The item template's first child, which every fixture here makes a label.
fn label_node(outcome: &PluginOutcome) -> &Node {
    match &plugin(outcome).surfaces["still"].kind {
        NodeKind::Box(root) => match &root.children[0].kind {
            NodeKind::List(list) => match &list.item.kind {
                NodeKind::Box(item) => &item.children[0],
                other => panic!("the item is a box: {other:?}"),
            },
            other => panic!("the root's child is a list: {other:?}"),
        },
        other => panic!("the root is a box: {other:?}"),
    }
}

fn ink() -> Color {
    Color {
        r: 0.06,
        g: 0.06,
        b: 0.08,
        a: 1.0,
    }
}

fn focus() -> Color {
    Color {
        r: 0.9,
        g: 0.8,
        b: 0.5,
        a: 1.0,
    }
}

fn unstyled(path: &str) -> (String, String, String) {
    (
        "ui/surfaces/still.ron".to_string(),
        path.to_string(),
        "ui.text.unstyled".to_string(),
    )
}

#[test]
fn a_box_styles_a_text_slot_inside_it() {
    let label = r#"(kind: "slot", slot: "still.label")"#;
    let outcome = load(&still(
        r#"typography: "{typography.body}", color: "{colors.ink}","#,
        label,
    ));
    let node = label_node(&outcome);
    assert_eq!(
        node.style.text.typography.as_ref().map(|face| face.size_px),
        Some(25.0)
    );
    assert_eq!(node.style.text.color, Some(ink()));
}

#[test]
fn the_nearest_enclosing_box_wins() {
    let item = r#"(kind: "box", typography: "{typography.mono}", children: [
        (kind: "slot", slot: "still.label"),
        (kind: "slot", slot: "still.art"),
    ])"#;
    let tree = format!(
        "(kind: \"box\", typography: \"{{typography.body}}\", color: \"{{colors.ink}}\", children: [
            (kind: \"list\", list: \"still.entry\", item: {item}),
        ])"
    );
    let outcome = load(&tree);
    let node = item_of(&outcome).children[0].clone();
    assert_eq!(
        node.style.text.typography.as_ref().map(|face| face.size_px),
        Some(12.0),
        "the item's own `mono` beats the outer box's `body`"
    );
    assert_eq!(node.style.text.color, Some(ink()), "the tone flows through");
}

#[test]
fn a_text_slot_with_no_face_fails() {
    let outcome = load(&still(
        r#"color: "{colors.ink}","#,
        r#"(kind: "slot", slot: "still.label")"#,
    ));
    assert_eq!(
        reported(&outcome),
        [unstyled("root.children[0].item.children[0]")]
    );
    assert!(
        messages(&outcome).contains("typography"),
        "the message names what is missing: {}",
        messages(&outcome)
    );
}

#[test]
fn a_text_slot_with_no_tone_fails() {
    let outcome = load(&still(
        r#"typography: "{typography.body}","#,
        r#"(kind: "slot", slot: "still.label")"#,
    ));
    assert_eq!(
        reported(&outcome),
        [unstyled("root.children[0].item.children[0]")]
    );
    assert!(
        messages(&outcome).contains("color"),
        "the message names what is missing: {}",
        messages(&outcome)
    );
}

#[test]
fn a_text_node_with_no_face_fails() {
    let outcome = load(
        r#"(kind: "box", color: "{colors.ink}", children: [
        (kind: "text", text: "still.legend"),
        (kind: "list", list: "still.entry", item: (kind: "box", children: [
            (kind: "slot", slot: "still.label", typography: "{typography.body}", color: "{colors.ink}"),
            (kind: "slot", slot: "still.art"),
        ])),
    ])"#,
    );
    assert_eq!(reported(&outcome), [unstyled("root.children[0]")]);
}

#[test]
fn a_glyph_with_no_tone_fails() {
    let outcome = load(
        r#"(kind: "box", children: [
        (kind: "glyph", glyph: "▶", typography: "{typography.body}"),
        (kind: "list", list: "still.entry", item: (kind: "box", children: [
            (kind: "slot", slot: "still.label", typography: "{typography.body}", color: "{colors.ink}"),
            (kind: "slot", slot: "still.art"),
        ])),
    ])"#,
    );
    assert_eq!(reported(&outcome), [unstyled("root.children[0]")]);
}

#[test]
fn a_text_node_takes_its_face_from_its_box() {
    let outcome = load(
        r#"(kind: "box", typography: "{typography.body}", children: [
        (kind: "list", list: "still.entry", item: (kind: "box", children: [
            (kind: "slot", slot: "still.label", color: "{colors.ink}"),
            (kind: "slot", slot: "still.art"),
        ])),
        (kind: "text", text: "still.legend", fill: "{colors.focus}"),
    ])"#,
    );
    let node = label_node(&outcome);
    assert_eq!(
        node.style.text.typography.as_ref().map(|face| face.size_px),
        Some(25.0)
    );
}

#[test]
fn a_label_slot_in_a_list_item_takes_a_focused_override() {
    let label = r#"(kind: "slot", slot: "still.label",
        typography: "{typography.body}", color: "{colors.ink}",
        focused: (color: "{colors.focus}"))"#;
    let outcome = load(&still("", label));
    let node = label_node(&outcome);
    let focused = node
        .states
        .focused
        .as_ref()
        .unwrap_or_else(|| panic!("the label carries a `focused` override"));
    assert_eq!(focused.color, Some(focus()));
}

#[test]
fn a_box_outside_a_list_item_takes_no_state_override() {
    let outcome = load(
        r#"(kind: "box", focused: (opacity: 0.5), children: [
        (kind: "list", list: "still.entry", item: (kind: "box", children: [
            (kind: "slot", slot: "still.label", typography: "{typography.body}", color: "{colors.ink}"),
            (kind: "slot", slot: "still.art"),
        ])),
    ])"#,
    );
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/still.ron".to_string(),
            "root.focused".to_string(),
            "ui.parse".to_string()
        )]
    );
}

fn item_of(outcome: &PluginOutcome) -> &ui::BoxNode {
    match &plugin(outcome).surfaces["still"].kind {
        NodeKind::Box(root) => match &root.children[0].kind {
            NodeKind::List(list) => match &list.item.kind {
                NodeKind::Box(item) => item,
                other => panic!("the item is a box: {other:?}"),
            },
            other => panic!("the root's child is a list: {other:?}"),
        },
        other => panic!("the root is a box: {other:?}"),
    }
}
