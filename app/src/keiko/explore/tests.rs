//! The story walk's findings and the cases it saves (spec §9.2, §9.4).

use std::path::PathBuf;

use bevy::prelude::*;
use serde_json::Value;
use unmei_engine::{load_pack, Pack};

use crate::keiko::finding::rules;
use crate::keiko::take;
use crate::menu::{MenuFocus, MenuState, MenuView};

fn demo_pack() -> (tempfile::TempDir, PathBuf, Pack) {
    let dir = crate::tests::temp_pack();
    let root = dir.path().join("pack");
    let loaded = load_pack(&root).expect("the demo pack loads");
    (dir, root, loaded.pack)
}

/// Opens the title with the focus on the overlay's Resume as soon as play
/// starts, so a story branch ends with a hidden focus.
struct HiddenFocusInPlay;

impl Plugin for HiddenFocusInPlay {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            |menu: Option<ResMut<MenuState>>, runtime: Option<Res<crate::Runtime>>| {
                let (Some(mut menu), Some(_)) = (menu, runtime) else {
                    return;
                };
                if !menu.open {
                    menu.open = true;
                    menu.view = MenuView::Title;
                    menu.focus = MenuFocus::Resume;
                }
            },
        );
    }
}

fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_story_branch_that_hides_the_focus_is_a_saved_exploration_finding() {
    let (_dir, root, pack) = demo_pack();
    let report = take::with_test_hook(
        |app| {
            app.add_plugins(HiddenFocusInPlay);
        },
        || super::run(&root, &pack, 40, None, false),
    );
    let hidden: Vec<&Value> = report
        .findings
        .iter()
        .filter(|finding| string(finding, "rule") == rules::FOCUS_HIDDEN)
        .collect();
    assert!(
        !hidden.is_empty(),
        "no hidden-focus finding: {:#?}",
        report.findings
    );
    let case_path = string(hidden[0], "case");
    assert!(
        !case_path.is_empty(),
        "the finding is saved: {:#?}",
        hidden[0]
    );
    let text = std::fs::read_to_string(root.join(&case_path)).expect("the saved case exists");
    assert!(text.contains("new-game"), "{text}");
    assert!(
        text.contains("# The take's focus check fails"),
        "the closing assertion is saved: {text}"
    );
}
