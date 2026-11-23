//! The one-question splice: where the RON structure is, and the rewrite that
//! changes only the named question's block (author-cli contract §2).

use unmei_engine::content::{DecisionConfig, Npc};

/// Rewrite only the named question's decision block inside the original RON
/// text. Everything before and after the block — comments included — passes
/// through byte-for-byte.
pub(super) fn splice_question(
    src: &str,
    existing: &Npc,
    question: &str,
    config: &DecisionConfig,
) -> String {
    let layout = Layout::of(src);
    let Some((map_open, map_close)) = layout.decisions_map(src) else {
        // No decisions map at all — unreachable for a parsed NPC, whose
        // `decisions` field is required; fall back to re-serializing (loses
        // comments).
        let mut npc = existing.clone();
        npc.decisions.insert(question.to_string(), config.clone());
        return ron::ser::to_string_pretty(&npc, pretty())
            .unwrap_or_else(|_| ron::ser::to_string(&npc).unwrap());
    };
    let rendered = ron::ser::to_string_pretty(config, pretty())
        .unwrap_or_else(|_| ron::ser::to_string(config).unwrap());
    match layout.question_block(src, map_open, map_close, question) {
        Some((start, end)) => {
            let indented = indent(&rendered, &indent_of(&src[..start]));
            let mut out = String::with_capacity(src.len() + 128);
            out.push_str(&src[..start]);
            out.push_str(&indented);
            out.push_str(&src[end..]);
            out
        }
        // New question: insert a block before the decisions map's closing
        // brace, reusing whatever trailing structure the file has.
        None => {
            let line_indent = indent_of(&src[..map_open]) + "    ";
            let indented = indent(&rendered, &line_indent);
            let mut out = String::with_capacity(src.len() + rendered.len() + 32);
            out.push_str(&src[..map_close]);
            out.push('\n');
            out.push_str(&line_indent);
            out.push_str(&format!("\"{question}\": "));
            out.push_str(&indented);
            out.push(',');
            out.push_str(&src[map_close..]);
            out
        }
    }
}

/// Pretty config with RON-style spacing the content files use.
fn pretty() -> ron::ser::PrettyConfig {
    ron::ser::PrettyConfig::default()
        .indentor("    ".to_string())
        .separate_tuple_members(true)
}

/// Where the RON code is in an NPC file, so the splice finds structure only
/// where the RON parser would: never in a comment or a string body.
///
/// `code[i]` is true when byte `i` is outside every comment and string body; a
/// string's opening quote counts as code, so a key is found where its quote
/// starts. `depth[i]` is the bracket depth before byte `i`, counting `(`, `[`
/// and `{` in code only. Comments are read as RON reads them: `//` to the end
/// of the line and nested `/* … */`. Strings are quoted with `\` escapes, or
/// raw (`r"…"`, `r#"…"#`).
struct Layout {
    code: Vec<bool>,
    depth: Vec<usize>,
}

impl Layout {
    fn of(src: &str) -> Self {
        let bytes = src.as_bytes();
        let mut code = vec![false; bytes.len()];
        let mut depth = vec![0; bytes.len()];
        let mut level = 0usize;
        let mut i = 0;
        while i < bytes.len() {
            let rest = &bytes[i..];
            let span = if rest.starts_with(b"//") {
                rest.iter().position(|&c| c == b'\n').unwrap_or(rest.len())
            } else if rest.starts_with(b"/*") {
                block_comment_len(rest)
            } else if let Some(len) = raw_string_len(bytes, i) {
                code[i] = true;
                len
            } else if bytes[i] == b'"' {
                code[i] = true;
                quoted_string_len(rest)
            } else {
                code[i] = true;
                1
            };
            let span = span.clamp(1, rest.len());
            depth[i..i + span].fill(level);
            if code[i] && span == 1 {
                match bytes[i] {
                    b'(' | b'[' | b'{' => level += 1,
                    b')' | b']' | b'}' => level = level.saturating_sub(1),
                    _ => {}
                }
            }
            i += span;
        }
        Self { code, depth }
    }

    /// The first code byte at or after `from` that is not whitespace.
    fn next_code(&self, src: &str, from: usize) -> Option<usize> {
        (from..src.len()).find(|&i| self.code[i] && !src.as_bytes()[i].is_ascii_whitespace())
    }

    /// The first `needle` in `src[from..to]` that starts in code at `depth`.
    fn find(&self, src: &str, needle: &str, from: usize, to: usize, depth: usize) -> Option<usize> {
        src[from..to]
            .match_indices(needle)
            .map(|(at, _)| from + at)
            .find(|&at| self.code[at] && self.depth[at] == depth)
    }

    /// The closing bracket of the bracket that opens at `open`.
    fn close_of(&self, src: &str, open: usize) -> usize {
        let inside = self.depth[open] + 1;
        (open + 1..src.len())
            .find(|&i| {
                self.code[i]
                    && self.depth[i] == inside
                    && matches!(src.as_bytes()[i], b')' | b']' | b'}')
            })
            .unwrap_or(src.len())
    }

    /// The `{` and `}` of the NPC's top-level `decisions` map.
    fn decisions_map(&self, src: &str) -> Option<(usize, usize)> {
        let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
        let bytes = src.as_bytes();
        let mut from = 0;
        while let Some(at) = self.find(src, "decisions", from, src.len(), 1) {
            from = at + 1;
            let end = at + "decisions".len();
            if (at > 0 && ident(bytes[at - 1])) || bytes.get(end).copied().is_some_and(ident) {
                continue;
            }
            let colon = self.next_code(src, end)?;
            if bytes[colon] != b':' {
                continue;
            }
            let open = self.next_code(src, colon + 1)?;
            if bytes[open] != b'{' {
                continue;
            }
            return Some((open, self.close_of(src, open)));
        }
        None
    }

    /// The span of `question`'s config — from its `(` through the matching `)`
    /// — among the decisions map's own entries.
    fn question_block(
        &self,
        src: &str,
        map_open: usize,
        map_close: usize,
        question: &str,
    ) -> Option<(usize, usize)> {
        let key = format!("\"{question}\"");
        let entry_depth = self.depth[map_open] + 1;
        let at = self.find(src, &key, map_open, map_close, entry_depth)?;
        let colon = self.next_code(src, at + key.len())?;
        if src.as_bytes()[colon] != b':' {
            return None;
        }
        let start = (colon + 1..map_close)
            .find(|&i| self.code[i] && self.depth[i] == entry_depth && src.as_bytes()[i] == b'(')?;
        Some((start, self.close_of(src, start) + 1))
    }
}

/// The length of the nested `/* … */` comment at the start of `rest`.
fn block_comment_len(rest: &[u8]) -> usize {
    let mut level = 0usize;
    let mut i = 0;
    while i < rest.len() {
        if rest[i..].starts_with(b"/*") {
            level += 1;
            i += 2;
        } else if rest[i..].starts_with(b"*/") {
            level -= 1;
            i += 2;
            if level == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    rest.len()
}

/// The length of the `"…"` string at the start of `rest`, escapes included.
fn quoted_string_len(rest: &[u8]) -> usize {
    let mut i = 1;
    while i < rest.len() {
        match rest[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    rest.len()
}

/// The length of the raw string (`r"…"`, `r#"…"#`, …) starting at `at`, if one
/// does: an `r` that does not continue an identifier, then hashes and a quote.
fn raw_string_len(bytes: &[u8], at: usize) -> Option<usize> {
    let continues_ident =
        at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
    if bytes[at] != b'r' || continues_ident {
        return None;
    }
    let hashes = bytes[at + 1..].iter().take_while(|&&c| c == b'#').count();
    let quote = at + 1 + hashes;
    if bytes.get(quote) != Some(&b'"') {
        return None;
    }
    let mut closing = vec![b'"'];
    closing.extend(std::iter::repeat_n(b'#', hashes));
    let body = &bytes[quote + 1..];
    let end = body
        .windows(closing.len())
        .position(|window| window == closing.as_slice())
        .map_or(body.len(), |pos| pos + closing.len());
    Some(quote + 1 - at + end)
}

/// The leading whitespace of the line containing `pos`.
fn indent_of(src: &str) -> String {
    let line_end = src.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_start = &src[line_end..];
    let ws: String = line_start
        .chars()
        .take_while(|c| c.is_whitespace() && *c != '\n')
        .collect();
    ws
}

/// Indent a rendered RON block to sit under `indent`.
fn indent(rendered: &str, indent: &str) -> String {
    let lines: Vec<String> = rendered
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 || line.is_empty() {
                line.to_string()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect();
    lines.join("\n")
}
