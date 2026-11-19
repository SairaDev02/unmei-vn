//! RON splice: add/replace one decision config in `npcs/<id>.ron` while
//! preserving every comment (author-cli contract §2 — RON was chosen partly
//! for comments; regeneration must never erase them).

use std::path::{Path, PathBuf};

use unmei_engine::content::{DecisionConfig, Npc};
use unmei_engine::read_pack_text;

use crate::target::{NpcTarget, TargetError};

/// What the write should do about the NPC file itself.
#[derive(Debug, Clone, PartialEq)]
pub enum CreateMode {
    /// The NPC file exists: surgically replace one question's config.
    Replace,
    /// `--create`: write a NEW npc file (identity head + the one config).
    Create { name: String },
}

#[derive(Debug, thiserror::Error)]
pub enum SpliceError {
    #[error("npc file already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("npc file does not exist (use --create): {0}")]
    NoSuchNpc(PathBuf),
    #[error("cannot parse {0}: {1}")]
    Parse(String, String),
    #[error(
        "the spliced npc file would not read back as the expected npc, so nothing was written: {0}"
    )]
    ReadBack(PathBuf),
    #[error(transparent)]
    Target(#[from] TargetError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Apply one candidate config to the pack's NPC file on disk.
///
/// - Replace: parse the existing RON, splice in the config, and **rewrite the
///   file text with only that question's block changed** — comments and all
///   other content survive byte-for-byte. The spliced text must read back as
///   the existing NPC with only that question set, or nothing is written.
/// - Create: refuse if the file exists; otherwise write identity + config.
pub fn apply_candidate(
    pack_root: &Path,
    npc: &Npc,
    question: &str,
    config: &DecisionConfig,
    mode: CreateMode,
) -> Result<(), SpliceError> {
    // Defensive writer adapter (contract §10): the supplied id becomes an
    // opaque checked target before any join, so a raw string can never name a
    // path outside the pack's `npcs/` subdirectory.
    let target = NpcTarget::new(&npc.id)?;
    let npc_path = target.path_in(pack_root);

    match mode {
        CreateMode::Create { name } => {
            if npc_path.exists() {
                return Err(SpliceError::AlreadyExists(npc_path));
            }
            let mut created = npc.clone();
            created.name = name;
            created
                .decisions
                .insert(question.to_string(), config.clone());
            let rendered = ron::ser::to_string_pretty(&created, ron::ser::PrettyConfig::default())
                .map_err(|e| SpliceError::Parse(npc_path.display().to_string(), e.to_string()))?;
            write_atomic(&npc_path, &format!("// npcs/{}.ron\n{rendered}", npc.id))?;
            Ok(())
        }
        CreateMode::Replace => {
            if !npc_path.exists() {
                return Err(SpliceError::NoSuchNpc(npc_path));
            }
            let src = read_pack_text(&npc_path)?;
            let existing: Npc = ron::from_str(&src)
                .map_err(|e| SpliceError::Parse(npc_path.display().to_string(), e.to_string()))?;
            let updated_src = splice_question(&src, &existing, question, config);
            #[cfg(test)]
            let updated_src = faults::corrupt_splice(&src, updated_src);
            // The splice edits text, not the parsed NPC, so check its result
            // before it reaches disk: it must read back as the existing NPC
            // with only this question set, or nothing is written.
            let mut expected = existing;
            expected
                .decisions
                .insert(question.to_string(), config.clone());
            if ron::from_str::<Npc>(&updated_src).ok() != Some(expected) {
                return Err(SpliceError::ReadBack(npc_path));
            }
            write_atomic(&npc_path, &updated_src)?;
            Ok(())
        }
    }
}

/// Write via a temp file + rename so a crash mid-write can never leave a
/// truncated content file behind (validation passed; the write must land
/// whole). Shared by the RON splice and the drafting scene splice.
///
/// `rename` atomically replaces an existing destination on both Unix and
/// Windows, so there is no delete-then-rename window. The temp file is unique
/// to the process and carries the destination's own extension plus `.tmp`, so
/// it can never be mistaken for pack content; it is removed if the rename
/// fails, leaving the original untouched. A **write** failure is reported
/// without a cleanup step, so a partial temp may stay beside the target; only
/// the rename failure above removes its temp.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let tmp = path.with_file_name(name);
    write_temp(&tmp, contents)?;
    if let Err(e) = rename_temp(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// The write phase: the whole new bytes land in `tmp`.
fn write_temp(tmp: &Path, contents: &str) -> std::io::Result<()> {
    #[cfg(test)]
    if let Some(landed) = faults::take_partial_write() {
        return faults::partial_temp_write(tmp, contents, landed);
    }
    std::fs::write(tmp, contents)
}

/// The rename phase: `tmp` replaces `path` as one file.
fn rename_temp(tmp: &Path, path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if faults::take_rename() {
        return Err(std::io::Error::other(faults::RENAME_FAILURE));
    }
    std::fs::rename(tmp, path)
}

/// Deterministic, test-only failure injection for the two write phases.
///
/// The receipts below need a truncated temp write and a refused replacement
/// over an existing regular file; an ordinary fixture cannot produce either
/// portably. None of this is compiled into a production build, and each fault
/// is armed for one write on one thread only.
#[cfg(test)]
mod faults {
    use std::cell::Cell;
    use std::io;
    use std::path::Path;

    /// Which phase the next write should fail in.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) enum WriteFault {
        /// Stop the temp write after this many bytes have landed.
        PartialWrite(usize),
        /// Refuse the rename after the complete temp has landed.
        Rename,
        /// Make the NPC splice produce text that does not parse.
        UnreadableSplice,
        /// Make the NPC splice return the original text, which parses but
        /// lacks the spliced question.
        UnchangedSplice,
    }

    /// What each injected failure reports, so a receipt can tell its own fault
    /// from an unrelated fixture error.
    pub(super) const PARTIAL_WRITE_FAILURE: &str = "injected partial temp write failure";
    pub(super) const RENAME_FAILURE: &str = "injected rename failure";

    thread_local! {
        static ARMED: Cell<Option<WriteFault>> = const { Cell::new(None) };
    }

    /// Arm `fault` for the next write on this thread. The guard disarms it on
    /// drop, so a panic cannot leave a later test on this thread armed.
    pub(super) fn arm(fault: WriteFault) -> Armed {
        ARMED.with(|armed| armed.set(Some(fault)));
        Armed
    }

    pub(super) struct Armed;

    impl Drop for Armed {
        fn drop(&mut self) {
            ARMED.with(|armed| armed.set(None));
        }
    }

    /// Take a partial-write fault; a rename fault stays armed for its phase.
    pub(super) fn take_partial_write() -> Option<usize> {
        ARMED.with(|armed| match armed.get() {
            Some(WriteFault::PartialWrite(landed)) => {
                armed.set(None);
                Some(landed)
            }
            _ => None,
        })
    }

    pub(super) fn take_rename() -> bool {
        ARMED.with(|armed| match armed.get() {
            Some(WriteFault::Rename) => {
                armed.set(None);
                true
            }
            _ => false,
        })
    }

    /// Apply an armed splice fault to the spliced text of `original`.
    pub(super) fn corrupt_splice(original: &str, spliced: String) -> String {
        ARMED.with(|armed| match armed.get() {
            Some(WriteFault::UnreadableSplice) => {
                armed.set(None);
                format!("{spliced})")
            }
            Some(WriteFault::UnchangedSplice) => {
                armed.set(None);
                original.to_string()
            }
            _ => spliced,
        })
    }

    /// Establish the partial temp the write phase leaves, then fail.
    pub(super) fn partial_temp_write(tmp: &Path, contents: &str, landed: usize) -> io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::File::create(tmp)?;
        file.write_all(&contents.as_bytes()[..landed.min(contents.len())])?;
        file.flush()?;
        Err(io::Error::other(PARTIAL_WRITE_FAILURE))
    }
}

/// Rewrite only the named question's decision block inside the original RON
/// text. Everything before and after the block — comments included — passes
/// through byte-for-byte.
fn splice_question(src: &str, existing: &Npc, question: &str, config: &DecisionConfig) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file beside the destination, so a receipt observes the writer's
    /// temps without re-deriving the writer's own naming rule.
    fn leftovers(dir: &Path) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.file_name() != Some(std::ffi::OsStr::new("scene.unmei")))
            .collect();
        paths.sort();
        paths
    }

    #[test]
    fn a_partial_temp_write_failure_keeps_the_target_and_a_whole_retry_lands() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.unmei");
        std::fs::write(&path, "old contents").unwrap();

        {
            let _armed = faults::arm(faults::WriteFault::PartialWrite(4));
            let error = write_atomic(&path, "a rewritten scene, longer than the old").unwrap_err();
            assert_eq!(error.to_string(), faults::PARTIAL_WRITE_FAILURE);
        }

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old contents");
        // This writer has no write-phase cleanup: the partial bytes stay in the
        // temp sibling. That is this receipt's observed postcondition, not a
        // promise; only a rename failure removes its temp.
        let temps = leftovers(dir.path());
        assert_eq!(temps.len(), 1, "one partial temp stays: {temps:?}");
        assert!(temps[0].to_string_lossy().ends_with(".tmp"), "{temps:?}");
        assert_eq!(std::fs::read_to_string(&temps[0]).unwrap(), "a re");

        write_atomic(&path, "a rewritten scene, longer than the old")
            .expect("the retry lands whole");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "a rewritten scene, longer than the old"
        );
        assert!(
            leftovers(dir.path()).is_empty(),
            "this process's retry leaves no temp"
        );
    }

    #[test]
    fn a_failed_replacement_removes_its_temp_and_keeps_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.unmei");
        std::fs::write(&path, "old contents").unwrap();

        {
            let _armed = faults::arm(faults::WriteFault::Rename);
            let error = write_atomic(&path, "new contents").unwrap_err();
            assert_eq!(error.to_string(), faults::RENAME_FAILURE);
        }

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old contents");
        assert!(
            leftovers(dir.path()).is_empty(),
            "the rename failure removes its temp"
        );

        write_atomic(&path, "new contents").expect("the retry lands whole");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new contents");
        assert!(leftovers(dir.path()).is_empty(), "the retry leaves no temp");
    }

    /// Splice fixtures: the demo pack's `ace.ron` with one edit, and a config to
    /// write into it.
    mod comment_fixtures {
        use unmei_engine::content::{Criteria, DecisionConfig, Fallback, Npc, QuestionType};

        pub(super) const ACE: &str = include_str!("../../examples/demo-pack/npcs/ace.ron");

        pub(super) fn config() -> DecisionConfig {
            DecisionConfig {
                question_type: QuestionType::Choice,
                instructions: "A new instruction.".into(),
                state_keys: vec!["met_ace".into()],
                criteria: Criteria::Choice(
                    [
                        ("open".to_string(), "x".to_string()),
                        ("shut".to_string(), "y".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                fallback: Fallback::Action("open".into()),
            }
        }

        /// `ace.ron` with `from` replaced by `to` exactly once.
        pub(super) fn ace_with(from: &str, to: &str) -> String {
            assert_eq!(
                ACE.matches(from).count(),
                1,
                "{from:?} occurs once in ace.ron"
            );
            ACE.replacen(from, to, 1)
        }

        /// Splice `question` into `src`, then require the result to read back as
        /// the original NPC with only that question set, keeping each of `kept`
        /// verbatim.
        pub(super) fn assert_spliced(src: &str, question: &str, kept: &[&str]) {
            let existing: Npc = ron::from_str(src).expect("the fixture parses");
            let out = super::super::splice_question(src, &existing, question, &config());
            let mut expected = existing;
            expected.decisions.insert(question.to_string(), config());
            let read: Result<Npc, _> = ron::from_str(&out);
            assert_eq!(read.ok(), Some(expected), "spliced file:\n{out}");
            for kept in kept {
                assert!(out.contains(kept), "{kept:?} survives in:\n{out}");
            }
        }
    }

    use comment_fixtures::{ace_with, assert_spliced};

    /// A pack whose `npcs/` holds the demo `ace.ron` and one sibling file.
    fn npc_pack() -> tempfile::TempDir {
        let pack = tempfile::tempdir().unwrap();
        let npcs = pack.path().join("npcs");
        std::fs::create_dir(&npcs).unwrap();
        std::fs::write(npcs.join("ace.ron"), comment_fixtures::ACE).unwrap();
        std::fs::write(npcs.join("bea.ron"), "// a sibling\n").unwrap();
        pack
    }

    /// Every file in the pack's `npcs/`, with its contents, in name order.
    fn npc_files(pack: &Path) -> Vec<(PathBuf, String)> {
        let mut files: Vec<_> = std::fs::read_dir(pack.join("npcs"))
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let text = std::fs::read_to_string(&path).unwrap();
                (path, text)
            })
            .collect();
        files.sort();
        files
    }

    fn ace() -> Npc {
        ron::from_str(comment_fixtures::ACE).unwrap()
    }

    #[test]
    fn a_replace_or_add_writes_the_npc_with_only_that_question_set() {
        for question in ["greet", "farewell"] {
            let pack = npc_pack();
            let config = comment_fixtures::config();
            apply_candidate(pack.path(), &ace(), question, &config, CreateMode::Replace).unwrap();
            let mut expected = ace();
            expected.decisions.insert(question.to_string(), config);
            let written = std::fs::read_to_string(pack.path().join("npcs/ace.ron")).unwrap();
            assert_eq!(
                ron::from_str::<Npc>(&written).ok(),
                Some(expected),
                "{question}"
            );
        }
    }

    #[test]
    fn a_splice_that_would_not_read_back_is_refused_and_writes_nothing() {
        for fault in [
            faults::WriteFault::UnreadableSplice,
            faults::WriteFault::UnchangedSplice,
        ] {
            let pack = npc_pack();
            let before = npc_files(pack.path());
            let result = {
                let _armed = faults::arm(fault);
                apply_candidate(
                    pack.path(),
                    &ace(),
                    "farewell",
                    &comment_fixtures::config(),
                    CreateMode::Replace,
                )
            };
            let error = result.expect_err("the splice is refused");
            assert!(error.to_string().contains("read back"), "{error}");
            assert_eq!(npc_files(pack.path()), before, "nothing is written");
        }
    }

    #[test]
    fn a_comment_naming_the_key_before_the_npc_is_not_spliced_into() {
        let comment = "// old \"greet\": (draft)\n";
        let src = ace_with("(\n    id:", &format!("{comment}(\n    id:"));
        assert_spliced(&src, "greet", &[comment]);
    }

    #[test]
    fn a_block_comment_with_a_brace_does_not_swallow_an_added_question() {
        let src = ace_with("        ),\n    },", "        ),\n        /* } */\n    },");
        assert_spliced(&src, "farewell", &["/* } */"]);
    }

    #[test]
    fn a_block_comment_with_a_paren_inside_the_block_does_not_end_it() {
        let src = ace_with(
            "            fallback: \"cold\",",
            "            /* ) */\n            fallback: \"cold\",",
        );
        // The comment sits inside the replaced block, so it goes with the block.
        assert_spliced(&src, "greet", &[]);
    }

    #[test]
    fn a_comment_with_a_paren_after_the_key_does_not_start_the_block() {
        let src = ace_with("\"greet\": (", "\"greet\": // (x)\n        (");
        assert_spliced(&src, "greet", &["// (x)"]);
    }

    #[test]
    fn a_comment_after_the_decisions_naming_a_new_key_is_not_spliced_into() {
        let comment = "    // \"farewell\": (\n";
        let src = ace_with("    },\n)", &format!("    }},\n{comment})"));
        assert_spliced(&src, "farewell", &[comment]);
    }

    /// The splice over generated NPC files: comment lines inserted anywhere,
    /// config strings holding brackets, quotes and comment markers, and a
    /// question that is either replaced or added.
    mod properties {
        use std::collections::HashMap;

        use proptest::prelude::*;
        use proptest::test_runner::{Config, RngSeed};
        use unmei_engine::content::{Criteria, DecisionConfig, Fallback, Npc, QuestionType};

        /// Fragments that confuse a scanner reading RON by hand. The first 12
        /// hold no `/` or `*`, so a block comment can use them safely.
        const FRAGMENTS: [&str; 14] = [
            "x",
            " ",
            "(",
            ")",
            "{",
            "}",
            "[",
            "]",
            "\"",
            "\\",
            "\"q0\": (",
            "decisions: {",
            "//",
            "r#\"",
        ];
        const QUESTIONS: [&str; 3] = ["q0", "q1", "q2"];

        fn text_from(fragments: &'static [&'static str]) -> impl Strategy<Value = String> {
            prop::collection::vec(prop::sample::select(fragments), 0..6)
                .prop_map(|parts| parts.concat())
        }

        fn text() -> impl Strategy<Value = String> {
            text_from(&FRAGMENTS)
        }

        /// A comment on its own line. A block comment's text leaves out `/` and
        /// `*`, so only its own nested `/* … */` opens or closes it.
        fn comment() -> impl Strategy<Value = String> {
            let plain = || text_from(&FRAGMENTS[..12]);
            prop_oneof![
                text().prop_map(|text| format!("// {text}")),
                (plain(), prop::option::of(plain())).prop_map(|(outer, inner)| match inner {
                    Some(inner) => format!("/* {outer} /* {inner} */ */"),
                    None => format!("/* {outer} */"),
                }),
            ]
        }

        fn descriptions(keys: Vec<String>) -> impl Strategy<Value = HashMap<String, String>> {
            prop::collection::vec(text(), keys.len())
                .prop_map(move |texts| keys.clone().into_iter().zip(texts).collect())
        }

        fn config() -> impl Strategy<Value = DecisionConfig> {
            let choice = prop::sample::subsequence(vec!["a", "b", "q1"], 1..=3)
                .prop_flat_map(|ids| {
                    let ids: Vec<String> = ids.into_iter().map(String::from).collect();
                    let fallback = prop::sample::select(ids.clone());
                    (descriptions(ids), fallback)
                })
                .prop_map(|(map, fallback)| {
                    (
                        QuestionType::Choice,
                        Criteria::Choice(map),
                        Fallback::Action(fallback),
                    )
                });
            let noul = (
                prop::option::of(descriptions(vec!["true".into(), "false".into()])),
                any::<bool>(),
            )
                .prop_map(|(map, fallback)| {
                    (
                        QuestionType::Noul,
                        Criteria::Noul(map),
                        Fallback::Bool(fallback),
                    )
                });
            let score = prop::collection::vec(text(), 2..=3).prop_flat_map(|levels| {
                let len = levels.len();
                (Just(levels), 0..len).prop_map(|(levels, index)| {
                    (
                        QuestionType::Score,
                        Criteria::Score(levels),
                        Fallback::Index(index),
                    )
                })
            });
            (
                text(),
                prop::sample::subsequence(vec!["met_ace", "trust"], 0..=2),
                prop_oneof![choice, noul, score],
            )
                .prop_map(
                    |(instructions, keys, (question_type, criteria, fallback))| DecisionConfig {
                        question_type,
                        instructions,
                        state_keys: keys.into_iter().map(String::from).collect(),
                        criteria,
                        fallback,
                    },
                )
        }

        fn npc() -> impl Strategy<Value = Npc> {
            (
                text(),
                prop::option::of(text()),
                prop::collection::hash_map(
                    prop::sample::select(&QUESTIONS[..]).prop_map(String::from),
                    config(),
                    1..=3,
                ),
            )
                .prop_map(|(name, model, decisions)| Npc {
                    id: "ace".into(),
                    name,
                    model,
                    decisions,
                })
        }

        /// One generated splice.
        #[derive(Debug, Clone)]
        struct Case {
            npc: Npc,
            /// The NPC pretty-printed, with comment lines inserted.
            src: String,
            /// The text the splice must keep byte for byte: everything before
            /// and after `question`'s config, or before and after the decisions
            /// map's closing brace when the question is new.
            before: String,
            after: String,
            question: String,
            config: DecisionConfig,
        }

        fn case() -> impl Strategy<Value = Case> {
            npc().prop_flat_map(|npc| {
                let text = ron::ser::to_string_pretty(&npc, ron::ser::PrettyConfig::default())
                    .expect("a generated NPC encodes");
                let lines = text.lines().count();
                let comments = prop::collection::vec((0..=lines, comment()), 0..6);
                let question = prop::sample::select(&QUESTIONS[..]).prop_map(String::from);
                (Just(npc), Just(text), comments, question, config()).prop_map(
                    |(npc, text, mut comments, question, config)| {
                        comments.sort_by_key(|(line, _)| *line);
                        let mut lines: Vec<String> = text.lines().map(String::from).collect();
                        for (line, comment) in comments.iter().rev() {
                            lines.insert(*line, comment.clone());
                        }
                        let src = lines.join("\n");
                        let cut = untouched(&lines, &question);
                        Case {
                            before: src[..cut.0].to_string(),
                            after: src[cut.1..].to_string(),
                            npc,
                            src,
                            question,
                            config,
                        }
                    },
                )
            })
        }

        /// The byte range of the joined `lines` a splice of `question` may
        /// change, read from the pretty-printed layout rather than by scanning
        /// RON. Each decision key sits alone at eight spaces with its config's
        /// `(` ending the line, and the config closes with `),` alone at the
        /// same indent; the decisions map closes with `},` at four. Inserted
        /// comment lines start with `//` or `/*`, so they match neither.
        fn untouched(lines: &[String], question: &str) -> (usize, usize) {
            let offset = |line: usize| lines[..line].iter().map(|l| l.len() + 1).sum::<usize>();
            let header = format!("        \"{question}\": (");
            match lines.iter().position(|line| *line == header) {
                Some(open) => {
                    let close = open
                        + lines[open..]
                            .iter()
                            .position(|line| line == "        ),")
                            .expect("a pretty-printed config closes");
                    (
                        offset(open) + header.len() - 1,
                        offset(close) + "        )".len(),
                    )
                }
                None => {
                    let close = lines
                        .iter()
                        .position(|line| line == "    },")
                        .expect("a pretty-printed decisions map closes");
                    let at = offset(close) + "    ".len();
                    (at, at)
                }
            }
        }

        proptest! {
            // A fixed seed keeps the workspace gate deterministic; a failure
            // becomes a named example test above, so nothing is persisted.
            #![proptest_config(Config {
                cases: 512,
                rng_seed: RngSeed::Fixed(0x756e_6d65_6902),
                failure_persistence: None,
                ..Config::default()
            })]

            /// Splicing one question leaves a file that reads back as the
            /// original NPC with only that question set, and every byte outside
            /// that question's config, comments included, is unchanged.
            #[test]
            fn a_splice_changes_only_the_one_config(case in case()) {
                let Case { npc, src, before, after, question, config } = case;
                let parsed: Npc = ron::from_str(&src).expect("the generated file parses");
                prop_assert_eq!(&parsed, &npc);
                let out = super::super::splice_question(&src, &npc, &question, &config);
                let mut expected = npc;
                expected.decisions.insert(question, config);
                let read: Result<Npc, _> = ron::from_str(&out);
                prop_assert_eq!(read.ok(), Some(expected), "spliced file:\n{}", out);
                prop_assert!(
                    out.len() >= before.len() + after.len()
                        && out.starts_with(&before)
                        && out.ends_with(&after),
                    "text outside the config changed:\n{out}"
                );
            }
        }
    }
}
