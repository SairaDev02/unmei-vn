//! The `<pack-dir>.log` session sink: its path, its one shared file writer and
//! the tracing layer that flushes every line.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bevy::log::tracing_subscriber;
use bevy::log::tracing_subscriber::fmt::MakeWriter;
use bevy::log::BoxedLayer;
use bevy::prelude::*;

use super::funnel::{report, DefensiveHold, Failure};
use super::notice::Notice;
use super::rows::FailureRow;

/// The pack's sibling session log path: `<pack-dir>.log`, named after the pack
/// exactly like the save and settings files.
pub fn session_log_path_for(pack_root: &Path) -> PathBuf {
    unmei_engine::sidecar::sibling_path(pack_root, "log")
}

/// The session log's file name as the error screen names it — `<pack-name>.log`.
///
/// Derived from [`session_log_path_for`] rather than formatted separately, so the
/// pointer a player reads and the file that gets written cannot drift apart.
pub fn session_log_file_name(pack_root: &Path) -> String {
    let path = session_log_path_for(pack_root);
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// An open (and already truncated) session log: one writer, shared by the
/// tracing layer, flushed after every write.
#[derive(Resource, Clone)]
pub struct SessionLog {
    path: PathBuf,
    file: Arc<Mutex<File>>,
}

impl SessionLog {
    /// Create/truncate `<path>` and write `header` as the first line, so a log a
    /// player keeps names the release that wrote it.
    pub fn open(path: PathBuf, header: &str) -> io::Result<Self> {
        let mut file = File::create(&path)?;
        writeln!(file, "{header}")?;
        file.flush()?;
        Ok(SessionLog {
            path,
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn file(&self) -> Arc<Mutex<File>> {
        Arc::clone(&self.file)
    }
}

/// The session log a launch brings to composition: opened, refused, or never
/// attempted because no pack path existed for a `<pack-dir>.log` to sit beside.
pub enum SessionLogResult {
    /// The log is open and already truncated, its first line written.
    Opened(SessionLog),
    /// Creating the log failed; play continues console-only (row 21).
    Unwritable(LogOpenError),
    /// No log was attempted, so nothing reports a log failure either.
    NotAttempted,
}

/// The session log could not be created or written. Recorded by the entry point
/// and reported once at startup; play continues console-only.
#[derive(Resource, Debug, Clone)]
pub struct LogOpenError {
    pub path: PathBuf,
    pub message: String,
}

/// Report a [`LogOpenError`] once (row 21): a `warn!` under [`LOG_TARGET`], no
/// Notice. The file that failed is the one that is missing, so this line lands
/// on Bevy's console sink — which writes to `stderr` — fulfilling row 21's one
/// `stderr` warning while play continues console-only.
///
/// [`LOG_TARGET`]: super::LOG_TARGET
pub fn report_log_open_error(
    error: Option<Res<LogOpenError>>,
    mut notice: ResMut<Notice>,
    mut hold: ResMut<DefensiveHold>,
) {
    let Some(error) = error else {
        return;
    };
    report(
        Failure::new(
            FailureRow::LogUnwritable,
            format!("`{}`: {}", error.path.display(), error.message),
        ),
        &mut notice,
        &mut hold,
    );
}

/// A file writer that flushes after every write, so a hard kill still leaves the
/// tail on disk.
struct LineFlushedFile(Arc<Mutex<File>>);

impl<'a> MakeWriter<'a> for LineFlushedFile {
    type Writer = LineFlushedWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LineFlushedWriter(Arc::clone(&self.0))
    }
}

struct LineFlushedWriter(Arc<Mutex<File>>);

impl Write for LineFlushedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut file = self
            .0
            .lock()
            .map_err(|_| io::Error::other("session log writer lock poisoned"))?;
        let written = file.write(buf)?;
        file.flush()?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("session log writer lock poisoned"))?
            .flush()
    }
}

/// The `<pack-dir>.log` sink as a [`tracing`] layer. Added to Bevy's `LogPlugin`
/// via `custom_layer`, it sits inside the same subscriber and therefore receives
/// exactly the events the console sink receives, under the same filter.
pub fn file_layer(file: Arc<Mutex<File>>) -> BoxedLayer {
    Box::new(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(LineFlushedFile(file)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::captures::{capture_records, install_capture};
    use super::super::LOG_TARGET;
    use bevy::log::tracing;
    use bevy::log::tracing_subscriber::prelude::*;
    #[test]
    fn the_session_log_path_is_the_pack_sibling() {
        let path = session_log_path_for(Path::new("/games/demo-pack"));
        assert_eq!(path, PathBuf::from("/games/demo-pack.log"));
    }
    #[test]
    fn opening_the_session_log_truncates_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("pack.log");
        std::fs::write(&path, "a previous session\n").expect("seed the file");

        let log = SessionLog::open(path.clone(), "unmei-play 9.9.9").expect("open the log");
        assert_eq!(log.path(), path.as_path());
        drop(log);
        let bytes = std::fs::read_to_string(&path).expect("read back");
        assert!(
            !bytes.contains("a previous session"),
            "opening truncates the log: {bytes:?}"
        );
    }

    /// The version is the session log's first line, so a log a player attaches
    /// to a bug report names the build that wrote it.
    #[test]
    fn the_session_log_opens_with_the_release_version_as_its_first_line() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("pack.log");
        std::fs::write(&path, "a previous session\n").expect("seed the file");

        let version = crate::launch::version_text();
        SessionLog::open(path.clone(), &version).expect("open the log");

        let text = std::fs::read_to_string(&path).expect("read back");
        let mut lines = text.lines();
        assert_eq!(
            lines.next(),
            Some(version.as_str()),
            "the first line names the release version: {text:?}"
        );
        assert_eq!(
            lines.next(),
            None,
            "nothing from the previous session survives: {text:?}"
        );
    }
    #[test]
    fn the_file_layer_receives_the_console_stream_under_the_same_filter() {
        use tracing_subscriber::filter::EnvFilter;
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("pack.log");
        let log = SessionLog::open(path.clone(), "unmei-play 9.9.9").expect("open the log");

        let subscriber = tracing_subscriber::registry()
            .with(file_layer(log.file()))
            .with(EnvFilter::new("info"));
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "unmei::failure", "and N more");
            tracing::debug!(target: "unmei::failure", "below the filter");
        });

        let bytes = std::fs::read_to_string(&path).expect("read the log");
        assert!(
            bytes.contains("and N more"),
            "the file must receive the console stream: {bytes:?}"
        );
        assert!(
            !bytes.contains("below the filter"),
            "the shared filter must hide what the console hides: {bytes:?}"
        );
    }
    #[test]
    fn a_log_open_failure_reports_once_without_a_notice() {
        use bevy::ecs::system::RunSystemOnce;

        install_capture();
        let mut app = App::new();
        app.insert_resource(LogOpenError {
            path: PathBuf::from("/read-only/pack.log"),
            message: "permission denied".into(),
        })
        .init_resource::<Notice>()
        .init_resource::<DefensiveHold>();

        capture_records();
        app.world_mut()
            .run_system_once(report_log_open_error)
            .expect("the startup system runs once");

        let records = capture_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, log::Level::Warn);
        assert_eq!(records[0].1, LOG_TARGET);
        assert!(records[0].2.contains("/read-only/pack.log"));
        assert!(
            !app.world().resource::<Notice>().is_showing(),
            "row 21 must not notice"
        );
    }
}
