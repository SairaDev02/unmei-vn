//! The demo pack's epilogue played live, both routes and both nested arms.

use super::*;

#[test]
fn demo_epilogue_plays_the_else_arm_for_the_shipped_state() {
    // The shipped `state.ron` declares `affinity_level: "low"`, so the
    // string-equality block takes the else arm; `met_ace: false` skips the
    // bare-bool block and the script ends.
    let mut runner = demo_pack_at_epilogue();

    let outcome = runner.run();
    assert_eq!(
        outcome.staged,
        vec![
            StageCommand::Bg("school_gate".to_string()),
            StageCommand::Play("music_evening".to_string()),
        ]
    );
    let (speaker, text) = say_text(outcome.wait.as_ref().unwrap());
    assert_eq!(speaker, "Narrator");
    assert_eq!(text, "She is already walking away.");

    runner.advance_say().unwrap();
    assert_eq!(runner.run().wait, None, "the epilogue ends after `stop`");
}

#[test]
fn demo_epilogue_high_arm_plays_the_nested_letters_arm() {
    // "Ask about the letter" sets `affinity_level = "high"` and
    // `letters_found = 1`, so both the outer then arm and the nested
    // `letters_found > 0` arm play, and the bare `met_ace` bool is true.
    let (mut runner, _, text) = play_demo_to_epilogue(0);
    assert_eq!(text, "She lingers by the gate, in no hurry to leave.");

    runner.advance_say().unwrap();
    let nested = runner.run();
    assert_eq!(
        say_text(nested.wait.as_ref().unwrap()).1,
        "Keep it. It suits you."
    );
    assert_eq!(say_after(&mut runner), "You will see her again.");

    runner.advance_say().unwrap();
    assert_eq!(runner.run().wait, None, "the epilogue ends after `stop`");
}

#[test]
fn demo_epilogue_low_arm_is_reachable_after_the_silent_route() {
    // "Stay silent" sets `affinity_level = "low"` and leaves
    // `letters_found = 0`, so the else arm plays; `met_ace` is now true.
    let (mut runner, _, text) = play_demo_to_epilogue(1);
    assert_eq!(text, "She is already walking away.");
    assert_eq!(
        runner.state()["affinity_level"],
        StateValue::Str("low".to_string())
    );
    assert_eq!(say_after(&mut runner), "You will see her again.");
}

#[test]
fn demo_epilogue_nested_letters_else_arm_plays() {
    // High affinity with no letter keeps the outer then arm but takes the
    // nested else arm — the fixture's one nesting level, both sides.
    let pack = demo_pack();
    let mut state = pack.state.clone();
    state.insert(
        "affinity_level".to_string(),
        StateValue::Str("high".to_string()),
    );
    state.insert("letters_found".to_string(), StateValue::Num(0.0));
    state.insert("met_ace".to_string(), StateValue::Bool(true));
    let mut runner = ScriptRunner::new(pack);
    runner
        .restore_position("epilogue".to_string(), 0, state, None)
        .unwrap();

    let outcome = runner.run();
    assert_eq!(
        say_text(outcome.wait.as_ref().unwrap()).1,
        "She lingers by the gate, in no hurry to leave."
    );
    runner.advance_say().unwrap();
    let nested = runner.run();
    assert_eq!(
        say_text(nested.wait.as_ref().unwrap()).1,
        "Next time, bring me something."
    );
    assert_eq!(say_after(&mut runner), "You will see her again.");
}
