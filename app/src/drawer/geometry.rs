//! The drawer's frozen geometry, tones and window-derived metric inputs.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::monitor_reader::Metrics;
use crate::presentation::style::TextFaces;

/// Candidate retained entries the projection may examine in one app update.
/// Independently of ingestion's own 128-event budget; a full 1,000-line pass
/// therefore completes in at most eight updates.
pub(crate) const PROJECTION_SCAN_BUDGET: usize = 128;

/// Line-sized text materializations/replacements the drawer may perform in one
/// app update — spawning one row or replacing its text. Separately bounded from
/// both ingestion and the projection scan.
pub(crate) const TEXT_WORK_BUDGET: usize = 128;

/// The newest logical rows kept materialized while following. The body shows
/// roughly fifteen single-line rows; the extra headroom keeps wrapped lines
/// filling it. It is a display window over [`MonitorProjection`]'s rows, not a
/// second history: the projection itself is bounded by the shared 1,000-line
/// retained cap.
pub(crate) const DISPLAY_ROWS: usize = 32;

/// Estimated display lines one full viewport of the fixed-height log body
/// holds. A single-line row is 12 × 1.45 + 2 × 1 = 19.4px, and the body is
/// 360 − 52 − 13 − 2 = 293px, so about fifteen single-line rows fit. The
/// estimate is only a navigation/scrollbar unit; the anchor identity is the
/// retained admission sequence.
pub(super) const VISIBLE_LINES: f32 = 15.0;

/// Display lines one wheel notch moves, and one page key moves by one viewport.
pub(super) const WHEEL_LINES: f32 = 3.0;

/// A held PgUp/PgDn repeats at this cadence; a fresh press acts immediately.
pub(super) const PAGE_REPEAT_SECONDS: f32 = 0.12;

/// The paged pitch of one display line: the 12px Fira Mono line at 1.45 plus
/// the row's 1px vertical padding on each side.
pub(super) const ROW_LINE_PX: f32 = LOG_FONT_SIZE * LOG_LINE_HEIGHT + 2.0 * ROW_VERTICAL_PADDING;

/// The scrollbar track's fixed width.
pub(crate) const SCROLLBAR_WIDTH: f32 = 10.0;

/// Fira Mono 12px advance per character — the same metric the target column
/// uses for its 224px/30-cell cell.
const CHAR_ADVANCE_PX: f32 = 7.2;

/// Row chrome that is not the flexible message column at the reference size:
/// the 2px left border, 24px horizontal padding, three 10px gaps, the
/// 72 + 46 + 224px fixed columns and the 10px scrollbar plus its 8px gap.
const ROW_CHROME_PX: f32 = 2.0 + 24.0 + 30.0 + 342.0 + SCROLLBAR_WIDTH + 8.0;

/// The message column's character capacity in a world without a window (the
/// headless harness): the 1280px reference size.
const FALLBACK_COLUMNS: f32 = 113.0;

/// The drawer's fixed reference geometry (`docs/runtime-hardening.md` § Monitor
/// → Layout). Every value is px at every resolution: the drawer widens with the
/// window and keeps this height and the dialogue reservation.
pub(crate) const PANEL_SIDE_INSET: f32 = 24.0;
pub(crate) const PANEL_HEIGHT: f32 = 360.0;
pub(crate) const PANEL_BOTTOM_RESERVATION: f32 = 148.0;
pub(crate) const HEADER_HEIGHT: f32 = 52.0;
pub(crate) const FOOTER_HEIGHT: f32 = 13.0;
pub(crate) const ROW_ELAPSED_WIDTH: f32 = 72.0;
pub(crate) const ROW_LEVEL_WIDTH: f32 = 46.0;
pub(crate) const ROW_TARGET_WIDTH: f32 = 224.0;
pub(crate) const ROW_COLUMN_GAP: f32 = 10.0;
pub(crate) const ROW_VERTICAL_PADDING: f32 = 1.0;
pub(crate) const LOG_FONT_SIZE: f32 = 12.0;
pub(crate) const LOG_LINE_HEIGHT: f32 = 1.45;

/// Fira Mono Medium 12px for the whole logical event.
pub(super) fn log_font(faces: &TextFaces) -> TextFont {
    faces.mono(LOG_FONT_SIZE)
}

/// The three level tones of the settled palette.
pub(crate) const TONE_INFO: Color = Color::srgba(204.0 / 255.0, 204.0 / 255.0, 217.0 / 255.0, 0.62);
pub(crate) const TONE_WARN: Color = Color::srgb(
    0xdb as f32 / 255.0,
    0xa1 as f32 / 255.0,
    0x66 as f32 / 255.0,
);
pub(crate) const TONE_ERROR: Color = Color::srgb(
    0xe0 as f32 / 255.0,
    0x7a as f32 / 255.0,
    0x6a as f32 / 255.0,
);

/// The whole-line text tone, and the faint/dim tones of the grid's fixed columns.
pub(crate) const TONE_TEXT: Color = Color::srgb(0.90, 0.90, 0.92);
pub(crate) const TONE_FAINT: Color =
    Color::srgba(204.0 / 255.0, 204.0 / 255.0, 217.0 / 255.0, 0.35);

/// The newest **retained** line marker: a 2px warm inset and a 4.5% wash. It
/// follows the newest retained event, never the newest *matching* one.
pub(crate) const NEWEST_INSET: Color = Color::srgb(
    0xe6 as f32 / 255.0,
    0x9e as f32 / 255.0,
    0x5c as f32 / 255.0,
);
pub(crate) const NEWEST_WASH: Color =
    Color::srgba(230.0 / 255.0, 158.0 / 255.0, 92.0 / 255.0, 0.045);

pub(super) const PANEL_BG: Color = Color::srgb(0.10, 0.11, 0.15);
pub(super) const BUTTON_BG: Color = Color::srgb(0.16, 0.17, 0.22);
pub(super) const FOCUS: Color = Color::srgb(0.90, 0.62, 0.36);
pub(super) const TEXT: Color = Color::srgb(0.90, 0.90, 0.92);
pub(super) const DIM: Color = Color::srgba(0.80, 0.80, 0.85, 0.62);
/// The scrollbar track and thumb are part of the dev drawer, not the player
/// palette: a barely visible track and a warm-neutral thumb in the drawer's own
/// vocabulary.
pub(super) const SCROLLBAR_TRACK: Color = Color::srgba(0.80, 0.80, 0.85, 0.06);
pub(super) const SCROLLBAR_THUMB: Color = Color::srgba(0.80, 0.80, 0.85, 0.30);

// ---------------------------------------------------------------------------
// Reader geometry: the window-derived column capacity and metric inputs
// ---------------------------------------------------------------------------

/// The message column's character capacity at the current window width, or the
/// 1280px reference when a headless world has no window.
pub(super) fn reader_columns(windows: &Query<&Window, With<PrimaryWindow>>) -> f32 {
    let Some(window) = windows.iter().next() else {
        return FALLBACK_COLUMNS;
    };
    let width = window.width();
    if width <= 0.0 {
        return FALLBACK_COLUMNS;
    }
    let panel_inner = (width - 2.0 * PANEL_SIDE_INSET - 2.0).max(1.0);
    ((panel_inner - ROW_CHROME_PX) / CHAR_ADVANCE_PX).max(1.0)
}

/// The reader's metric inputs at the current column capacity. The derivation
/// itself lives in [`crate::monitor_reader`]; the drawer supplies its frozen
/// geometry here.
pub(super) fn reader_metrics(columns: f32) -> Metrics {
    Metrics::new(
        columns,
        ROW_TARGET_WIDTH / CHAR_ADVANCE_PX,
        LOG_FONT_SIZE * LOG_LINE_HEIGHT,
        2.0 * ROW_VERTICAL_PADDING,
        ROW_LINE_PX,
        VISIBLE_LINES,
        DISPLAY_ROWS,
    )
}

pub(super) fn level_tone(level: log::Level) -> Color {
    match level {
        log::Level::Error => TONE_ERROR,
        log::Level::Warn => TONE_WARN,
        _ => TONE_INFO,
    }
}
