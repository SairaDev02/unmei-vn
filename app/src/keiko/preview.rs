//! The shared replay: turn the preview's request into cases (spec §9.1, §9.2,
//! §12).
//!
//! A selector — `--scene` or `--at` — maps onto `start`/`state` headers, the
//! `advance`/`pick` steps that reach its parks, and one `frame` step per park.
//! Without a selector the request covers §9.2's fixture states instead, one
//! generated case each. The preview runs every generated case through the same
//! take engine a test uses, so there is one replay and it cannot drift.

mod run;
mod states;

pub use run::run;

use unmei_engine::keiko::{parse_case, Case, Finding, Location};
use unmei_engine::runner::Wait;
use unmei_engine::{Pack, ScriptRunner, StateValue};

/// The preview's request, after its own argument parsing.
#[derive(Debug, Clone, Default)]
pub struct PreviewRequest {
    /// The scene to start at.
    pub scene: Option<String>,
    /// `(scene, line)` to reach and frame.
    pub at: Option<(String, u32)>,
    /// `--state` overrides, merged over the pack's initial state.
    pub state: Vec<(String, StateValue)>,
    /// The 1-based option to take at each successive choice.
    pub picks: Vec<usize>,
}

/// Which of a scene's parks the request frames (spec §9.1).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Parks {
    /// The one park this line produces.
    One(u32),
    /// Every park the scene meets until the replay leaves it.
    All,
}

impl Parks {
    /// The line a request names, when it names one.
    fn target(self) -> Option<u32> {
        match self {
            Parks::One(line) => Some(line),
            Parks::All => None,
        }
    }

    /// The frame name's prefix: `at-<scene>-<line>` or `scene-<id>-<line>`.
    fn prefix(self) -> &'static str {
        match self {
            Parks::One(_) => "at",
            Parks::All => "scene",
        }
    }
}

/// What one preview request covers.
pub enum Plan {
    /// The one case a selector builds (spec §9.1).
    One(Case),
    /// Every §9.2 fixture state with its own case, when no selector is named.
    Slate(Vec<states::Fixture>),
}

/// Plan a request: a selector builds one case, and its absence asks for
/// §9.2's fixture states (spec §9.1, §9.2).
pub fn plan(pack: &Pack, request: &PreviewRequest) -> Result<Plan, Finding> {
    if request.scene.is_none() && request.at.is_none() {
        return Ok(Plan::Slate(states::slate(pack, request)));
    }
    Ok(Plan::One(build_case(pack, request)?))
}

/// Turn a request that names one line or scene into an ordinary case.
pub fn build_case(pack: &Pack, request: &PreviewRequest) -> Result<Case, Finding> {
    let scene = request
        .at
        .as_ref()
        .map(|(scene, _)| scene.clone())
        .or_else(|| request.scene.clone())
        .ok_or_else(|| reference("the preview needs --scene or --at"))?;
    if !pack.scenes.contains_key(&scene) {
        return Err(reference(format!("unknown scene `{scene}`")));
    }
    let parks = match &request.at {
        Some((_, line)) => Parks::One(*line),
        None => Parks::All,
    };
    let walk = walk(pack, &scene, parks, request)?;

    let mut text = String::from("keiko 1\n");
    for point in &walk.holds {
        text.push_str(&format!("fault decision-hold {point}\n"));
    }
    text.push_str(&format!("start {scene}\n"));
    for (key, value) in &request.state {
        text.push_str(&format!("state {key} = {}\n", state_text(value)));
    }
    for step in &walk.steps {
        text.push_str(step);
        text.push('\n');
    }
    parse_case("preview", &text)
}

/// The steps that reach a request's parks, and the decision holds its frames
/// need.
#[derive(Default)]
struct Walk {
    steps: Vec<String>,
    holds: Vec<String>,
}

/// Walk the scene from its top, framing each park the request covers, and stop
/// where it leaves the scene or the script ends.
fn walk(pack: &Pack, scene: &str, parks: Parks, request: &PreviewRequest) -> Result<Walk, Finding> {
    let mut runner = ScriptRunner::new(pack.clone());
    let mut state = pack.state.clone();
    // The overrides move the control flow the replay walks, so the generated
    // steps describe the path the take will really take (spec §13 rule 23).
    for (key, value) in &request.state {
        state.insert(key.clone(), value.clone());
    }
    runner
        .restore_position(scene.to_string(), 0, state, None)
        .map_err(|error| reference(format!("scene `{scene}` could not be started: {error}")))?;

    let mut walk = Walk::default();
    let mut picks = request.picks.iter().copied();
    let mut framed = false;
    let mut parks_met = 0usize;
    for _ in 0..MAX_PARKS {
        // A cycle with no say-line, choice or decide stops the player with a
        // defensive failure (row 13). The replay must report why it stopped
        // rather than hand back a case that quietly ends short of the line.
        let outcome = runner.run().map_err(|error| {
            let stopped = match parks.target() {
                Some(line) => format!("the replay stopped before line {line}: {error}"),
                None => format!("the replay stopped: {error}"),
            };
            Finding::new(
                "preview",
                Location::File,
                unmei_engine::keiko::rules::DEFENSIVE,
                stopped,
            )
        })?;
        let Some(wait) = outcome.wait else {
            break;
        };
        // A jump into another scene ends this scene's parks (spec §9.1).
        if runner.scene_id() != scene {
            break;
        }
        let Some(line) = park_line(&runner) else {
            break;
        };
        parks_met += 1;
        let frame = parks.target().is_none_or(|target| target == line);
        if frame {
            walk.steps.push(format!(
                "frame {}-{}-{line}",
                parks.prefix(),
                super::render::sanitize(scene)
            ));
            if parks.target() == Some(line) {
                framed = true;
                break;
            }
        }
        // Move past the park, taking the default picks and the author's
        // decision fallbacks (spec §9.1).
        match wait {
            Wait::Say { .. } => {
                walk.steps.push("advance".to_string());
                if runner.advance_say().is_err() {
                    break;
                }
            }
            Wait::Choice { options } => {
                let index = picks.next().unwrap_or(1).clamp(1, options.len().max(1));
                walk.steps.push(format!("pick {index}"));
                if runner.choose(index - 1).is_err() {
                    break;
                }
            }
            Wait::Decision {
                npc_id, question, ..
            } => {
                // A held decision is what makes a parked decision observable, so
                // a framed one declares the fault and releases it to move on.
                if frame {
                    let point = format!("{npc_id}.{question}");
                    if !walk.holds.contains(&point) {
                        walk.holds.push(point.clone());
                    }
                    walk.steps.push(format!("release-decision {point}"));
                }
                let landing = pack
                    .npcs
                    .get(&npc_id)
                    .and_then(|npc| npc.decisions.get(&question))
                    .map(|config| config.fallback.landing_key());
                let Some(landing) = landing else {
                    break;
                };
                if runner.resolve_decision(&landing).is_err() {
                    break;
                }
            }
        }
    }
    if let Some(line) = parks.target() {
        if !framed {
            return Err(reference(format!(
                "the replay reaches no park at line {line} of scene `{scene}`"
            )));
        }
    }
    // A scene that never leaves is an authoring defect, and a case holding
    // 10 000 frames is not something a preview can render.
    if parks_met == MAX_PARKS {
        return Err(Finding::new(
            "preview",
            Location::File,
            unmei_engine::keiko::rules::DEFENSIVE,
            format!("scene `{scene}` did not end within {MAX_PARKS} parks"),
        ));
    }
    Ok(walk)
}

/// How many parks a walk may meet before the scene counts as never ending.
const MAX_PARKS: usize = 10_000;

fn reference(message: impl Into<String>) -> Finding {
    Finding::new(
        "preview",
        Location::File,
        unmei_engine::keiko::rules::CASE_REFERENCE,
        message,
    )
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

/// A `state` override parsed into the map's form.
pub fn parse_state(value: &str) -> Option<StateValue> {
    unmei_engine::keiko::parse_state_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keiko::take::{self, TakeRequest};
    use unmei_engine::keiko::Step;

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

    /// The steps the replay generates settle at the line the request names. The
    /// `frame` step is dropped so this stays a unit test with no rasterizer; the
    /// binary gate proves the frame itself.
    fn settled_line(pack: &Pack, root: &std::path::Path, request: &PreviewRequest) -> u32 {
        let mut case = build_case(pack, request).expect("the replay reaches the line");
        case.steps
            .retain(|line| !matches!(line.step, Step::Frame(_)));
        let result = take::run(TakeRequest {
            case: &case,
            case_file: "preview".to_string(),
            original_pack: root,
            pack,
            take_index: 1,
            keep: false,
            live: None,
            frames: None,
        });
        assert_eq!(result.outcome, Ok(()), "{:?}", result.outcome);
        result.observation.line.expect("a parked line")
    }

    /// `--pick` decides which option the replay takes, and a line the picks
    /// cannot reach is a reference finding (spec §9.1, §13 rule 23).
    #[test]
    fn the_picks_select_the_branch_the_replay_takes() {
        let (_dir, root, pack) = choice_pack();
        let second = PreviewRequest {
            at: Some(("a".to_string(), RIGHT_LINE)),
            picks: vec![2],
            ..PreviewRequest::default()
        };
        assert_eq!(settled_line(&pack, &root, &second), RIGHT_LINE);

        let first = PreviewRequest {
            at: Some(("a".to_string(), LEFT_LINE)),
            ..PreviewRequest::default()
        };
        assert_eq!(settled_line(&pack, &root, &first), LEFT_LINE);

        let unreachable = PreviewRequest {
            at: Some(("a".to_string(), LEFT_LINE)),
            picks: vec![2],
            ..PreviewRequest::default()
        };
        let error = build_case(&pack, &unreachable).expect_err("the pick cannot reach it");
        assert_eq!(
            error.rule,
            unmei_engine::keiko::rules::CASE_REFERENCE,
            "{error:?}"
        );
        assert!(
            error.message.contains("reaches no park at line 7"),
            "{error:?}"
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

    /// A scene whose say-line loops back to itself never leaves the scene, so
    /// the walk stops with the defensive rule rather than hand back a case
    /// holding ten thousand frames.
    #[test]
    fn a_scene_that_never_ends_reports_the_defensive_rule() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("loop");
        std::fs::create_dir_all(root.join("scenes")).unwrap();
        std::fs::create_dir_all(root.join("npcs")).unwrap();
        std::fs::write(root.join("pack.ron"), "(name: \"loop\", entry: \"a\")").unwrap();
        std::fs::write(root.join("state.ron"), "(met: false,)").unwrap();
        std::fs::write(
            root.join("scenes/a.unmei"),
            "label top\nNarrator: again.\njump top\n",
        )
        .unwrap();
        let loaded = unmei_engine::load_pack(&root).expect("the pack loads");
        let request = PreviewRequest {
            scene: Some("a".to_string()),
            ..PreviewRequest::default()
        };
        let error = build_case(&loaded.pack, &request).expect_err("the scene never ends");
        assert_eq!(
            error.rule,
            unmei_engine::keiko::rules::DEFENSIVE,
            "{error:?}"
        );
        assert!(
            error.message.contains("did not end within 10000 parks"),
            "{error:?}"
        );
    }
}
