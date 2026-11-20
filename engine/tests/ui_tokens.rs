//! Slice-1 rule 5 of the UI plugin spec (§15): `ui/tokens.ron` parses, a
//! `{group.name}` reference resolves, and an unknown token or a reference cycle
//! fails with the rule the spec names.
//!
//! A token file is loaded through [`ui::load_from`] with a memory source, so the
//! test exercises the same pass the check, the boot and the export run. A tree
//! is only present when a case needs a reference from one.

use unmei_engine::ui::{self, Fill, PluginOutcome};
use unmei_engine::Location;

const MANIFEST: &str = "(format_version: 1, name: \"Test\")";

const TOKENS: &str = r#"(
    colors: {
        "ink": (r: 0.06, g: 0.06, b: 0.08, a: 1.0),
        "focus": (r: 0.9, g: 0.8, b: 0.5, a: 1.0),
    },
    typography: {
        "body": (face: "IBM Plex Sans", weight: 400, size_px: 25.0, line_height_px: 38.0),
    },
    rounded: { "box": 14.0 },
    spacing: { "gap": 12.0 },
    components: {
        "panel": (
            fill: "{colors.ink}",
            border: (width: 1.0, color: "{colors.ink}"),
            radius: "{rounded.box}",
        ),
    },
)"#;

/// The interface a `play` tree must place: ticket #107's table as root children.
/// A case about a token appends it, so the tree is also one the interface pass
/// accepts.
const PLAY_INTERFACE: &str = r#"
        (kind: "slot", slot: "prose"),
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
        (kind: "slot", slot: "play.menu")"#;

/// Load a plugin whose only files are the manifest, `tokens` and `surfaces`.
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

/// `(file, path, rule)` for each finding, which is the shape the spec's rule 12
/// fixes.
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

#[test]
fn a_token_reference_that_names_no_token_fails() {
    let outcome = load(
        TOKENS,
        &[("play", "(kind: \"box\", gap: \"{spacing.nope}\")")],
    );
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            "root.gap".to_string(),
            "ui.token.unknown".to_string(),
        )]
    );
    assert!(
        messages(&outcome).contains("spacing.nope"),
        "the message names the reference: {}",
        messages(&outcome)
    );
}

#[test]
fn a_spacing_chain_that_returns_to_its_start_is_a_cycle() {
    let outcome = load(
        r#"(spacing: { "a": "{spacing.b}", "b": "{spacing.a}" })"#,
        &[],
    );
    assert_eq!(
        reported(&outcome),
        [(
            "ui/tokens.ron".to_string(),
            "spacing.b".to_string(),
            "ui.token.cycle".to_string(),
        )],
        "the finding sits at the property that closes the loop"
    );
}

#[test]
fn a_component_chain_that_returns_to_its_start_is_a_cycle() {
    let outcome = load(
        r#"(components: {
            "a": (component: "b", opacity: 1.0),
            "b": (component: "a", opacity: 0.5),
        })"#,
        &[],
    );
    assert_eq!(
        reported(&outcome),
        [(
            "ui/tokens.ron".to_string(),
            "components.b.component".to_string(),
            "ui.token.cycle".to_string(),
        )]
    );
}

#[test]
fn a_spacing_chain_resolves_and_a_node_overrides_its_component() {
    let tokens = r#"(
        colors: { "ink": (r: 0.1, g: 0.1, b: 0.1, a: 1.0),
                  "focus": (r: 0.9, g: 0.8, b: 0.5, a: 1.0) },
        spacing: { "a": "{spacing.b}", "b": 72.0 },
        rounded: { "box": "{rounded.panel}" , "panel": 9.0 },
        components: {
            "base": (fill: "{colors.ink}", opacity: 0.5),
            "child": (component: "base"),
        },
    )"#;
    let tree = format!(
        r#"(
        kind: "box",
        component: "child",
        fill: "{{colors.focus}}",
        gap: "{{spacing.a}}",
        radius: "{{rounded.box}}",
        children: [{PLAY_INTERFACE}],
    )"#
    );
    let outcome = load(tokens, &[("play", &tree)]);
    let plugin = plugin(&outcome);
    assert_eq!(plugin.tokens.spacing.get("a").copied(), Some(72.0));
    assert_eq!(plugin.tokens.rounded.get("box").copied(), Some(9.0));

    let root = &plugin.surfaces["play"];
    close(root.style.paint.opacity, 0.5);
    match root.style.paint.fill.as_ref().expect("a fill") {
        Fill::Color(color) => {
            close(color.r, 0.9);
            close(color.g, 0.8);
            close(color.b, 0.5);
        }
        other => panic!("the node's own fill wins over its component's: {other:?}"),
    }
    match root.kind {
        ui::NodeKind::Box(ref node) => close(node.gap, 72.0),
        ref other => panic!("the root is a box: {other:?}"),
    }
    assert_eq!(
        root.style.paint.radius,
        Some(ui::Radius::Uniform(9.0)),
        "a rounded token may name another rounded token"
    );
}

#[test]
fn a_radius_may_name_a_spacing_token() {
    let tree =
        format!("(kind: \"box\", radius: \"{{spacing.inset}}\", children: [{PLAY_INTERFACE}])");
    let outcome = load(r#"(spacing: { "inset": 3.0 })"#, &[("play", &tree)]);
    let root = &plugin(&outcome).surfaces["play"];
    assert_eq!(root.style.paint.radius, Some(ui::Radius::Uniform(3.0)));
}

#[test]
fn a_component_value_reference_is_an_unknown_token() {
    let outcome = load(
        r#"(components: {
            "a": (fill: "{components.b}"),
            "b": (opacity: 1.0),
        })"#,
        &[],
    );
    assert_eq!(
        reported(&outcome),
        [(
            "ui/tokens.ron".to_string(),
            "components.a.fill".to_string(),
            "ui.token.unknown".to_string(),
        )],
        "a component is named by `component`, never by a value reference"
    );
}

#[test]
fn an_unknown_component_on_a_node_fails() {
    let outcome = load(TOKENS, &[("play", "(kind: \"box\", component: \"nope\")")]);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/surfaces/play.ron".to_string(),
            "root.component".to_string(),
            "ui.token.unknown".to_string(),
        )]
    );
}

#[test]
fn an_unknown_property_in_the_token_file_names_its_field() {
    let unknown_group = load(r#"(colours: { "ink": 1.0 })"#, &[]);
    assert_eq!(
        reported(&unknown_group),
        [(
            "ui/tokens.ron".to_string(),
            "colours".to_string(),
            "ui.parse".to_string(),
        )]
    );

    let unknown_field = load(
        r#"(colors: { "ink": (r: 0.0, g: 0.0, b: 0.0, a: 1.0, alpha: 0.5) })"#,
        &[],
    );
    assert_eq!(
        reported(&unknown_field),
        [(
            "ui/tokens.ron".to_string(),
            "colors.ink.alpha".to_string(),
            "ui.parse".to_string(),
        )]
    );
}

#[test]
fn a_token_finding_is_a_field_location() {
    let outcome = load(
        r#"(spacing: { "a": "{spacing.b}", "b": "{spacing.a}" })"#,
        &[],
    );
    for finding in outcome.findings() {
        assert!(matches!(finding.path, Location::Field(_)), "{finding}");
    }
}
