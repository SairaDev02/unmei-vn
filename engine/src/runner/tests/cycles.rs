//! Wait-free jump cycles: a run that can never park must still return.

use super::*;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Run `runner` once on a worker thread and give up after `limit`, so a
/// regression fails the suite instead of hanging it. A timed-out worker keeps
/// spinning until the test process exits.
fn run_bounded(
    mut runner: ScriptRunner,
    limit: Duration,
) -> Option<Result<RunOutcome, RunnerError>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(runner.run());
    });
    rx.recv_timeout(limit).ok()
}

#[test]
fn a_wait_free_cycle_is_a_runner_error_naming_the_scene_and_target() {
    let pack = test_pack(&[("start", "label quiet\njump quiet\n")], HashMap::new());
    let outcome = run_bounded(ScriptRunner::new(pack), Duration::from_secs(5))
        .expect("run() returns on a wait-free cycle");
    assert_eq!(
        outcome,
        Err(RunnerError::WaitFreeCycle {
            scene: "start".to_string(),
            target: "quiet".to_string(),
        })
    );
}

#[test]
fn a_cycle_that_is_wait_free_only_on_some_state_is_caught() {
    // The loop parks while `open` holds; with the declared `false` it spins.
    let src = "label top\nif open\nNarrator: Door.\nendif\njump top\n";
    let state = HashMap::from([("open".to_string(), StateValue::Bool(false))]);
    let pack = test_pack(&[("start", src)], state);
    let outcome = run_bounded(ScriptRunner::new(pack), Duration::from_secs(5))
        .expect("run() returns on a state-dependent wait-free cycle");
    assert!(matches!(outcome, Err(RunnerError::WaitFreeCycle { .. })));
}

#[test]
fn a_loop_that_sets_its_exit_flag_runs_on() {
    // Revisits `top` once with different state, then leaves: not a cycle.
    let src = "label top\nif done\njump out\nendif\nset done = true\njump top\nlabel out\nNarrator: Out.\n";
    let state = HashMap::from([("done".to_string(), StateValue::Bool(false))]);
    let pack = test_pack(&[("start", src)], state);
    let outcome = run_bounded(ScriptRunner::new(pack), Duration::from_secs(5))
        .expect("run() returns")
        .expect("a terminating loop is not a cycle");
    assert_eq!(say_text(outcome.wait.as_ref().unwrap()).1, "Out.");
}

#[test]
fn a_cycle_across_two_scene_files_is_caught() {
    let pack = test_pack(&[("a", "jump b\n"), ("b", "jump a\n")], HashMap::new());
    let outcome = run_bounded(ScriptRunner::new(pack), Duration::from_secs(5))
        .expect("run() returns on a cross-scene cycle");
    assert!(matches!(outcome, Err(RunnerError::WaitFreeCycle { .. })));
}
