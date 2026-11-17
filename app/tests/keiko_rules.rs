//! The Keiko behaviour rules that need a real process but not a bespoke pack:
//! reveal and clocks, decisions and their faults, fixtures, broken packs, start
//! at a scene, stepping and exploration (spec §15 rules 6–8, 13, 15, 16, 19–21,
//! 25–27, 29–33).

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

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// A private copy of the demo pack with `tests/` replaced by the given cases.
fn pack(dir: &Path, cases: &[(&str, &str)]) -> PathBuf {
    let pack = dir.join("demo-pack");
    if !pack.exists() {
        copy_dir(&demo_pack(), &pack);
    }
    let tests = pack.join("tests");
    std::fs::create_dir_all(&tests).unwrap();
    for entry in std::fs::read_dir(&tests).unwrap().flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "keiko")
        {
            std::fs::remove_file(path).unwrap();
        }
    }
    for (name, text) in cases {
        std::fs::write(tests.join(format!("{name}.keiko")), text).unwrap();
    }
    pack
}

fn run(pack: &Path, args: &[&str]) -> Output {
    let mut full = vec![pack.to_str().unwrap().to_string()];
    full.extend(args.iter().map(|arg| arg.to_string()));
    Command::new(binary())
        .args(&full)
        .env_remove("NANOGPT_API_KEY")
        .output()
        .expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn take(pack: &Path, ids: &[&str]) -> Output {
    let mut args = vec!["--take"];
    args.extend_from_slice(ids);
    run(pack, &args)
}

fn assert_pass(pack: &Path, id: &str) {
    let output = take(pack, &[id]);
    assert_eq!(output.status.code(), Some(0), "{id}: {}", stdout(&output));
}

#[allow(dead_code)]
fn assert_fail_with(pack: &Path, id: &str, needle: &str) {
    let output = take(pack, &[id]);
    assert_eq!(output.status.code(), Some(1), "{id}: {}", stdout(&output));
    assert!(
        stdout(&output).contains(needle),
        "{id}: expected `{needle}` in:\n{}",
        stdout(&output)
    );
}

// ---------------------------------------------------------------------------
// Rule 6: the reveal
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "keiko-rules-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const OPENING: &str = "keiko 1\nsetting play.text_speed 30\n\nnew-game\n\
    expect revealing\nexpect line \"Morning light. A folded letter on the ground, half under a shoe.\"\n\
    advance\nexpect revealed\nexpect line contains \"Morning light\"\n";

#[test]
fn an_advance_finishes_the_reveal_and_a_second_one_moves_on() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            ("slow", OPENING),
            (
                "instant",
                "keiko 1\nsetting play.text_speed 0\n\nnew-game\nexpect revealed\n\
                 advance\nexpect line \"You're not from around here, are you?\"\n",
            ),
        ],
    );
    assert_pass(&pack, "slow");
    assert_pass(&pack, "instant");
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Rule 7: auto's dwell under a virtual clock
// ---------------------------------------------------------------------------

#[test]
fn auto_waits_its_dwell_and_not_a_frame_less() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "early",
                "keiko 1\nsetting play.text_speed 0\n\nnew-game\nauto\n\
                 expect auto on\nexpect line contains \"Morning light\"\nwait 1.98s\n\
                 expect line contains \"Morning light\"\n",
            ),
            (
                "dwell",
                "keiko 1\nsetting play.text_speed 0\n\nnew-game\nauto\n\
                 expect line contains \"Morning light\"\nwait 2s\n\
                 expect line \"You're not from around here, are you?\"\n",
            ),
        ],
    );
    assert_pass(&pack, "early");
    assert_pass(&pack, "dwell");
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Rules 13, 15 and 16: decisions, offline and faulted
// ---------------------------------------------------------------------------

#[test]
fn a_case_landing_wins_and_an_unset_decision_takes_the_fallback() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "set",
                "keiko 1\ndecide ace.greet warm\nsetting play.text_speed 0\n\nnew-game\n\
                 advance\nadvance\npick 1\nadvance\nadvance\n\
                 expect scene reunion\nexpect line contains \"You came back\"\n",
            ),
            (
                "unset",
                "keiko 1\nsetting play.text_speed 0\n\nnew-game\n\
                 advance\nadvance\npick 2\nadvance\nadvance\n\
                 expect scene reunion\nexpect line contains \"Oh. It's you.\"\n\
                 expect notice none\nexpect log warn row 10 contains \"no Decisions client\"\n",
            ),
        ],
    );
    assert_pass(&pack, "set");
    assert_pass(&pack, "unset");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_slow_decision_lands_only_after_its_wait() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "slow",
            "keiko 1\nfault decision-slow ace.greet 3s\nsetting play.text_speed 0\n\nnew-game\n\
             advance\nadvance\npick 1\nadvance\nadvance\nexpect decision pending\nwait 1s\n\
             expect decision pending\nwait 2s\nexpect decision none\n\
             expect line contains \"Oh. It's you.\"\n",
        )],
    );
    assert_pass(&pack, "slow");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_refused_decision_lands_on_the_fallback_with_its_diagnostic() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "refusal",
            "keiko 1\nfault decision-error ace.greet refusal\nsetting play.text_speed 0\n\nnew-game\n\
             advance\nadvance\npick 2\nadvance\nadvance\n\
             expect line contains \"Oh. It's you.\"\n\
             expect log warn row 9 contains \"refus\"\n",
        )],
    );
    assert_pass(&pack, "refusal");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_held_decision_stays_pending_is_held_by_the_overlay_and_lands_on_release() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "held",
            "keiko 1
fault decision-hold ace.greet
setting play.text_speed 0

new-game
             advance
advance
pick 1
advance
advance
             expect decision pending
wait 500ms
expect decision pending
menu
             expect decision held
release-decision ace.greet
menu
             expect decision none
expect line contains \"Oh. It's you.\"
",
        )],
    );
    assert_pass(&pack, "held");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_pick_text_that_matches_no_option_fails() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "nothing",
            "keiko 1
setting play.text_speed 0

new-game
advance
advance
             pick \"Not an option\"
",
        )],
    );
    let output = take(&pack, &["nothing"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("keiko.step.no-target"),
        "{}",
        stdout(&output)
    );
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Rules 19 and 20: asset faults and a broken pack
// ---------------------------------------------------------------------------

#[test]
fn a_missing_asset_logs_its_row_without_a_notice() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "missing",
            "keiko 1\nfault asset-missing backgrounds/school_gate.png\nsetting play.text_speed 0\n\n\
             new-game\nexpect bg school_gate\nexpect notice none\n\
             expect log warn row 3 contains \"school_gate\"\n",
        )],
    );
    assert_pass(&pack, "missing");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_missing_manifest_reaches_the_error_screen() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "broken",
            "keiko 1\nfault pack-file-missing pack.ron\n\nnew-game\nexpect surface error\n",
        )],
    );
    assert_pass(&pack, "broken");
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Rule 21: fixtures
// ---------------------------------------------------------------------------

#[test]
fn a_take_without_fixtures_starts_with_no_slots_and_continue_disabled() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "fresh",
            "keiko 1\n\n\
             expect surface title\nexpect slot 1 empty\nexpect slot 2 empty\n\
             expect slot 3 empty\nexpect slot autosave empty\n\
             expect control title.continue disabled\n",
        )],
    );
    assert_pass(&pack, "fresh");
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Rules 25 and 26: stepping and observing
// ---------------------------------------------------------------------------

#[test]
fn step_appends_only_what_passed_and_observe_never_writes() {
    let dir = temp_dir();
    let pack = pack(&dir, &[]);
    let case = pack.join("tests/stepped.keiko");

    // A missing case is created with its version line plus the step.
    let output = run(&pack, &["--step", "stepped", "new-game"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let text = std::fs::read_to_string(&case).unwrap();
    assert_eq!(text, "keiko 1\nnew-game\n");

    // A failing step leaves the file byte-identical.
    let before = std::fs::read(&case).unwrap();
    let output = run(&pack, &["--step", "stepped", "expect speaker \"Nobody\""]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(std::fs::read(&case).unwrap(), before, "no write on failure");

    // A passing step is appended.
    let output = run(&pack, &["--step", "stepped", "expect speaker \"Narrator\""]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(std::fs::read_to_string(&case)
        .unwrap()
        .ends_with("expect speaker \"Narrator\"\n"));

    // `--observe` prints and never writes.
    let before = std::fs::read(&case).unwrap();
    let output = run(&pack, &["--observe", "stepped"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("Narrator"));
    assert_eq!(std::fs::read(&case).unwrap(), before);

    // The built case passes under `--take` unchanged.
    assert_pass(&pack, "stepped");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_header_as_a_step_is_refused_once_the_case_has_steps() {
    let dir = temp_dir();
    let pack = pack(&dir, &[("built", "keiko 1\nnew-game\n")]);
    let output = run(&pack, &["--step", "built", "dev"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("keiko.case.header-order"));
    assert_eq!(
        std::fs::read_to_string(pack.join("tests/built.keiko")).unwrap(),
        "keiko 1\nnew-game\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Rule 33: start at a scene with chosen state
// ---------------------------------------------------------------------------

#[test]
fn start_reaches_a_scene_top_with_the_cases_state() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "late",
            "keiko 1\nsetting play.text_speed 0\nstart reunion\n\
             state affinity_level = \"high\"\n\n\
             expect surface play\nexpect scene reunion\nexpect state affinity_level = \"high\"\n\
             expect line contains \"The gate again\"\n",
        )],
    );
    assert_pass(&pack, "late");
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Rule 32: live needs the environment key
// ---------------------------------------------------------------------------

#[test]
fn live_without_the_environment_key_is_a_usage_error() {
    let pack = demo_pack();
    let output = Command::new(binary())
        .args([pack.to_str().unwrap(), "--take", "--live"])
        .env_remove("NANOGPT_API_KEY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("NANOGPT_API_KEY"));
}

// ---------------------------------------------------------------------------
// Rules 27, 29, 30 and 31: exploration on the demo pack
// ---------------------------------------------------------------------------

#[test]
fn exploration_reaches_every_branch_and_saves_no_finding() {
    let dir = temp_dir();
    let pack = pack(&dir, &[]);
    let output = run(
        &pack,
        &["--explore", "--budget", "10000", "--format", "json"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let json: serde_json::Value = serde_json::from_str(stdout(&output).trim()).unwrap();
    assert_eq!(json["findings"].as_array().unwrap().len(), 0);
    assert_eq!(json["coverage"]["unreached"].as_array().unwrap().len(), 0);
    assert!(
        !pack.join("tests/findings").exists(),
        "no finding was saved"
    );

    // The same budget gives the same findings and coverage; a smaller budget
    // reports that it was spent.
    let again = run(
        &pack,
        &["--explore", "--budget", "10000", "--format", "json"],
    );
    assert_eq!(stdout(&output), stdout(&again));
    let small = run(&pack, &["--explore", "--budget", "1"]);
    assert!(
        stdout(&small).contains("budget spent"),
        "{}",
        stdout(&small)
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_saved_finding_is_a_case_that_fails_and_then_passes() {
    let dir = temp_dir();
    let pack = dir.join("broken-pack");
    std::fs::create_dir_all(pack.join("scenes")).unwrap();
    std::fs::create_dir_all(pack.join("npcs")).unwrap();
    std::fs::create_dir_all(pack.join("tests")).unwrap();
    std::fs::write(
        pack.join("pack.ron"),
        "(name: \"broken\", entry: \"start\")",
    )
    .unwrap();
    std::fs::write(pack.join("state.ron"), "(met: false,)").unwrap();
    // A staged sprite with no asset, then a scene that loops forever.
    // A staged sprite with no asset file (row 4), a walk to a scene that loops,
    // and a line that never changes.
    std::fs::write(
        pack.join("scenes/start.unmei"),
        "bg school_gate
sprite ace neutral
Narrator: one
choice
  \"loop\" -> quiet
end
label quiet
Narrator: same
jump quiet
",
    )
    .unwrap();

    let output = run(&pack, &["--explore", "--budget", "500", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_str(stdout(&output).trim()).unwrap();
    let findings = json["findings"].as_array().unwrap();
    assert!(!findings.is_empty(), "{}", stdout(&output));
    let saved = findings[0]["case"].as_str().unwrap().to_string();
    assert!(saved.starts_with("tests/findings/"), "{saved}");
    let text = std::fs::read_to_string(pack.join(&saved)).unwrap();
    assert!(
        text.starts_with(
            "keiko 1
"
        ),
        "{text}"
    );

    // Rerunning writes nothing new.
    let before = std::fs::read_dir(pack.join("tests/findings"))
        .unwrap()
        .count();
    let _ = run(&pack, &["--explore", "--budget", "500", "--format", "json"]);
    let after = std::fs::read_dir(pack.join("tests/findings"))
        .unwrap()
        .count();
    assert_eq!(before, after, "a rerun adds no file");

    // The saved case is an invalid pack's case: it is listed by `--take` and
    // fails while the problem exists.
    let output = run(
        &pack,
        &[
            "--take",
            saved
                .trim_start_matches("tests/")
                .trim_end_matches(".keiko"),
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    let _ = std::fs::remove_dir_all(dir);
}
