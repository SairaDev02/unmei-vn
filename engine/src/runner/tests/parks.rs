//! Say, choice and decision parks: staging effects, branch targets, and the
//! errors a mis-indexed choice or an unknown action lands on.

use super::*;

#[test]
fn parks_on_say_then_choice_with_staging() {
    let mut runner = ScriptRunner::new(demo_pack());

    let first = runner.run();
    let (speaker, text) = say_text(first.wait.as_ref().unwrap());
    assert_eq!(speaker, "Narrator");
    assert!(text.starts_with("Morning light."));
    assert_eq!(
        first.staged,
        vec![
            StageCommand::Bg("school_gate".to_string()),
            StageCommand::Play("music_morning".to_string()),
        ]
    );

    runner.advance_say().unwrap();
    let second = runner.run();
    let (speaker, _) = say_text(second.wait.as_ref().unwrap());
    assert_eq!(speaker, "Ace");
    assert_eq!(
        second.staged,
        vec![StageCommand::Sprite {
            npc_id: "ace".to_string(),
            pose: "neutral".to_string(),
        }]
    );

    runner.advance_say().unwrap();
    let third = runner.run();
    match third.wait.unwrap() {
        Wait::Choice { options } => {
            assert_eq!(options.len(), 2);
            assert_eq!(options[0].1, "ask_letter");
        }
        other => panic!("expected a choice, got {other:?}"),
    }
}

#[test]
fn choose_branches_and_crosses_into_a_scene_file() {
    let mut runner = ScriptRunner::new(demo_pack());
    // Narrator -> Ace -> choice
    runner.run();
    runner.advance_say().unwrap();
    runner.run();
    runner.advance_say().unwrap();
    let choice = runner.run();
    assert!(matches!(choice.wait, Some(Wait::Choice { .. })));

    // Option 1 = "Stay silent" -> label silence.
    runner.choose(1).unwrap();
    let after = runner.run();
    let (speaker, text) = say_text(after.wait.as_ref().unwrap());
    assert_eq!(speaker, "Ace");
    assert!(text.contains("don't talk"));

    // jump converge -> stop -> jump reunion -> scene file from top.
    runner.advance_say().unwrap();
    let reunion = runner.run();
    assert_eq!(runner.scene_id(), "reunion");
    assert!(reunion
        .staged
        .contains(&StageCommand::Stop("music_morning".to_string())));
    assert!(reunion
        .staged
        .contains(&StageCommand::Bg("school_gate".to_string())));
    let (speaker, text) = say_text(reunion.wait.as_ref().unwrap());
    assert_eq!(speaker, "Narrator");
    assert!(text.contains("The gate again"));
}

#[test]
fn set_mutates_state_and_interpolates() {
    let pack = test_pack(
        &[("scene", "set mood = \"calm\"\nAce: Feeling {mood} today.\n")],
        HashMap::from([("mood".to_string(), StateValue::Str("low".to_string()))]),
    );
    // Ensure the fixture is a well-formed single scene.
    assert_eq!(pack.scenes["scene"].labels.len(), 0);

    let mut runner = ScriptRunner::new(pack);
    let outcome = runner.run();
    let (_, text) = say_text(outcome.wait.as_ref().unwrap());
    assert_eq!(text, "Feeling calm today.");
    assert_eq!(runner.state()["mood"], StateValue::Str("calm".to_string()));
}

#[test]
fn decision_parks_and_resolves_to_action_label() {
    let mut runner = ScriptRunner::new(demo_pack());
    drive_to_decision(&mut runner);

    match runner.wait() {
        Some(Wait::Decision {
            npc_id,
            question,
            prefix,
        }) => {
            assert_eq!(npc_id, "ace");
            assert_eq!(question, "greet");
            assert_eq!(prefix, "reply");
        }
        other => panic!("expected a decision wait, got {other:?}"),
    }

    runner.resolve_decision("warm").unwrap();
    let landed = runner.run();
    assert_eq!(
        landed.staged,
        vec![StageCommand::Sprite {
            npc_id: "ace".to_string(),
            pose: "happy".to_string(),
        }]
    );
    let (speaker, text) = say_text(landed.wait.as_ref().unwrap());
    assert_eq!(speaker, "Ace");
    assert!(text.contains("You came back!"));
}

#[test]
fn fallback_action_lands_on_fallback_label() {
    use crate::decision::{resolve_or_fallback, DecisionError};

    let pack = demo_pack();
    let config = pack.npcs["ace"].decisions["greet"].clone();
    let resolved = resolve_or_fallback(&config, Err(DecisionError::Http));
    assert_eq!(resolved.action, "cold");

    let mut runner = ScriptRunner::new(demo_pack());
    drive_to_decision(&mut runner);
    runner.resolve_decision(&resolved.action).unwrap();

    let landed = runner.run();
    let (speaker, text) = say_text(landed.wait.as_ref().unwrap());
    assert_eq!(speaker, "Ace");
    assert!(text.contains("It's you."));
}

#[test]
fn noul_decision_lands_on_the_boolean_label() {
    use crate::content::{Criteria, DecisionConfig, Fallback, Npc, QuestionType};
    use crate::decision::decision_from_body;

    let scene = "decide ace.is_hostile -> want\nlabel want.true\nNarrator: True.\nlabel want.false\nNarrator: False.\n";
    let mut pack = test_pack(
        &[("scene", scene)],
        HashMap::from([("met_ace".to_string(), StateValue::Bool(true))]),
    );
    let config = DecisionConfig {
        question_type: QuestionType::Noul,
        instructions: "Is the player hostile?".to_string(),
        state_keys: vec!["met_ace".to_string()],
        criteria: Criteria::Noul(Some(HashMap::from([
            ("true".to_string(), "Hostile.".to_string()),
            ("false".to_string(), "Not hostile.".to_string()),
        ]))),
        fallback: Fallback::Bool(false),
    };
    pack.npcs.insert(
        "ace".to_string(),
        Npc {
            id: "ace".to_string(),
            name: "Ace".to_string(),
            model: None,
            decisions: HashMap::from([("is_hostile".to_string(), config.clone())]),
        },
    );

    let mut runner = ScriptRunner::new(pack);
    assert!(matches!(runner.run().wait, Some(Wait::Decision { .. })));

    let body = r#"{ "model": "m", "answers": { "is_hostile": { "type": "noul", "noul": 0.9 } } }"#;
    let decision = decision_from_body(body, "is_hostile", &config).unwrap();
    runner.resolve_decision(&decision.action).unwrap();

    let landed = runner.run();
    assert_eq!(say_text(landed.wait.as_ref().unwrap()).1, "True.");
}

#[test]
fn score_decision_lands_on_the_probability_index() {
    use crate::content::{Criteria, DecisionConfig, Fallback, Npc, QuestionType};
    use crate::decision::decision_from_body;

    let scene = "decide ace.trust -> want\nlabel want.0\nNarrator: None.\nlabel want.1\nNarrator: Some.\nlabel want.2\nNarrator: All.\n";
    let mut pack = test_pack(
        &[("scene", scene)],
        HashMap::from([("met_ace".to_string(), StateValue::Bool(true))]),
    );
    let config = DecisionConfig {
        question_type: QuestionType::Score,
        instructions: "How much does Ace trust the player?".to_string(),
        state_keys: vec!["met_ace".to_string()],
        criteria: Criteria::Score(vec![
            "No trust.".to_string(),
            "Some trust.".to_string(),
            "Complete trust.".to_string(),
        ]),
        fallback: Fallback::Index(0),
    };
    pack.npcs.insert(
        "ace".to_string(),
        Npc {
            id: "ace".to_string(),
            name: "Ace".to_string(),
            model: None,
            decisions: HashMap::from([("trust".to_string(), config.clone())]),
        },
    );

    let mut runner = ScriptRunner::new(pack);
    assert!(matches!(runner.run().wait, Some(Wait::Decision { .. })));

    let body = r#"{ "model": "m", "answers": { "trust": { "type": "score", "score": 1.2, "probabilities": { "0": 0.1, "1": 0.2, "2": 0.7 } } } }"#;
    let decision = decision_from_body(body, "trust", &config).unwrap();
    runner.resolve_decision(&decision.action).unwrap();

    let landed = runner.run();
    assert_eq!(say_text(landed.wait.as_ref().unwrap()).1, "All.");
}

#[test]
fn unknown_action_label_is_an_error_and_keeps_the_wait() {
    let mut runner = ScriptRunner::new(demo_pack());
    drive_to_decision(&mut runner);
    let err = runner.resolve_decision("purple").unwrap_err();
    assert!(matches!(err, RunnerError::UnknownActionLabel { .. }));
    // Still parked: a caller can retry with the fallback.
    assert!(matches!(runner.wait(), Some(Wait::Decision { .. })));
}

#[test]
fn choice_index_out_of_range_is_an_error() {
    let mut runner = ScriptRunner::new(demo_pack());
    runner.run();
    runner.advance_say().unwrap();
    runner.run();
    runner.advance_say().unwrap();
    runner.run();
    assert_eq!(
        runner.choose(9).unwrap_err(),
        RunnerError::BadChoice { index: 9, len: 2 }
    );
}
