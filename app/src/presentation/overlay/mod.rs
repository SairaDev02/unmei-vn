//! The Esc overlay: rail, top bar, slot cards, confirmation and the
//! Legacy picker.
//!
//! This root composes the overlay's chrome — the rail, the top bar and the
//! sub-view dispatch — with its two dialogs, and re-exports them, so every
//! `overlay::<Item>` path callers already use stays the same.

mod chrome;
mod dialogs;

pub(super) use chrome::build_overlay;
pub(super) use dialogs::confirm_dialog;
