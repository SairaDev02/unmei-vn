//! Tests for the RON splice (comment preservation) + `--create` shape
//! (author-cli contract §1/§2).

use std::path::Path;

use unmei::splice::{apply_candidate, CreateMode, SpliceError};
#[cfg(windows)]
use unmei::target::{FileKind, FilenameFault, TargetError};
use unmei_engine::content::{Criteria, DecisionConfig, Fallback, Npc, QuestionType};

#[path = "common/pack.rs"]
mod pack;

use pack::pack_bytes;

fn demo_pack() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack")
}

fn temp_pack() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    // Copy the demo pack's content files (scenes need original bytes).
    std::fs::create_dir_all(root.join("scenes")).unwrap();
    std::fs::create_dir_all(root.join("npcs")).unwrap();
    for file in ["pack.ron", "state.ron", "pack.glossary.md"] {
        std::fs::copy(demo_pack().join(file), root.join(file)).unwrap();
    }
    for scene in ["first_meeting", "reunion", "epilogue"] {
        std::fs::copy(
            demo_pack().join(format!("scenes/{scene}.unmei")),
            root.join(format!("scenes/{scene}.unmei")),
        )
        .unwrap();
    }
    std::fs::copy(demo_pack().join("npcs/ace.ron"), root.join("npcs/ace.ron")).unwrap();
    (dir, root)
}

fn farewell_config() -> DecisionConfig {
    DecisionConfig {
        question_type: QuestionType::Choice,
        instructions: "How does Mira say goodbye?".into(),
        state_keys: vec!["ledger_found".into()],
        criteria: Criteria::Choice(
            [
                ("cordial".to_string(), "polite".to_string()),
                ("grateful".to_string(), "warm".to_string()),
                ("none".to_string(), "neither".to_string()),
            ]
            .into_iter()
            .collect(),
        ),
        fallback: Fallback::Action("cordial".into()),
    }
}

fn greet_config_cold() -> DecisionConfig {
    DecisionConfig {
        question_type: QuestionType::Choice,
        instructions: "How does Ace greet, given trust?".into(),
        state_keys: vec!["affinity_level".into()],
        criteria: Criteria::Choice(
            [
                ("warm".to_string(), "warm hello".to_string()),
                ("cold".to_string(), "cold hello".to_string()),
            ]
            .into_iter()
            .collect(),
        ),
        fallback: Fallback::Action("cold".into()),
    }
}

/// RED: replacing one question's config preserves the file's comments verbatim.
#[test]
fn replace_preserves_comments() {
    let (_dir, root) = temp_pack();
    let before = std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap();
    apply_candidate(
        &root,
        &Npc {
            id: "ace".into(),
            name: "Ace".into(),
            model: None,
            decisions: [].into(),
        },
        "greet",
        &greet_config_cold(),
        CreateMode::Replace,
    )
    .unwrap();
    let after = std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap();
    // Every comment line in the original survives.
    for line in before.lines().filter(|l| l.trim_start().starts_with("//")) {
        assert!(
            after.contains(line),
            "lost comment: {line}\n--- after ---\n{after}"
        );
    }
}

/// RED: replacing rewrites only that question's block — other decisions pass
/// through unchanged.
#[test]
fn replace_touches_only_the_named_question() {
    let (_dir, root) = temp_pack();
    // Give ace a second decision first.
    let npc: Npc =
        ron::from_str(&std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap()).unwrap();
    let mut two = npc.clone();
    two.decisions.insert("farewell".into(), farewell_config());
    std::fs::write(
        root.join("npcs/ace.ron"),
        ron::ser::to_string(&two).unwrap(),
    )
    .unwrap();

    apply_candidate(
        &root,
        &npc,
        "greet",
        &greet_config_cold(),
        CreateMode::Replace,
    )
    .unwrap();
    let after: Npc =
        ron::from_str(&std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap()).unwrap();
    // farewell untouched, greet updated.
    assert_eq!(
        after.decisions.get("farewell"),
        two.decisions.get("farewell")
    );
    assert_eq!(
        after.decisions.get("greet").unwrap().state_keys,
        vec!["affinity_level"]
    );
    assert_eq!(after.decisions.len(), 2);
}

/// RED: create writes a new file with identity + one config; errors when the
/// file exists.
#[test]
fn create_writes_new_file_and_errors_on_existing() {
    let (_dir, root) = temp_pack();
    let nova = Npc {
        id: "nova".into(),
        name: "Nova".into(),
        model: None,
        decisions: [].into(),
    };
    apply_candidate(
        &root,
        &nova,
        "greet",
        &greet_config_cold(),
        CreateMode::Create {
            name: "Nova".into(),
        },
    )
    .unwrap();

    let written: Npc =
        ron::from_str(&std::fs::read_to_string(root.join("npcs/nova.ron")).unwrap()).unwrap();
    assert_eq!(written.id, "nova");
    assert_eq!(written.name, "Nova");
    assert_eq!(written.decisions.len(), 1);
    assert!(written.decisions.contains_key("greet"));

    // Second create on the same file errors (replace-only thereafter).
    let err = apply_candidate(
        &root,
        &nova,
        "greet",
        &greet_config_cold(),
        CreateMode::Create {
            name: "Nova".into(),
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, SpliceError::AlreadyExists(ref path) if path.ends_with("nova.ron")),
        "{err:?}"
    );
}

/// RED: replace errors when the NPC file doesn't exist.
#[test]
fn replace_on_missing_npc_errors() {
    let (_dir, root) = temp_pack();
    let nova = Npc {
        id: "nova".into(),
        name: "Nova".into(),
        model: None,
        decisions: [].into(),
    };
    let err = apply_candidate(
        &root,
        &nova,
        "greet",
        &greet_config_cold(),
        CreateMode::Replace,
    )
    .unwrap_err();
    assert!(
        matches!(err, SpliceError::NoSuchNpc(ref path) if path.ends_with("nova.ron")),
        "{err:?}"
    );
}

/// RED: whatever the splice writes, the pack still loads (the gate holds at
/// the write boundary too).
#[test]
fn spliced_pack_still_loads() {
    let (_dir, root) = temp_pack();
    let npc: Npc =
        ron::from_str(&std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap()).unwrap();
    apply_candidate(
        &root,
        &npc,
        "greet",
        &greet_config_cold(),
        CreateMode::Replace,
    )
    .unwrap();
    unmei_engine::load_pack(&root).unwrap();
}

/// RED: hardening — parentheses, escaped quotes, and colons inside criterion
/// strings must not confuse the block scanner (LLM descriptions are prose and
/// routinely contain `(...)` and quoted state values).
#[test]
fn splice_survives_parens_quotes_and_comments_in_strings() {
    let (_dir, root) = temp_pack();
    let thorny = DecisionConfig {
        question_type: QuestionType::Choice,
        instructions: "How does Ace greet (given trust = \"high\" or (rarely) \"low\")?".into(),
        state_keys: vec!["affinity_level".into()],
        criteria: Criteria::Choice(
            [
                (
                    "warm".to_string(),
                    "a warm hello (with a :) smile) when trust is \"high\"".to_string(),
                ),
                (
                    "cold".to_string(),
                    "a cold \"hello\" (avoid: eye contact) otherwise".to_string(),
                ),
            ]
            .into_iter()
            .collect(),
        ),
        fallback: Fallback::Action("cold".into()),
    };
    let npc: Npc =
        ron::from_str(&std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap()).unwrap();

    apply_candidate(&root, &npc, "greet", &thorny, CreateMode::Replace).unwrap();
    // The written file parses and matches the candidate exactly.
    let after: Npc =
        ron::from_str(&std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap()).unwrap();
    assert_eq!(after.decisions.get("greet"), Some(&thorny));
    unmei_engine::load_pack(&root).unwrap();

    // And a second splice into the thorny file still works (add + replace).
    apply_candidate(
        &root,
        &after,
        "farewell",
        &farewell_config(),
        CreateMode::Replace,
    )
    .unwrap();
    let after2: Npc =
        ron::from_str(&std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap()).unwrap();
    assert_eq!(after2.decisions.get("greet"), Some(&thorny));
    assert_eq!(after2.decisions.get("farewell"), Some(&farewell_config()));
}

/// RED: replace never lets model output alter the NPC's identity — the file's
/// id/name/model pass through untouched (only the question block is spliced).
#[test]
fn replace_preserves_identity_fields() {
    let (_dir, root) = temp_pack();
    let npc: Npc =
        ron::from_str(&std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap()).unwrap();
    apply_candidate(
        &root,
        &npc,
        "greet",
        &greet_config_cold(),
        CreateMode::Replace,
    )
    .unwrap();
    let after: Npc =
        ron::from_str(&std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap()).unwrap();
    assert_eq!(after.id, "ace");
    assert_eq!(after.name, "Ace");
    assert_eq!(after.model.as_deref(), Some("typesafe/jev-1.13"));
}

/// The prefactor: a `choice` config serializes with no `type` key — the
/// default — so regenerated choice files stay byte-identical; non-choice
/// configs carry `type` and their typed `fallback`.
#[test]
fn serialized_choice_omits_type_but_typed_configs_emit_it() {
    let choice = ron::ser::to_string(&greet_config_cold()).unwrap();
    assert!(!choice.contains("type"), "choice emits no type: {choice}");
    assert!(choice.contains("criteria"), "{choice}");

    let noul = DecisionConfig {
        question_type: QuestionType::Noul,
        instructions: "Is the player hostile?".into(),
        state_keys: vec!["affinity_level".into()],
        criteria: Criteria::Noul(Some(
            [
                ("true".to_string(), "hostile".to_string()),
                ("false".to_string(), "not hostile".to_string()),
            ]
            .into_iter()
            .collect(),
        )),
        fallback: Fallback::Bool(false),
    };
    let rendered = ron::ser::to_string(&noul).unwrap();
    assert!(rendered.contains("noul"), "{rendered}");
    assert!(rendered.contains("fallback:false"), "{rendered}");
    // The `noul` criteria map is a map, not RON's `Some({...})` wrapper.
    assert!(!rendered.contains("Some("), "{rendered}");
    // And the rendered form round-trips back into the typed representation.
    let reparsed: DecisionConfig = ron::from_str(&rendered).unwrap();
    assert_eq!(reparsed, noul);
}

/// The exact pre-`type` RON shape for a single-criterion `choice` config
/// (deterministic: one criterion). Pins field order and the absence of `type`,
/// so a regenerated choice file stays byte-identical.
#[test]
fn choice_config_compact_ron_is_the_pre_type_shape() {
    let config = DecisionConfig {
        question_type: QuestionType::Choice,
        instructions: "How does Ace greet?".into(),
        state_keys: vec!["affinity_level".into()],
        criteria: Criteria::Choice(
            [("cold".to_string(), "cold hello".to_string())]
                .into_iter()
                .collect(),
        ),
        fallback: Fallback::Action("cold".into()),
    };
    assert_eq!(
        ron::ser::to_string(&config).unwrap(),
        r#"(instructions:"How does Ace greet?",state_keys:["affinity_level"],criteria:{"cold":"cold hello"},fallback:"cold")"#
    );

    // The same shape through the pretty printer the CLI splice uses.
    let pretty = ron::ser::PrettyConfig::default()
        .indentor("    ".to_string())
        .separate_tuple_members(true);
    assert_eq!(
        ron::ser::to_string_pretty(&config, pretty)
            .unwrap()
            .replace("\r\n", "\n"),
        "(\n    instructions: \"How does Ace greet?\",\n    state_keys: [\n        \"affinity_level\",\n    ],\n    criteria: {\n        \"cold\": \"cold hello\",\n    },\n    fallback: \"cold\",\n)"
    );
}

/// RED: the atomic write replaces an existing file whole and leaves no temp
/// sibling behind.
#[test]
fn write_atomic_replaces_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scene.unmei");
    std::fs::write(&path, "old contents").unwrap();

    unmei::splice::write_atomic(&path, "new contents").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new contents");

    let leftovers: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != "scene.unmei")
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {leftovers:?}"
    );
}

/// RED: an unsafe NPC id is a typed terminal refusal before any join.
/// The existing NPC file, an outside sentinel and every sibling in `npcs/`
/// are left exactly as they were, and no forbidden path appears.
#[test]
fn apply_candidate_refuses_an_unsafe_id_without_touching_the_pack() {
    let (_dir, root) = temp_pack();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel.txt");
    std::fs::write(&sentinel, "sentinel").unwrap();

    let before_ace = std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap();
    let err = apply_candidate(
        &root,
        &Npc {
            id: "../evil".into(),
            name: "Evil".into(),
            model: None,
            decisions: [].into(),
        },
        "greet",
        &greet_config_cold(),
        CreateMode::Create {
            name: "Evil".into(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, SpliceError::Target(_)), "{err:?}");
    assert!(err.to_string().contains("\"../evil\""), "{err}");

    assert_eq!(
        std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap(),
        before_ace,
        "the existing NPC file is untouched"
    );
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "sentinel");
    assert!(
        !root.join("evil.ron").exists(),
        "nothing escaped to the pack root"
    );
    let leftovers: Vec<String> = std::fs::read_dir(root.join("npcs"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != "ace.ron")
        .collect();
    assert!(
        leftovers.is_empty(),
        "no sibling writer temporary: {leftovers:?}"
    );
}

/// RED: an ordinary-but-unusual accepted id writes only its fixed
/// `npcs/<id>.ron`; the identity survives and a replacement preserves the
/// file's comments, while every other pack byte is unchanged.
#[test]
fn apply_candidate_accepts_an_unusual_ordinary_id_and_writes_only_its_file() {
    let (_dir, root) = temp_pack();
    let before_ace = std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap();
    let id = "act.one";
    let npc = Npc {
        id: id.into(),
        name: "Act One".into(),
        model: None,
        decisions: [].into(),
    };

    apply_candidate(
        &root,
        &npc,
        "greet",
        &greet_config_cold(),
        CreateMode::Create {
            name: "Act One".into(),
        },
    )
    .unwrap();

    let written = root.join("npcs/act.one.ron");
    assert!(written.exists(), "the exact fixed filename");
    let created = std::fs::read_to_string(&written).unwrap();
    assert!(created.contains("id: \"act.one\""), "{created}");
    unmei_engine::load_pack(&root).unwrap();

    // Replace the same file: its comments survive and only it changes.
    let before_replace = std::fs::read_to_string(&written).unwrap();
    apply_candidate(
        &root,
        &npc,
        "greet",
        &greet_config_cold(),
        CreateMode::Replace,
    )
    .unwrap();
    let after = std::fs::read_to_string(&written).unwrap();
    for line in before_replace
        .lines()
        .filter(|line| line.trim_start().starts_with("//"))
    {
        assert!(
            after.contains(line),
            "lost comment: {line}\n--- after ---\n{after}"
        );
    }

    assert_eq!(
        std::fs::read_to_string(root.join("npcs/ace.ron")).unwrap(),
        before_ace,
        "the other NPC is untouched"
    );
    unmei_engine::load_pack(&root).unwrap();
}

/// One real NPC create through the writer on the host's own filesystem, then
/// the loader: the exact `npcs/<id>.ron` must exist, its stem must be the id
/// itself, its bytes must be the intended NPC, and every other pack byte must
/// be unchanged. A replace on the same name keeps the created comment.
#[cfg(unix)]
fn assert_npc_writer_lands(id: &str) {
    let (_dir, root) = temp_pack();
    let before = pack_bytes(&root);
    let npc = Npc {
        id: id.into(),
        name: "Native".into(),
        model: None,
        decisions: [].into(),
    };

    apply_candidate(
        &root,
        &npc,
        "greet",
        &greet_config_cold(),
        CreateMode::Create {
            name: "Native".into(),
        },
    )
    .unwrap_or_else(|e| panic!("{id:?} should be an ordinary native filename: {e}"));

    let relative = Path::new("npcs").join(format!("{id}.ron"));
    let written = root.join(&relative);
    assert_eq!(
        written.file_stem().and_then(|stem| stem.to_str()),
        Some(id),
        "the loader's stem rule must round-trip {relative:?}"
    );

    let created = std::fs::read_to_string(&written).unwrap();
    let parsed: Npc = ron::from_str(&created).unwrap();
    assert_eq!((parsed.id.as_str(), parsed.name.as_str()), (id, "Native"));
    assert_eq!(parsed.decisions.get("greet"), Some(&greet_config_cold()));

    let loaded = unmei_engine::load_pack(&root).expect("the loader accepts the exact native name");
    assert!(
        loaded.pack.npcs.contains_key(id),
        "loaded keys: {:?}",
        loaded.pack.npcs.keys()
    );

    let mut expected = before.clone();
    expected.insert(relative.clone(), created.clone().into_bytes());
    assert_eq!(
        pack_bytes(&root),
        expected,
        "the create wrote only its own file"
    );

    let mut warmer = greet_config_cold();
    warmer.fallback = Fallback::Action("warm".into());
    apply_candidate(&root, &npc, "greet", &warmer, CreateMode::Replace).unwrap();
    let replaced = std::fs::read_to_string(&written).unwrap();
    assert_eq!(
        ron::from_str::<Npc>(&replaced)
            .unwrap()
            .decisions
            .get("greet"),
        Some(&warmer),
        "the intended edit landed"
    );
    for line in created
        .lines()
        .filter(|line| line.trim_start().starts_with("//"))
    {
        assert!(replaced.contains(line), "lost comment: {line}");
    }
    let mut expected = before;
    expected.insert(relative, replaced.into_bytes());
    assert_eq!(
        pack_bytes(&root),
        expected,
        "the replace touched only that file"
    );
}

/// Native witness: `\` and `:` are ordinary Linux filename characters,
/// so the real writer authors the exact `npcs/<id>.ron` for both, while the
/// matching `#[cfg(windows)]` witness below records the opposite verdict.
#[cfg(unix)]
#[test]
fn npc_writer_keeps_backslash_and_colon_ids_ordinary_on_native_linux() {
    for id in [r"back\slash", "co:lon"] {
        assert_npc_writer_lands(id);
    }
}

/// Native witness: the same two constructions are one Windows separator
/// and one alternate-stream form, so the real writer refuses them before any
/// join.
#[cfg(windows)]
#[test]
fn npc_writer_refuses_backslash_and_colon_ids_on_native_windows() {
    for (id, expected) in [
        (r"back\slash", FilenameFault::Separator('\\')),
        ("co:lon", FilenameFault::AlternateStream),
    ] {
        let (_dir, root) = temp_pack();
        let before = pack_bytes(&root);
        let err = apply_candidate(
            &root,
            &Npc {
                id: id.into(),
                name: "Native".into(),
                model: None,
                decisions: [].into(),
            },
            "greet",
            &greet_config_cold(),
            CreateMode::Create {
                name: "Native".into(),
            },
        )
        .expect_err("the native policy refuses this construction");

        match err {
            SpliceError::Target(TargetError::UnsafeFilename {
                kind: FileKind::Npc,
                fault,
                ..
            }) => assert_eq!(fault, expected, "{id:?}"),
            other => panic!("{id:?} must be a terminal NPC target refusal: {other:?}"),
        }
        assert_eq!(
            pack_bytes(&root),
            before,
            "refusal changes no pack byte and leaves no temporary"
        );
    }
}

/// A write into an NPC file that had a leading BOM leaves a file that loads,
/// with no U+FEFF left in it: the read drops the mark and the write does not
/// put it back.
#[test]
fn replace_into_a_bom_file_drops_the_mark_and_stays_loadable() {
    let (_dir, root) = temp_pack();
    let npc_path = root.join("npcs/ace.ron");
    let original = std::fs::read_to_string(&npc_path).unwrap();
    std::fs::write(&npc_path, format!("\u{feff}{original}")).unwrap();
    let npc: Npc = ron::from_str(&original).unwrap();

    apply_candidate(
        &root,
        &npc,
        "greet",
        &greet_config_cold(),
        CreateMode::Replace,
    )
    .expect("a BOM file is spliced like any other");

    let after = std::fs::read_to_string(&npc_path).unwrap();
    assert!(!after.starts_with('\u{feff}'), "the write drops the mark");
    assert!(!after.contains('\u{feff}'), "no mark is left in the file");
    unmei_engine::load_pack(&root).expect("the written file still loads");
}
