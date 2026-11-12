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
//! - NPC file ids must match their filename stem (`npcs/ace.ron` holds `id: "ace"`).
//! - `entry` in the manifest must be an existing scene.
//! - When the manifest's optional `scenes` list is non-empty it is the declared
//!   order: every id must match a loaded scene exactly and appear once, and
//!   every loaded scene not listed warns once (the entry scene's warning is
//!   dedicated). Diagnostics accumulate and are sorted by scene id.
//!
//! Asset files are **not** checked for existence: packs validate structurally
//! (see `examples/demo-pack/assets/README.md`).
//!
//! This root owns the directory read and the per-file decode; `validate` is the
//! semantic pass they hand their result to, and `entries` holds the read-only
//! views a loaded pack derives for the title, gallery and Timeline surfaces.

mod entries;
mod validate;

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::content::{Manifest, Npc, Pack, Scene, StateValue};
use crate::error::LoadError;
use crate::parser::Parser;

use self::validate::validate;

/// A validation warning: real but non-fatal (the pack still loads).
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub file: String,
    pub message: String,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.file, self.message)
    }
}

/// A loaded pack plus its warnings.
#[derive(Debug)]
pub struct LoadedPack {
    pub pack: Pack,
    pub warnings: Vec<Warning>,
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

/// Read one authored pack text file: UTF-8, with a single leading byte order
/// mark dropped (content format §2). Any other U+FEFF is part of the text.
pub fn read_pack_text(path: &Path) -> std::io::Result<String> {
    let text = fs::read_to_string(path)?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(stripped) => stripped.to_string(),
        None => text,
    })
}

/// Load and validate a content pack from a directory.
pub fn load_pack(root: &Path) -> Result<LoadedPack, Vec<LoadError>> {
    let mut errors: Vec<LoadError> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();

    // --- manifest -----------------------------------------------------------
    let manifest_path = root.join("pack.ron");
    let manifest_src = match read_pack_text(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            return Err(vec![LoadError::new(
                "pack.ron",
                None,
                format!("cannot read pack manifest: {e}"),
            )]);
        }
    };
    let mut manifest: Manifest = match ron::from_str(&manifest_src) {
        Ok(m) => m,
        Err(e) => {
            errors.push(LoadError::new(
                "pack.ron",
                None,
                format!("invalid RON: {e}"),
            ));
            return Err(errors);
        }
    };
    // Normalise the optional tagline before validation: the raw value is
    // checked for control characters first, then trimmed.
    if let Some(raw) = manifest.tagline.take() {
        match normalise_tagline(&raw) {
            Ok(Some(tagline)) => {
                let len = tagline.chars().count();
                if len > TAGLINE_WARN_LEN {
                    warnings.push(Warning {
                        file: "pack.ron".to_string(),
                        message: format!(
                            "tagline is {len} characters long; the 1280x720 title column fits {TAGLINE_WARN_LEN}"
                        ),
                    });
                }
                manifest.tagline = Some(tagline);
            }
            Ok(None) => {}
            Err(message) => errors.push(LoadError::new("pack.ron", None, message)),
        }
    }
    // The optional title backdrop names an asset resolved exactly like
    // `bg`, so it shares the ids' `[a-z0-9_]+` rule: a malformed id is a load
    // error. The file's existence is not checked here (assets are not), so a
    // declared-but-missing file stays a runtime silent degradation.
    if let Some(id) = &manifest.title_backdrop {
        if let Err(message) = crate::parser::check_id(id, "title_backdrop id") {
            errors.push(LoadError::new("pack.ron", None, message));
        }
    }

    // --- state vocabulary ---------------------------------------------------
    let state_path = root.join("state.ron");
    let state_src = match read_pack_text(&state_path) {
        Ok(s) => s,
        Err(e) => {
            return Err(vec![LoadError::new(
                "state.ron",
                None,
                format!("cannot read state file: {e}"),
            )]);
        }
    };
    let state: HashMap<String, StateValue> = match crate::pack_ron::decode_state(&state_src) {
        // `state.ron` uses RON struct syntax with pack-defined keys, so the
        // pack RON bridge reads it as a generic `ron::Value` (struct syntax ==
        // map there) and converts it to the typed state map.
        Ok(s) => s,
        Err(e) => {
            errors.push(LoadError::new("state.ron", None, e));
            return Err(errors);
        }
    };

    // --- scenes -------------------------------------------------------------
    let mut scenes: HashMap<String, Scene> = HashMap::new();
    let scenes_dir = root.join("scenes");
    let scene_files = match fs::read_dir(&scenes_dir) {
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
            errors.push(LoadError::new(
                "scenes/",
                None,
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
                errors.push(LoadError::new(
                    &rel,
                    None,
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

    // --- npcs ---------------------------------------------------------------
    let mut npcs: HashMap<String, Npc> = HashMap::new();
    let npcs_dir = root.join("npcs");
    let npc_files = match fs::read_dir(&npcs_dir) {
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
            errors.push(LoadError::new(
                "npcs/",
                None,
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
                errors.push(LoadError::new(
                    &rel,
                    None,
                    format!("cannot read NPC file: {e}"),
                ));
                continue;
            }
        };
        match ron::from_str::<Npc>(&src) {
            Ok(npc) => {
                if npc.id != stem {
                    errors.push(LoadError::new(
                        &rel,
                        None,
                        format!("npc id `{}` does not match filename stem `{stem}`", npc.id),
                    ));
                    continue;
                }
                npcs.insert(stem, npc);
            }
            Err(e) => {
                errors.push(LoadError::new(&rel, None, format!("invalid NPC RON: {e}")));
            }
        }
    }

    // --- semantic validation -------------------------------------------------
    validate(
        &manifest,
        &state,
        &scenes,
        &npcs,
        &mut errors,
        &mut warnings,
    );

    if errors.is_empty() {
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
    } else {
        Err(errors)
    }
}
