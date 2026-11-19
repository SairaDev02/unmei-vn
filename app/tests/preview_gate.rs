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

/// With no selector the preview reports the missing line or scene and writes
/// nothing. Ticket #99 replaces this error with the fixture-state slate.
#[test]
fn a_preview_with_no_selector_reports_the_missing_one() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let out = dir.path().join("out");
    let output = run(&[
        demo_pack().to_str().expect("a pack path"),
        "--preview",
        out.to_str().expect("a folder path"),
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
