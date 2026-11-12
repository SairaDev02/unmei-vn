//! The player's OpenRouter credential: a user-level plain-text file holding the
//! key and nothing else ([ADR 0010], [`docs/title-and-menu.md` § Decisions
//! settings]).
//!
//! Unlike every other sidecar the runtime writes — the save slots, the
//! pack-adjacent settings file, `<pack-dir>.log` — this file is **not** a pack
//! sibling: a pack directory can be zipped and shared without carrying a
//! working key. The path is one pure helper, the read is the whole file trimmed,
//! the write is one atomic temp-sibling-plus-rename, and `OPENROUTER_API_KEY`
//! (when non-empty) overrides the stored value rather than replacing it.
//!
//! Two rules shape the code here:
//!
//! - **No failure can quote the value.** There is no parse step (a plain-text
//!   file has no parse error) and every error, warning and fingerprint is built
//!   from the path and the IO error only. `KeyRow` deliberately carries a
//!   fingerprint and never the key; the raw bytes live only long enough to hand
//!   a shape-valid value to the resolver builder.
//! - **Tests never touch the real `%APPDATA%`/`~/.config`.** Path derivation is
//!   pure over given values ([`ConfigEnv`]) and every read/write takes an
//!   explicit config directory, so the real environment is read exactly once, in
//!   `main`, by [`default_config_dir`].
//!
//! Editing, clearing and the clipboard live here; the interactive row itself is
//! [`crate::menu`]'s, and the ping verdicts and the `KeyCheck` seam are
//! [`crate::core`]'s. The row states a commit can pass through — `Checking…`,
//! `Set`, `Set — … not checked`, `Invalid — rejected by OpenRouter` — are
//! [`KeyRow`]'s.
//!
//! [ADR 0010]: ../../docs/adr/0010-credentials-live-in-a-user-level-file.md
//! [`docs/title-and-menu.md` § Decisions settings]: ../../docs/title-and-menu.md#decisions-settings

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::clipboard::Clipboard;
use bevy::prelude::*;
use bevy::text::TextEdit;
use unmei_engine::decision::client::API_KEY_ENV;
use unmei_engine::DecisionResolver;

use crate::failure::{self, DefensiveHold, Failure, FailureRow, Notice};

/// The environment override for the key. A non-empty value wins over the stored
/// file (ADR 0010); the `unmei` CLI keeps the env-only contract for 0.1.0.
pub const ENV_KEY: &str = API_KEY_ENV;

/// The file's name is the helper's, never a caller's.
const CREDENTIALS_FILE: &str = "credentials";

/// The env note the key row shows while `OPENROUTER_API_KEY` is set
/// (`docs/title-and-menu.md` § Decisions settings §4.1).
pub const ENV_NOTE: &str =
    "OPENROUTER_API_KEY overrides the stored key. Unset it and relaunch to use the stored key.";

/// The shortest value the local shape check accepts; the ping, not this check,
/// decides whether a key works.
pub const MIN_KEY_LENGTH: usize = 16;

/// How many leading characters a fingerprint may show.
pub const FINGERPRINT_PREFIX: usize = 6;
/// How many trailing characters a fingerprint may show.
pub const FINGERPRINT_SUFFIX: usize = 4;

/// The placeholder the empty Not-set field shows
/// (`docs/title-and-menu.md` § Decisions settings §4.1).
pub const KEY_PLACEHOLDER: &str = "Paste or type an OpenRouter key";

/// How long `Copy path` reads `Copied` after an activation.
pub const COPIED_LABEL_SECONDS: f32 = 1.5;

/// Marks the built-in `EditableText` field the Decisions key row owns. There is
/// at most one, and only the Settings view builds it.
#[derive(Component)]
pub struct KeyField;

/// The key row's `Copy path` label countdown. `remaining > 0.0` renders
/// `Copied`; the presentation ticks it back to zero.
#[derive(Resource, Debug, Default)]
pub struct CopyPathLabel {
    pub remaining: f32,
}

// ---------------------------------------------------------------------------
// The clipboard port
// ---------------------------------------------------------------------------

/// The clipboard backend the key row uses. Production installs [`System`] so
/// `Copy path` and paste reach the real OS clipboard through the existing
/// `bevy_clipboard::Clipboard` resource; every test world keeps the default
/// [`Memory`] buffer, so no test ever reads or writes the developer's OS
/// clipboard.
///
/// [`System`]: ClipboardPort::System
/// [`Memory`]: ClipboardPort::Memory
#[derive(Resource, Debug, PartialEq, Eq)]
pub enum ClipboardPort {
    /// An in-process buffer. The default, so a world that never opts in cannot
    /// reach the OS clipboard.
    Memory(Option<String>),
    /// The real system clipboard, through the `Clipboard` resource.
    System,
}

impl Default for ClipboardPort {
    fn default() -> Self {
        ClipboardPort::Memory(None)
    }
}

/// Read text through the port: the memory buffer in tests, `fetch_text` on the
/// OS clipboard in production.
pub fn clipboard_read(port: &ClipboardPort, clipboard: Option<&mut Clipboard>) -> Option<String> {
    match port {
        ClipboardPort::Memory(buffer) => buffer.clone(),
        ClipboardPort::System => clipboard
            .and_then(|clipboard| clipboard.fetch_text().poll_result())
            .and_then(Result::ok),
    }
}

/// Write text through the port: the memory buffer in tests, `set_text` on the
/// OS clipboard in production. A failed write is silent — `Copy path` is a
/// convenience, and no key value is ever involved.
pub fn clipboard_write(port: &mut ClipboardPort, clipboard: Option<&mut Clipboard>, text: &str) {
    match port {
        ClipboardPort::Memory(buffer) => *buffer = Some(text.to_string()),
        ClipboardPort::System => {
            if let Some(clipboard) = clipboard {
                let _ = clipboard.set_text(text);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Paste normalisation
// ---------------------------------------------------------------------------

/// Surrounding **ASCII** whitespace, and only that — a trailing newline a key
/// paste carries is removed before the widget's all-or-nothing filter sees it
/// (`docs/title-and-menu.md` § Decisions settings §4.2).
pub fn trim_ascii_whitespace(text: &str) -> &str {
    text.trim_matches(|character: char| character.is_ascii_whitespace())
}

/// Replace every queued [`TextEdit::Paste`] with an `Insert` of the clipboard
/// text trimmed of surrounding ASCII whitespace. The widget's filter would drop
/// a whole paste for one rejected newline; trimming first means a `…\n` paste
/// survives. An empty result inserts nothing.
pub fn substitute_paste_edits(edits: &mut Vec<TextEdit>, clipboard: Option<&str>) {
    if !edits.iter().any(|edit| matches!(edit, TextEdit::Paste)) {
        return;
    }
    let trimmed = clipboard.map(trim_ascii_whitespace).unwrap_or("");
    let mut replaced = Vec::with_capacity(edits.len());
    for edit in edits.drain(..) {
        match edit {
            TextEdit::Paste => {
                if !trimmed.is_empty() {
                    replaced.push(TextEdit::Insert(trimmed.to_string().into()));
                }
            }
            other => replaced.push(other),
        }
    }
    *edits = replaced;
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// The credential file inside an explicit config directory. The file name is
/// added here, so no caller ever spells it.
pub fn credentials_path_in(config_dir: &Path) -> PathBuf {
    config_dir.join(CREDENTIALS_FILE)
}

/// Which platform's derivation to apply. Parameterized so both branches are
/// testable on one host (the real entry point picks by `cfg!(windows)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPlatform {
    Windows,
    Unix,
}

impl ConfigPlatform {
    /// The platform this build runs on.
    pub fn current() -> Self {
        if cfg!(windows) {
            ConfigPlatform::Windows
        } else {
            ConfigPlatform::Unix
        }
    }
}

/// The environment values the config-directory derivation reads, given rather
/// than fetched so no test mutates the process environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigEnv {
    pub appdata: Option<OsString>,
    pub xdg_config_home: Option<OsString>,
    pub home: Option<OsString>,
}

impl ConfigEnv {
    /// Read the three values from the process environment. Called exactly once,
    /// by [`default_config_dir`] in `main`.
    pub fn from_process() -> Self {
        ConfigEnv {
            appdata: non_empty(std::env::var_os("APPDATA")),
            xdg_config_home: non_empty(std::env::var_os("XDG_CONFIG_HOME")),
            home: non_empty(std::env::var_os("HOME")),
        }
    }
}

/// The pure derivation: Windows `%APPDATA%\unmei`, Unix
/// `$XDG_CONFIG_HOME/unmei` or `$HOME/.config/unmei`. `None` when the platform's
/// base variable is absent (or, for `$XDG_CONFIG_HOME`, not absolute — the XDG
/// spec says a relative value is invalid and must be ignored).
pub fn config_dir_from(platform: ConfigPlatform, env: &ConfigEnv) -> Option<PathBuf> {
    match platform {
        ConfigPlatform::Windows => env
            .appdata
            .as_deref()
            .map(|base| Path::new(base).join("unmei")),
        ConfigPlatform::Unix => {
            if let Some(xdg) = env.xdg_config_home.as_deref() {
                let xdg = Path::new(xdg);
                if xdg.is_absolute() {
                    return Some(xdg.join("unmei"));
                }
            }
            env.home
                .as_deref()
                .map(|home| Path::new(home).join(".config").join("unmei"))
        }
    }
}

/// The real derivation: the process environment's values, this platform's
/// rules. The one place the real `%APPDATA%`/`~/.config` is ever named.
pub fn default_config_dir() -> Option<PathBuf> {
    config_dir_from(ConfigPlatform::current(), &ConfigEnv::from_process())
}

fn non_empty(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

// ---------------------------------------------------------------------------
// Shape + fingerprint
// ---------------------------------------------------------------------------

/// The local, free shape check: after trimming, a key must be non-empty,
/// at least [`MIN_KEY_LENGTH`] characters, and every character ASCII graphic —
/// so no space, newline, control character or non-ASCII byte can hide in it.
/// This rejects obvious non-secrets without a network round-trip; it never
/// claims a key works.
pub fn shape_ok(key: &str) -> bool {
    key.len() >= MIN_KEY_LENGTH && key.bytes().all(|byte| byte.is_ascii_graphic())
}

/// The only rendering of a key anywhere: its first ≤6 characters, `…`, and its
/// last 4. The slices are disjoint, so a short value is truncated rather than
/// repeated and nothing beyond those bounds is ever produced.
pub fn fingerprint(key: &str) -> String {
    let characters: Vec<char> = key.chars().collect();
    let prefix_len = characters.len().min(FINGERPRINT_PREFIX);
    let suffix_len = (characters.len() - prefix_len).min(FINGERPRINT_SUFFIX);
    let prefix: String = characters[..prefix_len].iter().collect();
    let suffix: String = characters[characters.len() - suffix_len..].iter().collect();
    format!("{prefix}\u{2026}{suffix}")
}

// ---------------------------------------------------------------------------
// The row's states
// ---------------------------------------------------------------------------

/// What the key row shows. This is the whole surface: a fingerprint at most,
/// never the key. `Invalid` covers a stored value that fails the shape check and
/// a commit that fails it; `Rejected` is the live ping verdict for a stored key
/// OpenRouter refused (`docs/title-and-menu.md` § Decisions settings §4.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum KeyRow {
    /// No file, a whitespace-only one, or an unreadable one.
    #[default]
    NotSet,
    /// A shape-valid stored value.
    Set { fingerprint: String },
    /// A shape-valid key whose ping could not reach OpenRouter, so it is shown
    /// `not checked`; the verdict is live, not persisted.
    SetUnchecked { fingerprint: String },
    /// A shape-valid key whose ping is in flight (`Checking…`); the field is
    /// disabled until the verdict lands.
    Checking,
    /// Stored content that fails the shape check; the bytes stay on disk.
    Invalid,
    /// A stored, shape-valid key that OpenRouter rejected (`401`/`403`); the
    /// bytes stay on disk and the resolver is left alone.
    Rejected { fingerprint: String },
    /// A non-empty `OPENROUTER_API_KEY`, which wins over the stored value.
    FromEnvironment { fingerprint: String },
}

impl KeyRow {
    /// Whether the environment variable wins over the stored file, so the row is
    /// read-only and `Clear key` is disabled (`docs/title-and-menu.md`
    /// § Decisions settings §4.1).
    pub fn is_from_environment(&self) -> bool {
        matches!(self, KeyRow::FromEnvironment { .. })
    }

    /// Whether a ping is in flight, so the field must not enter edit mode
    /// (`docs/title-and-menu.md` § Decisions settings §4.1).
    pub fn is_checking(&self) -> bool {
        matches!(self, KeyRow::Checking)
    }

    /// The fingerprint a Set-like row carries, if any. Used to keep a live
    /// verdict across a Settings re-open when the stored value has not changed.
    pub fn fingerprint(&self) -> Option<&str> {
        match self {
            KeyRow::Set { fingerprint }
            | KeyRow::SetUnchecked { fingerprint }
            | KeyRow::Rejected { fingerprint }
            | KeyRow::FromEnvironment { fingerprint } => Some(fingerprint),
            KeyRow::NotSet | KeyRow::Checking | KeyRow::Invalid => None,
        }
    }

    /// The state line `docs/title-and-menu.md` § Decisions settings §4.1 fixes.
    pub fn state_line(&self) -> String {
        match self {
            KeyRow::NotSet => "Not set".to_string(),
            KeyRow::Set { fingerprint } => format!("Set \u{2014} {fingerprint}"),
            KeyRow::SetUnchecked { fingerprint } => {
                format!("Set \u{2014} {fingerprint} \u{00b7} not checked (see the log)")
            }
            KeyRow::Checking => "Checking\u{2026}".to_string(),
            KeyRow::Invalid => "Invalid \u{2014} does not look like a key".to_string(),
            KeyRow::Rejected { .. } => "Invalid \u{2014} rejected by OpenRouter".to_string(),
            KeyRow::FromEnvironment { fingerprint } => {
                format!("From environment \u{2014} {fingerprint}")
            }
        }
    }

    /// The row's full text: the state line, plus the env note on a second line
    /// while the variable wins (`docs/title-and-menu.md` § Decisions settings
    /// §4.1).
    pub fn row_text(&self) -> String {
        match self {
            KeyRow::FromEnvironment { .. } => format!("{}\n{ENV_NOTE}", self.state_line()),
            _ => self.state_line(),
        }
    }
}

// ---------------------------------------------------------------------------
// Live state
// ---------------------------------------------------------------------------

/// The read credential state the presentation renders. It never holds the raw
/// key: `path` and `row` are everything a row or a path line needs.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct CredentialsState {
    /// The config directory the path was derived from, kept so a later write or
    /// re-read needs no second derivation.
    pub config_dir: Option<PathBuf>,
    /// `config_dir/credentials`, or `None` when the directory could not be
    /// derived.
    pub path: Option<PathBuf>,
    pub row: KeyRow,
}

/// The `Stored in <path>` line, or `Stored in —` when no directory could be
/// derived.
pub fn path_line(path: Option<&Path>) -> String {
    match path {
        Some(path) => format!("Stored in {}", path.display()),
        None => "Stored in \u{2014}".to_string(),
    }
}

/// The launch read, preloaded by `main` before the app exists so the warning can
/// be emitted by a startup system (after `LogPlugin` installs) rather than
/// before the sinks exist.
#[derive(Resource, Debug, Clone)]
pub struct StartupCredentials {
    pub state: CredentialsState,
    pub warning: Option<String>,
}

/// Read the credential file and apply the environment precedence. Never writes.
///
/// The file is read even when the variable wins (cheap, and it keeps the read
/// moments uniform), but the variable never lets the stored value build a
/// resolver and the row shows the environment state. A file problem is a
/// **silent degradation** — a warning and a state line, the bytes left exactly
/// as they were — and is moot while the variable wins, so no warning fires then.
pub fn read(config_dir: Option<&Path>, env_key: Option<&str>) -> CredentialRead {
    let path = config_dir.map(credentials_path_in);
    let env_key = trimmed_env_key(env_key);

    let file = match config_dir {
        None => FileRead::NoDirectory,
        Some(dir) => read_file(&credentials_path_in(dir)),
    };

    let (row, usable_stored, warning) = match env_key {
        Some(env) => (
            KeyRow::FromEnvironment {
                fingerprint: fingerprint(env),
            },
            None,
            None,
        ),
        None => match file {
            FileRead::Value(value) if shape_ok(&value) => {
                let row = KeyRow::Set {
                    fingerprint: fingerprint(&value),
                };
                (row, Some(value), None)
            }
            FileRead::Value(_) => (
                KeyRow::Invalid,
                None,
                Some(
                    "the stored credential does not look like a key; the bytes were left untouched"
                        .to_string(),
                ),
            ),
            FileRead::Unreadable(detail) => (
                KeyRow::NotSet,
                None,
                Some(format!("could not read the credential file: {detail}")),
            ),
            FileRead::NoDirectory => (
                KeyRow::NotSet,
                None,
                Some(
                    "could not resolve the credential file's directory \u{2014} is APPDATA / XDG_CONFIG_HOME / HOME set?"
                        .to_string(),
                ),
            ),
            FileRead::Absent => (KeyRow::NotSet, None, None),
        },
    };

    CredentialRead {
        state: CredentialsState {
            config_dir: config_dir.map(Path::to_path_buf),
            path,
            row,
        },
        warning,
        usable_stored,
    }
}

/// One read's outcome: the renderable state, the warning to emit once the log
/// exists, and the shape-valid stored value kept only for the resolver builder.
/// Its `Debug` redacts that value, so a debug log can never quote a key.
pub struct CredentialRead {
    pub state: CredentialsState,
    pub warning: Option<String>,
    usable_stored: Option<String>,
}

impl CredentialRead {
    /// The stored value when it is shape-valid **and** no environment key won;
    /// `None` otherwise. The only reader is [`resolve_launch`].
    pub fn usable_stored(&self) -> Option<&str> {
        self.usable_stored.as_deref()
    }
}

impl std::fmt::Debug for CredentialRead {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialRead")
            .field("state", &self.state)
            .field("warning", &self.warning)
            .field(
                "usable_stored",
                &self.usable_stored.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

/// The raw file read: absent, a trimmed value, or an IO failure.
enum FileRead {
    Absent,
    Value(String),
    Unreadable(String),
    /// No config directory could be derived, so there was no path to read.
    NoDirectory,
}

fn read_file(path: &Path) -> FileRead {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            // Surrounding **ASCII** whitespace is trimmed; an internal space or
            // newline survives to fail the shape check.
            let trimmed = text.trim_matches(|character: char| character.is_ascii_whitespace());
            if trimmed.is_empty() {
                FileRead::Absent
            } else {
                FileRead::Value(trimmed.to_string())
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => FileRead::Absent,
        Err(error) => FileRead::Unreadable(format!("`{}`: {error}", path.display())),
    }
}

/// The environment key when it is non-empty after trimming; `None` when it is
/// absent, empty or whitespace-only — an empty variable counts as unset.
fn trimmed_env_key(env_key: Option<&str>) -> Option<&str> {
    env_key.map(str::trim).filter(|key| !key.is_empty())
}

// ---------------------------------------------------------------------------
// Launch resolution
// ---------------------------------------------------------------------------

/// The launch read and the resolver it built. `main` installs `state`/`warning`
/// as [`StartupCredentials`] and `resolver` into the runtime config, so the two
/// cannot diverge.
pub struct LaunchCredentials {
    pub state: CredentialsState,
    pub warning: Option<String>,
    /// The resolver built from the winning key — the environment variable first,
    /// then a shape-valid stored value, then `None` (the offline posture).
    pub resolver: Option<Arc<dyn DecisionResolver>>,
}

/// The launch resolution order: a non-empty variable → a shape-valid stored
/// value → `None`. A stored value that fails the shape check logs once and
/// builds nothing, so a known-bad key cannot produce a 401 per decision.
pub fn resolve_launch<F>(
    config_dir: Option<&Path>,
    env_key: Option<&str>,
    build: F,
) -> LaunchCredentials
where
    F: FnOnce(&str) -> Arc<dyn DecisionResolver>,
{
    let read = read(config_dir, env_key);
    let env_key = trimmed_env_key(env_key);
    let resolver = env_key.or(read.usable_stored()).map(build);
    LaunchCredentials {
        state: read.state,
        warning: read.warning,
        resolver,
    }
}

/// Install the preloaded launch read and funnel its warning (row 24).
///
/// **A host must preload [`StartupCredentials`]** to read the real file; a host
/// that does not (the headless tests, and any future embedder) keeps the default
/// [`CredentialsState`] — `Not set`, no path, **no filesystem or environment
/// access** — so a test can never read the developer's real `%APPDATA%` or
/// `~/.config`, and an un-preloaded embed simply runs key-less.
pub(crate) fn setup_credentials(
    preloaded: Option<Res<StartupCredentials>>,
    mut state: ResMut<CredentialsState>,
    mut notice: ResMut<Notice>,
    mut hold: ResMut<DefensiveHold>,
) {
    let Some(preloaded) = preloaded else {
        return;
    };
    if let Some(detail) = &preloaded.warning {
        failure::report(
            Failure::new(FailureRow::CredentialRead, detail.clone()),
            &mut notice,
            &mut hold,
        );
    }
    *state = preloaded.state.clone();
}

// ---------------------------------------------------------------------------
// The atomic write
// ---------------------------------------------------------------------------

/// One atomic write: a stale `credentials.tmp` is removed best-effort, a fresh
/// temp is created `0600` (Unix) with `create_new`, and the rename replaces the
/// target. The `unmei/` directory is created `0700` (Unix) when it is missing.
/// On any failure the temp is removed best-effort, so no partial credential file
/// is ever left behind.
pub fn write(config_dir: &Path, key: &str) -> Result<(), CredentialError> {
    let path = credentials_path_in(config_dir);
    let temp = temp_path(&path);

    create_dir_private(config_dir)
        .map_err(|error| CredentialError::new(&config_dir.display().to_string(), error))?;

    // A leftover from a crashed write must never block this one (or smuggle in
    // its pre-created mode).
    let _ = std::fs::remove_file(&temp);

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temp)
        .map_err(|error| CredentialError::new(&temp.display().to_string(), error))?;

    if let Err(error) = file.write_all(key.as_bytes()).and_then(|()| file.flush()) {
        drop(file);
        let _ = std::fs::remove_file(&temp);
        return Err(CredentialError::new(&temp.display().to_string(), error));
    }
    drop(file);

    if let Err(error) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(CredentialError::new(&path.display().to_string(), error));
    }
    Ok(())
}

/// Store a key and funnel the failure, returning whether the file landed. A
/// successful write touches only the file; the row's transition (Set /
/// Checking… / Invalid) belongs to the commit flow, which reads this result. A
/// failed write reports row 23 — `error!` and the Notice `Could not save the
/// key` — and leaves the state exactly as it was. No ping runs here.
pub fn store(
    state: &CredentialsState,
    key: &str,
    notice: &mut Notice,
    hold: &mut DefensiveHold,
) -> bool {
    let Some(config_dir) = state.config_dir.as_deref() else {
        failure::report(
            Failure::new(
                FailureRow::CredentialsWrite,
                "the credential file's directory could not be resolved",
            ),
            notice,
            hold,
        );
        return false;
    };
    if let Err(error) = write(config_dir, key) {
        failure::report(
            Failure::new(FailureRow::CredentialsWrite, error.to_string()),
            notice,
            hold,
        );
        return false;
    }
    true
}

/// Delete the credential file. A missing file is a no-op; any other IO error is
/// returned for the caller to decide on. The directory is left in place.
pub fn clear(config_dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(credentials_path_in(config_dir)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// The temp sibling the atomic write uses: `credentials.tmp` beside the target.
fn temp_path(path: &Path) -> PathBuf {
    path.with_extension("tmp")
}

fn create_dir_private(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// A credential write failure, rendered with the path the IO reached — never the
/// value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialError(String);

impl CredentialError {
    fn new(path: &str, error: std::io::Error) -> Self {
        CredentialError(format!("`{path}`: {error}"))
    }
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CredentialError {}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;
    use unmei_engine::{DecisionConfig, ResolvedDecision, StateValue};

    fn temp_root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    // -- paths ---------------------------------------------------------------

    #[test]
    fn the_file_name_is_the_helpers_never_a_callers() {
        let dir = temp_root();
        assert_eq!(
            credentials_path_in(dir.path()),
            dir.path().join("credentials")
        );
    }

    #[test]
    fn windows_derives_appdata_and_unix_prefers_xdg_then_home() {
        let env = ConfigEnv {
            appdata: Some(OsString::from(r"C:\Users\player\AppData\Roaming")),
            xdg_config_home: Some(OsString::from("/home/player/.config")),
            home: Some(OsString::from("/home/player")),
        };
        assert_eq!(
            config_dir_from(ConfigPlatform::Windows, &env),
            Some(PathBuf::from(r"C:\Users\player\AppData\Roaming").join("unmei"))
        );
        assert_eq!(
            config_dir_from(ConfigPlatform::Unix, &env),
            Some(PathBuf::from("/home/player/.config").join("unmei"))
        );

        let home_only = ConfigEnv {
            appdata: None,
            xdg_config_home: None,
            home: Some(OsString::from("/home/player")),
        };
        assert_eq!(
            config_dir_from(ConfigPlatform::Unix, &home_only),
            Some(PathBuf::from("/home/player/.config").join("unmei"))
        );
    }

    #[test]
    fn a_missing_base_variable_or_a_relative_xdg_derives_nothing() {
        let empty = ConfigEnv::default();
        assert_eq!(config_dir_from(ConfigPlatform::Windows, &empty), None);
        assert_eq!(config_dir_from(ConfigPlatform::Unix, &empty), None);

        let relative_xdg = ConfigEnv {
            appdata: None,
            xdg_config_home: Some(OsString::from("relative/config")),
            home: Some(OsString::from("/home/player")),
        };
        assert_eq!(
            config_dir_from(ConfigPlatform::Unix, &relative_xdg),
            Some(PathBuf::from("/home/player/.config").join("unmei")),
            "a relative XDG value is invalid and falls back to $HOME"
        );
    }

    // -- shape + fingerprint -------------------------------------------------

    #[test]
    fn the_shape_check_follows_the_settled_table() {
        assert!(!shape_ok(""), "empty rejects");
        assert!(!shape_ok("   "), "whitespace rejects");
        assert!(
            shape_ok(&format!("sk-or-v1-{}", "a".repeat(60))),
            "an OpenRouter-shaped key accepts"
        );
        assert!(!shape_ok("sk-or-v1-abc def"), "an internal space rejects");
        assert!(
            !shape_ok("sk-or-v1-abc\ndef"),
            "an internal newline rejects"
        );
        assert!(!shape_ok("fifteen-char-ok"), "a 15-character value rejects");
        assert!(
            !shape_ok("sk-or-v1-\u{00e9}\u{00e9}\u{00e9}"),
            "non-ASCII rejects"
        );
        assert!(
            shape_ok(&"p".repeat(400)),
            "a 400-character printable value accepts; the ping decides"
        );
    }

    #[test]
    fn the_fingerprint_shows_at_most_six_then_four() {
        let key = "sk-or-v1-0123456789abcdef";
        let shown = fingerprint(key);
        assert_eq!(shown, "sk-or-\u{2026}cdef");
        assert!(!shown.contains("v1-0123456789ab"));
        assert_eq!(fingerprint(key).len(), 6 + "\u{2026}".len() + 4);
    }

    #[test]
    fn a_short_value_fingerprints_without_panicking() {
        assert_eq!(fingerprint("short"), "short\u{2026}");
        assert_eq!(fingerprint("abcdefgh"), "abcdef\u{2026}gh");
        assert_eq!(fingerprint("0123456789"), "012345\u{2026}6789");
        assert_eq!(fingerprint(""), "\u{2026}");
    }

    #[test]
    fn the_fingerprint_slices_on_character_boundaries() {
        // Five multibyte characters: the prefix is the whole value, no suffix —
        // a byte slice here would panic mid-codepoint.
        assert_eq!(
            fingerprint("\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}"),
            "\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{2026}"
        );
        assert_eq!(
            fingerprint("\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}"),
            "\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{00e9}\u{2026}\u{00e9}\u{00e9}\u{00e9}\u{00e9}"
        );
    }

    // -- row state lines -----------------------------------------------------

    #[test]
    fn the_row_state_lines_follow_the_settled_table() {
        let fingerprint = fingerprint("sk-or-v1-0123456789abcdef");
        assert_eq!(KeyRow::NotSet.state_line(), "Not set");
        assert_eq!(
            KeyRow::Set {
                fingerprint: fingerprint.clone(),
            }
            .state_line(),
            format!("Set \u{2014} {fingerprint}")
        );
        assert_eq!(
            KeyRow::SetUnchecked {
                fingerprint: fingerprint.clone(),
            }
            .state_line(),
            format!("Set \u{2014} {fingerprint} \u{00b7} not checked (see the log)")
        );
        assert_eq!(KeyRow::Checking.state_line(), "Checking\u{2026}");
        assert_eq!(
            KeyRow::Invalid.state_line(),
            "Invalid \u{2014} does not look like a key"
        );
        assert_eq!(
            KeyRow::Rejected { fingerprint }.state_line(),
            "Invalid \u{2014} rejected by OpenRouter"
        );
    }

    #[test]
    fn the_fingerprint_helper_reads_only_set_like_rows() {
        let fingerprint = fingerprint("sk-or-v1-0123456789abcdef");
        assert_eq!(
            KeyRow::Set {
                fingerprint: fingerprint.clone(),
            }
            .fingerprint(),
            Some(fingerprint.as_str())
        );
        assert_eq!(
            KeyRow::SetUnchecked {
                fingerprint: fingerprint.clone(),
            }
            .fingerprint(),
            Some(fingerprint.as_str())
        );
        assert_eq!(
            KeyRow::Rejected {
                fingerprint: fingerprint.clone(),
            }
            .fingerprint(),
            Some(fingerprint.as_str())
        );
        assert!(KeyRow::NotSet.fingerprint().is_none());
        assert!(KeyRow::Checking.fingerprint().is_none());
        assert!(KeyRow::Invalid.fingerprint().is_none());
    }

    // -- paste normalisation ------------------------------------------------

    #[test]
    fn the_paste_trim_removes_only_surrounding_ascii_whitespace() {
        assert_eq!(trim_ascii_whitespace("  key\n"), "key");
        assert_eq!(trim_ascii_whitespace("\tkey\r\n"), "key");
        assert_eq!(
            trim_ascii_whitespace("key\nmore"),
            "key\nmore",
            "an internal newline survives to be rejected by the filter"
        );
        assert_eq!(trim_ascii_whitespace("key"), "key");
        assert_eq!(trim_ascii_whitespace(""), "");
    }

    #[test]
    fn a_paste_is_replaced_by_the_trimmed_insert_and_an_empty_one_inserts_nothing() {
        let mut edits = vec![TextEdit::Paste];
        substitute_paste_edits(&mut edits, Some("  sk-or-v1-abc\n"));
        assert_eq!(edits, vec![TextEdit::Insert("sk-or-v1-abc".into())]);

        let mut edits = vec![TextEdit::Paste];
        substitute_paste_edits(&mut edits, Some("   \n\t"));
        assert!(edits.is_empty(), "an all-whitespace paste inserts nothing");

        let mut edits = vec![
            TextEdit::Insert("x".into()),
            TextEdit::Paste,
            TextEdit::Backspace,
        ];
        substitute_paste_edits(&mut edits, Some("mid"));
        assert_eq!(
            edits,
            vec![
                TextEdit::Insert("x".into()),
                TextEdit::Insert("mid".into()),
                TextEdit::Backspace,
            ]
        );

        let mut edits = vec![TextEdit::Insert("x".into())];
        substitute_paste_edits(&mut edits, Some("ignored"));
        assert_eq!(
            edits,
            vec![TextEdit::Insert("x".into())],
            "an edit list with no paste is left alone"
        );
    }

    // -- read posture --------------------------------------------------------

    fn config_dir_with(dir: &Path) -> PathBuf {
        let config = dir.join("unmei");
        std::fs::create_dir_all(&config).unwrap();
        config
    }

    #[test]
    fn a_missing_or_whitespace_only_file_is_not_set_and_writes_nothing() {
        for contents in [None, Some("   \n\t\n"), Some("")] {
            let root = temp_root();
            let config = config_dir_with(root.path());
            if let Some(contents) = contents {
                std::fs::write(credentials_path_in(&config), contents).unwrap();
            }
            let before = std::fs::read_dir(&config).unwrap().count();

            let read = read(Some(&config), None);
            assert_eq!(read.state.row, KeyRow::NotSet, "{contents:?}");
            assert!(read.warning.is_none(), "{contents:?} must not warn");
            assert!(read.usable_stored().is_none());
            assert_eq!(
                std::fs::read_dir(&config).unwrap().count(),
                before,
                "a read creates nothing"
            );
        }
    }

    #[test]
    fn a_trailing_newline_and_surrounding_whitespace_are_trimmed() {
        let root = temp_root();
        let config = config_dir_with(root.path());
        let key = format!("sk-or-v1-{}", "a".repeat(60));
        std::fs::write(credentials_path_in(&config), format!("  {key}\n\n")).unwrap();

        let read = read(Some(&config), None);
        assert_eq!(
            read.state.row,
            KeyRow::Set {
                fingerprint: fingerprint(&key)
            }
        );
        assert_eq!(read.usable_stored(), Some(key.as_str()));
    }

    #[test]
    fn an_internal_space_or_newline_fails_the_shape_check_and_keeps_the_bytes() {
        for value in ["sk-or-v1-abc def", "sk-or-v1-abc\ndef"] {
            let root = temp_root();
            let config = config_dir_with(root.path());
            let bytes = format!("{value}\n");
            std::fs::write(credentials_path_in(&config), &bytes).unwrap();

            let read = read(Some(&config), None);
            assert_eq!(read.state.row, KeyRow::Invalid, "{value:?}");
            assert!(read.warning.is_some(), "{value:?} warns");
            assert!(read.usable_stored().is_none());
            assert_eq!(
                std::fs::read_to_string(credentials_path_in(&config)).unwrap(),
                bytes,
                "the bytes are untouched"
            );
        }
    }

    #[test]
    fn the_warning_never_quotes_the_value() {
        let root = temp_root();
        let config = config_dir_with(root.path());
        let secret = "sk-or-v1-this-is-a-secret-with-a space";
        std::fs::write(credentials_path_in(&config), secret).unwrap();

        let read = read(Some(&config), None);
        let warning = read.warning.expect("a shape failure warns");
        assert!(!warning.contains("secret"), "{warning}");
        assert!(!warning.contains("sk-or"), "{warning}");
    }

    #[test]
    fn an_unreadable_file_is_not_set_warns_and_leaves_the_bytes() {
        let root = temp_root();
        let config = config_dir_with(root.path());
        // A directory where the file should be: every platform's read fails.
        std::fs::create_dir(credentials_path_in(&config)).unwrap();

        let read = read(Some(&config), None);
        assert_eq!(read.state.row, KeyRow::NotSet);
        let warning = read.warning.expect("an IO failure warns");
        assert!(warning.contains("credentials"), "{warning}");
        assert!(
            credentials_path_in(&config).is_dir(),
            "the bytes (here a directory) are untouched"
        );
    }

    #[test]
    fn a_missing_config_directory_warns_and_is_not_set() {
        let read = read(None, None);
        assert_eq!(read.state.row, KeyRow::NotSet);
        assert!(read.warning.is_some(), "an underivable directory degrades");
    }

    #[test]
    fn an_environment_key_wins_reads_the_file_and_leaves_the_stored_bytes() {
        let root = temp_root();
        let config = config_dir_with(root.path());
        let stored = format!("sk-or-v1-{}", "b".repeat(60));
        std::fs::write(credentials_path_in(&config), &stored).unwrap();
        let bytes = std::fs::read(credentials_path_in(&config)).unwrap();

        let env = "sk-or-v1-0123456789abcdef";
        let read = read(Some(&config), Some(env));
        assert_eq!(
            read.state.row,
            KeyRow::FromEnvironment {
                fingerprint: fingerprint(env)
            }
        );
        assert!(
            read.usable_stored().is_none(),
            "no resolver may be built from the file while the variable wins"
        );
        assert_eq!(
            std::fs::read(credentials_path_in(&config)).unwrap(),
            bytes,
            "the stored bytes are untouched"
        );
    }

    #[test]
    fn an_empty_or_whitespace_variable_is_unset_and_the_stored_key_is_used() {
        let root = temp_root();
        let config = config_dir_with(root.path());
        let stored = format!("sk-or-v1-{}", "c".repeat(60));
        std::fs::write(credentials_path_in(&config), &stored).unwrap();

        for env in ["", "   ", "\n"] {
            let read = read(Some(&config), Some(env));
            assert_eq!(
                read.state.row,
                KeyRow::Set {
                    fingerprint: fingerprint(&stored)
                },
                "{env:?} is unset"
            );
            assert_eq!(read.usable_stored(), Some(stored.as_str()));
        }
    }

    // -- launch resolution ---------------------------------------------------

    struct FakeResolver;

    impl DecisionResolver for FakeResolver {
        fn resolve<'a>(
            &'a self,
            _model: &'a str,
            _question_id: &'a str,
            _config: &'a DecisionConfig,
            _state: &'a HashMap<String, StateValue>,
        ) -> Pin<Box<dyn Future<Output = ResolvedDecision> + Send + 'a>> {
            Box::pin(std::future::ready(ResolvedDecision {
                action: "fake".to_string(),
                model: None,
                confidence: None,
                fallback_reason: None,
            }))
        }
    }

    #[test]
    fn launch_resolution_is_env_then_stored_then_none() {
        let root = temp_root();
        let config = config_dir_with(root.path());

        // Nothing anywhere: no resolver.
        let used = std::cell::RefCell::new(Vec::new());
        let launch = resolve_launch(Some(&config), None, |key| {
            used.borrow_mut().push(key.to_string());
            Arc::new(FakeResolver) as Arc<dyn DecisionResolver>
        });
        assert!(launch.resolver.is_none());
        assert!(used.borrow().is_empty());

        // A shape-valid stored value builds one.
        let stored = format!("sk-or-v1-{}", "d".repeat(60));
        write(&config, &stored).unwrap();
        let launch = resolve_launch(Some(&config), None, |key| {
            used.borrow_mut().push(key.to_string());
            Arc::new(FakeResolver) as Arc<dyn DecisionResolver>
        });
        assert!(launch.resolver.is_some());
        assert_eq!(used.borrow().as_slice(), [stored.as_str()]);

        // A non-empty variable wins over the stored value.
        let env = "sk-or-v1-env-wins-over-stored-0000";
        let launch = resolve_launch(Some(&config), Some(env), |key| {
            used.borrow_mut().push(key.to_string());
            Arc::new(FakeResolver) as Arc<dyn DecisionResolver>
        });
        assert!(launch.resolver.is_some());
        assert_eq!(used.borrow().last().map(String::as_str), Some(env));
    }

    #[test]
    fn a_shape_invalid_stored_value_logs_once_and_builds_none() {
        let root = temp_root();
        let config = config_dir_with(root.path());
        std::fs::write(credentials_path_in(&config), "not a real key").unwrap();

        let mut built = 0;
        let launch = resolve_launch(Some(&config), None, |_key| {
            built += 1;
            Arc::new(FakeResolver) as Arc<dyn DecisionResolver>
        });
        assert!(launch.resolver.is_none());
        assert_eq!(built, 0, "a known-bad key never builds a resolver");
        assert!(
            launch.warning.is_some(),
            "the shape failure is the one logged warning"
        );
    }

    // -- the write -----------------------------------------------------------

    #[test]
    fn a_successful_write_creates_the_file_atomically_and_leaves_no_temp() {
        let root = temp_root();
        let config = root.path().join("unmei");
        let key = format!("sk-or-v1-{}", "e".repeat(60));

        write(&config, &key).unwrap();

        let path = credentials_path_in(&config);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), key);
        assert!(!temp_path(&path).exists(), "credentials.tmp is gone");
    }

    #[test]
    fn a_stale_temp_is_removed_before_the_new_write() {
        let root = temp_root();
        let config = config_dir_with(root.path());
        let path = credentials_path_in(&config);
        std::fs::write(temp_path(&path), "stale leftover").unwrap();

        let key = format!("sk-or-v1-{}", "f".repeat(60));
        write(&config, &key).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), key);
        assert!(!temp_path(&path).exists());
    }

    #[test]
    fn a_failed_rename_removes_the_temp_and_leaves_no_partial_file() {
        let root = temp_root();
        let config = config_dir_with(root.path());
        let path = credentials_path_in(&config);
        // The target is a directory, so the rename cannot land.
        std::fs::create_dir(&path).unwrap();

        let error = write(&config, &format!("sk-or-v1-{}", "g".repeat(60))).unwrap_err();
        assert!(error.to_string().contains("credentials"), "{error}");
        assert!(
            !temp_path(&path).exists(),
            "the temp is removed best-effort"
        );
    }

    #[test]
    fn a_failed_write_leaves_no_partial_file_and_keeps_the_row() {
        let root = temp_root();
        let config = config_dir_with(root.path());
        let path = credentials_path_in(&config);
        // The temp path is a directory, so the create fails before any bytes.
        std::fs::create_dir(temp_path(&path)).unwrap();

        let state = CredentialsState {
            config_dir: Some(config.clone()),
            path: Some(path.clone()),
            row: KeyRow::NotSet,
        };
        let mut notice = Notice::default();
        let mut hold = DefensiveHold::default();

        store(
            &state,
            &format!("sk-or-v1-{}", "h".repeat(60)),
            &mut notice,
            &mut hold,
        );

        assert!(!path.exists(), "no partial credential file");
        assert_eq!(state.row, KeyRow::NotSet, "the row is left as it was");
        assert_eq!(state.path, Some(path), "and so is the path");
        assert!(notice.is_showing());
        assert_eq!(notice.text.as_deref(), Some("Could not save the key"));
        assert!(!hold.is_held());
    }

    #[test]
    fn a_store_success_lands_the_file_and_leaves_the_row_to_the_commit_flow() {
        let root = temp_root();
        let config = root.path().join("unmei");
        let key = format!("sk-or-v1-{}", "z".repeat(60));
        let state = CredentialsState {
            config_dir: Some(config.clone()),
            path: None,
            row: KeyRow::NotSet,
        };
        let mut notice = Notice::default();
        let mut hold = DefensiveHold::default();

        store(&state, &key, &mut notice, &mut hold);

        assert!(!notice.is_showing(), "a success notices nothing");
        assert_eq!(
            std::fs::read_to_string(credentials_path_in(&config)).unwrap(),
            key
        );
        assert_eq!(
            state.row,
            KeyRow::NotSet,
            "the row transition is the commit flow's"
        );
    }

    #[test]
    fn a_store_without_a_directory_reports_the_write_row() {
        let state = CredentialsState::default();
        let mut notice = Notice::default();
        let mut hold = DefensiveHold::default();

        store(&state, "sk-or-v1-0123456789abcdef", &mut notice, &mut hold);

        assert!(notice.is_showing());
        assert_eq!(notice.text.as_deref(), Some("Could not save the key"));
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_directory_fails_the_write_and_leaves_nothing() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root();
        let config = config_dir_with(root.path());
        let path = credentials_path_in(&config);
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o500)).unwrap();

        let result = write(&config, &format!("sk-or-v1-{}", "j".repeat(60)));
        // Restore so the temp dir can be cleaned up.
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(result.is_err(), "a read-only directory fails the write");
        assert!(!path.exists(), "no partial file");
        assert!(!temp_path(&path).exists(), "no temp left behind");
    }

    #[cfg(unix)]
    #[test]
    fn unix_modes_are_private_from_the_first_byte() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root();
        let config = root.path().join("unmei");
        write(&config, &format!("sk-or-v1-{}", "i".repeat(60))).unwrap();

        let dir_mode = std::fs::metadata(&config).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "the unmei/ directory is private");
        let file_mode = std::fs::metadata(credentials_path_in(&config))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600, "the file is private");
    }
}
