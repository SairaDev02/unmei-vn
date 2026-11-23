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

mod question;

#[cfg(test)]
mod tests;

use question::splice_question;
