//! Spec rule 6 of the check-and-preview spec: every loader and parser error or
//! warning site carries the id of its family from §4. One fixture pack per id
//! asserts the id, spelled here as the spec table spells it, and the severity
//! of the list it arrives in.

use std::fs;
use std::path::Path;

use unmei_engine::finding::Severity;
use unmei_engine::load_pack;

const PACK_RON: &str = r#"(
    name: "rule ids",
    entry: "main",
)"#;

const STATE_RON: &str = "(met_ace: false,)";

const MAIN_SCENE: &str = "Narrator: Hello.\n";

const ACE_RON: &str = r#"(
    id: "ace",
    name: "Ace",
    decisions: {
        "greet": (
            instructions: "How does Ace greet?",
            state_keys: ["met_ace"],
            criteria: {
                "warm": "A warm greeting.",
                "cold": "A cold greeting.",
                "none": "Neither fits.",
            },
            fallback: "cold",
        ),
    },
)"#;

/// The base pack every fixture starts from: it loads with no finding.
const BASE: &[(&str, &str)] = &[
    ("pack.ron", PACK_RON),
    ("state.ron", STATE_RON),
    ("scenes/main.unmei", MAIN_SCENE),
    ("npcs/ace.ron", ACE_RON),
];

/// One fixture: the id it must report, the severity its list carries, the path
/// its findings point at and the base files it replaces (`None` deletes one).
struct Fixture {
    id: &'static str,
    severity: Severity,
    path: &'static str,
    files: &'static [(&'static str, Option<&'static str>)],
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        id: "pack.file.read",
        severity: Severity::Error,
        path: "",
        files: &[("pack.ron", None)],
    },
    Fixture {
        id: "pack.file.ron",
        severity: Severity::Error,
        path: "",
        files: &[("pack.ron", Some("(name: \"rule ids\", entry: "))],
    },
    Fixture {
        id: "pack.manifest",
        severity: Severity::Error,
        path: "",
        files: &[(
            "pack.ron",
            Some("(name: \"rule ids\", entry: \"ghost\")"),
        )],
    },
    Fixture {
        id: "pack.manifest.unlisted",
        severity: Severity::Warning,
        path: "",
        files: &[
            (
                "pack.ron",
                Some("(name: \"rule ids\", entry: \"main\", scenes: [\"main\"])"),
            ),
            ("scenes/extra.unmei", Some("Narrator: Extra.\n")),
        ],
    },
    Fixture {
        id: "pack.manifest.tagline-length",
        severity: Severity::Warning,
        path: "",
        files: &[(
            "pack.ron",
            Some(
                "(name: \"rule ids\", entry: \"main\", tagline: \"This tagline is longer than the title column fits, deliberately\")",
            ),
        )],
    },
    Fixture {
        id: "pack.scene.syntax",
        severity: Severity::Error,
        path: "line 1",
        files: &[("scenes/broken.unmei", Some("frobnicate the gate\n"))],
    },
    Fixture {
        id: "pack.scene.target",
        severity: Severity::Error,
        path: "line 2",
        files: &[("scenes/main.unmei", Some("Narrator: Hi.\njump nowhere\n"))],
    },
    Fixture {
        id: "pack.scene.cycle",
        severity: Severity::Error,
        path: "line 2",
        files: &[("scenes/main.unmei", Some("label quiet\njump quiet\n"))],
    },
    Fixture {
        id: "pack.state",
        severity: Severity::Error,
        path: "line 1",
        files: &[("scenes/main.unmei", Some("set ghost = 1\n"))],
    },
    Fixture {
        id: "pack.npc",
        severity: Severity::Error,
        path: "",
        files: &[(
            "npcs/ace.ron",
            Some("(id: \"other\", name: \"Ace\", decisions: {})"),
        )],
    },
    Fixture {
        id: "pack.decision",
        severity: Severity::Error,
        path: "line 1",
        files: &[(
            "scenes/main.unmei",
            Some("decide ace.greet -> reply\n"),
        )],
    },
    Fixture {
        id: "pack.decision.no-none",
        severity: Severity::Warning,
        path: "",
        files: &[(
            "npcs/ace.ron",
            Some(
                r#"(
    id: "ace",
    name: "Ace",
    decisions: {
        "greet": (
            instructions: "How does Ace greet?",
            state_keys: ["met_ace"],
            criteria: {
                "warm": "A warm greeting.",
                "cold": "A cold greeting.",
            },
            fallback: "cold",
        ),
    },
)"#,
            ),
        )],
    },
];

fn write_pack(root: &Path, fixture: &Fixture) {
    for (rel, text) in BASE {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().expect("a base file has a folder")).unwrap();
        fs::write(&path, text).unwrap();
    }
    for (rel, text) in fixture.files {
        let path = root.join(rel);
        match text {
            Some(text) => {
                fs::create_dir_all(path.parent().expect("a fixture file has a folder")).unwrap();
                fs::write(&path, text).unwrap();
            }
            None => fs::remove_file(&path).unwrap(),
        }
    }
}

#[test]
fn every_loader_and_parser_site_carries_its_family_id() {
    for fixture in FIXTURES {
        let dir = tempfile::tempdir().unwrap();
        write_pack(dir.path(), fixture);

        let (findings, loaded) = match load_pack(dir.path()) {
            Ok(pack) => (pack.warnings, true),
            Err(errors) => (errors, false),
        };
        assert!(
            !findings.is_empty(),
            "{}: the fixture must report its family, or this test proves nothing",
            fixture.id
        );
        for finding in &findings {
            assert_eq!(finding.rule, fixture.id, "{}: {finding:?}", fixture.id);
            assert_eq!(
                finding.severity, fixture.severity,
                "{}: {finding:?}",
                fixture.id
            );
            assert_eq!(finding.path, fixture.path, "{}: {finding:?}", fixture.id);
        }
        assert_eq!(
            loaded,
            fixture.severity == Severity::Warning,
            "{}: a warning list still loads, an error list does not",
            fixture.id
        );
    }
}
