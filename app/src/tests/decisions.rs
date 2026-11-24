use super::*;

#[test]
fn decision_without_a_client_takes_the_author_fallback() {
    let mut app = headless_app();
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    // The core sees the decision park and resolves it (fallback `cold`),
    // then the next step parks on `reply.cold`'s say-line.
    app.update();
    app.update();

    let runtime = app.world().resource::<Runtime>();
    let Some(Wait::Say { speaker, text }) = runtime.runner.wait() else {
        panic!(
            "expected a say after the fallback, got {:?}",
            runtime.runner.wait()
        );
    };
    assert_eq!(speaker, "Ace");
    assert!(text.contains("It's you."), "got {text:?}");
    assert!(app.world().resource::<PendingDecision>().0.is_none());
}

#[test]
fn a_model_answered_decision_writes_the_autosave_once() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    assert!(
        !autosave_path(&app).exists(),
        "a request in flight writes no autosave yet"
    );

    fake.release();
    pump_until(&mut app, |app| autosave_path(app).exists());

    let runtime = app.world().resource::<Runtime>();
    let save = save::read_slot(runtime.runner.pack(), Slot::Autosave).expect("autosaved");
    assert!(save.saved_at.is_some());
    assert_eq!(save.wait, runtime.runner.wait().cloned());
    assert!(
        matches!(save.wait, Some(Wait::Say { ref text, .. }) if text.contains("You came back!")),
        "the autosave captures the post-model-answer park: {:?}",
        save.wait
    );

    let before = std::fs::read(autosave_path(&app)).unwrap();
    app.update();
    assert_eq!(std::fs::read(autosave_path(&app)).unwrap(), before);
}

#[test]
fn a_fallback_resolved_decision_writes_no_autosave() {
    let fake = FakeResolver::new(fallback_answer("cold"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    fake.release();
    pump_until(&mut app, parked_on_say);
    assert!(
        !autosave_path(&app).exists(),
        "a fallback resolve never autosaves"
    );
}

#[test]
fn loading_a_decision_wait_refires_the_decide_line() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    assert_eq!(fake.calls(), 1);

    // Save while parked on the decision, then load: the wait comes back, the
    // in-flight request is cancelled, and the next `run` re-fires `decide`.
    press_and_update(&mut app, KeyCode::F5);
    press_and_update(&mut app, KeyCode::F9);

    // The restored decision-wait re-fires `decide`; the re-fire is a fresh
    // async request, so it can land on a later frame than the load.
    pump_until(&mut app, |_| fake.calls() == 2);
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));
    assert_eq!(fake.calls(), 2, "the restored decision re-fired");
}

#[test]
fn fake_resolver_withholds_a_model_answer_then_lands_the_runner() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);

    assert_eq!(fake.calls(), 1, "the core called the resolver once");
    assert!(app.world().resource::<PendingDecision>().0.is_some());

    // Withheld: the answer does not exist yet, so the runner stays parked.
    app.update();
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));

    fake.release();
    pump_until(&mut app, parked_on_say);
    let runtime = app.world().resource::<Runtime>();
    let Some(Wait::Say { speaker, text }) = runtime.runner.wait() else {
        panic!(
            "expected a say after the model answer, got {:?}",
            runtime.runner.wait()
        );
    };
    assert_eq!(speaker, "Ace");
    assert!(text.contains("You came back!"), "got {text:?}");
    assert!(app.world().resource::<PendingDecision>().0.is_none());
}

#[test]
fn fake_resolver_delivers_a_fallback_resolution() {
    let fake = FakeResolver::new(fallback_answer("cold"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    fake.release();
    pump_until(&mut app, parked_on_say);
    let runtime = app.world().resource::<Runtime>();
    let Some(Wait::Say { text, .. }) = runtime.runner.wait() else {
        panic!(
            "expected a say after the fallback, got {:?}",
            runtime.runner.wait()
        );
    };
    assert!(text.contains("It's you."), "got {text:?}");
}

#[test]
fn dropping_the_pending_decision_cancels_the_fake_future() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    assert_eq!(fake.calls(), 1);
    assert_eq!(fake.dropped(), 0);

    // What a load does: replace the parked task with `None`.
    app.world_mut().resource_mut::<PendingDecision>().0 = None;
    fake.wait_until_dropped();
    assert_eq!(fake.dropped(), 1, "dropping the task must drop the future");
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));
}

#[test]
fn a_model_answer_arriving_while_open_is_held_until_resume() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    let parked = parked_position(&app);

    esc(&mut app);
    assert!(menu(&app).open);
    fake.release();
    pump_until(&mut app, held);

    assert_eq!(parked_position(&app), parked, "the answer is not applied");
    assert!(matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Decision { .. })
    ));
    assert!(app.world().resource::<PendingDecision>().0.is_none());
    assert!(held(&app), "the completed answer is buffered");
    assert!(!autosave_path(&app).exists(), "held means no write yet");

    // Resume: the buffered answer is applied, the runner parks at the next
    // wait, and the model resolve writes the autosave exactly once.
    esc(&mut app);
    assert!(!menu(&app).open);
    assert!(!held(&app));
    let runtime = app.world().resource::<Runtime>();
    let Some(Wait::Say { text, .. }) = runtime.runner.wait() else {
        panic!(
            "expected a say after resume, got {:?}",
            runtime.runner.wait()
        );
    };
    assert!(text.contains("You came back!"), "got {text:?}");

    let save = save::read_slot(runtime.runner.pack(), Slot::Autosave).expect("autosaved");
    assert_eq!(save.wait, runtime.runner.wait().cloned());

    let before = std::fs::read(autosave_path(&app)).unwrap();
    app.update();
    assert_eq!(
        std::fs::read(autosave_path(&app)).unwrap(),
        before,
        "a held answer writes the autosave exactly once"
    );
}

#[test]
fn a_held_fallback_writes_no_autosave() {
    let fake = FakeResolver::new(fallback_answer("cold"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    let parked = parked_position(&app);

    esc(&mut app);
    fake.release();
    pump_until(&mut app, held);
    assert_eq!(parked_position(&app), parked, "the held fallback waits");
    assert!(held(&app));

    esc(&mut app);
    assert!(!held(&app));
    let Some(Wait::Say { text, .. }) = app.world().resource::<Runtime>().runner.wait() else {
        panic!("expected a say after the held fallback");
    };
    assert!(text.contains("It's you."), "got {text:?}");
    assert!(
        !autosave_path(&app).exists(),
        "a held fallback writes no autosave"
    );
}

#[test]
fn loading_discards_the_held_answer_and_refires_the_decide_line() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);

    press_and_update(&mut app, KeyCode::F5);
    esc(&mut app);
    fake.release();
    pump_until(&mut app, held);

    switch_mode(&mut app, MenuMode::Load);
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::Card(0);
    press_and_update(&mut app, KeyCode::Enter); // confirm
    press_and_update(&mut app, KeyCode::Enter); // accept

    assert!(!menu(&app).open, "a load closes the overlay");
    assert!(!held(&app), "the held answer is discarded by the load");
    // The restored decision-wait re-fires `decide`; the re-fire goes through
    // the task pool, so it can be observed on a later frame. This fake is
    // already released, so the fresh request answers at once rather than
    // parking: the runner carries on to the next say.
    pump_until(&mut app, |_| fake.calls() == 2);
    assert_eq!(fake.calls(), 2, "the restored decision re-fired");
    pump_until(&mut app, parked_on_say);
}

/// The load's own state transition, observed without the immediate re-fire that
/// the full app frame follows it with: `SessionTransitions::restore_loaded`
/// empties both the in-flight request and the held answer.
#[test]
fn a_load_clears_the_held_and_pending_decision_state() {
    let fake = FakeResolver::new(model_answer("warm"));
    let (mut app, _dir) = app_parked_on_decision(&fake);
    let save = {
        let runtime = app.world().resource::<Runtime>();
        SaveFile::capture(&runtime.runner, &runtime.staging)
    };

    esc(&mut app);
    fake.release();
    pump_until(&mut app, held);

    let path = PathBuf::from("held.slot1.save.ron");
    let save = std::sync::Mutex::new(Some(save));
    let outcome = app
        .world_mut()
        .run_system_once(move |mut session: SessionInput| {
            let save = save
                .lock()
                .expect("the test closure lock is never poisoned")
                .take()
                .expect("the system runs once");
            session.transitions().restore_loaded(save, &path)
        })
        .expect("the one-shot restore system runs");
    assert_eq!(outcome, LoadOutcome::Loaded, "the restore succeeds");

    assert!(!held(&app));
    assert!(app.world().resource::<PendingDecision>().0.is_none());
}
