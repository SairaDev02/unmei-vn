//! The Esc overlay: rail, top bar, slot cards, the confirm surface and the
//! Legacy picker.
//!
//! This root composes the overlay's chrome — the rail, the top bar and the
//! sub-view dispatch — with its confirm binding and its one remaining dialog,
//! and re-exports them, so every `overlay::<Item>` path callers already use
//! stays the same.

mod chrome;
mod confirm;
mod dialogs;

pub(super) use chrome::build_overlay;
pub(super) use confirm::confirm_dialog;
