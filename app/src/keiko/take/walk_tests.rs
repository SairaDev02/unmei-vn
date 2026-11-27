//! The menu's one walk function against the player's own input (spec §15 rule
//! 40): a test-only plugin records every frame in which exactly one walk key is
//! pressed, predicts the focus with [`crate::menu::walk`] and compares it with
//! the focus the player's systems leave.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use unmei_engine::keiko::{parse_case, Case};
use unmei_engine::{load_pack, Pack};

use super::{run_with, TakeRequest};
use crate::menu::{walk, MenuFocus, MenuState, WalkCounts};

const WALK_KEYS: [KeyCode; 8] = [
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::Tab,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
];

#[derive(Default)]
struct Ledger {
    checked: u32,
    mismatches: Vec<String>,
    pending: Option<(MenuState, WalkCounts, KeyCode)>,
}

/// Records the menu before the player's input reads a lone walk key, and
/// checks the focus once every system of the frame has run.
struct WalkOracle {
    ledger: Arc<Mutex<Ledger>>,
}

fn counts(runtime: Option<&crate::Runtime>) -> WalkCounts {
    runtime
        .map(|runtime| WalkCounts {
            gallery: runtime.runner.pack().gallery_entries().len(),
            timeline: runtime.runner.pack().timeline_entries().len(),
        })
        .unwrap_or_default()
}

impl Plugin for WalkOracle {
    fn build(&self, app: &mut App) {
        let before = self.ledger.clone();
        let after = self.ledger.clone();
        app.add_systems(
            Update,
            (move |keys: Res<ButtonInput<KeyCode>>,
                   menu: Option<Res<MenuState>>,
                   runtime: Option<Res<crate::Runtime>>| {
                let Some(menu) = menu else {
                    return;
                };
                let pressed: Vec<KeyCode> = keys.get_just_pressed().copied().collect();
                let lone = match pressed.as_slice() {
                    [key] if WALK_KEYS.contains(key) => Some(*key),
                    _ => None,
                };
                let mut ledger = before.lock().unwrap();
                ledger.pending = lone
                    .filter(|_| menu.open && !menu.gallery_still)
                    .map(|key| (menu.clone(), counts(runtime.as_deref()), key));
            })
            .before(crate::menu::menu_input),
        );
        app.add_systems(Last, move |menu: Option<Res<MenuState>>| {
            let Some(menu) = menu else {
                return;
            };
            let mut ledger = after.lock().unwrap();
            let Some((was, counts, key)) = ledger.pending.take() else {
                return;
            };
            let predicted: MenuFocus = walk(&was, counts, was.focus, key).unwrap_or(was.focus);
            ledger.checked += 1;
            if predicted != menu.focus {
                ledger.mismatches.push(format!(
                    "{:?} on {:?} from {:?}: walk says {:?}, the player left {:?}",
                    key, was.view, was.focus, predicted, menu.focus
                ));
            }
        });
    }
}

fn demo_pack() -> (tempfile::TempDir, PathBuf, Pack) {
    let dir = crate::tests::temp_pack();
    let root = dir.path().join("pack");
    let loaded = load_pack(&root).expect("the demo pack loads");
    (dir, root, loaded.pack)
}

fn request<'a>(case: &'a Case, pack: &'a Pack, root: &'a Path) -> TakeRequest<'a> {
    TakeRequest {
        case,
        case_file: "tests/walk.keiko".to_string(),
        original_pack: root,
        pack,
        take_index: 1,
        keep: false,
        live: None,
        frames: None,
    }
}

fn presses(keys: &[&str], times: usize) -> String {
    let mut out = String::new();
    for _ in 0..times {
        for key in keys {
            out.push_str(&format!("press {key}\n"));
        }
    }
    out
}

/// Every surface the demo pack reaches, each walked with every walk key.
fn walk_case() -> String {
    let all = [
        "ArrowDown",
        "ArrowRight",
        "ArrowUp",
        "ArrowLeft",
        "Tab",
        "Digit2",
    ];
    let mut case = String::from("keiko 1\nsetting play.text_speed 0\n\n");
    case.push_str(&presses(&["ArrowDown"], 6));
    case.push_str(&presses(&["ArrowUp"], 6));
    case.push_str("activate title.settings\nexpect surface settings\n");
    case.push_str(&presses(&["ArrowDown"], 14));
    case.push_str(&presses(&["ArrowUp", "Tab"], 2));
    case.push_str("press Escape\nexpect surface title\n");
    case.push_str("activate title.gallery\nexpect surface gallery\n");
    case.push_str(&presses(&all, 3));
    case.push_str("press Escape\nexpect surface title\n");
    case.push_str("new-game\nmenu\nexpect surface menu\n");
    case.push_str(&presses(&all, 3));
    case.push_str(&presses(&["Tab"], 7));
    case.push_str("press Escape\npress Escape\nexpect surface menu\n");
    for (tabs, surface) in [(5, "settings"), (4, "gallery"), (3, "timeline")] {
        case.push_str("press Digit1\n");
        case.push_str(&presses(&["Tab"], tabs));
        case.push_str(&format!("press Enter\nexpect surface {surface}\n"));
        case.push_str(&presses(&all, 3));
        case.push_str(&presses(&["ArrowDown"], 14));
        case.push_str("press Escape\nexpect surface menu\n");
    }
    case.push_str("save 1\nsave 1\nexpect surface confirm\n");
    case.push_str(&presses(&["ArrowLeft", "ArrowRight", "ArrowUp", "Tab"], 2));
    case.push_str("press Escape\nexpect surface menu\n");
    case
}

#[test]
fn the_walk_function_predicts_every_focus_move_the_player_makes() {
    let (_dir, root, pack) = demo_pack();
    let text = walk_case();
    let case = parse_case("tests/walk.keiko", &text).expect("the case parses");
    let ledger = Arc::new(Mutex::new(Ledger::default()));
    let result = run_with(
        request(&case, &pack, &root),
        vec![WalkOracle {
            ledger: ledger.clone(),
        }],
    );

    assert!(result.outcome.is_ok(), "{:?}", result.outcome);
    let ledger = ledger.lock().unwrap();
    assert!(
        ledger.checked >= 150,
        "the case presses enough walk keys to cover every surface, checked {}",
        ledger.checked
    );
    assert!(
        ledger.mismatches.is_empty(),
        "walk and the player disagree:\n{}",
        ledger.mismatches.join("\n")
    );
}

fn take(text: &str) -> super::TakeResult {
    let (_dir, root, pack) = demo_pack();
    let case = parse_case("tests/walk.keiko", text).expect("the case parses");
    super::run(request(&case, &pack, &root))
}

#[test]
fn activate_plans_its_route_through_the_top_bar() {
    let result =
        take("keiko 1\n\nnew-game\nmenu\nactivate overlay.timeline\nexpect surface timeline\n");
    assert!(result.outcome.is_ok(), "{:?}", result.outcome);
}

#[test]
fn activate_on_a_pointer_only_control_names_the_search_and_presses_nothing() {
    let result = take("keiko 1\n\nactivate title.settings\nactivate settings.back\n");
    let finding = result
        .outcome
        .expect_err("no walk key reaches the back control");
    assert_eq!(finding.rule, crate::keiko::finding::rules::STEP_NO_TARGET);
    assert!(
        finding.message.contains("from `settings.master`") && finding.message.contains("12 stops"),
        "the message names the start and the stops searched, got `{}`",
        finding.message
    );
    assert_eq!(result.observation.surface, "settings");
    let focused: Vec<&str> = result
        .observation
        .controls
        .iter()
        .filter(|control| control.focused)
        .map(|control| control.id.as_str())
        .collect();
    assert_eq!(focused, vec!["settings.master"], "nothing was pressed");
}

#[test]
fn the_overlays_sub_views_list_the_narrowed_top_bar_and_the_title_doors_do_not() {
    let result = take(
        "keiko 1\n\n\
         activate title.settings\nexpect control overlay.save-tab absent\npress Escape\n\
         activate title.gallery\nexpect control overlay.load-tab absent\npress Escape\n\
         new-game\nmenu\n\
         activate overlay.settings\nexpect control overlay.save-tab enabled\nexpect control overlay.load-tab enabled\n\
         activate overlay.load-tab\nexpect control overlay.load-tab focused\npress Escape\nexpect surface menu\n\
         activate overlay.gallery\nexpect control overlay.save-tab enabled\npress Escape\n\
         activate overlay.timeline\nexpect control overlay.load-tab enabled\n",
    );
    assert!(result.outcome.is_ok(), "{:?}", result.outcome);
}

/// The title's-door defect, planted: `↓` on the gallery moves the focus onto
/// the overlay's Resume, which the gallery does not draw.
pub(crate) struct DownToResume;

impl Plugin for DownToResume {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (|keys: Res<ButtonInput<KeyCode>>, menu: Option<ResMut<MenuState>>| {
                let Some(mut menu) = menu else {
                    return;
                };
                if menu.open
                    && menu.view == crate::menu::MenuView::Gallery
                    && !menu.gallery_still
                    && keys.just_pressed(KeyCode::ArrowDown)
                {
                    menu.focus = MenuFocus::Resume;
                }
            })
            .after(crate::menu::menu_input),
        );
    }
}

#[test]
fn a_focus_moved_onto_an_undrawn_control_fails_the_take_at_that_step() {
    let (_dir, root, pack) = demo_pack();
    let text = "keiko 1\n\nactivate title.gallery\nexpect surface gallery\npress ArrowDown\nexpect surface gallery\n";
    let case = parse_case("tests/walk.keiko", text).expect("the case parses");
    let result = run_with(request(&case, &pack, &root), vec![DownToResume]);
    let finding = result.outcome.expect_err("the focus went hidden");
    assert_eq!(finding.rule, crate::keiko::finding::rules::FOCUS_HIDDEN);
    assert_eq!(finding.path, unmei_engine::keiko::Location::Line(5));
    assert!(
        finding.message.contains("`overlay.resume`") && finding.message.contains("gallery"),
        "the finding names the focus and the surface, got `{}`",
        finding.message
    );
}

#[test]
fn a_take_whose_focus_stays_on_the_list_passes_the_focus_check() {
    let result = take("keiko 1\n\nactivate title.gallery\npress ArrowDown\npress ArrowDown\nexpect surface gallery\n");
    assert!(result.outcome.is_ok(), "{:?}", result.outcome);
}

/// `Esc` does nothing on the Settings view.
struct InertEscOnSettings;

impl Plugin for InertEscOnSettings {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (|mut keys: ResMut<ButtonInput<KeyCode>>, menu: Option<Res<MenuState>>| {
                if menu
                    .is_some_and(|menu| menu.open && menu.view == crate::menu::MenuView::Settings)
                {
                    keys.clear_just_pressed(KeyCode::Escape);
                }
            })
            .before(crate::menu::menu_input),
        );
    }
}

fn findings_by_rule(report: &crate::keiko::explore::Report, rule: &str) -> Vec<serde_json::Value> {
    report
        .findings
        .iter()
        .filter(|finding| finding.get("rule").and_then(serde_json::Value::as_str) == Some(rule))
        .cloned()
        .collect()
}

#[test]
fn the_menu_walk_saves_hidden_focus_findings_that_fail_until_the_defect_is_gone() {
    let (_dir, root, pack) = demo_pack();
    let report = super::with_test_hook(
        |app| {
            app.add_plugins(DownToResume);
        },
        || crate::keiko::explore::menus::run(&root, &pack, 4000, None, false),
    );
    let hidden = findings_by_rule(&report, crate::keiko::finding::rules::FOCUS_HIDDEN);
    assert_eq!(
        hidden.len(),
        report.findings.len(),
        "the planted defect is the only problem: {:#?}",
        report.findings
    );
    assert!(
        hidden.iter().any(|finding| finding
            .get("message")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|message| message.contains("`overlay.resume`"))),
        "{hidden:#?}"
    );
    for finding in &hidden {
        let case_path = finding
            .get("case")
            .and_then(serde_json::Value::as_str)
            .expect("each hidden-focus finding is saved as a case");
        let text = std::fs::read_to_string(root.join(case_path)).expect("the saved case exists");
        let case = parse_case(case_path, &text).expect("the saved case parses");

        let with_defect = super::with_test_hook(
            |app| {
                app.add_plugins(DownToResume);
            },
            || super::run(request(&case, &pack, &root)),
        );
        let failure = with_defect
            .outcome
            .expect_err("the saved case fails while the defect exists");
        assert_eq!(failure.rule, crate::keiko::finding::rules::FOCUS_HIDDEN);

        let fixed = super::run(request(&case, &pack, &root));
        assert!(
            fixed.outcome.is_ok(),
            "the saved case passes once the defect is gone: {:?}",
            fixed.outcome
        );
    }
}

#[test]
fn the_menu_walk_reports_an_inert_esc_as_unsaved_unwind_findings() {
    let (_dir, root, pack) = demo_pack();
    let report = super::with_test_hook(
        |app| {
            app.add_plugins(InertEscOnSettings);
        },
        || crate::keiko::explore::menus::run(&root, &pack, 4000, None, false),
    );
    let unwind = findings_by_rule(&report, crate::keiko::finding::rules::UNWIND);
    assert_eq!(
        unwind.len(),
        report.findings.len(),
        "the planted defect is the only problem: {:#?}",
        report.findings
    );
    assert!(
        unwind.iter().any(|finding| finding
            .get("message")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|message| message.contains("the settings surface"))),
        "{unwind:#?}"
    );
    for finding in &unwind {
        assert_eq!(finding.get("saved"), Some(&serde_json::Value::Bool(false)));
        assert!(finding.get("case").is_none());
    }
}

#[test]
fn the_menu_walk_over_the_demo_pack_finds_nothing_and_repeats_itself() {
    let (_dir, root, pack) = demo_pack();
    let first = crate::keiko::explore::menus::run(&root, &pack, 4000, None, false);
    assert!(first.findings.is_empty(), "{:#?}", first.findings);
    let second = crate::keiko::explore::menus::run(&root, &pack, 4000, None, false);
    assert_eq!(first.to_json(), second.to_json());
    let short = crate::keiko::explore::menus::run(&root, &pack, 40, None, false);
    assert!(short.budget_spent);
    assert_eq!(short.used, short.used.min(40));
}

static DRAWN_GAPS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static DRAWN_CHECKS: Mutex<u32> = Mutex::new(0);

/// In a take that draws, every control on the current surface's list has a
/// visible laid-out node (spec §15 rule 43).
fn check_listed_controls_are_drawn(world: &mut World) {
    let Some(menu) = world.get_resource::<MenuState>().cloned() else {
        return;
    };
    if !menu.open {
        return;
    }
    let ids: Vec<String> = crate::keiko::controls::observe_controls(world)
        .into_iter()
        .map(|control| control.id)
        .collect();
    for id in ids {
        *DRAWN_CHECKS.lock().unwrap() += 1;
        if let Err(error) = crate::keiko::render::control_node(world, &id) {
            DRAWN_GAPS
                .lock()
                .unwrap()
                .push(format!("{:?}: {}", menu.view, error.message));
        }
    }
}

#[test]
fn every_listed_control_has_a_visible_node_in_a_take_that_draws() {
    if !crate::keiko::render::available() {
        panic!("this rule needs the software rasterizer (spec §13)");
    }
    let (_dir, root, pack) = demo_pack();
    let text = "keiko 1\nsetting play.text_speed 0\n\nframe\n\
                activate title.settings\npress Escape\nactivate title.gallery\npress Escape\n\
                new-game\nmenu\nactivate overlay.settings\npress Escape\n\
                activate overlay.gallery\npress Escape\nactivate overlay.timeline\npress Escape\n\
                save 1\nsave 1\nexpect surface confirm\npress Escape\n";
    let case = parse_case("tests/drawn.keiko", text).expect("the case parses");
    let result = super::with_test_hook(
        |app| {
            app.add_systems(Last, check_listed_controls_are_drawn);
        },
        || super::run(request(&case, &pack, &root)),
    );
    assert!(result.outcome.is_ok(), "{:?}", result.outcome);
    assert!(*DRAWN_CHECKS.lock().unwrap() > 100);
    let gaps = DRAWN_GAPS.lock().unwrap();
    assert!(
        gaps.is_empty(),
        "listed but not drawn:\n{}",
        gaps.join("\n")
    );
}

static ROUTE_GAPS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static ROUTES_PLANNED: Mutex<u32> = Mutex::new(0);

/// From the current focus, a route reaches every listed control except the
/// pointer-only ones (spec §3.2, §15 rule 41).
fn plan_every_listed_control(world: &mut World) {
    let Some(menu) = world.get_resource::<MenuState>().cloned() else {
        return;
    };
    if menu.key_editing {
        return;
    }
    let counts = counts(world.get_resource::<crate::Runtime>());
    for control in crate::keiko::controls::observe_controls(world) {
        if matches!(control.id.as_str(), "settings.back" | "legacy.cancel") {
            continue;
        }
        if menu.picker.is_some() && !control.id.starts_with("legacy.target.") {
            continue;
        }
        let Some(target) = crate::keiko::controls::focus_for_id(&control.id) else {
            continue;
        };
        *ROUTES_PLANNED.lock().unwrap() += 1;
        if let Err(unreachable) = crate::keiko::route::plan(&menu, counts, menu.focus, target) {
            ROUTE_GAPS.lock().unwrap().push(format!(
                "{:?}: `{}` from {:?} ({} stops)",
                menu.view, control.id, unreachable.from, unreachable.searched
            ));
        }
    }
}

#[test]
fn a_route_reaches_every_listed_control_on_every_surface_the_fixture_packs_reach() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/keiko");
    for name in ["d", "e", "g", "l"] {
        let root = fixtures.join(name);
        let pack = load_pack(&root).expect("the fixture pack loads").pack;
        let mut cases: Vec<PathBuf> = std::fs::read_dir(root.join("tests"))
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "keiko")
            })
            .collect();
        cases.sort();
        for path in cases {
            let file = format!("tests/{}", path.file_name().unwrap().to_string_lossy());
            let text = std::fs::read_to_string(&path).unwrap();
            let case = parse_case(&file, &text).expect("the fixture case parses");
            let mut take_request = request(&case, &pack, &root);
            take_request.case_file = file;
            let _ = super::with_test_hook(
                |app| {
                    app.add_systems(Last, plan_every_listed_control);
                },
                || super::run(take_request),
            );
        }
    }
    assert!(*ROUTES_PLANNED.lock().unwrap() > 1000);
    let gaps = ROUTE_GAPS.lock().unwrap();
    assert!(
        gaps.is_empty(),
        "unreachable controls:\n{}",
        gaps.join("\n")
    );
}
