//! One take: a sandbox, a fresh windowless app, a manual clock, the settle rule
//! and a panic boundary (spec §4).
//!
//! A take executes one case from its first line to its last. It never touches
//! the original pack, the user's sidecars or a credential file outside its own
//! sandbox.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use unmei_engine::keiko::{Case, Expect, Fault, FixtureKind, KeyCheckFault, Step};
use unmei_engine::Pack;

use super::assert::evaluate;
use super::decide::KeikoResolver;
use super::finding::{rules, Finding};
use super::observe::{observe, LogLine, Observation};
use super::sandbox::Sandbox;
use super::steps::{apply, Applied, FRAME};
use crate::failure::{SessionLog, SessionLogResult};
use crate::{Runtime, RuntimeConfig};

mod support;
use support::*;

/// A live run's client, built from the environment key only.
#[derive(Clone)]
pub struct Live {
    pub key: String,
    pub endpoint: String,
}

/// Everything one take needs.
pub struct TakeRequest<'a> {
    pub case: &'a Case,
    pub case_file: String,
    pub original_pack: &'a Path,
    pub pack: &'a Pack,
    pub take_index: usize,
    pub keep: bool,
    pub live: Option<Live>,
}

/// What one take produced.
pub struct TakeResult {
    pub outcome: Result<(), Finding>,
    pub observation: Observation,
    /// One observation per settled step, in order.
    pub records: Vec<Observation>,
    /// Every funnel line the take wrote.
    pub log: Vec<LogLine>,
    pub kept: Option<PathBuf>,
    pub live: bool,
}

/// Run one take.
pub fn run(request: TakeRequest) -> TakeResult {
    let sandbox = match Sandbox::create(request.original_pack, request.take_index) {
        Ok(sandbox) => sandbox,
        Err(error) => {
            return TakeResult {
                outcome: Err(Finding::new(
                    request.case_file,
                    String::new(),
                    rules::CASE_REFERENCE,
                    format!("cannot create the take sandbox: {error}"),
                )),
                observation: super::observe::empty(),
                records: Vec::new(),
                log: Vec::new(),
                kept: None,
                live: false,
            }
        }
    };

    let setup = prepare(&sandbox, request.case, request.original_pack);
    if let Err(finding) = setup {
        let kept = sandbox.finish(request.keep);
        return TakeResult {
            outcome: Err(finding),
            observation: super::observe::empty(),
            records: Vec::new(),
            log: Vec::new(),
            kept,
            live: false,
        };
    }

    let log = SessionLog::open(sandbox.session_log_path(), &crate::launch::version_text());
    let session_log = match log {
        Ok(log) => SessionLogResult::Opened(log),
        Err(error) => SessionLogResult::Unwritable(crate::failure::LogOpenError {
            path: sandbox.session_log_path(),
            message: error.to_string(),
        }),
    };

    let clock = Arc::new(AtomicU64::new(0));
    let resolver = build_resolver(&request, clock.clone());
    let live_marked = resolver.live_marked.clone();
    let key_check_release = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let key_check = crate::KeyCheckChannel {
        checker: Some(Arc::new(KeikoKeyCheck::new(
            &request.case.headers,
            key_check_release.clone(),
        ))),
    };
    let faults: Vec<Fault> = request
        .case
        .headers
        .iter()
        .filter_map(|line| match &line.header {
            unmei_engine::keiko::Header::Fault { fault } => Some(fault.clone()),
            _ => None,
        })
        .collect();
    let asset_faults = Arc::new(super::assets::AssetFaults::of(faults.iter()));
    // The launch credential read is the read-only seam `main` uses, pointed at
    // the take's own config dir: an `env-key` fixture wins, then a `credential`
    // fixture, and an absent file is the offline posture (spec §7.1).
    let launch = crate::credentials::resolve_launch(
        Some(&sandbox.credential_dir()),
        sandbox.env_key().as_deref(),
        |key| {
            let endpoint = request
                .live
                .as_ref()
                .map(|live| live.endpoint.clone())
                .unwrap_or_else(|| unmei_engine::DEFAULT_ENDPOINT.to_string());
            super::decide::live_client(&endpoint, key)
        },
    );
    let config = RuntimeConfig::new(
        sandbox.pack_root().to_path_buf(),
        Some(resolver.clone() as Arc<dyn unmei_engine::DecisionResolver>),
    )
    .with_dev_mode(request.case.dev())
    .with_asset_root(sandbox.asset_root().to_path_buf());

    crate::failure::tap::arm();
    let mut state = TakeState {
        observation: super::observe::empty(),
        records: Vec::new(),
        log_cursor: 0,
        all_log: Vec::new(),
        clock,
        timed: Arc::new(AtomicU64::new(0)),
        live: live_marked.clone(),
        resolver,
        key_check_release,
    };

    // The app owns the open session-log file and the audio device. Both must be
    // gone before the sandbox is deleted, or Windows refuses to remove a folder
    // whose file is still open.
    let outcome = {
        let mut app = crate::composition::keiko_app(
            config,
            session_log,
            crate::credentials::ClipboardPort::Memory(None),
            key_check,
            crate::DecisionFactory { builder: None },
            asset_faults,
        );
        // The launch read is preloaded, so the sandbox's credential fixture and
        // the `env-key` fixture are what the app sees (spec §7.1).
        app.insert_resource(crate::credentials::StartupCredentials {
            state: launch.state,
            warning: launch.warning,
        });
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            boot(&mut app, &mut state, &request)?;
            for step in &request.case.steps {
                step_once(&mut app, &mut state, &request, &step.step, step.line)?;
            }
            // A key still held when the take ends is released.
            release_held(&mut app);
            Ok::<(), Finding>(())
        }))
        .unwrap_or_else(|payload| {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
                .unwrap_or_else(|| "a panic with no message".to_string());
            Err(Finding::new(
                request.case_file.clone(),
                String::new(),
                rules::PANIC,
                message,
            ))
        });
        // The observation is rebuilt once more so a failing step reports the
        // last frame it actually saw.
        state.observation = observe(&mut app, Vec::new(), state.live.load(Ordering::Relaxed));
        outcome
    };

    crate::failure::set_keiko_log(None);
    let _ = crate::failure::tap::finish_take();
    let kept = sandbox.finish(request.keep);
    TakeResult {
        outcome,
        observation: state.observation,
        records: state.records,
        log: state.all_log,
        kept,
        live: state.live.load(Ordering::Relaxed),
    }
}

pub(super) struct TakeState {
    observation: Observation,
    records: Vec<Observation>,
    log_cursor: usize,
    all_log: Vec<LogLine>,
    clock: Arc<AtomicU64>,
    timed: Arc<AtomicU64>,
    live: Arc<std::sync::atomic::AtomicBool>,
    resolver: Arc<KeikoResolver>,
    key_check_release: Arc<std::sync::atomic::AtomicBool>,
}

/// Boot the app and settle on the first surface (spec §4.3), then honour a
/// `start` header by reaching the scene top through the Timeline's own restart
/// seam (spec §12).
fn boot(app: &mut App, state: &mut TakeState, request: &TakeRequest) -> Result<(), Finding> {
    frame(app, &state.timed, &state.clock, Duration::ZERO);
    settle(app, state, request)?;
    if request.case.start_scene().is_some() {
        start_at_scene(app, state, request)?;
        settle(app, state, request)?;
    }
    Ok(())
}

/// `start <scene>` with the `state` overrides: enter play, then replace the
/// position through the one restart recipe the Timeline uses.
fn start_at_scene(
    app: &mut App,
    state: &mut TakeState,
    request: &TakeRequest,
) -> Result<(), Finding> {
    let fail = |message: String| {
        Finding::new(
            request.case_file.clone(),
            String::new(),
            rules::CASE_REFERENCE,
            message,
        )
    };
    let scene = request
        .case
        .start_scene()
        .expect("a start header")
        .to_string();

    // Enter play through the title's own New Game, so the live session the menu
    // keeps is real.
    let mut frame_run = |app: &mut App| frame(app, &state.timed, &state.clock, Duration::ZERO);
    super::steps::apply(app, &Step::NewGame, &mut frame_run, &state.resolver)
        .map_err(|finding| fail(finding.message))?;
    settle(app, state, request)?;

    let mut start_state = request.pack.state.clone();
    for (key, value) in request.case.state_overrides() {
        let parsed = unmei_engine::keiko::parse_state_value(value)
            .ok_or_else(|| fail(format!("`{value}` is not a state value for `{key}`")))?;
        start_state.insert(key.to_string(), parsed);
    }
    let scene_for_start = scene.clone();
    let outcome =
        app.world_mut()
            .run_system_once(move |mut session: crate::session::SessionInput| {
                let mut transitions = session.transitions();
                matches!(
                    transitions.restart_scene_with_state(&scene_for_start, start_state.clone()),
                    crate::session::RestartOutcome::Restarted
                )
            });
    match outcome {
        Ok(true) => Ok(()),
        _ => Err(fail(format!("scene `{scene}` could not be started"))),
    }
}

/// Apply one step, settle where the spec says to, and evaluate an assertion.
fn step_once(
    app: &mut App,
    state: &mut TakeState,
    request: &TakeRequest,
    step: &Step,
    line: usize,
) -> Result<(), Finding> {
    // The two fault-step releases act on the take's own fault state before the
    // step is applied like any other.
    match step {
        Step::ReleaseKeyCheck => state.key_check_release.store(true, Ordering::Relaxed),
        Step::Unblock(target) => {
            let pack_root = app
                .world()
                .get_resource::<Runtime>()
                .map(|runtime| runtime.pack_root.clone());
            if let Some(pack_root) = pack_root {
                let path = write_target_path_root(&pack_root, *target);
                let _ = std::fs::remove_dir_all(path.with_extension("ron.tmp"));
            }
        }
        _ => {}
    }
    let resolver = state.resolver.clone();
    let mut frame_run = |app: &mut App| frame(app, &state.timed, &state.clock, Duration::ZERO);
    let applied = apply(app, step, &mut frame_run, &resolver).map_err(|finding| {
        let mut finding = finding;
        finding.file = request.case_file.clone();
        finding.path = format!("line {line}");
        finding
    })?;
    match applied {
        Applied::Input => settle_with_step(app, state, request, line)?,
        Applied::Wait(duration) => {
            for _ in 0..frames_for(duration) {
                frame(app, &state.timed, &state.clock, FRAME);
            }
            settle_with_step(app, state, request, line)?;
        }
        Applied::Assert(expect) => {
            observe_step(app, state);
            evaluate(&expect, &state.observation, &state.all_log, app).map_err(|finding| {
                let mut finding = finding;
                finding.file = request.case_file.clone();
                finding.path = format!("line {line}");
                finding
            })?;
        }
    }
    Ok(())
}

/// The settle rule: zero-duration frames until the world is quiet, or 600
/// frames, whichever comes first (spec §4.3).
pub(super) fn settle(
    app: &mut App,
    state: &mut TakeState,
    request: &TakeRequest,
) -> Result<(), Finding> {
    let held_decision = request.case.headers.iter().any(|line| {
        matches!(
            line.header,
            unmei_engine::keiko::Header::Fault {
                fault: Fault::DecisionHold { .. } | Fault::DecisionSlow { .. }
            }
        )
    }) || request.case.headers.iter().any(|line| {
        matches!(
            line.header,
            unmei_engine::keiko::Header::Fault {
                fault: Fault::KeyCheck {
                    verdict: KeyCheckFault::Hold
                }
            }
        )
    });

    let mut previous: Option<Observation> = None;
    let mut quiet = 0u32;
    // A live answer arrives on the network's schedule, so its pending frames
    // wait in real time and do not spend the quiet-frame budget (spec §6).
    let live_deadline = std::time::Instant::now() + LIVE_ANSWER_LIMIT;
    while quiet < 600 {
        frame(app, &state.timed, &state.clock, Duration::ZERO);
        observe_step(app, state);
        let observation = &state.observation;
        let decision_pending = observation.decision.is_some() && !held_decision;
        if decision_pending && request.live.is_some() {
            if std::time::Instant::now() >= live_deadline {
                return Err(Finding::new(
                    request.case_file.clone(),
                    String::new(),
                    rules::UNSETTLED,
                    "the live decision did not answer within 60 s",
                ));
            }
            std::thread::sleep(Duration::from_millis(2));
            previous = None;
            continue;
        }
        let assets_settled = app
            .world()
            .get_resource::<crate::presentation::assets::AssetWatch>()
            .is_none_or(|watch| watch.is_empty());
        let stable = previous
            .as_ref()
            .is_some_and(|previous| previous.same_world(observation));
        quiet += 1;
        if !decision_pending && assets_settled && stable {
            state.records.push(observation.clone());
            return Ok(());
        }
        previous = Some(observation.clone());
    }
    Err(Finding::new(
        request.case_file.clone(),
        String::new(),
        rules::UNSETTLED,
        "the world did not become quiet within 600 frames",
    ))
}

/// How long a live answer may take before the take gives up (spec §6).
const LIVE_ANSWER_LIMIT: Duration = Duration::from_secs(60);

/// One frame of the take's clock. Only this function moves time.
fn frame(app: &mut App, timed: &AtomicU64, clock: &AtomicU64, delta: Duration) {
    let delta = if delta.is_zero() {
        Duration::ZERO
    } else {
        // 1/60 s is not a whole number of nanoseconds, so the frame's length
        // alternates by one nanosecond: frame n ends at exactly `n * 1e9 / 60`
        // ns, which keeps both the take's clock and Bevy's own `Time` exact.
        // Summing a truncated 16,666,666 ns delta would leave `wait 2s` a
        // nanosecond short of the auto dwell (spec §3.4, §4.3).
        let frames = timed.fetch_add(1, Ordering::Relaxed) + 1;
        let total = frames * NANOS_PER_FRAME / 60;
        let previous = (frames - 1) * NANOS_PER_FRAME / 60;
        clock.store(total, Ordering::Relaxed);
        Duration::from_nanos(total - previous)
    };
    app.insert_resource(TimeUpdateStrategy::ManualDuration(delta));
    app.update();
}

/// One frame of the take's clock in nanoseconds, as `n * 1e9 / 60`.
const NANOS_PER_FRAME: u64 = 1_000_000_000;

/// Apply the case's fixtures and faults to the sandbox before boot (spec §7).
fn prepare(sandbox: &Sandbox, case: &Case, original_pack: &Path) -> Result<(), Finding> {
    let tests_dir = original_pack.join("tests");
    let fail =
        |message: String| Finding::new("case", String::new(), rules::CASE_REFERENCE, message);

    for line in &case.headers {
        if let unmei_engine::keiko::Header::Fixture { kind, path } = &line.header {
            let source = tests_dir.join(path);
            let destination = sandbox.fixture_path(*kind);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| fail(format!("fixture `{path}`: {error}")))?;
            }
            std::fs::copy(&source, &destination)
                .map_err(|error| fail(format!("fixture `{path}`: {error}")))?;
        }
    }

    // `setting` headers: written into the sandbox settings file before boot, on
    // top of any `fixture settings` the case copied.
    let settings_headers: Vec<(&str, &str)> = case
        .headers
        .iter()
        .filter_map(|line| match &line.header {
            unmei_engine::keiko::Header::Setting { key, value } => {
                Some((key.as_str(), value.as_str()))
            }
            _ => None,
        })
        .collect();
    if !settings_headers.is_empty() {
        let mut settings = crate::settings::load(sandbox.pack_root()).settings;
        for (key, value) in settings_headers {
            apply_setting(&mut settings, key, value).map_err(&fail)?;
        }
        crate::settings::save(sandbox.pack_root(), &settings)
            .map_err(|error| fail(format!("setting: {error}")))?;
    }

    for line in &case.headers {
        let unmei_engine::keiko::Header::Fault { fault } = &line.header else {
            continue;
        };
        match fault {
            Fault::WriteBlocked { target } => {
                let path = write_target_path(sandbox, *target);
                let temp = path.with_extension("ron.tmp");
                std::fs::create_dir_all(&temp)
                    .map_err(|error| fail(format!("write-blocked: {error}")))?;
            }
            Fault::PackFileMissing { path } => {
                let _ = std::fs::remove_file(sandbox.pack_root().join(path));
            }
            Fault::PackFile { path, fixture } => {
                let source = tests_dir.join(fixture);
                let destination = sandbox.pack_root().join(path);
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|error| fail(format!("pack-file: {error}")))?;
                }
                std::fs::copy(&source, &destination)
                    .map_err(|error| fail(format!("pack-file `{fixture}`: {error}")))?;
            }
            Fault::CredentialUnreadable => {
                let path = sandbox.fixture_path(FixtureKind::Credential);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::create_dir_all(&path)
                    .map_err(|error| fail(format!("credential-unreadable: {error}")))?;
            }
            Fault::AssetMissing { .. }
            | Fault::AssetCorrupt { .. }
            | Fault::DecisionError { .. }
            | Fault::DecisionSlow { .. }
            | Fault::DecisionHold { .. }
            | Fault::KeyCheck { .. } => {}
        }
    }
    Ok(())
}

/// One step's expectation, exposed for the take's tests.
pub fn expects(case: &Case) -> Vec<&Expect> {
    case.steps
        .iter()
        .filter_map(|line| match &line.step {
            Step::Expect(expect) => Some(expect),
            _ => None,
        })
        .collect()
}
