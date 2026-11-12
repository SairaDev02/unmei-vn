//! `unmei author draft`'s drafting primitives (drafting contract §1/§2/§4/§6):
//! run addressing, the layer-1 say-line gate, and the run-region splice.
//!
//! A **run** is a maximal contiguous block of say-lines. Blank and comment
//! lines *between* say-lines stay inside it; any command or flow line (or the
//! scene start/end) ends it. A run's **handle** is its innermost enclosing named
//! anchor — `start` at the scene top level, or a `label` — plus a `#2`…
//! ordinal when several runs fall under it. A run inside an `if`/`else` arm
//! folds to the nearest enclosing `label` (or `start`); anonymous containers add
//! no handle.
//!
//! The engine parser already drops blank and comment lines, so a run is a
//! maximal consecutive block of [`Line::Say`] entries in [`Scene::lines`] —
//! except that `endif` is consumed rather than stored as a line, so a run must
//! also break where an `if`/`else` block ends ([`Line::If::end_index`]).
//! Labels are top-level only (the parser rejects one inside an `if` arm), so the
//! enclosing anchor is simply the most recent [`Line::Label`]. `choice` bodies
//! hold only options and `decide` is one line, so neither encloses a run.

use std::collections::{HashMap, HashSet};
use std::fmt;

use unmei_engine::content::{Line, Pack, Scene};
use unmei_engine::{is_keyword, split_say_line};

/// The scene top level's anchor name; its first run takes the bare handle.
pub const START: &str = "start";

/// One addressable run in a scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// The handle `--from` accepts: `start`, `start#2`, `reply.warm`, …
    pub handle: String,
    /// 1-based source line of the run's first say-line.
    pub start_line: usize,
    /// 1-based source line of the run's last say-line. Blank and comment lines
    /// between say-lines sit inside the span.
    pub end_line: usize,
    /// The run's first say-line rendered as `Speaker: text`.
    pub first_say: String,
}

impl Run {
    /// The `--list` line-span field: `L7` for a single-line run, `L7-L10` when
    /// the run's say-lines are spread across a range.
    pub fn span(&self) -> String {
        if self.start_line == self.end_line {
            format!("L{}", self.start_line)
        } else {
            format!("L{}-L{}", self.start_line, self.end_line)
        }
    }
}

/// The [`Scene::lines`] indices after which an `if`/`else` block ends. The
/// `endif` itself is parser-controlled and carries no [`Line`], so a say-line at
/// one of these indices must not merge with a say-line that follows the block.
fn block_ends(scene: &Scene) -> HashSet<usize> {
    scene
        .lines
        .iter()
        .filter_map(|line| match line {
            Line::If { end_index, .. } => Some(*end_index),
            Line::Else { end_index } => Some(*end_index),
            _ => None,
        })
        .collect()
}

/// Every run in `scene`, in scene order.
pub fn runs(scene: &Scene) -> Vec<Run> {
    let ends = block_ends(scene);
    let mut runs = Vec::new();
    // Ordinals are keyed by handle name, so a label that reuses `start` cannot
    // shadow the top-level anchor: handles stay unique and every run is
    // addressable. (The contract leaves a same-named label undefined; uniqueness
    // is the property addressing needs.)
    let mut anchor = START.to_string();
    let mut ordinals: HashMap<String, usize> = HashMap::new();

    let mut i = 0;
    while i < scene.lines.len() {
        match &scene.lines[i] {
            Line::Label(name) => {
                anchor = name.clone();
                i += 1;
            }
            Line::Say { .. } => {
                let first = i;
                let mut last = i;
                while last + 1 < scene.lines.len()
                    && matches!(scene.lines[last + 1], Line::Say { .. })
                    && !ends.contains(&last)
                {
                    last += 1;
                }

                let ordinal = ordinals.entry(anchor.clone()).or_insert(0);
                *ordinal += 1;
                let handle = if *ordinal == 1 {
                    anchor.clone()
                } else {
                    format!("{anchor}#{ordinal}")
                };
                let first_say = match &scene.lines[first] {
                    Line::Say { speaker, text } => format!("{speaker}: {text}"),
                    _ => unreachable!("a run starts on a say-line"),
                };

                runs.push(Run {
                    handle,
                    start_line: scene.source_line(first),
                    end_line: scene.source_line(last),
                    first_say,
                });
                i = last + 1;
            }
            _ => i += 1,
        }
    }

    runs
}

/// A `--from` address that names no run in the scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownRun {
    /// The scene the address was looked up in.
    pub scene_id: String,
    /// The address exactly as the author wrote it.
    pub address: String,
}

impl fmt::Display for UnknownRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "no run `{}` in scene `{}` — run with `--list` to see the addresses",
            self.address, self.scene_id
        )
    }
}

impl std::error::Error for UnknownRun {}

/// Resolve a `--from` address (`start`, a label name, or either with a `#n`
/// ordinal) to exactly one run, or [`UnknownRun`] when it names nothing.
pub fn resolve<'a>(runs: &'a [Run], scene_id: &str, address: &str) -> Result<&'a Run, UnknownRun> {
    runs.iter()
        .find(|run| run.handle == address)
        .ok_or_else(|| UnknownRun {
            scene_id: scene_id.to_string(),
            address: address.to_string(),
        })
}

/// The `--list` table: handle, line span and first say-line per run, in scene
/// order (drafting contract §1).
pub fn render_list(runs: &[Run]) -> String {
    runs.iter()
        .map(|run| format!("  {:<10}  {:<7}{}\n", run.handle, run.span(), run.first_say))
        .collect()
}

// ---------------------------------------------------------------------------
// The layer-1 drafting gate (contract §4)
// ---------------------------------------------------------------------------

/// The canonical narration speaker (contract §7): always an allowed speaker.
pub const NARRATOR: &str = "Narrator";

/// One drafted say-line, validated by the layer-1 gate and ready to render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SayLine {
    pub speaker: String,
    pub text: String,
}

impl SayLine {
    /// `Speaker: text` — the exact `.unmei` line the splice writes back.
    pub fn render(&self) -> String {
        format!("{}: {}", self.speaker, self.text)
    }
}

/// The speakers a drafted line may use (contract §7): `Narrator`, every speaker
/// already used in the target scene, and every declared NPC id.
pub fn allowed_speakers(scene: &Scene, pack: &Pack) -> HashSet<String> {
    let mut allowed: HashSet<String> = pack.npcs.keys().cloned().collect();
    allowed.insert(NARRATOR.to_string());
    for line in &scene.lines {
        if let Line::Say { speaker, .. } = line {
            allowed.insert(speaker.clone());
        }
    }
    allowed
}

/// Split a raw model reply into candidate lines: trimmed, non-empty, in order.
/// Blank lines are separators, not content.
pub fn candidate_lines(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Layer-1 gate (contract §4): every candidate must be a bare say-line whose
/// leading token is not a reserved keyword and whose speaker is allowed; a
/// draft with no say-lines is a rejection too. Every failure is a correction
/// line, so the caller can feed the errors back to the model.
pub fn gate(candidates: &[String], allowed: &HashSet<String>) -> Result<Vec<SayLine>, Vec<String>> {
    if candidates.is_empty() {
        return Err(vec![no_say_lines_error(false)]);
    }

    let mut accepted = Vec::new();
    let mut errors = Vec::new();
    for (i, raw) in candidates.iter().enumerate() {
        let n = i + 1;
        let line = raw.trim();

        // A keyword in the leading token is control flow or a command smuggled
        // into prose (the loader would accept `bg: school_gate` as a say-line,
        // so this check is what keeps a draft prose-only). The leading token is
        // the speaker part before `: ` when the line has one, else the first
        // whitespace token.
        let head = match line.find(": ") {
            Some(idx) => &line[..idx],
            None => line,
        };
        if let Some(first) = head.split_whitespace().next() {
            if is_keyword(first) {
                errors.push(keyword_error(n, line, first));
                continue;
            }
        }

        match split_say_line(line) {
            None => errors.push(not_a_say_line_error(n, line)),
            Some((speaker, text)) => match check_item(n, &speaker, &text, allowed) {
                Ok(say) => accepted.push(say),
                Err(err) => errors.push(err),
            },
        }
    }

    finalize(accepted, errors)
}

/// Layer-1 gate for **structured** items (contract §4/§7): every pair must name
/// an allowed, colon-free speaker and carry non-empty single-line text. The
/// item is rendered back to `Speaker: text` by [`SayLine::render`], so a colon
/// inside the text and escaped quotes/backslashes survive deserialisation
/// untouched; a newline would break the one-line DSL rule, so it is rejected.
/// The speaker/text rules are shared with the free-text gate ([`check_item`]).
pub fn gate_items(
    items: &[(String, String)],
    allowed: &HashSet<String>,
) -> Result<Vec<SayLine>, Vec<String>> {
    if items.is_empty() {
        return Err(vec![no_say_lines_error(true)]);
    }

    let mut accepted = Vec::new();
    let mut errors = Vec::new();
    for (i, (speaker, text)) in items.iter().enumerate() {
        match check_item(i + 1, speaker, text, allowed) {
            Ok(say) => accepted.push(say),
            Err(err) => errors.push(err),
        }
    }

    finalize(accepted, errors)
}

/// The speaker/text rules shared by both layer-1 paths (free text and the
/// structured `LINES` items) — one implementation, so the two cannot drift.
fn check_item(
    n: usize,
    speaker: &str,
    text: &str,
    allowed: &HashSet<String>,
) -> Result<SayLine, String> {
    if let Some(first) = speaker.split_whitespace().next() {
        if is_keyword(first) {
            return Err(keyword_error(n, speaker, first));
        }
    }
    if speaker.contains(':') || speaker.trim() != speaker || speaker.is_empty() {
        return Err(format!(
            "line {n}: `{speaker}` is not a valid speaker (`{NARRATOR}`, a speaker in the scene, or a declared NPC id)"
        ));
    }
    if !allowed.contains(speaker) {
        return Err(format!(
            "line {n}: speaker `{speaker}` is not allowed — use `{NARRATOR}`, a speaker already in the scene, or a declared NPC id (allowed: {})",
            speaker_list(allowed)
        ));
    }
    if text.contains('\n') || text.contains('\r') {
        return Err(format!(
            "line {n}: the text for `{speaker}` contains a newline — split multi-line narration into separate items, one `Speaker: text` line each"
        ));
    }
    let text = text.trim_end();
    if text.trim().is_empty() {
        return Err(format!(
            "line {n}: `{speaker}` has empty text — every item needs non-empty text"
        ));
    }
    Ok(SayLine {
        speaker: speaker.to_string(),
        text: text.to_string(),
    })
}

fn keyword_error(n: usize, shown: &str, first: &str) -> String {
    format!(
        "line {n}: `{shown}` begins with the keyword `{first}` — drafts are say-lines only, never control flow or commands"
    )
}

fn not_a_say_line_error(n: usize, line: &str) -> String {
    format!("line {n}: `{line}` is not a say-line (`Speaker: text` with one `: ` separator)")
}

fn no_say_lines_error(structured: bool) -> String {
    if structured {
        "the draft contains no say-lines — reply with at least one `speaker`/`text` item"
            .to_string()
    } else {
        "the draft contains no say-lines — reply with one `Speaker: text` line per line".to_string()
    }
}

fn finalize(accepted: Vec<SayLine>, errors: Vec<String>) -> Result<Vec<SayLine>, Vec<String>> {
    if errors.is_empty() {
        Ok(accepted)
    } else {
        Err(errors)
    }
}

/// The sorted, comma-separated speaker list shared by the gate's correction
/// line and the drafting prompt.
pub fn speaker_list(allowed: &HashSet<String>) -> String {
    let mut names: Vec<&str> = allowed.iter().map(String::as_str).collect();
    names.sort_unstable();
    names.join(", ")
}

// ---------------------------------------------------------------------------
// The run-region splice (contract §6)
// ---------------------------------------------------------------------------

/// Replace the run's source lines with `replacement`, returning the new scene
/// text. Every line outside the run — its first say-line through its last,
/// including any interstitial blank and comment lines — survives byte-for-byte,
/// and the replacement reuses the text's own line-ending style (contract §6).
pub fn splice_run(src: &str, run: &Run, replacement: &[String]) -> String {
    let newline = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let start = line_start_offset(src, run.start_line);
    let end = line_end_offset(src, run.end_line);
    let mut out = String::with_capacity(src.len() + replacement.len() * 24);
    out.push_str(&src[..start]);
    out.push_str(&replacement.join(newline));
    out.push_str(&src[end..]);
    out
}

/// Byte offset of the first character of 1-based line `n`; past the end of the
/// text when `n` is past the last line.
fn line_start_offset(src: &str, n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    let mut line = 1;
    for (i, byte) in src.bytes().enumerate() {
        if byte == b'\n' {
            line += 1;
            if line == n {
                return i + 1;
            }
        }
    }
    src.len()
}

/// Byte offset of the line terminator that ends 1-based line `n` — just past
/// the line's last content character, so a `\r\n` terminator stays with the
/// line; past the end of the text when `n` is past the last line.
fn line_end_offset(src: &str, n: usize) -> usize {
    let bytes = src.as_bytes();
    let mut line = 1;
    for (i, byte) in src.bytes().enumerate() {
        if byte == b'\n' {
            if line == n {
                return if i > 0 && bytes[i - 1] == b'\r' {
                    i - 1
                } else {
                    i
                };
            }
            line += 1;
        }
    }
    src.len()
}
