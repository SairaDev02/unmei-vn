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
    #[error(transparent)]
    Target(#[from] TargetError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Apply one candidate config to the pack's NPC file on disk.
///
/// - Replace: parse the existing RON, splice in the config, and **rewrite the
///   file text with only that question's block changed** — comments and all
///   other content survive byte-for-byte.
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
    // The block's text starts at `"question": (` and ends at the matching `)`.
    let needle = format!("\"{question}\":");
    if let Some(start) = find_block_start(src, &needle) {
        let end = find_block_end(src, start);
        let rendered = ron::ser::to_string_pretty(config, pretty())
            .unwrap_or_else(|_| ron::ser::to_string(config).unwrap());
        let indented = indent(&rendered, &indent_of(&src[..start]));
        let mut out = String::with_capacity(src.len() + 128);
        out.push_str(&src[..start]);
        out.push_str(&indented);
        out.push_str(&src[end..]);
        out
    } else {
        // New question: insert a block before the closing paren of `decisions`,
        // reusing whatever trailing structure the file has.
        insert_question(src, existing, question, config)
    }
}

/// Pretty config with RON-style spacing the content files use.
fn pretty() -> ron::ser::PrettyConfig {
    ron::ser::PrettyConfig::default()
        .indentor("    ".to_string())
        .separate_tuple_members(true)
}

/// Find where the named block's `(` begins (after the `"question":` needle).
fn find_block_start(src: &str, needle: &str) -> Option<usize> {
    let pos = src.find(needle)?;
    let after = &src[pos + needle.len()..];
    let open = after.find('(')?;
    Some(pos + needle.len() + open)
}

/// Walk to the `)` that closes the `(` at `start`, skipping strings/comments.
fn find_block_end(src: &str, start: usize) -> usize {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    let mut in_string = false;
    let mut in_comment = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_comment {
            if c == b'\n' {
                in_comment = false;
            }
        } else if in_string {
            if c == b'\\' {
                i += 1; // skip escaped char
            } else if c == b'"' {
                in_string = false;
            }
        } else {
            match c {
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => in_comment = true,
                b'"' => in_string = true,
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    src.len()
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

/// Insert a new question's config before the `decisions` map's closing brace.
fn insert_question(src: &str, existing: &Npc, question: &str, config: &DecisionConfig) -> String {
    // Find the `decisions: {` map and insert before its closing `}`.
    let marker = "decisions: {";
    let map_pos = match src.find(marker) {
        Some(p) => p,
        None => {
            // No decisions map at all — shouldn't happen for a parsed NPC;
            // fall back to re-serializing (loses comments, but unreachable in
            // practice since every pack NPC has decisions).
            let mut npc = existing.clone();
            npc.decisions.insert(question.to_string(), config.clone());
            return ron::ser::to_string_pretty(&npc, pretty())
                .unwrap_or_else(|_| ron::ser::to_string(&npc).unwrap());
        }
    };
    let map_close = find_map_close(src, map_pos + marker.len() - 1);

    let rendered = ron::ser::to_string_pretty(config, pretty())
        .unwrap_or_else(|_| ron::ser::to_string(config).unwrap());
    let line_indent = indent_of(&src[..map_pos]) + "    ";
    let indented = indent(&rendered, &line_indent);
    let mut rebuilt = String::with_capacity(src.len() + rendered.len() + 32);
    rebuilt.push_str(&src[..map_close]);
    rebuilt.push('\n');
    rebuilt.push_str(&line_indent);
    rebuilt.push_str(&format!("\"{question}\": "));
    rebuilt.push_str(&indented);
    rebuilt.push(',');
    rebuilt.push_str(&src[map_close..]);
    rebuilt
}

/// Find the `}` closing the map whose `{` sits at `open`.
fn find_map_close(src: &str, open: usize) -> usize {
    find_matching(src, open, b'{', b'}')
}

fn find_matching(src: &str, open: usize, open_c: u8, close_c: u8) -> usize {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    let mut in_string = false;
    let mut in_comment = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_comment {
            if c == b'\n' {
                in_comment = false;
            }
        } else if in_string {
            if c == b'\\' {
                i += 1;
            } else if c == b'"' {
                in_string = false;
            }
        } else {
            match c {
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => in_comment = true,
                b'"' => in_string = true,
                x if x == open_c => depth += 1,
                x if x == close_c => {
                    depth -= 1;
                    if depth == 0 {
                        return i;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    src.len()
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
}
