//! The pause surface's state and focus model, the confirmation kinds and the
//! legacy picker's own state.

use bevy::prelude::*;
use unmei_engine::save::{legacy_save, list_slots, Slot, SlotInfo, SlotStatus};
use unmei_engine::Pack;

use super::cards::{format_saved_at, slot_index};

/// Which surface the pause layer shows: the boot title, the slot grid, the
/// Settings view, the gallery or the Timeline. The title is a fifth view on the
/// same state, not a Bevy `States` migration (`docs/title-and-menu.md` § Title
/// §1); `menu_closed` freezes the play path for it exactly as it does for the
/// overlay. Timeline is overlay-only: its one door is the slot grid's top bar
/// and it has no title entry (`docs/title-and-menu.md` § Timeline §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuView {
    Title,
    Slots,
    Settings,
    Gallery,
    Timeline,
}

/// Which door opened the Settings or Gallery view. The back control and the
/// overlay's `Exit to title` rail control both branch on it
/// (`docs/title-and-menu.md` § Title §4, § Gallery `### Behavior` §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOrigin {
    /// The Esc overlay's top bar.
    Slots,
    /// The title's Settings row.
    Title,
}

/// The Save / Load segmented toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuMode {
    Save,
    Load,
}

/// What the keyboard/pointer currently points at. Focus is always visible (the
/// presentation highlights whichever control matches this).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuFocus {
    /// A card in `Slot::ALL` order (`0..=3`).
    Card(usize),
    /// A card in the gallery's entry order (`Pack::gallery_entries()`), walked
    /// by the arrow keys (`docs/title-and-menu.md` § Gallery `### Behavior`
    /// §4, `### Layout` §5).
    GalleryCard(usize),
    /// A card in the Timeline's declared order (`Pack::timeline_entries()`),
    /// walked by the arrow keys (`docs/title-and-menu.md` § Timeline §2,
    /// `### Layout` `#### Walk and scrolling`).
    TimelineCard(usize),
    SaveTab,
    LoadTab,
    SettingsButton,
    Resume,
    SettingsBack,
    /// The one-time Legacy save card, when it is shown.
    LegacyCard,
    /// A row of the legacy slot picker (`0..=2` → `Manual(1..=3)`).
    LegacyTarget(usize),
    /// The legacy picker's cancel control.
    LegacyCancel,
    /// The confirmation dialog's accept button.
    ConfirmYes,
    /// The confirmation dialog's cancel button.
    ConfirmNo,
    /// The Settings view's Master slider.
    AudioMaster,
    /// The Settings view's Music slider.
    AudioMusic,
    /// The Settings view's SFX slider.
    AudioSfx,
    /// The Settings view's Mute toggle.
    AudioMute,
    /// The Settings view's audio-only Reset control.
    AudioReset,
    /// The Settings view's stepped Text-speed control (`play.text_speed`).
    TextSpeed,
    /// The Settings view's Auto switch (`play.auto`).
    PlayAuto,
    /// The Settings view's Fullscreen switch (`play.fullscreen`).
    PlayFullscreen,
    /// The Settings view's play-only Reset control.
    PlayReset,
    /// The Decisions group's key row (`docs/title-and-menu.md`
    /// § Decisions settings §4).
    KeyRow,
    /// The Decisions group's `Copy path` control.
    CopyKeyPath,
    /// The Decisions group's `Clear key` control.
    ClearKey,
    /// The title's `Continue` row.
    TitleContinue,
    /// The title's `New Game` row.
    TitleNewGame,
    /// The title's `Settings` row.
    TitleSettings,
    /// The title's `Gallery` row.
    TitleGallery,
    /// The title's `Quit` row.
    TitleQuit,
    /// The overlay top bar's `Timeline` control, immediately left of `Gallery`
    /// and rendered only while the slot grid is up
    /// (`docs/title-and-menu.md` § Timeline §1).
    TimelineButton,
    /// The overlay top bar's `Gallery` control, immediately left of Settings
    /// and rendered only while the slot grid is up.
    GalleryButton,
}

/// A confirmation awaiting an answer. `Overwrite` and `Load` each ask once;
/// cancelling leaves the target file byte-identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    Overwrite(Slot),
    Load(Slot),
    /// The Settings view's audio-only Reset.
    ResetAudio,
    /// The Settings view's play-only Reset.
    ResetPlay,
    /// The title's `New Game` with a live session to discard.
    NewGame,
    /// The title's `Quit` with a live session to lose.
    Quit,
    /// The overlay Settings rail's `Exit to title`.
    ExitToTitle,
    /// The Decisions group's `Clear key` confirmation.
    ClearKey,
    /// The Timeline's one confirm, naming the declared card index. The scene id
    /// is read from `Pack::timeline_entries()` at render and jump time, so this
    /// stays `Copy` (`docs/title-and-menu.md` § Timeline §3).
    TimelineJump(usize),
}
/// The whole pause-surface state. `open` is the only thing a closed app reads.
#[derive(Resource, Debug, Clone)]
pub struct MenuState {
    pub open: bool,
    pub view: MenuView,
    pub mode: MenuMode,
    pub focus: MenuFocus,
    pub confirm: Option<ConfirmKind>,
    /// The open legacy slot picker, if any.
    pub picker: Option<LegacyPicker>,
    /// `list_slots` read once per open, and re-read after a save (or import).
    pub slots: Vec<SlotInfo>,
    /// Whether the one-time legacy file is importable right now. Drives the
    /// Legacy card, in save mode only.
    pub legacy_importable: bool,
    /// Whether play was entered this launch — the title's `Continue` precedence,
    /// `New Game` and `Quit` confirms, and the `Exit to title` rail control all
    /// branch on it through [`live_session`]. Never persisted, cleared only by
    /// app exit.
    pub(super) live_session: bool,
    /// Which door opened the current Settings view (only meaningful while
    /// `view == Settings`).
    pub settings_origin: MenuOrigin,
    /// Which door opened the current gallery view (only meaningful while
    /// `view == Gallery`). Recorded at open, every way out returns to it
    /// (`docs/title-and-menu.md` § Gallery `### Behavior` §3).
    pub gallery_origin: MenuOrigin,
    /// The gallery grid's current column count, recomputed from the laid-out
    /// grid so `↑`/`↓` step by exactly one row (`docs/title-and-menu.md`
    /// § Gallery `### Layout` §5). Three at the 1280×720 reference window.
    pub gallery_columns: usize,
    /// Whether the still view is up: one entry alone, contain-fit above the
    /// foot caption strip, over the gallery's own backdrop. The grid's focused
    /// card and the still's current entry are the one value [`Self::focus`]
    /// holds, so the caption and the walk cannot drift apart
    /// (`docs/title-and-menu.md` § Gallery `### Behavior` §5).
    pub gallery_still: bool,
    /// Whether the Decisions key field is being edited. While true the `↑`/`↓`
    /// walk is paused and the built-in widget owns typing; `Enter` commits and
    /// returns to the walk, `Esc` discards and leaves the field
    /// (`docs/title-and-menu.md` § Decisions settings §4.2).
    pub key_editing: bool,
    /// Continue's reason line, recomputed on every title entry.
    pub continue_line: String,
    /// Whether Continue is selectable (a live session, or a loadable autosave).
    pub continue_enabled: bool,
}

impl Default for MenuState {
    fn default() -> Self {
        MenuState {
            open: false,
            view: MenuView::Slots,
            mode: MenuMode::Save,
            focus: MenuFocus::Card(0),
            confirm: None,
            picker: None,
            slots: Vec::new(),
            legacy_importable: false,
            live_session: false,
            settings_origin: MenuOrigin::Slots,
            gallery_origin: MenuOrigin::Slots,
            gallery_columns: 3,
            gallery_still: false,
            key_editing: false,
            continue_line: String::new(),
            continue_enabled: false,
        }
    }
}

impl MenuState {
    /// Read `list_slots` into the cached cards, and re-derive whether the
    /// one-time legacy entry is still importable.
    pub fn refresh(&mut self, pack: &Pack) {
        self.slots = list_slots(pack).to_vec();
        self.legacy_importable = legacy_save(pack).is_some();
    }

    /// Whether the title is the surface that is up.
    pub fn title_up(&self) -> bool {
        self.open && self.view == MenuView::Title
    }

    /// Whether a surface reached from the title is up: the title itself, or the
    /// Settings or gallery view its rows opened. The play path stays silent and
    /// undrawn behind all of them, so the presentation's suppression reads this
    /// rather than [`Self::title_up`], which is the title's own input branch
    /// (`docs/title-and-menu.md` § Title §1, § Gallery `### Behavior` §6).
    pub fn title_surface_up(&self) -> bool {
        if !self.open {
            return false;
        }
        match self.view {
            MenuView::Title => true,
            MenuView::Settings => self.settings_origin == MenuOrigin::Title,
            MenuView::Gallery => self.gallery_origin == MenuOrigin::Title,
            MenuView::Slots | MenuView::Timeline => false,
        }
    }

    /// Enter the title: read the slots once (Continue's metadata), recompute
    /// Continue's line and enablement, and drop any half-open overlay state.
    /// Focus starts on Continue, or on New Game when Continue is disabled.
    pub fn enter_title(&mut self, pack: &Pack, live_scene: &str) {
        self.open = true;
        self.view = MenuView::Title;
        self.mode = MenuMode::Save;
        self.confirm = None;
        self.picker = None;
        self.key_editing = false;
        self.refresh(pack);
        self.refresh_continue(live_scene);
        self.focus = if self.continue_enabled {
            MenuFocus::TitleContinue
        } else {
            MenuFocus::TitleNewGame
        };
    }

    /// Recompute Continue's reason line and whether it is selectable. A live
    /// session resumes verbatim; otherwise the autosave is classified exactly
    /// as the overlay's card classifies it.
    pub fn refresh_continue(&mut self, live_scene: &str) {
        if live_session(self) {
            self.continue_line = live_scene.to_string();
            self.continue_enabled = true;
            return;
        }
        match self.slots.get(slot_index(Slot::Autosave)) {
            Some(info) => match &info.status {
                SlotStatus::Ok { scene_id, saved_at } => {
                    self.continue_line =
                        format!("{} \u{00b7} {}", scene_id, format_saved_at(*saved_at));
                    self.continue_enabled = true;
                }
                SlotStatus::Unreadable { reason } => {
                    // The engine's own string, verbatim — one source of truth
                    // with the overlay's card.
                    self.continue_line = reason.clone();
                    self.continue_enabled = false;
                }
                SlotStatus::Empty => {
                    self.continue_line = "No autosave yet".to_string();
                    self.continue_enabled = false;
                }
            },
            None => {
                self.continue_line = "No autosave yet".to_string();
                self.continue_enabled = false;
            }
        }
    }
}

/// The open legacy slot picker: which `Manual(1..=3)` target is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyPicker {
    /// `0..=2`, mapping to `Slot::Manual(target + 1)`.
    pub target: usize,
}

/// How many manual slots the legacy picker offers.
pub const LEGACY_TARGETS: usize = 3;

/// The `Slot` a picker row names (row `0` → `Manual(1)`).
pub fn legacy_slot(index: usize) -> Slot {
    debug_assert!(
        index < LEGACY_TARGETS,
        "picker row {index} is outside 0..{LEGACY_TARGETS}"
    );
    Slot::Manual(index as u8 + 1)
}

/// The Legacy card is shown only in **save mode**, in the grid view, and only
/// while the legacy file is importable ([`docs/runtime-hardening.md` § Menu
/// overlay §4]).
///
/// [`docs/runtime-hardening.md` § Menu overlay §4]: ../../docs/runtime-hardening.md
pub fn legacy_visible(menu: &MenuState) -> bool {
    menu.mode == MenuMode::Save && menu.view == MenuView::Slots && menu.legacy_importable
}

/// The run condition every in-play input system shares: the script is frozen
/// while the overlay is open.
pub fn menu_closed(menu: Res<MenuState>) -> bool {
    !menu.open
}

/// The one predicate the four live-session branches share — Continue's
/// precedence, New Game's confirm, Quit's confirm and whether `Exit to title`
/// is offered — so the flag can never be read stale in one place. The flag is
/// launch-scoped and never persisted; a relaunch always starts from the
/// autosave.
pub fn live_session(menu: &MenuState) -> bool {
    menu.live_session
}
