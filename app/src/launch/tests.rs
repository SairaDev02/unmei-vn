//! The launch grammar's tests.

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

/// No `PACK_DIR` and an unknown executable folder leaves nothing to play. Only
/// play falls back to the error screen; every mode builds no app, so it prints
/// the message and exits 2 rather than opening a window (check and preview spec
/// §1). One condition covers the check, the preview and every Keiko mode.
#[test]
fn a_launch_without_a_pack_sends_every_mode_to_the_usage_exit() {
    assert_eq!(run(&[]).no_pack_exit(), NoPackExit::ErrorScreen);
    assert_eq!(run(&["--dev"]).no_pack_exit(), NoPackExit::ErrorScreen);

    assert_eq!(run(&["--check"]).no_pack_exit(), NoPackExit::Usage);
    assert_eq!(
        run(&["--check", "--format", "json"]).no_pack_exit(),
        NoPackExit::Usage
    );
    assert_eq!(run(&["--preview", "out"]).no_pack_exit(), NoPackExit::Usage);
    assert_eq!(
        run(&["--preview", "out", "--scene", "a"]).no_pack_exit(),
        NoPackExit::Usage
    );
    assert_eq!(run(&["--export-ui"]).no_pack_exit(), NoPackExit::Usage);
    assert_eq!(run(&["--take"]).no_pack_exit(), NoPackExit::Usage);
    assert_eq!(
        run(&["--step", "a", "advance"]).no_pack_exit(),
        NoPackExit::Usage
    );
    assert_eq!(run(&["--observe", "a"]).no_pack_exit(), NoPackExit::Usage);
    assert_eq!(run(&["--explore"]).no_pack_exit(), NoPackExit::Usage);
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
        vec!["--take", "--explore"],
        vec!["--observe", "one", "--take"],
        vec!["--observe", "one", "--live"],
        vec!["--take", "--budget", "5"],
        vec!["--take", "--from", "one"],
        vec!["--dev", "--take"],
        vec!["--take", "--dev"],
        vec!["--dev", "--explore"],
        vec!["--step", "one"],
        vec!["--observe"],
        vec!["--take", "Bad"],
        vec!["--observe", "a/b"],
        vec!["--step", "a.b", "advance"],
        vec!["--keep"],
        vec!["--format", "json"],
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

#[test]
fn check_is_a_mode_of_its_own_with_an_optional_format() {
    for (args, format) in [
        (vec!["--check"], Format::Text),
        (vec!["my-pack", "--check"], Format::Text),
        (vec!["--check", "--format", "json"], Format::Json),
        (vec!["--format", "json", "my-pack", "--check"], Format::Json),
    ] {
        let launch = run(&args);
        assert!(launch.check, "{args:?}");
        assert_eq!(launch.keiko, None, "{args:?}");
        assert_eq!(launch.format, format, "{args:?}");
    }
}

#[test]
fn check_is_exclusive_with_every_other_mode_and_with_dev() {
    for args in [
        vec!["--check", "--take"],
        vec!["--take", "--check"],
        vec!["--check", "--step", "one", "advance 2"],
        vec!["--check", "--observe", "one"],
        vec!["--check", "--explore"],
        vec!["--dev", "--check"],
        vec!["--check", "--dev"],
        vec!["--check", "--keep"],
        vec!["--check", "--live"],
        vec!["--check", "--format", "yaml"],
    ] {
        assert!(
            parse_strs(&args).is_err(),
            "{args:?} should be a usage error"
        );
    }
}

#[test]
fn the_help_lists_the_check_mode() {
    assert!(help().contains("--check"));
}

/// `--preview DIR` is a mode of its own: its folder is the flag's value, a bare
/// token stays `PACK_DIR`, and `--scene`, `--at`, `--state` and `--pick` are
/// collected beside it (spec §1, §9.1).
#[test]
fn preview_is_a_mode_of_its_own_with_its_selector() {
    for (args, expected) in [
        (
            vec!["--preview", "out", "--at", "reunion:19"],
            PreviewArgs {
                dir: PathBuf::from("out"),
                scene: None,
                at: Some(("reunion".to_string(), 19)),
                state: None,
                picks: Vec::new(),
            },
        ),
        (
            vec!["my-pack", "--preview", "out", "--scene", "first_meeting"],
            PreviewArgs {
                dir: PathBuf::from("out"),
                scene: Some("first_meeting".to_string()),
                at: None,
                state: None,
                picks: Vec::new(),
            },
        ),
        (
            vec![
                "--preview",
                "out",
                "--state",
                "high.ron",
                "--pick",
                "2,1",
                "--format",
                "json",
            ],
            PreviewArgs {
                dir: PathBuf::from("out"),
                scene: None,
                at: None,
                state: Some(PathBuf::from("high.ron")),
                picks: vec![2, 1],
            },
        ),
    ] {
        let launch = run(&args);
        assert_eq!(launch.keiko, Some(KeikoMode::Preview(expected)), "{args:?}");
        assert!(!launch.check, "{args:?}");
    }
    assert_eq!(
        run(&["my-pack", "--preview", "out", "--scene", "a"]).pack_dir,
        Some(PathBuf::from("my-pack")),
        "the folder is the flag's value, so a bare token stays PACK_DIR"
    );
}

#[test]
fn preview_is_exclusive_and_its_flags_are_rejected_without_it() {
    for args in [
        vec!["--preview", "out", "--take"],
        vec!["--take", "--preview", "out"],
        vec!["--preview", "out", "--explore"],
        vec!["--preview", "out", "--check"],
        vec!["--check", "--preview", "out"],
        vec!["--dev", "--preview", "out"],
        vec!["--preview", "out", "--dev"],
        vec!["--scene", "a"],
        vec!["--at", "a:1"],
        vec!["--state", "high.ron"],
        vec!["--pick", "2"],
        vec!["--preview", "out", "--scene", "a", "--at", "a:1"],
        vec!["--preview"],
        vec!["--preview", "out", "--at", "a"],
        vec!["--preview", "out", "--at", "a:sideways"],
        vec!["--preview", "out", "--scene", "two words"],
        vec!["--preview", "out", "--pick", "two"],
        vec!["--preview", "out", "--pick", "0"],
        vec!["--preview", "out", "--pick", "2,"],
        vec!["--preview", "out", "--pick", ""],
        // A Keiko-only modifier on the preview.
        vec!["--preview", "out", "--live", "--at", "a:1"],
        vec!["--preview", "out", "--budget", "5", "--at", "a:1"],
        vec!["--preview", "out", "--from", "one", "--at", "a:1"],
    ] {
        assert!(
            parse_strs(&args).is_err(),
            "{args:?} should be a usage error"
        );
    }
}

#[test]
fn the_help_lists_the_preview() {
    let help = help();
    for flag in ["--preview", "--scene", "--at", "--state", "--pick"] {
        assert!(help.contains(flag), "{flag} missing from help");
    }
}

/// `--export-ui` is a mode of its own: it takes no operand, repeats are
/// idempotent, and a bare token stays `PACK_DIR` (ui-plugin spec §1).
#[test]
fn export_ui_is_a_mode_of_its_own() {
    for args in [
        vec!["--export-ui"],
        vec!["my-pack", "--export-ui"],
        vec!["--export-ui", "my-pack"],
        vec!["--export-ui", "--export-ui"],
    ] {
        let launch = run(&args);
        assert!(launch.export_ui, "{args:?}");
        assert!(!launch.check, "{args:?}");
        assert_eq!(launch.keiko, None, "{args:?}");
    }
    assert_eq!(
        run(&["my-pack", "--export-ui"]).pack_dir,
        Some(PathBuf::from("my-pack"))
    );
}

#[test]
fn export_ui_is_exclusive_with_every_other_mode_and_with_dev() {
    for args in [
        vec!["--export-ui", "--check"],
        vec!["--check", "--export-ui"],
        vec!["--export-ui", "--preview", "out"],
        vec!["--preview", "out", "--export-ui"],
        vec!["--export-ui", "--take"],
        vec!["--take", "--export-ui"],
        vec!["--export-ui", "--step", "one", "advance 2"],
        vec!["--observe", "one", "--export-ui"],
        vec!["--explore", "--export-ui"],
        vec!["--dev", "--export-ui"],
        vec!["--export-ui", "--dev"],
        // A modifier that belongs to another mode, like `--keep` or `--format`,
        // still needs that mode rather than the export.
        vec!["--export-ui", "--keep"],
        vec!["--export-ui", "--format", "json"],
    ] {
        assert!(
            parse_strs(&args).is_err(),
            "{args:?} should be a usage error"
        );
    }

    for (args, message) in [
        (
            vec!["--export-ui", "--check"],
            "unmei-play: --export-ui cannot be combined with --check; choose one mode",
        ),
        (
            vec!["--dev", "--export-ui"],
            "unmei-play: --dev cannot be combined with --export-ui",
        ),
        (
            vec!["--take", "--export-ui"],
            "unmei-play: --export-ui cannot be combined with --take; choose one mode",
        ),
        (
            vec!["--preview", "out", "--export-ui"],
            "unmei-play: --export-ui cannot be combined with --preview; choose one mode",
        ),
    ] {
        assert_eq!(
            parse_strs(&args).unwrap_err().to_string(),
            message,
            "{args:?}"
        );
    }
}

#[test]
fn the_help_lists_the_export() {
    let help = help();
    for needle in [
        "Export:",
        "--export-ui",
        "unmei-play [PACK_DIR] --export-ui",
    ] {
        assert!(help.contains(needle), "{needle:?} missing from help");
    }
}
