//! The smoke checklist's fixture packs, run as Keiko takes on every push
//! (spec §14, rules 34 and 29–31 of §15): the built binary takes each fixture
//! pack under `app/tests/keiko/`, and every case in it passes.
//!
//! A fixture pack is a real pack — manifest, state, NPCs, scenes, the few demo
//! assets its cases stage, and its own `tests/` folder of cases. Because the
//! takes run on the headless player, nothing here needs a GPU, an audio device
//! or a physical keyboard; what each case leaves out is named in the case's own
//! comment.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_unmei-play"))
}

/// `app/tests/keiko/<name>`, canonical so the child sees an absolute pack.
fn fixture_pack(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/keiko")
        .join(name)
        .canonicalize()
        .unwrap_or_else(|error| panic!("fixture pack `{name}` exists: {error}"))
}

fn run(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .output()
        .expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Run every case under one fixture pack and require a zero exit and a PASS line
/// for each expected case id.
fn take_pack(name: &str, cases: &[&str]) {
    let pack = fixture_pack(name);
    let output = run(&[pack.to_str().unwrap(), "--take"]);
    let text = stdout(&output);
    assert_eq!(
        output.status.code(),
        Some(0),
        "fixture pack `{name}`: stdout:\n{}\nstderr:\n{}",
        text,
        stderr(&output)
    );
    for case in cases {
        assert!(
            text.contains(&format!("PASS {case}")),
            "fixture pack `{name}`: `{case}` did not pass:\n{text}"
        );
    }
}

/// Recipe D: a clean content-only copy of the demo pack. The title, offline
/// decision and comfort cases play its story; the live-session, save, credential
/// and dev cases keep their own fixtures under its `tests/fixtures/`.
#[test]
fn the_clean_demo_fixture_packs_cases_pass() {
    take_pack(
        "d",
        &[
            "t1-title",
            "p1-choice-fallback",
            "t2-exit-to-title",
            "t3-autosave-beats-manual-slot",
            "t3b-unusable-autosave",
            "t4-new-game-keeps-saves",
            "s1-manual-save-and-load",
            "s1b-relaunch-loads-a-manual-slot",
            "s2-legacy-import",
            "s3-blocked-save-recovery",
            "k1-no-key-config",
            "k2a-stored-key",
            "k2b-env-override",
            "k3-offline-blocking",
            "k3b-checking-holds",
            "k3c-clear-key",
            "k4-unreadable-credential",
            "k4b-blocked-credential-write",
            "m1-normal-mode-shortcuts-inert",
            "m1-dev-shortcuts",
            "m2-dev-input-ownership",
            "a1-held-decision",
            "a1-slow-decision",
        ],
    );
}

/// Recipe G: the clean demo copy with one sprite file removed and another made
/// undecodable, so the gallery's failed art is a take's subject.
#[test]
fn the_failed_gallery_art_fixture_packs_cases_pass() {
    take_pack("g", &["g2-failed-gallery-art", "g3-title-door-walk"]);
}

/// Pack E: no art and no declared scene order, so the Gallery from both doors
/// and the Timeline are empty (spec §14, slice 3).
#[test]
fn the_empty_menus_fixture_packs_cases_pass() {
    take_pack(
        "e",
        &[
            "e1-empty-gallery-title-door",
            "e2-empty-gallery-overlay-door",
            "e3-empty-timeline",
        ],
    );
}

/// Recipe B: a loadable copy whose case removes the manifest from the take's
/// own sandbox, so boot reaches the error screen before the title.
#[test]
fn the_broken_pack_fixture_packs_cases_pass() {
    take_pack("b", &["b1-broken-pack"]);
}

/// Recipe L: the demo copy with the smoke lane's three declared scenes. The
/// read-log, skip and Timeline cases play those.
#[test]
fn the_reading_and_timeline_fixture_packs_cases_pass() {
    take_pack(
        "l",
        &[
            "r1-seen-skip",
            "r2-force-skip",
            "r3-load-resets-skip",
            "l1-timeline",
            "l2-timeline-state",
            "l3-timeline-walk",
        ],
    );
}

/// Determinism (spec §15 rule 5) over a fixture pack: the same case run twice
/// prints byte-identical JSON. A seeded read log and the virtual clock make this
/// hold without a real clock.
#[test]
fn a_fixture_packs_case_is_byte_identical_twice() {
    let pack = fixture_pack("l");
    let first = run(&[
        pack.to_str().unwrap(),
        "--take",
        "r1-seen-skip",
        "--format",
        "json",
    ]);
    let second = run(&[
        pack.to_str().unwrap(),
        "--take",
        "r1-seen-skip",
        "--format",
        "json",
    ]);
    assert_eq!(first.status.code(), Some(0), "{}", stdout(&first));
    assert_eq!(second.status.code(), Some(0), "{}", stdout(&second));
    assert_eq!(stdout(&first), stdout(&second));
}
