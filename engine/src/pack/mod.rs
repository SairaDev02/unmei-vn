//! Content-pack loading: read a pack directory, validate it, return a [`Pack`].
//!
//! A pack is a directory in the layout from `docs/content-format.md` §2:
//!
//! ```text
//! my-pack/
//! ├── pack.ron          # manifest: name + entry scene
//! ├── state.ron         # declared initial state
//! ├── pack.glossary.md  # optional
//! ├── scenes/*.unmei
//! ├── npcs/*.ron
//! └── assets/...
//! ```
//!
//! Validation rules (load-time, not runtime):
//! - `state.ron` is the state vocabulary: every `set` target, every
//!   condition's state key, every `state_keys` entry, and every `{var}`
//!   reference must be declared there.
//! - Each key's declared kind is fixed: a `set` or a condition literal of a
//!   different kind is an error, as is an ordering operator on a non-number.
//! - `fallback` names a landing key in the config's landing space (a criterion
//!   id for `choice`, `true`/`false` for `noul`, a legend index for `score`;
//!   ADR 0005).
//! - A `decide` line's labels (`<prefix>.<landing key>` for every key the
//!   config can reach, including the fallback) must exist in the same scene — a
//!   missing label is a parse error, not a runtime surprise.
//! - `jump` targets resolve to a local label or a scene file (local wins).
//! - Choice option targets resolve the same way.
//! - No jump cycle without a say-line, `choice`, `decide` or `if` on it: it
//!   would never park. A cycle through an `if` is left to the runner.
//! - NPC file ids must match their filename stem (`npcs/ace.ron` holds `id: "ace"`).
//! - `entry` in the manifest must be an existing scene.
//! - When the manifest's optional `scenes` list is non-empty it is the declared
//!   order: every id must match a loaded scene exactly and appear once, and
//!   every loaded scene not listed warns once (the entry scene's warning is
//!   dedicated). Diagnostics accumulate from every rule rather than stopping at
//!   the first, and `load_pack` sorts them.
//!
//! A `pack.ron` or `state.ron` that will not read or decode contributes its one
//! finding and skips only the rules that read it (check-and-preview spec §3):
//! without a manifest the entry-scene, `scenes`, tagline and backdrop rules do
//! not run, and without a state vocabulary so do the declared-key, kind,
//! `{var}`, condition and `state_keys` rules. Every other rule still runs.
//!
//! Loading validates the pack structurally and checks no asset file: play
//! degrades silently on a missing one. The author-facing check reports the
//! referenced files that do not exist, through the loaded pack's separate asset
//! rule ([`Pack::missing_asset_findings`], check-and-preview spec §5).
//!
//! This root owns the directory read and the per-file decode; `validate` is the
//! semantic pass they hand their result to, `state_rules` holds the rules that
//! need the state vocabulary, and `entries` holds the read-only views a loaded
//! pack derives for the title, gallery and Timeline surfaces.

mod assets;
mod entries;
mod state_rules;
mod validate;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::content::{Manifest, Npc, Pack, Scene, StateValue};
use crate::finding::{rules, sort_findings, Finding, Location};
use crate::parser::Parser;

use self::validate::validate;

pub use self::assets::{AssetKind, AssetReference};

/// A loaded pack plus its warnings.
#[derive(Debug)]
pub struct LoadedPack {
    pub pack: Pack,
    pub warnings: Vec<Finding>,
}

/// The tagline length the 1280x720 title column fits; past it the loader warns
/// once (never an error) and the tagline still renders.
const TAGLINE_WARN_LEN: usize = 48;

/// Normalise a manifest `tagline`. The raw value is checked for control
/// characters *first* (a newline or control character is a load error), then it
/// is trimmed of surrounding ASCII spaces; an empty or space-only value means
/// the tagline is absent. `{var}` interpolation is deliberately not performed —
/// the value is returned exactly as authored (minus the surrounding spaces).
fn normalise_tagline(raw: &str) -> Result<Option<String>, String> {
    if let Some(bad) = raw.chars().find(|c| is_line_break(*c)) {
        return Err(format!(
            "tagline must be a single line with no control characters; found {bad:?}"
        ));
    }
    let trimmed = raw.trim_matches(' ');
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

/// A character that breaks the tagline's one-line contract: a control character
/// (including `\n`, `\r` and `\t`) or a Unicode line/paragraph separator, which
/// `is_control` does not cover.
fn is_line_break(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// The largest pack file Unmei reads or copies, in bytes (content format §2).
pub const PACK_FILE_LIMIT: u64 = 16 * 1024 * 1024;

/// Check that `path` is a regular file of at most [`PACK_FILE_LIMIT`] bytes,
/// without following it: a downloaded pack is untrusted, so a symlink, folder,
/// pipe or device in it is refused rather than read (content format §2).
pub fn check_pack_file(path: &Path) -> std::io::Result<u64> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file; a pack file cannot be a symlink, folder or device",
        ));
    }
    within_pack_file_limit(meta.len())
}

fn within_pack_file_limit(len: u64) -> std::io::Result<u64> {
    if len > PACK_FILE_LIMIT {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("the file is over the {PACK_FILE_LIMIT}-byte limit for a pack file"),
        ));
    }
    Ok(len)
}

/// Check that a pack folder such as `scenes/` is a real folder, not a symlink
/// to somewhere else.
fn check_pack_dir(path: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(path)?.file_type().is_dir() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a folder; a pack folder cannot be a symlink",
        ))
    }
}

/// Read one authored pack text file: UTF-8, with a single leading byte order
/// mark dropped (content format §2). Any other U+FEFF is part of the text.
/// The file must pass [`check_pack_file`].
pub fn read_pack_text(path: &Path) -> std::io::Result<String> {
    use std::io::Read as _;
    check_pack_file(path)?;
    let mut text = String::new();
    fs::File::open(path)?
        .take(PACK_FILE_LIMIT + 1)
        .read_to_string(&mut text)?;
    within_pack_file_limit(text.len() as u64)?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(stripped) => stripped.to_string(),
        None => text,
    })
}

/// Load and validate a content pack from a directory.
///
/// Every file is read and parsed before the result is decided, so one broken
/// file never hides the problems in the others (spec §3).
pub fn load_pack(root: &Path) -> Result<LoadedPack, Vec<Finding>> {
    let mut errors: Vec<Finding> = Vec::new();
    let mut warnings: Vec<Finding> = Vec::new();

    let manifest = read_manifest(root, &mut errors, &mut warnings);
    let state = read_state(root, &mut errors);
    let scenes = read_scenes(root, &mut errors);
    let npcs = read_npcs(root, &mut errors);

    validate(
        manifest.as_ref(),
        state.as_ref(),
        &scenes,
        &npcs,
        &mut errors,
        &mut warnings,
    );
    sort_findings(&mut errors);
    sort_findings(&mut warnings);

    // A file that did not read or decode recorded its finding above, so the
    // pack is returned only when both decoded and no rule objected.
    let (Some(manifest), Some(state)) = (manifest, state) else {
        return Err(errors);
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(LoadedPack {
        pack: Pack {
            manifest,
            state,
            scenes,
            npcs,
            root: root.to_path_buf(),
        },
        warnings,
    })
}

/// Read and decode `pack.ron` with its optional tagline and backdrop id; `None`
/// when a finding was recorded instead, which skips the manifest rules.
fn read_manifest(
    root: &Path,
    errors: &mut Vec<Finding>,
    warnings: &mut Vec<Finding>,
) -> Option<Manifest> {
    let src = match read_pack_text(&root.join("pack.ron")) {
        Ok(s) => s,
        Err(e) => {
            errors.push(Finding::new(
                "pack.ron",
                Location::File,
                rules::PACK_FILE_READ,
                format!("cannot read pack manifest: {e}"),
            ));
            return None;
        }
    };
    let mut manifest: Manifest = match ron::from_str(&src) {
        Ok(m) => m,
        Err(e) => {
            errors.push(Finding::new(
                "pack.ron",
                Location::File,
                rules::PACK_FILE_RON,
                format!("invalid RON: {e}"),
            ));
            return None;
        }
    };
    // Normalise the optional tagline before validation: the raw value is
    // checked for control characters first, then trimmed.
    if let Some(raw) = manifest.tagline.take() {
        match normalise_tagline(&raw) {
            Ok(Some(tagline)) => {
                let len = tagline.chars().count();
                if len > TAGLINE_WARN_LEN {
                    warnings.push(Finding::warning(
                        "pack.ron", Location::File,
                        rules::PACK_MANIFEST_TAGLINE_LENGTH,
                        format!(
                            "tagline is {len} characters long; the 1280x720 title column fits {TAGLINE_WARN_LEN}"
                        ),
                    ));
                }
                manifest.tagline = Some(tagline);
            }
            Ok(None) => {}
            Err(message) => errors.push(Finding::new(
                "pack.ron",
                Location::File,
                rules::PACK_MANIFEST,
                message,
            )),
        }
    }
    // The optional title backdrop names an asset resolved exactly like
    // `bg`, so it shares the ids' `[a-z0-9_]+` rule: a malformed id is a load
    // error. Loading checks no asset file, so a declared-but-missing file stays
    // a runtime silent degradation; the check's asset rule reports it.
    if let Some(id) = &manifest.title_backdrop {
        if let Err(message) = crate::parser::check_id(id, "title_backdrop id") {
            errors.push(Finding::new(
                "pack.ron",
                Location::File,
                rules::PACK_MANIFEST,
                message,
            ));
        }
    }
    Some(manifest)
}

/// Read and decode the declared state vocabulary; `None` when a finding was
/// recorded instead, which skips the rules that read it.
fn read_state(root: &Path, errors: &mut Vec<Finding>) -> Option<HashMap<String, StateValue>> {
    let src = match read_pack_text(&root.join("state.ron")) {
        Ok(s) => s,
        Err(e) => {
            errors.push(Finding::new(
                "state.ron",
                Location::File,
                rules::PACK_FILE_READ,
                format!("cannot read state file: {e}"),
            ));
            return None;
        }
    };
    // `state.ron` uses RON struct syntax with pack-defined keys, so the pack RON
    // bridge reads it as a generic `ron::Value` (struct syntax == map there) and
    // converts it to the typed state map.
    match crate::pack_ron::decode_state(&src) {
        Ok(state) => Some(state),
        Err(e) => {
            errors.push(Finding::new(
                "state.ron",
                Location::File,
                rules::PACK_FILE_RON,
                e,
            ));
            None
        }
    }
}

/// Read and parse every `scenes/*.unmei`. A file reports at most one finding,
/// because the parser stops at its first syntax error; a scene that does not
/// parse stays out of the semantic pass.
fn read_scenes(root: &Path, errors: &mut Vec<Finding>) -> HashMap<String, Scene> {
    let mut scenes: HashMap<String, Scene> = HashMap::new();
    let scenes_dir = root.join("scenes");
    let scene_files = match check_pack_dir(&scenes_dir).and_then(|()| fs::read_dir(&scenes_dir)) {
        Ok(d) => {
            let mut files: Vec<PathBuf> = d
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "unmei").unwrap_or(false))
                .collect();
            files.sort();
            files
        }
        Err(e) => {
            errors.push(Finding::new(
                "scenes/",
                Location::File,
                rules::PACK_FILE_READ,
                format!("cannot read scenes directory: {e}"),
            ));
            Vec::new()
        }
    };
    for path in scene_files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let rel = format!("scenes/{}.unmei", stem);
        let src = match read_pack_text(&path) {
            Ok(s) => s,
            Err(e) => {
                errors.push(Finding::new(
                    &rel,
                    Location::File,
                    rules::PACK_FILE_READ,
                    format!("cannot read scene: {e}"),
                ));
                continue;
            }
        };
        match Parser::new(&rel).parse(&stem, &src) {
            Ok(scene) => {
                scenes.insert(stem, scene);
            }
            Err(e) => errors.push(e),
        }
    }
    scenes
}

/// Read and decode every `npcs/*.ron`.
fn read_npcs(root: &Path, errors: &mut Vec<Finding>) -> HashMap<String, Npc> {
    let mut npcs: HashMap<String, Npc> = HashMap::new();
    let npcs_dir = root.join("npcs");
    let npc_files = match check_pack_dir(&npcs_dir).and_then(|()| fs::read_dir(&npcs_dir)) {
        Ok(d) => {
            let mut files: Vec<PathBuf> = d
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "ron").unwrap_or(false))
                .collect();
            files.sort();
            files
        }
        Err(e) => {
            errors.push(Finding::new(
                "npcs/",
                Location::File,
                rules::PACK_FILE_READ,
                format!("cannot read npcs directory: {e}"),
            ));
            Vec::new()
        }
    };
    for path in npc_files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let rel = format!("npcs/{}.ron", stem);
        let src = match read_pack_text(&path) {
            Ok(s) => s,
            Err(e) => {
                errors.push(Finding::new(
                    &rel,
                    Location::File,
                    rules::PACK_FILE_READ,
                    format!("cannot read NPC file: {e}"),
                ));
                continue;
            }
        };
        match ron::from_str::<Npc>(&src) {
            Ok(npc) => {
                if npc.id != stem {
                    errors.push(Finding::new(
                        &rel,
                        Location::File,
                        rules::PACK_NPC,
                        format!("npc id `{}` does not match filename stem `{stem}`", npc.id),
                    ));
                    continue;
                }
                npcs.insert(stem, npc);
            }
            Err(e) => {
                errors.push(Finding::new(
                    &rel,
                    Location::File,
                    rules::PACK_FILE_RON,
                    format!("invalid NPC RON: {e}"),
                ));
            }
        }
    }
    npcs
}
