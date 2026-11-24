//! The table itself: every keyed interface string, its placeholders and its
//! reference text.
//!
//! The module above owns the lookups and this one owns their data, so neither
//! file carries the other's bulk, and the ids stay together so a surface's keys
//! are added in one place.

use super::CopyKey;

/// The keyed copy of every surface a player can read, grouped by the surface
/// that draws it and listed in the spec's §5 surface order: `title`, `play`,
/// `contents`, `settings`, `gallery`, `still`, `timeline`, `legacy-picker`,
/// `confirm`. `title-settings` draws the `settings` body from its own door, so
/// it shares those keys. The engine owns the ids and the placeholders; a
/// plugin's `copy` overrides only replace a value, so this list stays the
/// reference text.
pub const KEYS: &[CopyKey] = &[
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
    CopyKey {
        id: "play.quick.save",
        placeholders: &[],
        text: "Save",
    },
    CopyKey {
        id: "play.quick.load",
        placeholders: &[],
        text: "Load",
    },
    CopyKey {
        id: "play.quick.auto",
        placeholders: &[],
        text: "Auto A",
    },
    CopyKey {
        id: "play.quick.skip",
        placeholders: &[],
        text: "Skip Tab",
    },
    CopyKey {
        id: "play.quick.menu",
        placeholders: &[],
        text: "Menu Esc",
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
    // A state line carries the fingerprint's four-to-six characters, never
    // more of the key.
    CopyKey {
        id: "settings.key.not-set",
        placeholders: &[],
        text: "Not set",
    },
    CopyKey {
        id: "settings.key.set",
        placeholders: &["fingerprint"],
        text: "Set \u{2014} {fingerprint}",
    },
    CopyKey {
        id: "settings.key.set-unchecked",
        placeholders: &["fingerprint"],
        text: "Set \u{2014} {fingerprint} \u{00b7} not checked (see the log)",
    },
    CopyKey {
        id: "settings.key.checking",
        placeholders: &[],
        text: "Checking\u{2026}",
    },
    CopyKey {
        id: "settings.key.invalid-shape",
        placeholders: &[],
        text: "Invalid \u{2014} does not look like a key",
    },
    CopyKey {
        id: "settings.key.invalid-rejected",
        placeholders: &[],
        text: "Invalid \u{2014} rejected by NanoGPT",
    },
    CopyKey {
        id: "settings.key.invalid-openrouter",
        placeholders: &[],
        text: "Invalid \u{2014} an OpenRouter key; enter a NanoGPT key",
    },
    CopyKey {
        id: "settings.key.from-environment",
        placeholders: &["fingerprint"],
        text: "From environment \u{2014} {fingerprint}",
    },
    CopyKey {
        id: "settings.key.from-environment-openrouter",
        placeholders: &[],
        text: "From environment \u{2014} Invalid, an OpenRouter key",
    },
    CopyKey {
        id: "settings.key.env-note",
        placeholders: &[],
        text: "NANOGPT_API_KEY overrides the stored key. Unset it and relaunch to use the stored key.",
    },
    CopyKey {
        id: "settings.key.placeholder",
        placeholders: &[],
        text: "Paste or type a NanoGPT key",
    },
    CopyKey {
        id: "settings.key.path",
        placeholders: &["path"],
        text: "Stored in {path}",
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
    // The gallery grid, from the title's door and the overlay's alike.
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
    // The gallery's still view, which replaces the grid inside the same surface.
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
];
