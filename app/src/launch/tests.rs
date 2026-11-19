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
