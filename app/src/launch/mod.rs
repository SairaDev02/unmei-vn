//! The `unmei-play` launch grammar.
//!
//! This is the binary's argument contract, kept in the library so it can be
//! tested without starting an app: one optional positional `PACK_DIR`, the
//! exact `--dev` force-on flag before `--`, `-h`/`--help`, `-V`/`--version`, and
//! no other option. [`parse`] never touches the filesystem, the executable, the
//! credentials or a logger, so a help, version or usage exit happens before any
//! startup work, and an absent positional stays absent.
//!
//! The default pack is a separate step ([`resolve_pack`]) because it needs one
//! OS fact — the folder the executable lives in ([`executable_dir`]) — which the
//! parser must not ask for. Resolution takes that folder as a parameter so the
//! rule is testable without depending on where the test binary runs from.
//!
//! The rest of the launch policy — what dev mode enables and where the value is
//! consumed — lives in [`crate::RuntimeConfig`] and the dev-mode gate. This
//! module only resolves the value and the pack path.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

mod check;
mod keiko;
mod pack;
mod preview;

use check::CheckArgs;
pub use keiko::{validate_id, Format, KeikoMode, DEFAULT_BUDGET};
pub use pack::{
    executable_dir, resolve_pack, ExecutableUnknown, PackOrigin, ResolvedPack, DEFAULT_PACK_NAME,
};
pub use preview::PreviewArgs;

/// The release version, from the crate's manifest (`[workspace.package]`).
/// Read, never restated, so a release bump touches `Cargo.toml` only.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What `-V`/`--version` prints and what the session log's first line names.
pub fn version_text() -> String {
    format!("unmei-play {VERSION}")
}

/// The one text for "this executable cannot find itself", printed to stderr and
/// drawn on the startup error screen from this single string so the two cannot
/// drift.
pub const NO_EXECUTABLE_MESSAGE: &str = "unmei-play could not locate its own executable, so demo-pack beside it cannot be found — pass PACK_DIR.";

/// A syntactically valid launch: the positional `PACK_DIR` exactly as given
/// (absent stays absent) and whether exact `--dev` requested the force-on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// The positional `PACK_DIR`, or `None` when the default rule applies. The
    /// default needs the executable's folder, so it is resolved by
    /// [`resolve_pack`](pack::resolve_pack) rather than here.
    pub pack_dir: Option<PathBuf>,
    /// Whether exact `--dev` was present — the force-on request. The effective
    /// value also folds in the build default; see [`Launch::dev_mode`].
    pub force_dev: bool,
    /// The Keiko testing mode, when one was asked for. `None` is the ordinary
    /// player launch.
    pub keiko: Option<KeikoMode>,
    /// Whether `--check` was asked for: report the pack's findings and exit.
    pub check: bool,
    /// Keep every take's sandbox instead of deleting it.
    pub keep: bool,
    /// The Keiko output format. Text is the default for every mode.
    pub format: Format,
}

impl Launch {
    /// The one effective dev-mode value the binary installs: the `--dev`
    /// force-on request, or else the compiler's debug-assertions default. This
    /// is `cfg!(debug_assertions)` — on in a normal debug/test build, off in a
    /// normal release build — never an optimization level or debug symbols.
    pub fn dev_mode(&self) -> bool {
        self.force_dev || cfg!(debug_assertions)
    }

    /// The exit this launch takes when no `PACK_DIR` was given and the
    /// executable's folder is unknown, so the default pack cannot be derived.
    /// Only play may fall back to the error screen: `--check`, `--preview` and
    /// the Keiko modes build no app, so they never open a window.
    pub fn no_pack_exit(&self) -> NoPackExit {
        if self.check || self.keiko.is_some() {
            NoPackExit::Usage
        } else {
            NoPackExit::ErrorScreen
        }
    }
}

/// Which exit a launch takes when there is no pack at all: no `PACK_DIR` was
/// given and the executable cannot find its own folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoPackExit {
    /// Play: the one error screen that exists before any pack is known.
    ErrorScreen,
    /// A mode — `--check`, `--preview` or a Keiko mode: print
    /// [`NO_EXECUTABLE_MESSAGE`] and exit 2, because a mode builds no app.
    Usage,
}

/// What the binary should do with the parsed arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchRequest {
    /// Run the app with these settings.
    Run(Launch),
    /// Print [`help`] to stdout and exit 0.
    Help,
    /// Print [`version_text`] to stdout and exit 0.
    Version,
}

/// An invalid launch. The binary prints this plus [`USAGE`] to stderr and exits
/// 2, before any startup work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError {
    message: String,
}

impl UsageError {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "unmei-play: {}", self.message)
    }
}

impl std::error::Error for UsageError {}

/// Parse the arguments that follow the program name.
///
/// The whole grammar is validated before help or version is honoured, so an
/// invalid invocation is a usage error even when `-h`/`--help` or `-V` was also
/// present.
pub fn parse<I>(args: I) -> Result<LaunchRequest, UsageError>
where
    I: IntoIterator<Item = OsString>,
{
    let tokens: Vec<OsString> = args.into_iter().collect();
    let mut dev = false;
    let mut help = false;
    let mut version = false;
    let mut positional: Option<OsString> = None;
    let mut options_ended = false;
    let mut keiko = keiko::KeikoArgs::default();
    let mut check = CheckArgs::default();

    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        index += 1;

        // Before `--`, an exact `--` ends option parsing. After it, `--` is a
        // path token like any other.
        if !options_ended && token == OsStr::new("--") {
            options_ended = true;
            continue;
        }

        if !options_ended {
            let text = token.to_string_lossy();
            match text.as_ref() {
                "--dev" => {
                    dev = true;
                    continue;
                }
                "-h" | "--help" => {
                    help = true;
                    continue;
                }
                "-V" | "--version" => {
                    version = true;
                    continue;
                }
                "--check" => {
                    check.request();
                    continue;
                }
                "--take" => {
                    keiko.take()?;
                    continue;
                }
                "--step" => {
                    keiko.step()?;
                    continue;
                }
                "--observe" => {
                    keiko.observe()?;
                    continue;
                }
                "--explore" => {
                    keiko.explore()?;
                    continue;
                }
                "--live" => {
                    keiko.live()?;
                    continue;
                }
                "--keep" => {
                    keiko.keep();
                    continue;
                }
                "--menus" => {
                    keiko.menus();
                    continue;
                }
                "--format" | "--budget" | "--from" => {
                    let value = tokens
                        .get(index)
                        .ok_or_else(|| UsageError::new(format!("{text} needs a value after it")))?;
                    index += 1;
                    let value = value.to_string_lossy();
                    match text.as_ref() {
                        "--format" => keiko.set_format(&value)?,
                        "--budget" => keiko.set_budget(&value)?,
                        _ => keiko.set_from(&value)?,
                    }
                    continue;
                }
                // The preview's folder and state file are paths, so they keep
                // the operating system's own encoding.
                "--preview" | "--state" => {
                    let value = tokens
                        .get(index)
                        .ok_or_else(|| UsageError::new(format!("{text} needs a value after it")))?;
                    index += 1;
                    let path = PathBuf::from(value);
                    match text.as_ref() {
                        "--preview" => keiko.preview(path)?,
                        _ => keiko.set_state(path),
                    }
                    continue;
                }
                "--scene" | "--at" | "--pick" => {
                    let value = tokens
                        .get(index)
                        .ok_or_else(|| UsageError::new(format!("{text} needs a value after it")))?;
                    index += 1;
                    let value = value.to_string_lossy();
                    match text.as_ref() {
                        "--scene" => keiko.set_scene(&value)?,
                        "--at" => keiko.set_at(&value)?,
                        _ => keiko.set_picks(&value)?,
                    }
                    continue;
                }
                // Any other leading-dash token is an unknown option, including
                // `--dev=false`, `--pack` and `--no-dev`. A non-Unicode path is
                // not rejected for its encoding: only the leading dash is a
                // shape.
                other if other.starts_with('-') => {
                    return Err(UsageError::new(format!(
                        "unrecognized option '{}'",
                        token.to_string_lossy()
                    )));
                }
                _ => {}
            }
        }

        // A bare token is the open mode's operand when it is waiting for one;
        // otherwise it is the positional `PACK_DIR`. `--take` keeps taking ids
        // until the next option, so its ids must follow it (spec §1).
        match keiko.bare_target() {
            keiko::Bare::TakeId | keiko::Bare::Value => {
                keiko.push_value(token.clone())?;
                continue;
            }
            keiko::Bare::None => {}
        }

        if token.is_empty() {
            return Err(UsageError::new("PACK_DIR must not be empty"));
        }
        if positional.is_some() {
            return Err(UsageError::new(format!(
                "unexpected extra argument '{}'",
                token.to_string_lossy()
            )));
        }
        positional = Some(token.clone());
    }

    // The settled precedence: usage errors beat both exits (they returned
    // above), and help beats version, so `-V --help` prints help.
    if help {
        return Ok(LaunchRequest::Help);
    }
    if version {
        return Ok(LaunchRequest::Version);
    }

    let keiko_mode = keiko.clone().finish(dev, check.requested())?;
    let check = check.finish(dev, keiko_mode.is_some())?;
    Ok(LaunchRequest::Run(Launch {
        pack_dir: positional.map(PathBuf::from),
        force_dev: dev,
        keiko: keiko_mode,
        check,
        keep: keiko.keeping(),
        format: keiko.output_format(),
    }))
}

/// The concise usage block the binary prints to stderr alongside a diagnostic.
pub const USAGE: &str = "\
Usage: unmei-play [--dev] [PACK_DIR]
       unmei-play [PACK_DIR] --check [--format text|json]
       unmei-play [PACK_DIR] --preview DIR [--scene ID | --at SCENE:LINE]
                          [--state FILE] [--pick N,N,...] [--format text|json]
       unmei-play --help
       unmei-play --version
";

/// The developer-only help printed to stdout for `-h`/`--help`.
///
/// It covers the beside-the-executable default, `--` escaping, and the marked
/// developer section: build defaults, `--dev`, the F5/F9 manual-slot-1 contract,
/// and the live F3 Monitor drawer.
pub fn help() -> String {
    format!(
        "\
unmei-play — play an Unmei content pack.

{usage}
Arguments:
  PACK_DIR        Content pack directory. Defaults to {default} in the
                  folder containing the executable.

Options:
  --dev           Force developer affordances on, even in a release build.
  -h, --help      Print this help and exit. Repeats are idempotent.
  -V, --version   Print the version and exit. Repeats are idempotent.

Check:
  --check         Report every load finding and every Keiko case finding in the
                  pack without running a take. It opens no window and never
                  touches the pack's sidecars or the user's credential file.
  --format F      With --check: `text` (the default) or `json`.
  Exit codes: 0 no error finding, 1 an error finding, 2 usage.

Preview:
  --preview DIR   Draw the pack on the software rasterizer, one 1280x720 PNG per
                  frame, into DIR. It opens no window, needs no GPU or key, and
                  never reads or writes the pack's sidecars or the user's
                  credential file. DIR is created when missing; a DIR inside the
                  pack folder is a usage error. Exit codes: 0 frames written,
                  1 a finding or no rasterizer, 2 usage.
  --scene ID      With --preview: one frame at every park the scene meets from
                  its first line until it leaves the scene.
  --at SCENE:LINE With --preview: one frame at the park that line produces. The
                  line must be a say-line, a choice or a decide.
  --state FILE    With --preview: a RON map in state.ron's form, merged over the
                  pack's initial state.
  --pick N,N,...  With --preview: the 1-based option to take at each successive
                  choice; past the end of the list, option 1.

Testing (Keiko):
  --take [ID...]  Run the pack's cases (`tests/<id>.keiko`), or every case
                  under `tests/` when no id is given. Ids follow `--take`.
  --step ID STEP  Replay a case, apply one step line and append it only if it
                  passed.
  --observe ID    Replay a case and print what the player sees; never writes.
  --explore       Walk every choice option and landing key within a budget and
                  save each finding as a case under `tests/findings/`.
  --live          With --take only: send unset decisions to NanoGPT. Needs
                  NANOGPT_API_KEY; results are marked nondeterministic.
  --budget N      With --explore only: the step budget (default 10000).
  --from ID       With --explore only: start from the end of an existing case.
  --menus         With --explore only: walk the menus instead of the story,
                  pressing every walk key, Enter and Esc from each focus.
  --keep          Keep each take's sandbox and print its path.
  --format F      `text` (the default) or `json`.
  A case sets dev mode itself, so `--dev` cannot be combined with a Keiko mode.
  Exit codes: 0 pass, 1 failure or findings, 2 usage.

A literal `--` ends option parsing: every later token is a path, so
`unmei-play -- --dev` plays a pack named `--dev`, and `unmei-play -- -h` names a
pack `-h`. `unmei-play --` alone uses the default pack. The runtime has no
`--pack` and no `--no-dev`.

Developer controls (not part of the player interface):
  One app-lifetime dev mode gates the developer affordances. It defaults on when
  the build enables debug assertions (`cfg!(debug_assertions)` — the normal
  debug and test build) and off otherwise (the normal release build). Exactly
  `--dev`, before or after PACK_DIR and repeatable, forces it on in any build.
  It is resolved before the logger and runtime start and never changes or
  persists during the session.
  F5 immediately saves the whole position to manual slot 1, overwriting an
  occupied or unreadable slot without confirmation. F9 loads that slot; a
  missing slot is a clean no-op. While dev mode is off both keys are silent
  no-ops.
  Monitor: while dev mode is on, the runtime retains a bounded, escaped
  in-memory copy of admitted diagnostics (startup, pack and dependency events)
  for the whole launch, independently of the session log. F3 opens the bottom
  drawer over any runtime surface and toggles it closed; Esc or the drawer's
  Close control dismiss only Monitor. While open it owns the window's app
  keyboard and pointer input — F5/F9 and F11 included — and pauses nothing:
  reveal, auto, Decisions and autosave keep their ordinary clocks. The default
  view shows exact `unmei::` targets; Show everything reveals the rest. The
  body initially follows the newest events. Scrolling away reads older lines
  safely: the wheel scrolls only over the log body, the scrollbar drags,
  PgUp/PgDn page, and Home/End jump to the oldest/newest. New arrivals never
  pull an older-lines reader to the tail; only End, Follow latest or a
  deliberate scroll to the bottom resumes following. A reading position is
  kept as a retained event plus a within-row offset, so it survives hiding,
  reopening, loads and a rewrap; if retention evicts it, the panel shows
  `older lines expired` until the next deliberate navigation.
",
        usage = USAGE,
        default = DEFAULT_PACK_NAME,
    )
}

#[cfg(test)]
mod tests;
