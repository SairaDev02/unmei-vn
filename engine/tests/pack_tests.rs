//! Integration tests for content-pack loading, validated against
//! `examples/demo-pack/` (the load-bearing parse fixtures) and synthetic
//! invalid packs built in temp dirs.

use std::fs;
use std::path::Path;

use unmei_engine::{load_pack, Criteria, Fallback, Line};

fn demo_pack() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack")
}

fn write_pack(dir: &Path, files: &[(&str, String)]) -> std::path::PathBuf {
    for (rel, content) in files {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    dir.to_path_buf()
}

const PACK_RON: &str = r#"(
    name: "test pack",
    entry: "scene_a",
)"#;

const STATE_RON: &str = r#"(
    affinity_level: "low",
    met_ace: false,
)"#;

const SCENE_A: &str = r#"# scene a
bg school_gate
Narrator: Hello.

choice
  "Say hi" -> greeted
  "Stay quiet" -> quiet
end

label greeted
set met_ace = true
jump after

label quiet
set met_ace = true
set affinity_level = "high"
jump after

label after
decide ace.greet -> reply

label reply.warm
Ace: Warm hello.

label reply.cold
Ace: Cold hello.

label reply.none
Narrator: Nothing fits.
"#;

const ACE_RON: &str = r#"(
    id: "ace",
    name: "Ace",
    model: "typesafe/jev-1.13",
    decisions: {
        "greet": (
            instructions: "How does Ace greet the player?",
            state_keys: ["affinity_level", "met_ace"],
            criteria: {
                "warm": "A warm greeting.",
                "cold": "A cold greeting.",
                "none": "Neither fits.",
            },
            fallback: "cold",
        ),
    },
)"#;

const TYPED_ACE_RON: &str = r#"(
    id: "ace",
    name: "Ace",
    decisions: {
        "is_hostile": (
            type: "noul",
            instructions: "Is the player hostile?",
            state_keys: ["met_ace"],
            criteria: {
                "true": "Hostile.",
                "false": "Not hostile.",
            },
            fallback: false,
        ),
        "trust": (
            type: "score",
            instructions: "How much does Ace trust the player?",
            state_keys: ["met_ace"],
            criteria: [
                "No trust.",
                "Some trust.",
                "Complete trust.",
            ],
            fallback: 0,
        ),
    },
)"#;

const NOUL_ACE_RON: &str = r#"(
    id: "ace",
    name: "Ace",
    decisions: {
        "is_hostile": (
            type: "noul",
            instructions: "Is the player hostile?",
            state_keys: ["met_ace"],
            criteria: {
                "true": "Hostile.",
                "false": "Not hostile.",
            },
            fallback: false,
        ),
    },
)"#;

const NOUL_WITHOUT_CRITERIA_ACE_RON: &str = r#"(
    id: "ace",
    name: "Ace",
    decisions: {
        "is_hostile": (
            type: "noul",
            instructions: "Is the player hostile?",
            state_keys: ["met_ace"],
            fallback: false,
        ),
    },
)"#;

const NOUL_SCENE: &str = "decide ace.is_hostile -> want\nlabel want.true\nNarrator: True.\nlabel want.false\nNarrator: False.\n";

const SCORE_SCENE: &str = "decide ace.trust -> want\nlabel want.0\nNarrator: None.\nlabel want.1\nNarrator: Some.\nlabel want.2\nNarrator: All.\n";

fn valid_pack_files() -> Vec<(&'static str, String)> {
    vec![
        ("pack.ron", PACK_RON.to_string()),
        ("state.ron", STATE_RON.to_string()),
        ("scenes/scene_a.unmei", SCENE_A.to_string()),
        ("npcs/ace.ron", ACE_RON.to_string()),
        ("assets/README.md", String::new()),
    ]
}

#[test]
fn demo_pack_loads_without_errors_or_warnings() {
    let loaded = load_pack(&demo_pack()).expect("demo pack loads");
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let pack = loaded.pack;
    assert_eq!(pack.manifest.name, "unmei demo");
    assert_eq!(pack.manifest.entry, "first_meeting");
    assert_eq!(pack.scenes.len(), 3);
    assert_eq!(pack.npcs.len(), 1);
    // state.ron is the declared vocabulary with initial values.
    assert_eq!(
        pack.state.get("affinity_level"),
        Some(&unmei_engine::StateValue::Str("low".into()))
    );
    assert_eq!(
        pack.state.get("letters_found"),
        Some(&unmei_engine::StateValue::Num(0.0))
    );
    // The decide point in reunion has its label closure satisfied.
    let ace = &pack.npcs["ace"];
    let greet = &ace.decisions["greet"];
    assert_eq!(greet.fallback, Fallback::Action("cold".into()));
    assert!(matches!(&greet.criteria, Criteria::Choice(map) if map.contains_key("none")));
}

#[test]
fn demo_pack_scene_structure_is_sane() {
    let loaded = load_pack(&demo_pack()).unwrap();
    let pack = loaded.pack;
    let reunion = &pack.scenes["reunion"];
    assert!(reunion.label_index("reply.warm").is_some());
    let first = &pack.scenes["first_meeting"];
    assert!(first.label_index("converge").is_some());
}

#[test]
fn demo_pack_epilogue_loads_and_branches_on_collected_state() {
    let loaded = load_pack(&demo_pack()).expect("epilogue pack loads");
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let pack = loaded.pack;

    // `reunion`'s final label reaches the epilogue scene file.
    let reunion = &pack.scenes["reunion"];
    let end = reunion
        .label_index("end_reunion")
        .expect("end_reunion label");
    assert!(reunion.lines[end..]
        .iter()
        .any(|l| matches!(l, Line::Jump(target) if target == "epilogue")));

    // The fixture exercises string equality, numeric ordering, a bare bool and
    // one nesting level, using only keys declared in `state.ron`.
    let epilogue = &pack.scenes["epilogue"];
    let conditions: Vec<&unmei_engine::Condition> = epilogue
        .lines
        .iter()
        .filter_map(|l| match l {
            Line::If { condition, .. } => Some(condition),
            _ => None,
        })
        .collect();
    assert!(conditions.iter().any(|c| c.key == "affinity_level"
        && c.op == unmei_engine::CondOp::Eq
        && c.value == unmei_engine::Literal::Str("high".into())));
    assert!(conditions
        .iter()
        .any(|c| c.key == "letters_found" && c.op == unmei_engine::CondOp::Gt));
    assert!(conditions.iter().any(|c| c.key == "met_ace" && c.bare));

    // One nesting level: an `if` opens inside the outer block's then arm.
    let (outer_if, else_index) = epilogue
        .lines
        .iter()
        .enumerate()
        .find_map(|(i, l)| match l {
            Line::If {
                else_index: Some(else_index),
                ..
            } => Some((i, *else_index)),
            _ => None,
        })
        .expect("an if with an else arm");
    assert!(
        epilogue.lines[outer_if + 1..else_index]
            .iter()
            .any(|l| matches!(l, Line::If { .. })),
        "the outer then arm nests another if"
    );
}

#[test]
fn valid_synthetic_pack_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let root = write_pack(tmp.path(), &valid_pack_files());
    let loaded = load_pack(&root).expect("valid pack loads");
    assert!(loaded.warnings.is_empty());
}

#[test]
fn missing_manifest_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let root = write_pack(tmp.path(), &[("state.ron", STATE_RON.to_string())]);
    let errs = load_pack(&root).unwrap_err();
    assert!(errs.iter().any(|e| e.message.contains("pack manifest")));
}

#[test]
fn missing_state_file_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let root = write_pack(tmp.path(), &[("pack.ron", PACK_RON.to_string())]);
    let errs = load_pack(&root).unwrap_err();
    assert!(errs.iter().any(|e| e.file == "state.ron"));
}

#[test]
fn entry_scene_must_exist() {
    let tmp = tempfile::tempdir().unwrap();
    let files = valid_pack_files();
    let mut files = files;
    files[0] = ("pack.ron", "(name: \"p\", entry: \"nope\")".to_string());
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("entry scene `nope`")),
        "{errs:?}"
    );
}

#[test]
fn undeclared_set_target_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("set met_ace = true", "set met_bob = true"),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter().any(|e| e.message.contains("`set met_bob`")),
        "{errs:?}"
    );
}

#[test]
fn undeclared_interpolation_var_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("Narrator: Nothing fits.", "Narrator: {mood} today."),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("unknown variable `{mood}`")),
        "{errs:?}"
    );
}

#[test]
fn undeclared_state_key_in_config_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[3] = (
        "npcs/ace.ron",
        ACE_RON.replace(
            "state_keys: [\"affinity_level\", \"met_ace\"]",
            "state_keys: [\"mood\"]",
        ),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("state_keys entry `mood`")),
        "{errs:?}"
    );
}

#[test]
fn fallback_not_in_criteria_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[3] = (
        "npcs/ace.ron",
        ACE_RON.replace("fallback: \"cold\"", "fallback: \"shy\""),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("fallback `shy` is not a criterion id")),
        "{errs:?}"
    );
}

/// Explicit `type: "choice"` is the default spelled out: the config loads and
/// keeps choice behaviour.
#[test]
fn explicit_choice_type_is_accepted() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[3] = (
        "npcs/ace.ron",
        ACE_RON.replace(
            "instructions:",
            "type: \"choice\",\n            instructions:",
        ),
    );
    let root = write_pack(tmp.path(), &files);
    let loaded = load_pack(&root).expect("explicit choice loads");
    let greet = &loaded.pack.npcs["ace"].decisions["greet"];
    assert_eq!(greet.question_type, unmei_engine::QuestionType::Choice);
}

/// The prefactor: a `type` discriminator and the typed `criteria` /
/// `fallback` shapes parse. These configs are referenced by no scene, so this
/// test covers parsing only.
#[test]
fn typed_question_shapes_parse() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[3] = ("npcs/ace.ron", TYPED_ACE_RON.to_string());
    // The synthetic scene's `decide ace.greet` would be an unknown question now.
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("decide ace.greet -> reply\n", ""),
    );
    let root = write_pack(tmp.path(), &files);
    let loaded = load_pack(&root).expect("typed shapes parse");
    assert_eq!(loaded.pack.npcs["ace"].decisions.len(), 2);
}

/// `noul` end-to-end: a decide point loads when both boolean landing
/// labels exist, and the config keeps its typed shape.
#[test]
fn noul_decide_point_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = ("scenes/scene_a.unmei", NOUL_SCENE.to_string());
    files[3] = ("npcs/ace.ron", NOUL_ACE_RON.to_string());
    let root = write_pack(tmp.path(), &files);
    let loaded = load_pack(&root).expect("noul decide point loads");
    let config = &loaded.pack.npcs["ace"].decisions["is_hostile"];
    assert_eq!(config.question_type, unmei_engine::QuestionType::Noul);
    assert_eq!(config.fallback, Fallback::Bool(false));
    assert!(matches!(&config.criteria, Criteria::Noul(Some(map)) if map.len() == 2));
}

/// The `noul` label closure is both booleans, not just the fallback: a missing
/// `<prefix>.false` is a load error even when the fallback is `false`.
#[test]
fn noul_missing_landing_label_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        NOUL_SCENE.replace("label want.false\nNarrator: False.\n", ""),
    );
    files[3] = ("npcs/ace.ron", NOUL_ACE_RON.to_string());
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("missing label `want.false`")),
        "{errs:?}"
    );
}

/// A `noul` criteria map is both-or-neither: a single key is a load error.
#[test]
fn noul_single_criteria_key_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("decide ace.greet -> reply\n", ""),
    );
    files[3] = (
        "npcs/ace.ron",
        NOUL_ACE_RON.replace("\"false\": \"Not hostile.\",\n", ""),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter().any(|e| e
            .message
            .contains("exactly the keys \"true\" and \"false\"")),
        "{errs:?}"
    );
}

/// A `noul` criteria map with a third key is a load error, same as a single
/// key: the map is exactly `"true"`/`"false"` or absent.
#[test]
fn noul_extra_criteria_key_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("decide ace.greet -> reply\n", ""),
    );
    files[3] = (
        "npcs/ace.ron",
        NOUL_ACE_RON.replace(
            "\"false\": \"Not hostile.\",\n",
            "\"false\": \"Not hostile.\",\n                \"maybe\": \"Unsure.\",\n",
        ),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter().any(|e| e
            .message
            .contains("exactly the keys \"true\" and \"false\"")),
        "{errs:?}"
    );
}

/// Omitting the criteria map is valid: the model judges from `instructions`
/// alone, and the landing labels are unchanged.
#[test]
fn noul_without_a_criteria_map_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = ("scenes/scene_a.unmei", NOUL_SCENE.to_string());
    files[3] = ("npcs/ace.ron", NOUL_WITHOUT_CRITERIA_ACE_RON.to_string());
    let root = write_pack(tmp.path(), &files);
    let loaded = load_pack(&root).expect("noul without criteria loads");
    let config = &loaded.pack.npcs["ace"].decisions["is_hostile"];
    assert!(matches!(&config.criteria, Criteria::Noul(None)));
}

/// `score` end-to-end: a decide point loads when every legend index
/// `<prefix>.0` … `<prefix>.<n-1>` exists, and the config keeps its typed shape.
/// The `"none"` advisory stays choice-only, so the score config warns neither
/// way.
#[test]
fn score_decide_point_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = ("scenes/scene_a.unmei", SCORE_SCENE.to_string());
    files[3] = ("npcs/ace.ron", TYPED_ACE_RON.to_string());
    let root = write_pack(tmp.path(), &files);
    let loaded = load_pack(&root).expect("score decide point loads");
    assert!(
        !loaded.warnings.iter().any(|w| w.message.contains("`none`")),
        "{:?}",
        loaded.warnings
    );
    let config = &loaded.pack.npcs["ace"].decisions["trust"];
    assert_eq!(config.question_type, unmei_engine::QuestionType::Score);
    assert_eq!(config.fallback, Fallback::Index(0));
    assert!(matches!(&config.criteria, Criteria::Score(levels) if levels.len() == 3));
}

/// Every legend index is part of the label closure: dropping `<prefix>.1` is a
/// load error even though the fallback (`0`) is present.
#[test]
fn score_missing_landing_label_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCORE_SCENE.replace("label want.1\nNarrator: Some.\n", ""),
    );
    files[3] = ("npcs/ace.ron", TYPED_ACE_RON.to_string());
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("missing label `want.1`")),
        "{errs:?}"
    );
}

/// A `score` legend needs at least two levels; one level is a load error.
#[test]
fn score_with_one_level_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("decide ace.greet -> reply\n", ""),
    );
    files[3] = (
        "npcs/ace.ron",
        TYPED_ACE_RON.replace(
            "                \"Some trust.\",\n                \"Complete trust.\",\n",
            "",
        ),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("at least two levels")),
        "{errs:?}"
    );
}

/// `fallback` is a legend index and must be in range: index 3 with a three-level
/// legend is a load error.
#[test]
fn score_out_of_range_fallback_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("decide ace.greet -> reply\n", ""),
    );
    files[3] = (
        "npcs/ace.ron",
        TYPED_ACE_RON.replace("fallback: 0,", "fallback: 3,"),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("outside the legend")),
        "{errs:?}"
    );
}

/// A `score` config requires a `fallback` index; omitting it is a load error.
#[test]
fn score_without_fallback_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("decide ace.greet -> reply\n", ""),
    );
    files[3] = (
        "npcs/ace.ron",
        TYPED_ACE_RON.replace("            fallback: 0,\n", ""),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter().any(|e| e.message.contains("fallback")),
        "{errs:?}"
    );
}

#[test]
fn missing_decide_label_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        // drop reply.none so the closure over criteria+fallback breaks
        SCENE_A.replace("label reply.none\nNarrator: Nothing fits.\n", ""),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("missing label `reply.none`")),
        "{errs:?}"
    );
}

#[test]
fn decide_unknown_npc_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("decide ace.greet", "decide mira.greet"),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| e.message.contains("unknown npc `mira`")));
}

#[test]
fn decide_unknown_question_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("decide ace.greet", "decide ace.farewell"),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("unknown question `farewell`")),
        "{errs:?}"
    );
}

#[test]
fn jump_to_nowhere_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("jump after", "jump limbo"),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("`limbo` is neither a label")),
        "{errs:?}"
    );
}

#[test]
fn jump_to_scene_file_resolves() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files.push((
        "scenes/scene_b.unmei",
        "# scene b\nNarrator: End.\n".to_string(),
    ));
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("jump after", "jump scene_b"),
    );
    let root = write_pack(tmp.path(), &files);
    let loaded = load_pack(&root).expect("cross-scene jump resolves");
    assert_eq!(loaded.pack.scenes.len(), 2);
}

#[test]
fn choice_target_must_resolve() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("\"Stay quiet\" -> quiet", "\"Stay quiet\" -> void"),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter().any(|e| e.message.contains("targets `void`")),
        "{errs:?}"
    );
}

#[test]
fn npc_id_must_match_filename() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[3] = ("npcs/bob.ron", ACE_RON.to_string());
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("does not match filename stem")),
        "{errs:?}"
    );
}

#[test]
fn none_advisory_is_warning_not_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[3] = (
        "npcs/ace.ron",
        ACE_RON.replace("                \"none\": \"Neither fits.\",\n", ""),
    );
    let root = write_pack(tmp.path(), &files);
    let loaded = load_pack(&root).expect("missing `none` is a warning, not an error");
    assert_eq!(loaded.warnings.len(), 1);
    assert!(loaded.warnings[0].message.contains("no `none` action"));
}

#[test]
fn all_errors_reported_together() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[3] = (
        "npcs/ace.ron",
        ACE_RON.replace("fallback: \"cold\"", "fallback: \"shy\""),
    );
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("jump after", "jump limbo"),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(errs.len() >= 2, "expected multiple errors, got {errs:?}");
}

#[test]
fn malformed_ron_fails_usefully() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[3] = ("npcs/ace.ron", "( id: \"ace\", ".to_string());
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(errs.iter().any(|e| e.message.contains("invalid NPC RON")));
}

#[test]
fn unknown_npc_field_fails() {
    // deny_unknown_fields catches author typos at load.
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[3] = (
        "npcs/ace.ron",
        ACE_RON.replace("name: \"Ace\",", "namee: \"Ace\","),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter().any(|e| e.message.contains("NPC RON")),
        "{errs:?}"
    );
}

// --- conditionals ---------------------------------------------------------

#[test]
fn conditional_scene_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if affinity_level == \"low\"\nNarrator: Low.\nelse\nNarrator: Not low.\nendif\nif met_ace\nNarrator: Met.\nendif\nif met_ace != true\nNarrator: Not met.\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let loaded = load_pack(&root).expect("conditional scene loads");
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn if_without_endif_is_a_load_error_naming_file_and_line() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "Narrator: Hi.\nif met_ace\nNarrator: Met.\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("no matching `endif`"))
        .expect("unclosed-if error");
    assert_eq!(err.file, "scenes/scene_a.unmei");
    assert_eq!(err.line, Some(2));
}

#[test]
fn stray_else_is_a_load_error_naming_the_line() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = ("scenes/scene_a.unmei", "Narrator: Hi.\nelse\n".to_string());
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("without a matching `if`"))
        .expect("stray-else error");
    assert_eq!(err.line, Some(2));
}

#[test]
fn doubled_else_is_a_load_error_naming_the_line() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if met_ace\nelse\nelse\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("duplicate `else`"))
        .expect("doubled-else error");
    assert_eq!(err.line, Some(3));
}

#[test]
fn stray_endif_is_a_load_error_naming_the_line() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = ("scenes/scene_a.unmei", "endif\n".to_string());
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("without a matching `if`"))
        .expect("stray-endif error");
    assert_eq!(err.file, "scenes/scene_a.unmei");
    assert_eq!(err.line, Some(1));
}

#[test]
fn label_inside_arm_is_a_load_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if met_ace\nlabel inside\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("`label` is not allowed"))
        .expect("label-in-arm error");
    assert_eq!(err.line, Some(2));
}

#[test]
fn undeclared_condition_key_is_a_load_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "# comment\nif letters_found == 1\nNarrator: One.\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("condition key `letters_found`"))
        .expect("undeclared condition key error");
    assert_eq!(err.file, "scenes/scene_a.unmei");
    assert_eq!(err.line, Some(2));
}

#[test]
fn capitalised_if_speaker_still_parses_as_a_say_line() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "If: only she had known.\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    load_pack(&root).expect("a say-line whose speaker is `If` still parses");
}

#[test]
fn lowercase_if_reads_as_a_conditional_not_a_say_line() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if only she had known: yes.\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(errs.iter().any(|e| e.message.contains("`if`")), "{errs:?}");
}

// --- nesting and control flow inside arms --------------------------------

#[test]
fn nested_conditional_scene_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if met_ace\nif affinity_level == \"low\"\nif met_ace == false\ndecide ace.greet -> reply\nelse\nNarrator: Unreachable.\nendif\nchoice\n  \"Say hi\" -> greeted\n  \"Stay quiet\" -> quiet\nend\nelse\nNarrator: Not low.\nendif\nelse\nNarrator: Not met.\nendif\nlabel greeted\nNarrator: Greeted.\nlabel quiet\nNarrator: Quiet.\nlabel reply.warm\nAce: Warm.\nlabel reply.cold\nAce: Cold.\nlabel reply.none\nAce: Nothing.\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    load_pack(&root)
        .expect("a scene nesting three deep, with a choice and a decide inside arms, loads");
}

#[test]
fn conditional_inside_a_choice_block_is_a_load_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "choice\n  \"Say hi\" -> greeted\n  if met_ace\n  \"Stay quiet\" -> quiet\nend\nlabel greeted\nNarrator: Hi.\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("not allowed inside a `choice` block"))
        .expect("choice-block conditional error");
    assert_eq!(err.file, "scenes/scene_a.unmei");
    assert_eq!(err.line, Some(3));
}

#[test]
fn label_nested_in_an_arm_is_a_load_error() {
    // A `label` may not appear at any depth inside an arm.
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if met_ace\nif affinity_level == \"low\"\nlabel deep\nendif\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("`label` is not allowed"))
        .expect("nested label error");
    assert_eq!(err.line, Some(3));
}

// --- fixed state kinds and numeric ordering ------------------------------

#[test]
fn set_of_the_wrong_kind_is_a_load_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        SCENE_A.replace("set affinity_level = \"high\"", "set affinity_level = 3"),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("`set affinity_level` assigns a number"))
        .unwrap_or_else(|| panic!("set kind-mismatch error, got {errs:?}"));
    assert_eq!(err.file, "scenes/scene_a.unmei");
    assert!(err.message.contains("declared as a string"), "{err}");
}

#[test]
fn condition_literal_of_the_wrong_kind_is_a_load_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if affinity_level == 3\nNarrator: No.\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("condition literal is a number"))
        .unwrap_or_else(|| panic!("condition literal kind-mismatch error, got {errs:?}"));
    assert_eq!(err.file, "scenes/scene_a.unmei");
    assert_eq!(err.line, Some(1));
    assert!(
        err.message
            .contains("`affinity_level` is declared as a string"),
        "{err}"
    );
}

#[test]
fn bare_non_bool_key_is_a_load_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if affinity_level\nNarrator: No.\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("bare condition key `affinity_level`"))
        .unwrap_or_else(|| panic!("bare non-bool error, got {errs:?}"));
    assert!(err.message.contains("must be a bool"), "{err}");
    assert_eq!(err.line, Some(1));
}

#[test]
fn explicit_bool_literal_on_a_non_bool_key_is_a_kind_mismatch_not_a_bare_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if affinity_level == true\nNarrator: No.\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("condition literal is a bool"))
        .unwrap_or_else(|| panic!("explicit bool literal kind-mismatch, got {errs:?}"));
    assert!(!err.message.contains("bare condition key"), "{err}");
}

#[test]
fn ordering_on_a_non_number_key_is_a_load_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if affinity_level > \"low\"\nNarrator: No.\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    let err = errs
        .iter()
        .find(|e| e.message.contains("uses an ordering operator"))
        .unwrap_or_else(|| panic!("ordering-on-non-number error, got {errs:?}"));
    assert_eq!(err.file, "scenes/scene_a.unmei");
    assert_eq!(err.line, Some(1));
    assert!(
        err.message.contains("ordering applies to numbers only"),
        "{err}"
    );
}

#[test]
fn ordering_on_a_bool_key_is_a_load_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[2] = (
        "scenes/scene_a.unmei",
        "if met_ace >= false\nNarrator: No.\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("uses an ordering operator")),
        "{errs:?}"
    );
}

#[test]
fn numeric_ordering_conditions_load_cleanly() {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[1] = (
        "state.ron",
        "(\n    affinity_level: \"low\",\n    met_ace: false,\n    letters_found: 0,\n)"
            .to_string(),
    );
    files[2] = (
        "scenes/scene_a.unmei",
        "if letters_found > 0\nNarrator: Some.\nelse\nNarrator: None.\nendif\nif letters_found <= 3\nNarrator: Few.\nendif\n".to_string(),
    );
    let root = write_pack(tmp.path(), &files);
    let loaded = load_pack(&root).expect("numeric ordering conditions load");
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn demo_pack_all_five_set_lines_kind_match_and_load_clean() {
    let loaded = load_pack(&demo_pack()).expect("demo pack loads with fixed state kinds");
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let sets = loaded
        .pack
        .scenes
        .values()
        .flat_map(|s| s.lines.iter())
        .filter(|l| matches!(l, Line::Set { .. }))
        .count();
    assert_eq!(sets, 5, "all five demo `set` lines survive kind validation");
}

// --- manifest tagline ----------------------------------------------------

/// A valid pack whose manifest is replaced wholesale, for tagline shape tests.
fn pack_with_manifest(manifest: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[0] = ("pack.ron", manifest.to_string());
    let root = write_pack(tmp.path(), &files);
    (tmp, root)
}

#[test]
fn demo_pack_declares_a_tagline() {
    let loaded = load_pack(&demo_pack()).expect("demo pack loads");
    assert_eq!(
        loaded.pack.manifest.tagline.as_deref(),
        Some("a demo pack"),
        "the demo pack declares its tagline"
    );
}

#[test]
fn absent_manifest_tagline_is_none() {
    let tmp = tempfile::tempdir().unwrap();
    let root = write_pack(tmp.path(), &valid_pack_files());
    let loaded = load_pack(&root).expect("pack without a tagline loads");
    assert_eq!(loaded.pack.manifest.tagline, None);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn manifest_tagline_is_trimmed_of_surrounding_spaces() {
    let (_tmp, root) = pack_with_manifest(
        "(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    tagline: \"  a tagline  \",\n)",
    );
    let loaded = load_pack(&root).expect("padded tagline loads");
    assert_eq!(loaded.pack.manifest.tagline.as_deref(), Some("a tagline"));
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn empty_or_space_only_tagline_is_absent() {
    for raw in ["\"\"", "\"   \""] {
        let manifest =
            format!("(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    tagline: {raw},\n)");
        let (_tmp, root) = pack_with_manifest(&manifest);
        let loaded = load_pack(&root).expect("empty tagline loads");
        assert_eq!(loaded.pack.manifest.tagline, None, "tagline {raw}");
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    }
}

#[test]
fn forty_eight_char_tagline_does_not_warn() {
    let exact = "x".repeat(48);
    let manifest = format!(
        "(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    tagline: \"{exact}\",\n)"
    );
    let (_tmp, root) = pack_with_manifest(&manifest);
    let loaded = load_pack(&root).expect("48-char tagline loads");
    assert_eq!(
        loaded.pack.manifest.tagline.as_deref(),
        Some(exact.as_str())
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn long_tagline_warns_once_and_still_loads() {
    let long = "x".repeat(49);
    let manifest = format!(
        "(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    tagline: \"{long}\",\n)"
    );
    let (_tmp, root) = pack_with_manifest(&manifest);
    let loaded = load_pack(&root).expect("a long tagline still loads");
    assert_eq!(loaded.pack.manifest.tagline.as_deref(), Some(long.as_str()));
    assert_eq!(
        loaded.warnings.len(),
        1,
        "exactly one warning: {:?}",
        loaded.warnings
    );
    assert!(
        loaded.warnings[0].message.contains("48"),
        "the warning names the 48-character budget: {:?}",
        loaded.warnings
    );
}

#[test]
fn newline_or_control_character_in_tagline_is_a_load_error() {
    for raw in ["\"line\nbreak\"", "\"tab\there\""] {
        let manifest =
            format!("(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    tagline: {raw},\n)");
        let (_tmp, root) = pack_with_manifest(&manifest);
        let errs = load_pack(&root).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.file == "pack.ron" && e.message.contains("tagline")),
            "tagline {raw} fails at load: {errs:?}"
        );
    }
}

#[test]
fn unicode_line_separator_in_tagline_is_a_load_error() {
    let manifest =
        "(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    tagline: \"line\u{2028}break\",\n)";
    let (_tmp, root) = pack_with_manifest(manifest);
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.file == "pack.ron" && e.message.contains("tagline")),
        "{errs:?}"
    );
}

#[test]
fn unknown_manifest_field_still_fails() {
    let (_tmp, root) = pack_with_manifest(
        "(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    taglne: \"typo\",\n)",
    );
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.file == "pack.ron" && e.message.contains("invalid RON")),
        "{errs:?}"
    );
}

// --- manifest title backdrop ---------------------------------------------

#[test]
fn demo_pack_declares_a_title_backdrop() {
    let loaded = load_pack(&demo_pack()).expect("demo pack loads");
    assert_eq!(
        loaded.pack.manifest.title_backdrop.as_deref(),
        Some("school_gate"),
        "the demo pack reuses its stand-in background as the title backdrop"
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn absent_manifest_title_backdrop_is_none() {
    let tmp = tempfile::tempdir().unwrap();
    let root = write_pack(tmp.path(), &valid_pack_files());
    let loaded = load_pack(&root).expect("pack without a title backdrop loads");
    assert_eq!(loaded.pack.manifest.title_backdrop, None);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn a_valid_title_backdrop_id_loads_unchanged() {
    let (_tmp, root) = pack_with_manifest(
        "(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    title_backdrop: \"school_gate\",\n)",
    );
    let loaded = load_pack(&root).expect("a valid backdrop id loads");
    assert_eq!(
        loaded.pack.manifest.title_backdrop.as_deref(),
        Some("school_gate")
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn malformed_title_backdrop_id_is_a_load_error() {
    for raw in [
        "\"\"",
        "\"has space\"",
        "\"has-dash\"",
        "\"has.dot\"",
        "\"../escape\"",
        "\"with/slash\"",
        "\"School_Gate\"",
        "\"UPPER\"",
    ] {
        let manifest = format!(
            "(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    title_backdrop: {raw},\n)"
        );
        let (_tmp, root) = pack_with_manifest(&manifest);
        let errs = load_pack(&root).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.file == "pack.ron" && e.message.contains("title_backdrop")),
            "title_backdrop {raw} must fail at load: {errs:?}"
        );
    }
}

// --- manifest declared scene order ---------------------------------------

/// Write a real temp pack with the given scene ids and a raw manifest `scenes`
/// field. Every scene is a one-line say except ids in `bodies`, which use the
/// supplied source. `scenes_field` is the raw RON value placed after `scenes:`
/// (e.g. `["b", "a"]`), or `None` for an absent field.
fn order_pack(
    entry: &str,
    scenes_field: Option<&str>,
    scene_ids: &[&str],
    bodies: &[(&str, &str)],
) -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let scenes_line = match scenes_field {
        Some(value) => format!("    scenes: {value},\n"),
        None => String::new(),
    };
    fs::write(
        root.join("pack.ron"),
        format!("(\n    name: \"test pack\",\n    entry: \"{entry}\",\n{scenes_line})"),
    )
    .unwrap();
    fs::write(root.join("state.ron"), STATE_RON).unwrap();
    fs::create_dir_all(root.join("scenes")).unwrap();
    fs::create_dir_all(root.join("npcs")).unwrap();
    for id in scene_ids {
        let body = bodies
            .iter()
            .find(|(b_id, _)| b_id == id)
            .map(|(_, source)| source.to_string())
            .unwrap_or_else(|| format!("Narrator: {id}.\n"));
        fs::write(root.join(format!("scenes/{id}.unmei")), body).unwrap();
    }
    (tmp, root)
}

/// The timeline's ids as borrowed strings, for readable order assertions.
fn timeline_ids(pack: &unmei_engine::Pack) -> Vec<&str> {
    pack.timeline_entries().iter().map(String::as_str).collect()
}

#[test]
fn absent_declared_order_has_no_entries_and_no_warnings() {
    let (_tmp, root) = order_pack("scene_a", None, &["scene_a", "scene_b"], &[]);
    let loaded = load_pack(&root).expect("a pack without a scenes list loads");
    assert!(loaded.pack.manifest.scenes.is_empty());
    assert!(loaded.pack.timeline_entries().is_empty());
    assert!(
        loaded.warnings.is_empty(),
        "absent order adds no unlisted warnings: {:?}",
        loaded.warnings
    );
}

#[test]
fn empty_declared_order_matches_absent() {
    let (_tmp, root) = order_pack("scene_a", Some("[]"), &["scene_a", "scene_b"], &[]);
    let loaded = load_pack(&root).expect("`scenes: []` loads");
    assert!(loaded.pack.manifest.scenes.is_empty());
    assert!(loaded.pack.timeline_entries().is_empty());
    assert!(
        loaded.warnings.is_empty(),
        "empty order is no order — no entry requirement, no unlisted warnings: {:?}",
        loaded.warnings
    );
}

#[test]
fn non_empty_declared_order_preserves_authored_order() {
    let (_tmp, root) = order_pack(
        "scene_b",
        Some("[\"scene_c\", \"scene_a\", \"scene_b\"]"),
        &["scene_a", "scene_b", "scene_c"],
        &[],
    );
    let loaded = load_pack(&root).expect("a fully listed pack loads");
    assert_eq!(
        timeline_ids(&loaded.pack),
        ["scene_c", "scene_a", "scene_b"],
        "the manifest's authored order is the timeline order"
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn wrong_typed_declared_order_is_a_load_error() {
    for raw in [
        "\"scene_a\"",
        "[1, 2]",
        "Some([\"scene_a\"])",
        "3",
        "[\"scene_a\", 5]",
    ] {
        let (_tmp, root) = order_pack("scene_a", Some(raw), &["scene_a"], &[]);
        let errs = load_pack(&root).unwrap_err();
        assert!(
            errs.iter().any(|e| e.file == "pack.ron"),
            "scenes {raw} must fail at load: {errs:?}"
        );
    }
}

#[test]
fn listed_missing_id_is_a_load_error_naming_it_and_the_available_ids() {
    let (_tmp, root) = order_pack(
        "scene_a",
        Some("[\"scene_a\", \"ghost\"]"),
        &["scene_a"],
        &[],
    );
    let errs = load_pack(&root).unwrap_err();
    assert_eq!(errs.len(), 1, "{errs:?}");
    let e = &errs[0];
    assert_eq!(e.file, "pack.ron");
    assert_eq!(e.line, None, "manifest diagnostics carry no line number");
    assert!(e.message.contains("`ghost`"), "{e:?}");
    assert!(e.message.contains("not among loaded scenes"), "{e:?}");
    assert!(e.message.contains("scene_a"), "{e:?}");
}

#[test]
fn repeated_id_reports_one_error_per_distinct_repeated_id() {
    let (_tmp, root) = order_pack(
        "scene_a",
        Some("[\"scene_a\", \"scene_a\", \"scene_b\", \"scene_b\", \"scene_b\", \"scene_c\"]"),
        &["scene_a", "scene_b", "scene_c"],
        &[],
    );
    let errs = load_pack(&root).unwrap_err();
    assert_eq!(errs.len(), 2, "one error per repeated id: {errs:?}");
    assert!(
        errs.iter()
            .all(|e| e.file == "pack.ron" && e.line.is_none()),
        "{errs:?}"
    );
    assert!(
        errs[0].message.contains("`scene_a`") && errs[0].message.contains("more than once"),
        "{errs:?}"
    );
    assert!(
        errs[1].message.contains("`scene_b`") && errs[1].message.contains("more than once"),
        "{errs:?}"
    );
}

#[test]
fn missing_and_repeated_id_reports_both() {
    let (_tmp, root) = order_pack("scene_a", Some("[\"ghost\", \"ghost\"]"), &["scene_a"], &[]);
    let errs = load_pack(&root).unwrap_err();
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert!(
        errs.iter()
            .any(|e| e.message.contains("`ghost`") && e.message.contains("more than once")),
        "{errs:?}"
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains("`ghost`") && e.message.contains("not among loaded scenes")),
        "{errs:?}"
    );
}

#[test]
fn case_and_whitespace_are_matched_exactly() {
    for raw in [
        "[\"Scene_a\"]",
        "[\" scene_a\"]",
        "[\"scene_a \"]",
        "[\"SCENE_A\"]",
    ] {
        let (_tmp, root) = order_pack("scene_a", Some(raw), &["scene_a"], &[]);
        let errs = load_pack(&root).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.contains("not among loaded scenes")),
            "scenes {raw} must not match `scene_a`: {errs:?}"
        );
    }
}

#[test]
fn parse_failed_listed_scene_reports_parse_and_order_errors() {
    let (_tmp, root) = order_pack(
        "scene_a",
        Some("[\"scene_a\", \"scene_b\"]"),
        &["scene_a", "scene_b"],
        &[("scene_b", "endif\n")],
    );
    let errs = load_pack(&root).unwrap_err();
    assert!(
        errs.iter().any(|e| e.file == "scenes/scene_b.unmei"),
        "the parse failure is reported: {errs:?}"
    );
    assert!(
        errs.iter().any(|e| e.file == "pack.ron"
            && e.message.contains("`scene_b`")
            && e.message.contains("not among loaded scenes")),
        "the order error is reported too: {errs:?}"
    );
}

#[test]
fn unlisted_scene_loads_warns_once_and_is_absent_from_timeline() {
    let (_tmp, root) = order_pack(
        "scene_a",
        Some("[\"scene_a\"]"),
        &["scene_a", "scene_c"],
        &[("scene_a", "jump scene_c\n")],
    );
    let loaded = load_pack(&root).expect("a jump to an unlisted scene still resolves");
    assert!(
        loaded.pack.scenes.contains_key("scene_c"),
        "the unlisted scene stays loaded and jump-reachable"
    );
    assert_eq!(
        timeline_ids(&loaded.pack),
        ["scene_a"],
        "only declared scenes are enumerated"
    );
    assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
    let warning = &loaded.warnings[0];
    assert_eq!(warning.file, "pack.ron");
    assert!(warning.message.contains("`scene_c`"), "{warning:?}");
    assert!(
        !warning.message.contains("entry"),
        "a non-entry unlisted scene gets the generic warning: {warning:?}"
    );
}

#[test]
fn unlisted_entry_gets_a_dedicated_warning() {
    let (_tmp, root) = order_pack(
        "scene_a",
        Some("[\"scene_b\"]"),
        &["scene_a", "scene_b"],
        &[],
    );
    let loaded = load_pack(&root).expect("an unlisted entry still loads");
    assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
    let warning = &loaded.warnings[0];
    assert_eq!(warning.file, "pack.ron");
    assert!(warning.message.contains("`scene_a`"), "{warning:?}");
    assert!(
        warning.message.contains("entry"),
        "the entry scene's warning is dedicated: {warning:?}"
    );
}

#[test]
fn the_unlisted_entry_warning_replaces_the_generic_one() {
    // The entry scene *and* another scene are unlisted, so both warnings exist:
    // exactly one dedicated for the entry and one generic for the other.
    let (_tmp, root) = order_pack(
        "scene_a",
        Some("[\"scene_c\"]"),
        &["scene_a", "scene_b", "scene_c"],
        &[],
    );
    let loaded = load_pack(&root).expect("the listed scene loads");
    assert_eq!(loaded.warnings.len(), 2, "{:?}", loaded.warnings);
    let entry_warning = loaded
        .warnings
        .iter()
        .find(|w| w.message.contains("`scene_a`"))
        .expect("the unlisted entry warns");
    assert!(
        entry_warning.message.contains("entry"),
        "the entry gets the dedicated warning: {entry_warning:?}"
    );
    let generic_warning = loaded
        .warnings
        .iter()
        .find(|w| w.message.contains("`scene_b`"))
        .expect("the other unlisted scene warns");
    assert!(
        !generic_warning.message.contains("entry"),
        "the other scene gets the generic warning: {generic_warning:?}"
    );
}

#[test]
fn an_outright_missing_entry_adds_no_unlisted_warning() {
    // The entry scene is missing from disk *and* declared, so the only problems
    // are its existence error and the order error; an unlisted warning could
    // not name it because it is not among the loaded scenes.
    let (_tmp, root) = order_pack("ghost", Some("[\"ghost\"]"), &["scene_a"], &[]);
    let errs = load_pack(&root).unwrap_err();
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert!(
        errs.iter()
            .any(|e| e.message.contains("entry scene `ghost`")
                && e.message.contains("does not exist")),
        "{errs:?}"
    );
    assert!(
        errs.iter().any(|e| e.message.contains("`ghost`")
            && e.message.contains("not among loaded scenes")),
        "{errs:?}"
    );
}

#[test]
fn unlisted_warnings_are_sorted_by_scene_id() {
    let (_tmp, root) = order_pack(
        "scene_z",
        Some("[\"scene_z\"]"),
        &["scene_z", "scene_b", "scene_a"],
        &[],
    );
    let loaded = load_pack(&root).expect("the listed entry loads");
    assert_eq!(loaded.warnings.len(), 2, "{:?}", loaded.warnings);
    assert!(
        loaded.warnings[0].message.contains("`scene_a`"),
        "sorted by id: {:?}",
        loaded.warnings
    );
    assert!(
        loaded.warnings[1].message.contains("`scene_b`"),
        "sorted by id: {:?}",
        loaded.warnings
    );
}

#[test]
fn order_errors_are_sorted_by_scene_id() {
    let (_tmp, root) = order_pack(
        "scene_a",
        Some("[\"zeta\", \"alpha\", \"mid\", \"alpha\"]"),
        &["scene_a"],
        &[],
    );
    let errs = load_pack(&root).unwrap_err();
    let messages: Vec<&str> = errs.iter().map(|e| e.message.as_str()).collect();
    let dup = messages
        .iter()
        .position(|m| m.contains("`alpha`") && m.contains("more than once"))
        .expect("alpha is repeated");
    let alpha_missing = messages
        .iter()
        .position(|m| m.contains("`alpha`") && m.contains("not among loaded scenes"))
        .expect("alpha is missing");
    let mid = messages
        .iter()
        .position(|m| m.contains("`mid`"))
        .expect("mid is missing");
    let zeta = messages
        .iter()
        .position(|m| m.contains("`zeta`"))
        .expect("zeta is missing");
    assert!(
        dup < alpha_missing && alpha_missing < mid && mid < zeta,
        "diagnostics sorted by scene id: {messages:?}"
    );
}

// --- state RON bridge diagnostics and optional-string consumers -----------

/// Every state read failure is surfaced once, verbatim from the bridge, with
/// `state.ron` context and no line number, and the loader returns early (its
/// existing policy). Comparing against `decode_state` keeps this a wiring
/// witness rather than a second copy of the bridge's message assertions.
#[test]
fn state_read_failures_carry_state_ron_context_once() {
    let cases = [
        "( affinity_level: )",
        "\"not a map\"",
        "{ 1: \"x\" }",
        "{ \"affinity_level\": [1] }",
    ];
    for state_ron in cases {
        let tmp = tempfile::tempdir().unwrap();
        let mut files = valid_pack_files();
        files[1] = ("state.ron", state_ron.to_string());
        let root = write_pack(tmp.path(), &files);
        let errs = load_pack(&root).unwrap_err();
        assert_eq!(
            errs.len(),
            1,
            "one early-return error for {state_ron}: {errs:?}"
        );
        assert_eq!(errs[0].file, "state.ron", "{state_ron}: {errs:?}");
        assert_eq!(errs[0].line, None, "{state_ron}: {errs:?}");
        assert_eq!(
            errs[0].message,
            unmei_engine::pack_ron::decode_state(state_ron).unwrap_err(),
            "the loader surfaces the bridge message verbatim: {state_ron}"
        );
    }
}

/// The content optional-string consumers (NPC `model`, manifest `tagline` and
/// `title_backdrop`) still accept a plain string, unit `()` and `None`, and
/// still tolerate the legacy `Some("…")` wrapper, through the real loader.
#[test]
fn content_optional_strings_accept_plain_unit_none_and_legacy_wrapper() {
    let legacy = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[0] = (
        "pack.ron",
        "(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    tagline: Some(\"legacy tagline\"),\n    title_backdrop: Some(\"school_gate\"),\n)"
            .to_string(),
    );
    files[3] = (
        "npcs/ace.ron",
        ACE_RON.replace(
            "model: \"typesafe/jev-1.13\"",
            "model: Some(\"typesafe/jev-1.13\")",
        ),
    );
    let root = write_pack(legacy.path(), &files);
    let pack = load_pack(&root)
        .expect("legacy-wrapped optional strings load")
        .pack;
    assert_eq!(pack.manifest.tagline.as_deref(), Some("legacy tagline"));
    assert_eq!(pack.manifest.title_backdrop.as_deref(), Some("school_gate"));
    assert_eq!(pack.npcs["ace"].model.as_deref(), Some("typesafe/jev-1.13"));

    let absent = tempfile::tempdir().unwrap();
    let mut files = valid_pack_files();
    files[0] = (
        "pack.ron",
        "(\n    name: \"test pack\",\n    entry: \"scene_a\",\n    tagline: (),\n    title_backdrop: None,\n)"
            .to_string(),
    );
    files[3] = (
        "npcs/ace.ron",
        ACE_RON.replace("model: \"typesafe/jev-1.13\"", "model: ()"),
    );
    let root = write_pack(absent.path(), &files);
    let pack = load_pack(&root).expect("unit and None mean absent").pack;
    assert_eq!(pack.manifest.tagline, None);
    assert_eq!(pack.manifest.title_backdrop, None);
    assert_eq!(pack.npcs["ace"].model, None);
}

/// A single leading U+FEFF is not content: each pack text file reads exactly
/// like the same file without it, warnings and line numbers included.
#[test]
fn bom_files_load_like_their_originals() {
    let baseline = {
        let tmp = tempfile::tempdir().unwrap();
        let root = write_pack(tmp.path(), &valid_pack_files());
        load_pack(&root).expect("the original pack loads")
    };
    let baseline_scene = &baseline.pack.scenes["scene_a"];

    for (rel, _) in valid_pack_files() {
        if rel == "assets/README.md" {
            continue;
        }
        let tmp = tempfile::tempdir().unwrap();
        let files: Vec<(&str, String)> = valid_pack_files()
            .into_iter()
            .map(|(name, text)| {
                if name == rel {
                    (name, format!("\u{feff}{text}"))
                } else {
                    (name, text)
                }
            })
            .collect();
        let root = write_pack(tmp.path(), &files);
        let loaded = load_pack(&root)
            .unwrap_or_else(|errors| panic!("{rel} with a leading BOM: {errors:?}"));

        assert_eq!(
            loaded.warnings, baseline.warnings,
            "{rel} loaded differently with a BOM"
        );
        assert_eq!(loaded.pack.manifest, baseline.pack.manifest, "{rel}");
        assert_eq!(loaded.pack.state, baseline.pack.state, "{rel}");
        assert_eq!(loaded.pack.npcs, baseline.pack.npcs, "{rel}");
        let scene = &loaded.pack.scenes["scene_a"];
        assert_eq!(scene.lines, baseline_scene.lines, "{rel}");
        assert_eq!(scene.line_numbers, baseline_scene.line_numbers, "{rel}");
    }
}

/// Only the file's first character can be a byte order mark; a U+FEFF anywhere
/// else is text the parser keeps.
#[test]
fn a_byte_order_mark_inside_the_text_is_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let files = vec![
        ("pack.ron", PACK_RON.to_string()),
        ("state.ron", STATE_RON.to_string()),
        (
            "scenes/scene_a.unmei",
            "\u{feff}# scene a\nNarrator: before\u{feff}after\n".to_string(),
        ),
        ("npcs/ace.ron", ACE_RON.to_string()),
    ];
    let root = write_pack(tmp.path(), &files);
    let loaded = load_pack(&root).expect("the pack loads");

    let scene = &loaded.pack.scenes["scene_a"];
    assert_eq!(
        scene.lines,
        vec![Line::Say {
            speaker: "Narrator".into(),
            text: "before\u{feff}after".into(),
        }]
    );
    assert_eq!(
        scene.line_numbers,
        vec![2],
        "the leading mark stays out of the numbering"
    );
}
