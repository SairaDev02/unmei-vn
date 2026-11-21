//! `unmei-play --export-ui` at the binary (UI plugin spec §1, §15 rule 23).
//!
//! The export is the only writer of a pack's `ui/` folder, so these tests run the
//! real binary over temp packs: what it writes, what it refuses, and that the
//! written plugin then passes the pack's own `--check`. The written tree is
//! compared with [`reference_plugin::FILES`], so a second copy of the plugin
//! cannot appear here without failing.
//!
//! A refusal must leave the folder untouched byte for byte, so the non-empty case
//! snapshots the whole tree. Every run is isolated from the user's files: the
//! export needs no credential and writes no sidecar, which the first test
//! observes through a canary config folder and the pack's untouched siblings.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use unmei_app::reference_plugin;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_unmei-play"))
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

/// A pack that loads cleanly and has no `ui/` folder: the one shape the export
/// starts from.
fn write_pack(root: &Path) {
    for (rel, text) in [
        ("pack.ron", "(name: \"export ui\", entry: \"main\")"),
        ("state.ron", "(met_ace: false,)"),
        ("scenes/main.unmei", "Narrator: A clean line.\n"),
    ] {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::create_dir_all(root.join("npcs")).unwrap();
}

/// Every regular file under `root`, as `/`-separated relative paths, with its
/// bytes. Following the folders rather than a known list is what makes "changed
/// nothing" a claim about the whole tree.
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("below the root")
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// The embedded reference plugin as the exported tree must carry it.
fn reference_files() -> BTreeMap<String, Vec<u8>> {
    reference_plugin::FILES
        .iter()
        .map(|(path, text)| ((*path).to_string(), text.as_bytes().to_vec()))
        .collect()
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

#[test]
fn an_export_writes_the_reference_plugin_and_the_pack_then_passes_the_check() {
    let dir = tempfile::tempdir().unwrap();
    let canary = dir.path().join("appdata");
    std::fs::create_dir_all(canary.join("unmei")).unwrap();
    std::fs::write(canary.join("unmei/credentials"), "sk-nano-canary\n").unwrap();

    let pack = dir.path().join("pack");
    write_pack(&pack);
    let before = sidecars(&pack);

    let output = Command::new(binary())
        .args([pack.to_str().unwrap(), "--export-ui"])
        .env("APPDATA", &canary)
        .env("XDG_CONFIG_HOME", &canary)
        .env_remove("NANOGPT_API_KEY")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    let written = pack.join("ui");
    assert!(
        stdout(&output).contains(&written.canonicalize().unwrap().display().to_string()),
        "the confirmation names the folder: {}",
        stdout(&output)
    );

    assert_eq!(
        files(&pack.join("ui")),
        reference_files(),
        "the export writes the embedded reference plugin, file for file"
    );
    assert_eq!(
        sidecars(&pack),
        before,
        "the export wrote no sidecar beside the pack"
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

    // Rule 23's real claim: the written plugin is a plugin the engine accepts.
    let check = run(&[pack.to_str().unwrap(), "--check", "--format", "json"]);
    let text = stdout(&check);
    assert_eq!(check.status.code(), Some(0), "{text}");
    let document: Value =
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("one JSON document: {error}"));
    assert_eq!(document["errors"].as_u64(), Some(0), "{text}");
    assert_eq!(document["warnings"].as_u64(), Some(0), "{text}");
    assert_eq!(
        document["findings"].as_array().map(Vec::len),
        Some(0),
        "the exported plugin reports no finding: {text}"
    );
}

#[test]
fn an_export_over_a_non_empty_ui_exits_two_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("pack");
    write_pack(&pack);
    let ui = pack.join("ui");
    std::fs::create_dir_all(&ui).unwrap();
    std::fs::write(
        ui.join("plugin.ron"),
        "(format_version: 1, name: \"Mine\")\n",
    )
    .unwrap();
    std::fs::write(ui.join("notes.txt"), "keep me").unwrap();
    let before = files(&ui);

    let output = run(&[pack.to_str().unwrap(), "--export-ui"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    let reported = stderr(&output);
    assert!(reported.contains("unmei-play:"), "{reported}");
    assert!(reported.contains("Usage:"), "{reported}");
    let named = ui.canonicalize().expect("the refused folder exists");
    assert!(
        reported.contains(&named.display().to_string()),
        "the usage error names the folder: {reported}"
    );
    assert_eq!(
        files(&ui),
        before,
        "the refused folder is byte for byte unchanged"
    );
}

#[test]
fn an_export_into_an_existing_empty_ui_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("pack");
    write_pack(&pack);
    std::fs::create_dir(pack.join("ui")).unwrap();

    let output = run(&[pack.to_str().unwrap(), "--export-ui"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        files(&pack.join("ui")),
        reference_files(),
        "an empty ui/ receives the whole plugin"
    );
}

/// A `PACK_DIR` that does not exist exits 2 and creates nothing: export makes
/// the plugin folder, never the pack (spec §1, rule 32).
#[test]
fn an_export_into_a_missing_pack_exits_two_and_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("typoed-pack");

    let output = run(&[pack.to_str().unwrap(), "--export-ui"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(!pack.exists(), "a mistyped path makes no folder");
    let reported = stderr(&output);
    assert!(
        reported.contains("existing pack folder"),
        "the usage error says what is missing: {reported}"
    );
    assert!(reported.contains("Usage:"), "{reported}");
}

#[test]
fn export_ui_with_another_mode_or_with_dev_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("pack");
    write_pack(&pack);
    let pack = pack.to_str().unwrap();
    for args in [
        vec![pack, "--export-ui", "--check"],
        vec![pack, "--check", "--export-ui"],
        vec![pack, "--export-ui", "--preview", "out"],
        vec![pack, "--preview", "out", "--export-ui"],
        vec![pack, "--export-ui", "--take"],
        vec![pack, "--take", "--export-ui"],
        vec![pack, "--observe", "one", "--export-ui"],
        vec![pack, "--explore", "--export-ui"],
        vec![pack, "--dev", "--export-ui"],
        vec![pack, "--export-ui", "--dev"],
    ] {
        let output = run(&args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?}: {}",
            stderr(&output)
        );
        assert!(stdout(&output).is_empty(), "{args:?}: {}", stdout(&output));
        assert!(
            stderr(&output).contains("unmei-play:"),
            "{args:?}: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains("cannot be combined"),
            "{args:?} names the conflict: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains("Usage:"),
            "{args:?}: {}",
            stderr(&output)
        );
    }
    assert!(
        !dir.path().join("pack/ui").exists(),
        "a refused launch writes nothing"
    );
}

#[test]
fn the_help_lists_the_export() {
    let output = run(&["--help"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let help = stdout(&output);
    for needle in [
        "--export-ui",
        "unmei-play [PACK_DIR] --export-ui",
        "Export:",
    ] {
        assert!(
            help.contains(needle),
            "{needle:?} missing from help:\n{help}"
        );
    }
}
