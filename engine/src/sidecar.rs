//! The sidecar file: the one naming rule and the one atomic write every file
//! beside the pack directory shares.
//!
//! A **sidecar** is a sibling of the pack directory, named after it, so the pack
//! itself stays pristine and distributable: the save slots ([`crate::save`]),
//! the read log ([`crate::read_log`]), and — in the app — the settings file and
//! the session log. Those classes are deliberately different (their versions,
//! their tolerance for a bad file and their error types all differ), so this
//! module owns only what they must agree on:
//!
//! - [`sibling_path`] — where a sidecar lives.
//! - [`write_atomic`] — how its bytes land, whole or not at all.
//!
//! What is *inside* a sidecar is its owner's business, and so is what a failed
//! write means to the caller. The credential file is deliberately not a sidecar
//! and keeps its own private write ([ADR 0010]): it lives outside the pack and
//! needs a mode the plain write cannot express.
//!
//! [ADR 0010]: ../../docs/adr/0010-credentials-live-in-a-user-level-file.md

use std::io;
use std::path::{Path, PathBuf};

/// The pack directory's sibling: `<parent>/<pack-name>.<suffix>`.
///
/// The whole pack directory name is kept, extension or not, so a pack at
/// `/games/my.pack` names its log `/games/my.pack.log`. A pack with no parent
/// keeps the bare relative name, and one with no name at all falls back to
/// `pack`, so a sidecar path is always derivable.
pub fn sibling_path(pack_root: &Path, suffix: &str) -> PathBuf {
    let parent = pack_root.parent().unwrap_or_else(|| Path::new("."));
    let name = pack_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pack");
    parent.join(format!("{name}.{suffix}"))
}

/// Write `contents` to the sidecar at `path`, atomically.
///
/// The bytes land in the temp sibling `<path>.ron.tmp` — derived from the whole
/// target path, so no two sidecars can share one — and are renamed over the
/// target, so a reader never sees a half-written file and a failed write leaves
/// the target exactly as it was. The next write overwrites any temp it finds.
///
/// A failure in the **write** phase leaves a partial temp, so that temp is
/// removed. A failure in the **rename** phase is different: the temp then holds
/// a *complete* copy of the new bytes, and it is deliberately left beside the
/// target — a rename can fail on a locked target (a scanner, an indexer or a
/// second instance on Windows) while the target still holds the previous bytes,
/// and that temp is the only complete copy of the newest write.
///
/// The error is the raw IO error; the caller adds the path it asked for, because
/// only the caller knows which sidecar it was writing.
pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let temp = path.with_extension("ron.tmp");
    if let Err(error) = write_temp(&temp, contents) {
        discard(&temp);
        return Err(error);
    }
    rename_temp(&temp, path)
}

/// The write phase: the whole new bytes land in `temp`.
fn write_temp(temp: &Path, contents: &str) -> io::Result<()> {
    #[cfg(test)]
    if let Some(landed) = faults::take_partial_write() {
        return faults::partial_temp_write(temp, contents, landed);
    }
    std::fs::write(temp, contents)
}

/// The rename phase: `temp` replaces `path` as one file.
fn rename_temp(temp: &Path, path: &Path) -> io::Result<()> {
    #[cfg(test)]
    if faults::take_rename() {
        return Err(io::Error::other(faults::RENAME_FAILURE));
    }
    std::fs::rename(temp, path)
}

/// Best-effort removal of a temp sibling this write created. Never overwrites
/// the original error, and never removes anything but that one path.
fn discard(temp: &Path) {
    let _ = std::fs::remove_file(temp);
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
    pub(super) fn partial_temp_write(temp: &Path, contents: &str, landed: usize) -> io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::File::create(temp)?;
        file.write_all(&contents.as_bytes()[..landed.min(contents.len())])?;
        file.flush()?;
        Err(io::Error::other(PARTIAL_WRITE_FAILURE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sidecar_is_named_after_the_whole_pack_directory() {
        assert_eq!(
            sibling_path(Path::new("/games/demo-pack"), "log"),
            PathBuf::from("/games/demo-pack.log")
        );
        // An extension on the pack directory is part of its name, never replaced.
        assert_eq!(
            sibling_path(Path::new("/games/my.pack"), "settings.ron"),
            PathBuf::from("/games/my.pack.settings.ron")
        );
        // The slot suffix is composed by the caller and lands in one place.
        assert_eq!(
            sibling_path(Path::new("/games/demo-pack"), "slot1.save.ron"),
            PathBuf::from("/games/demo-pack.slot1.save.ron")
        );
        // A parentless pack keeps its relative name.
        assert_eq!(
            sibling_path(Path::new("demo-pack"), "read.ron"),
            PathBuf::from("demo-pack.read.ron")
        );
    }

    #[test]
    fn a_sidecar_write_replaces_the_whole_file_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo-pack.settings.ron");

        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second, which is longer").unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "second, which is longer"
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "only the sidecar is left"
        );
    }

    #[test]
    fn an_immediate_write_failure_leaves_the_target_intact() {
        // A directory where the temp sibling goes: the write itself fails.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo-pack.slot1.save.ron");
        std::fs::write(&path, "previous save").unwrap();
        std::fs::create_dir(path.with_extension("ron.tmp")).unwrap();

        assert!(write_atomic(&path, "new save").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous save");
    }

    #[test]
    fn a_directory_target_fails_the_rename_and_keeps_the_complete_new_bytes() {
        // The target is a directory, so the write succeeds and the rename fails:
        // the temp then holds the only complete copy of the new bytes.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo-pack.read.ron");
        std::fs::create_dir(&path).unwrap();

        assert!(write_atomic(&path, "new log").is_err());
        let temp = path.with_extension("ron.tmp");
        assert_eq!(
            std::fs::read_to_string(&temp).unwrap(),
            "new log",
            "the complete copy stays beside the target"
        );
    }

    #[test]
    fn a_write_failing_after_a_partial_temp_discards_it_and_keeps_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo-pack.slot1.save.ron");
        std::fs::write(&path, "previous save").unwrap();
        let before = std::fs::read(&path).unwrap();
        let temp = path.with_extension("ron.tmp");

        {
            let _armed = faults::arm(faults::WriteFault::PartialWrite(4));
            let error = write_atomic(&path, "a whole new save").unwrap_err();
            assert_eq!(error.to_string(), faults::PARTIAL_WRITE_FAILURE);
        }

        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "byte-identical target"
        );
        assert!(!temp.exists(), "the partial temp was discarded");

        write_atomic(&path, "a whole new save").expect("the retry lands whole");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a whole new save");
        assert!(!temp.exists(), "the retry leaves no temp");
    }

    #[test]
    fn a_failed_replacement_keeps_the_target_and_the_complete_new_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo-pack.read.ron");
        std::fs::write(&path, "previous log").unwrap();
        let temp = path.with_extension("ron.tmp");

        {
            let _armed = faults::arm(faults::WriteFault::Rename);
            let error = write_atomic(&path, "new log").unwrap_err();
            assert_eq!(error.to_string(), faults::RENAME_FAILURE);
        }

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous log");
        assert_eq!(
            std::fs::read_to_string(&temp).unwrap(),
            "new log",
            "the complete copy stays beside the target"
        );

        write_atomic(&path, "new log").expect("the retry lands whole");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new log");
        assert!(!temp.exists(), "the retry leaves no temp");
    }
}
