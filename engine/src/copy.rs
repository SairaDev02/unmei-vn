//! The interface copy table: every string a surface shows, keyed, with the
//! placeholders that text carries.
//!
//! The player's surfaces read their text from here instead of holding literals,
//! so one owner decides what each screen says and a later ticket can check a
//! pack's UI-plugin `copy` overrides against [`KEYS`] ([spec §5], R6–R9). The
//! values are the player's current text, byte for byte: this table arrived as a
//! prefactor, so nothing a player sees changed.
//!
//! Three surfaces stay literal on purpose — the error screen, the Notice and the
//! Monitor — because a plugin never restyles them (R7). Engine-supplied reasons
//! (an unreadable slot's reason, for one) and pack content (say-lines, option
//! text, the pack name, the tagline, NPC names) are not interface copy either;
//! they reach a surface as engine slots, never as keys.
//!
//! An id is dotted and borrows the [Keiko control ids]' own vocabulary, so the
//! text a player reads about a control sits beside that control's name. Where a
//! label would collide with its control's id — `title.new-game` is a control —
//! the key names the text role instead: `title.row.new-game`.
//!
//! [`text`] is a `const fn` on purpose. The player's own copy is `const` and
//! `&'static str` typed, and a prefactor may not force a type change through
//! every caller; a plain function would have.
//!
//! [spec §5]: https://github.com/SairaDev02/unmei-vn/blob/main/docs/ui-plugin-spec.md#5-surfaces-slots-controls-and-copy-keys-r6r9-r25
//! [Keiko control ids]: crate::keiko::CONTROL_IDS

/// One keyed interface string: its id, the placeholders its text carries, and
/// that text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyKey {
    /// The dotted id, for example `confirm.overwrite.prompt`.
    pub id: &'static str,
    /// The placeholders [`Self::text`] carries, in the order it carries them,
    /// for example `&["slot"]`.
    pub placeholders: &'static [&'static str],
    /// The default text, with a `{name}` for each placeholder.
    pub text: &'static str,
}

/// The keyed copy of every surface a player can read, grouped by the surface
/// that draws it: `play`, `title`, `contents`, `legacy-picker`, `confirm`,
/// `settings`, `gallery`, `still` and `timeline`, in the spec's surface order.
/// The engine owns the ids and the placeholders; a plugin's `copy` overrides
/// only replace a value, so this list stays the reference text.
pub const KEYS: &[CopyKey] = &[
    // The play surface: the dialogue band, the cues and the two non-say parks.
    CopyKey {
        id: "play.help",
        placeholders: &[],
        text: "Space/Enter/click advance \u{00b7} Tab skip \u{00b7} Ctrl skip all \u{00b7} 1-9 or click choose \u{00b7} A auto \u{00b7} F11 fullscreen \u{00b7} Esc menu",
    },
    CopyKey {
        id: "play.cue.auto",
        placeholders: &[],
        text: "AUTO",
    },
    CopyKey {
        id: "play.cue.skip",
        placeholders: &[],
        text: "SKIP",
    },
    CopyKey {
        id: "play.deciding",
        placeholders: &["npc"],
        text: "{npc} is deciding…",
    },
    CopyKey {
        id: "play.end-of-scene",
        placeholders: &[],
        text: "— end of scene —",
    },
    // The title surface: the five rows, the legend and Continue's reason line.
    CopyKey {
        id: "title.row.continue",
        placeholders: &[],
        text: "Continue",
    },
    CopyKey {
        id: "title.row.new-game",
        placeholders: &[],
        text: "New Game",
    },
    CopyKey {
        id: "title.row.settings",
        placeholders: &[],
        text: "Settings",
    },
    CopyKey {
        id: "title.row.gallery",
        placeholders: &[],
        text: "Gallery",
    },
    CopyKey {
        id: "title.row.quit",
        placeholders: &[],
        text: "Quit",
    },
    CopyKey {
        id: "title.legend",
        placeholders: &[],
        text: "\u{2191}/\u{2193} move \u{00b7} Enter select \u{00b7} click to choose",
    },
    CopyKey {
        id: "title.continue.empty",
        placeholders: &[],
        text: "No autosave yet",
    },
    CopyKey {
        id: "title.continue.autosave",
        placeholders: &["scene", "saved_at"],
        text: "{scene} \u{00b7} {saved_at}",
    },
    // The contents surface: the Esc overlay's rail, tabs and slot cards.
    CopyKey {
        id: "contents.rail.scene",
        placeholders: &["scene"],
        text: "scene: {scene}",
    },
    CopyKey {
        id: "contents.rail.resume",
        placeholders: &[],
        text: "\u{25b6} Resume  (Esc)",
    },
    CopyKey {
        id: "contents.rail.exit-to-title",
        placeholders: &[],
        text: "Exit to title",
    },
    CopyKey {
        id: "contents.tab.save",
        placeholders: &[],
        text: "Save",
    },
    CopyKey {
        id: "contents.tab.load",
        placeholders: &[],
        text: "Load",
    },
    CopyKey {
        id: "contents.tab.timeline",
        placeholders: &[],
        text: "Timeline",
    },
    CopyKey {
        id: "contents.tab.gallery",
        placeholders: &[],
        text: "Gallery",
    },
    CopyKey {
        id: "contents.tab.settings",
        placeholders: &[],
        text: "\u{2699} Settings",
    },
    CopyKey {
        id: "contents.card.legacy-import",
        placeholders: &[],
        text: "Legacy save \u{2014} Import\u{2026}",
    },
    CopyKey {
        id: "contents.slot.1",
        placeholders: &[],
        text: "Slot 1",
    },
    CopyKey {
        id: "contents.slot.2",
        placeholders: &[],
        text: "Slot 2",
    },
    CopyKey {
        id: "contents.slot.3",
        placeholders: &[],
        text: "Slot 3",
    },
    CopyKey {
        id: "contents.slot.autosave",
        placeholders: &[],
        text: "Autosave",
    },
    CopyKey {
        id: "contents.slot.manual",
        placeholders: &[],
        text: "Slot",
    },
    CopyKey {
        id: "contents.slot.kind.manual",
        placeholders: &[],
        text: "manual",
    },
    CopyKey {
        id: "contents.slot.kind.autosave",
        placeholders: &[],
        text: "game-written",
    },
    CopyKey {
        id: "contents.slot.status.empty",
        placeholders: &[],
        text: "Empty",
    },
    CopyKey {
        id: "contents.slot.status.ok",
        placeholders: &[],
        text: "Ok",
    },
    CopyKey {
        id: "contents.slot.status.unreadable",
        placeholders: &[],
        text: "Unreadable",
    },
    CopyKey {
        id: "contents.slot.none",
        placeholders: &[],
        text: "\u{2014}",
    },
    CopyKey {
        id: "contents.slot.saved-at.none",
        placeholders: &[],
        text: "no save yet",
    },
    CopyKey {
        id: "contents.action.save-here",
        placeholders: &[],
        text: "Save here",
    },
    CopyKey {
        id: "contents.action.overwrite",
        placeholders: &[],
        text: "Overwrite",
    },
    CopyKey {
        id: "contents.action.load",
        placeholders: &[],
        text: "Load",
    },
    CopyKey {
        id: "contents.action.load-only",
        placeholders: &[],
        text: "Load-only",
    },
    // The legacy-save target picker.
    CopyKey {
        id: "legacy-picker.heading",
        placeholders: &[],
        text: "Import legacy save",
    },
    CopyKey {
        id: "legacy-picker.prompt",
        placeholders: &[],
        text: "Choose a slot. Occupied slots are overwritten.",
    },
    CopyKey {
        id: "legacy-picker.cancel",
        placeholders: &[],
        text: "Cancel",
    },
    CopyKey {
        id: "legacy-picker.target.overwrite",
        placeholders: &["slot"],
        text: "{slot}  (overwrite)",
    },
    // The one confirmation tree, one title/prompt/accept triple per kind.
    CopyKey {
        id: "confirm.overwrite.title",
        placeholders: &[],
        text: "Overwrite",
    },
    CopyKey {
        id: "confirm.overwrite.prompt",
        placeholders: &["slot"],
        text: "Overwrite {slot}? Its current save is replaced.",
    },
    CopyKey {
        id: "confirm.load.title",
        placeholders: &[],
        text: "Load",
    },
    CopyKey {
        id: "confirm.load.prompt",
        placeholders: &["slot"],
        text: "Load {slot}? Unsaved progress is lost and any decision in flight is cancelled.",
    },
    CopyKey {
        id: "confirm.reset-audio.title",
        placeholders: &[],
        text: "Reset audio",
    },
    CopyKey {
        id: "confirm.reset-audio.prompt",
        placeholders: &[],
        text: "Reset Master, Music and SFX to 100% and turn mute off? The file is not deleted.",
    },
    CopyKey {
        id: "confirm.reset-play.title",
        placeholders: &[],
        text: "Reset play",
    },
    CopyKey {
        id: "confirm.reset-play.prompt",
        placeholders: &[],
        text: "Reset text speed to Instant, turn auto off and return to windowed? The audio mix is untouched and the file is not deleted.",
    },
    CopyKey {
        id: "confirm.new-game.title",
        placeholders: &[],
        text: "Start a new game?",
    },
    CopyKey {
        id: "confirm.new-game.prompt",
        placeholders: &[],
        text: "The session in progress is discarded. Your saved slots and the autosave are untouched.",
    },
    CopyKey {
        id: "confirm.quit.title",
        placeholders: &[],
        text: "Quit?",
    },
    CopyKey {
        id: "confirm.quit.prompt",
        placeholders: &[],
        text: "Unsaved progress is lost and any decision in flight is cancelled.",
    },
    CopyKey {
        id: "confirm.exit-to-title.title",
        placeholders: &[],
        text: "Exit to the title?",
    },
    CopyKey {
        id: "confirm.exit-to-title.prompt",
        placeholders: &[],
        text: "Your session is kept \u{2014} Continue resumes it where you left off.",
    },
    CopyKey {
        id: "confirm.clear-key.title",
        placeholders: &[],
        text: "Clear key?",
    },
    CopyKey {
        id: "confirm.clear-key.prompt",
        placeholders: &["path"],
        text: "The stored key is deleted from {path}. You can paste it again at any time.",
    },
    CopyKey {
        id: "confirm.clear-key.none",
        placeholders: &[],
        text: "\u{2014}",
    },
    CopyKey {
        id: "confirm.timeline-jump.title",
        placeholders: &["scene"],
        text: "Jump to {scene}?",
    },
    CopyKey {
        id: "confirm.timeline-jump.detail",
        placeholders: &[],
        text: "Unsaved progress is lost and any decision in flight is cancelled. The scene restarts from the beginning. Current game state is retained.",
    },
    CopyKey {
        id: "confirm.button.yes",
        placeholders: &[],
        text: "Yes",
    },
    CopyKey {
        id: "confirm.button.start",
        placeholders: &[],
        text: "Start",
    },
    CopyKey {
        id: "confirm.button.quit",
        placeholders: &[],
        text: "Quit",
    },
    CopyKey {
        id: "confirm.button.exit",
        placeholders: &[],
        text: "Exit",
    },
    CopyKey {
        id: "confirm.button.jump",
        placeholders: &[],
        text: "Jump",
    },
    CopyKey {
        id: "confirm.button.cancel",
        placeholders: &[],
        text: "Cancel",
    },
    // The settings surface, and the title's door onto the same body.
    CopyKey {
        id: "settings.heading",
        placeholders: &[],
        text: "Settings",
    },
    CopyKey {
        id: "settings.group.audio",
        placeholders: &[],
        text: "Audio",
    },
    CopyKey {
        id: "settings.group.play",
        placeholders: &[],
        text: "Play",
    },
    CopyKey {
        id: "settings.group.decisions",
        placeholders: &[],
        text: "Decisions",
    },
    CopyKey {
        id: "settings.control.master",
        placeholders: &[],
        text: "Master",
    },
    CopyKey {
        id: "settings.control.music",
        placeholders: &[],
        text: "Music",
    },
    CopyKey {
        id: "settings.control.sfx",
        placeholders: &[],
        text: "SFX",
    },
    CopyKey {
        id: "settings.control.mute",
        placeholders: &[],
        text: "Mute",
    },
    CopyKey {
        id: "settings.control.reset-audio",
        placeholders: &[],
        text: "Reset audio",
    },
    CopyKey {
        id: "settings.control.text-speed",
        placeholders: &[],
        text: "Text speed",
    },
    CopyKey {
        id: "settings.control.auto",
        placeholders: &[],
        text: "Auto",
    },
    CopyKey {
        id: "settings.control.fullscreen",
        placeholders: &[],
        text: "Fullscreen",
    },
    CopyKey {
        id: "settings.control.reset-play",
        placeholders: &[],
        text: "Reset play",
    },
    CopyKey {
        id: "settings.control.copy-path",
        placeholders: &[],
        text: "Copy path",
    },
    CopyKey {
        id: "settings.control.copy-path.copied",
        placeholders: &[],
        text: "Copied",
    },
    CopyKey {
        id: "settings.control.clear-key",
        placeholders: &[],
        text: "Clear key",
    },
    CopyKey {
        id: "settings.value.on",
        placeholders: &[],
        text: "On",
    },
    CopyKey {
        id: "settings.value.off",
        placeholders: &[],
        text: "Off",
    },
    CopyKey {
        id: "settings.back.slots",
        placeholders: &[],
        text: "\u{2190} Back to slots",
    },
    CopyKey {
        id: "settings.back.title",
        placeholders: &[],
        text: "\u{2190} Back to title",
    },
    CopyKey {
        id: "settings.text-speed.instant",
        placeholders: &[],
        text: "Instant",
    },
    CopyKey {
        id: "settings.text-speed.fast",
        placeholders: &[],
        text: "Fast",
    },
    CopyKey {
        id: "settings.text-speed.normal",
        placeholders: &[],
        text: "Normal",
    },
    CopyKey {
        id: "settings.text-speed.slow",
        placeholders: &[],
        text: "Slow",
    },
    CopyKey {
        id: "settings.text-speed.slowest",
        placeholders: &[],
        text: "Slowest",
    },
    // The gallery, its still view and the Timeline, each from both doors.
    CopyKey {
        id: "gallery.heading",
        placeholders: &[],
        text: "Gallery",
    },
    CopyKey {
        id: "gallery.empty",
        placeholders: &[],
        text: "No images in this pack",
    },
    CopyKey {
        id: "gallery.legend",
        placeholders: &[],
        text: "\u{2191}/\u{2193}/\u{2190}/\u{2192} move \u{00b7} Enter view \u{00b7} click to open",
    },
    CopyKey {
        id: "gallery.back.slots",
        placeholders: &[],
        text: "\u{2190} Back to slots",
    },
    CopyKey {
        id: "gallery.back.title",
        placeholders: &[],
        text: "\u{2190} Back to title",
    },
    CopyKey {
        id: "still.legend",
        placeholders: &[],
        text: "\u{2190}/\u{2192} browse \u{00b7} \u{2191}/\u{2193} or Esc or click to close",
    },
    CopyKey {
        id: "timeline.heading",
        placeholders: &[],
        text: "Timeline",
    },
    CopyKey {
        id: "timeline.back",
        placeholders: &[],
        text: "\u{2190} Back to slots",
    },
    CopyKey {
        id: "timeline.explanation",
        placeholders: &[],
        text: "Choose a visited scene to start it from the beginning with your current state.",
    },
    CopyKey {
        id: "timeline.empty",
        placeholders: &[],
        text: "This pack has no Timeline.",
    },
    CopyKey {
        id: "timeline.locked",
        placeholders: &[],
        text: "???",
    },
    CopyKey {
        id: "timeline.legend",
        placeholders: &[],
        text: "\u{2191}/\u{2193}/\u{2190}/\u{2192} move \u{00b7} Enter/Space select \u{00b7} Esc back \u{00b7} click to choose",
    },
];

/// The key `id` names, or `None` when the table has no such id.
pub fn key(id: &str) -> Option<&'static CopyKey> {
    KEYS.iter().find(|key| key.id == id)
}

/// The text `id` names.
///
/// # Panics
///
/// Panics on an id the table does not hold. A `const` caller fails to compile
/// instead, at the constant that names the unknown id.
pub const fn text(id: &str) -> &'static str {
    let mut index = 0;
    while index < KEYS.len() {
        if str_eq(KEYS[index].id, id) {
            return KEYS[index].text;
        }
        index += 1;
    }
    panic!("unknown copy key")
}

/// The text of `id` with each placeholder `values` supplies replaced by its
/// value.
///
/// One pass over the source text, so a substituted value is never read again: a
/// copied path may hold `{`, and it must survive verbatim. A declared
/// placeholder that `values` omits keeps its own `{name}`, which lets a caller
/// fill a line in stages.
///
/// # Panics
///
/// Panics on an id the table does not hold.
pub fn render(id: &str, values: &[(&str, &str)]) -> String {
    let key = key(id).unwrap_or_else(|| panic!("unknown copy key: {id}"));
    let mut rendered = String::with_capacity(key.text.len());
    let mut rest = key.text;
    while let Some((before, after)) = rest.split_once('{') {
        rendered.push_str(before);
        let Some((name, tail)) = after.split_once('}') else {
            rendered.push('{');
            rest = after;
            continue;
        };
        match values.iter().find(|(placeholder, _)| *placeholder == name) {
            Some((_, value)) => rendered.push_str(value),
            None => {
                rendered.push('{');
                rendered.push_str(name);
                rendered.push('}');
            }
        }
        rest = tail;
    }
    rendered.push_str(rest);
    rendered
}

/// Whether two strings are equal, for the `const` lookup above: `==` on `str` is
/// not usable in a `const fn`.
const fn str_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

#[cfg(test)]
mod tests;
