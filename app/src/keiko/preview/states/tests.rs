//! §9.2's slate planning, proved on the demo pack and on a pack with no park
//! four of the states need (check and preview spec §13 rule 24).

use super::*;
use std::path::PathBuf;
use unmei_engine::keiko::Pick;
use unmei_engine::StateValue;

/// The repository's demo pack: a say-line, a narration, a choice, a decide
/// point, three gallery cards and every confirmation dialog.
fn demo() -> Pack {
    load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack"))
}

/// A pack whose one scene is all narration and which declares no art.
fn sparse() -> Pack {
    load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/preview/sparse"))
}

fn load(root: PathBuf) -> Pack {
    unmei_engine::load_pack(&root).expect("the pack loads").pack
}

/// The steps of a state the slate planned a case for.
fn steps(slate: &[Fixture], name: &str) -> Vec<Step> {
    match &find(slate, name).outcome {
        Outcome::Case(case) => case.steps.iter().map(|line| line.step.clone()).collect(),
        Outcome::Unreachable(reason) => panic!("`{name}` is unreachable: {reason}"),
    }
}

fn find<'a>(slate: &'a [Fixture], name: &str) -> &'a Fixture {
    slate
        .iter()
        .find(|fixture| fixture.name == name)
        .unwrap_or_else(|| panic!("the slate holds `{name}`"))
}

/// §9.2's table in its order, each state a case the demo pack reaches.
#[test]
fn the_demo_pack_reaches_every_fixture_state() {
    let slate = slate(&demo(), &PreviewRequest::default());
    let names: Vec<&str> = slate.iter().map(|fixture| fixture.name).collect();
    assert_eq!(
        names,
        [
            "title-fresh",
            "title-continue",
            "say",
            "narration",
            "choice",
            "decision",
            "scene-end",
            "menu-save",
            "menu-load",
            "menu-settings",
            "menu-gallery",
            "menu-timeline",
            "still",
            "confirm-overwrite",
            "confirm-load",
            "confirm-reset-audio",
            "confirm-reset-play",
            "confirm-new-game",
            "confirm-quit",
            "confirm-exit-to-title",
            "confirm-clear-key",
        ]
    );
    for fixture in &slate {
        let Outcome::Case(case) = &fixture.outcome else {
            let Outcome::Unreachable(reason) = &fixture.outcome else {
                unreachable!()
            };
            panic!("`{}` should be reachable: {reason}", fixture.name);
        };
        // Every state asserts the surface it reached, then frames itself.
        assert_eq!(
            case.steps.last().map(|line| line.step.clone()),
            Some(frame(fixture.name)),
            "{}",
            fixture.name
        );
        assert!(
            case.steps
                .iter()
                .any(|line| matches!(line.step, Step::Expect(_))),
            "`{}` asserts what it reached",
            fixture.name
        );
    }
}

/// The two title states boot on the title, and the one with a live session
/// leaves play through the overlay's own rail so Continue is enabled.
#[test]
fn the_title_states_assert_continues_row() {
    let slate = slate(&demo(), &PreviewRequest::default());
    assert_eq!(
        steps(&slate, "title-fresh"),
        [
            Step::Expect(surface("title")),
            Step::Expect(enabled("title.continue", ControlCondition::Disabled)),
            frame("title-fresh"),
        ]
    );
    let continuing = steps(&slate, "title-continue");
    assert_eq!(continuing.first(), Some(&Step::NewGame));
    assert!(continuing.contains(&activate("overlay.exit-to-title")));
    assert_eq!(
        continuing[continuing.len() - 3..].to_vec(),
        vec![
            Step::Expect(surface("title")),
            Step::Expect(enabled("title.continue", ControlCondition::Enabled)),
            frame("title-continue"),
        ]
    );
}

/// The play states walk the pack's own path: the first character's
/// say-line, the first `Narrator` line, the first choice and the first
/// decide point, each from a fresh take.
#[test]
fn a_play_state_walks_the_packs_own_path() {
    let slate = slate(&demo(), &PreviewRequest::default());
    assert_eq!(
        steps(&slate, "say"),
        [
            Step::NewGame,
            Step::Advance(1),
            Step::Expect(surface("play")),
            Step::Expect(Expect::Speaker(Some("Ace".to_string()))),
            Step::Expect(Expect::Revealed(true)),
            frame("say"),
        ]
    );
    assert_eq!(
        steps(&slate, "narration"),
        [
            Step::NewGame,
            Step::Expect(surface("play")),
            Step::Expect(Expect::Speaker(Some("Narrator".to_string()))),
            Step::Expect(Expect::Revealed(true)),
            frame("narration"),
        ]
    );
    assert_eq!(
        steps(&slate, "choice"),
        [
            Step::NewGame,
            Step::Advance(2),
            Step::Expect(surface("play")),
            Step::Expect(Expect::Choices(vec![
                "Ask about the letter".to_string(),
                "Stay silent".to_string(),
            ])),
            frame("choice"),
        ]
    );
    assert_eq!(
        steps(&slate, "decision"),
        [
            Step::NewGame,
            Step::Advance(2),
            Step::Pick(Pick::Index(1)),
            Step::Advance(2),
            Step::Expect(surface("play")),
            Step::Expect(Expect::Decision(DecisionState::Pending)),
            frame("decision"),
        ]
    );
    // The `decision-hold` is what parks the point, so the case carries the
    // point it holds (spec §9.2).
    let Outcome::Case(case) = &find(&slate, "decision").outcome else {
        panic!("a case")
    };
    let holds: Vec<&Header> = case
        .headers
        .iter()
        .filter(|line| matches!(line.header, Header::Fault { .. }))
        .map(|line| &line.header)
        .collect();
    assert_eq!(
        holds,
        [&Header::Fault {
            fault: Fault::DecisionHold {
                point: "ace.greet".to_string()
            }
        }]
    );
}

/// A scene end follows the whole path to the script's own end, where no
/// line is parked.
#[test]
fn a_scene_end_walks_past_the_last_park() {
    let slate = slate(&demo(), &PreviewRequest::default());
    let steps = steps(&slate, "scene-end");
    assert_eq!(steps.first(), Some(&Step::NewGame));
    assert_eq!(
        steps[steps.len() - 3..].to_vec(),
        vec![
            Step::Expect(surface("play")),
            Step::Expect(Expect::Speaker(None)),
            frame("scene-end"),
        ]
    );
}

/// `--state` seeds the walk, so an override that moves a branch moves the
/// steps and rides the take's `start` header.
#[test]
fn a_state_override_seeds_the_take_through_the_start_header() {
    let pack = demo();
    let request = PreviewRequest {
        state: vec![(
            "affinity_level".to_string(),
            StateValue::Str("high".to_string()),
        )],
        ..PreviewRequest::default()
    };
    let seeded = slate(&pack, &request);
    let Outcome::Case(case) = &find(&seeded, "say").outcome else {
        panic!("a case")
    };
    assert_eq!(case.start_scene(), Some("first_meeting"));
    assert_eq!(case.state_overrides(), [("affinity_level", "\"high\"")]);
    assert!(
        !case.steps.iter().any(|line| line.step == Step::NewGame),
        "a `start` header enters play itself"
    );

    // The second option's path draws one fewer epilogue line, so the
    // request's picks reach the states the walk plans (spec §9.1).
    let advances = |slate: &[Fixture]| {
        steps(slate, "scene-end")
            .iter()
            .map(|step| match step {
                Step::Advance(count) => *count,
                _ => 0,
            })
            .sum::<u32>()
    };
    let default = slate(&pack, &PreviewRequest::default());
    let second = slate(
        &pack,
        &PreviewRequest {
            picks: vec![2],
            ..PreviewRequest::default()
        },
    );
    assert_eq!(advances(&second), advances(&default) - 1);
}

/// A pack with no character say-line, no choice, no decide point and no
/// declared art cannot reach those four states, and says why.
#[test]
fn a_pack_without_a_park_reports_that_state_unreachable() {
    let slate = slate(&sparse(), &PreviewRequest::default());
    for name in ["say", "choice", "decision", "still"] {
        let Outcome::Unreachable(reason) = &find(&slate, name).outcome else {
            panic!("`{name}` should be unreachable")
        };
        assert!(!reason.is_empty(), "{name} says why");
    }
    let Outcome::Unreachable(reason) = &find(&slate, "still").outcome else {
        panic!("no gallery card")
    };
    assert!(reason.contains("no gallery art"), "{reason}");
    for name in [
        "title-fresh",
        "title-continue",
        "narration",
        "scene-end",
        "menu-save",
        "menu-settings",
        "confirm-overwrite",
        "confirm-load",
        "confirm-clear-key",
    ] {
        assert!(
            matches!(find(&slate, name).outcome, Outcome::Case(_)),
            "`{name}` needs no park the pack lacks"
        );
    }
}
