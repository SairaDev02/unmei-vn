//! Condition truth at each arm: equality, ordering, nesting and `set` order.

use super::*;

#[test]
fn string_equality_takes_the_matching_arm_and_skips_the_other() {
    let pack = test_pack(
        &[(
            "scene",
            "if affinity_level == \"high\"\nbg warm_bg\nNarrator: High.\nelse\nbg cold_bg\nNarrator: Low.\nendif\nNarrator: After.\n",
        )],
        HashMap::from([(
            "affinity_level".to_string(),
            StateValue::Str("high".to_string()),
        )]),
    );
    let mut runner = ScriptRunner::new(pack);

    let taken = runner.run();
    assert_eq!(taken.staged, vec![StageCommand::Bg("warm_bg".to_string())]);
    assert_eq!(say_text(taken.wait.as_ref().unwrap()).1, "High.");

    // The untaken arm staged nothing; the script continues after `endif`.
    runner.advance_say().unwrap();
    let after = runner.run();
    assert_eq!(say_text(after.wait.as_ref().unwrap()).1, "After.");
    assert!(after.staged.is_empty());
}

#[test]
fn false_condition_takes_the_else_arm() {
    let pack = test_pack(
        &[(
            "scene",
            "if affinity_level == \"high\"\nbg warm_bg\nNarrator: High.\nelse\nbg cold_bg\nNarrator: Low.\nendif\n",
        )],
        HashMap::from([(
            "affinity_level".to_string(),
            StateValue::Str("low".to_string()),
        )]),
    );
    let mut runner = ScriptRunner::new(pack);
    let outcome = runner.run();
    assert_eq!(
        outcome.staged,
        vec![StageCommand::Bg("cold_bg".to_string())]
    );
    assert_eq!(say_text(outcome.wait.as_ref().unwrap()).1, "Low.");
}

#[test]
fn number_and_bool_equality() {
    let pack = test_pack(
        &[(
            "scene",
            "if letters_found == 2\nNarrator: Two.\nendif\nif met_ace == true\nNarrator: Met.\nendif\n",
        )],
        HashMap::from([
            ("letters_found".to_string(), StateValue::Num(2.0)),
            ("met_ace".to_string(), StateValue::Bool(true)),
        ]),
    );
    let mut runner = ScriptRunner::new(pack);
    let first = runner.run();
    assert_eq!(say_text(first.wait.as_ref().unwrap()).1, "Two.");
    runner.advance_say().unwrap();
    let second = runner.run();
    assert_eq!(say_text(second.wait.as_ref().unwrap()).1, "Met.");
}

#[test]
fn numeric_ordering_picks_the_right_arm_at_the_boundary() {
    for (op, actual, expected) in [
        (">", 2.0, "Taken."),
        (">", 1.0, "Zero."),
        (">=", 1.0, "Taken."),
        (">=", 0.0, "Zero."),
        ("<", 0.0, "Taken."),
        ("<", 1.0, "Zero."),
        ("<=", 1.0, "Taken."),
        ("<=", 2.0, "Zero."),
    ] {
        let src =
            format!("if letters_found {op} 1\nNarrator: Taken.\nelse\nNarrator: Zero.\nendif\n");
        let pack = test_pack(
            &[("scene", src.as_str())],
            HashMap::from([("letters_found".to_string(), StateValue::Num(actual))]),
        );
        let mut runner = ScriptRunner::new(pack);
        let outcome = runner.run();
        assert_eq!(
            say_text(outcome.wait.as_ref().unwrap()).1,
            expected,
            "`letters_found ({actual}) {op} 1`"
        );
    }
}

#[test]
fn ordering_on_a_mismatched_kind_is_false_not_a_panic() {
    // Load validation rejects this for real packs; a hand-built scene must
    // still stay total.
    let pack = test_pack(
        &[(
            "scene",
            "if affinity_level > 1\nbg wrong\nNarrator: Wrong.\nendif\nNarrator: After.\n",
        )],
        HashMap::from([(
            "affinity_level".to_string(),
            StateValue::Str("high".to_string()),
        )]),
    );
    let mut runner = ScriptRunner::new(pack);
    let outcome = runner.run();
    assert!(outcome.staged.is_empty());
    assert_eq!(say_text(outcome.wait.as_ref().unwrap()).1, "After.");
}

#[test]
fn bare_bool_key_is_eq_true_and_a_false_key_skips_the_arm() {
    let scene = "if met_ace\nbg wrong\nNarrator: Met.\nendif\nNarrator: After.\n";
    let true_pack = test_pack(
        &[("scene", scene)],
        HashMap::from([("met_ace".to_string(), StateValue::Bool(true))]),
    );
    let mut runner = ScriptRunner::new(true_pack);
    let taken = runner.run();
    assert_eq!(taken.staged, vec![StageCommand::Bg("wrong".to_string())]);
    assert_eq!(say_text(taken.wait.as_ref().unwrap()).1, "Met.");

    let false_pack = test_pack(
        &[("scene", scene)],
        HashMap::from([("met_ace".to_string(), StateValue::Bool(false))]),
    );
    let mut runner = ScriptRunner::new(false_pack);
    let skipped = runner.run();
    assert!(skipped.staged.is_empty());
    assert_eq!(say_text(skipped.wait.as_ref().unwrap()).1, "After.");
}

#[test]
fn inequality_negates_the_match() {
    let pack = test_pack(
        &[(
            "scene",
            "if affinity_level != \"high\"\nbg not_high\nNarrator: Not high.\nelse\nbg high\nNarrator: High.\nendif\n",
        )],
        HashMap::from([(
            "affinity_level".to_string(),
            StateValue::Str("low".to_string()),
        )]),
    );
    let mut runner = ScriptRunner::new(pack);
    let outcome = runner.run();
    assert_eq!(
        outcome.staged,
        vec![StageCommand::Bg("not_high".to_string())]
    );
    assert_eq!(say_text(outcome.wait.as_ref().unwrap()).1, "Not high.");
}

#[test]
fn inner_false_if_inside_a_taken_then_arm_skips_the_outer_else() {
    // Outer then-arm taken; the inner `if` (no else) is false, so it must
    // land on the outer `else` marker and skip the outer else arm entirely.
    let pack = test_pack(
        &[(
            "scene",
            "if met_ace\nif affinity_level == \"high\"\nbg inner_high\nNarrator: Both.\nendif\nelse\nbg outer_else\nNarrator: Outer else.\nendif\nNarrator: After.\n",
        )],
        HashMap::from([
            ("met_ace".to_string(), StateValue::Bool(true)),
            (
                "affinity_level".to_string(),
                StateValue::Str("low".to_string()),
            ),
        ]),
    );
    let mut runner = ScriptRunner::new(pack);
    let outcome = runner.run();
    assert!(outcome.staged.is_empty());
    assert_eq!(say_text(outcome.wait.as_ref().unwrap()).1, "After.");
}

#[test]
fn an_untaken_arm_sets_nothing() {
    let pack = test_pack(
        &[(
            "scene",
            "if met_ace\nset letters_found = 1\nelse\nset letters_found = 2\nendif\nNarrator: Done.\n",
        )],
        HashMap::from([
            ("met_ace".to_string(), StateValue::Bool(true)),
            ("letters_found".to_string(), StateValue::Num(0.0)),
        ]),
    );
    let mut runner = ScriptRunner::new(pack);
    runner.run();
    assert_eq!(
        runner.state()["letters_found"],
        StateValue::Num(1.0),
        "only the taken arm's `set` runs"
    );
}

#[test]
fn kind_mismatch_evaluates_false_without_panicking() {
    // Kind validation is the fixed-state-kind ticket's job; until then a
    // mismatched comparison must merely be false, never a panic.
    let pack = test_pack(
        &[(
            "scene",
            "if affinity_level == 3\nbg wrong\nNarrator: Wrong.\nendif\nNarrator: After.\n",
        )],
        HashMap::from([(
            "affinity_level".to_string(),
            StateValue::Str("high".to_string()),
        )]),
    );
    let mut runner = ScriptRunner::new(pack);
    let outcome = runner.run();
    assert!(outcome.staged.is_empty());
    assert_eq!(say_text(outcome.wait.as_ref().unwrap()).1, "After.");
}

#[test]
fn nested_if_inside_an_else_arm_plays() {
    let scene = "if met_ace\nNarrator: Met.\nelse\nif affinity_level == \"high\"\nNarrator: High.\nelse\nNarrator: Not high.\nendif\nendif\nNarrator: After.\n";

    // `met_ace` false -> outer else arm -> inner then-arm.
    let high = test_pack(
        &[("scene", scene)],
        HashMap::from([
            ("met_ace".to_string(), StateValue::Bool(false)),
            (
                "affinity_level".to_string(),
                StateValue::Str("high".to_string()),
            ),
        ]),
    );
    let mut runner = ScriptRunner::new(high);
    let first = runner.run();
    assert_eq!(say_text(first.wait.as_ref().unwrap()).1, "High.");
    assert_eq!(say_after(&mut runner), "After.");

    // `met_ace` false -> outer else arm -> inner else-arm.
    let low = test_pack(
        &[("scene", scene)],
        HashMap::from([
            ("met_ace".to_string(), StateValue::Bool(false)),
            (
                "affinity_level".to_string(),
                StateValue::Str("low".to_string()),
            ),
        ]),
    );
    let mut runner = ScriptRunner::new(low);
    let first = runner.run();
    assert_eq!(say_text(first.wait.as_ref().unwrap()).1, "Not high.");
    assert_eq!(say_after(&mut runner), "After.");

    // `met_ace` true -> outer then-arm; the whole nested block is skipped.
    let met = test_pack(
        &[("scene", scene)],
        HashMap::from([
            ("met_ace".to_string(), StateValue::Bool(true)),
            (
                "affinity_level".to_string(),
                StateValue::Str("high".to_string()),
            ),
        ]),
    );
    let mut runner = ScriptRunner::new(met);
    let first = runner.run();
    assert_eq!(say_text(first.wait.as_ref().unwrap()).1, "Met.");
    assert_eq!(say_after(&mut runner), "After.");
}

#[test]
fn three_deep_nesting_plays_and_a_set_in_the_innermost_arm_affects_a_later_condition() {
    let scene = "if met_ace\nNarrator: Met.\nelse\nif affinity_level == \"high\"\nif letters_found == 0\nset letters_found = 9\nNarrator: Deep.\nelse\nNarrator: Not deep.\nendif\nelse\nNarrator: Not high.\nendif\nendif\nNarrator: After.\nif letters_found == 9\nNarrator: Nine.\nelse\nNarrator: Not nine.\nendif\n";

    // outer else -> middle then -> inner then: the depth-3 arm runs.
    let deep = test_pack(
        &[("scene", scene)],
        HashMap::from([
            ("met_ace".to_string(), StateValue::Bool(false)),
            (
                "affinity_level".to_string(),
                StateValue::Str("high".to_string()),
            ),
            ("letters_found".to_string(), StateValue::Num(0.0)),
        ]),
    );
    let mut runner = ScriptRunner::new(deep);
    let first = runner.run();
    assert_eq!(say_text(first.wait.as_ref().unwrap()).1, "Deep.");
    assert_eq!(runner.state()["letters_found"], StateValue::Num(9.0));
    // The innermost arm's `set` feeds the condition after the whole block.
    assert_eq!(say_after(&mut runner), "After.");
    runner.advance_say().unwrap();
    let nine = runner.run();
    assert_eq!(say_text(nine.wait.as_ref().unwrap()).1, "Nine.");

    // outer else -> middle then -> inner else: the innermost arm is skipped
    // and its `set` never runs.
    let skip = test_pack(
        &[("scene", scene)],
        HashMap::from([
            ("met_ace".to_string(), StateValue::Bool(false)),
            (
                "affinity_level".to_string(),
                StateValue::Str("high".to_string()),
            ),
            ("letters_found".to_string(), StateValue::Num(5.0)),
        ]),
    );
    let mut runner = ScriptRunner::new(skip);
    let first = runner.run();
    assert_eq!(say_text(first.wait.as_ref().unwrap()).1, "Not deep.");
    assert_eq!(runner.state()["letters_found"], StateValue::Num(5.0));
    assert_eq!(say_after(&mut runner), "After.");
    runner.advance_say().unwrap();
    let not_nine = runner.run();
    assert_eq!(say_text(not_nine.wait.as_ref().unwrap()).1, "Not nine.");
}

#[test]
fn set_in_an_arm_affects_later_conditions_not_its_own_test() {
    let pack = test_pack(
        &[(
            "scene",
            "if met_ace\nset met_ace = false\nNarrator: Inside.\nendif\nif met_ace\nNarrator: Still met.\nelse\nNarrator: No longer met.\nendif\n",
        )],
        HashMap::from([("met_ace".to_string(), StateValue::Bool(true))]),
    );
    let mut runner = ScriptRunner::new(pack);
    // The `if met_ace` test ran before the `set`, so the arm is entered.
    let inside = runner.run();
    assert_eq!(say_text(inside.wait.as_ref().unwrap()).1, "Inside.");
    // The later condition reads the value the arm just set.
    let later = say_after(&mut runner);
    assert_eq!(later, "No longer met.");
    assert_eq!(runner.state()["met_ace"], StateValue::Bool(false));
}

#[test]
fn choice_inside_an_arm_presents_and_choosing_leaves_the_block() {
    let scene = "if met_ace\nchoice\n  \"Ask\" -> ask\n  \"Leave\" -> leave\nend\nelse\nNarrator: Skipped.\nendif\nlabel ask\nNarrator: Asked.\nlabel leave\nNarrator: Left.\n";

    let taken = test_pack(
        &[("scene", scene)],
        HashMap::from([("met_ace".to_string(), StateValue::Bool(true))]),
    );
    let mut runner = ScriptRunner::new(taken);
    match runner.run().wait.unwrap() {
        Wait::Choice { options } => {
            assert_eq!(options.len(), 2);
            assert_eq!(options[0].1, "ask");
        }
        other => panic!("expected a choice, got {other:?}"),
    }
    // Choosing resolves like `jump`, leaving the block for a top-level label.
    runner.choose(1).unwrap();
    let landed = runner.run();
    assert_eq!(say_text(landed.wait.as_ref().unwrap()).1, "Left.");

    let skipped = test_pack(
        &[("scene", scene)],
        HashMap::from([("met_ace".to_string(), StateValue::Bool(false))]),
    );
    let mut runner = ScriptRunner::new(skipped);
    let outcome = runner.run();
    assert_eq!(say_text(outcome.wait.as_ref().unwrap()).1, "Skipped.");
}

#[test]
fn decide_inside_a_nested_arm_parks_and_resolves() {
    let scene = "if met_ace\nif affinity_level == \"high\"\ndecide ace.greet -> reply\nelse\nNarrator: Low.\nendif\nelse\nNarrator: Skipped.\nendif\nlabel reply.warm\nNarrator: Warm.\nlabel reply.cold\nNarrator: Cold.\n";
    let pack = test_pack(
        &[("scene", scene)],
        HashMap::from([
            ("met_ace".to_string(), StateValue::Bool(true)),
            (
                "affinity_level".to_string(),
                StateValue::Str("high".to_string()),
            ),
        ]),
    );
    let mut runner = ScriptRunner::new(pack);
    match runner.run().wait.unwrap() {
        Wait::Decision {
            npc_id,
            question,
            prefix,
        } => assert_eq!(
            (npc_id.as_str(), question.as_str(), prefix.as_str()),
            ("ace", "greet", "reply")
        ),
        other => panic!("expected a decision wait, got {other:?}"),
    }
    runner.resolve_decision("warm").unwrap();
    let landed = runner.run();
    assert_eq!(say_text(landed.wait.as_ref().unwrap()).1, "Warm.");
}

#[test]
fn jump_out_of_an_arm_resolves_label_then_scene_file() {
    // A matching label wins and stays in the scene.
    let local = test_pack(
        &[(
            "scene",
            "if met_ace\njump after\nendif\nNarrator: Not jumped.\nlabel after\nNarrator: After.\n",
        )],
        HashMap::from([("met_ace".to_string(), StateValue::Bool(true))]),
    );
    let mut runner = ScriptRunner::new(local);
    let outcome = runner.run();
    assert_eq!(say_text(outcome.wait.as_ref().unwrap()).1, "After.");
    assert_eq!(runner.scene_id(), "scene");

    // No matching label -> the scene file from the top.
    let cross = test_pack(
        &[
            (
                "scene",
                "if met_ace\njump scene_b\nendif\nNarrator: Stayed.\n",
            ),
            ("scene_b", "Narrator: B.\n"),
        ],
        HashMap::from([("met_ace".to_string(), StateValue::Bool(true))]),
    );
    let mut runner = ScriptRunner::new(cross);
    let outcome = runner.run();
    assert_eq!(say_text(outcome.wait.as_ref().unwrap()).1, "B.");
    assert_eq!(runner.scene_id(), "scene_b");
}

#[test]
fn entered_scene_marks_only_cross_scene_jumps() {
    // A jump landing in a different scene file reports the scene it entered.
    let cross = test_pack(
        &[("scene", "jump scene_b\n"), ("scene_b", "Narrator: B.\n")],
        HashMap::new(),
    );
    let mut runner = ScriptRunner::new(cross);
    let outcome = runner.run();
    assert_eq!(outcome.entered_scene.as_deref(), Some("scene_b"));
    assert_eq!(runner.scene_id(), "scene_b");

    // A local-label jump stays in the scene and reports nothing.
    let local = test_pack(
        &[(
            "scene",
            "jump after\nNarrator: Skipped.\nlabel after\nNarrator: Here.\n",
        )],
        HashMap::new(),
    );
    let mut runner = ScriptRunner::new(local);
    let outcome = runner.run();
    assert_eq!(outcome.entered_scene, None);
    assert_eq!(runner.scene_id(), "scene");

    // The initial run and a jump to the current scene's own id both report
    // nothing: the first parks on the choice, the second re-enters the scene.
    let same = test_pack(
        &[(
            "scene",
            "choice\n  \"Again\" -> scene\nend\nNarrator: Done.\n",
        )],
        HashMap::new(),
    );
    let mut runner = ScriptRunner::new(same);
    let parked = runner.run();
    assert!(matches!(parked.wait, Some(Wait::Choice { .. })));
    assert_eq!(parked.entered_scene, None);

    runner.choose(0).unwrap();
    let again = runner.run();
    assert_eq!(again.entered_scene, None);
    assert_eq!(runner.scene_id(), "scene");
    assert_eq!(runner.pc(), 1);
}
