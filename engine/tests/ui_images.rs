//! Slice-2 rule 39 (§15) for the plugin's images: every image the manifest
//! declares has a `.png` name and PNG bytes, which is the one image format the
//! player decodes (spec §11).
//!
//! A plugin is read through [`ui::load_from`] with a memory source, so each
//! fixture exercises the pass the check, the boot and the export run. The
//! reference plugin's own clause of rule 39 lives where the embedded bundle is
//! read: `app/tests/ui_plugin_reference.rs`.

use unmei_engine::ui::{self, PluginOutcome};

/// A manifest with no images declared.
const MANIFEST: &str = "(format_version: 1, name: \"Test\")";

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// A manifest declaring one image file.
fn declaring(file: &str) -> String {
    format!("(format_version: 1, name: \"Test\", images: [(file: \"{file}\")])")
}

/// A PNG header claiming 2x2, and no pixel data: the format rule reads the
/// signature alone.
fn png() -> Vec<u8> {
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes
}

/// A JPEG whose start-of-frame segment states 2x2.
fn jpeg() -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8];
    bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 8]);
    bytes.extend_from_slice(&2u16.to_be_bytes());
    bytes.extend_from_slice(&2u16.to_be_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes
}

fn load(files: &[(&str, &[u8])]) -> PluginOutcome {
    let files = files
        .iter()
        .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
        .collect();
    ui::load_from(&ui::MemorySource::new(files))
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

/// Rule 39: a declared PNG passes, and its bytes reach the loaded plugin's
/// manifest unchanged.
#[test]
fn a_declared_png_passes() {
    let manifest = declaring("ornament/arch.png");
    let outcome = load(&[
        ("plugin.ron", manifest.as_bytes()),
        ("ornament/arch.png", &png()),
    ]);
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
    let plugin = outcome.plugin().expect("the plugin loads");
    assert_eq!(plugin.manifest.images.len(), 1);
    assert_eq!(plugin.manifest.images[0].file, "ornament/arch.png");
}

/// Rule 39: an image declared under another extension fails with
/// `ui.image.format`, and the message names the file.
#[test]
fn a_declared_image_with_another_name_is_ui_image_format() {
    let manifest = declaring("ornament/arch.jpg");
    let outcome = load(&[
        ("plugin.ron", manifest.as_bytes()),
        ("ornament/arch.jpg", &jpeg()),
    ]);
    assert_eq!(
        reported(&outcome),
        [("ui/ornament/arch.jpg", String::new(), "ui.image.format")]
    );
    assert!(
        messages(&outcome).contains("ornament/arch.jpg"),
        "the message names the file: {}",
        messages(&outcome)
    );
}

/// Rule 39: a `.png` file whose bytes are a JPEG fails with `ui.image.format`,
/// and the message names the file.
#[test]
fn a_png_name_holding_other_bytes_is_ui_image_format() {
    let manifest = declaring("ornament/arch.png");
    let outcome = load(&[
        ("plugin.ron", manifest.as_bytes()),
        ("ornament/arch.png", &jpeg()),
    ]);
    assert_eq!(
        reported(&outcome),
        [("ui/ornament/arch.png", String::new(), "ui.image.format")]
    );
    assert!(
        messages(&outcome).contains("ornament/arch.png"),
        "the message names the file: {}",
        messages(&outcome)
    );
}

/// Every image the manifest declares is judged, so one bad file does not stop
/// the others from being read, and a good one beside a bad one is not reported.
#[test]
fn each_declared_image_is_judged_and_a_good_one_is_not_reported() {
    let manifest = "(format_version: 1, name: \"Test\", images: [\
        (file: \"ornament/good.png\"), (file: \"ornament/bad.png\")])";
    let outcome = load(&[
        ("plugin.ron", manifest.as_bytes()),
        ("ornament/good.png", &png()),
        ("ornament/bad.png", &jpeg()),
    ]);
    assert_eq!(
        reported(&outcome),
        [("ui/ornament/bad.png", String::new(), "ui.image.format")]
    );
}

/// The extension is read in either case, as the font extensions are.
#[test]
fn a_png_name_is_read_in_either_case() {
    let manifest = declaring("ornament/arch.PNG");
    let outcome = load(&[
        ("plugin.ron", manifest.as_bytes()),
        ("ornament/arch.PNG", &png()),
    ]);
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
}

/// An image the manifest does not declare is never read by this rule: only the
/// declarations say which files are the plugin's images.
#[test]
fn an_undeclared_file_is_not_a_format_finding() {
    let outcome = load(&[
        ("plugin.ron", MANIFEST.as_bytes()),
        ("ornament/arch.jpg", &jpeg()),
    ]);
    assert_eq!(reported(&outcome), [], "{}", messages(&outcome));
}
