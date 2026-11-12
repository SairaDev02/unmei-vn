//! The session log's progress lines: the title's first show, each scene entry
//! and each manual save (`docs/runtime-hardening.md` § Session log).
//!
//! The three formats live here and nowhere else, so the lines an automated walk
//! waits on cannot drift between the surfaces that write them.

use std::path::Path;

/// The title became the surface for the first time this launch.
pub(crate) fn title_ready() {
    log::info!("unmei: title ready");
}

/// Play entered `scene_id` — a new game's entry scene, a restored save's scene,
/// a Timeline scene restart, or a `jump` that landed in another scene file.
pub(crate) fn entered_scene(scene_id: &str) {
    log::info!("unmei: entered scene {scene_id}");
}

/// A manual save wrote `path`: the F5 shortcut and the Esc overlay's Save share
/// this one line, so a save is announced whichever surface asked for it. An
/// autosave stays silent.
pub(crate) fn saved_to(path: &Path) {
    log::info!("unmei: saved to {}", path.display());
}
