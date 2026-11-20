//! `ui/plugin.ron`: the manifest's types and the rules its own fields obey
//! (spec §2).
//!
//! The manifest is the plugin's entry point — the file that says which format
//! version the folder is written in — so its rules decide whether the rest of
//! the folder is read at all. Every path it names is confinement-checked here,
//! before a rule or the renderer opens one. Its fonts, images, numbering and
//! copy overrides are typed here and judged by the passes that own them
//! (tickets #107 and #108).

use std::collections::BTreeMap;

use crate::finding::{rules, Finding, Location};

use super::source::{check_relative, Source};

/// The manifest's file, relative to the plugin folder.
pub(super) const FILE: &str = "plugin.ron";

/// The format version this build reads, matched exactly (spec §2).
pub const FORMAT_VERSION: u32 = 1;

/// `ui/plugin.ron`.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Manifest {
    /// The plugin format's version. Any other value fails the plugin, and the
    /// message names this one.
    pub format_version: u32,
    /// The plugin's display name.
    #[serde(default)]
    pub name: String,
    /// How choice numbers display.
    #[serde(default)]
    pub numbering: Numbering,
    /// The faces the plugin ships, each with its license.
    #[serde(default)]
    pub fonts: Vec<FontDecl>,
    /// The art the plugin ships.
    #[serde(default)]
    pub images: Vec<ImageDecl>,
    /// Copy-key overrides: the plugin's words for a key the copy table owns.
    #[serde(default)]
    pub copy: BTreeMap<String, String>,
}

/// How a choice's number displays (spec §5). The keys stay `1`–`9`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub enum Numbering {
    #[default]
    Arabic,
    RomanUpper,
    RomanLower,
    AlphaUpper,
    AlphaLower,
}

/// One shipped face: a family, its weight, its file and the license it comes
/// under with the file that carries the license text.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct FontDecl {
    pub family: String,
    pub weight: u16,
    pub file: String,
    pub license: String,
    pub license_file: String,
}

/// One shipped image: a file under the plugin folder.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct ImageDecl {
    pub file: String,
}

/// Read and judge `ui/plugin.ron`, returning the manifest the later passes
/// read. `None` means the manifest failed, so the folder is never read on.
pub(super) fn check(source: &dyn Source, findings: &mut Vec<Finding>) -> Option<Manifest> {
    let file = source.display(FILE);
    if !source.paths().iter().any(|path| path == FILE) {
        findings.push(Finding::new(
            file,
            Location::File,
            rules::UI_FILE_READ,
            "the plugin folder has no plugin.ron",
        ));
        return None;
    }
    let bytes = match source.read(FILE) {
        Ok(bytes) => bytes,
        Err(reason) => {
            findings.push(Finding::new(
                file,
                Location::File,
                rules::UI_FILE_READ,
                format!("cannot read the plugin manifest: {reason}"),
            ));
            return None;
        }
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => {
            findings.push(Finding::new(
                file,
                Location::File,
                rules::UI_PARSE,
                "the plugin manifest is not UTF-8",
            ));
            return None;
        }
    };
    let manifest: Manifest = match ron::from_str(&text) {
        Ok(manifest) => manifest,
        Err(error) => {
            findings.push(Finding::new(
                file,
                Location::File,
                rules::UI_PARSE,
                format!("invalid RON: {error}"),
            ));
            return None;
        }
    };

    // A version this build does not read says nothing about the rest of the
    // file's shape, so no later rule runs on it.
    if manifest.format_version != FORMAT_VERSION {
        findings.push(Finding::new(
            file,
            Location::Field("format_version".to_string()),
            rules::UI_VERSION,
            format!(
                "format_version is {}; this build reads version {FORMAT_VERSION}",
                manifest.format_version
            ),
        ));
        return None;
    }

    confine_named_paths(&manifest, &file, findings);
    Some(manifest)
}

/// Every path the manifest names is checked before anything reads one, so an
/// escape is reported where the author wrote it.
fn confine_named_paths(manifest: &Manifest, file: &str, findings: &mut Vec<Finding>) {
    let font_paths = manifest.fonts.iter().enumerate().flat_map(|(index, font)| {
        [
            (format!("fonts[{index}].file"), font.file.as_str()),
            (
                format!("fonts[{index}].license_file"),
                font.license_file.as_str(),
            ),
        ]
    });
    let image_paths = manifest
        .images
        .iter()
        .enumerate()
        .map(|(index, image)| (format!("images[{index}].file"), image.file.as_str()));

    for (path, named) in font_paths.chain(image_paths) {
        if let Err(reason) = check_relative(named) {
            findings.push(Finding::new(
                file,
                Location::Field(path),
                rules::UI_PATH_ESCAPE,
                reason,
            ));
        }
    }
}
