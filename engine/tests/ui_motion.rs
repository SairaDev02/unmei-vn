//! The loop rules of the UI plugin spec §6 (rules 9 and 10 of §15): where a
//! loop may appear, the period floor beneath it and the flash cap above it.
//!
//! Every case loads through [`ui::load_from`] with a memory source, so it runs
//! the pass the check, the boot and the export run. The fixtures are synthetic
//! and name only the surface and the node a rule is about. A fixture whose loop
//! is legal also places the interface ticket #107 requires, because a tree that
//! left a required slot out would stop the plugin before its motion is read.

use unmei_engine::ui::{self, NodeKind, PluginOutcome};

const MANIFEST: &str = "(format_version: 1, name: \"Test\")";

const TOKENS: &str = r#"(
    typography: {
        "flourish": (face: "IBM Plex Sans", weight: 400, size_px: 40.0, line_height_px: 40.0),
    },
)"#;

/// The interface a `play` tree must place: ticket #107's table as root children.
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

/// The interface a `title` tree must place, in the same form.
const TITLE_INTERFACE: &str = r#"
        (kind: "slot", slot: "pack.name"),
        (kind: "slot", slot: "title.continue-reason"),
        (kind: "list", list: "title.rows", item: (kind: "box", children: [
            (kind: "slot", slot: "title.row.label"),
            (kind: "slot", slot: "title.continue"),
            (kind: "slot", slot: "title.new-game"),
            (kind: "slot", slot: "title.settings"),
            (kind: "slot", slot: "title.gallery"),
            (kind: "slot", slot: "title.quit"),
        ]))"#;

/// [`PLAY_INTERFACE`] without the two marker slots, for the one fixture that
/// places those itself.
const REST_OF_PLAY: &str = r#"(kind: "slot", slot: "prose"),
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

fn load(surfaces: &[(&str, &str)]) -> PluginOutcome {
    let mut files = vec![
        ("plugin.ron".to_string(), MANIFEST.as_bytes().to_vec()),
        ("tokens.ron".to_string(), TOKENS.as_bytes().to_vec()),
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

fn finding(file: &str, path: &str, rule: &str) -> (String, String, String) {
    (file.to_string(), path.to_string(), rule.to_string())
}

fn forbidden(path: &str) -> (String, String, String) {
    finding("ui/surfaces/play.ron", path, "ui.motion.forbidden")
}

/// One loop in the format's syntax: an opacity per keyframe, `period_ms` for
/// the whole cycle.
fn loop_ron(period_ms: u32, opacities: &[f32]) -> String {
    let frames = opacities
        .iter()
        .map(|opacity| format!("(opacity: {opacity})"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("loop: (period_ms: {period_ms}, easing: \"linear\", keyframes: [{frames}])")
}

/// A glyph ornament; glyphs and images are the nodes a loop is for.
fn glyph(extra: &str) -> String {
    format!("(kind: \"glyph\", glyph: \"❧\", typography: \"{{typography.flourish}}\", {extra})")
}

fn image(extra: &str) -> String {
    format!("(kind: \"image\", file: \"ornament/arch.png\", {extra})")
}

/// A glow that fades once and returns: half a flash a second at 2 s.
fn glow(period_ms: u32) -> String {
    loop_ron(period_ms, &[1.0, 0.4, 1.0])
}

// --- Where a loop may appear ------------------------------------------------

#[test]
fn a_loop_on_a_control_fails() {
    let tree = format!(
        "(kind: \"box\", children: [ (kind: \"slot\", slot: \"confirm.yes\", {}) ])",
        glow(2000)
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), [forbidden("root.children[0]")]);
    let message = messages(&outcome);
    assert!(
        message.contains("confirm.yes") && message.contains("control"),
        "the message names the control: {message}"
    );
}

#[test]
fn a_loop_on_a_list_item_fails() {
    let tree = format!(
        "(kind: \"box\", children: [ (kind: \"list\", list: \"title.rows\", item: \
         (kind: \"box\", {})) ])",
        glow(2000)
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), [forbidden("root.children[0].item")]);
    assert!(
        messages(&outcome).contains("list item"),
        "the message names the list item: {}",
        messages(&outcome)
    );
}

#[test]
fn a_loop_on_a_node_inside_a_list_item_fails() {
    let tree = format!(
        "(kind: \"box\", children: [ (kind: \"list\", list: \"title.rows\", item: \
         (kind: \"box\", children: [ {} ])) ])",
        glyph(&glow(2000))
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(
        reported(&outcome),
        [forbidden("root.children[0].item.children[0]")]
    );
}

#[test]
fn a_loop_on_a_contents_ornament_fails() {
    let tree = format!("(kind: \"box\", children: [ {} ])", glyph(&glow(2000)));
    let outcome = load(&[("contents", &tree)]);
    assert_eq!(
        reported(&outcome),
        [finding(
            "ui/surfaces/contents.ron",
            "root.children[0]",
            "ui.motion.forbidden"
        )]
    );
    assert!(
        messages(&outcome).contains("contents"),
        "the message names the surface: {}",
        messages(&outcome)
    );
}

#[test]
fn a_loop_on_any_menu_surface_node_fails() {
    let tree = format!("(kind: \"box\", {})", glow(2000));
    let surfaces: Vec<(&str, &str)> = [
        "title-settings",
        "contents",
        "settings",
        "gallery",
        "still",
        "timeline",
        "legacy-picker",
        "confirm",
    ]
    .into_iter()
    .map(|id| (id, tree.as_str()))
    .collect();
    let outcome = load(&surfaces);
    let mut expected: Vec<(String, String, String)> = [
        "confirm",
        "contents",
        "gallery",
        "legacy-picker",
        "settings",
        "still",
        "timeline",
        "title-settings",
    ]
    .into_iter()
    .map(|id| {
        finding(
            &format!("ui/surfaces/{id}.ron"),
            "root",
            "ui.motion.forbidden",
        )
    })
    .collect();
    expected.sort();
    let mut found = reported(&outcome);
    found.sort();
    assert_eq!(found, expected);
}

#[test]
fn a_loop_on_a_text_node_fails() {
    let tree = format!(
        "(kind: \"box\", children: [ (kind: \"text\", text: \"play.deciding\", \
         typography: \"{{typography.flourish}}\", {}) ])",
        glow(2000)
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), [forbidden("root.children[0]")]);
    assert!(
        messages(&outcome).contains("`box`, `image` and `glyph`"),
        "the message names the kinds a loop is for: {}",
        messages(&outcome)
    );
}

#[test]
fn a_loop_inside_an_engine_slot_fails() {
    let tree = format!(
        "(kind: \"box\", children: [ (kind: \"slot\", slot: \"prose\", children: [ {} ]) ])",
        glyph(&glow(2000))
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(
        reported(&outcome),
        [forbidden("root.children[0].children[0]")]
    );
    assert!(
        messages(&outcome).contains("prose"),
        "the message names the slot: {}",
        messages(&outcome)
    );
}

#[test]
fn a_loop_on_the_marker_slot_itself_fails() {
    let tree = format!(
        "(kind: \"box\", children: [ (kind: \"slot\", slot: \"advance-marker\", {}) ])",
        glow(2000)
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), [forbidden("root.children[0]")]);
    assert!(
        messages(&outcome).contains("advance-marker"),
        "the message names the slot: {}",
        messages(&outcome)
    );
}

// --- The period floor ------------------------------------------------------

#[test]
fn a_loop_with_a_300_ms_period_fails() {
    let tree = format!("(kind: \"box\", children: [ {} ])", glyph(&glow(300)));
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), [forbidden("root.children[0]")]);
    let message = messages(&outcome);
    assert!(
        message.contains("300") && message.contains("400"),
        "the message names the period found and the floor: {message}"
    );
}

#[test]
fn a_loop_with_a_400_ms_period_passes() {
    let tree = format!(
        "(kind: \"box\", children: [ {},{} ])",
        glyph(&glow(400)),
        PLAY_INTERFACE
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), [], "the floor is inclusive");
}

/// Below the floor the period is the only finding: the flash count is not
/// reported for a loop that already failed on its period (spec §6, rule 30).
#[test]
fn a_two_keyframe_loop_at_300_ms_fails_only_the_period() {
    let tree = format!(
        "(kind: \"box\", children: [ {} ])",
        glyph(&loop_ron(300, &[1.0, 0.0]))
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), [forbidden("root.children[0]")]);
}

// --- The flash cap ---------------------------------------------------------

#[test]
fn a_400_ms_loop_that_flashes_three_times_fails() {
    let tree = format!(
        "(kind: \"box\", children: [ {} ])",
        glyph(&loop_ron(400, &[1.0, 0.0, 1.0, 0.0, 1.0]))
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(
        reported(&outcome),
        [finding(
            "ui/surfaces/play.ron",
            "root.children[0]",
            "ui.motion.flash"
        )]
    );
    let message = messages(&outcome);
    assert!(
        message.contains("400") && message.contains("5.0"),
        "the message names the period and the rate: {message}"
    );
}

#[test]
fn a_glow_pulse_over_two_seconds_passes() {
    let tree = format!(
        "(kind: \"box\", children: [ {},{} ])",
        glyph(&glow(2000)),
        PLAY_INTERFACE
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), []);
    assert_eq!(
        plugin(&outcome).surfaces["play"].style.motion,
        None,
        "the loop belongs to the glyph, not the root"
    );
}

#[test]
fn three_flashes_in_a_second_pass() {
    let tree = format!(
        "(kind: \"box\", children: [ {},{} ])",
        glyph(&loop_ron(1000, &[1.0, 0.0, 1.0, 0.0, 1.0])),
        PLAY_INTERFACE
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), [], "the cap is above 3 a second");
}

#[test]
fn a_flash_of_exactly_0_2_counts() {
    let tree = format!(
        "(kind: \"box\", children: [ {} ])",
        glyph(&loop_ron(400, &[1.0, 0.8, 1.0, 0.8, 1.0]))
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(
        reported(&outcome),
        [finding(
            "ui/surfaces/play.ron",
            "root.children[0]",
            "ui.motion.flash"
        )],
        "f32 subtraction lands 1.0 - 0.8 just under 0.2, and the step is at the floor"
    );
}

/// The closing keyframe is implied when it repeats the first, so writing it or
/// leaving it out is the same loop (spec §6, rule 30). A two-keyframe loop is
/// therefore one flash per period, not two.
#[test]
fn a_two_keyframe_loop_at_400_ms_passes() {
    let tree = format!(
        "(kind: \"box\", children: [ {},{} ])",
        glyph(&loop_ron(400, &[1.0, 0.0])),
        PLAY_INTERFACE
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(
        reported(&outcome),
        [],
        "one flash in 400 ms is 2.5 a second"
    );
}

/// The same fade written with its closing keyframe counts the same one flash.
#[test]
fn the_implied_closing_keyframe_changes_nothing() {
    let implied = format!(
        "(kind: \"box\", children: [ {},{} ])",
        glyph(&loop_ron(400, &[1.0, 0.0])),
        PLAY_INTERFACE
    );
    let written = format!(
        "(kind: \"box\", children: [ {},{} ])",
        glyph(&loop_ron(400, &[1.0, 0.0, 1.0])),
        PLAY_INTERFACE
    );
    assert_eq!(reported(&load(&[("play", &implied)])), []);
    assert_eq!(reported(&load(&[("play", &written)])), []);
}

/// Steps in one direction are one change, so a fine-grained fade flashes once
/// and passes; the same fade three times in the period is three flashes and
/// fails (spec §6, rule 31).
#[test]
fn a_fade_in_small_steps_is_one_flash() {
    let mut down: Vec<f32> = (0..=10).map(|step| 1.0 - step as f32 * 0.1).collect();
    down.extend((1..=9).map(|step| step as f32 * 0.1));
    let tree = format!(
        "(kind: \"box\", children: [ {},{} ])",
        glyph(&loop_ron(400, &down)),
        PLAY_INTERFACE
    );
    assert_eq!(reported(&load(&[("play", &tree)])), []);
}

/// A change under 0.2 is dropped, so two small wobbles between two big ones do
/// not each count as a reversal: this loop flashes once, not twice.
#[test]
fn a_change_under_0_2_is_dropped() {
    let tree = format!(
        "(kind: \"box\", children: [ {},{} ])",
        glyph(&loop_ron(400, &[1.0, 0.0, 0.15, 0.0, 1.0])),
        PLAY_INTERFACE
    );
    assert_eq!(reported(&load(&[("play", &tree)])), []);
}

#[test]
fn the_same_fade_three_times_in_the_period_fails() {
    let mut thrice: Vec<f32> = Vec::new();
    for _ in 0..3 {
        thrice.extend((0..=10).map(|step| 1.0 - step as f32 * 0.1));
        thrice.extend((1..=9).map(|step| step as f32 * 0.1));
    }
    let tree = format!(
        "(kind: \"box\", children: [ {} ])",
        glyph(&loop_ron(400, &thrice))
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(
        reported(&outcome),
        [finding(
            "ui/surfaces/play.ron",
            "root.children[0]",
            "ui.motion.flash"
        )]
    );
}

/// The same two-keyframe loop over two seconds is one flash a second.
#[test]
fn a_two_keyframe_loop_at_two_seconds_passes() {
    let tree = format!(
        "(kind: \"box\", children: [ {},{} ])",
        glyph(&loop_ron(2000, &[1.0, 0.0])),
        PLAY_INTERFACE
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), []);
}

// --- Where a loop is allowed ------------------------------------------------

#[test]
fn a_loop_on_title_and_play_ornament_passes() {
    let title = format!(
        "(kind: \"box\", children: [ {}, {},{TITLE_INTERFACE} ])",
        glyph(&glow(400)),
        image(&glow(400))
    );
    let play = format!(
        "(kind: \"box\", {cycle}, children: [ {}, {},{PLAY_INTERFACE} ])",
        glyph(&glow(400)),
        image(&glow(400)),
        cycle = glow(400)
    );
    let outcome = load(&[("title", &title), ("play", &play)]);
    assert_eq!(reported(&outcome), []);
    let plugin = plugin(&outcome);
    let root = &plugin.surfaces["play"];
    assert_eq!(
        root.style.motion.as_ref().map(|motion| motion.period_ms),
        Some(400),
        "the ornament box keeps its loop"
    );
    let NodeKind::Box(ref box_node) = root.kind else {
        panic!("the root is a box");
    };
    assert!(
        box_node.children[..2].iter().all(|child| {
            child
                .style
                .motion
                .as_ref()
                .is_some_and(|motion| motion.period_ms == 400)
        }),
        "the glyph and the image keep their loops"
    );
}

#[test]
fn a_loop_on_the_content_of_a_marker_slot_passes() {
    let tree = format!(
        "(kind: \"box\", children: [
            (kind: \"slot\", slot: \"advance-marker\", children: [ {} ]),
            (kind: \"slot\", slot: \"deciding-marker\", children: [
                (kind: \"image\", file: \"ornament/dots.png\", {}),
            ]),
            {},
        ])",
        glyph(&glow(400)),
        glow(400),
        REST_OF_PLAY
    );
    let outcome = load(&[("play", &tree)]);
    assert_eq!(reported(&outcome), []);
    let root = &plugin(&outcome).surfaces["play"];
    let NodeKind::Box(ref box_node) = root.kind else {
        panic!("the root is a box");
    };
    let NodeKind::Slot(ref advance) = box_node.children[0].kind else {
        panic!(
            "the advance marker is a slot: {:?}",
            box_node.children[0].kind
        );
    };
    assert_eq!(advance.slot, "advance-marker");
    assert_eq!(advance.children.len(), 1);
    assert_eq!(
        advance.children[0]
            .style
            .motion
            .as_ref()
            .map(|motion| motion.period_ms),
        Some(400),
        "the mark keeps its loop"
    );
}
