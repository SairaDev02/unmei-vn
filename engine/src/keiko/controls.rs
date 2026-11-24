//! The control-id table: the one owner of every stable role id (spec §3.2).
//!
//! The player's `MenuFocus` enum maps one-to-one onto this table, and the UI
//! plugin's control names reuse the same strings. The ids live here so the
//! Bevy-free case validator can check them without depending on the runtime.
//!
//! One group is the exception: the play frame's quick row has its ids, because a
//! plugin places and checks them, and the player builds no control behind them
//! until the quick row ships. [`CONTROL_IDS_WITHOUT_FOCUS`] names that group, so
//! the one-to-one map starts at the first id the player does have. `play.log`
//! stays reserved for the Backlog, which is not built, so it has no id here.

/// The fixed control ids, in the player's walk order where one exists.
pub const CONTROL_IDS: &[&str] = &[
    "title.continue",
    "title.new-game",
    "title.settings",
    "title.gallery",
    "title.quit",
    "play.save",
    "play.load",
    "play.auto",
    "play.skip",
    "play.menu",
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
    "legacy.target.1",
    "legacy.target.2",
    "legacy.target.3",
    "legacy.cancel",
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
    "confirm.yes",
    "confirm.no",
];

/// The ids the player has no focus target for. The quick row is built with
/// variant E, and until then the ids exist for the plugin's tables and for Keiko
/// cases that name the row the reference plugin will draw.
pub const CONTROL_IDS_WITHOUT_FOCUS: &[&str] = &[
    "play.save",
    "play.load",
    "play.auto",
    "play.skip",
    "play.menu",
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

    /// The play frame's five quick-row ids are in the table, and `play.log` is
    /// reserved for the Backlog: no table names it, so a plugin that places it
    /// is placing a control the format does not have.
    #[test]
    fn the_quick_row_has_five_ids_and_a_reserved_log() {
        for id in CONTROL_IDS_WITHOUT_FOCUS {
            assert!(is_control_id(id), "`{id}` is a control id");
            assert!(CONTROL_IDS.contains(id), "`{id}` is in the fixed table");
        }
        assert_eq!(
            CONTROL_IDS_WITHOUT_FOCUS.len(),
            5,
            "the quick row is five controls"
        );
        assert!(
            !is_control_id("play.log"),
            "`play.log` is reserved for the Backlog"
        );
    }

    #[test]
    fn the_slot_ids_are_in_the_fixed_table() {
        for slot in ["1", "2", "3", "autosave"] {
            assert!(is_control_id(&slot_control(slot)));
        }
    }
}
