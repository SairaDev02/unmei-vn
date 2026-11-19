//! Spec §3 of the check-and-preview spec: loading reports everything at once.
//! A `pack.ron` or `state.ron` that will not decode no longer ends the load; the
//! rules that read the missing file are skipped, and every other rule runs.

use std::fs;
use std::path::{Path, PathBuf};

use unmei_engine::finding::rules;
use unmei_engine::{load_pack, Finding};

const PACK_RON: &str = r#"(
    name: "all errors",
    entry: "main",
)"#;

const STATE_RON: &str = "(met_ace: false,)";

/// Write the pack's files and return its root. A file the fixture does not list
/// is absent, which is how the broken-file fixtures are made; both load folders
/// exist, so their own absence never adds a finding.
fn pack(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    write_files(dir.path(), files);
    for folder in ["scenes", "npcs"] {
        fs::create_dir_all(dir.path().join(folder)).unwrap();
    }
    let root = dir.path().to_path_buf();
    (dir, root)
}

fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (rel, text) in files {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().expect("a pack file has a folder")).unwrap();
        fs::write(path, text).unwrap();
    }
}

fn rules_of(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule.as_str()).collect()
}

/// Rule 3: an invalid `pack.ron` beside a scene with a syntax error reports
/// both, and invents no manifest finding for the manifest it never decoded.
#[test]
fn a_broken_manifest_and_a_broken_scene_both_report() {
    let (_dir, root) = pack(&[
        ("pack.ron", r#"(name: "all errors", entry: "#),
        ("state.ron", STATE_RON),
        ("scenes/main.unmei", "Narrator: Hello.\n"),
        ("scenes/broken.unmei", "frobnicate the gate\n"),
    ]);

    let errors = load_pack(&root).unwrap_err();

    assert!(
        errors
            .iter()
            .any(|f| f.file == "pack.ron" && f.rule == rules::PACK_FILE_RON),
        "the manifest decode error is reported: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|f| f.file == "scenes/broken.unmei" && f.rule == rules::PACK_SCENE_SYNTAX),
        "the scene behind the broken manifest still reports: {errors:?}"
    );
    assert!(
        !rules_of(&errors).contains(&rules::PACK_MANIFEST),
        "no manifest rule guesses from a manifest that did not decode: {errors:?}"
    );
    assert_eq!(errors.len(), 2, "exactly the two problems: {errors:?}");
}

/// Rule 4: an invalid `state.ron` still reports a `jump` to a missing target,
/// and the rules that would read the missing vocabulary stay silent.
#[test]
fn a_broken_state_file_still_reports_a_jump_to_a_missing_target() {
    let (_dir, root) = pack(&[
        ("pack.ron", PACK_RON),
        ("state.ron", "(met_ace: )"),
        ("scenes/main.unmei", "set undeclared = true\njump nowhere\n"),
    ]);

    let errors = load_pack(&root).unwrap_err();

    assert!(
        errors
            .iter()
            .any(|f| f.file == "state.ron" && f.rule == rules::PACK_FILE_RON),
        "the state decode error is reported: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|f| f.rule == rules::PACK_SCENE_TARGET && f.path == "line 2"),
        "the jump behind the broken state file still reports: {errors:?}"
    );
    assert!(
        !rules_of(&errors).contains(&rules::PACK_STATE),
        "the state rules are skipped, not guessed: {errors:?}"
    );
    assert_eq!(errors.len(), 2, "exactly the two problems: {errors:?}");
}

/// Rule 5: the parser stops at a scene file's first syntax error, so the second
/// one on a later line is not reported.
#[test]
fn a_scene_file_reports_only_its_first_syntax_error() {
    let (_dir, root) = pack(&[
        ("pack.ron", PACK_RON),
        ("state.ron", STATE_RON),
        ("scenes/main.unmei", "Narrator: Fine.\n"),
        ("scenes/broken.unmei", "frobnicate the gate\nendif\n"),
    ]);

    let errors = load_pack(&root).unwrap_err();

    let broken: Vec<&Finding> = errors
        .iter()
        .filter(|f| f.file == "scenes/broken.unmei")
        .collect();
    assert_eq!(
        broken.len(),
        1,
        "one finding per scene file, whatever follows the first error: {errors:?}"
    );
    assert_eq!(broken[0].rule, rules::PACK_SCENE_SYNTAX);
    assert_eq!(
        broken[0].path, "line 1",
        "the reported error is the first one: {errors:?}"
    );
}

/// Spec §3's order, through the real loader: file, then line, then rule id, then
/// message — and the same pack gives the same order on every load, whatever the
/// order the unordered scene and NPC maps hand the pass.
#[test]
fn findings_come_out_in_spec_order_on_every_load() {
    let scene = format!(
        "{}jump nowhere_a\njump nowhere_b\n",
        "Narrator: hi.\n".repeat(8)
    );
    let (_dir, root) = pack(&[
        ("scenes/main.unmei", &scene),
        ("pack.ron", r#"(name: "all errors", entry: "#),
        ("state.ron", STATE_RON),
        (
            "npcs/ace.ron",
            "(id: \"other\", name: \"Ace\", decisions: {})",
        ),
    ]);

    let first = load_pack(&root).unwrap_err();
    let second = load_pack(&root).unwrap_err();

    let order: Vec<(&str, &str, &str)> = first
        .iter()
        .map(|f| (f.file.as_str(), f.path.as_str(), f.rule.as_str()))
        .collect();
    assert_eq!(
        order,
        [
            ("npcs/ace.ron", "", rules::PACK_NPC),
            ("pack.ron", "", rules::PACK_FILE_RON),
            ("scenes/main.unmei", "line 9", rules::PACK_SCENE_TARGET),
            ("scenes/main.unmei", "line 10", rules::PACK_SCENE_TARGET),
        ],
        "file order, then line order in numbers rather than text: {first:?}"
    );
    assert_eq!(
        first, second,
        "the same pack reports the same findings in the same order"
    );
}
