//! Checked authoring targets and the checked Pack-key inventory (author-cli
//! contract §10).
//!
//! An authoring id becomes an ordinary fixed-extension filename on the current
//! platform **before any pack-relative join happens**, and every join goes
//! through the fixed subdirectory and extension here, never through a raw
//! string. [`CheckedPack`] borrows a supplied [`Pack`] and holds a checked
//! target for every Scene/NPC key, so staging consumes checked keys for both
//! its source reads and its writes.

use std::path::{Path, PathBuf};

use unmei_engine::Pack;

use super::filename::{check_filename, FileKind, Platform, TargetError};

/// One checked NPC target: an id whose `npcs/<id>.ron` filename is an ordinary
/// native component. The id is private and immutable; joins go through the
/// fixed subdirectory and extension here, never through a raw string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcTarget {
    id: String,
}

impl NpcTarget {
    /// Check `id` as an NPC filename on the current platform.
    pub fn new(id: &str) -> Result<Self, TargetError> {
        check_filename(id, FileKind::Npc, Platform::native())?;
        Ok(Self { id: id.to_string() })
    }

    /// The exact accepted id (the filename stem).
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    /// The completed filename, `"<id>.ron"`.
    fn filename(&self) -> String {
        format!("{}.{}", self.id, FileKind::Npc.extension())
    }

    /// The path under `root`, built as a direct child of the fixed subdirectory.
    pub(crate) fn path_in(&self, root: &Path) -> PathBuf {
        root.join(FileKind::Npc.subdirectory())
            .join(self.filename())
    }
}

/// One checked Scene target: an id whose `scenes/<id>.unmei` filename is an
/// ordinary native component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneTarget {
    id: String,
}

impl SceneTarget {
    /// Check `id` as a Scene filename on the current platform.
    pub fn new(id: &str) -> Result<Self, TargetError> {
        check_filename(id, FileKind::Scene, Platform::native())?;
        Ok(Self { id: id.to_string() })
    }

    /// The exact accepted id (the filename stem).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The completed filename, `"<id>.unmei"`.
    fn filename(&self) -> String {
        format!("{}.{}", self.id, FileKind::Scene.extension())
    }

    /// The path under `root`, built as a direct child of the fixed subdirectory.
    pub fn path_in(&self, root: &Path) -> PathBuf {
        root.join(FileKind::Scene.subdirectory())
            .join(self.filename())
    }
}

/// A supplied [`Pack`] plus one checked target for every Scene/NPC key.
///
/// The inventory borrows the Pack immutably; staging consumes these checked
/// keys for both its source reads and its staged writes, so loader provenance
/// is never treated as permission for an unchecked key. Manually constructed
/// or mutated Packs are checked the same way.
pub struct CheckedPack<'a> {
    pack: &'a Pack,
    scenes: Vec<SceneTarget>,
    npcs: Vec<NpcTarget>,
}

impl<'a> CheckedPack<'a> {
    /// Check every Scene and NPC key in `pack`.
    pub fn new(pack: &'a Pack) -> Result<Self, TargetError> {
        let mut scene_ids: Vec<&String> = pack.scenes.keys().collect();
        scene_ids.sort();
        let scenes = scene_ids
            .into_iter()
            .map(|id| SceneTarget::new(id))
            .collect::<Result<Vec<_>, _>>()?;

        let mut npc_ids: Vec<&String> = pack.npcs.keys().collect();
        npc_ids.sort();
        let npcs = npc_ids
            .into_iter()
            .map(|id| NpcTarget::new(id))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { pack, scenes, npcs })
    }

    /// The borrowed Pack the checked keys came from.
    pub(crate) fn pack(&self) -> &'a Pack {
        self.pack
    }

    /// Every checked Scene key, sorted by id.
    pub(crate) fn scenes(&self) -> &[SceneTarget] {
        &self.scenes
    }

    /// Every checked NPC key, sorted by id.
    pub(crate) fn npcs(&self) -> &[NpcTarget] {
        &self.npcs
    }

    /// The checked target for `id`, when the Pack holds that Scene.
    fn scene(&self, id: &str) -> Option<&SceneTarget> {
        self.scenes.iter().find(|target| target.id() == id)
    }

    /// The checked Scene target for `id`, or a terminal membership refusal.
    pub fn require_scene(&self, id: &str) -> Result<&SceneTarget, TargetError> {
        self.scene(id)
            .ok_or_else(|| TargetError::AbsentScene { id: id.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_pack_accepts_and_reports_every_key() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
        let loaded = unmei_engine::load_pack(&root).expect("demo pack loads");
        let checked = CheckedPack::new(&loaded.pack).expect("demo keys are safe");
        assert!(checked.require_scene("first_meeting").is_ok());
        assert!(checked.require_scene("no_such").is_err());
    }

    /// The production constructors use the host's own semantics: a backslash or
    /// colon is an ordinary filename character on Linux and a path/ADS form on
    /// Windows, and a reserved device basename is only reserved on Windows.
    #[test]
    fn native_constructors_apply_this_platforms_policy() {
        assert_eq!(NpcTarget::new("NUL").is_ok(), !cfg!(windows));
        assert_eq!(NpcTarget::new("a\\b").is_ok(), !cfg!(windows));
        assert_eq!(NpcTarget::new("a:b").is_ok(), !cfg!(windows));
        assert!(SceneTarget::new("act.one").is_ok());
    }
}
