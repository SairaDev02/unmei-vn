//! The asset references a pack's own files declare, and the one rule that
//! checks the referenced files exist (spec `docs/check-preview-spec.md` §5).
//!
//! The split is §5's: the engine lists the references and checks that each file
//! exists, and the player decodes the files that do. So this module decodes
//! nothing and pulls no decoder, and play's lazy loading is untouched.
//!
//! It is the one owner of the **pack-relative** path spelling —
//! `assets/backgrounds/<id>.png`, `assets/sprites/<npc_id>/<pose>.png` and
//! `assets/audio/<id>.ogg` — and every path uses `/`, so a finding reads the
//! same on every platform. The runtime owns the other spelling, the path under
//! the pack's asset root that play and the gallery load through
//! (`app/src/presentation/assets.rs`).

use std::collections::{HashMap, HashSet};

use crate::content::{Line, Pack};
use crate::finding::{rules, Finding, Location};

/// The kind of asset a reference names, which fixes its pack-relative path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetKind {
    /// `bg <id>` and the manifest's `title_backdrop`.
    Background,
    /// `sprite <npc_id> <pose>`.
    Sprite,
    /// `play <id>`. `stop <id>` names a sound `play` already names, so it adds
    /// no reference.
    Sound,
}

/// One asset file a pack file names: where it is named and the pack-relative
/// path it resolves to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AssetReference {
    /// The pack-relative file that names the asset, such as
    /// `scenes/reunion.unmei` or `pack.ron`.
    pub file: String,
    /// The source line of the reference, or `None` for `pack.ron`'s
    /// `title_backdrop`: the loader decodes the manifest without tracking a
    /// property's line, so that one reference is to the file as a whole.
    pub line: Option<usize>,
    /// Which path rule the asset id resolves through.
    pub kind: AssetKind,
    /// The pack-relative asset file, such as
    /// `assets/backgrounds/school_gate.png`.
    pub path: String,
}

impl Pack {
    /// Every asset the pack's files reference, in §3's order — file, then line
    /// with the no-line manifest reference first — so the first reference to a
    /// path is the first one the check reports.
    ///
    /// It reads the loaded pack only, never the filesystem: every line of every
    /// scene counts, including both `if` arms and arms no playthrough takes
    /// (content format §3).
    pub fn asset_references(&self) -> Vec<AssetReference> {
        let mut references = Vec::new();
        for scene in self.scenes.values() {
            let file = format!("scenes/{}.unmei", scene.id);
            for (index, line) in scene.lines.iter().enumerate() {
                let (kind, path) = match line {
                    Line::Bg(id) => (AssetKind::Background, background_path(id)),
                    Line::Sprite { npc_id, pose } => (AssetKind::Sprite, sprite_path(npc_id, pose)),
                    Line::Play(id) => (AssetKind::Sound, sound_path(id)),
                    // `hide` names no file, and every other line names none.
                    _ => continue,
                };
                references.push(AssetReference {
                    file: file.clone(),
                    line: Some(scene.source_line(index)),
                    kind,
                    path,
                });
            }
        }
        if let Some(id) = &self.manifest.title_backdrop {
            // The title backdrop resolves exactly like `bg`, from `pack.ron`
            // rather than a scene line.
            references.push(AssetReference {
                file: "pack.ron".to_string(),
                line: None,
                kind: AssetKind::Background,
                path: background_path(id),
            });
        }
        references.sort();
        references
    }

    /// One `pack.asset.missing` finding per distinct referenced file that is
    /// not a file on disk, at its first reference in [`asset_references`]
    /// order, whose message counts the references to that path (§5).
    ///
    /// A file under `assets/` that nothing references is never considered: the
    /// rule reads the references, not the folder (§5, content format §2).
    ///
    /// [`asset_references`]: Pack::asset_references
    pub fn missing_asset_findings(&self) -> Vec<Finding> {
        let references = self.asset_references();
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for reference in &references {
            *counts.entry(reference.path.as_str()).or_default() += 1;
        }

        let mut findings = Vec::new();
        let mut reported: HashSet<&str> = HashSet::new();
        for reference in &references {
            // Only a path's first reference can report it, so a repeated
            // missing sprite stays one finding.
            if !reported.insert(reference.path.as_str()) {
                continue;
            }
            if self.root.join(&reference.path).is_file() {
                continue;
            }
            let count = counts[reference.path.as_str()];
            let message = match count {
                1 => format!("{} does not exist", reference.path),
                count => format!(
                    "{} does not exist; referenced {count} times",
                    reference.path
                ),
            };
            findings.push(match reference.line {
                Some(line) => {
                    Finding::at_line(&reference.file, line, rules::PACK_ASSET_MISSING, message)
                }
                None => Finding::new(
                    &reference.file,
                    Location::File,
                    rules::PACK_ASSET_MISSING,
                    message,
                ),
            });
        }
        findings
    }
}

/// The pack-relative path a `bg <id>` and the manifest's `title_backdrop`
/// resolve to.
fn background_path(id: &str) -> String {
    format!("assets/backgrounds/{id}.png")
}

/// The pack-relative path a `sprite <npc_id> <pose>` resolves to.
fn sprite_path(npc_id: &str, pose: &str) -> String {
    format!("assets/sprites/{npc_id}/{pose}.png")
}

/// The pack-relative path a `play <id>` resolves to.
fn sound_path(id: &str) -> String {
    format!("assets/audio/{id}.ogg")
}
