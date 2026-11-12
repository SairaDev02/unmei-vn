//! Player input: F5/F9 save/load, say-line advancement, choice picking, and the
//! `A` / `F11` comfort hotkeys.
//!
//! The app-input ownership boundary and its held-input quarantine live in
//! [`ownership`] and [`quarantine`]; this root keeps the paths every caller
//! already uses. The dispatch systems are grouped by what they act on:
//! [`shortcuts`] holds the developer and comfort keys, [`play`] the in-play
//! advance/skip/choice path.

mod ownership;
mod play;
mod quarantine;
mod shortcuts;

pub use ownership::{InputClaim, InputOwnership, InputOwnershipSet};
pub(crate) use play::{advance_say_input, choice_input, skip_input};
pub(crate) use quarantine::{app_input_ready, track_quarantine, InputQuarantine};
pub(crate) use shortcuts::{auto_input, fullscreen_input, save_load_input};
