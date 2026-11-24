//! The embedded reference plugin (#110): the files in `app/reference-plugin/`,
//! the build-time copy the player embeds, and the engine path both travel.
//!
//! Two contracts live here. The folder and [`reference_plugin::FILES`] hold the
//! same files byte for byte, so an edit to one cannot silently leave the other
//! behind. And the embedded bundle passes every static rule through
//! [`unmei_engine::ui::load_from`], which is the function a pack's `ui/` folder,
//! the check and the boot reach (UI plugin spec §8, R24).
//!
//! The test is the only reader of the reference plugin in this slice: no
//! renderer draws it yet. Its findings name the plugin file and the RON field or
//! node path, so a failure here says what to fix rather than only that the
//! reference look is no longer valid.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use unmei_app::reference_plugin;
use unmei_engine::ui::{self, PluginOutcome};

/// The authored plugin folder, beside `src/` and `tests/`.
fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-plugin")
}

/// Every regular file under `root`, as `/`-separated paths relative to it, with
/// its bytes. A walk of the authored folder, not of the embedded list: the two
/// sides are compared, so neither may define the other.
fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| {
        panic!(
            "the reference plugin folder reads at {}: {error}",
            dir.display()
        )
    });
    for entry in entries {
        let entry = entry.expect("a folder entry");
        let path = entry.path();
        let name = entry
            .file_name()
            .into_string()
            .unwrap_or_else(|name| panic!("{} is not UTF-8", name.to_string_lossy()));
        let relative = path
            .strip_prefix(root)
            .expect("an entry below the plugin root")
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        assert_eq!(
            relative.rsplit('/').next(),
            Some(name.as_str()),
            "the relative path keeps the file name"
        );
        let kind = entry.file_type().expect("an entry kind");
        if kind.is_dir() {
            walk(root, &path, files);
        } else if kind.is_file() {
            let bytes =
                fs::read(&path).unwrap_or_else(|error| panic!("{} reads: {error}", path.display()));
            files.insert(relative, bytes);
        }
    }
}

/// The authored folder as `path -> bytes`.
fn authored() -> BTreeMap<String, Vec<u8>> {
    let root = folder();
    let mut files = BTreeMap::new();
    walk(&root, &root, &mut files);
    assert!(!files.is_empty(), "the reference plugin has files");
    files
}

/// The findings as `file: path [rule] message` lines, so a failure names every
/// place the reference trees no longer satisfy the format.
fn findings(outcome: &PluginOutcome) -> Vec<String> {
    outcome
        .findings()
        .iter()
        .map(|finding| {
            format!(
                "{}: {} [{}] {}",
                finding.file, finding.path, finding.rule, finding.message
            )
        })
        .collect()
}

/// A plugin's loader must be pointed at its own source, not at a pack: the
/// embedded files are read through the same `Source` a pack's folder implements.
#[test]
fn the_embedded_list_matches_the_files_in_the_repository() {
    let authored = authored();
    let embedded: BTreeMap<&str, &[u8]> = reference_plugin::FILES
        .iter()
        .map(|(path, text)| (*path, text.as_bytes()))
        .collect();

    assert_eq!(
        embedded.len(),
        reference_plugin::FILES.len(),
        "FILES names each path once"
    );
    assert_eq!(
        embedded.keys().copied().collect::<Vec<_>>(),
        authored.keys().map(String::as_str).collect::<Vec<_>>(),
        "the embedded paths are the folder's paths"
    );
    for (path, bytes) in &authored {
        assert_eq!(
            embedded.get(path.as_str()),
            Some(&bytes.as_slice()),
            "`{path}` embeds the folder's bytes"
        );
    }
}

/// The reference look's one static guarantee: every rule the format imposes
/// accepts it, so a pack that copies these files adds no finding to its check.
#[test]
fn the_embedded_plugin_loads_with_no_finding() {
    let outcome = ui::load_from(&reference_plugin::source());
    let reported = findings(&outcome);
    assert!(
        reported.is_empty(),
        "the reference plugin loads cleanly:\n{}",
        reported.join("\n")
    );
    let PluginOutcome::Loaded(plugin) = &outcome else {
        panic!("the reference plugin loads: {outcome:?}");
    };
    assert_eq!(
        plugin.surfaces.len(),
        ui::SURFACE_IDS.len(),
        "the reference plugin restyles every surface"
    );
}

/// The node that places the slot `name`, anywhere under `tree`.
fn slot_node<'a>(tree: &'a ui::Node, name: &str) -> Option<&'a ui::Node> {
    if let ui::NodeKind::Slot(slot) = &tree.kind {
        if slot.slot == name {
            return Some(tree);
        }
    }
    match &tree.kind {
        ui::NodeKind::Box(container) => container
            .children
            .iter()
            .find_map(|child| slot_node(child, name)),
        ui::NodeKind::Slot(slot) => slot
            .children
            .iter()
            .find_map(|child| slot_node(child, name)),
        ui::NodeKind::List(list) => slot_node(&list.item, name),
        ui::NodeKind::Image(_) | ui::NodeKind::Glyph(_) | ui::NodeKind::Text(_) => None,
    }
}

/// The node the reference look places for `name`, on the surface `id`.
fn placed<'a>(plugin: &'a ui::UiPlugin, id: &str, name: &str) -> &'a ui::Node {
    let tree = plugin
        .surfaces
        .get(id)
        .unwrap_or_else(|| panic!("the reference plugin restyles `{id}`"));
    slot_node(tree, name).unwrap_or_else(|| panic!("`{id}` places `{name}`"))
}

fn face<'a>(plugin: &'a ui::UiPlugin, id: &str, name: &str) -> &'a ui::Typography {
    placed(plugin, id, name)
        .style
        .text
        .typography
        .as_ref()
        .unwrap_or_else(|| panic!("`{name}` resolves a face"))
}

/// The faces Default look §6 and the 2026-11-21 settlement name for the text
/// slice 1 did not style, and the section entry's `current` state (spec §11,
/// rules 37's parsing and 38).
#[test]
fn the_reference_look_styles_the_options_and_the_section_entries() {
    let outcome = ui::load_from(&reference_plugin::source());
    let PluginOutcome::Loaded(plugin) = &outcome else {
        panic!("the reference plugin loads: {outcome:?}");
    };

    let option = face(plugin, "play", "choice.text");
    assert_eq!(option.face, "IBM Plex Sans");
    assert_eq!(option.weight, 400);
    assert_eq!(option.size_px, 21.0, "§2's option text is 21px");

    let badge = face(plugin, "play", "choice.number");
    assert_eq!(badge.face, "Fira Mono", "§2's number badge is mono");
    assert_eq!(badge.size_px, 12.0);

    let entry = face(plugin, "contents", "contents.section.label");
    assert_eq!(entry.face, "IBM Plex Sans");
    assert_eq!(entry.weight, 600, "§4's section entries are SemiBold");

    let label = placed(plugin, "contents", "contents.section.label");
    let current = label
        .states
        .current
        .as_ref()
        .expect("the entry carries its `current` override");
    assert_eq!(
        current.color,
        plugin.tokens.colors.get("text").copied(),
        "the current section is white, and the others stay faint"
    );
}
