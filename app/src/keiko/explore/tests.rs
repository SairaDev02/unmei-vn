//! The story walk's findings and the cases it saves (spec §9.2, §9.4).

use std::path::PathBuf;

use bevy::prelude::*;
use serde_json::Value;
use unmei_engine::keiko::{parse_case, Header, HeaderLine, Step, StepLine};
use unmei_engine::{load_pack, Pack};

use super::save::{save_finding, Collected};
use super::{load_branch, Branch};
use crate::keiko::finding::{rules, Finding, Location};
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

#[test]
fn a_from_case_keeps_its_fault_header_and_click_step_when_saved() {
    let (_dir, root, _pack) = demo_pack();
    std::fs::write(
        root.join("tests/seed.keiko"),
        "keiko 1\n\nfault credential-unreadable\nnew-game\nclick 10 20\nclick overlay.resume\nadvance\n",
    )
    .expect("the seed case is written");
    let branch = load_branch(&root, "seed").expect("the seed loads");
    let text = branch.case_text(&["expect surface play".to_string()]);
    let case = parse_case("tests/out.keiko", &text).expect("the saved case parses");
    assert!(
        case.headers
            .iter()
            .any(|line| matches!(line.header, Header::Fault { .. })),
        "the fault header is kept: {text}"
    );
    let clicks = case
        .steps
        .iter()
        .filter(|line| matches!(line.step, Step::Click(_)))
        .count();
    assert_eq!(clicks, 2, "both clicks are kept: {text}");
}

fn branch_deciding(action: &str) -> Branch {
    Branch {
        headers: vec![HeaderLine {
            line: 1,
            header: Header::Decide {
                point: "kira.verdict".to_string(),
                action: action.to_string(),
            },
        }],
        steps: vec![StepLine {
            line: 1,
            step: Step::NewGame,
        }],
    }
}

fn panic_finding(message: &str) -> Finding {
    Finding::new(
        "tests/findings/explore.keiko",
        Location::File,
        rules::PANIC,
        message.to_string(),
    )
}

#[test]
fn findings_that_differ_only_in_decide_save_two_files() {
    let (_dir, root, pack) = demo_pack();
    let mut collected = Collected::default();
    save_finding(
        &root,
        &pack,
        &branch_deciding("accept"),
        &panic_finding("first"),
        None,
        &mut collected,
    );
    save_finding(
        &root,
        &pack,
        &branch_deciding("refuse"),
        &panic_finding("second"),
        None,
        &mut collected,
    );
    let cases: Vec<String> = collected
        .findings
        .iter()
        .map(|finding| string(finding, "case"))
        .collect();
    assert_eq!(cases.len(), 2);
    assert_ne!(cases[0], cases[1], "one file per branch: {cases:?}");
    let first = std::fs::read_to_string(root.join(&cases[0])).expect("the first case exists");
    let second = std::fs::read_to_string(root.join(&cases[1])).expect("the second case exists");
    assert!(first.contains("decide kira.verdict accept"), "{first}");
    assert!(second.contains("decide kira.verdict refuse"), "{second}");
}

#[test]
fn a_finding_whose_case_cannot_be_written_is_reported_unsaved() {
    let (_dir, root, pack) = demo_pack();
    let findings_dir = root.join("tests/findings");
    let _ = std::fs::remove_dir_all(&findings_dir);
    std::fs::write(&findings_dir, "not a directory").expect("the blocker is written");
    let mut collected = Collected::default();
    save_finding(
        &root,
        &pack,
        &branch_deciding("accept"),
        &panic_finding("unwritable"),
        None,
        &mut collected,
    );
    let finding = &collected.findings[0];
    assert_eq!(finding.get("saved"), Some(&Value::Bool(false)), "{finding}");
    assert!(finding.get("case").is_none(), "{finding}");
}
