//! Shared fixtures for the save suites: the demo pack pointed at a temp
//! directory, a captured valid save, and the drive/position helpers.

use super::*;
use crate::content::{Line, Pack};
use std::path::Path as StdPath;

mod legacy;
mod resume;
mod slots;

fn demo_pack() -> Pack {
    let root = StdPath::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
    crate::pack::load_pack(&root).expect("demo pack loads").pack
}

/// The demo pack pointed at a temp dir, so sibling slot files never dirty
/// the repo. The runner reads only pack content, never `root`.
fn temp_pack() -> (tempfile::TempDir, Pack) {
    let dir = tempfile::tempdir().unwrap();
    let mut pack = demo_pack();
    pack.root = dir.path().join("demo-pack");
    (dir, pack)
}

/// A valid save parked on the demo pack's first say-line.
fn parked_save() -> SaveFile {
    let mut runner = ScriptRunner::new(demo_pack());
    runner.run();
    SaveFile::capture(&runner, &StagingState::default())
}

/// Serialise a v1-shaped save — one with no `saved_at` key at all.
fn v1_ron(save: &SaveFile) -> String {
    #[derive(serde::Serialize)]
    struct V1Save {
        version: u32,
        state: HashMap<String, StateValue>,
        scene_id: String,
        pc: usize,
        wait: Option<Wait>,
        staging: StagingState,
    }
    let v1 = V1Save {
        version: 1,
        state: save.state.clone(),
        scene_id: save.scene_id.clone(),
        pc: save.pc,
        wait: save.wait.clone(),
        staging: save.staging.clone(),
    };
    ron::ser::to_string_pretty(&v1, ron::ser::PrettyConfig::default()).unwrap()
}

fn status(pack: &Pack, slot: Slot) -> SlotStatus {
    list_slots(pack)
        .into_iter()
        .find(|info| info.slot == slot)
        .expect("slot is in Slot::ALL")
        .status
}

fn drive_to_say(runner: &mut ScriptRunner) -> Wait {
    runner.run().wait.expect("runner parks on a say")
}

fn drive_to_choice(runner: &mut ScriptRunner) -> Wait {
    runner.run();
    runner.advance_say().unwrap();
    runner.run();
    runner.advance_say().unwrap();
    runner.run().wait.expect("runner parks on a choice")
}

fn drive_to_decision(runner: &mut ScriptRunner) -> Wait {
    loop {
        let outcome = runner.run();
        match outcome.wait {
            Some(Wait::Say { .. }) => runner.advance_say().unwrap(),
            Some(Wait::Choice { .. }) => runner.choose(0).unwrap(),
            Some(Wait::Decision { .. }) => return runner.wait().unwrap().clone(),
            None => panic!("script ended before reaching a decision"),
        }
    }
}

/// A one-scene pack whose first arm contains the only say-line, so a save
/// can be taken while parked inside a conditional.
fn conditional_pack() -> Pack {
    let scene = crate::parser::Parser::new("scene.unmei")
        .parse(
            "scene",
            "if met_ace\nbg warm\nNarrator: In the arm.\nelse\nNarrator: Else arm.\nendif\nNarrator: After.\n",
        )
        .expect("conditional scene parses");
    Pack {
        manifest: crate::content::Manifest {
            name: "test".to_string(),
            entry: "scene".to_string(),
            tagline: None,
            title_backdrop: None,
            scenes: Vec::new(),
        },
        state: HashMap::from([("met_ace".to_string(), StateValue::Bool(true))]),
        scenes: HashMap::from([("scene".to_string(), scene)]),
        npcs: HashMap::new(),
        root: PathBuf::from("."),
    }
}

/// The runner's four observable position fields, for refusal-preservation
/// checks: scene id, `pc`, state map and parked wait.
fn position(runner: &ScriptRunner) -> (String, usize, HashMap<String, StateValue>, Option<Wait>) {
    (
        runner.scene_id().to_string(),
        runner.pc(),
        runner.state().clone(),
        runner.wait().cloned(),
    )
}

/// The demo pack with the saved wait's own line turned into a non-wait
/// command. The scene id, line count and state vocabulary stay identical,
/// so the same value is structurally valid for it except that the parked
/// wait no longer matches its line.
fn pack_with_mismatched_wait_line(save: &SaveFile) -> Pack {
    let mut pack = demo_pack();
    let scene = pack
        .scenes
        .get_mut(&save.scene_id)
        .expect("the save names a demo scene");
    scene.lines[save.pc - 1] = Line::Bg("school_gate".to_string());
    pack
}

/// A runner driven across into the `reunion` scene with non-default state and
/// a parked say — a live destination for refusal-preservation checks, so a
/// refusal has a real scene, `pc`, state map and wait to leave untouched.
fn runner_in_reunion() -> ScriptRunner {
    let mut runner = ScriptRunner::new(demo_pack());
    // Advance through the "ask about the letter" branch so state differs and
    // the script has crossed into the reunion scene.
    runner.run();
    runner.advance_say().unwrap();
    runner.run();
    runner.advance_say().unwrap();
    runner.run();
    runner.choose(0).unwrap();
    runner.run();
    runner.advance_say().unwrap();
    drive_to_say(&mut runner);
    runner
}

fn status_of(save: &SaveFile) -> SlotStatus {
    SlotStatus::Ok {
        scene_id: save.scene_id.clone(),
        saved_at: save.saved_at,
    }
}

fn tmp_path_for(pack: &Pack, slot: Slot) -> PathBuf {
    save_path_for(pack, slot).with_extension("ron.tmp")
}
