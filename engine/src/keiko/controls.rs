//! The control-id table: the one owner of every stable role id (spec §3.2).
//!
//! The player's `MenuFocus` enum maps one-to-one onto this table, and the UI
//! plugin's control names reuse the same strings. The ids live here so the
//! Bevy-free case validator can check them without depending on the runtime.

/// The fixed control ids, in the player's walk order where one exists.
pub const CONTROL_IDS: &[&str] = &[
    // Title
    "title.continue",
    "title.new-game",
    "title.settings",
    "title.gallery",
    "title.quit",
    // Pause overlay
    "overlay.save-tab",
    "overlay.load-tab",
    "overlay.slot.1",
    "overlay.slot.2",
    "overlay.slot.3",
    "overlay.slot.autosave",
    "overlay.resume",
    "overlay.exit-to-title",
    "overlay.settings",
    "overlay.gallery",
    "overlay.timeline",
    "overlay.legacy",
    // Legacy picker
    "legacy.target.1",
    "legacy.target.2",
    "legacy.target.3",
    "legacy.cancel",
    // Settings
    "settings.back",
    "settings.master",
    "settings.music",
    "settings.sfx",
    "settings.mute",
    "settings.reset-audio",
    "settings.text-speed",
    "settings.auto",
    "settings.fullscreen",
    "settings.reset-play",
    "settings.key",
    "settings.copy-path",
    "settings.clear-key",
    // Confirm
    "confirm.yes",
    "confirm.no",
];

/// The families whose ids carry a 1-based index: `gallery.card.<n>` and
/// `timeline.card.<n>`.
pub const INDEXED_CONTROL_FAMILIES: &[&str] = &["gallery.card", "timeline.card"];

/// Whether `id` names a control: a fixed id, or an indexed family member with a
/// 1-based index.
pub fn is_control_id(id: &str) -> bool {
    if CONTROL_IDS.contains(&id) {
        return true;
    }
    INDEXED_CONTROL_FAMILIES.iter().any(|family| {
        id.strip_prefix(family)
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(|index| {
                !index.is_empty()
                    && index.bytes().all(|byte| byte.is_ascii_digit())
                    && index.parse::<u32>().is_ok_and(|value| value >= 1)
            })
    })
}

/// The slot ids, so a save/load intent and a slot assertion name one control.
pub fn slot_control(slot: &str) -> String {
    format!("overlay.slot.{slot}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixed_id_is_unique() {
        let mut seen = std::collections::HashSet::new();
        for id in CONTROL_IDS {
            assert!(seen.insert(*id), "duplicate control id {id}");
        }
    }

    #[test]
    fn indexed_families_need_a_one_based_number() {
        assert!(is_control_id("gallery.card.1"));
        assert!(is_control_id("timeline.card.12"));
        assert!(!is_control_id("gallery.card.0"));
        assert!(!is_control_id("gallery.card."));
        assert!(!is_control_id("gallery.card.x"));
        assert!(!is_control_id("gallery.card"));
        assert!(!is_control_id("title.nope"));
    }

    #[test]
    fn the_slot_ids_are_in_the_fixed_table() {
        for slot in ["1", "2", "3", "autosave"] {
            assert!(is_control_id(&slot_control(slot)));
        }
    }
}
