//! The Esc menu overlay: the pause surface's state, its card model, and its
//! input.
//!
//! Esc is a **presentation pause** ([`docs/runtime-hardening.md` § Menu overlay]):
//! it never advances the script, never cancels an in-flight Decisions request,
//! and never drops a player choice. It opens from every parked state — a
//! say-wait, a choice-wait, a decision-wait (even with a request in flight) and
//! the end of a scene. One key performs four moments, in this order: cancel an
//! open confirmation, back out of Settings to the grid, then close the overlay
//! (Resume).
//!
//! This module owns the *headless* half: [`MenuState`], the contextual card
//! model derived from [`unmei_engine::list_slots`], and the input/action
//! systems. The rendering lives in [`crate::presentation`], which reads the same
//! state so the two never diverge.
//!
//! The held-answer buffer lives in [`crate::core`] (`HeldDecision`): a completed
//! answer is buffered there while the overlay is open and applied on resume,
//! and a load cancels the request and discards the buffer through the
//! [`crate::session`] recipes.
//!
//! This root composes the submodules — the state/focus model, the copy and walk
//! tables, the card model, the pure navigation, the actions and their
//! application, the view transitions, the save/load/import paths, the Decisions
//! key field and the input glue — and re-exports their public items, so every
//! `menu::<Item>` path callers already use stays the same.
//!
//! [`docs/runtime-hardening.md` § Menu overlay]: ../../docs/runtime-hardening.md

mod action;
mod cards;
mod confirm;
mod credential_sync;
mod input;
mod key_field;
mod labels;
mod navigation;
mod persistence;
mod state;
mod transition;

pub(crate) use action::action_for_focus;
pub use action::MenuAction;
pub use cards::{
    card_action, card_model, card_models, format_saved_at, legacy_targets, slot_index, slot_label,
    CardAction, CardModel, LegacyTargetModel, SlotAction,
};
pub(crate) use credential_sync::{read_on_settings_open, sync_key_focus};
pub(crate) use input::menu_input;
// The key row's system parameters are read by the app's tests only.
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "the pre-split `menu::KeyRowInput` path stays valid; no test names it yet"
)]
pub(crate) use input::KeyRowInput;
pub use labels::{
    control_focus, focus_control, focus_track, gallery_back_label, GALLERY_EMPTY,
    GALLERY_GRID_LEGEND, GALLERY_HEADING, GALLERY_STILL_LEGEND, SETTINGS_ORDER, TIMELINE_BACK,
    TIMELINE_CONFIRM_DETAIL, TIMELINE_EMPTY, TIMELINE_EXPLANATION, TIMELINE_HEADING,
    TIMELINE_LEGEND, TIMELINE_LOCKED, TITLE_LEGEND, TITLE_ORDER,
};
pub use navigation::{gallery_browse, gallery_move, timeline_move, GalleryDir};
// The load-completion pair is called and named by the app's tests only.
#[cfg(test)]
pub(crate) use persistence::{complete_load, LoadSurface};
pub use state::{
    legacy_slot, legacy_visible, live_session, menu_closed, ConfirmKind, LegacyPicker, MenuFocus,
    MenuMode, MenuOrigin, MenuState, MenuView, LEGACY_TARGETS,
};
