//! The names whose content is text the engine draws, so a plugin must give
//! each a face and a tone (spec §4, "Engine text", rule 36).
//!
//! [`TEXT`] is the one list, and it mirrors the format page's per-surface
//! `Shows` column: a name is in when the engine draws words inside it. A row
//! that shows its words through a sibling slot is not — a title row's label is
//! `title.row.label`, and the row's control paints the focus ring and nothing
//! else. The tables the names come from are [`super::interface`]'s.

use super::interface::SURFACES;

/// The slots and controls whose content is text the engine draws.
const TEXT: &[&str] = &[
    "pack.name",
    "title.continue-reason",
    "title.tagline",
    "title.row.label",
    "title.row.reason",
    "prose",
    "speaker",
    "deciding",
    "scene-end",
    "choice.number",
    "choice.text",
    "play.save",
    "play.load",
    "play.auto",
    "play.skip",
    "play.menu",
    "settings.heading",
    "settings.key-state",
    "settings.key-path",
    "settings.key-placeholder",
    "settings.copy-path-label",
    "settings.audio.label",
    "settings.audio.value",
    "settings.mute",
    "settings.reset-audio",
    "settings.text-speed",
    "settings.auto",
    "settings.fullscreen",
    "settings.reset-play",
    "settings.key",
    "settings.clear-key",
    "settings.back",
    "contents.scene",
    "contents.section.label",
    "contents.ledger.label",
    "contents.ledger.detail",
    "contents.ledger.saved-at",
    "contents.ledger.action",
    "overlay.resume",
    "overlay.exit-to-title",
    "gallery.empty",
    "gallery.card.label",
    "still.label",
    "timeline.explanation",
    "timeline.empty",
    "timeline.card.label",
    "legacy.heading",
    "legacy.target.label",
    "legacy.target.detail",
    "legacy.cancel",
    "confirm.prompt",
    "confirm.accept-label",
    "confirm.detail",
    "confirm.no",
];

/// Whether the slot or control `name` shows text the engine draws.
pub fn shows_text(name: &str) -> bool {
    TEXT.contains(&name)
}

/// The names a surface lists that draw no words of the engine's, sorted by why.
///
/// Two families: a container — a list, a row, a marker slot or the backdrop —
/// holds other nodes and draws nothing itself, and a control is a hit area whose
/// words are drawn by a slot beside it, such as the title rows' labels or a
/// slider's value. [`TEXT`] holds the rest, so a name added to a surface has to
/// land in one of the two lists before [`every_name_draws_text_or_is_listed_here`]
/// passes.
#[cfg(test)]
const NOT_TEXT: &[&str] = &[
    "advance-marker",
    "choices",
    "confirm.yes",
    "contents.ledger",
    "contents.sections",
    "deciding-marker",
    "gallery.card.<n>",
    "gallery.card.art",
    "gallery.cards",
    "legacy.target.1",
    "legacy.target.2",
    "legacy.target.3",
    "legacy.targets",
    "overlay.gallery",
    "overlay.legacy",
    "overlay.load-tab",
    "overlay.save-tab",
    "overlay.settings",
    "overlay.slot.1",
    "overlay.slot.2",
    "overlay.slot.3",
    "overlay.slot.autosave",
    "overlay.timeline",
    "quick-row",
    "settings.audio-rows",
    "settings.copy-path",
    "settings.master",
    "settings.music",
    "settings.sfx",
    "still.art",
    "still.entry",
    "timeline.card.<n>",
    "timeline.cards",
    "title.backdrop",
    "title.continue",
    "title.gallery",
    "title.new-game",
    "title.quit",
    "title.rows",
    "title.settings",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every slot and control name a surface lists is either one the engine
    /// draws words into or one [`NOT_TEXT`] says it does not, so adding a surface
    /// name forces the choice instead of silently skipping the rule.
    #[test]
    fn every_name_draws_text_or_is_listed_here() {
        let mut listed = BTreeSet::new();
        for spec in SURFACES {
            listed.extend(spec.slots.iter().copied());
            listed.extend(spec.optional_slots.iter().copied());
            listed.extend(spec.controls.iter().copied());
            for list in spec.lists {
                listed.insert(list.slot);
                listed.extend(list.item_slots.iter().copied());
                listed.extend(list.optional_item_slots.iter().copied());
                listed.extend(list.item_controls.iter().copied());
            }
        }
        let text: BTreeSet<&str> = TEXT.iter().copied().collect();
        let silent: BTreeSet<&str> = NOT_TEXT.iter().copied().collect();
        assert!(
            text.is_disjoint(&silent),
            "a name is in both lists: {:?}",
            text.intersection(&silent).collect::<Vec<_>>()
        );
        let sorted: BTreeSet<&str> = text.union(&silent).copied().collect();
        assert_eq!(listed, sorted, "every listed name is in TEXT or NOT_TEXT");
    }

    /// Every name a surface lists, as `(name, is_a_control)`.
    fn listed() -> BTreeSet<&'static str> {
        let mut names = BTreeSet::new();
        for spec in SURFACES {
            names.extend(spec.slots.iter().copied());
            names.extend(spec.optional_slots.iter().copied());
            names.extend(spec.controls.iter().copied());
            for list in spec.lists {
                names.extend(list.item_slots.iter().copied());
                names.extend(list.optional_item_slots.iter().copied());
                names.extend(list.item_controls.iter().copied());
            }
        }
        names
    }

    #[test]
    fn every_text_name_is_a_slot_or_control_of_a_surface() {
        let listed = listed();
        for name in TEXT {
            assert!(
                listed.contains(name),
                "`{name}` is no surface's slot or control"
            );
        }
    }

    #[test]
    fn the_text_list_holds_each_name_once() {
        let unique: BTreeSet<&&str> = TEXT.iter().collect();
        assert_eq!(unique.len(), TEXT.len(), "the text list repeats a name");
    }
}
