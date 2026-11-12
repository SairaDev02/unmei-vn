//! The read log: the player's pack-wide knowledge of what they have read.
//!
//! Runtime-free by design, mirroring [`crate::save`]. The log is the third
//! persistence class beside the save slots ([ADR 0006]) and the settings file
//! ([ADR 0007]): cheap to lose, dangerous to misread. It is a sibling of the
//! pack directory, named after it — `<pack-dir>.read.ron` — RON, and written
//! atomically (temp sibling plus rename).
//!
//! A **say-line** is keyed by `(scene id, authored source line)`. The authored
//! source line is what [`crate::content::Scene::source_line`] records for every
//! command: at a say-park the runner's `pc` is the say-line's `lines` index plus
//! one, so the app's key is `source_line(pc - 1)`. Simultaneously the log holds
//! the **visited** scene ids — scenes the player has played in.
//!
//! The version policy is **strict equality** (ADR 0011): a file loads only when
//! `version == READ_LOG_VERSION`. An absent, undecodable, wrongly versioned or
//! IO-unreadable file means **everything unread** — the empty log, never an
//! error, and the bytes on disk are left exactly as they are. A later flush
//! writes the current format over whatever was there. The **app** owns the marks
//! and *when* the file is written; this module owns the naming, the format and
//! the IO.
//!
//! [ADR 0006]: ../../docs/adr/0006-save-slots-on-disk-format-and-version-policy.md
//! [ADR 0007]: ../../docs/adr/0007-settings-file-beside-the-pack.md

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::content::Pack;

/// On-disk read-log format version. Bump when the shape of [`ReadLog`] changes.
///
/// The policy is **strict equality** (ADR 0011), deliberately unlike the save
/// slots' additive floor: a partially understood older key set risks skipping
/// text the player never saw. A bump therefore re-locks every line and every
/// scene rather than migrating them.
pub const READ_LOG_VERSION: u32 = 1;

/// The whole read log, as the app holds it in memory.
///
/// Both collections are ordered on purpose: [`BTreeMap`] / [`BTreeSet`]
/// serialize in sorted order, so writing an unchanged log twice produces
/// identical bytes. The RON wire shape is `version`, then `read` (scene id ->
/// sorted authored source lines), then `visited` (sorted scene ids).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadLog {
    pub version: u32,
    /// Scene id -> the authored 1-based source lines of the say-lines seen.
    pub read: BTreeMap<String, BTreeSet<usize>>,
    /// The scene ids the player has played in.
    pub visited: BTreeSet<String>,
}

impl Default for ReadLog {
    fn default() -> Self {
        ReadLog {
            version: READ_LOG_VERSION,
            read: BTreeMap::new(),
            visited: BTreeSet::new(),
        }
    }
}

impl ReadLog {
    /// Whether the say-line at `(scene_id, source_line)` has been read.
    pub fn is_read(&self, scene_id: &str, source_line: usize) -> bool {
        self.read
            .get(scene_id)
            .is_some_and(|lines| lines.contains(&source_line))
    }

    /// Mark one say-line read. Returns `true` only when it was newly inserted —
    /// the app's dirty trigger.
    pub fn mark_read(&mut self, scene_id: &str, source_line: usize) -> bool {
        self.read
            .entry(scene_id.to_string())
            .or_default()
            .insert(source_line)
    }

    /// Whether the scene id has been visited.
    pub fn is_visited(&self, scene_id: &str) -> bool {
        self.visited.contains(scene_id)
    }

    /// Mark one scene visited. Returns `true` only when it was newly inserted.
    pub fn mark_visited(&mut self, scene_id: &str) -> bool {
        self.visited.insert(scene_id.to_string())
    }
}

/// The sibling path of the pack's read log: `<pack-dir>.read.ron`.
///
/// "Next to the pack": the pack directory stays pristine and distributable;
/// every player file sits beside it (ADR 0011).
pub fn read_log_path_for(pack: &Pack) -> PathBuf {
    crate::sidecar::sibling_path(&pack.root, "read.ron")
}

/// Read the log from its path.
///
/// **Never an error**: a missing file is the defined first-run state — the
/// empty log with no reason — while a *present* file that cannot be read, does
/// not decode or carries another version is the empty log plus the reason the
/// caller logs (row 26). Nothing is deleted, renamed or truncated, and a
/// mismatch is not migrated: the next dirty flush writes the current format.
///
/// Decoding is pack-free: scene ids and source lines no pack knows are schema-valid
/// content and are kept. Filtering against the loaded pack is a separate app-side
/// boundary, so a decode must not destroy entries the file legitimately holds.
pub fn load_read_log(path: &Path) -> (ReadLog, Option<String>) {
    let missing = matches!(path.try_exists(), Ok(false));
    if missing {
        return (ReadLog::default(), None);
    }
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            return (
                ReadLog::default(),
                Some(format!("could not read `{}`: {error}", path.display())),
            )
        }
    };
    let log: ReadLog = match ron::from_str(&text) {
        Ok(log) => log,
        Err(error) => {
            return (
                ReadLog::default(),
                Some(format!("invalid read log `{}`: {error}", path.display())),
            )
        }
    };
    if log.version != READ_LOG_VERSION {
        return (
            ReadLog::default(),
            Some(format!(
                "read log `{}` is version {} (expected {READ_LOG_VERSION})",
                path.display(),
                log.version
            )),
        );
    }
    (log, None)
}

/// Write the whole log atomically ([`crate::sidecar::write_atomic`]). Never once
/// per line, never a partial write, never a merge.
pub fn write_read_log(path: &Path, log: &ReadLog) -> Result<(), ReadLogError> {
    let text = ron::ser::to_string_pretty(log, ron::ser::PrettyConfig::default())
        .map_err(|e| ReadLogError::Encode(e.to_string()))?;
    crate::sidecar::write_atomic(path, &text).map_err(|e| ReadLogError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ReadLogError {
    #[error("could not read/write read log `{}`: {message}", path.display())]
    Io { path: PathBuf, message: String },
    #[error("could not encode read log: {0}")]
    Encode(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Line;
    use crate::runner::ScriptRunner;
    use std::path::Path as StdPath;

    fn demo_pack() -> Pack {
        let root = StdPath::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
        crate::pack::load_pack(&root).expect("demo pack loads").pack
    }

    /// The demo pack pointed at a temp dir, so sibling files never dirty the
    /// repo. The runner reads only pack content, never `root`.
    fn temp_pack() -> (tempfile::TempDir, Pack) {
        let dir = tempfile::tempdir().unwrap();
        let mut pack = demo_pack();
        pack.root = dir.path().join("demo-pack");
        (dir, pack)
    }

    fn read_bytes(path: &Path) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }

    // --- Path derivation ---------------------------------------------------

    #[test]
    fn the_read_log_is_the_packs_sibling_named_after_it() {
        let (dir, pack) = temp_pack();
        let path = read_log_path_for(&pack);
        assert_eq!(path, dir.path().join("demo-pack.read.ron"));
        assert!(
            !path.starts_with(&pack.root),
            "never a path inside the pack"
        );
    }

    #[test]
    fn the_read_log_shares_the_sibling_derivation_with_the_slots() {
        let (dir, pack) = temp_pack();
        let slot = crate::save::save_path_for(&pack, crate::save::Slot::Manual(1));
        assert_eq!(slot.parent(), read_log_path_for(&pack).parent());
        assert_eq!(
            slot.parent(),
            Some(dir.path()),
            "both siblings sit beside the pack"
        );
    }

    // --- The key -----------------------------------------------------------

    #[test]
    fn a_say_park_records_the_authored_source_line() {
        let pack = demo_pack();
        let mut runner = ScriptRunner::new(pack.clone());
        let outcome = runner.run();
        assert!(matches!(
            outcome.wait,
            Some(crate::runner::Wait::Say { .. })
        ));
        let scene = pack.scenes.get(runner.scene_id()).unwrap();
        let idx = runner.pc() - 1;
        let line = scene.source_line(idx);
        assert_ne!(line, 0);
        assert!(matches!(scene.lines[idx], Line::Say { .. }));

        let mut log = ReadLog::default();
        assert!(log.mark_read(runner.scene_id(), line));
        assert!(log.is_read(runner.scene_id(), line));
    }

    #[test]
    fn two_visits_to_the_same_line_leave_one_entry() {
        let mut log = ReadLog::default();
        // A `{var}` say-line whose interpolated text differs between visits is
        // still one key: the mark is on the authored source line.
        assert!(log.mark_read("scene", 12));
        assert!(!log.mark_read("scene", 12));
        assert_eq!(log.read.get("scene").map(BTreeSet::len), Some(1));
    }

    #[test]
    fn marks_report_only_new_insertions() {
        let mut log = ReadLog::default();
        assert!(log.mark_visited("scene_a"));
        assert!(!log.mark_visited("scene_a"));
        assert!(log.is_visited("scene_a"));
        assert!(!log.is_visited("scene_b"));
    }

    // --- Version policy ----------------------------------------------------

    #[test]
    fn a_missing_file_is_the_empty_log_with_no_reason() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.read.ron");
        let (log, reason) = load_read_log(&path);
        assert_eq!(log, ReadLog::default());
        assert_eq!(reason, None);
        assert!(!path.exists(), "a missing file stays missing");
    }

    #[test]
    fn an_undecodable_file_is_empty_plus_a_reason_and_keeps_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.read.ron");
        std::fs::write(&path, "not ron at all {").unwrap();
        let before = read_bytes(&path);

        let (log, reason) = load_read_log(&path);
        assert_eq!(log, ReadLog::default());
        assert!(
            reason.is_some(),
            "a present undecodable file explains itself"
        );
        assert_eq!(read_bytes(&path), before, "the bytes are untouched");
    }

    #[test]
    fn an_io_unreadable_file_is_empty_plus_a_reason_and_is_not_renamed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.read.ron");
        // A directory at the log path makes the read fail (an unreadable file).
        std::fs::create_dir(&path).unwrap();

        let (log, reason) = load_read_log(&path);
        assert_eq!(log, ReadLog::default());
        assert!(reason.is_some(), "an unreadable file explains itself");
        assert!(path.is_dir(), "the target is not deleted or renamed");
    }

    #[test]
    fn older_and_newer_versions_are_both_empty() {
        for version in [0u32, 2] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("pack.read.ron");
            let text = format!("(version: {version}, read: {{}}, visited: [])");
            std::fs::write(&path, &text).unwrap();
            let before = read_bytes(&path);

            let (log, reason) = load_read_log(&path);
            assert_eq!(log, ReadLog::default(), "version {version} is unread");
            assert!(reason.is_some());
            assert_eq!(read_bytes(&path), before);
        }
    }

    #[test]
    fn schema_valid_unknown_scene_ids_and_source_lines_survive_decode() {
        // Content authored by a *different* pack: no pack is consulted, so these
        // ids and source lines are retained rather than read as an unusable file.
        // Line 0 is schema-valid here; the app's launch filter, not this decoder,
        // is what refuses it.
        let text = r#"(
            version: 1,
            read: {
                "scene_from_another_pack": [0, 1, 7],
                "scene_edited_since": [42],
            },
            visited: ["never_in_this_pack", "scene_edited_since"],
        )"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.read.ron");
        std::fs::write(&path, text).unwrap();

        let (log, reason) = load_read_log(&path);

        assert_eq!(reason, None, "a schema-valid file is not unusable");
        let expected_read = BTreeMap::from([
            (
                "scene_from_another_pack".to_string(),
                BTreeSet::from([0, 1, 7]),
            ),
            ("scene_edited_since".to_string(), BTreeSet::from([42])),
        ]);
        assert_eq!(log.read, expected_read);
        assert_eq!(
            log.visited,
            BTreeSet::from([
                "never_in_this_pack".to_string(),
                "scene_edited_since".to_string()
            ])
        );
    }

    #[test]
    fn a_later_write_replaces_a_wrong_version_with_the_current_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.read.ron");
        std::fs::write(&path, "(version: 2, read: {}, visited: [])").unwrap();

        let (mut log, _) = load_read_log(&path);
        assert!(log.mark_read("scene", 3));
        write_read_log(&path, &log).unwrap();

        let (reloaded, reason) = load_read_log(&path);
        assert_eq!(reason, None);
        assert_eq!(reloaded.version, READ_LOG_VERSION);
        assert!(reloaded.is_read("scene", 3));
    }

    #[test]
    fn default_serializes_with_the_current_version_and_empty_collections() {
        let text = ron::ser::to_string_pretty(&ReadLog::default(), Default::default()).unwrap();
        let decoded: ReadLog = ron::from_str(&text).unwrap();
        assert_eq!(decoded, ReadLog::default());
        assert_eq!(decoded.version, READ_LOG_VERSION);
        assert!(decoded.read.is_empty());
        assert!(decoded.visited.is_empty());
    }

    // --- Round trip + sorted, stable bytes ---------------------------------

    #[test]
    fn the_log_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.read.ron");
        let mut log = ReadLog::default();
        log.mark_read("reunion", 12);
        log.mark_read("reunion", 13);
        log.mark_read("first_meeting", 4);
        log.mark_visited("first_meeting");
        log.mark_visited("reunion");

        write_read_log(&path, &log).unwrap();
        let (reloaded, reason) = load_read_log(&path);
        assert_eq!(reason, None);
        assert_eq!(reloaded, log);
    }

    #[test]
    fn the_bytes_are_sorted_and_stable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.read.ron");

        // Insert out of sorted order on purpose.
        let mut log = ReadLog::default();
        log.mark_read("zeta", 9);
        log.mark_read("alpha", 5);
        log.mark_read("alpha", 2);
        log.mark_visited("zeta");
        log.mark_visited("alpha");

        write_read_log(&path, &log).unwrap();
        let first = std::fs::read_to_string(&path).unwrap();
        write_read_log(&path, &log).unwrap();
        let second = std::fs::read_to_string(&path).unwrap();
        assert_eq!(first, second, "writing an unchanged log is byte-stable");

        // Scene ids and source lines both appear in sorted order.
        let alpha = first.find("alpha").unwrap();
        let zeta = first.find("zeta").unwrap();
        assert!(alpha < zeta, "scene ids sort: {first}");
        let two = first.find("2").unwrap();
        let five = first.find("5").unwrap();
        let nine = first.find("9").unwrap();
        assert!(two < five && five < nine, "source lines sort: {first}");
    }

    #[test]
    fn a_write_error_carries_the_path() {
        let dir = tempfile::tempdir().unwrap();
        // A directory at the target path makes the rename fail.
        let path = dir.path().join("pack.read.ron");
        std::fs::create_dir(&path).unwrap();
        let error = write_read_log(&path, &ReadLog::default()).unwrap_err();
        assert!(matches!(error, ReadLogError::Io { .. }));
        assert!(error.to_string().contains("pack.read.ron"));
    }
}
