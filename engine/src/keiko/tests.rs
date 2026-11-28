//! Parser and static-validation tests: the case format's grammar, its version
//! line, header order and every kind of unknown reference (spec §15 rules 3, 4
//! and 10's table half).

use super::*;
use crate::content::{Manifest, Npc, Pack, Scene, StateValue};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

fn parse(text: &str) -> Result<Case, Finding> {
    parse_case("tests/a.keiko", text)
}

fn parse_ok(text: &str) -> Case {
    parse(text).unwrap_or_else(|finding| panic!("expected a parse, got {finding:?}"))
}

#[test]
fn keiko_one_parses_and_the_rest_of_the_file_follows() {
    let case = parse_ok("keiko 1\n# a comment\n\nnew-game\nadvance 2\n");
    assert_eq!(case.steps.len(), 2);
    assert_eq!(case.steps[0].step, Step::NewGame);
    assert_eq!(case.steps[1].step, Step::Advance(2));
}

#[test]
fn a_missing_version_line_fails_alone() {
    for text in [
        "",
        "\nkeiko 1\n",
        "# a comment\nkeiko 1\n",
        "keiko 2\nnew-game\n",
        "keiko x\nnew-game\n",
        "new-game\n",
    ] {
        let finding = parse(text).expect_err("must fail");
        assert_eq!(finding.rule, rules::CASE_VERSION, "{text:?}");
        assert_eq!(finding.path, Location::Line(1));
    }
}

#[test]
fn crlf_line_endings_are_accepted() {
    let case = parse_ok("keiko 1\r\nnew-game\r\nadvance\r\n");
    assert_eq!(case.steps.len(), 2);
}

#[test]
fn comments_and_blank_lines_are_ignored_anywhere() {
    let case = parse_ok("keiko 1\n\n# first\nnew-game\n   # second\nadvance\n");
    assert_eq!(case.steps.len(), 2);
}

#[test]
fn a_hash_inside_a_quoted_text_is_data_not_a_comment() {
    let case = parse_ok("keiko 1\nexpect line \"a # b\"\n");
    assert_eq!(
        case.steps[0].step,
        Step::Expect(Expect::Line {
            text: "a # b".to_string(),
            contains: false
        })
    );
}

#[test]
fn quoted_strings_escape_only_a_quote_and_a_backslash() {
    let case = parse_ok("keiko 1\npick \"say \\\"hi\\\" \\\\ bye\"\n");
    assert_eq!(
        case.steps[0].step,
        Step::Pick(Pick::Text("say \"hi\" \\ bye".to_string()))
    );

    for text in [
        "keiko 1\npick \"unterminated\n",
        "keiko 1\npick \"bad \\n escape\"\n",
    ] {
        let finding = parse(text).expect_err("must fail");
        assert_eq!(finding.rule, rules::CASE_SYNTAX, "{text:?}");
    }
}

#[test]
fn an_unknown_verb_or_a_malformed_step_is_a_syntax_finding() {
    for text in [
        "keiko 1\nnope\n",
        "keiko 1\nadvance many\n",
        "keiko 1\nwait 2\n",
        "keiko 1\nwait -1s\n",
        "keiko 1\nsurface title\n",
        "keiko 1\nexpect surface nowhere\n",
        "keiko 1\nexpect\n",
        "keiko 1\nexpect auto maybe\n",
        "keiko 1\nclick a b c\n",
        "keiko 1\nsave 4\n",
    ] {
        let finding = parse(text).expect_err("must fail");
        assert_eq!(finding.rule, rules::CASE_SYNTAX, "{text:?}");
    }
}

#[test]
fn a_duration_takes_seconds_or_milliseconds_and_rounds_later() {
    assert_eq!(
        parse_ok("keiko 1\nwait 2s\n").steps[0].step,
        Step::Wait(Duration::from_secs(2))
    );
    assert_eq!(
        parse_ok("keiko 1\nwait 250ms\n").steps[0].step,
        Step::Wait(Duration::from_millis(250))
    );
    assert_eq!(
        parse_ok("keiko 1\nwait 0.98s\n").steps[0].step,
        Step::Wait(Duration::from_millis(980))
    );
}

#[test]
fn a_duration_past_the_range_of_duration_is_a_syntax_finding_and_does_not_panic() {
    for text in [
        "keiko 1\nwait 1e20s\n",
        "keiko 1\nwait 1e30ms\n",
        "keiko 1\nfault decision-slow ace.greet 1e20s\n",
    ] {
        let finding = parse(text).expect_err("must fail");
        assert_eq!(finding.rule, rules::CASE_SYNTAX, "{text:?}");
        assert_eq!(finding.path, Location::Line(2), "{text:?}");
    }
}

#[test]
fn a_wait_or_a_decision_slow_over_sixty_seconds_is_a_syntax_finding() {
    for text in [
        "keiko 1\nwait 61s\n",
        "keiko 1\nwait 60001ms\n",
        "keiko 1\nfault decision-slow ace.greet 61s\n",
    ] {
        let finding = parse(text).expect_err("must fail");
        assert_eq!(finding.rule, rules::CASE_SYNTAX, "{text:?}");
        assert_eq!(finding.path, Location::Line(2), "{text:?}");
    }
    assert_eq!(
        parse_ok("keiko 1\nwait 60s\n").steps[0].step,
        Step::Wait(Duration::from_secs(60))
    );
    parse_ok("keiko 1\nfault decision-slow ace.greet 60s\n");
}

#[test]
fn an_advance_count_over_a_thousand_is_a_syntax_finding() {
    let finding = parse("keiko 1\nadvance 1001\n").expect_err("must fail");
    assert_eq!(finding.rule, rules::CASE_SYNTAX);
    assert_eq!(finding.path, Location::Line(2));
    assert_eq!(
        parse_ok("keiko 1\nadvance 1000\n").steps[0].step,
        Step::Advance(1000)
    );
}

#[test]
fn every_header_parses_before_the_first_step() {
    let case = parse_ok(
        "keiko 1\n\
         decide ace.greet cold\n\
         setting play.text_speed 0\n\
         fixture slot-1 saves/slot1.ron\n\
         fixture autosave saves/auto.ron\n\
         fixture legacy-save saves/legacy.ron\n\
         fixture read-log read.ron\n\
         fixture settings settings.ron\n\
         fixture credential cred.txt\n\
         fixture env-key env.txt\n\
         dev\n\
         start first_meeting\n\
         state affinity_level = \"high\"\n\
         fault write-blocked slot-1\n\
         fault asset-missing backgrounds/x.png\n\
         fault asset-corrupt backgrounds/y.png\n\
         fault pack-file-missing pack.ron\n\
         fault pack-file scenes/a.unmei broken/a.unmei\n\
         fault credential-unreadable\n\
         fault decision-error ace.greet transport\n\
         fault decision-slow ace.greet 3s\n\
         fault decision-hold ace.greet\n\
         fault key-check hold\n\
         new-game\n",
    );
    assert_eq!(case.headers.len(), 22);
    assert_eq!(case.steps.len(), 1);
    assert!(case.dev());
}

#[test]
fn a_header_after_a_step_is_a_header_order_finding() {
    for header in [
        "dev",
        "start first_meeting",
        "state met = true",
        "setting play.auto true",
        "decide ace.greet cold",
        "fixture slot-1 x.ron",
        "fault credential-unreadable",
    ] {
        let text = format!("keiko 1\nnew-game\n{header}\n");
        let finding = parse(&text).expect_err("must fail");
        assert_eq!(finding.rule, rules::CASE_HEADER_ORDER, "{header}");
        assert_eq!(finding.path, Location::Line(3));
    }
}

#[test]
fn a_state_header_needs_the_equals_sign() {
    let finding = parse("keiko 1\nstate met true\n").expect_err("must fail");
    assert_eq!(finding.rule, rules::CASE_SYNTAX);
}

fn facts_pack() -> Pack {
    let mut state = HashMap::new();
    state.insert("met".to_string(), StateValue::Bool(false));
    state.insert("mood".to_string(), StateValue::Str("low".to_string()));
    state.insert("count".to_string(), StateValue::Num(0.0));

    let mut labels = HashMap::new();
    labels.insert("top".to_string(), 0);
    let scene = Scene {
        id: "first".to_string(),
        lines: Vec::new(),
        line_numbers: vec![1, 2],
        labels,
    };
    let mut scenes = HashMap::new();
    scenes.insert("first".to_string(), scene);

    let npc = Npc {
        id: "ace".to_string(),
        name: "Ace".to_string(),
        model: None,
        decisions: {
            let mut decisions = HashMap::new();
            decisions.insert(
                "greet".to_string(),
                crate::content::DecisionConfig {
                    question_type: crate::content::QuestionType::Noul,
                    instructions: String::new(),
                    state_keys: Vec::new(),
                    criteria: crate::content::Criteria::Noul(None),
                    fallback: crate::content::Fallback::Bool(false),
                },
            );
            decisions
        },
    };
    let mut npcs = HashMap::new();
    npcs.insert("ace".to_string(), npc);

    Pack {
        manifest: Manifest {
            name: "test".to_string(),
            entry: "first".to_string(),
            tagline: None,
            title_backdrop: None,
            scenes: Vec::new(),
        },
        state,
        scenes,
        npcs,
        root: PathBuf::from("/pack"),
    }
}

const SETTINGS: &[&str] = &["audio.master", "play.text_speed"];
const KEYS: &[&str] = &["Space", "Enter", "Escape", "KeyA"];

fn validate_text(text: &str) -> Vec<Finding> {
    let case = parse_ok(text);
    let pack = facts_pack();
    let facts = Facts {
        pack: &pack,
        settings_keys: SETTINGS,
        key_names: KEYS,
        tests_dir: None,
    };
    validate(&case, "tests/a.keiko", &facts)
}

#[test]
fn a_well_formed_case_has_no_findings() {
    let findings = validate_text(
        "keiko 1\n\
         decide ace.greet true\n\
         decide ace.greet fallback\n\
         setting play.text_speed 0\n\
         dev\n\
         start first\n\
         state mood = \"high\"\n\
         new-game\n\
         press Space\n\
         activate title.new-game\n\
         expect surface play\n\
         expect scene first\n\
         expect label top\n\
         expect state mood = \"low\"\n\
         expect control gallery.card.3 enabled\n",
    );
    assert_eq!(findings, Vec::new());
}

#[test]
fn every_unknown_reference_is_a_reference_finding() {
    let cases = [
        ("decide nobody.greet cold", "unknown NPC"),
        ("decide ace.nope cold", "unknown question"),
        ("decide ace.greet maybe", "not a landing"),
        ("decide ace cold", "not `<npc>.<question>`"),
        ("setting play.nope 1", "unknown settings key"),
        ("start nowhere", "unknown scene"),
        ("state nope = 1", "unknown state key"),
        ("state met = 1", "declared as a boolean"),
        ("state mood = 1", "declared as a string"),
        ("fixture slot-1 missing.ron", "does not exist"),
        ("fault pack-file pack.ron missing.ron", "does not exist"),
        ("fault decision-hold ace.nope", "not a decision point"),
        (
            "fault decision-error ace.nope transport",
            "not a decision point",
        ),
    ];
    for (header, expected) in cases {
        let findings = validate_text(&format!("keiko 1\n{header}\nstart first\nnew-game\n"));
        let reference: Vec<_> = findings
            .iter()
            .filter(|finding| finding.rule == rules::CASE_REFERENCE)
            .collect();
        assert_eq!(reference.len(), 1, "{header}: {findings:?}");
        assert!(
            reference[0].message.contains(expected),
            "{header}: {findings:?}"
        );
    }
}

#[test]
fn a_fault_point_or_sprite_expect_naming_no_known_npc_is_a_reference_finding() {
    let cases = [
        ("fault decision-hold nobody.greet", 2, "unknown NPC"),
        (
            "fault decision-error nobody.greet transport",
            2,
            "unknown NPC",
        ),
        ("fault decision-hold greet", 2, "not `<npc>.<question>`"),
        ("expect sprite nobody happy", 5, "unknown NPC"),
        ("expect sprite nobody none", 5, "unknown NPC"),
    ];
    for (line, number, expected) in cases {
        let (header, step) = if line.starts_with("fault") {
            (line, "")
        } else {
            ("", line)
        };
        let findings = validate_text(&format!(
            "keiko 1\n{header}\nstart first\nnew-game\n{step}\n"
        ));
        let reference: Vec<_> = findings
            .iter()
            .filter(|finding| finding.rule == rules::CASE_REFERENCE)
            .collect();
        assert_eq!(reference.len(), 1, "{line}: {findings:?}");
        assert!(
            reference[0].message.contains(expected),
            "{line}: {findings:?}"
        );
        assert_eq!(
            reference[0].path,
            Location::Line(number),
            "{line}: {findings:?}"
        );
    }
    let known = validate_text(
        "keiko 1\nstart first\nnew-game\nexpect sprite ace anything\nexpect sprite ace none\n",
    );
    assert_eq!(known, Vec::new());
}

#[test]
fn a_state_header_without_a_start_is_a_reference_finding() {
    let findings = validate_text("keiko 1\nstate mood = \"high\"\nnew-game\n");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == rules::CASE_REFERENCE
                && finding.message.contains("needs a `start` header")),
        "{findings:?}"
    );
}

#[test]
fn an_unknown_key_name_is_a_syntax_finding_and_an_unknown_control_is_a_reference() {
    let findings = validate_text("keiko 1\npress KeyZ\nactivate title.nope\nnew-game\n");
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(findings
        .iter()
        .any(|finding| finding.rule == rules::CASE_SYNTAX && finding.message.contains("KeyZ")));
    assert!(findings
        .iter()
        .any(|finding| finding.rule == rules::CASE_REFERENCE
            && finding.message.contains("title.nope")));
}

#[test]
fn a_fault_step_needs_the_fault_it_releases() {
    let findings = validate_text(
        "keiko 1\n\
         release-decision ace.greet\n\
         release-key-check\n\
         unblock slot-1\n\
         new-game\n",
    );
    let reference: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule == rules::CASE_REFERENCE)
        .collect();
    assert_eq!(reference.len(), 3, "{findings:?}");

    let clean = validate_text(
        "keiko 1\n\
         fault decision-hold ace.greet\n\
         fault key-check hold\n\
         fault write-blocked slot-1\n\
         release-decision ace.greet\n\
         release-key-check\n\
         unblock slot-1\n\
         new-game\n",
    );
    assert_eq!(clean, Vec::new());
}

#[test]
fn the_control_table_is_the_validator_s_authority() {
    let findings =
        validate_text("keiko 1\nactivate overlay.resume\nexpect control confirm.no focused\n");
    assert_eq!(findings, Vec::new());
}

#[test]
fn a_rect_assertion_parses_its_four_numbers_and_an_optional_tolerance() {
    let case = parse_ok(
        "keiko 1\nexpect control title.new-game rect 10 20.5 300 40\nexpect control title.quit rect 0 0 1280 720 \u{b1}2\n",
    );
    assert_eq!(
        case.steps[0].step,
        Step::Expect(Expect::ControlRect {
            id: "title.new-game".to_string(),
            rect: [10.0, 20.5, 300.0, 40.0],
            tolerance: 0.5,
        })
    );
    assert_eq!(
        case.steps[1].step,
        Step::Expect(Expect::ControlRect {
            id: "title.quit".to_string(),
            rect: [0.0, 0.0, 1280.0, 720.0],
            tolerance: 2.0,
        })
    );
}

#[test]
fn a_malformed_rect_assertion_is_a_syntax_error() {
    for step in [
        "expect control title.quit rect 0 0 10",
        "expect control title.quit rect -1 0 10 10",
        "expect control title.quit rect 0 0 10 10 \u{b1}",
        "expect control title.quit rect 0 0 10 10 \u{b1}-1",
        "expect control title.quit rect 0 0 10 wide",
        "expect control title.quit rect 0 0 10 10 2",
    ] {
        let finding = parse(&format!("keiko 1\n{step}\n")).expect_err(step);
        assert_eq!(finding.rule, rules::CASE_SYNTAX, "{step}");
    }
}

#[test]
fn a_fixture_or_pack_file_path_must_stay_inside_its_folder() {
    let pack_dir = tempfile::tempdir().unwrap();
    let tests_dir = pack_dir.path().join("tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(tests_dir.join("ok.ron"), "()").unwrap();
    std::fs::write(pack_dir.path().join("outside.txt"), "host file").unwrap();
    let absolute = pack_dir.path().join("outside.txt");
    let absolute = absolute.to_str().unwrap();

    let pack = facts_pack();
    let facts = Facts {
        pack: &pack,
        settings_keys: SETTINGS,
        key_names: KEYS,
        tests_dir: Some(&tests_dir),
    };
    let cases = [
        "fixture env-key ../outside.txt".to_string(),
        format!("fixture env-key {absolute}"),
        "fault pack-file pack.ron ../outside.txt".to_string(),
        "fault pack-file ../outside.ron ok.ron".to_string(),
        "fault pack-file-missing ../outside.txt".to_string(),
    ];
    for header in &cases {
        let case = parse_ok(&format!("keiko 1\n{header}\nstart first\nnew-game\n"));
        let findings = validate(&case, "tests/a.keiko", &facts);
        let reference: Vec<_> = findings
            .iter()
            .filter(|finding| finding.rule == rules::CASE_REFERENCE)
            .collect();
        assert_eq!(reference.len(), 1, "{header}: {findings:?}");
        assert!(
            reference[0].message.contains("relative path inside"),
            "{header}: {findings:?}"
        );
    }

    let case = parse_ok(
        "keiko 1\nfixture env-key ok.ron\nfault pack-file pack.ron ok.ron\nstart first\nnew-game\n",
    );
    assert_eq!(validate(&case, "tests/a.keiko", &facts), Vec::new());
}
