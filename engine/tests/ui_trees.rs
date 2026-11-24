//! Slice-1 rules of the UI plugin spec (§4, §15): a surface tree parses into
//! the six node kinds with their paint, components and control states, findings
//! name the node's path, and the node-count and depth caps hold.
//!
//! Every case loads through [`ui::load_from`] with a memory source, so the test
//! runs the same pass the check, the boot and the export run. The fixtures are
//! synthetic: no case depends on the demo pack or on a shipped plugin.

use unmei_engine::ui::{
    self, AlignItems, Color, Dimension, Direction, Easing, Fill, Fit, JustifyContent, NodeKind,
    Padding, PluginOutcome, Position, Radius,
};

const MANIFEST: &str = "(format_version: 1, name: \"Test\")";

const TOKENS: &str = r#"(
    colors: {
        "ink": (r: 0.06, g: 0.06, b: 0.08, a: 1.0),
        "focus": (r: 0.9, g: 0.8, b: 0.5, a: 1.0),
    },
    typography: {
        "body": (face: "IBM Plex Sans", weight: 400, size_px: 25.0, line_height_px: 38.0),
        "flourish": (face: "IBM Plex Sans", weight: 600, size_px: 40.0, line_height_px: 40.0),
    },
    rounded: { "box": 14.0 },
    spacing: { "gap": 12.0 },
    components: {
        "panel": (
            fill: "{colors.ink}",
            border: (width: 1.0, color: "{colors.ink}"),
            radius: "{rounded.box}",
            shadow: [ (x: 0.0, y: 8.0, blur: 24.0, spread: 0.0,
                       color: (r: 0.0, g: 0.0, b: 0.0, a: 0.45)) ],
            opacity: 1.0,
        ),
    },
)"#;

fn load(tokens: &str, surfaces: &[(&str, &str)]) -> PluginOutcome {
    let mut files = vec![
        ("plugin.ron".to_string(), MANIFEST.as_bytes().to_vec()),
        ("tokens.ron".to_string(), tokens.as_bytes().to_vec()),
    ];
    for (id, text) in surfaces {
        files.push((format!("surfaces/{id}.ron"), text.as_bytes().to_vec()));
    }
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

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-5,
        "{actual} is not {expected}"
    );
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

fn color_of(fill: Option<&Fill>) -> Color {
    match fill {
        Some(Fill::Color(color)) => *color,
        other => panic!("expected a colour fill: {other:?}"),
    }
}

/// `depth` nodes on the longest chain, each a box nested in the one before. The
/// `play` interface stands beside the chain, at depth 2, so the tree is also one
/// the interface pass accepts.
fn nested(depth: usize) -> String {
    let mut tree = "(kind: \"box\")".to_string();
    for _ in 2..depth {
        tree = format!("(kind: \"box\", children: [{tree}])");
    }
    format!("(kind: \"box\", children: [{tree},{PLAY_INTERFACE}])")
}

/// A root box with [`PLAY_INTERFACE`] and enough empty-box children to hold
/// `nodes` nodes in all.
fn wide(nodes: usize) -> String {
    let filler = nodes - 1 - PLAY_INTERFACE_NODES;
    let children = std::iter::repeat_n("(kind: \"box\")", filler)
        .collect::<Vec<_>>()
        .join(",");
    format!("(kind: \"box\", children: [{children},{PLAY_INTERFACE}])")
}

/// The interface a `play` tree must place: ticket #107's table as root children.
/// A fixture about one node or one property appends it, so the tree is also one
/// the interface pass accepts.
const PLAY_INTERFACE: &str = r#"
        (kind: "slot", slot: "prose", typography: "{typography.body}", color: "{colors.ink}"),
        (kind: "slot", slot: "advance-marker"),
        (kind: "slot", slot: "deciding-marker"),
        (kind: "list", list: "choices", item: (kind: "box", children: [
            (kind: "slot", slot: "choice.number", typography: "{typography.body}", color: "{colors.ink}"),
            (kind: "slot", slot: "choice.text", typography: "{typography.body}", color: "{colors.ink}"),
        ])),
        (kind: "slot", slot: "quick-row"),
        (kind: "slot", slot: "play.save", typography: "{typography.body}", color: "{colors.ink}"),
        (kind: "slot", slot: "play.load", typography: "{typography.body}", color: "{colors.ink}"),
        (kind: "slot", slot: "play.auto", typography: "{typography.body}", color: "{colors.ink}"),
        (kind: "slot", slot: "play.skip", typography: "{typography.body}", color: "{colors.ink}"),
        (kind: "slot", slot: "play.menu", typography: "{typography.body}", color: "{colors.ink}")"#;

/// The nodes [`PLAY_INTERFACE`] adds, its ten root children included.
const PLAY_INTERFACE_NODES: usize = 13;

const ALL_KINDS: &str = r#"(
    kind: "box",
    component: "panel",
    direction: "column",
    gap: "{spacing.gap}",
    padding: (top: 1.0, right: 2.0, bottom: 3.0, left: 4.0),
    align_items: "center",
    justify_content: "space-between",
    width: 560.0,
    height: "fill",
    position: "absolute",
    left: 10.0, top: 20.0, right: 30.0, bottom: 40.0,
    z_index: 2,
    opacity: 0.9,
    transform: (x: 1.0, y: 2.0, scale: 1.5, rotation_deg: 5.0),
    loop: (period_ms: 2400, easing: "ease-in-out",
           keyframes: [ (opacity: 1.0), (opacity: 0.55) ]),
    children: [
        (kind: "image", file: "ornament/pin.png", fit: "cover",
         fill: (radial: (stops: [ (at: 0.0, color: "{colors.ink}") ]))),
        (kind: "glyph", glyph: "❧", typography: "{typography.flourish}",
         fill: "{colors.ink}"),
        (kind: "text", text: "play.deciding", typography: "{typography.body}",
         fill: "{colors.ink}"),
        (kind: "slot", slot: "play.save", typography: "{typography.body}", color: "{colors.ink}",
         focused: (fill: "{colors.focus}"),
         disabled: (opacity: 0.5),
         lit: (border: (width: 1.0, color: "{colors.ink}")),
         current: (radius: "{rounded.box}")),
        (kind: "list", list: "choices", focused: (opacity: 0.8),
         item: (kind: "box", direction: "row", children: [
             (kind: "slot", slot: "choice.number", typography: "{typography.body}", color: "{colors.ink}"),
             (kind: "slot", slot: "choice.text", typography: "{typography.body}", color: "{colors.ink}"),
         ])),
        (kind: "slot", slot: "prose", typography: "{typography.body}", color: "{colors.ink}"),
        (kind: "slot", slot: "advance-marker"),
        (kind: "slot", slot: "deciding-marker"),
        (kind: "slot", slot: "quick-row"),
        (kind: "slot", slot: "play.load", typography: "{typography.body}", color: "{colors.ink}"),
        (kind: "slot", slot: "play.auto", typography: "{typography.body}", color: "{colors.ink}"),
        (kind: "slot", slot: "play.skip", typography: "{typography.body}", color: "{colors.ink}"),
        (kind: "slot", slot: "play.menu", typography: "{typography.body}", color: "{colors.ink}"),
    ],
)"#;

#[test]
fn the_six_node_kinds_round_trip_through_the_tree() {
    let outcome = load(TOKENS, &[("play", ALL_KINDS)]);
    let plugin = plugin(&outcome);
    let root = &plugin.surfaces["play"];

    // The root's own properties, and the component it named underneath them.
    assert_eq!(root.style.width, Dimension::Px(560.0));
    assert_eq!(root.style.height, Dimension::Fill);
    assert_eq!(root.style.position, Position::Absolute);
    assert_eq!(root.style.offsets.left, Some(10.0));
    assert_eq!(root.style.offsets.top, Some(20.0));
    assert_eq!(root.style.offsets.right, Some(30.0));
    assert_eq!(root.style.offsets.bottom, Some(40.0));
    assert_eq!(root.style.z_index, 2);
    close(root.style.paint.opacity, 0.9);
    close(root.style.paint.transform.x, 1.0);
    close(root.style.paint.transform.scale, 1.5);
    close(root.style.paint.transform.rotation_deg, 5.0);
    assert_eq!(color_of(root.style.paint.fill.as_ref()), ink());
    let border = root.style.paint.border.as_ref().expect("the panel border");
    close(border.width, 1.0);
    assert_eq!(border.color, ink());
    assert_eq!(root.style.paint.radius, Some(Radius::Uniform(14.0)));
    assert_eq!(root.style.paint.shadows.len(), 1);
    close(root.style.paint.shadows[0].blur, 24.0);
    let motion = root.style.motion.as_ref().expect("the root loop");
    assert_eq!(motion.period_ms, 2400);
    assert_eq!(motion.easing, Easing::EaseInOut);
    assert_eq!(motion.keyframes.len(), 2);
    close(motion.keyframes[1].opacity, 0.55);

    let NodeKind::Box(ref root_box) = root.kind else {
        panic!("the root is a box: {:?}", root.kind);
    };
    assert_eq!(root_box.direction, Direction::Column);
    close(root_box.gap, 12.0);
    assert_eq!(
        root_box.padding,
        Padding::Sides {
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
            left: 4.0,
        }
    );
    assert_eq!(root_box.align_items, AlignItems::Center);
    assert_eq!(root_box.justify_content, JustifyContent::SpaceBetween);
    assert_eq!(root_box.children.len(), 13);

    let NodeKind::Image(ref image) = root_box.children[0].kind else {
        panic!("an image: {:?}", root_box.children[0].kind);
    };
    assert_eq!(image.file, "ornament/pin.png");
    assert_eq!(image.fit, Fit::Cover);
    match root_box.children[0].style.paint.fill.as_ref() {
        Some(Fill::Radial { stops }) => {
            assert_eq!(stops.len(), 1);
            assert_eq!(stops[0].color, ink());
        }
        other => panic!("a radial gradient: {other:?}"),
    }

    let NodeKind::Glyph(ref glyph) = root_box.children[1].kind else {
        panic!("a glyph: {:?}", root_box.children[1].kind);
    };
    assert_eq!(glyph.glyph, "❧");
    let face = root_box.children[1]
        .style
        .text
        .typography
        .as_ref()
        .expect("the glyph's face");
    assert_eq!(face.face, "IBM Plex Sans");
    assert_eq!(face.weight, 600);
    assert_eq!(
        color_of(root_box.children[1].style.paint.fill.as_ref()),
        ink()
    );

    let NodeKind::Text(ref text) = root_box.children[2].kind else {
        panic!("text: {:?}", root_box.children[2].kind);
    };
    assert_eq!(text.text, "play.deciding");
    close(
        root_box.children[2]
            .style
            .text
            .typography
            .as_ref()
            .expect("the text's face")
            .size_px,
        25.0,
    );

    let NodeKind::Slot(ref slot) = root_box.children[3].kind else {
        panic!("a slot: {:?}", root_box.children[3].kind);
    };
    assert_eq!(slot.slot, "play.save");
    let states = &root_box.children[3].states;
    let focused = states.focused.as_ref().expect("the focused state");
    assert_eq!(color_of(focused.fill.as_ref()), focus());
    let disabled = states.disabled.as_ref().expect("the disabled state");
    close(disabled.opacity.expect("a disabled opacity"), 0.5);
    let lit = states.lit.as_ref().expect("the lit state");
    assert_eq!(lit.border.as_ref().expect("a lit border").color, ink());
    let current = states.current.as_ref().expect("the current state");
    assert_eq!(current.radius, Some(Radius::Uniform(14.0)));

    let NodeKind::List(ref list) = root_box.children[4].kind else {
        panic!("a list: {:?}", root_box.children[4].kind);
    };
    assert_eq!(list.list, "choices");
    close(
        root_box.children[4]
            .states
            .focused
            .as_ref()
            .expect("the list's focused state")
            .opacity
            .expect("an opacity"),
        0.8,
    );
    let NodeKind::Box(ref item) = list.item.kind else {
        panic!("the item template is a box: {:?}", list.item.kind);
    };
    assert_eq!(item.direction, Direction::Row);
    let NodeKind::Slot(ref label) = item.children[0].kind else {
        panic!("the item's slot: {:?}", item.children[0].kind);
    };
    assert_eq!(label.slot, "choice.number");
}

#[test]
fn a_finding_inside_a_tree_names_the_node_path() {
    let tree = r#"(
        kind: "box",
        children: [
            (kind: "box"),
            (kind: "box", children: [
                (kind: "text", text: "a.b", typography: "{typography.body}",
                 colour: "{colors.ink}"),
            ]),
        ],
    )"#;
    let outcome = load(TOKENS, &[("play", tree)]);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            "root.children[1].children[0].colour".to_string(),
            "ui.parse".to_string(),
        )]
    );
}

#[test]
fn a_list_item_path_stays_under_item() {
    let tree = r#"(kind: "box", children: [
        (kind: "list", list: "title.rows", item: (kind: "box", bad: 1.0)),
    ])"#;
    let outcome = load(TOKENS, &[("play", tree)]);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            "root.children[0].item.bad".to_string(),
            "ui.parse".to_string(),
        )]
    );
}

fn glyph(text: &str) -> String {
    format!(
        "(kind: \"box\", children: [ (kind: \"glyph\", glyph: \"{text}\", \
         typography: \"{{typography.flourish}}\") ])"
    )
}

#[test]
fn a_glyph_of_eight_characters_passes() {
    let tree = format!(
        "(kind: \"box\", children: [ (kind: \"glyph\", glyph: \"❧❧❧❧❧❧❧❧\", \
         typography: \"{{typography.flourish}}\", fill: \"{{colors.ink}}\"),{PLAY_INTERFACE}])"
    );
    let outcome = load(TOKENS, &[("play", &tree)]);
    assert_eq!(reported(&outcome), []);
    let root = &plugin(&outcome).surfaces["play"];
    let NodeKind::Box(ref root_box) = root.kind else {
        panic!("the root is a box");
    };
    let NodeKind::Glyph(ref node) = root_box.children[0].kind else {
        panic!("a glyph");
    };
    assert_eq!(node.glyph.chars().count(), 8);
}

#[test]
fn a_glyph_of_nine_characters_fails() {
    let outcome = load(TOKENS, &[("play", &glyph("❧❧❧❧❧❧❧❧❧"))]);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            "root.children[0].glyph".to_string(),
            "ui.parse".to_string(),
        )]
    );
    let message = messages(&outcome);
    assert!(
        message.contains('9') && message.contains('8'),
        "the message names the length found and the limit: {message}"
    );
}

#[test]
fn a_property_the_node_kind_does_not_take_fails() {
    let tree = r#"(kind: "box", children: [
        (kind: "text", text: "play.deciding", typography: "{typography.body}",
         direction: "row"),
    ])"#;
    let outcome = load(TOKENS, &[("play", tree)]);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            "root.children[0].direction".to_string(),
            "ui.parse".to_string(),
        )]
    );
    assert!(
        messages(&outcome).contains("text"),
        "the message names the kind: {}",
        messages(&outcome)
    );
}

/// A text slot takes the face the engine's words are drawn in and their tone,
/// because the words are the engine's and the look is the plugin's (spec §4,
/// "Engine text", rule 34). Its own pair beats the enclosing `Box`'s.
#[test]
fn a_text_slot_takes_a_typography_token_and_a_color() {
    let tree = r#"(kind: "box", typography: "{typography.flourish}", color: "{colors.ink}", children: [
        (kind: "slot", slot: "prose", typography: "{typography.body}", color: "{colors.focus}"),
        (kind: "slot", slot: "advance-marker"),
        (kind: "slot", slot: "deciding-marker"),
        (kind: "list", list: "choices", item: (kind: "box", children: [
            (kind: "slot", slot: "choice.number"),
            (kind: "slot", slot: "choice.text"),
        ])),
        (kind: "slot", slot: "quick-row"),
        (kind: "slot", slot: "play.save"),
        (kind: "slot", slot: "play.load"),
        (kind: "slot", slot: "play.auto"),
        (kind: "slot", slot: "play.skip"),
        (kind: "slot", slot: "play.menu"),
    ])"#;
    let outcome = load(TOKENS, &[("play", tree)]);
    assert_eq!(reported(&outcome), []);
    let plugin = plugin(&outcome);
    let NodeKind::Box(ref root) = plugin.surfaces["play"].kind else {
        panic!("the root is a box");
    };
    let prose = root.children[0]
        .style
        .text
        .typography
        .as_ref()
        .expect("the slot's face");
    close(prose.size_px, 25.0);
    close(prose.line_height_px, 38.0);
    assert_eq!(
        root.children[0].style.text.color,
        Some(focus()),
        "the slot's own tone"
    );
    let inherited = root.children[8]
        .style
        .text
        .typography
        .as_ref()
        .expect("`play.load` inherits the box's face");
    close(inherited.size_px, 40.0);
    assert_eq!(
        root.children[8].style.text.color,
        Some(ink()),
        "and the box's tone"
    );
}

/// An unknown token in either property is the same `ui.token.unknown` it is
/// anywhere else (spec §15, rule 34).
#[test]
fn an_unknown_token_on_a_text_slot_fails() {
    for property in ["typography", "color"] {
        let value = if property == "typography" {
            "{typography.unknown}"
        } else {
            "{colors.unknown}"
        };
        let tree = format!(
            "(kind: \"box\", children: [\n\
             (kind: \"slot\", slot: \"prose\", {property}: \"{value}\"),\n\
             {PLAY_INTERFACE}\n\
             ])",
        );
        let outcome = load(TOKENS, &[("play", &tree)]);
        assert_eq!(
            reported(&outcome),
            [(
                "ui/surfaces/play.ron".to_string(),
                format!("root.children[0].{property}"),
                "ui.token.unknown".to_string(),
            )],
            "{property}"
        );
    }
}

/// A state override may change a control label's tone, so a `lit` quick-row
/// control can look active (spec §4, "Engine text").
#[test]
fn a_state_override_changes_a_label_tone() {
    let interface = PLAY_INTERFACE.replace(
        "(kind: \"slot\", slot: \"play.auto\", typography: \"{typography.body}\", color: \"{colors.ink}\"),",
        "(kind: \"slot\", slot: \"play.auto\", typography: \"{typography.body}\",\n         \
         color: \"{colors.ink}\",\n         \
         lit: (color: \"{colors.focus}\")),",
    );
    let tree = format!("(kind: \"box\", children: [{interface}])");
    let outcome = load(TOKENS, &[("play", &tree)]);
    assert_eq!(reported(&outcome), []);
    let plugin = plugin(&outcome);
    let NodeKind::Box(ref root) = plugin.surfaces["play"].kind else {
        panic!("the root is a box");
    };
    let NodeKind::Slot(ref auto) = root.children[7].kind else {
        panic!("the eighth child is `play.auto`");
    };
    assert_eq!(auto.slot, "play.auto");
    let lit = root.children[7]
        .states
        .lit
        .as_ref()
        .expect("the lit override");
    assert_eq!(
        lit.color,
        Some(focus()),
        "the lit label takes the focus tone"
    );
    assert_eq!(
        root.children[7].style.text.color,
        Some(ink()),
        "and the label keeps its own"
    );
}

#[test]
fn an_image_path_outside_the_folder_escapes() {
    let outcome = load(
        TOKENS,
        &[(
            "play",
            "(kind: \"box\", children: [ (kind: \"image\", file: \"../art.png\") ])",
        )],
    );
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            "root.children[0].file".to_string(),
            "ui.path.escape".to_string(),
        )]
    );
}

#[test]
fn a_tree_for_an_unknown_surface_fails() {
    let outcome = load("()", &[("hero", "(kind: \"box\")")]);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/hero.ron".to_string(),
            String::new(),
            "ui.surface.unknown".to_string(),
        )]
    );
    assert!(
        messages(&outcome).contains("hero"),
        "the message names the surface: {}",
        messages(&outcome)
    );
}

#[test]
fn a_tree_whose_root_is_not_a_box_fails() {
    let outcome = load(
        TOKENS,
        &[(
            "play",
            "(kind: \"text\", text: \"play.deciding\", typography: \"{typography.body}\")",
        )],
    );
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            "root.kind".to_string(),
            "ui.parse".to_string(),
        )]
    );
}

#[test]
fn a_surface_with_no_tree_file_is_not_a_failure() {
    let tree = format!("(kind: \"box\", children: [{PLAY_INTERFACE}])");
    let outcome = load(TOKENS, &[("play", &tree)]);
    let plugin = plugin(&outcome);
    assert_eq!(reported(&outcome), []);
    assert_eq!(plugin.surfaces.len(), 1);
    assert!(plugin.surfaces.contains_key("play"));
    assert!(!plugin.surfaces.contains_key("title"));
}

#[test]
fn the_node_count_cap_passes_at_the_limit_and_fails_one_over() {
    let at_the_cap = load(TOKENS, &[("play", &wide(ui::MAX_NODES))]);
    assert_eq!(reported(&at_the_cap), [], "the cap is inclusive");

    let over = load(TOKENS, &[("play", &wide(ui::MAX_NODES + 1))]);
    assert_eq!(
        reported(&over),
        [(
            "ui/surfaces/play.ron".to_string(),
            String::new(),
            "ui.cap".to_string(),
        )]
    );
    let message = messages(&over);
    assert!(
        message.contains(&ui::MAX_NODES.to_string())
            && message.contains(&(ui::MAX_NODES + 1).to_string()),
        "the message names the cap and the count found: {message}"
    );
}

#[test]
fn the_depth_cap_passes_at_the_limit_and_fails_one_over() {
    let at_the_cap = load(TOKENS, &[("play", &nested(ui::MAX_DEPTH))]);
    assert_eq!(reported(&at_the_cap), [], "the cap is inclusive");

    let over = load(TOKENS, &[("play", &nested(ui::MAX_DEPTH + 1))]);
    assert_eq!(
        reported(&over),
        [(
            "ui/surfaces/play.ron".to_string(),
            String::new(),
            "ui.cap".to_string(),
        )],
        "{}",
        messages(&over)
    );
    let message = messages(&over);
    assert!(
        message.contains(&ui::MAX_DEPTH.to_string())
            && message.contains(&(ui::MAX_DEPTH + 1).to_string()),
        "the message names the cap and the depth found: {message}"
    );
}
