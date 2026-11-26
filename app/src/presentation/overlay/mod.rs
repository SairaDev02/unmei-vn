//! The contents page: the plugin-drawn page, the confirm surface and the Legacy
//! picker.
//!
//! This root composes the page drawn from the `contents` surface with the
//! confirm binding and the legacy picker's own surface — and re-exports them, so
//! every `overlay::<Item>` path callers already use stays the same.

mod confirm;
mod dialogs;
mod page;

pub(super) use confirm::confirm_dialog;
pub(super) use page::build_page;
