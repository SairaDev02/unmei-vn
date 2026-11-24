//! Slice-1 rules 8 and 11 of the UI plugin spec (§15) for the plugin's fonts: a
//! shipped font file needs a manifest declaration and a license text, and a
//! typography token names one of the five embedded faces or a declared face
//! (§11).
//!
//! A plugin is read through [`ui::load_from`] with a memory source, so each
//! fixture exercises the pass the check, the boot and the export run; one
//! fixture reads a folder through [`ui::load`], because that is the source a
//! player's pack holds.

use std::fs;
use std::path::Path;

use unmei_engine::ui::{self, PluginOutcome};

/// A manifest with no fonts declared.
const MANIFEST: &str = "(format_version: 1, name: \"Test\")";

/// The bytes behind every fixture font and license file. No font rule reads
/// them: a declaration and its license text's presence are the whole rule.
const BODY: &str = "never read";

/// A manifest declaring one face of `fonts/plex.ttf`.
fn declaring(family: &str, weight: u16, license: &str, license_file: &str) -> String {
    format!(
        "(format_version: 1, name: \"Test\", fonts: [(family: \"{family}\", \
         weight: {weight}, file: \"fonts/plex.ttf\", license: \"{license}\", \
         license_file: \"{license_file}\")])"
    )
}

/// A token file whose `body` typography token names one face.
fn typography(face: &str, weight: u16) -> String {
    format!(
        r#"(
    typography: {{
        "body": (face: "{face}", weight: {weight}, size_px: 25.0, line_height_px: 38.0),
    }},
)"#
    )
}

fn load(files: &[(&str, &str)]) -> PluginOutcome {
    ui::load_from(&ui::MemorySource::from_text(files))
}

/// `(file, path, rule)` for each finding: the shape rule 12 fixes.
fn reported(outcome: &PluginOutcome) -> Vec<(&str, String, &str)> {
    outcome
        .findings()
        .iter()
        .map(|finding| {
            (
                finding.file.as_str(),
                finding.path.to_string(),
                finding.rule.as_str(),
            )
        })
        .collect()
}

fn messages(outcome: &PluginOutcome) -> String {
    outcome
        .findings()
        .iter()
        .map(|finding| finding.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("a parent")).expect("the parent");
    fs::write(path, text).expect("the file");
}

/// A file with a font's extension and no declaration naming it is the plugin
/// shipping a font nobody licensed.
#[test]
fn a_font_file_no_declaration_names_is_ui_font_license() {
    let outcome = load(&[("plugin.ron", MANIFEST), ("fonts/plex.ttf", BODY)]);
    assert_eq!(
        reported(&outcome),
        [("ui/fonts/plex.ttf", String::new(), "ui.font.license")]
    );
}

/// The extension is the whole test: the six font extensions mark a file
/// wherever it sits in the folder, and any other name marks none.
#[test]
fn every_font_extension_marks_a_file_and_nothing_else_does() {
    let outcome = load(&[
        ("plugin.ron", MANIFEST),
        ("fonts/a.ttf", BODY),
        ("fonts/b.otf", BODY),
        ("fonts/c.ttc", BODY),
        ("fonts/d.woff", BODY),
        ("fonts/e.woff2", BODY),
        ("fonts/f.eot", BODY),
        ("fonts/G.TTF", BODY),
        ("fonts/readme.txt", BODY),
        ("fonts/plex", BODY),
    ]);
    let files: Vec<&str> = outcome
        .findings()
        .iter()
        .map(|finding| finding.file.as_str())
        .collect();
    assert_eq!(
        files,
        [
            "ui/fonts/G.TTF",
            "ui/fonts/a.ttf",
            "ui/fonts/b.otf",
            "ui/fonts/c.ttc",
            "ui/fonts/d.woff",
            "ui/fonts/e.woff2",
            "ui/fonts/f.eot",
        ],
        "{}",
        messages(&outcome)
    );
    for finding in outcome.findings() {
        assert_eq!(finding.rule, "ui.font.license", "{finding}");
    }
}

/// A declaration that names a license text the folder does not hold fails at
/// the field that names it.
#[test]
fn a_declaration_whose_license_text_is_missing_is_ui_font_license() {
    let manifest = declaring("Inter", 400, "OFL-1.1", "fonts/OFL.txt");
    let outcome = load(&[("plugin.ron", &manifest), ("fonts/plex.ttf", BODY)]);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/plugin.ron",
            "fonts[0].license_file".to_string(),
            "ui.font.license"
        )]
    );
    assert!(
        messages(&outcome).contains("fonts/OFL.txt"),
        "the message names the file: {}",
        messages(&outcome)
    );
}

/// A declaration naming no license licenses nothing, whether the field is empty
/// or only spaces.
#[test]
fn a_declaration_with_no_license_is_ui_font_license() {
    for license in ["", "  "] {
        let manifest = declaring("Inter", 400, license, "fonts/OFL.txt");
        let outcome = load(&[
            ("plugin.ron", &manifest),
            ("fonts/plex.ttf", BODY),
            ("fonts/OFL.txt", BODY),
        ]);
        assert_eq!(
            reported(&outcome),
            [(
                "ui/plugin.ron",
                "fonts[0].license".to_string(),
                "ui.font.license"
            )],
            "license {license:?}"
        );
    }
}

/// A declaration with no `license` field at all is the manifest's own shape, so
/// the manifest parser owns it: every field of a declaration is required (§2).
/// The check therefore reports `ui.parse`, not `ui.font.license`.
#[test]
fn a_declaration_without_a_license_field_is_ui_parse() {
    let manifest = "(format_version: 1, fonts: [(family: \"Inter\", weight: 400, \
                    file: \"fonts/plex.ttf\", license_file: \"fonts/OFL.txt\")])";
    let outcome = load(&[("plugin.ron", manifest), ("fonts/plex.ttf", BODY)]);
    assert_eq!(
        reported(&outcome),
        [("ui/plugin.ron", String::new(), "ui.parse")]
    );
}

/// The five embedded faces are addressable by family and weight with no font
/// shipped (R19, R20).
#[test]
fn the_five_embedded_faces_load_without_a_shipped_font() {
    let tokens = r#"(
    typography: {
        "body": (face: "IBM Plex Sans", weight: 400, size_px: 25.0, line_height_px: 38.0),
        "semi-bold": (face: "IBM Plex Sans", weight: 600, size_px: 25.0, line_height_px: 38.0),
        "bold": (face: "IBM Plex Sans", weight: 700, size_px: 25.0, line_height_px: 38.0),
        "mono": (face: "Fira Mono", weight: 400, size_px: 18.0, line_height_px: 24.0),
        "mono-medium": (face: "Fira Mono", weight: 500, size_px: 18.0, line_height_px: 24.0),
    },
)"#;
    let outcome = load(&[("plugin.ron", MANIFEST), ("tokens.ron", tokens)]);
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
    let plugin = outcome.plugin().expect("the plugin loads");
    assert_eq!(plugin.tokens.typography.len(), 5);
}

/// A family the plugin ships passes once it is declared.
#[test]
fn a_declared_face_loads() {
    let manifest = declaring("Inter", 400, "OFL-1.1", "fonts/OFL.txt");
    let tokens = typography("Inter", 400);
    let outcome = load(&[
        ("plugin.ron", &manifest),
        ("tokens.ron", &tokens),
        ("fonts/plex.ttf", BODY),
        ("fonts/OFL.txt", BODY),
    ]);
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
    let plugin = outcome.plugin().expect("the plugin loads");
    assert_eq!(plugin.tokens.typography["body"].face, "Inter");
}

/// A family the plugin neither embeds nor ships has no font the player can
/// draw, so the token that names it fails at the family it names.
#[test]
fn a_face_that_is_neither_embedded_nor_declared_is_ui_font_unknown() {
    let tokens = typography("Inter", 400);
    let outcome = load(&[("plugin.ron", MANIFEST), ("tokens.ron", &tokens)]);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/tokens.ron",
            "typography.body.face".to_string(),
            "ui.font.unknown"
        )]
    );
    assert!(
        messages(&outcome).contains("Inter"),
        "the message names the family: {}",
        messages(&outcome)
    );
}

/// The face is the family and its weight: an embedded family at another weight
/// is shipped by nobody.
#[test]
fn another_weight_of_an_embedded_family_is_ui_font_unknown() {
    for (face, weight) in [("IBM Plex Sans", 500), ("Fira Mono", 700)] {
        let tokens = typography(face, weight);
        let outcome = load(&[("plugin.ron", MANIFEST), ("tokens.ron", &tokens)]);
        assert_eq!(
            reported(&outcome),
            [(
                "ui/tokens.ron",
                "typography.body.face".to_string(),
                "ui.font.unknown"
            )],
            "{face} at {weight}"
        );
    }
}

/// A declared family is declared at one weight, not all of them.
#[test]
fn a_declared_family_at_another_weight_is_ui_font_unknown() {
    let manifest = declaring("Inter", 400, "OFL-1.1", "fonts/OFL.txt");
    let tokens = typography("Inter", 500);
    let outcome = load(&[
        ("plugin.ron", &manifest),
        ("tokens.ron", &tokens),
        ("fonts/plex.ttf", BODY),
        ("fonts/OFL.txt", BODY),
    ]);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/tokens.ron",
            "typography.body.face".to_string(),
            "ui.font.unknown"
        )]
    );
}

/// A declaration names a face and the file that ships it. A declaration whose
/// file the folder does not hold ships nothing, so a token naming it still asks
/// for a face the player cannot draw.
#[test]
fn a_declared_face_with_no_font_file_is_ui_font_unknown() {
    let manifest = declaring("Inter", 400, "OFL-1.1", "fonts/OFL.txt");
    let tokens = typography("Inter", 400);
    let outcome = load(&[
        ("plugin.ron", &manifest),
        ("tokens.ron", &tokens),
        ("fonts/OFL.txt", BODY),
    ]);
    assert_eq!(
        reported(&outcome),
        [(
            "ui/tokens.ron",
            "typography.body.face".to_string(),
            "ui.font.unknown"
        )]
    );
}

/// A folder source reaches the same font rules as a memory source, so the check
/// and the boot accept exactly the same plugins (R24).
#[test]
fn a_folder_source_applies_the_same_font_rules() {
    let dir = tempfile::tempdir().expect("a temp dir");
    write(dir.path(), "ui/plugin.ron", MANIFEST);
    write(dir.path(), "ui/fonts/plex.ttf", BODY);
    let outcome = ui::load(dir.path());
    assert_eq!(
        reported(&outcome),
        [("ui/fonts/plex.ttf", String::new(), "ui.font.license")]
    );
}
