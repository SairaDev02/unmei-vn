//! Shared fixtures for the runner suites: the demo pack, synthetic single-scene
//! packs, and the drive helpers that walk a runner to a park.

use super::*;
use crate::content::Manifest;
use crate::parser::Parser;
use std::path::{Path, PathBuf};

mod conditions;
mod epilogue;
mod parks;

fn demo_pack() -> Pack {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
    crate::pack::load_pack(&root).expect("demo pack loads").pack
}

fn test_pack(scenes: &[(&str, &str)], state: HashMap<String, StateValue>) -> Pack {
    let mut map = HashMap::new();
    for (id, src) in scenes {
        let parsed = Parser::new(&format!("{id}.unmei"))
            .parse(id, src)
            .expect("test scene parses");
        map.insert(id.to_string(), parsed);
    }
    Pack {
        manifest: Manifest {
            name: "test".to_string(),
            entry: scenes[0].0.to_string(),
            tagline: None,
            title_backdrop: None,
            scenes: Vec::new(),
        },
        state,
        scenes: map,
        npcs: HashMap::new(),
        root: PathBuf::from("."),
    }
}

/// Drive through say/choice waits until the runner parks on a decision.
fn drive_to_decision(runner: &mut ScriptRunner) {
    loop {
        let outcome = runner.run();
        match outcome.wait {
            Some(Wait::Say { .. }) => runner.advance_say().unwrap(),
            Some(Wait::Choice { .. }) => runner.choose(0).unwrap(),
            Some(Wait::Decision { .. }) => return,
            None => panic!("script ended before reaching a decision"),
        }
    }
}

fn say_text(wait: &Wait) -> (&str, &str) {
    match wait {
        Wait::Say { speaker, text } => (speaker, text),
        other => panic!("expected a say-wait, got {other:?}"),
    }
}

fn say_after(runner: &mut ScriptRunner) -> String {
    // Advance past the current say-wait and return the next line's text.
    runner.advance_say().unwrap();
    let outcome = runner.run();
    say_text(outcome.wait.as_ref().unwrap()).1.to_string()
}

/// A runner parked at the top of the demo pack's epilogue with the shipped
/// initial state and no wait — the position a `jump epilogue` leaves behind.
fn demo_pack_at_epilogue() -> ScriptRunner {
    let pack = demo_pack();
    let initial = pack.state.clone();
    let mut runner = ScriptRunner::new(pack);
    runner
        .restore_position("epilogue".to_string(), 0, initial, None)
        .expect("epilogue is a scene in the demo pack");
    runner
}

/// Drive the demo pack from its entry scene until it parks on the
/// epilogue's first say-line, taking `entry_choice` at the `first_meeting`
/// choice and landing the greet decision on `warm`.
fn play_demo_to_epilogue(entry_choice: usize) -> (ScriptRunner, String, String) {
    let mut runner = ScriptRunner::new(demo_pack());
    let mut picked = false;
    loop {
        let outcome = runner.run();
        match outcome.wait {
            Some(Wait::Say { speaker, text }) => {
                if runner.scene_id() == "epilogue" {
                    return (runner, speaker, text);
                }
                runner.advance_say().unwrap();
            }
            Some(Wait::Choice { .. }) => {
                runner
                    .choose(if picked { 0 } else { entry_choice })
                    .unwrap();
                picked = true;
            }
            Some(Wait::Decision { .. }) => runner.resolve_decision("warm").unwrap(),
            None => panic!("demo pack ended before reaching the epilogue"),
        }
    }
}
