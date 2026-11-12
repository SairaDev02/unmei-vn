//! The created window: its reference size, its resize floor, and the
//! `play.fullscreen` → `WindowMode` mapping.

use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode};

/// The reference window size — 1280×720 — and the floor the windowed layout
/// supports. Sub-1280 layouts are consciously unsupported rather than
/// responsive (`docs/runtime-hardening.md` § Player comfort §5), so the same
/// size seeds both the created window's resolution and its `resize_constraints`
/// minimum.
const WINDOW_SIZE: (u32, u32) = (1280, 720);

/// The window the app builds, in the fixed 1280×720 reference size with a
/// matching `resize_constraints` floor, so the OS refuses a smaller windowed
/// resize. `main` spreads this into its primary-window literal alongside the
/// mode from [`window_mode`], so the title/resolution/floor literals live in
/// one place.
pub fn window_config() -> Window {
    Window {
        title: "unmei".into(),
        resolution: WINDOW_SIZE.into(),
        resize_constraints: WindowResizeConstraints {
            min_width: WINDOW_SIZE.0 as f32,
            min_height: WINDOW_SIZE.1 as f32,
            ..default()
        },
        ..default()
    }
}

/// Maps `play.fullscreen` to the created window's mode: `Windowed`, or
/// borderless-primary fullscreen. `main` calls this **before** `App::new()` and
/// spreads the mode over the window [`window_config`] builds, so a `true`
/// setting is fullscreen from the first frame with no windowed frame in
/// between. Exclusive fullscreen
/// (`WindowMode::Fullscreen`) is deliberately not used: it changes video mode
/// and refresh rate for a comfort toggle's worth of benefit.
pub fn window_mode(fullscreen: bool) -> WindowMode {
    if fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
    } else {
        WindowMode::Windowed
    }
}
