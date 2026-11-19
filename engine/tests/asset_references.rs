//! Spec rules 9, 10 and 12 of the check-and-preview spec: the asset reference
//! list a loaded pack yields, and the one `pack.asset.missing` finding per
//! distinct missing path.
//!
//! The list is the engine's read-only view of what the pack's own files name.
//! Each test builds a synthetic pack in a temp dir, so the reference's file,
//! line and pack-relative path are asserted independently of the demo pack.

use std::fs;

use unmei_engine::finding::rules;
use unmei_engine::{load_pack, AssetKind, Pack};

const ACE_RON: &str = r#"(
    id: "ace",
    name: "Ace",
    decisions: {},
)"#;

/// Write one fixture pack and keep its folder alive for the test.
fn write_pack(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temp dir");
    for (rel, text) in files {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().expect("a parent")).expect("the parent");
        fs::write(path, text).expect("the file");
    }
    dir
}

fn load(dir: &tempfile::TempDir) -> Pack {
    load_pack(dir.path()).expect("the fixture pack loads").pack
}

/// The reference list as `(file, line, kind, path)`, the four facts a caller
/// reads and the order the check reports them in.
fn references(pack: &Pack) -> Vec<(String, Option<usize>, AssetKind, String)> {
    pack.asset_references()
        .into_iter()
        .map(|reference| {
            (
                reference.file,
                reference.line,
                reference.kind,
                reference.path,
            )
        })
        .collect()
}

/// Rule 9: `bg`, `sprite`, `play` and the manifest's `title_backdrop`, each with
/// its file and line — inside both `if` arms and after a `choice` too. `stop`
/// names a sound `play` already names, so it adds no reference.
#[test]
fn the_reference_list_holds_every_kind_in_every_arm() {
    let scene = r#"bg gate
play theme

if met_ace
  sprite ace happy
else
  sprite ace sad
endif

choice
  "Ask" -> ask
  "Leave" -> leave
end

label ask
sprite ace neutral
play late_theme
jump done

label leave
bg night
jump done

label done
stop theme
Narrator: Done.
"#;
    let dir = write_pack(&[
        (
            "pack.ron",
            r#"(
    name: "asset references",
    entry: "main",
    title_backdrop: "title_art",
)
"#,
        ),
        ("state.ron", "(met_ace: false,)\n"),
        ("npcs/ace.ron", ACE_RON),
        ("scenes/main.unmei", scene),
    ]);

    let got = references(&load(&dir));
    let expected: Vec<(String, Option<usize>, AssetKind, String)> = vec![
        // The manifest's reference is on the file as a whole: the loader
        // decodes `pack.ron` without tracking the property's line.
        (
            "pack.ron".into(),
            None,
            AssetKind::Background,
            "assets/backgrounds/title_art.png".into(),
        ),
        (
            "scenes/main.unmei".into(),
            Some(1),
            AssetKind::Background,
            "assets/backgrounds/gate.png".into(),
        ),
        (
            "scenes/main.unmei".into(),
            Some(2),
            AssetKind::Sound,
            "assets/audio/theme.ogg".into(),
        ),
        (
            "scenes/main.unmei".into(),
            Some(5),
            AssetKind::Sprite,
            "assets/sprites/ace/happy.png".into(),
        ),
        (
            "scenes/main.unmei".into(),
            Some(7),
            AssetKind::Sprite,
            "assets/sprites/ace/sad.png".into(),
        ),
        (
            "scenes/main.unmei".into(),
            Some(16),
            AssetKind::Sprite,
            "assets/sprites/ace/neutral.png".into(),
        ),
        (
            "scenes/main.unmei".into(),
            Some(17),
            AssetKind::Sound,
            "assets/audio/late_theme.ogg".into(),
        ),
        (
            "scenes/main.unmei".into(),
            Some(21),
            AssetKind::Background,
            "assets/backgrounds/night.png".into(),
        ),
    ];
    assert_eq!(got, expected);
}

/// Rule 10: one missing sprite referenced three times is one finding, at its
/// first reference in the check's order, and the message counts the references.
#[test]
fn a_missing_sprite_referenced_three_times_is_one_finding_at_its_first_reference() {
    let dir = write_pack(&[
        (
            "pack.ron",
            r#"(
    name: "missing once",
    entry: "main",
)
"#,
        ),
        ("state.ron", "(met_ace: false,)\n"),
        ("npcs/ace.ron", ACE_RON),
        (
            "scenes/main.unmei",
            "bg gate\nsprite ace neutral\njump next\n",
        ),
        (
            "scenes/next.unmei",
            "sprite ace neutral\nsprite ace neutral\nNarrator: Done.\n",
        ),
        // The background is referenced too, and exists, so only the sprite is
        // reported.
        ("assets/backgrounds/gate.png", "not decoded by this rule"),
    ]);

    let findings = load(&dir).missing_asset_findings();
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.file, "scenes/main.unmei");
    assert_eq!(finding.path, "line 2");
    assert_eq!(finding.rule, rules::PACK_ASSET_MISSING);
    assert_eq!(
        finding.message,
        "assets/sprites/ace/neutral.png does not exist; referenced 3 times"
    );
}

/// A missing `title_backdrop` is reported on `pack.ron` with no line, because
/// the manifest names the file without an authored line the loader tracks.
#[test]
fn a_missing_title_backdrop_is_reported_on_the_manifest() {
    let dir = write_pack(&[
        (
            "pack.ron",
            r#"(
    name: "missing backdrop",
    entry: "main",
    title_backdrop: "ghost",
)
"#,
        ),
        ("state.ron", "(met_ace: false,)\n"),
        ("npcs/ace.ron", ACE_RON),
        ("scenes/main.unmei", "Narrator: Done.\n"),
    ]);

    let findings = load(&dir).missing_asset_findings();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].file, "pack.ron");
    assert_eq!(findings[0].path, "");
    assert_eq!(findings[0].rule, rules::PACK_ASSET_MISSING);
}

/// Rule 12: a file in `assets/` that nothing references is not checked, and a
/// referenced file that exists reports nothing.
#[test]
fn a_file_in_assets_that_nothing_references_reports_nothing() {
    let dir = write_pack(&[
        (
            "pack.ron",
            r#"(
    name: "unreferenced",
    entry: "main",
    title_backdrop: "title_art",
)
"#,
        ),
        ("state.ron", "(met_ace: false,)\n"),
        ("npcs/ace.ron", ACE_RON),
        (
            "scenes/main.unmei",
            "bg ground\nplay theme\nsprite ace neutral\nNarrator: Done.\n",
        ),
        ("assets/backgrounds/ground.png", "referenced"),
        ("assets/backgrounds/title_art.png", "referenced"),
        ("assets/sprites/ace/neutral.png", "referenced"),
        ("assets/audio/theme.ogg", "referenced"),
        // Nothing names these four, so the check has no opinion about them.
        ("assets/backgrounds/spare_art.png", "unreferenced"),
        ("assets/sprites/ace/spare_pose.png", "unreferenced"),
        ("assets/audio/spare_track.ogg", "unreferenced"),
        ("assets/README.md", "unreferenced"),
    ]);

    let findings = load(&dir).missing_asset_findings();
    assert_eq!(findings, Vec::new(), "{findings:?}");
}
