//! Slice-1 rules 1–4 of the UI plugin spec (§15): the plugin outcome a pack's
//! `ui/` folder produces, the format version, the folder's confinement and the
//! file-level caps of §7.
//!
//! Each fixture is a synthetic pack in a temp dir, so a rule is asserted
//! independently of the demo pack and of a shipped plugin. A pack that holds
//! `ui/` always loads: the plugin's outcome is the pack's own field, never a
//! load error (§8).

use std::fs;
use std::path::Path;

use unmei_engine::ui::{self, PluginOutcome};
use unmei_engine::Location;

const PACK_RON: &str = "(name: \"ui plugin\", entry: \"main\")";
const STATE_RON: &str = "(met_ace: false,)";
const MAIN_SCENE: &str = "Narrator: A clean line.\n";

/// A manifest with everything but the fields the test is about left out.
const MANIFEST: &str = "(format_version: 1, name: \"Test\")";

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Write one fixture pack: a pack that loads on its own, plus `files`.
fn pack(files: &[(&str, &[u8])]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temp dir");
    for (rel, text) in [
        ("pack.ron", PACK_RON),
        ("state.ron", STATE_RON),
        ("scenes/main.unmei", MAIN_SCENE),
    ] {
        write(dir.path(), rel, text.as_bytes());
    }
    fs::create_dir_all(dir.path().join("npcs")).expect("the npcs folder");
    for (rel, bytes) in files {
        write(dir.path(), rel, bytes);
    }
    dir
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("a parent")).expect("the parent");
    fs::write(path, bytes).expect("the file");
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

/// A file filled to `size` bytes without writing them: the cap reads sizes, and
/// no rule reads a file this test does not hand to one.
fn sized(root: &Path, rel: &str, size: u64) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("a parent")).expect("the parent");
    fs::File::create(path)
        .expect("the file")
        .set_len(size)
        .expect("the size");
}

/// A PNG header claiming `side`x`side`, and no pixel data. The image cap reads
/// the header only, so a file behind this header trips it when it is read.
fn png_header(side: u32) -> Vec<u8> {
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&side.to_be_bytes());
    bytes.extend_from_slice(&side.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes
}

/// A JPEG header whose start-of-frame segment states `side`x`side`. The
/// dimensions are 16-bit, so every side the cap cares about fits.
fn jpeg_header(side: u32) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8];
    bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 8]);
    bytes.extend_from_slice(&(side as u16).to_be_bytes());
    bytes.extend_from_slice(&(side as u16).to_be_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes
}

/// A lossless WEBP header whose canvas is `side`x`side`.
fn webp_header(side: u32) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(b"WEBPVP8L");
    bytes.extend_from_slice(&5u32.to_le_bytes());
    bytes.push(0x2F);
    let bits = (side - 1) | ((side - 1) << 14);
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes
}

/// A BMP header whose 40-byte DIB header states `side`x`side`.
fn bmp_header(side: u32) -> Vec<u8> {
    let mut bytes = b"BM".to_vec();
    bytes.extend_from_slice(&[0; 12]);
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&side.to_le_bytes());
    bytes.extend_from_slice(&side.to_le_bytes());
    bytes.extend_from_slice(&[0; 24]);
    bytes
}

/// One image header per format the source recognises, each claiming
/// `side`x`side` with no pixel data.
fn image_headers(side: u32) -> [(&'static str, Vec<u8>); 4] {
    [
        ("big.png", png_header(side)),
        ("big.jpg", jpeg_header(side)),
        ("big.webp", webp_header(side)),
        ("big.bmp", bmp_header(side)),
    ]
}

fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }
}

/// The same, for a link to a folder, which Windows marks as a directory link.
fn directory_link(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }
}

/// A pack with no `ui/` has no plugin, which is not a failure.
#[test]
fn a_pack_with_no_ui_folder_has_no_plugin() {
    let dir = pack(&[]);
    let loaded = unmei_engine::load_pack(dir.path()).expect("the pack loads");
    assert_eq!(loaded.plugin, PluginOutcome::Absent);
}

/// A `ui/` with no `plugin.ron` is a failed plugin, and the pack still loads.
#[test]
fn a_ui_folder_without_a_manifest_fails_the_plugin_only() {
    let dir = pack(&[("ui/tokens.ron", b"(colors: {})")]);
    let loaded = unmei_engine::load_pack(dir.path()).expect("the pack still loads");
    assert_eq!(
        reported(&loaded.plugin),
        [("ui/plugin.ron", String::new(), "ui.file.read")]
    );
    assert!(loaded.plugin.plugin().is_none(), "no plugin is loaded");
}

/// Version 1 is the only version this build reads.
#[test]
fn a_version_one_manifest_loads() {
    let dir = pack(&[("ui/plugin.ron", MANIFEST.as_bytes())]);
    let outcome = ui::load(dir.path());
    assert_eq!(reported(&outcome), [], "a clean plugin reports nothing");
    assert_eq!(
        outcome.plugin().map(|plugin| plugin.manifest.name.as_str()),
        Some("Test")
    );
}

/// Another version fails with `ui.version`, and the message names version 1.
#[test]
fn a_version_two_manifest_names_the_expected_version() {
    let dir = pack(&[("ui/plugin.ron", b"(format_version: 2, name: \"Test\")")]);
    let outcome = ui::load(dir.path());
    assert_eq!(
        reported(&outcome),
        [("ui/plugin.ron", "format_version".to_string(), "ui.version")]
    );
    assert!(
        messages(&outcome).contains("version 1"),
        "the message names version 1: {}",
        messages(&outcome)
    );
}

/// A manifest that is not RON is `ui.parse`, not a load error.
#[test]
fn a_manifest_that_is_not_ron_is_ui_parse() {
    let dir = pack(&[("ui/plugin.ron", b"format_version = 1")]);
    let outcome = ui::load(dir.path());
    assert_eq!(
        reported(&outcome),
        [("ui/plugin.ron", String::new(), "ui.parse")]
    );
}

/// `..` is an escape, checked before anything reads the file it names. The file
/// behind the path is bait: an image the cap would reject, so a plugin that
/// read it would report `ui.cap` beside the escape.
#[test]
fn a_dot_dot_path_escapes() {
    let bait = png_header(9000);
    let dir = pack(&[
        (
            "ui/plugin.ron",
            b"(format_version: 1, images: [(file: \"../pit/secret.png\")])",
        ),
        ("pit/secret.png", &bait),
    ]);
    let outcome = ui::load(dir.path());
    assert_eq!(
        reported(&outcome),
        [(
            "ui/plugin.ron",
            "images[0].file".to_string(),
            "ui.path.escape"
        )],
        "only the escape is reported, so the file it named was never read"
    );
}

/// An absolute path is an escape on every platform.
#[test]
fn an_absolute_path_escapes() {
    let dir = pack(&[(
        "ui/plugin.ron",
        b"(format_version: 1, images: [(file: \"/etc/secret.png\")])",
    )]);
    let outcome = ui::load(dir.path());
    assert_eq!(
        reported(&outcome),
        [(
            "ui/plugin.ron",
            "images[0].file".to_string(),
            "ui.path.escape"
        )]
    );
}

/// A font path is confined like an image path: confinement belongs to the
/// source, not to one manifest field.
#[test]
fn a_font_path_escapes_by_the_same_rule() {
    let dir = pack(&[(
        "ui/plugin.ron",
        b"(format_version: 1, fonts: [(family: \"F\", weight: 400, \
          file: \"../f.ttf\", license: \"OFL-1.1\", license_file: \"../f.txt\")])",
    )]);
    let outcome = ui::load(dir.path());
    let mut pairs = reported(&outcome);
    pairs.sort();
    assert_eq!(
        pairs,
        [
            (
                "ui/plugin.ron",
                "fonts[0].file".to_string(),
                "ui.path.escape"
            ),
            (
                "ui/plugin.ron",
                "fonts[0].license_file".to_string(),
                "ui.path.escape"
            ),
        ]
    );
}

/// A symlink out of `ui/` is an escape, and the file behind it is not read.
#[test]
fn a_symlink_out_of_the_folder_escapes() {
    let bait = png_header(9000);
    let dir = pack(&[("pit/secret.png", &bait)]);
    let secret = dir.path().join("pit/secret.png");
    let link = dir.path().join("ui/link.png");
    fs::create_dir_all(link.parent().expect("a parent")).expect("the parent");
    if let Err(error) = symlink(&secret, &link) {
        // Windows without Developer Mode or the symlink privilege cannot make
        // one; the CI gate and every other platform run this case for real.
        eprintln!("skipped: the platform refused a symlink: {error}");
        return;
    }
    write(dir.path(), "ui/plugin.ron", MANIFEST.as_bytes());

    let outcome = ui::load(dir.path());
    assert_eq!(
        reported(&outcome),
        [("ui/link.png", String::new(), "ui.path.escape")],
        "only the escape is reported, so the file behind it was never read"
    );
}

/// A `ui/` that is a link to a folder outside the pack is an escape, and
/// nothing inside its target is read (spec §7, rule 28). The target holds a
/// valid plugin, so a build that followed the link would report its rules
/// instead of one finding.
#[test]
fn a_ui_folder_that_links_out_of_the_pack_fails() {
    let outside = tempfile::tempdir().expect("a temp dir");
    write(
        outside.path(),
        "plugin.ron",
        b"(format_version: 1, name: \"Elsewhere\")",
    );
    write(outside.path(), "tokens.ron", b"(colors: {})");
    let dir = pack(&[]);
    if let Err(error) = directory_link(outside.path(), &dir.path().join("ui")) {
        // Windows without Developer Mode or the symlink privilege cannot make
        // one; the CI gate and every other platform run this case for real.
        eprintln!("skipped: the platform refused a symlink: {error}");
        return;
    }

    let outcome = ui::load(dir.path());
    assert_eq!(
        reported(&outcome),
        [("ui/", String::new(), "ui.path.escape")],
        "the folder's own escape is the only finding, so its target was never listed"
    );
}

/// A folder far past the file cap fails with `ui.cap`, and the walk reports the
/// count it stopped at rather than the count the folder holds (spec §7,
/// rule 29).
#[test]
fn the_walk_stops_one_file_past_the_file_count_cap() {
    let dir = pack(&[("ui/plugin.ron", MANIFEST.as_bytes())]);
    for index in 0..10_000 {
        write(dir.path(), &format!("ui/bulk-{index:05}.txt"), b"");
    }

    let outcome = ui::load(dir.path());
    assert_eq!(
        reported(&outcome),
        [("ui/", String::new(), "ui.cap")],
        "the cap ends the walk, so no later rule sees the folder"
    );
    let message = messages(&outcome);
    let stopped_at = ui::MAX_FILES + 1;
    assert!(
        message.contains(&format!("at least {stopped_at}")),
        "the walk lists {stopped_at} files and stops, not all 10,001: {message}"
    );
}

/// One file over the folder's file-count cap fails it, and one fewer passes.
#[test]
fn the_file_count_cap_passes_at_the_limit_and_fails_one_over() {
    let dir = pack(&[("ui/plugin.ron", MANIFEST.as_bytes())]);
    for index in 1..ui::MAX_FILES {
        sized(dir.path(), &format!("ui/extra-{index:04}.txt"), 0);
    }
    assert_eq!(
        reported(&ui::load(dir.path())),
        [],
        "{} files is the cap",
        ui::MAX_FILES
    );

    sized(dir.path(), "ui/one-more.txt", 0);
    let outcome = ui::load(dir.path());
    assert_eq!(reported(&outcome), [("ui/", String::new(), "ui.cap")]);
    let message = messages(&outcome);
    assert!(
        message.contains(&ui::MAX_FILES.to_string())
            && message.contains(&(ui::MAX_FILES + 1).to_string()),
        "the message names the cap and the count found: {message}"
    );
}

/// One file over the per-file cap fails it, and the file at the cap passes.
#[test]
fn the_file_size_cap_passes_at_the_limit_and_fails_one_byte_over() {
    let dir = pack(&[
        ("ui/plugin.ron", MANIFEST.as_bytes()),
        ("ui/padding.bin", b""),
    ]);
    sized(dir.path(), "ui/padding.bin", ui::MAX_FILE_BYTES);
    assert_eq!(reported(&ui::load(dir.path())), [], "the cap is inclusive");

    sized(dir.path(), "ui/padding.bin", ui::MAX_FILE_BYTES + 1);
    let outcome = ui::load(dir.path());
    assert_eq!(
        reported(&outcome),
        [("ui/padding.bin", String::new(), "ui.cap")]
    );
    let message = messages(&outcome);
    assert!(
        message.contains(&ui::MAX_FILE_BYTES.to_string()),
        "the message names the cap: {message}"
    );
}

/// The folder's total is a cap of its own, beside the per-file one.
#[test]
fn the_total_size_cap_passes_at_the_limit_and_fails_one_byte_over() {
    let dir = pack(&[("ui/plugin.ron", MANIFEST.as_bytes())]);
    let manifest = u64::try_from(MANIFEST.len()).expect("a small manifest");
    for index in 0..3 {
        sized(
            dir.path(),
            &format!("ui/big-{index}.bin"),
            ui::MAX_FILE_BYTES,
        );
    }
    let remainder = ui::MAX_TOTAL_BYTES - 3 * ui::MAX_FILE_BYTES - manifest;
    sized(dir.path(), "ui/rest.bin", remainder);
    assert_eq!(
        reported(&ui::load(dir.path())),
        [],
        "the total is the cap exactly"
    );

    sized(dir.path(), "ui/rest.bin", remainder + 1);
    let outcome = ui::load(dir.path());
    assert_eq!(reported(&outcome), [("ui/", String::new(), "ui.cap")]);
    let message = messages(&outcome);
    assert!(
        message.contains(&ui::MAX_TOTAL_BYTES.to_string()),
        "the message names the cap: {message}"
    );
}

/// A shipped image over the image cap fails it; the same header at the cap
/// passes. Every format the source recognises is measured, and the cap reads
/// the header, so no pixel data is needed here.
#[test]
fn the_image_cap_passes_at_the_limit_and_fails_one_pixel_over() {
    let one_over = image_headers(ui::MAX_IMAGE_SIDE + 1);
    for ((name, at_the_cap), (_, over)) in
        image_headers(ui::MAX_IMAGE_SIDE).into_iter().zip(one_over)
    {
        let path = format!("ui/{name}");
        let dir = pack(&[("ui/plugin.ron", MANIFEST.as_bytes()), (&path, &at_the_cap)]);
        assert_eq!(
            reported(&ui::load(dir.path())),
            [],
            "{name} at the cap passes"
        );

        write(dir.path(), &path, &over);
        let outcome = ui::load(dir.path());
        assert_eq!(
            reported(&outcome),
            [(&*path, String::new(), "ui.cap")],
            "{name} one pixel over fails"
        );
        let message = messages(&outcome);
        assert!(
            message.contains(&ui::MAX_IMAGE_SIDE.to_string()),
            "the message names the cap: {message}"
        );
    }
}

/// The embedded reference plugin and the export read through the same pass, so
/// a memory source accepts and rejects exactly what a folder does.
#[test]
fn a_memory_source_runs_the_same_rules() {
    let clean = ui::MemorySource::from_text(&[("plugin.ron", MANIFEST)]);
    assert_eq!(reported(&ui::load_from(&clean)), []);
    assert!(ui::load_from(&clean).plugin().is_some());

    let bad_version = ui::MemorySource::from_text(&[("plugin.ron", "(format_version: 2)")]);
    assert_eq!(
        reported(&ui::load_from(&bad_version)),
        [("ui/plugin.ron", "format_version".to_string(), "ui.version")]
    );
}

/// A source that lists a path outside its root is refused like a manifest path:
/// confinement is a rule of the pass, not a promise of one source.
#[test]
fn a_memory_source_path_outside_the_root_escapes() {
    let source = ui::MemorySource::from_text(&[("../plugin.ron", MANIFEST)]);
    assert_eq!(
        reported(&ui::load_from(&source)),
        [("ui/../plugin.ron", String::new(), "ui.path.escape")]
    );
}

/// Rule 12: every plugin finding carries a `ui.*` id and a path in its file.
#[test]
fn every_finding_carries_a_ui_rule_and_a_location() {
    let dir = pack(&[(
        "ui/plugin.ron",
        b"(format_version: 1, images: [(file: \"../a.png\")], \
          fonts: [(family: \"F\", weight: 400, file: \"../f.ttf\", \
          license: \"OFL-1.1\", license_file: \"../l.txt\")])",
    )]);
    let outcome = ui::load(dir.path());
    assert_eq!(outcome.findings().len(), 3, "{}", messages(&outcome));
    for finding in outcome.findings() {
        assert!(
            finding.rule.starts_with("ui."),
            "{}: {}",
            finding.rule,
            finding
        );
        assert!(finding.file.starts_with("ui/"), "{}", finding);
        match &finding.path {
            Location::Field(path) => assert!(!path.is_empty(), "{finding}"),
            Location::File | Location::Line(_) => {
                panic!("a plugin finding names its field: {finding}")
            }
        }
    }
}
