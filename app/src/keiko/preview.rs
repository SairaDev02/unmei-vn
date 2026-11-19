//! The shared replay: turn the plugin family's preview flags into a case
//! (spec §12).
//!
//! `--scene`, `--at`, `--state` and `--pick` map onto `start`/`state` headers
//! and the `advance`/`pick` steps that reach a named line. The preview runs the
//! generated case through the same take engine a test uses, so there is one
//! replay and it cannot drift.

mod run;

pub use run::run;

use unmei_engine::keiko::{parse_case, Case, Finding, Step};
use unmei_engine::runner::Wait;
use unmei_engine::{Pack, ScriptRunner, StateValue};

/// The preview's request, after its own argument parsing.
#[derive(Debug, Clone, Default)]
pub struct PreviewRequest {
    /// The scene to start at.
    pub scene: Option<String>,
    /// `(scene, line)` to reach.
    pub at: Option<(String, u32)>,
    /// `--state` overrides, merged over the pack's initial state.
    pub state: Vec<(String, StateValue)>,
    /// The 1-based option to take at each successive choice.
    pub picks: Vec<usize>,
}

/// Turn a preview request into an ordinary case.
pub fn build_case(pack: &Pack, request: &PreviewRequest) -> Result<Case, Finding> {
    let scene = request
        .at
        .as_ref()
        .map(|(scene, _)| scene.clone())
        .or_else(|| request.scene.clone())
        .ok_or_else(|| {
            Finding::new(
                "preview",
                String::new(),
                unmei_engine::keiko::rules::CASE_REFERENCE,
                "the preview needs --scene or --at",
            )
        })?;
    if !pack.scenes.contains_key(&scene) {
        return Err(Finding::new(
            "preview",
            String::new(),
            unmei_engine::keiko::rules::CASE_REFERENCE,
            format!("unknown scene `{scene}`"),
        ));
    }

    let mut text = String::from("keiko 1\n");
    text.push_str(&format!("start {scene}\n"));
    for (key, value) in &request.state {
        text.push_str(&format!("state {key} = {}\n", state_text(value)));
    }
    if let Some((_, line)) = &request.at {
        for step in steps_to(pack, &scene, *line, &request.picks)? {
            text.push_str(&step);
            text.push('\n');
        }
    }
    parse_case("preview", &text)
}

/// The steps that reach `line` from the scene's top.
fn steps_to(pack: &Pack, scene: &str, line: u32, picks: &[usize]) -> Result<Vec<String>, Finding> {
    let mut runner = ScriptRunner::new(pack.clone());
    if runner
        .restore_position(scene.to_string(), 0, pack.state.clone(), None)
        .is_err()
    {
        return Ok(Vec::new());
    }
    let mut steps = Vec::new();
    let mut picks = picks.iter().copied();
    for _ in 0..10_000 {
        // A cycle with no say-line, choice or decide stops the player with a
        // defensive failure (row 13). The replay must report why it stopped
        // rather than hand back a case that quietly ends short of the line.
        let outcome = runner.run().map_err(|error| {
            Finding::new(
                "preview",
                String::new(),
                unmei_engine::keiko::rules::DEFENSIVE,
                format!("the replay stopped before line {line}: {error}"),
            )
        })?;
        let Some(wait) = outcome.wait else {
            break;
        };
        let parked = park_line(&runner);
        if parked == Some(line) {
            break;
        }
        match wait {
            Wait::Say { .. } => {
                steps.push("advance".to_string());
                if runner.advance_say().is_err() {
                    break;
                }
            }
            Wait::Choice { options } => {
                let index = picks.next().unwrap_or(1).clamp(1, options.len().max(1));
                steps.push(format!("pick {index}"));
                if runner.choose(index - 1).is_err() {
                    break;
                }
            }
            Wait::Decision {
                npc_id, question, ..
            } => {
                // No decide form: every decision takes the author's fallback.
                let Some(config) = pack
                    .npcs
                    .get(&npc_id)
                    .and_then(|npc| npc.decisions.get(&question))
                else {
                    break;
                };
                let landing = config.fallback.landing_key();
                if runner.resolve_decision(&landing).is_err() {
                    break;
                }
            }
        }
    }
    Ok(steps)
}

fn park_line(runner: &ScriptRunner) -> Option<u32> {
    let scene = runner.pack().scenes.get(runner.scene_id())?;
    let index = runner.pc().saturating_sub(1);
    let line = scene.source_line(index);
    (line > 0).then_some(line as u32)
}

/// A state value in the case's `state <key> = <value>` form.
fn state_text(value: &StateValue) -> String {
    match value {
        StateValue::Str(text) => {
            // The case grammar's quoted token accepts exactly these two escapes.
            let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        }
        StateValue::Num(number) => number.to_string(),
        StateValue::Bool(flag) => flag.to_string(),
    }
}

/// The step list a generated case holds, for a caller that reads it back.
pub fn generated_steps(case: &Case) -> Vec<&Step> {
    case.steps.iter().map(|line| &line.step).collect()
}

/// A `state` override parsed into the map's form.
pub fn parse_state(value: &str) -> Option<StateValue> {
    unmei_engine::keiko::parse_state_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keiko::take::{self, TakeRequest};

    #[test]
    fn parse_state_reads_the_state_ron_form() {
        assert_eq!(parse_state("1"), Some(StateValue::Num(1.0)));
        assert_eq!(parse_state("true"), Some(StateValue::Bool(true)));
        assert_eq!(
            parse_state("\"high\""),
            Some(StateValue::Str("high".to_string()))
        );
        assert_eq!(parse_state("high"), None);
    }

    /// A scene whose loop parks only while `met_ace` holds: with the declared
    /// `false` it spins, a cycle pack load cannot see because it passes through
    /// an `if`.
    const SPINNING_SCENE: &str = "A: entry.\nlabel top\nif met_ace\nA: met.\nendif\njump top\n";

    fn spinning_pack() -> (tempfile::TempDir, Pack) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("spin");
        std::fs::create_dir_all(root.join("scenes")).unwrap();
        std::fs::create_dir_all(root.join("npcs")).unwrap();
        std::fs::write(root.join("pack.ron"), "(name: \"spin\", entry: \"a\")").unwrap();
        std::fs::write(root.join("state.ron"), "(met_ace: false,)").unwrap();
        std::fs::write(root.join("scenes/a.unmei"), SPINNING_SCENE).unwrap();
        let loaded = unmei_engine::load_pack(&root).expect("the pack loads");
        (dir, loaded.pack)
    }

    /// Two options that land on different say-lines, so a settled line names
    /// which option the replay took.
    const CHOICE_SCENE: &str = "Narrator: entry.
choice
  \"Left\" -> left
  \"Right\" -> right
end
label left
Narrator: the left line.
jump done
label right
Narrator: the right line.
label done
Narrator: done.
";

    /// Lines 7 and 10 are the two options' say-lines.
    const LEFT_LINE: u32 = 7;
    const RIGHT_LINE: u32 = 10;

    fn choice_pack() -> (tempfile::TempDir, std::path::PathBuf, Pack) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("pick");
        std::fs::create_dir_all(root.join("scenes")).unwrap();
        std::fs::create_dir_all(root.join("npcs")).unwrap();
        std::fs::write(root.join("pack.ron"), "(name: \"pick\", entry: \"a\")").unwrap();
        std::fs::write(root.join("state.ron"), "(met: false,)").unwrap();
        std::fs::write(root.join("scenes/a.unmei"), CHOICE_SCENE).unwrap();
        let loaded = unmei_engine::load_pack(&root).expect("the pack loads");
        (dir, root, loaded.pack)
    }

    /// Build the preview's case and run it through the same take engine a test
    /// uses, which is the preview's whole contract.
    fn take(request: &PreviewRequest) -> (tempfile::TempDir, take::TakeResult) {
        let (dir, root, pack) = choice_pack();
        let case = build_case(&pack, request).expect("the replay reaches the line");
        let result = take::run(TakeRequest {
            case: &case,
            case_file: "preview".to_string(),
            original_pack: &root,
            pack: &pack,
            take_index: 1,
            keep: false,
            live: None,
        });
        (dir, result)
    }

    /// `--at` past a choice with a `--pick` list settles at that line.
    #[test]
    fn a_replay_past_a_choice_settles_at_the_picked_options_line() {
        let (_dir, result) = take(&PreviewRequest {
            at: Some(("a".to_string(), RIGHT_LINE)),
            picks: vec![2],
            ..PreviewRequest::default()
        });
        assert_eq!(result.outcome, Ok(()), "{:?}", result.outcome);
        assert_eq!(result.observation.line, Some(RIGHT_LINE));
        assert_eq!(
            result.observation.text.as_deref(),
            Some("the right line."),
            "{:?}",
            result.observation
        );
    }

    /// A choice past the end of the `--pick` list takes option 1.
    #[test]
    fn an_empty_pick_list_takes_the_first_option() {
        let (_dir, result) = take(&PreviewRequest {
            at: Some(("a".to_string(), LEFT_LINE)),
            ..PreviewRequest::default()
        });
        assert_eq!(result.outcome, Ok(()), "{:?}", result.outcome);
        assert_eq!(result.observation.line, Some(LEFT_LINE));
        assert_eq!(
            result.observation.text.as_deref(),
            Some("the left line."),
            "{:?}",
            result.observation
        );
    }

    /// The replay stops on the runner's wait-free cycle. It must report why in
    /// the preview's own finding shape, not return a silently shortened case.
    #[test]
    fn a_replay_that_closes_a_wait_free_cycle_reports_the_runner_error() {
        let (_dir, pack) = spinning_pack();
        let request = PreviewRequest {
            at: Some(("a".to_string(), 5)),
            ..PreviewRequest::default()
        };
        let error = build_case(&pack, &request).expect_err("the replay cannot reach line 5");
        assert_eq!(
            error.rule,
            unmei_engine::keiko::rules::DEFENSIVE,
            "{error:?}"
        );
        assert!(
            error
                .message
                .contains("closes a cycle that reaches no say-line"),
            "{error:?}"
        );
        assert!(
            error.message.contains("`jump top` in scene `a`"),
            "{error:?}"
        );
    }
}
