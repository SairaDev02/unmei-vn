//! Live OpenRouter Decisions smoke test.
//!
//! Ignored by default: it makes a real network call and needs a key.
//!
//! ```console
//! $ OPENROUTER_API_KEY=... cargo test -p unmei-engine --features network \
//!       --test decision_live -- --ignored
//! ```
//!
//! What it proves: the demo pack's `decide ace.greet ->
//! reply` line resolves a live Decisions answer and lands the script on
//! `reply.<action>`, offline fallback already covered by `runner` unit tests.
#![cfg(feature = "network")]

use std::path::Path;

use unmei_engine::{
    decision::DEFAULT_ENDPOINT, load_pack, resolve_or_fallback, DecisionsClient, ScriptRunner, Wait,
};

#[tokio::test]
#[ignore = "live OpenRouter call; see the command in this file's header"]
async fn live_decide_lands_on_action_label() {
    let key = std::env::var("OPENROUTER_API_KEY")
        .expect("set OPENROUTER_API_KEY (env only) to run the live Decisions test");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
    let pack = load_pack(&root).expect("demo pack loads").pack;

    let config = pack.npcs["ace"].decisions["greet"].clone();
    let model = pack.npcs["ace"]
        .model
        .clone()
        .expect("demo NPC pins a decision model");

    // Drive first_meeting -> reunion -> the decide point.
    let mut runner = ScriptRunner::new(pack);
    loop {
        match runner.run().wait {
            Some(Wait::Say { .. }) => runner.advance_say().expect("say wait"),
            Some(Wait::Choice { .. }) => runner.choose(0).expect("choice wait"),
            Some(Wait::Decision { .. }) => break,
            None => panic!("script ended before the decide point"),
        }
    }
    let state = runner.state().clone();

    let client = DecisionsClient::new(DEFAULT_ENDPOINT, key);
    let result = client.decide(&model, "greet", &config, &state).await;
    let resolved = resolve_or_fallback(&config, result);
    assert!(
        !resolved.is_fallback(),
        "live Decisions call failed: {}",
        resolved
            .fallback_reason
            .as_ref()
            .map_or_else(|| "<none>".to_string(), ToString::to_string)
    );
    eprintln!(
        "live decision: action `{}` via model `{}` (confidence {:?})",
        resolved.action,
        resolved.model.as_deref().unwrap_or("?"),
        resolved.confidence
    );

    runner
        .resolve_decision(&resolved.action)
        .expect("resolved action has a label");
    let landed = runner.run();
    let Some(Wait::Say { speaker, text }) = landed.wait else {
        panic!("decision did not land on a say-line");
    };
    let expected = match resolved.action.as_str() {
        "warm" => ("Ace", "You came back!"),
        "cold" => ("Ace", "It's you."),
        "none" => ("Narrator", "unreadable"),
        other => panic!("unexpected action `{other}`"),
    };
    assert_eq!(speaker, expected.0);
    assert!(
        text.contains(expected.1),
        "expected `{}` to contain `{}`",
        text,
        expected.1
    );
}
