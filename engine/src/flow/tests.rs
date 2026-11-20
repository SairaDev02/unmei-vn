//! Flow lint tests: each rule's warning, the reachability the lint must not
//! report, and the demo pack's clean graph (check and preview spec §6, §13
//! rules 13–15).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::content::{Manifest, Npc, Pack};
use crate::finding::Severity;
use crate::parser::Parser;

use super::findings;

/// A pack whose manifest enters `entry`, holding the given scene sources.
fn test_pack(entry: &str, scenes: &[(&str, &str)]) -> Pack {
    let mut parsed = HashMap::new();
    for (id, src) in scenes {
        let scene = Parser::new(&format!("scenes/{id}.unmei"))
            .parse(id, src)
            .unwrap_or_else(|finding| panic!("scene `{id}` parses: {finding}"));
        parsed.insert(id.to_string(), scene);
    }
    Pack {
        manifest: Manifest {
            name: "flow".to_string(),
            entry: entry.to_string(),
            tagline: None,
            title_backdrop: None,
            scenes: Vec::new(),
        },
        state: HashMap::new(),
        scenes: parsed,
        npcs: HashMap::new(),
        root: PathBuf::from("."),
    }
}

/// One `choice` question whose landing keys are `warm`, `cold` and `none`.
fn ace() -> Npc {
    ron::from_str(
        r#"(
            id: "ace",
            name: "Ace",
            decisions: {
                "greet": (
                    instructions: "How does Ace greet the player?",
                    state_keys: [],
                    criteria: {
                        "warm": "A warm greeting.",
                        "cold": "A cold greeting.",
                        "none": "Neither fits.",
                    },
                    fallback: "cold",
                ),
            },
        )"#,
    )
    .expect("the test NPC decodes")
}

/// `file|path|rule` for every finding, after checking each one is a warning:
/// flow lint never reports an error, so it never fails the check on its own.
fn warnings(pack: &Pack) -> Vec<String> {
    let found = findings(pack);
    for finding in &found {
        assert_eq!(finding.severity, Severity::Warning, "{finding}");
    }
    found
        .iter()
        .map(|finding| format!("{}|{}|{}", finding.file, finding.path, finding.rule))
        .collect()
}

/// Rule 13: a scene entered only through a decision landing is reachable; a
/// scene nothing reaches is one `flow.unreachable-scene` warning.
#[test]
fn a_scene_a_decision_landing_reaches_is_reachable() {
    let mut pack = test_pack(
        "main",
        &[
            (
                "main",
                "Narrator: The gate.\n\
                 decide ace.greet -> reply\n\
                 label reply.warm\n\
                 jump greeted_warmly\n\
                 label reply.cold\n\
                 jump greeted_coldly\n\
                 label reply.none\n\
                 jump greeted_coldly\n",
            ),
            ("greeted_warmly", "Narrator: Warm.\n"),
            ("greeted_coldly", "Narrator: Cold.\n"),
            ("orphan", "Narrator: Nobody comes here.\n"),
        ],
    );
    pack.npcs.insert("ace".to_string(), ace());
    assert_eq!(
        warnings(&pack),
        ["scenes/orphan.unmei||flow.unreachable-scene"]
    );
}

/// Rule 14: a `jump` past a label leaves it unreachable; a label the walk falls
/// through and one the jump lands on are reachable.
#[test]
fn a_label_a_jump_skips_is_unreachable() {
    let pack = test_pack(
        "main",
        &[(
            "main",
            "Narrator: Start.\n\
             label opening\n\
             Narrator: Still here.\n\
             jump after\n\
             label skipped\n\
             Narrator: Never shown.\n\
             label after\n\
             Narrator: End.\n",
        )],
    );
    assert_eq!(
        warnings(&pack),
        ["scenes/main.unmei|line 5|flow.unreachable-label"]
    );
}

/// Rule 15: a scene whose every path loops back through a choice can never end;
/// a choice with an option that leaves the scene can.
#[test]
fn a_scene_that_only_loops_back_has_no_exit() {
    let pack = test_pack(
        "start",
        &[
            (
                "start",
                "Narrator: Where to?\n\
                 choice\n\
                 \"Wander\" -> wander\n\
                 \"Leave\" -> leaving\n\
                 end\n\
                 label wander\n\
                 jump loop\n",
            ),
            (
                "loop",
                "label top\n\
                 choice\n\
                 \"One\" -> top\n\
                 \"Two\" -> top\n\
                 end\n",
            ),
            ("leaving", "Narrator: You leave.\n"),
        ],
    );
    assert_eq!(warnings(&pack), ["scenes/loop.unmei||flow.no-exit"]);
}

/// A label inside an unreachable scene is not reported again: the scene's one
/// warning is what the author acts on.
#[test]
fn a_label_inside_an_unreachable_scene_is_not_reported_again() {
    let pack = test_pack(
        "main",
        &[
            ("main", "Narrator: Alone.\n"),
            (
                "orphan",
                "jump past\n\
                 label skipped\n\
                 Narrator: Never shown.\n\
                 label past\n\
                 Narrator: End.\n",
            ),
        ],
    );
    assert_eq!(
        warnings(&pack),
        ["scenes/orphan.unmei||flow.unreachable-scene"]
    );
}

/// `jump <own scene id>` restarts the file rather than leaving it, so a scene
/// that only does that has no exit.
#[test]
fn a_jump_to_the_scenes_own_id_does_not_exit() {
    let pack = test_pack(
        "main",
        &[("main", "label top\nNarrator: Again.\njump main\n")],
    );
    assert_eq!(warnings(&pack), ["scenes/main.unmei||flow.no-exit"]);
}

/// Both `if` arms count as reachable: the else arm reaches the label after the
/// block even though the then arm jumps away.
#[test]
fn the_else_arm_is_reachable_from_the_conditional() {
    let pack = test_pack(
        "main",
        &[
            (
                "main",
                "if met_ace\n\
                 jump elsewhere\n\
                 else\n\
                 Narrator: The other arm.\n\
                 endif\n\
                 label after\n\
                 Narrator: End.\n",
            ),
            ("elsewhere", "Narrator: Elsewhere.\n"),
        ],
    );
    assert!(warnings(&pack).is_empty(), "{:?}", warnings(&pack));
}

/// A false condition without an else arm resumes after the block, so the label
/// there is reachable even though the then arm jumps away.
#[test]
fn a_false_condition_without_else_resumes_after_the_block() {
    let pack = test_pack(
        "main",
        &[
            (
                "main",
                "if met_ace\n\
                 jump elsewhere\n\
                 endif\n\
                 label after\n\
                 Narrator: End.\n",
            ),
            ("elsewhere", "Narrator: Elsewhere.\n"),
        ],
    );
    assert!(warnings(&pack).is_empty(), "{:?}", warnings(&pack));
}

/// An entry scene with no line is reachable and ends at once: no warning.
#[test]
fn an_empty_entry_scene_exits_at_its_first_position() {
    let pack = test_pack("main", &[("main", "# only a comment\n")]);
    assert!(warnings(&pack).is_empty());
}

/// The demo pack earns no flow warning: its entry scene reaches every scene,
/// every label is reachable state-blind, and each scene ends at end of file or a
/// jump (spec rule 19).
#[test]
fn the_demo_pack_earns_no_flow_warning() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
    let pack = crate::load_pack(&root).expect("the demo pack loads").pack;
    assert!(warnings(&pack).is_empty(), "{:?}", warnings(&pack));
}
