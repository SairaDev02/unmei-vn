//! The one row metric derivation: display cells, wrapping and single-line units.

use crate::monitor::MonitorLine;

/// A within-row offset is clamped just short of the anchor row's end so the
/// anchor itself always stays partly visible.
pub(super) const OFFSET_EPSILON: f32 = 0.001;

/// The one row metric derivation: display-cell wrapping, per-column row extent
/// and conversion to the single-line row units the walks, clamps and scrollbar
/// use.
///
/// The drawer supplies the numbers — its current window/font geometry and the
/// requested viewport sizes — and this type owns the formula, so navigation,
/// rewrap clamping, imposed row height, shift/spacer and the scrollbar can
/// never drift onto separate reader and render formulas. The analytical basis
/// is frozen: monospace display cells (wide CJK/Hangul/fullwidth/emoji glyphs
/// count two) and the `ceil` line count, never laid-out glyph feedback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Metrics {
    /// The message/fields column's character capacity.
    columns: f32,
    /// The fixed target column's character capacity.
    target_columns: f32,
    /// One display line's pitch, in px.
    line_px: f32,
    /// The row's vertical padding on both edges, in px.
    row_pad_px: f32,
    /// One single-line row's pitch (`line_px + row_pad_px`), in px.
    row_line_px: f32,
    /// The viewport's capacity in single-line row units.
    visible_lines: f32,
    /// The materialized display window's cap in rows.
    window_rows: usize,
}

impl Metrics {
    /// Build the derivation from the drawer's current geometry inputs.
    pub(crate) fn new(
        columns: f32,
        target_columns: f32,
        line_px: f32,
        row_pad_px: f32,
        row_line_px: f32,
        visible_lines: f32,
        window_rows: usize,
    ) -> Self {
        Self {
            columns,
            target_columns,
            line_px,
            row_pad_px,
            row_line_px,
            visible_lines,
            window_rows,
        }
    }

    /// The row's height in single-line row units: the reader's navigation unit
    /// and the scrollbar's size unit. A one-line row is exactly `1.0`; a
    /// multi-line row is its laid-out text lines plus the row padding,
    /// converted by the single-line row pitch.
    pub(crate) fn height_units(&self, line: &MonitorLine) -> f32 {
        let lines = self.text_lines(line);
        (lines * self.line_px + self.row_pad_px) / self.row_line_px
    }

    /// The display lines a row's text occupies, from the real column
    /// capacities rather than a guessed line height. The target wraps inside
    /// its own fixed column and the message/fields column takes the rest; a
    /// byte-truncated event keeps its preserved single warm line.
    pub(super) fn text_lines(&self, line: &MonitorLine) -> f32 {
        if line.truncated {
            // A byte-truncated event keeps its preserved single warm line.
            return 1.0;
        }
        wrap_lines(display_cells(&line.target_display), self.target_columns)
            .max(wrap_lines(display_cells(line.body()), self.columns))
            .max(1.0)
    }

    /// Keep a within-row offset inside the row's current estimated wrapping.
    pub(super) fn clamp_offset(&self, line: &MonitorLine, offset: f32) -> f32 {
        offset.clamp(0.0, self.height_units(line) - OFFSET_EPSILON)
    }

    /// The viewport's capacity in single-line row units.
    pub(super) fn visible_lines(&self) -> f32 {
        self.visible_lines
    }

    /// The materialized display window's cap in rows.
    pub(super) fn window_rows(&self) -> usize {
        self.window_rows
    }
}

/// The display cells `text` occupies in the monospace row: one per ordinary
/// character, two for the wide CJK/Hangul/fullwidth/emoji ranges whose
/// fallback glyphs advance two cells.
fn display_cells(text: &str) -> usize {
    text.chars()
        .map(|character| if is_wide_character(character) { 2 } else { 1 })
        .sum()
}

/// Whether `character` is a wide (two-cell) glyph. The ranges cover the CJK,
/// Hangul, fullwidth and emoji blocks the log fixtures and ordinary content
/// use; an unrecognised wide glyph is at worst one cell under-counted, which
/// the single-line minimum still keeps from clipping.
fn is_wide_character(character: char) -> bool {
    matches!(character as u32,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1FAFF
        | 0x20000..=0x3FFFD)
}

/// The display lines `cells` monospace cells occupy in a `capacity`-cell
/// column, at least one.
fn wrap_lines(cells: usize, capacity: f32) -> f32 {
    (cells as f32 / capacity.max(1.0)).ceil().max(1.0)
}
