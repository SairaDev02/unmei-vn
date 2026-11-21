//! The Keiko behaviour rules that need a real process but not a bespoke pack:
//! reveal and clocks, decisions and their faults, fixtures, broken packs, start
//! at a scene, stepping and exploration (spec §15 rules 6–8, 13, 15, 16, 19–21,
//! 25–27, 29–33).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

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

/// Run the binary and fail the test if it does not finish inside `limit`, so a
/// walk that hangs is a test failure rather than a stuck job.
fn run_with_timeout(pack: &Path, args: &[&str], limit: Duration) -> Output {
    let mut full = vec![pack.to_str().unwrap().to_string()];
    full.extend(args.iter().map(|arg| arg.to_string()));
    let mut child = Command::new(binary())
        .args(&full)
        .env_remove("NANOGPT_API_KEY")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the binary starts");
    let started = Instant::now();
    loop {
        if child.try_wait().expect("the child is pollable").is_some() {
            return child.wait_with_output().expect("the output is captured");
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let output = child.wait_with_output().expect("the output is captured");
            panic!(
                "the run did not finish within {limit:?}:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
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

// Rule 6: the reveal

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

// Rule 7: auto's dwell under a virtual clock

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
                // The dwell starts on the frame after the line is whole, so the
                // boundary is two seconds plus that one frame.
                "keiko 1\nsetting play.text_speed 0\n\nnew-game\nauto\n\
                 expect line contains \"Morning light\"\nwait 2.04s\n\
                 expect line \"You're not from around here, are you?\"\n",
            ),
        ],
    );
    assert_pass(&pack, "early");
    assert_pass(&pack, "dwell");
    let _ = std::fs::remove_dir_all(dir);
}

// Rule 8: the Notice's lifetime under a virtual clock

/// A Notice lives `NOTICE_SECONDS` (5 s) of unpaused time. Steps that move no
/// time leave it up; `wait` is what spends it. The menu pauses play, so the
/// case closes the menu before it waits.
#[test]
fn a_notice_expires_under_wait_and_not_without_it() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "notice",
            "keiko 1\nfault write-blocked slot-1\nsetting play.text_speed 0\n\n\
             new-game\nsave 1\nexpect notice contains \"couldn't save\"\n\
             menu\nexpect surface play\nadvance\n\
             expect notice contains \"couldn't save\"\n\
             wait 4.9s\nexpect notice contains \"couldn't save\"\n\
             wait 200ms\nexpect notice none\n",
        )],
    );
    assert_pass(&pack, "notice");
    let _ = std::fs::remove_dir_all(dir);
}

// Rules 13, 15 and 16: decisions, offline and faulted

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

// Rules 19 and 20: asset faults and a broken pack

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
            // A pack that fails to load boots to the error surface, so the
            // title's controls are not there for a step to reach.
            "keiko 1\nfault pack-file-missing pack.ron\n\nexpect surface error\n",
        )],
    );
    assert_pass(&pack, "broken");
    let _ = std::fs::remove_dir_all(dir);
}

// Rule 21: fixtures

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

// Rules 25 and 26: stepping and observing

#[test]
fn step_appends_only_what_passed_and_observe_never_writes() {
    let dir = temp_dir();
    let pack = pack(&dir, &[]);
    let case = pack.join("tests/stepped.keiko");

    let output = run(&pack, &["--step", "stepped", "new-game"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let text = std::fs::read_to_string(&case).unwrap();
    assert_eq!(text, "keiko 1\nnew-game\n");

    let before = std::fs::read(&case).unwrap();
    let output = run(&pack, &["--step", "stepped", "expect speaker \"Nobody\""]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(std::fs::read(&case).unwrap(), before, "no write on failure");

    let output = run(&pack, &["--step", "stepped", "expect speaker \"Narrator\""]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(std::fs::read_to_string(&case)
        .unwrap()
        .ends_with("expect speaker \"Narrator\"\n"));

    let before = std::fs::read(&case).unwrap();
    let output = run(&pack, &["--observe", "stepped"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("Narrator"));
    assert_eq!(std::fs::read(&case).unwrap(), before);

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

// Rule 33: start at a scene with chosen state

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

// Rule 32: live needs the environment key

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

// Rules 27, 29, 30 and 31: exploration on the demo pack

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

/// Rule 28: one problem is one finding, however many branches replay it, and the
/// line a broken pack never reaches is coverage, not a finding. The saved row-4
/// case then fails on the broken fixture and passes once it is repaired (rule
/// 29's repair half).
#[test]
fn exploration_reports_one_problem_once_and_the_saved_case_repairs() {
    let dir = temp_dir();
    let pack = pack(&dir, &[]);
    // The fixture: a sprite file deleted, `epilogue` ending in a jump to a
    // looping scene with an unreachable say-line after it, and that scene.
    std::fs::remove_file(pack.join("assets/sprites/ace/neutral.png")).unwrap();
    let epilogue = std::fs::read_to_string(pack.join("scenes/epilogue.unmei")).unwrap();
    std::fs::write(
        pack.join("scenes/epilogue.unmei"),
        format!(
            "{epilogue}
jump spin

label orphan
Narrator: unreachable.
"
        ),
    )
    .unwrap();
    std::fs::write(
        pack.join("scenes/spin.unmei"),
        "Narrator: spin start.
label round
Narrator: again.
jump round
",
    )
    .unwrap();

    let explore = |pack: &Path| {
        let output = run_with_timeout(
            pack,
            &["--explore", "--budget", "10000", "--format", "json"],
            Duration::from_secs(120),
        );
        // Findings fail the command (spec §10.3), so this walk exits 1.
        assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
        let json: serde_json::Value = serde_json::from_str(stdout(&output).trim()).unwrap();
        json
    };
    let json = explore(&pack);
    let findings = json["findings"].as_array().unwrap();
    let count = |rule: &str| {
        findings
            .iter()
            .filter(|finding| finding["rule"] == rule)
            .count()
    };
    assert_eq!(
        count("keiko.log.row-4"),
        1,
        "one row-4 finding:
{json}"
    );
    assert_eq!(
        count("keiko.stuck"),
        1,
        "one stuck finding:
{json}"
    );

    // The orphan say-line is unreached coverage. Moving it above the jump makes
    // the walk read it, which is what an unreached count is measuring.
    let say_of = |json: &serde_json::Value| {
        let say = &json["coverage"]["scenes"]["epilogue"]["say_lines"];
        (
            say["reached"].as_u64().unwrap(),
            say["total"].as_u64().unwrap(),
        )
    };
    let (reached, total) = say_of(&json);
    assert!(
        reached < total,
        "the orphan line is not counted as reached:
{json}"
    );
    let reachable = dir.join("reachable");
    copy_dir(&pack, &reachable);
    std::fs::write(
        reachable.join("scenes/epilogue.unmei"),
        format!(
            "{epilogue}
label orphan
Narrator: unreachable.
jump spin
"
        ),
    )
    .unwrap();
    let (reached_when_readable, total_when_readable) = say_of(&explore(&reachable));
    assert_eq!(total_when_readable, total, "the same say-lines");
    assert_eq!(
        reached_when_readable,
        reached + 1,
        "the line the jump skips is the one that counts as unreached"
    );

    let findings_dir = pack.join("tests/findings");
    let before = std::fs::read_dir(&findings_dir).unwrap().count();
    let _ = explore(&pack);
    let after = std::fs::read_dir(&findings_dir).unwrap().count();
    assert_eq!(before, after, "a rerun adds no file");

    // The saved row-4 case is a case that fails while the sprite is missing,
    // naming the log line, and passes once the sprite is back.
    let saved = findings
        .iter()
        .find(|finding| finding["rule"] == "keiko.log.row-4")
        .and_then(|finding| finding["case"].as_str())
        .expect("the row-4 finding is saved")
        .to_string();
    let id = saved
        .trim_start_matches("tests/")
        .trim_end_matches(".keiko")
        .to_string();
    let output = take(&pack, &[id.as_str()]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("expected no warn row 4 line"),
        "{}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("observed warn row 4"),
        "the failing assertion names the log line:
{}",
        stdout(&output)
    );
    std::fs::copy(
        demo_pack().join("assets/sprites/ace/neutral.png"),
        pack.join("assets/sprites/ace/neutral.png"),
    )
    .unwrap();
    assert_pass(&pack, &id);
    let _ = std::fs::remove_dir_all(dir);
}

/// Rule 35's message shape: a failed `log` or `no-log` assertion names the log
/// line it found, which is what the author needs, not the park the take stopped
/// at, which says nothing about the log.
#[test]
fn a_failed_log_assertion_names_the_line_it_found() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "absent",
                "keiko 1
setting play.text_speed 0

new-game
advance
                 expect no-log warn row 4
",
            ),
            (
                "mismatch",
                "keiko 1
setting play.text_speed 0

new-game
advance
                 expect log warn row 4 contains \"this text is not in the log\"
",
            ),
        ],
    );
    std::fs::remove_file(pack.join("assets/sprites/ace/neutral.png")).unwrap();

    let output = take(&pack, &["absent"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    let text = stdout(&output);
    assert!(text.contains("expected no warn row 4 line"), "{text}");
    assert!(text.contains("observed warn row 4 \""), "{text}");
    assert!(text.contains("sprites/ace/neutral.png"), "{text}");

    let output = take(&pack, &["mismatch"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    let text = stdout(&output);
    assert!(
        text.contains("expected log warn row 4 containing \"this text is not in the log\""),
        "{text}"
    );
    assert!(text.contains("observed warn row 4 \""), "{text}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn wait_advances_exactly_sixty_frames_a_second() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "one",
                "keiko 1\nsetting play.text_speed 0\n\nnew-game\nwait 1s\n",
            ),
            (
                "two",
                "keiko 1\nsetting play.text_speed 0\n\nnew-game\nwait 2s\n",
            ),
        ],
    );
    let output = run(&pack, &["--take", "one", "two", "--format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let document: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid JSON");
    let takes = document["takes"].as_array().expect("a takes array");
    let clock = |index: usize| {
        takes[index]["observation"]["clock"]
            .as_u64()
            .expect("a clock")
    };
    assert_eq!(clock(0), 1_000, "wait 1s is 60 frames");
    assert_eq!(clock(1), 2_000, "wait 2s is 120 frames");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn observe_prints_one_case_document() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "seen",
            "keiko 1\nsetting play.text_speed 0\n\nexpect surface title\n",
        )],
    );
    let output = run(&pack, &["--observe", "seen", "--format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let document: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid JSON");
    assert!(document.get("result").is_some(), "{document}");
    assert!(document.get("observation").is_some(), "{document}");
    assert!(document.get("takes").is_none(), "{document}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn new_game_in_play_fails_instead_of_moving_the_focus() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "twice",
            "keiko 1\nsetting play.text_speed 0\n\nnew-game\nnew-game\n",
        )],
    );
    let output = take(&pack, &["twice"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("keiko.step.no-target"),
        "{}",
        stdout(&output)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Rule 9: a disabled control fails with `keiko.step.disabled`, while a control
/// the current surface does not show still fails with `keiko.step.no-target`.
#[test]
fn a_disabled_control_reports_step_disabled_and_an_absent_one_step_no_target() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "disabled",
                "keiko 1
setting play.text_speed 0

                 expect control title.continue disabled
activate title.continue
",
            ),
            (
                "absent",
                "keiko 1
setting play.text_speed 0

activate overlay.slot.1
",
            ),
        ],
    );

    let output = take(&pack, &["disabled"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("keiko.step.disabled"),
        "{}",
        stdout(&output)
    );
    assert!(
        !stdout(&output).contains("keiko.step.no-target"),
        "a disabled control is not a missing one:
{}",
        stdout(&output)
    );

    let output = take(&pack, &["absent"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("keiko.step.no-target"),
        "{}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("`overlay.slot.1` is not on the title surface"),
        "{}",
        stdout(&output)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Rule 18: a blocked save reports itself and leaves the old slot byte for byte
/// as it was, and `unblock` then lets the save land. Both halves start from a
/// fixture that holds a park in another scene, so a landing save is visible as
/// a changed scene and as changed bytes.
#[test]
fn a_blocked_save_keeps_the_old_slot_bytes_and_an_unblocked_one_lands() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "blocked",
                "keiko 1
fixture slot-1 fixtures/old.ron
fault write-blocked slot-1
                 setting play.text_speed 0

new-game
menu
save 1
                 activate confirm.yes
expect notice contains \"couldn't save\"
                 expect slot 1 scene reunion
",
            ),
            (
                "recovered",
                "keiko 1
fixture slot-1 fixtures/old.ron
fault write-blocked slot-1
                 setting play.text_speed 0

new-game
menu
unblock slot-1
save 1
                 activate confirm.yes
expect slot 1 scene first_meeting
",
            ),
        ],
    );
    // A manual slot holding a park in `reunion`, copied from the demo fixture
    // pack, so the slot's scene and bytes both differ from a save at the
    // new-game park.
    let fixture = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/keiko/d/tests/fixtures/slot-reunion.ron"),
    )
    .expect("the slot fixture is checked in");
    std::fs::create_dir_all(pack.join("tests/fixtures")).unwrap();
    std::fs::write(pack.join("tests/fixtures/old.ron"), &fixture).unwrap();
    let name = pack.file_name().unwrap().to_string_lossy().to_string();
    let slot = |kept: &Path| kept.join(format!("{name}.slot1.save.ron"));

    let output = run(&pack, &["--take", "blocked", "--keep"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let kept = kept_path(&output);
    assert_eq!(
        std::fs::read(slot(&kept)).expect("the fixture slot is still there"),
        fixture,
        "a refused write leaves the slot byte-identical"
    );
    let _ = std::fs::remove_dir_all(&kept);

    let output = run(&pack, &["--take", "recovered", "--keep"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let kept = kept_path(&output);
    assert_ne!(
        std::fs::read(slot(&kept)).expect("the save wrote the slot"),
        fixture,
        "the unblocked save landed"
    );
    let _ = std::fs::remove_dir_all(&kept);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_save_fixture_lands_at_its_sidecar_path() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "writer",
            "keiko 1\nsetting play.text_speed 0\n\nnew-game\nmenu\nsave 1\n",
        )],
    );
    let output = run(&pack, &["--take", "writer", "--keep"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let kept = stdout(&output)
        .lines()
        .find_map(|line| line.strip_prefix("kept: "))
        .map(|path| PathBuf::from(path.trim()))
        .expect("a kept sandbox path");
    let name = pack.file_name().unwrap().to_string_lossy().to_string();
    let written = kept.join(format!("{name}.slot1.save.ron"));
    assert!(written.is_file(), "{}", written.display());
    let fixtures = pack.join("tests/fixtures");
    std::fs::create_dir_all(&fixtures).unwrap();
    std::fs::copy(&written, fixtures.join("save.ron")).unwrap();
    std::fs::write(
        pack.join("tests/reads.keiko"),
        "keiko 1\nfixture slot-1 fixtures/save.ron\nsetting play.text_speed 0\n\n\
         expect slot 1 scene first_meeting\n",
    )
    .unwrap();
    assert_pass(&pack, "reads");
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(kept);
}

/// The live half of rule 32: a real answer arrives inside the 60 s real-time
/// wait. Ignored by default because it needs a key and the network; run it with
/// `NANOGPT_API_KEY=… cargo test -p unmei-app --test keiko_rules -- --ignored`.
#[test]
#[ignore = "needs NANOGPT_API_KEY and the network"]
fn a_live_answer_lands_inside_the_real_time_wait() {
    if std::env::var_os("NANOGPT_API_KEY").is_none_or(|value| value.is_empty()) {
        eprintln!("NANOGPT_API_KEY is not set; the live take is skipped");
        return;
    }
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "live",
            "keiko 1\nsetting play.text_speed 0\n\nnew-game\nadvance\nadvance\npick 1\n\
             advance\nadvance\nexpect decision none\nexpect surface play\n",
        )],
    );
    let output = Command::new(binary())
        .args([pack.to_str().unwrap(), "--take", "live", "--live"])
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_saved_stuck_finding_parses_and_fails_until_the_pack_moves_on() {
    let dir = temp_dir();
    let pack = dir.join("looping");
    std::fs::create_dir_all(pack.join("scenes")).unwrap();
    std::fs::create_dir_all(pack.join("npcs")).unwrap();
    std::fs::create_dir_all(pack.join("tests")).unwrap();
    std::fs::write(
        pack.join("pack.ron"),
        "(name: \"looping\", entry: \"start\")",
    )
    .unwrap();
    std::fs::write(pack.join("state.ron"), "(met: false,)").unwrap();
    // A scene that jumps back to itself: every advance leaves the park in place,
    // so the line after it is never read.
    std::fs::write(
        pack.join("scenes/start.unmei"),
        // The loop never reaches the line after it, so the saved case can
        // assert that say-line's read mark.
        "Narrator: one\nlabel quiet\nNarrator: same\njump quiet\nNarrator: never\n",
    )
    .unwrap();
    let output = run(&pack, &["--explore", "--budget", "200", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_str(stdout(&output).trim()).unwrap();
    let findings = json["findings"].as_array().unwrap();
    let stuck = findings
        .iter()
        .find(|finding| finding["rule"] == "keiko.stuck")
        .expect("a stuck finding");
    let saved = stuck["case"].as_str().unwrap().to_string();
    let text = std::fs::read_to_string(pack.join(&saved)).unwrap();
    assert!(text.contains("expect read"), "{text}");
    // The saved case must parse (no syntax error) and fail on the loop.
    let id = saved
        .trim_start_matches("tests/")
        .trim_end_matches(".keiko");
    let output = run(&pack, &["--take", id]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        !stdout(&output).contains("keiko.case.syntax"),
        "{}",
        stdout(&output)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Rule 38: one question id belongs to one NPC, whether it is set by `decide`
/// or by a decision fault.
#[test]
fn one_question_id_on_two_npcs_is_refused() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "decides",
                "keiko 1\ndecide ace.greet fallback\ndecide bea.greet fallback\n\nnew-game\n",
            ),
            (
                "faults",
                "keiko 1\ndecide ace.greet fallback\nfault decision-hold bea.greet\n\nnew-game\n",
            ),
        ],
    );
    // A second NPC with the same question id as ace.
    let ace = std::fs::read_to_string(pack.join("npcs/ace.ron")).unwrap();
    // The NPC's own id must match its file stem.
    let bea = ace.replacen("id: \"ace\"", "id: \"bea\"", 1);
    std::fs::write(pack.join("npcs/bea.ron"), bea).unwrap();

    for id in ["decides", "faults"] {
        let output = take(&pack, &[id]);
        assert_eq!(output.status.code(), Some(1), "{id}: {}", stdout(&output));
        let text = stdout(&output);
        assert!(text.contains("keiko.case.reference"), "{id}: {text}");
        assert!(text.contains("cannot tell them apart"), "{id}: {text}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Rule 29: a saved finding must fail under `--take`. A stuck park with no
/// say-line after it has no expressible closure, so it must not be saved as a
/// passing case.
#[test]
fn a_stuck_finding_without_a_later_say_line_saves_no_case() {
    let dir = temp_dir();
    let pack = dir.join("noclosure");
    std::fs::create_dir_all(pack.join("scenes")).unwrap();
    std::fs::create_dir_all(pack.join("npcs")).unwrap();
    std::fs::create_dir_all(pack.join("tests")).unwrap();
    std::fs::write(
        pack.join("pack.ron"),
        "(name: \"noclosure\", entry: \"start\")",
    )
    .unwrap();
    std::fs::write(pack.join("state.ron"), "(met: false,)").unwrap();
    // The only way onward is a choice that loops back to the same say line:
    // there is no say-line after the choice park, so no closure is expressible.
    std::fs::write(
        pack.join("scenes/start.unmei"),
        "Narrator: one\nlabel quiet\nNarrator: same\nchoice\n  \"again\" -> quiet\nend\n",
    )
    .unwrap();
    let output = run(&pack, &["--explore", "--budget", "200", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_str(stdout(&output).trim()).unwrap();
    let findings = json["findings"].as_array().unwrap();
    let stuck = findings
        .iter()
        .find(|finding| finding["rule"] == "keiko.stuck")
        .expect("a stuck finding");
    assert!(stuck.get("case").is_none(), "{stuck}");
    assert_eq!(stuck["saved"], serde_json::json!(false), "{stuck}");
    let findings_dir = pack.join("tests/findings");
    let files = std::fs::read_dir(&findings_dir)
        .map(|entries| entries.flatten().count())
        .unwrap_or(0);
    assert_eq!(files, 0, "no case was written");
    let output = run(&pack, &["--explore", "--budget", "200"]);
    assert!(stdout(&output).contains("unsaved"), "{}", stdout(&output));
    let _ = std::fs::remove_dir_all(dir);
}

// The cycle, rules 11, 12 and 17, and the K2–K4 credential cases

/// The case's observation of one control, so a test can read the label the
/// player actually rendered.
fn control_label(take: &serde_json::Value, id: &str) -> String {
    take["observation"]["controls"]
        .as_array()
        .expect("controls")
        .iter()
        .find(|control| control["id"] == id)
        .and_then(|control| control["label"].as_str())
        .unwrap_or_else(|| panic!("no control `{id}` in {take}"))
        .to_string()
}

fn control_enabled(take: &serde_json::Value, id: &str) -> bool {
    take["observation"]["controls"]
        .as_array()
        .expect("controls")
        .iter()
        .find(|control| control["id"] == id)
        .and_then(|control| control["enabled"].as_bool())
        .unwrap_or_else(|| panic!("no control `{id}` in {take}"))
}

fn kept_path(output: &Output) -> PathBuf {
    PathBuf::from(
        stdout(output)
            .lines()
            .find_map(|line| line.strip_prefix("kept: "))
            .map(|path| path.trim())
            .expect("a kept sandbox path"),
    )
}

/// A loop that parks only while `met_ace` holds: with the declared `false` it
/// spins, a cycle pack load cannot see because it passes through an `if`.
const SPINNING_LOOP: &str = "label top\nif met_ace\nA: met.\nendif\njump top\n";

fn spinning_pack(dir: &Path) -> PathBuf {
    let pack = dir.join("spin");
    std::fs::create_dir_all(pack.join("scenes")).unwrap();
    std::fs::create_dir_all(pack.join("npcs")).unwrap();
    std::fs::create_dir_all(pack.join("tests")).unwrap();
    std::fs::write(pack.join("pack.ron"), "(name: \"spin\", entry: \"a\")").unwrap();
    std::fs::write(pack.join("state.ron"), "(met_ace: false,)").unwrap();
    std::fs::write(
        pack.join("scenes/a.unmei"),
        format!("A: entry.\n{SPINNING_LOOP}"),
    )
    .unwrap();
    pack
}

/// A take over a state-spinning cycle ends held on the defensive surface with
/// row 13 logged; exploration reports it instead of hanging.
#[test]
fn a_state_spinning_cycle_ends_held_and_exploration_reports_it() {
    let dir = temp_dir();
    let pack = spinning_pack(&dir);
    std::fs::write(
        pack.join("tests/cycle.keiko"),
        "keiko 1\nsetting play.text_speed 0\n\nnew-game\nexpect line \"entry.\"\nadvance\n\
         expect surface play\nexpect notice contains \"the story can't continue\"\n\
         expect log error row 13 contains \"closes a cycle that reaches no say-line\"\n",
    )
    .unwrap();
    assert_pass(&pack, "cycle");

    let output = run_with_timeout(
        &pack,
        &["--explore", "--budget", "200", "--format", "json"],
        Duration::from_secs(60),
    );
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        !stdout(&output).contains("keiko.case.syntax"),
        "{}",
        stdout(&output)
    );
    let json: serde_json::Value = serde_json::from_str(stdout(&output).trim()).unwrap();
    let findings = json["findings"].as_array().expect("findings");
    assert!(
        findings
            .iter()
            .any(|finding| finding["rule"] == "keiko.stuck"),
        "the cycle is reported as a finding: {}",
        stdout(&output)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Rule 11: while the Monitor owns input, an intent does not reach play.
#[test]
fn an_open_monitor_owns_input_so_new_game_does_not_start_play() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "claimed",
            "keiko 1\ndev\nsetting play.text_speed 0\n\npress F3\nexpect monitor open\n\
             new-game\nexpect surface title\nexpect monitor open\n\
             press Escape\nexpect monitor closed\npress Enter\nexpect surface play\n",
        )],
    );
    assert_pass(&pack, "claimed");
    let _ = std::fs::remove_dir_all(dir);
}

/// Rule 12: a key held at a load does not act on the loaded park until it is
/// released and pressed again.
#[test]
fn a_held_key_at_a_load_does_not_act_until_it_is_released() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "held",
            "keiko 1\nsetting play.text_speed 0\n\nnew-game\n\
             expect line \"Morning light. A folded letter on the ground, half under a shoe.\"\n\
             menu\nactivate overlay.slot.1\nexpect slot 1 scene first_meeting\nmenu\nadvance\n\
             expect line \"You're not from around here, are you?\"\n\
             hold Space\nload 1\nactivate confirm.yes\nexpect surface play\n\
             expect line \"Morning light. A folded letter on the ground, half under a shoe.\"\n\
             release Space\n\
             expect line \"Morning light. A folded letter on the ground, half under a shoe.\"\n\
             advance\nexpect line \"You're not from around here, are you?\"\n",
        )],
    );
    assert_pass(&pack, "held");
    let _ = std::fs::remove_dir_all(dir);
}

const KEY_ROW_CASE: &str = "setting play.text_speed 0\n\nactivate title.settings\n\
    activate settings.key\ntype \"sk-nano-abcdefghijklmnop\"\npress Enter\n";

/// Rule 17: every key-check verdict sets the row state the Decisions settings
/// contract names.
#[test]
fn every_key_check_verdict_sets_the_key_row_state() {
    let dir = temp_dir();
    let accepted = format!("keiko 1\nfault key-check accepted\n{KEY_ROW_CASE}");
    let rejected = format!("keiko 1\nfault key-check rejected\n{KEY_ROW_CASE}");
    let unreachable = format!("keiko 1\nfault key-check unreachable\n{KEY_ROW_CASE}");
    let checking = format!("keiko 1\nfault key-check hold\n{KEY_ROW_CASE}");
    let released = format!("keiko 1\nfault key-check hold\n{KEY_ROW_CASE}release-key-check\n");
    let pack = pack(
        &dir,
        &[
            ("accepted", &accepted),
            ("rejected", &rejected),
            ("unreachable", &unreachable),
            ("checking", &checking),
            ("released", &released),
        ],
    );
    let output = run(
        &pack,
        &[
            "--take",
            "accepted",
            "rejected",
            "unreachable",
            "checking",
            "released",
            "--format",
            "json",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let document: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let takes = document["takes"].as_array().expect("a takes array");
    let row = |index: usize| control_label(&takes[index], "settings.key");
    assert!(
        row(0).starts_with("Set — sk-nano-…mnop") && !row(0).contains("not checked"),
        "accepted: {}",
        row(0)
    );
    assert!(
        row(1).starts_with("Invalid — rejected by NanoGPT"),
        "rejected: {}",
        row(1)
    );
    assert!(
        row(2).starts_with("Set — sk-nano-…mnop · not checked (see the log)"),
        "unreachable: {}",
        row(2)
    );
    assert!(row(3).starts_with("Checking…"), "checking: {}", row(3));
    assert!(
        row(4).starts_with("Set — sk-nano-…mnop · not checked (see the log)"),
        "released: {}",
        row(4)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// K2: a stored key shows Set without a ping; a non-empty environment key wins
/// and leaves the stored bytes untouched.
#[test]
fn a_stored_key_shows_set_and_an_environment_key_wins() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "stored",
                "keiko 1\nfixture credential fixtures/cred.ron\nsetting play.text_speed 0\n\n\
                 activate title.settings\n",
            ),
            (
                "env",
                "keiko 1\nfixture credential fixtures/cred.ron\n\
                 fixture env-key fixtures/env.ron\nsetting play.text_speed 0\n\n\
                 activate title.settings\n",
            ),
        ],
    );
    std::fs::create_dir_all(pack.join("tests/fixtures")).unwrap();
    std::fs::write(
        pack.join("tests/fixtures/cred.ron"),
        "sk-nano-abcdefghijklmnop",
    )
    .unwrap();
    std::fs::write(
        pack.join("tests/fixtures/env.ron"),
        "sk-nano-envenvenvenv1234",
    )
    .unwrap();

    let output = run(&pack, &["--take", "stored", "env", "--format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let document: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let takes = document["takes"].as_array().expect("a takes array");
    assert!(
        control_label(&takes[0], "settings.key").starts_with("Set — sk-nano-…mnop"),
        "{}",
        control_label(&takes[0], "settings.key")
    );
    assert!(
        control_label(&takes[1], "settings.key").starts_with("From environment — sk-nano-…1234"),
        "{}",
        control_label(&takes[1], "settings.key")
    );
    assert!(
        !control_enabled(&takes[1], "settings.clear-key"),
        "the environment key makes Clear key inert"
    );

    // The override never rewrites the stored key.
    let kept_output = run(&pack, &["--take", "env", "--keep"]);
    assert_eq!(
        kept_output.status.code(),
        Some(0),
        "{}",
        stdout(&kept_output)
    );
    let kept = kept_path(&kept_output);
    let stored = std::fs::read_to_string(kept.join("config/unmei/credentials")).unwrap();
    assert_eq!(stored, "sk-nano-abcdefghijklmnop");
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(kept);
}

/// K3: a committed shape-valid key is written before its check, an
/// unreachable check is Set and not Invalid, and Clear returns the row to Not
/// set.
#[test]
fn a_committed_key_checks_offline_and_clears() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "cleared",
            "keiko 1\nfault key-check unreachable\nsetting play.text_speed 0\n\n\
             activate title.settings\nactivate settings.key\n\
             type \"sk-nano-abcdefghijklmnop\"\npress Enter\n\
             activate settings.clear-key\nactivate confirm.yes\n",
        )],
    );
    let output = run(&pack, &["--take", "cleared", "--format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let document: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let take = &document["takes"][0];
    assert!(
        control_label(take, "settings.key").starts_with("Not set"),
        "{}",
        control_label(take, "settings.key")
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// K4: an unreadable credential file is Not set plus a row-24 warning, with no
/// Notice and no auto-delete.
#[test]
fn an_unreadable_credential_file_is_not_set_with_its_warning() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "unreadable",
            "keiko 1\nfault credential-unreadable\nsetting play.text_speed 0\n\n\
             expect notice none\n\
             expect log warn row 24 contains \"could not read the credential file\"\n\
             activate title.settings\n",
        )],
    );
    let output = run(&pack, &["--take", "unreadable", "--format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let document: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let take = &document["takes"][0];
    assert!(
        control_label(take, "settings.key").starts_with("Not set"),
        "{}",
        control_label(take, "settings.key")
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// K4: a blocked credential write shows the Notice, keeps the stored key and
/// the row, and leaves no partial file.
#[test]
fn a_blocked_credential_write_keeps_the_stored_key_and_shows_its_notice() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "refused",
            "keiko 1\nfixture credential fixtures/stored.ron\n\
             fault write-blocked credential\nsetting play.text_speed 0\n\n\
             activate title.settings\nactivate settings.key\n\
             type \"sk-nano-bbbbbbbbbbbbbwxyz\"\npress Enter\n\
             expect notice contains \"Could not save the key\"\n",
        )],
    );
    std::fs::create_dir_all(pack.join("tests/fixtures")).unwrap();
    std::fs::write(
        pack.join("tests/fixtures/stored.ron"),
        "sk-nano-abcdefghijklmnop",
    )
    .unwrap();

    let output = run(&pack, &["--take", "refused", "--keep"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let kept = kept_path(&output);
    let stored = std::fs::read_to_string(kept.join("config/unmei/credentials")).unwrap();
    assert_eq!(
        stored, "sk-nano-abcdefghijklmnop",
        "the failed write must not replace the stored key"
    );
    let temp = kept.join("config/unmei/credentials.tmp");
    assert!(
        temp.is_dir(),
        "the write was blocked at its temp path, not past it: {}",
        temp.display()
    );

    let json = run(&pack, &["--take", "refused", "--format", "json"]);
    let document: serde_json::Value = serde_json::from_str(&stdout(&json)).unwrap();
    let row = control_label(&document["takes"][0], "settings.key");
    assert!(row.starts_with("Set — sk-nano-…mnop"), "{row}");
    assert!(
        !row.contains("not checked"),
        "a blocked write starts no check: {row}"
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(kept);
}

/// K4: removing the obstruction lets the same commit land.
#[test]
fn an_unblocked_credential_write_lands() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "recovered",
            "keiko 1\nfault write-blocked credential\nsetting play.text_speed 0\n\n\
             activate title.settings\nactivate settings.key\n\
             type \"sk-nano-bbbbbbbbbbbbbwxyz\"\npress Enter\n\
             unblock credential\nactivate settings.key\n\
             type \"sk-nano-bbbbbbbbbbbbbwxyz\"\npress Enter\n",
        )],
    );
    let output = run(&pack, &["--take", "recovered", "--format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let document: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let row = control_label(&document["takes"][0], "settings.key");
    assert!(row.starts_with("Set — sk-nano-…wxyz"), "{row}");
    let _ = std::fs::remove_dir_all(dir);
}

/// The overlay's Settings walk reaches the rail's `Exit to title` control, its
/// cancel returns focus to it, confirming runs to the title and Continue resumes
/// the live park; the title's door draws no such control.
#[test]
fn the_overlay_settings_rail_reaches_the_title_through_its_control() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "route",
                "keiko 1
setting play.text_speed 0

new-game
menu
                 press Tab
press Tab
press Tab
press Tab
press Tab
press Enter
                 expect surface settings
activate overlay.exit-to-title
                 activate confirm.yes
expect surface title
",
            ),
            (
                "resume",
                "keiko 1
setting play.text_speed 0

new-game
menu
                 press Tab
press Tab
press Tab
press Tab
press Tab
press Enter
                 activate overlay.exit-to-title
activate confirm.yes
                 activate title.continue
expect surface play
                 expect line \"Morning light. A folded letter on the ground, half under a shoe.\"
",
            ),
            (
                "cancel",
                "keiko 1
setting play.text_speed 0

new-game
menu
                 press Tab
press Tab
press Tab
press Tab
press Tab
press Enter
                 activate overlay.exit-to-title
activate confirm.no
                 expect surface settings
expect control overlay.exit-to-title focused
",
            ),
            (
                "title-door",
                "keiko 1
activate title.settings
expect surface settings
                 expect control overlay.exit-to-title absent
                 expect control overlay.resume absent
",
            ),
        ],
    );
    let output = run(
        &pack,
        &[
            "--take",
            "route",
            "resume",
            "cancel",
            "title-door",
            "--format",
            "json",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let document: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let takes = document["takes"].as_array().expect("a takes array");
    assert_eq!(
        takes[0]["observation"]["surface"], "title",
        "the route runs to the title through the rail control"
    );
    assert_eq!(
        takes[1]["observation"]["surface"], "play",
        "Continue resumes the live park the exit kept"
    );
    let rail = takes[2]["observation"]["controls"]
        .as_array()
        .expect("controls")
        .iter()
        .find(|control| control["id"] == "overlay.exit-to-title")
        .expect("the rail control is on the overlay Settings surface");
    assert_eq!(rail["label"], "Exit to title");
    assert_eq!(rail["focused"], true, "cancel returns focus to the control");
    assert!(control_enabled(&takes[2], "overlay.exit-to-title"));
    assert!(
        takes[3]["observation"]["controls"]
            .as_array()
            .expect("controls")
            .iter()
            .all(|control| control["id"] != "overlay.exit-to-title"),
        "the title's door draws no rail control"
    );
    let _ = std::fs::remove_dir_all(dir);
}

// Rules 35-37: the render path (slice 2)

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Run the binary with extra environment, so a test can take the fallback
/// adapter away with no product hook (`WGPU_BACKEND` names a backend this
/// platform does not have).
fn run_env(pack: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut full = vec![pack.to_str().unwrap().to_string()];
    full.extend(args.iter().map(|arg| arg.to_string()));
    let mut command = Command::new(binary());
    command.args(&full).env_remove("NANOGPT_API_KEY");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("the binary runs")
}

/// Every `frame <n>: <path>` line a take printed, in order.
fn frame_paths(output: &Output) -> Vec<PathBuf> {
    stderr(output)
        .lines()
        .filter_map(|line| line.strip_prefix("frame "))
        .filter_map(|line| line.split_once(": "))
        .map(|(_, path)| PathBuf::from(path.trim()))
        .collect()
}

/// Rule 35: a `frame` writes a 1280×720 PNG at the documented path on the
/// software rasterizer, numbered by the step's ordinal and named by the step.
#[test]
fn a_frame_step_writes_a_1280_by_720_png() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "frames",
            "keiko 1
setting play.text_speed 0

new-game
expect line contains \"Morning light\"
frame
frame titled
",
        )],
    );
    // No `--keep`: the frames land beside the real pack, so they outlive the
    // take and its sandbox (spec §13).
    let output = run(&pack, &["--take", "frames"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let paths = frame_paths(&output);
    assert_eq!(
        paths.len(),
        2,
        "one path per frame step:\n{}",
        stderr(&output)
    );
    let folder = std::fs::canonicalize(pack.parent().unwrap())
        .unwrap()
        .join("demo-pack.keiko")
        .join("frames")
        .join("frames");
    for (path, name) in paths.iter().zip(["1.png", "2-titled.png"]) {
        assert!(path.exists(), "{} outlives the take", path.display());
        assert_eq!(
            std::fs::canonicalize(path).unwrap(),
            folder.join(name),
            "the frame takes its ordinal and its name, beside the pack"
        );
        let image = image::open(path).expect("the frame decodes as an image");
        assert_eq!(
            (image.width(), image.height()),
            (1280, 720),
            "the frame is the reference size"
        );
    }
    // The renderer's console logging stays out: stderr holds only the paths.
    let stray: Vec<String> = stderr(&output)
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with("frame "))
        .map(str::to_string)
        .collect();
    assert!(
        stray.is_empty(),
        "stderr holds only frame paths: {stray:#?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Rule 35, and the multi-frame capture the preview's rule 22 exposed: every
/// `frame` a take is given writes its own PNG. A readback that redrew the
/// composition into the target image while the previous copy was still queued
/// fell one frame further behind each time and failed on the fourth.
#[test]
fn every_frame_step_a_take_is_given_writes_its_png() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[(
            "many-frames",
            "keiko 1
setting play.text_speed 0

new-game
expect line contains \"Morning light\"
frame
frame one
frame two
frame three
",
        )],
    );
    let output = run(&pack, &["--take", "many-frames"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let paths = frame_paths(&output);
    assert_eq!(
        paths.len(),
        4,
        "one path per frame step:\n{}",
        stderr(&output)
    );
    for (path, name) in paths
        .iter()
        .zip(["1.png", "2-one.png", "3-two.png", "4-three.png"])
    {
        assert!(path.exists(), "{} exists", path.display());
        assert_eq!(
            path.file_name().unwrap(),
            name,
            "the frame's ordinal and name"
        );
        let image = image::open(path).expect("the frame decodes as an image");
        assert_eq!(
            (image.width(), image.height()),
            (1280, 720),
            "the frame is the reference size"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Rule 36: a control click goes through the player's own hit test — it starts
/// play, and a click on a control the confirm surface covers does nothing.
#[test]
fn a_click_reaches_the_player_hit_test_and_a_covered_click_does_nothing() {
    let dir = temp_dir();
    // The covering surface is the New Game confirm's full-screen dimmer: while
    // it is up, `title.quit` lies under it. Were the click to reach that button
    // the confirm would ask about quitting instead (its accept is "Quit"), so
    // the "Start" label proves the covering surface took the click.
    let pack = pack(
        &dir,
        &[(
            "clicks",
            "keiko 1
setting play.text_speed 0

click title.new-game
expect surface play
expect line contains \"Morning light\"
menu
press Tab
press Tab
press Tab
press Tab
press Tab
press Enter
expect surface settings
activate overlay.exit-to-title
activate confirm.yes
expect surface title
click title.quit
expect surface confirm
expect control confirm.yes label \"Quit\"
activate confirm.no
expect surface title
click title.new-game
expect surface confirm
expect control confirm.yes label \"Start\"
click title.quit
expect surface confirm
expect control confirm.yes label \"Start\"
click confirm.no
expect surface title
",
        )],
    );
    let output = run(&pack, &["--take", "clicks"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let _ = std::fs::remove_dir_all(dir);
}

/// Rule 37: with no fallback adapter, `frame` (and `click`) fails with
/// `keiko.render.unavailable`, and the steps before it still apply.
#[test]
fn without_a_fallback_adapter_frames_and_clicks_report_render_unavailable() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "frame",
                "keiko 1
setting play.text_speed 0

new-game
expect line contains \"Morning light\"
frame
expect surface play
",
            ),
            (
                "click",
                "keiko 1
setting play.text_speed 0

new-game
click title.new-game
",
            ),
        ],
    );
    // `metal` is not a backend this platform compiles, so the probe finds no
    // fallback adapter and the take keeps the windowless app.
    for (id, line) in [("frame", 6), ("click", 5)] {
        let output = run_env(&pack, &["--take", id], &[("WGPU_BACKEND", "metal")]);
        assert_eq!(output.status.code(), Some(1), "{id}: {}", stdout(&output));
        let text = stdout(&output);
        assert!(
            text.contains("keiko.render.unavailable"),
            "{id}: expected the render rule, got:\n{text}"
        );
        assert!(
            text.contains(&format!("line {line}")),
            "{id}: the render step is line {line}, and no earlier step failed:\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Decision 3 of the slice-2 brief: every mode that runs a `frame` step writes
/// its PNG, and `--observe` still leaves the case file alone.
#[test]
fn a_frame_writes_in_observe_and_step_modes_and_never_edits_the_case() {
    let dir = temp_dir();
    let pack = pack(
        &dir,
        &[
            (
                "watched",
                "keiko 1\nsetting play.text_speed 0\n\nnew-game\nframe\n",
            ),
            (
                "stepped",
                "keiko 1\nsetting play.text_speed 0\n\nnew-game\n",
            ),
        ],
    );
    let watched = pack.join("tests/watched.keiko");
    let before = std::fs::read(&watched).unwrap();
    let output = run(&pack, &["--observe", "watched"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let watched_frames = frame_paths(&output);
    assert_eq!(
        watched_frames.len(),
        1,
        "--observe writes the frame:\n{}",
        stderr(&output)
    );
    assert!(watched_frames[0].exists(), "the frame outlives --observe");
    assert_eq!(
        std::fs::read(&watched).unwrap(),
        before,
        "--observe never writes the case file"
    );

    let stepped = pack.join("tests/stepped.keiko");
    let before = std::fs::read(&stepped).unwrap();
    let output = run(&pack, &["--step", "stepped", "frame"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert_eq!(
        frame_paths(&output).len(),
        1,
        "--step writes the frame too:\n{}",
        stderr(&output)
    );
    let after = std::fs::read(&stepped).unwrap();
    let appended = after.get(before.len()..).unwrap_or_default();
    assert!(
        after.starts_with(&before) && appended == b"frame\n",
        "the passed step is appended, and the file is otherwise unchanged: {:?}",
        String::from_utf8_lossy(appended)
    );
    let _ = std::fs::remove_dir_all(dir);
}
