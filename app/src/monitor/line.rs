//! One admitted event as a single escaped, size-bounded logical line.
//!
//! The line's whole byte contract lives here: the truncation marker, the content
//! budget reserved from [`super::MAX_LINE_BYTES`], the fixed-width elapsed field
//! and its saturation bound.

use super::MAX_LINE_BYTES;

/// The visible tail marker appended to a truncated line. Its length is reserved
/// out of [`MAX_LINE_BYTES`] before any content is written, so appending it can
/// never push a line over the cap.
pub(super) const TRUNCATION_SUFFIX: &str = " [truncated]";

/// Bytes available to a line's content after reserving the truncation suffix.
pub(super) const CONTENT_BUDGET: usize = MAX_LINE_BYTES - TRUNCATION_SUFFIX.len();

/// The fixed rendered width of the elapsed field: ten zero-padded digits and
/// `ms`. [`MonitorLine::finalize`] overwrites exactly these bytes in place, so
/// admission-time stamping never shifts the rest of the line.
pub(super) const ELAPSED_FIELD_BYTES: usize = 12;

/// The largest elapsed value the ten-digit rendered field can show. A session
/// older than this keeps its true [`MonitorLine::elapsed_ms`] but saturates the
/// displayed prefix instead of widening it and misaligning the line.
pub(super) const MAX_RENDERED_ELAPSED_MS: u64 = 9_999_999_999;

/// One admitted source event as a single escaped logical Monitor line.
///
/// `text` is the complete rendered line — elapsed prefix, level, escaped target,
/// message and named fields, plus `[truncated]` when needed — always at or below
/// [`MAX_LINE_BYTES`]. `target` keeps the *original*, un-escaped target so the
/// display filter can project the exact case-sensitive `unmei::` prefix without
/// depending on a possibly shortened rendered prefix. The grid's last column is
/// the suffix of `text` from [`Self::body_start`] (see [`Self::body`]), so the
/// raw escaped body is stored once rather than copied beside `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorLine {
    /// Monotonic admission identity: assigned when the line is inserted into
    /// the completed queue, so retained order and row identity never depend on
    /// timestamps or on when a formatter reserved its slot.
    pub sequence: u64,
    /// Monotonic elapsed milliseconds since capture installation, stamped at
    /// successful completed-line insertion.
    pub elapsed_ms: u64,
    /// The event's original level.
    pub level: log::Level,
    /// The event's original target, for later exact projection.
    pub target: String,
    /// The target as escaped for display — the grid's target column. The
    /// original `target` drives filtering; this is what a control-bearing or
    /// bidi target must render as.
    pub target_display: String,
    /// The complete escaped logical line, at most [`MAX_LINE_BYTES`] bytes.
    pub text: String,
    /// Byte offset in `text` where the escaped message and named fields begin —
    /// the full grid's last column.
    pub body_start: usize,
    /// Whether `text` ends in the visible `[truncated]` marker.
    pub truncated: bool,
}

impl MonitorLine {
    /// The escaped message and named fields — the full grid's last column. It
    /// borrows the one stored copy of the raw escaped text instead of holding a
    /// duplicate, so the pending/retained byte ceilings are not silently doubled.
    pub fn body(&self) -> &str {
        &self.text[self.body_start..]
    }

    /// Stamp admission metadata onto a formatted line: overwrite the reserved
    /// elapsed field with the admission time and append the truncation suffix.
    /// The elapsed field has a fixed width, so the rest of `text` never shifts
    /// and `body_start` stays valid.
    pub(super) fn stamp_admission(&mut self, elapsed_ms: u64) {
        let shown = elapsed_ms.min(MAX_RENDERED_ELAPSED_MS);
        let prefix = format!("{shown:010}ms");
        debug_assert_eq!(prefix.len(), ELAPSED_FIELD_BYTES);
        self.text.replace_range(0..ELAPSED_FIELD_BYTES, &prefix);
        self.elapsed_ms = elapsed_ms;
        if self.truncated {
            self.text.push_str(TRUNCATION_SUFFIX);
        }
    }
}
