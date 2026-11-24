//! The per-surface interface tables: what each surface requires, what it also
//! allows, and which copy keys it may show (spec §5).
//!
//! A table is the surface's contract with a plugin: the engine's content and
//! controls appear as named slots, so a tree that places a name the surface does
//! not have is a mistake and not a restyling. The tables hold names only. The
//! text behind a copy key and its placeholders are [`crate::copy`]'s, and the
//! node a name refers to is [`super::node`]'s.
//!
//! Two names are not engine content: [`super::ORNAMENT_SLOTS`] carry the
//! plugin's own marker nodes, which is why a loop is allowed inside them
//! (spec §6).
//!
//! Every table below is the reference the format page documents, and the page is
//! checked against these tables, so a slot added here is a slot the page must
//! list.

/// One surface's list: the slot the list fills and what one item's template
/// places.
///
/// A list's `slot` is also one of its surface's required slots, because the
/// engine fills that slot with the list's items.
#[derive(Clone, Copy, Debug)]
pub struct ListSpec {
    /// The list's slot, such as `title.rows`.
    pub slot: &'static str,
    /// The item slots every item places.
    pub item_slots: &'static [&'static str],
    /// The item slots an item may also place.
    pub optional_item_slots: &'static [&'static str],
    /// The controls one item places, one row each: the engine shows the row's
    /// own control and lays every control out in the item template. An entry
    /// ending in `.<n>` is an indexed family (`gallery.card.<n>`); the template
    /// places one member and the engine numbers it per item.
    pub item_controls: &'static [&'static str],
}

/// One surface's interface (spec §5).
#[derive(Clone, Copy, Debug)]
pub struct SurfaceSpec {
    /// The surface id, which is also one of [`super::SURFACE_IDS`].
    pub id: &'static str,
    /// The slots a tree must place, engine content and lists alike.
    pub slots: &'static [&'static str],
    /// The slots a tree may also place.
    pub optional_slots: &'static [&'static str],
    /// The controls a tree must place, named by Keiko control id. A control a
    /// placed list carries in its item template counts as placed.
    pub controls: &'static [&'static str],
    /// The surface's lists, each also one of [`Self::slots`].
    pub lists: &'static [ListSpec],
    /// The copy keys a `text` node on this surface may name.
    pub copy_keys: &'static [&'static str],
}

const TITLE_ROWS: ListSpec = ListSpec {
    slot: "title.rows",
    item_slots: &["title.row.label"],
    optional_item_slots: &["title.row.reason"],
    item_controls: &[
        "title.continue",
        "title.new-game",
        "title.settings",
        "title.gallery",
        "title.quit",
    ],
};

/// The Settings body's three audio rows, which the title door draws too.
const AUDIO_ROWS: ListSpec = ListSpec {
    slot: "settings.audio-rows",
    item_slots: &["settings.audio.label", "settings.audio.value"],
    optional_item_slots: &[],
    item_controls: &["settings.master", "settings.music", "settings.sfx"],
};

/// The play frame's options, in script order. A choice button has no Keiko id
/// yet, so the item places no control.
const CHOICES: ListSpec = ListSpec {
    slot: "choices",
    item_slots: &["choice.number", "choice.text"],
    optional_item_slots: &[],
    item_controls: &[],
};

const CONTENTS_SECTIONS: ListSpec = ListSpec {
    slot: "contents.sections",
    item_slots: &["contents.section.label"],
    optional_item_slots: &[],
    item_controls: &[
        "overlay.save-tab",
        "overlay.load-tab",
        "overlay.timeline",
        "overlay.gallery",
        "overlay.settings",
    ],
};

/// The ledger's rows: the three manual slots, the autosave and the legacy entry
/// the engine appends in save mode.
const CONTENTS_LEDGER: ListSpec = ListSpec {
    slot: "contents.ledger",
    item_slots: &[
        "contents.ledger.label",
        "contents.ledger.detail",
        "contents.ledger.saved-at",
        "contents.ledger.action",
    ],
    optional_item_slots: &[],
    item_controls: &[
        "overlay.slot.1",
        "overlay.slot.2",
        "overlay.slot.3",
        "overlay.slot.autosave",
        "overlay.legacy",
    ],
};

const GALLERY_CARDS: ListSpec = ListSpec {
    slot: "gallery.cards",
    item_slots: &["gallery.card.label", "gallery.card.art"],
    optional_item_slots: &[],
    item_controls: &["gallery.card.<n>"],
};

/// The still view's one entry, browsed left and right.
const STILL_ENTRY: ListSpec = ListSpec {
    slot: "still.entry",
    item_slots: &["still.label", "still.art"],
    optional_item_slots: &[],
    item_controls: &[],
};

const TIMELINE_CARDS: ListSpec = ListSpec {
    slot: "timeline.cards",
    item_slots: &["timeline.card.label"],
    optional_item_slots: &[],
    item_controls: &["timeline.card.<n>"],
};

const LEGACY_TARGETS: ListSpec = ListSpec {
    slot: "legacy.targets",
    item_slots: &["legacy.target.label", "legacy.target.detail"],
    optional_item_slots: &[],
    item_controls: &["legacy.target.1", "legacy.target.2", "legacy.target.3"],
};

/// The Settings body plus its back control. Both doors draw the way out
/// (`← Back to slots` from the contents page, `← Back to title` from the
/// title), so the surface that lays the body out can reach it.
const SETTINGS_CONTROLS: &[&str] = &[
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
];

/// The Settings body's words. The `title-settings` door draws that body, so both
/// surfaces may show them; `settings.back.title` is the door's own label.
const SETTINGS_KEYS: &[&str] = &[
    "settings.heading",
    "settings.group.audio",
    "settings.group.play",
    "settings.group.decisions",
    "settings.control.master",
    "settings.control.music",
    "settings.control.sfx",
    "settings.control.mute",
    "settings.control.reset-audio",
    "settings.control.text-speed",
    "settings.control.auto",
    "settings.control.fullscreen",
    "settings.control.reset-play",
    "settings.control.copy-path",
    "settings.control.copy-path.copied",
    "settings.control.clear-key",
    "settings.value.on",
    "settings.value.off",
    "settings.key.not-set",
    "settings.key.set",
    "settings.key.set-unchecked",
    "settings.key.checking",
    "settings.key.invalid-shape",
    "settings.key.invalid-rejected",
    "settings.key.invalid-openrouter",
    "settings.key.from-environment",
    "settings.key.from-environment-openrouter",
    "settings.key.env-note",
    "settings.key.placeholder",
    "settings.key.path",
    "settings.back.slots",
    "settings.back.title",
    "settings.text-speed.instant",
    "settings.text-speed.fast",
    "settings.text-speed.normal",
    "settings.text-speed.slow",
    "settings.text-speed.slowest",
];

/// The ten surfaces of format version 1, in the order the surface trees are
/// listed. A surface with no tree keeps the reference plugin's, so a table is a
/// set of rules for the tree a plugin writes and not a demand that it write one.
pub const SURFACES: &[SurfaceSpec] = &[
    SurfaceSpec {
        id: "title",
        slots: &["pack.name", "title.continue-reason", "title.rows"],
        optional_slots: &["title.tagline", "title.backdrop"],
        controls: &[
            "title.continue",
            "title.new-game",
            "title.settings",
            "title.gallery",
            "title.quit",
        ],
        lists: &[TITLE_ROWS],
        copy_keys: &[
            "title.row.continue",
            "title.row.new-game",
            "title.row.settings",
            "title.row.gallery",
            "title.row.quit",
            "title.legend",
            "title.continue.empty",
            "title.continue.autosave",
        ],
    },
    SurfaceSpec {
        id: "title-settings",
        slots: &[
            "pack.name",
            "title.backdrop",
            "settings.audio-rows",
            "settings.key-state",
            "settings.key-path",
        ],
        optional_slots: &["settings.key-placeholder", "settings.copy-path-label"],
        controls: SETTINGS_CONTROLS,
        lists: &[AUDIO_ROWS],
        copy_keys: SETTINGS_KEYS,
    },
    SurfaceSpec {
        id: "play",
        slots: &[
            "prose",
            "advance-marker",
            "deciding-marker",
            "choices",
            "quick-row",
        ],
        optional_slots: &["speaker", "deciding", "scene-end"],
        controls: &[
            "play.save",
            "play.load",
            "play.auto",
            "play.skip",
            "play.menu",
        ],
        lists: &[CHOICES],
        copy_keys: &[
            "play.help",
            "play.cue.auto",
            "play.cue.skip",
            "play.deciding",
            "play.end-of-scene",
        ],
    },
    SurfaceSpec {
        id: "contents",
        slots: &["contents.sections", "contents.ledger"],
        optional_slots: &["pack.name", "contents.scene"],
        controls: &[
            "overlay.resume",
            "overlay.exit-to-title",
            "overlay.save-tab",
            "overlay.load-tab",
            "overlay.timeline",
            "overlay.gallery",
            "overlay.settings",
        ],
        lists: &[CONTENTS_SECTIONS, CONTENTS_LEDGER],
        copy_keys: &[
            "contents.rail.scene",
            "contents.rail.resume",
            "contents.rail.exit-to-title",
            "contents.tab.save",
            "contents.tab.load",
            "contents.tab.timeline",
            "contents.tab.gallery",
            "contents.tab.settings",
            "contents.card.legacy-import",
            "contents.slot.1",
            "contents.slot.2",
            "contents.slot.3",
            "contents.slot.autosave",
            "contents.slot.manual",
            "contents.slot.kind.manual",
            "contents.slot.kind.autosave",
            "contents.slot.status.empty",
            "contents.slot.status.ok",
            "contents.slot.status.unreadable",
            "contents.slot.none",
            "contents.slot.saved-at.none",
            "contents.action.save-here",
            "contents.action.overwrite",
            "contents.action.load",
            "contents.action.load-only",
        ],
    },
    SurfaceSpec {
        id: "settings",
        slots: &[
            "settings.heading",
            "settings.audio-rows",
            "settings.key-state",
            "settings.key-path",
        ],
        optional_slots: &["settings.key-placeholder", "settings.copy-path-label"],
        controls: SETTINGS_CONTROLS,
        lists: &[AUDIO_ROWS],
        copy_keys: SETTINGS_KEYS,
    },
    SurfaceSpec {
        id: "gallery",
        slots: &["gallery.cards"],
        optional_slots: &["gallery.empty"],
        controls: &[],
        lists: &[GALLERY_CARDS],
        copy_keys: &[
            "gallery.heading",
            "gallery.empty",
            "gallery.legend",
            "gallery.back.slots",
            "gallery.back.title",
        ],
    },
    SurfaceSpec {
        id: "still",
        slots: &["still.entry"],
        optional_slots: &[],
        controls: &[],
        lists: &[STILL_ENTRY],
        copy_keys: &["still.legend"],
    },
    SurfaceSpec {
        id: "timeline",
        slots: &["timeline.cards", "timeline.explanation"],
        optional_slots: &["timeline.empty"],
        controls: &[],
        lists: &[TIMELINE_CARDS],
        copy_keys: &[
            "timeline.heading",
            "timeline.back",
            "timeline.explanation",
            "timeline.empty",
            "timeline.locked",
            "timeline.legend",
        ],
    },
    SurfaceSpec {
        id: "legacy-picker",
        slots: &["legacy.heading", "legacy.targets"],
        optional_slots: &[],
        controls: &["legacy.cancel"],
        lists: &[LEGACY_TARGETS],
        copy_keys: &[
            "legacy-picker.heading",
            "legacy-picker.prompt",
            "legacy-picker.cancel",
            "legacy-picker.target.overwrite",
        ],
    },
    SurfaceSpec {
        id: "confirm",
        slots: &[
            "confirm.prompt",
            "confirm.accept-label",
            "confirm.yes",
            "confirm.no",
        ],
        optional_slots: &["confirm.detail"],
        controls: &["confirm.yes", "confirm.no"],
        lists: &[],
        copy_keys: &[
            "confirm.overwrite.title",
            "confirm.overwrite.prompt",
            "confirm.load.title",
            "confirm.load.prompt",
            "confirm.reset-audio.title",
            "confirm.reset-audio.prompt",
            "confirm.reset-play.title",
            "confirm.reset-play.prompt",
            "confirm.new-game.title",
            "confirm.new-game.prompt",
            "confirm.quit.title",
            "confirm.quit.prompt",
            "confirm.exit-to-title.title",
            "confirm.exit-to-title.prompt",
            "confirm.clear-key.title",
            "confirm.clear-key.prompt",
            "confirm.clear-key.none",
            "confirm.timeline-jump.title",
            "confirm.timeline-jump.detail",
            "confirm.button.yes",
            "confirm.button.start",
            "confirm.button.quit",
            "confirm.button.exit",
            "confirm.button.jump",
            "confirm.button.cancel",
        ],
    },
];

/// The table for the surface `id` names.
pub fn surface(id: &str) -> Option<&'static SurfaceSpec> {
    SURFACES.iter().find(|spec| spec.id == id)
}

/// The slots and controls whose content is text the engine draws, so a plugin
/// must give each a face and a tone (spec §4, "Engine text", rule 36).
///
/// A name is here when the engine draws words inside it. A row that shows its
/// words through a sibling slot is not: a title row's label is
/// `title.row.label`, and the row's control paints the focus ring and nothing
/// else. The format page's `Shows` column describes the same content.
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
