//! The check gate: the built binary checks the demo pack and every fixture
//! pack under `app/tests/check/` on every push (spec §11).
//!
//! A fixture pack breaks one family of rules. Adding one means adding the pack
//! folder and one row to [`EXPECTATIONS`]; the scaffolding, the exit code and
//! the order assertions stay here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_unmei-play"))
}

fn demo_pack() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/demo-pack")
        .canonicalize()
        .expect("the demo pack exists")
}

/// `app/tests/check/<name>`, canonical so the child sees an absolute pack.
fn fixture_pack(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/check")
        .join(name)
        .canonicalize()
        .unwrap_or_else(|error| panic!("fixture pack `{name}` exists: {error}"))
}

fn pack(name: &str) -> PathBuf {
    if name == "demo" {
        demo_pack()
    } else {
        fixture_pack(name)
    }
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

fn check_json(pack: &Path) -> Output {
    run(&[
        pack.to_str().expect("a pack path"),
        "--check",
        "--format",
        "json",
    ])
}

/// One fixture's expected report: its JSON counts and its findings as
/// `(file, rule)` in the spec's §3 order.
struct Expectation {
    /// `demo`, or a folder under `app/tests/check/`.
    pack: &'static str,
    errors: usize,
    warnings: usize,
    findings: &'static [(&'static str, &'static str)],
}

impl Expectation {
    /// Exit 0 with no error finding, 1 with at least one (§8).
    fn exit(&self) -> i32 {
        if self.errors == 0 {
            0
        } else {
            1
        }
    }
}

/// The check's whole report for each pack. A later rule family adds its fixture
/// folder and one row here.
const EXPECTATIONS: &[Expectation] = &[
    Expectation {
        pack: "demo",
        errors: 0,
        warnings: 0,
        findings: &[],
    },
    Expectation {
        pack: "warnings",
        errors: 0,
        warnings: 1,
        findings: &[("pack.ron", "pack.manifest.tagline-length")],
    },
    Expectation {
        pack: "cases",
        errors: 5,
        warnings: 0,
        findings: &[
            ("tests/bad-version.keiko", "keiko.case.version"),
            ("tests/findings/reference.keiko", "keiko.case.reference"),
            ("tests/multi.keiko", "keiko.case.reference"),
            ("tests/multi.keiko", "keiko.case.reference"),
            ("tests/unknown-scene.keiko", "keiko.case.reference"),
        ],
    },
    // The pack does not load, so only the case parser runs: the case that names
    // a missing scene reports nothing, and the bad version still reports. The
    // orphan scene's flow finding is not reported either (§8).
    Expectation {
        pack: "unloaded",
        errors: 2,
        warnings: 0,
        findings: &[
            ("pack.ron", "pack.manifest"),
            ("tests/bad-version.keiko", "keiko.case.version"),
        ],
    },
    // §6's three flow warnings, one fixture each. Rule 13's decision-landing
    // scenes are reachable beside an orphan; rule 14's skipped label sits beside
    // a fall-through one; rule 15's looping scene sits beside a leaving option.
    Expectation {
        pack: "flow-unreachable-scene",
        errors: 0,
        warnings: 1,
        findings: &[("scenes/orphan.unmei", "flow.unreachable-scene")],
    },
    Expectation {
        pack: "flow-unreachable-label",
        errors: 0,
        warnings: 1,
        findings: &[("scenes/main.unmei", "flow.unreachable-label")],
    },
    Expectation {
        pack: "flow-no-exit",
        errors: 0,
        warnings: 1,
        findings: &[("scenes/loop.unmei", "flow.no-exit")],
    },
];

#[test]
fn the_check_reports_each_packs_findings_in_order() {
    for expectation in EXPECTATIONS {
        let pack = pack(expectation.pack);
        let output = check_json(&pack);
        let text = stdout(&output);
        assert_eq!(
            output.status.code(),
            Some(expectation.exit()),
            "{}: stdout:\n{text}\nstderr:\n{}",
            expectation.pack,
            stderr(&output)
        );

        let document: Value = serde_json::from_str(&text).unwrap_or_else(|error| {
            panic!("{}: one JSON document: {error}\n{text}", expectation.pack)
        });
        assert_eq!(
            document["errors"].as_u64(),
            Some(expectation.errors as u64),
            "{}: {text}",
            expectation.pack
        );
        assert_eq!(
            document["warnings"].as_u64(),
            Some(expectation.warnings as u64),
            "{}: {text}",
            expectation.pack
        );

        let findings = document["findings"]
            .as_array()
            .unwrap_or_else(|| panic!("{}: a findings list: {text}", expectation.pack));
        let got: Vec<(&str, &str)> = findings
            .iter()
            .map(|finding| {
                for field in ["file", "path", "rule", "severity", "message"] {
                    assert!(
                        finding.get(field).is_some_and(Value::is_string),
                        "{}: finding without `{field}`: {finding}",
                        expectation.pack
                    );
                }
                (
                    finding["file"].as_str().expect("a file"),
                    finding["rule"].as_str().expect("a rule"),
                )
            })
            .collect();
        assert_eq!(
            got, expectation.findings,
            "{}: findings are in §3 order: {text}",
            expectation.pack
        );

        let errors = findings
            .iter()
            .filter(|finding| finding["severity"] == "error")
            .count();
        let warnings = findings
            .iter()
            .filter(|finding| finding["severity"] == "warning")
            .count();
        assert_eq!(
            (errors, warnings),
            (expectation.errors, expectation.warnings),
            "{}: severities match the counts: {text}",
            expectation.pack
        );
    }
}

#[test]
fn the_text_form_names_each_finding_and_summarizes() {
    let pack = fixture_pack("cases");
    let output = run(&[pack.to_str().unwrap(), "--check"]);
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("error: tests/bad-version.keiko:1: "),
        "a finding is `<severity>: <display> [<rule>]`: {text}"
    );
    assert!(
        text.contains("[keiko.case.version]"),
        "a finding names its rule: {text}"
    );
    assert!(
        text.trim_end().ends_with("check: 5 errors, 0 warnings"),
        "the summary closes the report: {text}"
    );

    let pack = demo_pack();
    let output = run(&[pack.to_str().unwrap(), "--check"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert_eq!(
        stdout(&output).trim(),
        "check: 0 errors, 0 warnings",
        "a clean pack prints the summary only"
    );
}

#[test]
fn two_runs_over_the_same_pack_give_byte_identical_json() {
    for name in [
        "demo",
        "cases",
        "warnings",
        "unloaded",
        "flow-unreachable-scene",
        "flow-unreachable-label",
        "flow-no-exit",
    ] {
        let pack = pack(name);
        let first = check_json(&pack);
        let second = check_json(&pack);
        assert_eq!(
            stdout(&first),
            stdout(&second),
            "{name}: two runs are byte-identical"
        );
    }
}

#[test]
fn check_usage_errors_exit_two() {
    let demo = demo_pack();
    let demo = demo.to_str().unwrap();
    for args in [
        vec![demo, "--check", "--take"],
        vec![demo, "--check", "--observe", "opening"],
        vec![demo, "--dev", "--check"],
        vec![demo, "--check", "--keep"],
    ] {
        let output = run(&args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

/// Rule 2: no window, no sidecar and no credential access. The process runs
/// with a canary config folder and a copy of the demo pack, and neither gains a
/// file.
#[test]
fn a_check_never_reads_or_writes_the_users_files() {
    let dir = tempfile::tempdir().unwrap();
    let canary = dir.path().join("appdata");
    std::fs::create_dir_all(canary.join("unmei")).unwrap();
    std::fs::write(canary.join("unmei/credentials"), "sk-nano-canary\n").unwrap();

    let pack = dir.path().join("demo-pack");
    copy_dir(&demo_pack(), &pack);
    let before = sidecars(&pack);

    let output = Command::new(binary())
        .args([pack.to_str().unwrap(), "--check"])
        .env("APPDATA", &canary)
        .env("XDG_CONFIG_HOME", &canary)
        .env_remove("NANOGPT_API_KEY")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));

    assert_eq!(
        sidecars(&pack),
        before,
        "the pack's folder gained no sidecar"
    );
    assert_eq!(
        std::fs::read_to_string(canary.join("unmei/credentials")).unwrap(),
        "sk-nano-canary\n",
        "the credential file is untouched"
    );
    let mut canary_entries = std::fs::read_dir(canary.join("unmei"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    canary_entries.sort();
    assert_eq!(
        canary_entries,
        ["credentials"],
        "the config folder is untouched"
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
