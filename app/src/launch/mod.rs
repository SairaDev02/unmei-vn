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

mod keiko;
mod pack;

pub use keiko::{validate_id, Format, KeikoMode, DEFAULT_BUDGET};
pub use pack::{
    executable_dir, resolve_pack, ExecutableUnknown, PackOrigin, ResolvedPack, DEFAULT_PACK_NAME,
};

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

    let keiko_mode = keiko.clone().finish(dev)?;
    Ok(LaunchRequest::Run(Launch {
        pack_dir: positional.map(PathBuf::from),
        force_dev: dev,
        keiko: keiko_mode,
        keep: keiko.keeping(),
        format: keiko.output_format(),
    }))
}

/// The concise usage block the binary prints to stderr alongside a diagnostic.
pub const USAGE: &str = "\
Usage: unmei-play [--dev] [PACK_DIR]
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
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<LaunchRequest, UsageError> {
        parse(args.iter().map(OsString::from))
    }

    fn run(args: &[&str]) -> Launch {
        match parse_strs(args) {
            Ok(LaunchRequest::Run(launch)) => launch,
            other => panic!("expected a run request, got {other:?}"),
        }
    }

    #[test]
    fn no_positional_stays_absent_and_the_dev_value_follows_the_build() {
        let launch = run(&[]);
        assert_eq!(launch.pack_dir, None, "an absent positional stays absent");
        assert!(!launch.force_dev);
        // With no `--dev`, the effective value is the build default.
        assert_eq!(launch.dev_mode(), cfg!(debug_assertions));
    }

    #[test]
    fn dev_alone_forces_on_and_still_has_no_pack_dir() {
        let launch = run(&["--dev"]);
        assert_eq!(launch.pack_dir, None);
        assert!(launch.force_dev);
        // Exact `--dev` forces on in any build, including release.
        assert!(launch.dev_mode());
    }

    /// The version text is the binary name plus the crate's own version, which
    /// comes from `[workspace.package]` rather than a literal here.
    #[test]
    fn the_version_text_names_the_binary_and_the_crate_version() {
        assert_eq!(
            version_text(),
            format!("unmei-play {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn version_is_recognized_in_both_spellings_and_repeats() {
        assert_eq!(parse_strs(&["-V"]).unwrap(), LaunchRequest::Version);
        assert_eq!(parse_strs(&["--version"]).unwrap(), LaunchRequest::Version);
        assert_eq!(
            parse_strs(&["-V", "--version", "-V"]).unwrap(),
            LaunchRequest::Version
        );
        assert_eq!(
            parse_strs(&["my-pack", "--dev", "--version"]).unwrap(),
            LaunchRequest::Version
        );
    }

    /// The settled precedence: usage errors beat both exits, and `--help` beats
    /// `--version` in either order, so `-V --help` prints help and exits 0.
    #[test]
    fn help_beats_version_and_errors_beat_both() {
        assert_eq!(parse_strs(&["-V", "--help"]).unwrap(), LaunchRequest::Help);
        assert_eq!(
            parse_strs(&["--help", "--version"]).unwrap(),
            LaunchRequest::Help
        );
        for args in [
            vec!["-V", "--bogus"],
            vec!["--version", "--dev=false"],
            vec!["-V", "pack-a", "pack-b"],
            vec!["--version", ""],
        ] {
            assert!(
                parse_strs(&args).is_err(),
                "{args:?} should be a usage error"
            );
        }
    }

    #[test]
    fn dev_may_precede_or_follow_the_pack_and_repeats_are_idempotent() {
        let before = run(&["--dev", "my-pack"]);
        let after = run(&["my-pack", "--dev"]);
        assert_eq!(before, after);
        assert_eq!(before.pack_dir, Some(PathBuf::from("my-pack")));
        assert!(before.force_dev);

        let repeated = run(&["--dev", "--dev", "my-pack", "--dev"]);
        assert_eq!(repeated, before);
    }

    #[test]
    fn a_bare_double_dash_has_no_pack_and_a_later_dash_token_is_a_path() {
        let bare = run(&["--"]);
        assert_eq!(bare.pack_dir, None);
        assert!(!bare.force_dev);

        let escaped_dev = run(&["--", "--dev"]);
        assert_eq!(escaped_dev.pack_dir, Some(PathBuf::from("--dev")));
        assert!(!escaped_dev.force_dev);

        let escaped_help = run(&["--", "-h"]);
        assert_eq!(escaped_help.pack_dir, Some(PathBuf::from("-h")));
        assert!(!escaped_help.force_dev);

        // `--` also makes the version spellings plain paths.
        assert_eq!(run(&["--", "-V"]).pack_dir, Some(PathBuf::from("-V")));
        assert_eq!(
            run(&["--", "--version"]).pack_dir,
            Some(PathBuf::from("--version"))
        );

        let forced_with_escaped_path = run(&["--dev", "--", "--dev"]);
        assert_eq!(
            forced_with_escaped_path.pack_dir,
            Some(PathBuf::from("--dev"))
        );
        assert!(forced_with_escaped_path.force_dev);
    }

    #[test]
    fn help_is_recognized_in_both_spellings_and_repeats() {
        assert_eq!(parse_strs(&["--help"]).unwrap(), LaunchRequest::Help);
        assert_eq!(parse_strs(&["-h"]).unwrap(), LaunchRequest::Help);
        assert_eq!(
            parse_strs(&["--help", "--help", "-h"]).unwrap(),
            LaunchRequest::Help
        );
        assert_eq!(
            parse_strs(&["my-pack", "--dev", "--help"]).unwrap(),
            LaunchRequest::Help
        );
        assert_eq!(
            parse_strs(&["--help", "missing-pack"]).unwrap(),
            LaunchRequest::Help
        );
    }

    #[test]
    fn errors_beat_help_even_when_help_comes_first() {
        for args in [
            vec!["--help", "--bogus"],
            vec!["-h", "pack-a", "pack-b"],
            vec!["--help", "--dev=false"],
            vec!["--help", ""],
        ] {
            assert!(
                parse_strs(&args).is_err(),
                "{args:?} should be a usage error"
            );
        }
    }

    #[test]
    fn unknown_options_are_rejected() {
        for arg in ["--bogus", "--dev=false", "--pack", "--no-dev", "-", "-dev"] {
            let error = parse_strs(&[arg]).unwrap_err();
            assert!(
                error.to_string().contains("unrecognized option"),
                "{arg}: {error}"
            );
        }
    }

    #[test]
    fn empty_and_extra_positionals_are_rejected() {
        assert_eq!(
            parse_strs(&[""]).unwrap_err().to_string(),
            "unmei-play: PACK_DIR must not be empty"
        );
        assert!(parse_strs(&["pack-a", "pack-b"])
            .unwrap_err()
            .to_string()
            .contains("unexpected extra argument 'pack-b'"));
        // Extra positionals after `--` are still an error.
        assert!(parse_strs(&["--", "pack-a", "pack-b"]).is_err());
        assert!(parse_strs(&["--dev", "pack-a", "pack-b"]).is_err());
    }

    fn keiko(args: &[&str]) -> crate::launch::KeikoMode {
        match parse_strs(args) {
            Ok(LaunchRequest::Run(launch)) => launch.keiko.expect("a Keiko mode"),
            other => panic!("expected a run request, got {other:?}"),
        }
    }

    #[test]
    fn every_keiko_mode_parses_with_its_modifiers() {
        for (args, expected) in [
            (
                vec!["--take"],
                KeikoMode::Take {
                    ids: Vec::new(),
                    live: false,
                },
            ),
            (
                vec!["pack", "--take", "one", "findings/keiko.panic-ab12cd34"],
                KeikoMode::Take {
                    ids: vec!["one".into(), "findings/keiko.panic-ab12cd34".into()],
                    live: false,
                },
            ),
            (
                vec!["--take", "--live", "--keep", "--format", "json"],
                KeikoMode::Take {
                    ids: Vec::new(),
                    live: true,
                },
            ),
            (
                vec!["--step", "one", "advance 2"],
                KeikoMode::Step {
                    id: "one".into(),
                    step: "advance 2".into(),
                },
            ),
            (
                vec!["--observe", "one"],
                KeikoMode::Observe { id: "one".into() },
            ),
            (
                vec!["--explore"],
                KeikoMode::Explore {
                    budget: DEFAULT_BUDGET,
                    from: None,
                },
            ),
            (
                vec!["--explore", "--budget", "25", "--from", "one"],
                KeikoMode::Explore {
                    budget: 25,
                    from: Some("one".into()),
                },
            ),
        ] {
            assert_eq!(keiko(&args), expected, "{args:?}");
        }
    }

    #[test]
    fn keiko_usage_errors_exit_before_any_startup_work() {
        for args in [
            // Two modes together.
            vec!["--take", "--explore"],
            vec!["--observe", "one", "--take"],
            // A modifier on the wrong mode.
            vec!["--observe", "one", "--live"],
            vec!["--take", "--budget", "5"],
            vec!["--take", "--from", "one"],
            // `--dev` with any mode.
            vec!["--dev", "--take"],
            vec!["--take", "--dev"],
            vec!["--dev", "--explore"],
            // A mode that is missing an operand.
            vec!["--step", "one"],
            vec!["--observe"],
            // A malformed case id.
            vec!["--take", "Bad"],
            vec!["--observe", "a/b"],
            vec!["--step", "a.b", "advance"],
            // A modifier with no mode at all.
            vec!["--keep"],
            vec!["--format", "json"],
            // A malformed value.
            vec!["--explore", "--budget", "many"],
            vec!["--take", "--format", "yaml"],
        ] {
            let error = parse_strs(&args).expect_err(&format!("{args:?}"));
            let _ = error;
        }
        assert_eq!(
            parse_strs(&["--take"]).map(|request| matches!(request, LaunchRequest::Run(_))),
            Ok(true)
        );
    }

    #[test]
    fn the_help_lists_the_keiko_group() {
        let help = help();
        assert!(help.contains("Testing (Keiko)"));
        for flag in [
            "--take",
            "--step",
            "--observe",
            "--explore",
            "--live",
            "--budget",
            "--from",
            "--keep",
            "--format",
        ] {
            assert!(help.contains(flag), "{flag} missing from help");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_non_unicode_path_is_not_invalid_for_its_encoding() {
        use std::os::unix::ffi::OsStringExt;

        // 0x80 is not valid UTF-8; the path is still a path.
        let path = OsString::from_vec(vec![b'p', b'a', b't', b'h', 0x80]);
        let launch = match parse([path.clone()]) {
            Ok(LaunchRequest::Run(launch)) => launch,
            other => panic!("expected a run request, got {other:?}"),
        };
        assert_eq!(launch.pack_dir, Some(PathBuf::from(path)));
        assert!(!launch.force_dev);
    }
}
