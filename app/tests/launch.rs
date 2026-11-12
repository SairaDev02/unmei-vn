//! Hermetic subprocess checks for the `unmei-play` launch contract, including
//! version reporting.
//!
//! These run the real binary so the stdout/stderr/exit-status and
//! side-effect-free behavior are proven for the shipping entry point, not just
//! the parser. Only the help, version and usage paths are exercised: those
//! return before the App or a window exists, so a run never opens a window or
//! touches the pack, credentials or a logger. The environment and current
//! working directory are isolated per invocation.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The compiled binary the integration test runner built.
fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_unmei-play"))
}

struct Output {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Run the binary in `dir` with an isolated environment: no API key, and every
/// credential-file base pointing inside the temp directory.
fn run_in(dir: &Path, args: &[&OsStr]) -> Output {
    let mut command = Command::new(binary());
    command
        .current_dir(dir)
        .env_remove("OPENROUTER_API_KEY")
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env("APPDATA", dir)
        .env("XDG_CONFIG_HOME", dir)
        .args(args);
    let output = command.output().expect("spawn unmei-play");
    Output {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn os<'a>(args: &'a [&'a str]) -> Vec<&'a OsStr> {
    args.iter().map(OsStr::new).collect()
}

#[test]
fn help_prints_the_developer_contract_to_stdout_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        vec!["-h"],
        vec!["--help"],
        vec!["--help", "--help"],
        // A valid nonexistent pack path with help is not accessed or checked.
        vec!["missing-pack", "--dev", "--help"],
    ] {
        let output = run_in(dir.path(), &os(&args));
        assert_eq!(output.code, Some(0), "{args:?}: stderr={}", output.stderr);
        assert!(output.stderr.is_empty(), "{args:?}: {}", output.stderr);
        for needle in [
            "Usage:",
            "demo-pack",
            "folder containing the executable",
            "-V, --version",
            "`--` ends option parsing",
            "--dev",
            "debug assertions",
            "F5 immediately saves",
            "F9 loads that slot",
            "F3 opens the bottom",
            "dismiss only Monitor",
            "pauses nothing",
            "scrollbar drags",
            "older lines expired",
        ] {
            assert!(
                output.stdout.contains(needle),
                "{args:?} help is missing {needle:?}:\n{}",
                output.stdout
            );
        }
    }
}

#[test]
fn version_prints_the_release_to_stdout_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let expected = format!("unmei-play {}\n", env!("CARGO_PKG_VERSION"));
    for args in [
        vec!["-V"],
        vec!["--version"],
        vec!["-V", "--version"],
        // A valid nonexistent pack path with the version is not accessed.
        vec!["missing-pack", "--dev", "-V"],
    ] {
        let output = run_in(dir.path(), &os(&args));
        assert_eq!(output.code, Some(0), "{args:?}: stderr={}", output.stderr);
        assert!(output.stderr.is_empty(), "{args:?}: {}", output.stderr);
        assert_eq!(output.stdout, expected, "{args:?}: exact version line");
    }
}

#[test]
fn help_beats_version_in_a_real_invocation() {
    let dir = tempfile::tempdir().unwrap();
    for args in [vec!["-V", "--help"], vec!["--help", "--version"]] {
        let output = run_in(dir.path(), &os(&args));
        assert_eq!(output.code, Some(0), "{args:?}: {}", output.stderr);
        assert!(output.stdout.contains("Usage:"), "{args:?} prints help");
        assert!(
            !output
                .stdout
                .contains(&format!("unmei-play {}", env!("CARGO_PKG_VERSION"))),
            "{args:?} must not print the bare version line"
        );
    }
}

#[test]
fn a_version_request_with_a_grammar_error_exits_two() {
    let dir = tempfile::tempdir().unwrap();
    for args in [vec!["-V", "--bogus"], vec!["--version", "pack-a", "pack-b"]] {
        let output = run_in(dir.path(), &os(&args));
        assert_eq!(output.code, Some(2), "{args:?}: {:?}", output.stdout);
        assert!(output.stdout.is_empty(), "{args:?}: {}", output.stdout);
        assert!(output.stderr.contains("Usage:"), "{args:?}");
    }
}

#[test]
fn representative_grammar_errors_print_to_stderr_and_exit_two() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        vec!["--bogus"],
        vec!["--dev=false"],
        vec!["--pack"],
        vec!["--no-dev"],
        vec![""],
        vec!["pack-a", "pack-b"],
        vec!["--dev", "pack-a", "pack-b"],
        vec!["--", "pack-a", "pack-b"],
    ] {
        let output = run_in(dir.path(), &os(&args));
        assert_eq!(output.code, Some(2), "{args:?}: {:?}", output.code);
        assert!(output.stdout.is_empty(), "{args:?}: {}", output.stdout);
        assert!(
            output.stderr.contains("unmei-play:"),
            "{args:?}: {}",
            output.stderr
        );
        assert!(
            output.stderr.contains("Usage:"),
            "{args:?}: {}",
            output.stderr
        );
    }
}

#[test]
fn errors_beat_help_in_a_real_invocation() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        vec!["--help", "--bogus"],
        vec!["-h", "pack-a", "pack-b"],
        vec!["--help", "--dev=false"],
    ] {
        let output = run_in(dir.path(), &os(&args));
        assert_eq!(output.code, Some(2), "{args:?}: {}", output.stdout);
        assert!(output.stdout.is_empty(), "{args:?}: {}", output.stdout);
        assert!(output.stderr.contains("Usage:"), "{args:?}");
    }
}

#[test]
fn help_version_and_usage_errors_leave_the_log_and_sidecars_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("pack");
    std::fs::create_dir(&pack).unwrap();
    let log = dir.path().join("pack.log");
    let settings = dir.path().join("pack.settings.ron");
    std::fs::write(&log, b"PREVIOUS SESSION BYTES").unwrap();
    std::fs::write(&settings, b"(keep: true)").unwrap();

    let pack_arg = pack.as_os_str();
    // A valid help invocation that names a real pack must not open it.
    let help = run_in(dir.path(), &[OsStr::new("--help"), pack_arg]);
    assert_eq!(help.code, Some(0), "{}", help.stderr);
    // Neither must a version request naming a real pack.
    let version = run_in(dir.path(), &[pack_arg, OsStr::new("-V")]);
    assert_eq!(version.code, Some(0), "{}", version.stderr);
    // A usage error naming a real pack must not open it either.
    let error = run_in(dir.path(), &[pack_arg, OsStr::new("--bogus")]);
    assert_eq!(error.code, Some(2), "{}", error.stdout);

    assert_eq!(
        std::fs::read(&log).unwrap(),
        b"PREVIOUS SESSION BYTES",
        "the session log was not opened or truncated"
    );
    assert_eq!(
        std::fs::read(&settings).unwrap(),
        b"(keep: true)",
        "the settings sidecar was not rewritten"
    );
    // No slot, log or settings file was created beside the pack.
    let mut names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["pack", "pack.log", "pack.settings.ron"],
        "{names:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_non_unicode_pack_path_is_not_a_usage_error() {
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir().unwrap();
    // 0x80 is not valid UTF-8; the token is still a positional path, so help
    // parses cleanly and exits 0 rather than taking the usage-error path.
    let name = OsStr::from_bytes(b"pack-\x80");
    let output = run_in(dir.path(), &[OsStr::new("--help"), name]);
    assert_eq!(output.code, Some(0), "{}", output.stderr);
    assert!(output.stdout.contains("Usage:"));
}
