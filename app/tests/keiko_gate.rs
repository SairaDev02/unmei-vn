//! The Keiko gate: the built binary runs the demo pack's cases on every push
//! (spec §14), plus the binary-level rules that need a real process — usage
//! errors, isolation, determinism, invalid cases and sandbox cleanup.

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

/// The gate: the demo pack's cases pass under the built binary.
#[test]
fn the_demo_packs_cases_pass() {
    let pack = demo_pack();
    let output = run(&[pack.to_str().unwrap(), "--take"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(stdout(&output).contains("PASS opening"), "{}", stdout(&output));
}

// ---------------------------------------------------------------------------
// Spec §15 rule 1: the launch grammar
// ---------------------------------------------------------------------------

#[test]
fn keiko_usage_errors_exit_two_before_startup() {
    let pack = demo_pack();
    let pack = pack.to_str().unwrap();
    for args in [
        vec![pack, "--take", "--observe", "opening"],
        vec![pack, "--observe", "opening", "--live"],
        vec![pack, "--take", "--budget", "5"],
        vec![pack, "--dev", "--take"],
        vec![pack, "--step", "opening"],
        vec![pack, "--take", "Bad"],
        vec![pack, "--keep"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {}", stderr(&output));
    }
}

// ---------------------------------------------------------------------------
// Spec §15 rule 2: a take never touches the user's files
// ---------------------------------------------------------------------------

#[test]
fn a_take_never_reads_or_writes_the_users_files() {
    let dir = tempfile::tempdir().unwrap();
    let canary = dir.path().join("appdata");
    std::fs::create_dir_all(canary.join("unmei")).unwrap();
    std::fs::write(canary.join("unmei/credentials"), "sk-nano-canary\n").unwrap();

    // A copy of the demo pack, so the original's sidecars are not the subject.
    let pack = dir.path().join("demo-pack");
    copy_dir(&demo_pack(), &pack);
    let before = sidecars(&dir.path().join("demo-pack"));

    let output = Command::new(binary())
        .args([pack.to_str().unwrap(), "--take"])
        .env("APPDATA", &canary)
        .env("XDG_CONFIG_HOME", &canary)
        .env_remove("NANOGPT_API_KEY")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));

    assert_eq!(
        sidecars(&pack),
        before,
        "the real pack's folder gained no sidecar"
    );
    assert_eq!(
        std::fs::read_to_string(canary.join("unmei/credentials")).unwrap(),
        "sk-nano-canary\n",
        "the credential file is untouched"
    );
}

/// Every sidecar sibling of a pack folder: `<name>.*`.
fn sidecars(pack: &Path) -> Vec<(String, u64)> {
    let parent = pack.parent().unwrap();
    let name = pack.file_name().unwrap().to_string_lossy().into_owned();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(parent).unwrap() {
        let entry = entry.unwrap();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.starts_with(&format!("{name}.")) {
            out.push((file_name, entry.metadata().unwrap().len()));
        }
    }
    out.sort();
    out
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

// ---------------------------------------------------------------------------
// Spec §15 rules 3 and 4: version, header order and references
// ---------------------------------------------------------------------------

fn case_pack(dir: &Path, case: &str) -> PathBuf {
    let pack = dir.join(format!("pack-{}", case.len()));
    copy_dir(&demo_pack(), &pack);
    std::fs::create_dir_all(pack.join("tests")).unwrap();
    std::fs::write(pack.join("tests/bad.keiko"), case).unwrap();
    pack
}

#[test]
fn a_bad_version_fails_with_its_rule_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    for case in ["", "keiko 2\nnew-game\n", "keiko x\nnew-game\n", "new-game\n"] {
        let pack = case_pack(dir.path(), case);
        let output = run(&[pack.to_str().unwrap(), "--take", "bad"]);
        assert_eq!(output.status.code(), Some(1), "{case:?}");
        assert!(
            stdout(&output).contains("keiko.case.version"),
            "{case:?}: {}",
            stdout(&output)
        );
    }
}

#[test]
fn a_header_after_a_step_fails_with_header_order() {
    let dir = tempfile::tempdir().unwrap();
    let pack = case_pack(dir.path(), "keiko 1\nnew-game\ndev\n");
    let output = run(&[pack.to_str().unwrap(), "--take", "bad"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains("keiko.case.header-order"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn an_unknown_reference_fails_before_a_take() {
    let dir = tempfile::tempdir().unwrap();
    for (case, rule) in [
        ("keiko 1\ndecide nobody.greet cold\nnew-game\n", "keiko.case.reference"),
        ("keiko 1\nstart nowhere\nnew-game\n", "keiko.case.reference"),
        ("keiko 1\nsetting play.nope 1\nnew-game\n", "keiko.case.reference"),
        ("keiko 1\nactivate title.nope\nnew-game\n", "keiko.case.reference"),
        ("keiko 1\npress KeyZ\nnew-game\n", "keiko.case.syntax"),
    ] {
        let pack = case_pack(dir.path(), case);
        let output = run(&[pack.to_str().unwrap(), "--take", "bad"]);
        assert_eq!(output.status.code(), Some(1), "{case:?}");
        assert!(stdout(&output).contains(rule), "{case:?}: {}", stdout(&output));
    }
}

// ---------------------------------------------------------------------------
// Spec §15 rule 5: determinism
// ---------------------------------------------------------------------------

#[test]
fn the_same_case_run_twice_is_byte_identical() {
    let pack = demo_pack();
    let first = run(&[pack.to_str().unwrap(), "--take", "--format", "json"]);
    let second = run(&[pack.to_str().unwrap(), "--take", "--format", "json"]);
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(stdout(&first), stdout(&second));
}

// ---------------------------------------------------------------------------
// Spec §15 rule 22: the sandbox is cleaned up
// ---------------------------------------------------------------------------

#[test]
fn a_take_leaves_no_sandbox_unless_kept() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("demo-pack");
    copy_dir(&demo_pack(), &pack);
    // A private temp root for the child, so a parallel test's take cannot show
    // up in this test's inventory.
    let temp = dir.path().join("temp");
    std::fs::create_dir_all(&temp).unwrap();

    let output = Command::new(binary())
        .args([pack.to_str().unwrap(), "--take"])
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .env("TMPDIR", &temp)
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert_eq!(
        sandboxes(&temp),
        Vec::<PathBuf>::new(),
        "no sandbox folder is left behind"
    );

    let output = Command::new(binary())
        .args([pack.to_str().unwrap(), "--take", "--keep"])
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .env("TMPDIR", &temp)
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(0));
    let kept = sandboxes(&temp);
    assert_eq!(kept.len(), 1, "exactly one kept take folder: {kept:?}");
    let take = kept[0].join("take-1");
    assert!(take.exists(), "the kept folder holds the take: {take:?}");
    assert!(
        stdout(&output).contains(&take.display().to_string()),
        "a kept take prints its path: {}",
        stdout(&output)
    );
}

fn sandboxes(temp: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(temp) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("unmei-keiko-") {
            out.push(entry.path());
        }
    }
    out.sort();
    out
}
