//! The preview gate: the built binary draws the demo pack on the software
//! rasterizer (check and preview spec §11, §13 rules 21-27).
//!
//! Every rendered take costs seconds, so the cases below assert what a frame is
//! — its name, its folder and its 1280×720 size — and never its pixels.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_unmei-play"))
}

fn demo_pack() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/demo-pack")
        .canonicalize()
        .expect("the demo pack exists")
}

/// A fixture pack that breaks one family of rules, shared with the check gate.
fn fixture_pack(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/check")
        .join(name)
        .canonicalize()
        .unwrap_or_else(|error| panic!("fixture pack `{name}` exists: {error}"))
}

/// A fixture pack for the slate's unreachable states.
fn slate_pack(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/preview")
        .join(name)
        .canonicalize()
        .unwrap_or_else(|error| panic!("fixture pack `{name}` exists: {error}"))
}

fn run(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env_remove("NANOGPT_API_KEY")
        .output()
        .expect("the binary runs")
}

/// Run with extra environment, so a test can take the fallback adapter away
/// with no product hook.
fn run_env(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(binary());
    command.args(args).env_remove("NANOGPT_API_KEY");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Every `.png` in a folder, by name, sorted.
fn written(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The frame's size, read back from the PNG header.
fn size(path: &Path) -> (u32, u32) {
    let image =
        image::open(path).unwrap_or_else(|error| panic!("{} decodes: {error}", path.display()));
    (image.width(), image.height())
}

// ---------------------------------------------------------------------------
// Spec §13 rule 1, preview part: one mode per invocation, its own flags, and a
// folder that is never inside the pack
// ---------------------------------------------------------------------------

#[test]
fn preview_usage_errors_exit_two_before_startup() {
    let pack = demo_pack();
    let pack = pack.to_str().expect("a pack path");
    let inside = demo_pack().join("frames");
    let inside = inside.to_str().expect("a folder path");
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let out = out.to_str().expect("a folder path");
    for args in [
        // Two modes together.
        vec![pack, "--check", "--preview", out, "--at", "reunion:7"],
        vec![pack, "--preview", out, "--at", "reunion:7", "--check"],
        vec![pack, "--preview", out, "--take"],
        vec![pack, "--take", "--preview", out],
        // `--dev` with the mode.
        vec![pack, "--dev", "--preview", out, "--at", "reunion:7"],
        // A preview-only flag with no preview.
        vec![pack, "--scene", "reunion"],
        vec![pack, "--at", "reunion:7"],
        vec![pack, "--state", "high.ron"],
        vec![pack, "--pick", "2"],
        // The selectors exclude each other.
        vec![
            pack,
            "--preview",
            out,
            "--scene",
            "reunion",
            "--at",
            "reunion:7",
        ],
        // The output folder may not lie inside the pack.
        vec![pack, "--preview", inside, "--at", "reunion:7"],
    ] {
        let output = run(&args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?}: {}",
            stderr(&output)
        );
    }
    assert!(
        !demo_pack().join("frames").exists(),
        "a refused folder is never created"
    );
}

// ---------------------------------------------------------------------------
// Spec §13 rule 24: `--preview` alone writes §9.2's fixture states, and a state
// a pack cannot reach is a warning that leaves the exit code at 0
// ---------------------------------------------------------------------------

/// With no selector the preview writes §9.2's fixture states without any help,
/// and a folder it is given is created when missing.
#[test]
fn the_preview_alone_writes_the_demo_packs_fixture_states() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let output = run(&[
        demo_pack().to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );
    // Every state this pack reaches is one 1280×720 PNG. The demo pack declares
    // no scene order, so the Timeline has no card and its one confirm is the
    // state it cannot reach (spec §9.2).
    let states: Vec<&str> = SLATE_STATES
        .iter()
        .copied()
        .filter(|name| *name != TIMELINE_CONFIRM)
        .collect();
    assert_eq!(written(&out), expected(&states));
    for name in states {
        assert_eq!(size(&out.join(name)), (1280, 720), "{name}");
    }
    let text = stdout(&output);
    assert!(text.contains("preview: 0 errors, 1 warnings"), "{text}");
    assert!(
        text.contains("preview: confirm-timeline-jump: "),
        "the warning names the state:\n{text}"
    );
    assert_eq!(
        text.matches("[preview.state.unreachable]").count(),
        1,
        "{text}"
    );
}

/// A pack that reaches only some of the states writes the rest, reports the
/// others as warnings naming them, and still exits 0 (spec §9.2, §13 rule 24).
#[test]
fn a_pack_with_only_unreachable_states_exits_zero() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let output = run(&[
        slate_pack("sparse").to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );
    const SPARSE_STATES: [&str; 18] = [
        "title-fresh.png",
        "title-continue.png",
        "narration.png",
        "scene-end.png",
        "menu-save.png",
        "menu-load.png",
        "menu-settings.png",
        "menu-gallery.png",
        "menu-timeline.png",
        "confirm-overwrite.png",
        "confirm-load.png",
        "confirm-reset-audio.png",
        "confirm-reset-play.png",
        "confirm-new-game.png",
        "confirm-quit.png",
        "confirm-exit-to-title.png",
        "confirm-clear-key.png",
        "confirm-timeline-jump.png",
    ];
    assert_eq!(written(&out), expected(&SPARSE_STATES));
    let text = stdout(&output);
    assert!(text.contains("preview: 0 errors, 4 warnings"), "{text}");
    for state in ["say", "choice", "decision", "still"] {
        assert!(
            text.contains(&format!("preview: {state}: ")),
            "the warning names `{state}`:\n{text}"
        );
        assert!(!out.join(format!("{state}.png")).exists(), "{state}");
    }
    assert_eq!(
        text.matches("[preview.state.unreachable]").count(),
        4,
        "{text}"
    );
}

/// The slate's states in §9.2's order, as the file names they write.
const SLATE_STATES: [&str; 22] = [
    "title-fresh.png",
    "title-continue.png",
    "say.png",
    "narration.png",
    "choice.png",
    "decision.png",
    "scene-end.png",
    "menu-save.png",
    "menu-load.png",
    "menu-settings.png",
    "menu-gallery.png",
    "menu-timeline.png",
    "still.png",
    "confirm-overwrite.png",
    "confirm-load.png",
    "confirm-reset-audio.png",
    "confirm-reset-play.png",
    "confirm-new-game.png",
    "confirm-quit.png",
    "confirm-exit-to-title.png",
    "confirm-clear-key.png",
    "confirm-timeline-jump.png",
];

/// The one state the demo pack cannot reach: the Timeline confirm needs a
/// declared scene order and the demo pack has none.
const TIMELINE_CONFIRM: &str = "confirm-timeline-jump.png";

/// A list of names in the order [`written`] returns them.
fn expected(names: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    names.sort();
    names
}

// ---------------------------------------------------------------------------
// Rule 21: `--at` writes the one frame its line's park shows
// ---------------------------------------------------------------------------

/// `reunion:19` is the say-line the authored fallback lands on, so the frame
/// also proves the replay walked past the decide point at line 10.
#[test]
fn an_at_line_writes_one_1280_by_720_frame_named_for_the_line() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let output = run(&[
        demo_pack().to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
        "--at",
        "reunion:19",
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(written(&out), ["at-reunion-19.png"]);
    assert_eq!(size(&out.join("at-reunion-19.png")), (1280, 720));
}

#[test]
fn an_at_line_that_is_no_park_reports_the_reference_rule_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let output = run(&[
        demo_pack().to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
        "--at",
        "reunion:9",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("keiko.case.reference"),
        "{}",
        stdout(&output)
    );
    assert!(written(&out).is_empty(), "no frame is written");
}

// ---------------------------------------------------------------------------
// Rule 22: `--scene` writes one frame per park, and stops at the scene's edge
// ---------------------------------------------------------------------------

/// `first_meeting` parks twice on say-lines, once on its choice and once on the
/// first option's say-line; the jump to `reunion` ends the walk.
#[test]
fn a_scene_writes_one_frame_at_every_park_it_meets() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let output = run(&[
        demo_pack().to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
        "--scene",
        "first_meeting",
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );
    let names = [
        "scene-first_meeting-7.png",
        "scene-first_meeting-10.png",
        "scene-first_meeting-12.png",
        "scene-first_meeting-21.png",
    ];
    let mut expected: Vec<&str> = names.to_vec();
    expected.sort_unstable();
    assert_eq!(
        written(&out),
        expected,
        "one frame per park, no scene cross"
    );
    for name in names {
        assert_eq!(size(&out.join(name)), (1280, 720), "{name}");
    }
}

/// `reunion` parks on a say-line, on its decide point and on the say-line the
/// authored fallback lands, so a decision park is framed like any other.
#[test]
fn a_scene_frames_the_decide_park_and_the_fallback_it_lands_on() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let output = run(&[
        demo_pack().to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
        "--scene",
        "reunion",
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(
        written(&out),
        [
            "scene-reunion-10.png",
            "scene-reunion-19.png",
            "scene-reunion-7.png",
        ]
    );
}

// ---------------------------------------------------------------------------
// Rule 23: `--pick` and `--state` choose the branch the replay walks
// ---------------------------------------------------------------------------

/// `first_meeting:28` is the second option's say-line, so the frame proves the
/// pick reached that branch, and the default picks cannot reach it at all.
#[test]
fn a_pick_reaches_the_second_options_branch() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let pack = demo_pack();
    let pack = pack.to_str().expect("a pack path");
    let out_arg = out.to_str().expect("a folder path");

    let missed = run(&[pack, "--preview", out_arg, "--at", "first_meeting:28"]);
    assert_eq!(missed.status.code(), Some(1), "{}", stdout(&missed));
    assert!(
        stdout(&missed).contains("keiko.case.reference"),
        "{}",
        stdout(&missed)
    );
    assert!(written(&out).is_empty(), "no frame is written");

    let picked = run(&[
        pack,
        "--preview",
        out_arg,
        "--at",
        "first_meeting:28",
        "--pick",
        "2",
    ]);
    assert_eq!(
        picked.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&picked),
        stderr(&picked)
    );
    assert_eq!(written(&out), ["at-first_meeting-28.png"]);
}

/// `epilogue:10` is the say-line in the `affinity_level == "high"` arm, which
/// the pack's own initial state skips (spec §13 rule 23).
#[test]
fn a_state_file_reaches_the_if_arm_the_initial_state_skips() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let state = dir.path().join("high.ron");
    std::fs::write(&state, "(affinity_level: \"high\",)").expect("the state file");
    let pack = demo_pack();
    let args = [
        pack.to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
        "--at",
        "epilogue:10",
        "--state",
        state.to_str().expect("a state path"),
    ];

    let skipped = run(&args[..5]);
    assert_eq!(skipped.status.code(), Some(1), "{}", stdout(&skipped));
    assert!(
        stdout(&skipped).contains("keiko.case.reference"),
        "{}",
        stdout(&skipped)
    );
    assert!(written(&out).is_empty(), "no frame is written");

    let reached = run(&args);
    assert_eq!(
        reached.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&reached),
        stderr(&reached)
    );
    assert_eq!(written(&out), ["at-epilogue-10.png"]);
}

// ---------------------------------------------------------------------------
// Rule 1's `--format` half: the preview reports its finding in the check's
// JSON shape too
// ---------------------------------------------------------------------------

#[test]
fn a_preview_finding_has_the_checks_json_shape() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let output = run(&[
        demo_pack().to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
        "--at",
        "reunion:9",
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout(&output).trim()).expect("one JSON document");
    assert_eq!(document["errors"], 1, "{document}");
    assert_eq!(document["findings"][0]["rule"], "keiko.case.reference");
    assert_eq!(document["findings"][0]["severity"], "error");
    assert!(written(&out).is_empty(), "no frame is written");
}

// ---------------------------------------------------------------------------
// Rule 25: a pack that fails to load writes no frame
// ---------------------------------------------------------------------------

#[test]
fn a_pack_that_fails_to_load_reports_its_findings_and_writes_no_frame() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let pack = fixture_pack("unloaded");
    let output = run(&[
        pack.to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
        "--at",
        "main:1",
    ]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    let text = stdout(&output);
    assert!(text.contains("pack.manifest"), "{text}");
    assert!(
        !text.contains("keiko.case.version"),
        "the preview runs no part of the check: {text}"
    );
    assert!(written(&out).is_empty(), "no frame is written");
}

// ---------------------------------------------------------------------------
// Rule 26: no fallback adapter fails the preview before any frame
// ---------------------------------------------------------------------------

#[test]
fn without_a_fallback_adapter_the_preview_fails_before_any_frame() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    // `metal` is not a backend this platform compiles, so the probe finds no
    // fallback adapter.
    let output = run_env(
        &[
            demo_pack().to_str().expect("a pack path"),
            "--preview",
            out.to_str().expect("a folder path"),
            "--at",
            "reunion:19",
        ],
        &[("WGPU_BACKEND", "metal")],
    );
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("keiko.render.unavailable"),
        "{}",
        stdout(&output)
    );
    assert!(written(&out).is_empty(), "no frame is written");
}

// ---------------------------------------------------------------------------
// Rule 27: a rerun overwrites its own files and leaves the rest alone
// ---------------------------------------------------------------------------

#[test]
fn a_rerun_overwrites_its_files_and_leaves_another_file_alone() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).expect("the folder");
    std::fs::write(out.join("keep.txt"), "not a frame").expect("the other file");
    let pack = demo_pack();
    let args = [
        pack.to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
        "--at",
        "reunion:7",
    ];

    let first = run(&args);
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    let frame = out.join("at-reunion-7.png");
    assert_eq!(size(&frame), (1280, 720));

    // A rerun replaces the frame it names, whatever is there now.
    std::fs::write(&frame, b"not a png any more").expect("the corrupted frame");
    let second = run(&args);
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    assert_eq!(size(&frame), (1280, 720), "the frame is written again");
    assert_eq!(
        std::fs::read_to_string(out.join("keep.txt")).expect("the other file"),
        "not a frame",
        "every other file in the folder is left alone"
    );
    assert_eq!(written(&out), ["at-reunion-7.png"]);
}

/// A preview writes the frames it is asked for and nothing else: the pack's
/// folder gains no frames folder and no session log, and the user's credential
/// folder is untouched (spec §1).
#[test]
fn a_preview_writes_only_the_frames_it_is_asked_for() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let canary = dir.path().join("config");
    std::fs::create_dir_all(canary.join("unmei")).expect("the canary folder");
    let key = canary.join("unmei").join("credential.ron");
    std::fs::write(&key, "(api_key: \"canary\")").expect("the canary key");
    let out = dir.path().join("out");
    let canary_path = canary.to_str().expect("a canary path");
    let output = run_env(
        &[
            demo_pack().to_str().expect("a pack path"),
            "--preview",
            out.to_str().expect("a folder path"),
            "--at",
            "reunion:7",
        ],
        &[
            ("APPDATA", canary_path),
            ("XDG_CONFIG_HOME", canary_path),
            ("HOME", canary_path),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(written(&out), ["at-reunion-7.png"]);
    assert_eq!(
        std::fs::read_to_string(&key).expect("the canary key"),
        "(api_key: \"canary\")",
        "the user's credential file is untouched"
    );
    let parent = demo_pack()
        .parent()
        .expect("the pack's folder")
        .to_path_buf();
    assert!(
        !parent.join("demo-pack.keiko").exists(),
        "no frames folder beside the pack"
    );
    assert!(
        !parent.join("demo-pack.log").exists(),
        "no session log beside the pack"
    );
}
