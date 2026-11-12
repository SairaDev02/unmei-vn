//! The byte-budgeted escaped line renderer and its field visitor.
//!
//! Every push is a whole character, whole escape sequence or fixed separator, so
//! a truncation always lands on a complete Unicode/escape boundary.

use std::fmt;

use bevy::log::tracing::field::{Field, Visit};
use bevy::log::tracing::Event;
use tracing_log::NormalizeEvent;

use super::line::{MonitorLine, CONTENT_BUDGET};

pub(super) fn render_line(
    level: log::Level,
    metadata_target: &str,
    event: &Event<'_>,
) -> Option<MonitorLine> {
    // A genuine `log`-bridge record is identified by its own bridge callsite —
    // never by a field that merely shares a bridge field's name. Only such a
    // record's original target is recovered from `log.target`; an ordinary
    // event keeps its metadata target even when it declares a `log.target`
    // field. The bridge's normalized metadata reads the target as a `&str`, so
    // recovery never runs an unbounded `Debug`/`format!` pass.
    let bridged = event.is_log();
    let target = if bridged {
        event
            .normalized_metadata()
            .map(|metadata| metadata.target().to_string())
            .unwrap_or_else(|| metadata_target.to_string())
    } else {
        metadata_target.to_string()
    };
    render_with(level, &target, bridged, |visitor| event.record(visitor))
}

/// The shared renderer, separable from a concrete [`Event`] so the byte-bound
/// and field-visit behavior can be exercised with an explicit visitor.
pub(super) fn render_with(
    level: log::Level,
    target: &str,
    bridged: bool,
    mut record: impl FnMut(&mut dyn Visit),
) -> Option<MonitorLine> {
    let mut line = LineBuilder::new();
    // A fixed-width placeholder prefix; [`MonitorShared::admit`] overwrites the
    // elapsed field in place when it stamps admission time, so the byte
    // accounting here already reserves the real prefix width.
    line.push_literal(&format!("{:010}ms {level:<5} ", 0u64));
    let target_start = line.text.len();
    line.push_escaped(target);
    let target_end = line.text.len();
    let body_start = line.text.len();
    // The message is rendered exactly once, immediately after the target, even
    // when an explicitly named `message` field is declared after another field.
    {
        let mut visitor = CaptureVisitor {
            line: &mut line,
            message_pass: true,
            bridged,
        };
        record(&mut visitor);
    }
    // Other named fields follow in recorded order. A message that already filled
    // the budget stops this pass before any further value is formatted.
    if !line.stopped() {
        let mut visitor = CaptureVisitor {
            line: &mut line,
            message_pass: false,
            bridged,
        };
        record(&mut visitor);
    }

    if line.failed {
        return None;
    }
    let truncated = line.truncated;
    // The whole escaped target is rendered; the fixed 224px target column wraps
    // a long target instead of shortening it under an ellipsis (finding B5).
    // Only a genuine byte-cap truncation collapses the row to the warm line.
    let target_display = line.text[target_start..target_end].to_string();
    Some(MonitorLine {
        sequence: 0,
        elapsed_ms: 0,
        level,
        target: target.to_string(),
        target_display,
        text: line.text,
        body_start,
        truncated,
    })
}

/// Accumulates the escaped line under [`CONTENT_BUDGET`]. Every push is a whole
/// character, whole escape sequence or fixed separator, so a truncation always
/// lands on a complete Unicode/escape boundary.
pub(super) struct LineBuilder {
    pub(super) text: String,
    pub(super) truncated: bool,
    failed: bool,
}

impl LineBuilder {
    pub(super) fn new() -> Self {
        Self {
            text: String::new(),
            truncated: false,
            failed: false,
        }
    }

    fn stopped(&self) -> bool {
        self.truncated
    }

    fn try_push(&mut self, piece: &str) -> bool {
        if self.truncated {
            return false;
        }
        if self.text.len() + piece.len() > CONTENT_BUDGET {
            self.truncated = true;
            return false;
        }
        self.text.push_str(piece);
        true
    }

    fn push_literal(&mut self, piece: &str) {
        let _ = self.try_push(piece);
    }

    /// Escape and push `raw`, stopping at the budget. Returns `false` once the
    /// budget is exhausted (which also sets [`Self::truncated`]).
    pub(super) fn push_escaped(&mut self, raw: &str) -> bool {
        for ch in raw.chars() {
            if self.truncated {
                return false;
            }
            if ch == '\\' || ch == '\n' || ch == '\r' || ch == '\t' || needs_escape(ch) {
                let mut buffer = [0u8; 12];
                let piece = escaped_piece(ch, &mut buffer);
                if !self.try_push(piece) {
                    return false;
                }
            } else {
                let mut buffer = [0u8; 4];
                let piece = ch.encode_utf8(&mut buffer);
                if !self.try_push(piece) {
                    return false;
                }
            }
        }
        true
    }

    /// Format a `Debug` value through the escaping writer. A formatter that
    /// fails for a reason other than our cap marks the line failed.
    pub(super) fn push_debug(&mut self, value: &dyn fmt::Debug) {
        if self.truncated {
            return;
        }
        let mut writer = EscapeWriter { line: self };
        if fmt::write(&mut writer, format_args!("{value:?}")).is_err() && !writer.line.truncated {
            writer.line.failed = true;
        }
    }

    /// Format a `Display` value through the escaping writer.
    fn push_display(&mut self, value: impl fmt::Display) {
        if self.truncated {
            return;
        }
        let mut writer = EscapeWriter { line: self };
        if fmt::write(&mut writer, format_args!("{value}")).is_err() && !writer.line.truncated {
            writer.line.failed = true;
        }
    }
}

/// A `fmt::Write` adapter that escapes every `write_str` chunk and returns an
/// error once the budget is exhausted, which stops an oversized `Debug` from
/// formatting further into us.
struct EscapeWriter<'a> {
    line: &'a mut LineBuilder,
}

impl fmt::Write for EscapeWriter<'_> {
    fn write_str(&mut self, piece: &str) -> fmt::Result {
        if self.line.push_escaped(piece) {
            Ok(())
        } else {
            Err(fmt::Error)
        }
    }
}

/// Whether `ch` must be escaped to keep one logical line from forging another
/// row or reversing prefix text: C0/C1 controls (ANSI's `ESC` included),
/// Unicode line/paragraph separators, bidi controls and the zero-width/format
/// characters that can hide or reorder text.
fn needs_escape(ch: char) -> bool {
    ch.is_control()
        || ch == '\u{2028}'
        || ch == '\u{2029}'
        || ('\u{202A}'..='\u{202E}').contains(&ch)
        || ('\u{2066}'..='\u{2069}').contains(&ch)
        || ch == '\u{200E}'
        || ch == '\u{200F}'
        || ch == '\u{061C}'
        || ('\u{200B}'..='\u{200D}').contains(&ch)
        || ch == '\u{2060}'
        || ch == '\u{FEFF}'
}

/// The escaped replacement for `ch`: `\n`, `\r`, `\t` and `\\` get their short
/// forms; every other escaped char gets `\u{HEX}` so a literal backslash cannot
/// be confused with an escape. Only called for a char [`needs_escape`] selected.
fn escaped_piece(ch: char, buffer: &mut [u8; 12]) -> &str {
    match ch {
        '\\' => "\\\\",
        '\n' => "\\n",
        '\r' => "\\r",
        '\t' => "\\t",
        _ => write_unicode_escape(ch, buffer),
    }
}

fn write_unicode_escape(ch: char, buffer: &mut [u8; 12]) -> &str {
    buffer[0] = b'\\';
    buffer[1] = b'u';
    buffer[2] = b'{';
    let mut index = 3;
    let digits = format!("{:x}", ch as u32);
    for byte in digits.bytes() {
        buffer[index] = byte;
        index += 1;
    }
    buffer[index] = b'}';
    index += 1;
    std::str::from_utf8(&buffer[..index]).expect("escape framing is ASCII")
}

struct CaptureVisitor<'a> {
    line: &'a mut LineBuilder,
    /// `true` in the message pass: only the `message` field is rendered.
    message_pass: bool,
    /// `true` for a genuine `log`-bridge record: only that record's own
    /// scaffolding fields are omitted. An ordinary event renders every declared
    /// field, including one named like bridge metadata.
    bridged: bool,
}

/// The event's message field name, in `tracing` and in the `log` bridge alike.
const MESSAGE_FIELD: &str = "message";

/// The original-target field `tracing-log` adds to bridged `log!` events.
const LOG_TARGET_FIELD: &str = "log.target";

/// `tracing-log`'s bridge plumbing fields: the recovered target plus the
/// source-location fields the contract excludes (no location rows). Only a
/// genuine bridge record's fields are matched here.
fn is_bridge_field(name: &str) -> bool {
    matches!(
        name,
        LOG_TARGET_FIELD | "log.module_path" | "log.file" | "log.line"
    )
}

impl CaptureVisitor<'_> {
    fn skip(&self, field: &Field) -> bool {
        if self.line.stopped() {
            return true;
        }
        if field.name() == MESSAGE_FIELD {
            // Only the message pass renders the message, and it renders it once.
            return !self.message_pass;
        }
        // A non-message field is only rendered by the field pass, so the message
        // pass never formats one and the field pass never re-renders the message.
        self.message_pass || (self.bridged && is_bridge_field(field.name()))
    }

    fn begin_field(&mut self, name: &str) {
        self.line.push_literal(" ");
        if name != MESSAGE_FIELD {
            self.line.push_escaped(name);
            self.line.push_literal("=");
        }
    }
}

impl Visit for CaptureVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if self.skip(field) {
            return;
        }
        self.begin_field(field.name());
        self.line.push_debug(value);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if self.skip(field) {
            return;
        }
        self.begin_field(field.name());
        self.line.push_escaped(value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_number(field, value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_number(field, value);
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.record_number(field, value);
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.record_number(field, value);
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record_number(field, value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_number(field, value);
    }
}

impl CaptureVisitor<'_> {
    fn record_number(&mut self, field: &Field, value: impl fmt::Display) {
        if self.skip(field) {
            return;
        }
        self.begin_field(field.name());
        self.line.push_display(value);
    }
}
