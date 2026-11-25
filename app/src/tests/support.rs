use super::*;

pub(super) fn demo_pack_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/demo-pack")
        .canonicalize()
        .expect("demo pack path exists")
}

pub(super) fn headless_app() -> App {
    app_with_decisions(None)
}

/// Attach the embedded interface faces to a headless test world. The core test
/// apps do not add `TextPlugin`, so this inserts a bare `Assets<Font>` and the
/// [`crate::presentation::style::TextFaces`] resource the presentation systems read; no
/// pack asset root is touched (`docs/title-and-menu.md` § Type system §1). The
/// renderer decodes a plugin's shipped images into their own store, which is
/// bare here for the same reason: no `ImagePlugin` is added.
pub(super) fn attach_text_faces(app: &mut App) {
    let mut fonts = bevy::asset::Assets::<bevy::text::Font>::default();
    let faces = crate::presentation::style::TextFaces::embed(&mut fonts);
    app.insert_resource(fonts);
    app.insert_resource(faces);
    app.init_resource::<bevy::asset::Assets<bevy::image::Image>>();
}

/// A headless core app over an explicit pack root and resolver. `ButtonInput`
/// is initialised directly (not via `InputPlugin`) so a manually pressed key
/// survives into `Update` for the input systems to see. The first frame leaves
/// the gated title up.
pub(super) fn boot_app_unstarted(
    pack_root: PathBuf,
    decisions: Option<Arc<dyn DecisionResolver>>,
) -> App {
    boot_app_unstarted_dev(pack_root, decisions, true)
}

/// [`boot_app_unstarted`], with the shared dev-mode gate set explicitly. Test
/// fixtures that press F5/F9 pass `true` so a release test run protects the same
/// shortcut behavior; the dev-off regressions pass `false`.
///
/// The renderer's boot and repaint systems live in the presentation plugin a
/// session adds; a headless app registers them here, so a surface drawn from a
/// plugin tree behaves as it does in the player.
pub(super) fn boot_app_unstarted_dev(
    pack_root: PathBuf,
    decisions: Option<Arc<dyn DecisionResolver>>,
    dev_mode: bool,
) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(pack_root, decisions).with_dev_mode(dev_mode))
        .add_plugins(RuntimeCorePlugin);
    attach_text_faces(&mut app);
    crate::presentation::render::register(&mut app);
    app
}

pub(super) fn boot_app(pack_root: PathBuf, decisions: Option<Arc<dyn DecisionResolver>>) -> App {
    let mut app = boot_app_unstarted(pack_root, decisions);
    app.update(); // Startup: load the pack, then show the title
    app
}

/// Cross the real boot path: press the title's `New Game` (the demo packs have
/// no autosave, so it is immediate and confirm-free). Every pre-existing test
/// enters play through this, so no production bypass exists.
pub(super) fn enter_play(app: &mut App) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_and_update(app, KeyCode::Enter);
    assert!(!menu(app).open, "enter_play must actually start play");
}

/// A headless core app with an explicit decision resolver: `None` exercises the
/// offline author fallback, `Some` a test-local fake driven in-process.
pub(super) fn app_with_decisions(decisions: Option<Arc<dyn DecisionResolver>>) -> App {
    app_with_decisions_at(demo_pack_root(), decisions)
}

/// A headless core app over an explicit pack root and resolver, already in
/// play: boot the title, then press `New Game`.
pub(super) fn app_with_decisions_at(
    pack_root: PathBuf,
    decisions: Option<Arc<dyn DecisionResolver>>,
) -> App {
    let mut app = boot_app(pack_root, decisions);
    enter_play(&mut app);
    app
}

/// Advance the runner directly to the pack's decision point, staging as the
/// core would, so the app systems can be exercised from a known park.
pub(super) fn drive_runner_to_decision(runtime: &mut Runtime) {
    loop {
        let outcome = runtime.runner.run().unwrap();
        runtime.staging.apply_all(&outcome.staged);
        match outcome.wait {
            Some(Wait::Say { .. }) => runtime.runner.advance_say().unwrap(),
            Some(Wait::Choice { .. }) => runtime.runner.choose(0).unwrap(),
            Some(Wait::Decision { .. }) => return,
            None => panic!("script ended before the decision point"),
        }
    }
}

/// Copy the demo pack into a temp dir so F5/F9 write `<temp>/pack.slot1.save.ron`
/// instead of dirtying the repo.
pub(crate) fn temp_pack() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &dst);
    dir
}

pub(super) fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// The sibling path one slot resolves to for the running app's pack.
pub(super) fn slot_path(app: &App, slot: Slot) -> PathBuf {
    let runtime = app.world().resource::<Runtime>();
    save::save_path_for(runtime.runner.pack(), slot)
}

/// The F5/F9 manual target (`Manual(1)`) for the running app's pack.
pub(super) fn manual_path(app: &App) -> PathBuf {
    slot_path(app, Slot::Manual(1))
}

/// The game-written autosave path for the running app's pack.
pub(super) fn autosave_path(app: &App) -> PathBuf {
    slot_path(app, Slot::Autosave)
}

pub(crate) fn app_for_pack(pack_root: PathBuf) -> App {
    let mut app = boot_app(pack_root, None);
    enter_play(&mut app);
    app
}

pub(crate) fn press_and_update(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    // Fully reset so the same key can be pressed again next frame.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
}

pub(super) fn click_and_update(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .reset_all();
}

/// The shared control surface of [`FakeResolver`]: it withholds a resolution
/// until [`FakeResolver::release`] and counts how many of its futures were
/// dropped **before producing an answer** (the signal a load cancels the
/// in-flight request).
#[derive(Default)]
pub(super) struct FakeState {
    pub(super) released: AtomicBool,
    pub(super) dropped: AtomicUsize,
    pub(super) waker: Mutex<Option<Waker>>,
}

/// A jointly-controllable, network-free resolver: the app spawns it exactly as
/// it spawns the concrete client, and the test decides when it answers.
#[derive(Clone)]
pub(super) struct FakeResolver {
    pub(super) resolution: ResolvedDecision,
    pub(super) calls: Arc<AtomicUsize>,
    pub(super) state: Arc<FakeState>,
}

impl FakeResolver {
    pub(super) fn new(resolution: ResolvedDecision) -> Self {
        FakeResolver {
            resolution,
            calls: Arc::new(AtomicUsize::new(0)),
            state: Arc::new(FakeState::default()),
        }
    }

    pub(super) fn release(&self) {
        self.state.released.store(true, Ordering::SeqCst);
        if let Some(waker) = self.state.waker.lock().unwrap().take() {
            waker.wake();
        }
    }

    pub(super) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub(super) fn dropped(&self) -> usize {
        self.state.dropped.load(Ordering::SeqCst)
    }

    /// Dropping `Task` cancels it, which drops this future — possibly on a pool
    /// worker, so observe it with a bounded wait rather than assuming a frame.
    /// A completed future does not count as dropped (see [`FakeFuture`]'s
    /// `Drop`), which is why a released fake cannot mask a real cancellation.
    /// The deadline is only a safety bound (the assertion is about cancellation,
    /// not latency); it is generous so a loaded test pool cannot flake it.
    pub(super) fn wait_until_dropped(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.dropped() == 0 {
            assert!(
                Instant::now() < deadline,
                "the fake future was never dropped"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

pub(super) struct FakeFuture {
    pub(super) resolution: Option<ResolvedDecision>,
    pub(super) state: Arc<FakeState>,
}

impl Future for FakeFuture {
    type Output = ResolvedDecision;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<ResolvedDecision> {
        if self.state.released.load(Ordering::SeqCst) {
            return Poll::Ready(self.take_resolution());
        }
        *self.state.waker.lock().unwrap() = Some(cx.waker().clone());
        // `release` may have raced the registration above.
        if self.state.released.load(Ordering::SeqCst) {
            return Poll::Ready(self.take_resolution());
        }
        Poll::Pending
    }
}

impl FakeFuture {
    pub(super) fn take_resolution(&mut self) -> ResolvedDecision {
        self.resolution
            .take()
            .expect("the fake future was polled after it completed")
    }
}

impl Drop for FakeFuture {
    fn drop(&mut self) {
        // A completed future is emptied by `take_resolution`, so a remaining
        // resolution means it was dropped before it ever answered — a
        // cancellation (a load replacing the parked task), not the routine drop
        // that follows `Ready`.
        if self.resolution.is_some() {
            self.state.dropped.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl DecisionResolver for FakeResolver {
    fn resolve<'a>(
        &'a self,
        _model: &'a str,
        _question_id: &'a str,
        _config: &'a DecisionConfig,
        _state: &'a HashMap<String, StateValue>,
    ) -> Pin<Box<dyn Future<Output = ResolvedDecision> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(FakeFuture {
            resolution: Some(self.resolution.clone()),
            state: self.state.clone(),
        })
    }
}

/// The shared control surface of [`FakeKeyCheck`]: the call count, and the one
/// verdict it answers with once released. An unreleased fake never answers — the
/// "ping that never answers" case.
#[derive(Default)]
pub(super) struct FakeKeyCheckState {
    pub(super) calls: AtomicUsize,
    pub(super) released: AtomicBool,
    pub(super) verdict: Mutex<Option<KeyVerdict>>,
    pub(super) waker: Mutex<Option<Waker>>,
}

/// A jointly-controllable, network-free [`KeyCheck`]: the app spawns it exactly
/// as it spawns `HttpKeyCheck`, and the test decides when (and with what) it
/// answers.
#[derive(Clone, Default)]
pub(super) struct FakeKeyCheck {
    pub(super) state: Arc<FakeKeyCheckState>,
}

impl FakeKeyCheck {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Answer with `verdict` at the next poll.
    pub(super) fn release(&self, verdict: KeyVerdict) {
        *self.state.verdict.lock().unwrap() = Some(verdict);
        self.state.released.store(true, Ordering::SeqCst);
        if let Some(waker) = self.state.waker.lock().unwrap().take() {
            waker.wake();
        }
    }

    pub(super) fn calls(&self) -> usize {
        self.state.calls.load(Ordering::SeqCst)
    }

    /// Arm the fake for a second commit: clear the released flag and the answer.
    /// Only valid once the previous future has completed.
    pub(super) fn reset(&self) {
        self.state.released.store(false, Ordering::SeqCst);
        *self.state.verdict.lock().unwrap() = None;
    }
}

pub(super) struct FakeKeyCheckFuture {
    pub(super) state: Arc<FakeKeyCheckState>,
}

impl FakeKeyCheckFuture {
    pub(super) fn verdict(&self) -> KeyVerdict {
        self.state
            .verdict
            .lock()
            .unwrap()
            .take()
            .unwrap_or(KeyVerdict::Unreachable)
    }
}

impl Future for FakeKeyCheckFuture {
    type Output = KeyVerdict;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<KeyVerdict> {
        if self.state.released.load(Ordering::SeqCst) {
            return Poll::Ready(self.verdict());
        }
        *self.state.waker.lock().unwrap() = Some(cx.waker().clone());
        // `release` may have raced the registration above.
        if self.state.released.load(Ordering::SeqCst) {
            return Poll::Ready(self.verdict());
        }
        Poll::Pending
    }
}

impl KeyCheck for FakeKeyCheck {
    fn check<'a>(&'a self, _key: &'a str) -> Pin<Box<dyn Future<Output = KeyVerdict> + Send + 'a>> {
        self.state.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(FakeKeyCheckFuture {
            state: self.state.clone(),
        })
    }
}

pub(super) fn model_answer(action: &str) -> ResolvedDecision {
    ResolvedDecision {
        action: action.to_string(),
        model: Some("fake/model".to_string()),
        confidence: Some(0.9),
        fallback_reason: None,
    }
}

/// A fallback the test controls directly: the engine's transport error, so the
/// row the app reports is the real classification of a real variant.
pub(super) fn fallback_answer(action: &str) -> ResolvedDecision {
    ResolvedDecision {
        action: action.to_string(),
        model: None,
        confidence: None,
        fallback_reason: Some(DecisionError::Http),
    }
}

/// Build an app whose resolver is the fake, parked by the core on the demo
/// pack's decision point, with the request spawned. The pack is a temp copy so
/// a model-answer autosave never dirties the repo; the caller must hold the
/// returned guard for the app's lifetime.
pub(super) fn app_parked_on_decision(fake: &FakeResolver) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("pack");
    copy_dir(&demo_pack_root(), &dst);
    let mut app = app_with_decisions_at(dst, Some(Arc::new(fake.clone())));
    {
        let mut runtime = app.world_mut().resource_mut::<Runtime>();
        drive_runner_to_decision(&mut runtime);
    }
    app.update(); // `drive_script` spawns the resolver task

    // `start_decision` calls the resolver inside the spawned future, so the
    // call is observed on the task pool — a frame after the spawn.
    pump_until(&mut app, |_| fake.calls() == 1);
    (app, dir)
}

/// Update until `check` holds. The fake's answer is polled through the async
/// task pool, so it can land on a later frame than the release.
pub(super) fn pump_until(app: &mut App, mut check: impl FnMut(&App) -> bool) {
    for _ in 0..64 {
        if check(app) {
            return;
        }
        app.update();
    }
    panic!("the condition never became true");
}

/// Whether a completed answer is buffered in `HeldDecision`.
pub(super) fn held(app: &App) -> bool {
    app.world().resource::<HeldDecision>().0.is_some()
}

/// Whether the defensive hold is engaged.
pub(super) fn on_hold(app: &App) -> bool {
    app.world().resource::<DefensiveHold>().0
}

/// Whether the runner is parked on a say-line.
pub(super) fn parked_on_say(app: &App) -> bool {
    matches!(
        app.world().resource::<Runtime>().runner.wait(),
        Some(Wait::Say { .. })
    )
}

pub(super) fn app_for_broken_pack() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(
            RuntimeConfig::new(PathBuf::from("definitely/not/a/pack"), None).with_dev_mode(true),
        )
        .add_plugins(RuntimeCorePlugin);
    app.update(); // Startup: the failed load is recorded, not exited
    app
}

/// Build an app parked on the demo decision whose pack has had its deciding NPC
/// removed, so `start_decision` finds no matching NPC (row 11) — otherwise
/// unreachable, since the loader enforces the reference.
pub(super) fn app_with_missing_decide_npc(fake: &FakeResolver) -> App {
    let mut app = app_with_decisions(Some(Arc::new(fake.clone())));
    let loaded = load_pack(&demo_pack_root()).expect("demo pack loads");
    let mut pack = loaded.pack;
    pack.npcs.remove("ace");
    let mut runtime = Runtime {
        runner: ScriptRunner::new(pack),
        staging: StagingState::default(),
        pack_root: demo_pack_root(),
        asset_root: demo_pack_root().join("assets"),
    };
    drive_runner_to_decision(&mut runtime);
    app.world_mut().insert_resource(runtime);
    app
}

/// A test image loader: read the file, reject empty bytes as undecodable,
/// otherwise hand back a plain in-memory image. It lets one sibling load while
/// another fails in the loader, without a renderer.
#[derive(TypePath)]
pub(super) struct PathImageLoader;

impl FromWorld for PathImageLoader {
    fn from_world(_world: &mut bevy::ecs::world::World) -> Self {
        PathImageLoader
    }
}

impl AssetLoader for PathImageLoader {
    type Asset = Image;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        if bytes.is_empty() {
            return Err(std::io::Error::other("undecodable image"));
        }
        Ok(Image::default())
    }

    fn extensions(&self) -> &[&str] {
        &["png"]
    }
}

/// Pin a schedule to the calling thread, so the thread-local log capture sees
/// every line the systems emit (Bevy's default executor may use task threads).
pub(super) fn force_single_threaded(app: &mut App) {
    app.edit_schedule(Update, |schedule| {
        schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    });
}

/// Drive the asset server until every watched load has settled — the same
/// `app.update()` loop Bevy's own asset tests use, with a wall-clock deadline so
/// a busy task pool cannot make the wait spin forever.
pub(super) fn pump_until_watch_drains(app: &mut App) {
    // The load sites add their watches during Update, so the first frame has to
    // run before "settled" can be judged.
    app.update();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut spins = 0u32;
    while !app.world().resource::<AssetWatch>().is_empty() {
        assert!(
            Instant::now() < deadline,
            "the watched asset loads never settled"
        );
        app.update();
        spins += 1;
        if spins.is_multiple_of(64) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// The `unmei::failure` lines captured so far.
pub(super) fn failure_lines() -> Vec<(log::Level, String, String)> {
    captures::take()
        .into_iter()
        .filter(|(_, target, _)| target == crate::failure::LOG_TARGET)
        .collect()
}

pub(super) fn menu(app: &App) -> &MenuState {
    app.world().resource::<MenuState>()
}

/// The one live-session predicate the title branches share.
pub(super) fn live_session(app: &App) -> bool {
    menu::live_session(menu(app))
}

pub(super) fn parked_position(app: &App) -> (usize, Option<Wait>) {
    let runtime = app.world().resource::<Runtime>();
    (runtime.runner.pc(), runtime.runner.wait().cloned())
}

/// Press Esc through the input system: open, back out, or close.
pub(super) fn esc(app: &mut App) {
    press_and_update(app, KeyCode::Escape);
}

/// Click a control by driving the same `Interaction` the pointer would.
pub(super) fn click(app: &mut App, action: MenuAction) {
    app.world_mut().spawn((action, Interaction::Pressed));
    app.update();
}

pub(super) fn switch_mode(app: &mut App, mode: MenuMode) {
    click(app, MenuAction::SetMode(mode));
}

/// Play the demo script to its end through the app systems, taking the first
/// choice and the author fallback at the decide point.
pub(super) fn play_to_end(app: &mut App) {
    for _ in 0..64 {
        match app.world().resource::<Runtime>().runner.wait().cloned() {
            Some(Wait::Say { .. }) => press_and_update(app, KeyCode::Space),
            Some(Wait::Choice { .. }) => press_and_update(app, KeyCode::Digit1),
            Some(Wait::Decision { .. }) => app.update(),
            None => return,
        }
    }
    panic!("the demo script never ended");
}

/// Whether the built pause tree carries a real control with this action. The
/// `MenuAction` a `click` spawns carries no `Button`, so it never counts.
pub(super) fn has_action(app: &App, action: MenuAction) -> bool {
    app.world().iter_entities().any(|entity| {
        entity.get::<MenuAction>() == Some(&action) && entity.get::<Button>().is_some()
    })
}

/// Build the pause tree once, initialising the structural cache a core-only
/// test world does not carry.
pub(super) fn build_pause_tree(app: &mut App) {
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(app);
}

/// The card entity carrying one gallery action.
pub(super) fn action_entity(app: &App, action: MenuAction) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            (entity.get::<MenuAction>() == Some(&action) && entity.get::<Button>().is_some())
                .then_some(entity.id())
        })
        .unwrap_or_else(|| panic!("no built control {action:?}"))
}

/// The one radial gradient `entity` draws, so its stops or centre can be
/// pinned.
pub(super) fn single_radial(app: &App, entity: Entity) -> RadialGradient {
    match app.world().get::<BackgroundGradient>(entity) {
        Some(BackgroundGradient(gradients)) => match gradients.as_slice() {
            [Gradient::Radial(radial)] => radial.clone(),
            other => panic!("one radial gradient expected, got {other:?}"),
        },
        None => panic!("the entity draws a gradient"),
    }
}

/// The running app's settings path: the pack directory's sibling.
pub(super) fn settings_path(app: &App) -> PathBuf {
    let runtime = app.world().resource::<Runtime>();
    settings::settings_path_for(&runtime.pack_root)
}

/// Open the overlay and click through to the Settings view.
pub(super) fn open_settings(app: &mut App) {
    while menu(app).open {
        esc(app);
    }
    esc(app);
    click(app, MenuAction::OpenSettings);
    assert_eq!(menu(app).view, MenuView::Settings);
}

/// The live audio mix.
pub(super) fn audio(app: &App) -> settings::AudioSettings {
    app.world()
        .resource::<SettingsState>()
        .settings
        .audio
        .clone()
}

/// Walk the Settings view's vertical order down by `count` controls.
pub(super) fn focus_down(app: &mut App, count: usize) {
    for _ in 0..count {
        press_and_update(app, KeyCode::ArrowDown);
    }
}

/// The rendered value of one live Settings label, read through the same
/// `sync_settings_labels` the overlay runs. This is the value seam: no
/// rendering, just the string the value node would show. The switch row's sans
/// label is a separate node, so a switch reads back as its bare `On`/`Off`
/// (`docs/title-and-menu.md` § Type system §2/§4).
pub(super) fn settings_switch_text(app: &mut App, kind: SettingsLabelKind) -> String {
    let entity = app
        .world_mut()
        .spawn((SettingsLabel { kind }, Text::new(String::new())))
        .id();
    app.world_mut()
        .run_system_once(crate::presentation::settings_view::sync_settings_labels)
        .expect("the label sync runs");
    app.world()
        .get::<Text>(entity)
        .expect("the spawned label still exists")
        .0
        .clone()
}

/// The prose the play frame drew this frame, empty when it draws none.
pub(super) fn drawn_prose(app: &App) -> String {
    app.world()
        .get_resource::<crate::presentation::play::PlayFrame>()
        .and_then(|frame| frame.prose())
        .unwrap_or_default()
        .to_string()
}

/// The name tab's speaker, when the frame draws one: narration has no tab.
pub(super) fn drawn_speaker(app: &App) -> Option<String> {
    app.world()
        .get_resource::<crate::presentation::play::PlayFrame>()
        .and_then(|frame| frame.speaker())
        .map(str::to_string)
}

/// A text block's root `Text` plus its `TextSpan` children, concatenated in
/// render order.
pub(super) fn flattened_text(app: &App, root: Entity) -> String {
    fn walk(app: &App, entity: Entity, out: &mut String) {
        if let Some(text) = app.world().get::<Text>(entity) {
            out.push_str(&text.0);
        } else if let Some(span) = app.world().get::<TextSpan>(entity) {
            out.push_str(&span.0);
        }
        if let Some(children) = app.world().get::<Children>(entity) {
            for child in children.iter() {
                walk(app, child, out);
            }
        }
    }
    let mut out = String::new();
    walk(app, root, &mut out);
    out
}

/// The parked say-wait, as `(speaker, text)`.
pub(super) fn say_parts(app: &App) -> (String, String) {
    match app.world().resource::<Runtime>().runner.wait() {
        Some(Wait::Say { speaker, text }) => (speaker.clone(), text.clone()),
        other => panic!("expected a say, got {other:?}"),
    }
}

/// Draw the play frame over the current park, through the same four systems the
/// player's `Update` runs in this order. The headless harness registers the
/// renderer but not the presentation plugin, so a test renders on demand.
pub(super) fn render_dialogue(app: &mut App) {
    use crate::presentation::play;
    app.world_mut()
        .run_system_once(play::spawn_play_frame)
        .expect("spawn_play_frame runs");
    app.world_mut()
        .run_system_once(play::read_play_frame)
        .expect("read_play_frame runs");
    app.world_mut()
        .run_system_once(play::sync_play_options)
        .expect("sync_play_options runs");
    app.world_mut()
        .run_system_once(play::sync_play_frame)
        .expect("sync_play_frame runs");
}

/// Age the simulated clock and the reveal; the app is never slept on.
pub(super) fn advance_time(app: &mut App, seconds: f32) {
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(seconds));
    app.world_mut()
        .run_system_once(crate::presentation::play::advance_reveal)
        .expect("advance_reveal runs");
}

/// Set the stored reveal pace directly (persistence is exercised separately).
pub(super) fn set_text_speed(app: &mut App, text_speed: f32) {
    app.world_mut()
        .resource_mut::<SettingsState>()
        .settings
        .play
        .text_speed = text_speed;
}

/// The parked wait, cloned for comparison.
pub(super) fn wait(app: &App) -> Option<Wait> {
    app.world().resource::<Runtime>().runner.wait().cloned()
}

/// Whether `play.auto` is on.
pub(super) fn auto_on(app: &App) -> bool {
    app.world().resource::<SettingsState>().auto()
}

/// Turn auto on through the in-play `A` hotkey, exercising the real input path.
pub(super) fn enable_auto(app: &mut App) {
    assert!(!auto_on(app), "auto starts off");
    press_and_update(app, KeyCode::KeyA);
    assert!(auto_on(app), "A turned auto on");
}

/// Run the core's auto dwell once, with whatever `Time` currently holds.
pub(super) fn tick_auto(app: &mut App) {
    app.world_mut()
        .run_system_once(crate::core::auto_dwell)
        .expect("auto_advance runs");
}

/// Run the real `Update` schedule, so the plugin's `run_if` gates (the overlay
/// and the defensive hold) are honoured exactly as in the app.
pub(super) fn tick_update(app: &mut App) {
    app.world_mut().run_schedule(Update);
}

/// Build the play HUD's own chrome once: the camera and the Notice node.
pub(super) fn build_ui(app: &mut App) {
    app.world_mut()
        .run_system_once(crate::presentation::play::setup_ui)
        .expect("setup_ui runs");
}

/// A copied pack kept alive for the test app's lifetime, so a title test never
/// reads the developer's save files beside `examples/demo-pack`.
#[derive(Resource)]
pub(super) struct TempPack(#[allow(dead_code)] pub(super) tempfile::TempDir);

/// Boot the title over a fresh copy of the demo pack: deterministic (no
/// autosave, so Continue is disabled) and never touching the repo.
pub(super) fn temp_title_app() -> App {
    temp_title_app_with_manifest(None)
}

/// Boot the title over a fresh demo-pack copy, replacing `pack.ron` when a
/// manifest is given (for tagline rendering tests).
pub(super) fn temp_title_app_with_manifest(manifest: Option<&str>) -> App {
    let dir = temp_pack();
    if let Some(manifest) = manifest {
        std::fs::write(dir.path().join("pack/pack.ron"), manifest).unwrap();
    }
    let mut app = boot_app(dir.path().join("pack"), None);
    app.insert_resource(TempPack(dir));
    app
}

/// A title app with the asset rig attached, so a declared `title_backdrop`
/// loads through the real watcher and can be sized to cover. A `PrimaryWindow`
/// supplies the fit's target size.
pub(super) fn title_asset_app(manifest: Option<&str>) -> App {
    title_asset_app_with_assets(manifest, &[])
}

/// As [`title_asset_app`], with extra bytes written under the pack's `assets/`
/// before the pack load — an empty `png` is the loader's undecodable case.
pub(super) fn title_asset_app_with_assets(manifest: Option<&str>, extra: &[(&str, &[u8])]) -> App {
    let dir = temp_pack();
    if let Some(manifest) = manifest {
        std::fs::write(dir.path().join("pack/pack.ron"), manifest).unwrap();
    }
    let pack_root = dir.path().join("pack");
    for (rel, bytes) in extra {
        let path = pack_root.join("assets").join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin {
            file_path: pack_root.join("assets").to_string_lossy().into_owned(),
            ..default()
        })
        // The same 1x1 test double the asset-watch tests use, so a present file
        // loads and a missing one fails without a real decoder.
        .init_asset::<Image>()
        .init_asset_loader::<PathImageLoader>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<AssetWatch>()
        .init_resource::<crate::presentation::MenuBuilt>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    crate::presentation::render::register(&mut app);
    app.add_systems(
        Update,
        (
            crate::presentation::sync_menu_ui,
            crate::presentation::title::fit_title_backdrop,
            report_silent_degradations,
        )
            .chain(),
    );
    app.world_mut().spawn((
        // The real window config, so the title-layout tests target the size and
        // floor the shipping primary window carries.
        settings::window_config(),
        PrimaryWindow,
    ));
    attach_text_faces(&mut app);
    app.insert_resource(TempPack(dir));
    app.update(); // Startup: load the pack, then show the title
    app
}

/// The built title's backdrop image node and layout node, if the title drew one.
pub(super) fn title_backdrop_node(app: &App) -> Option<(ImageNode, Node)> {
    app.world().iter_entities().find_map(|entity| {
        entity
            .get::<crate::presentation::title::TitleBackdropImage>()
            .is_some()
            .then(|| {
                let image = entity.get::<ImageNode>()?.clone();
                let node = entity.get::<Node>()?.clone();
                Some((image, node))
            })
            .flatten()
    })
}

/// Every text node's string, for asserting the title's rendered copy.
pub(super) fn all_texts(app: &App) -> Vec<String> {
    app.world()
        .iter_entities()
        .filter_map(|entity| entity.get::<Text>().map(|text| text.0.clone()))
        .collect()
}

/// The label rendered beside one title action, if the title is built: the
/// control's recorded label when a sibling slot draws it, otherwise the sans
/// root plus any mono caption span, so a hand-built row reads back as one
/// two-line string.
pub(super) fn title_label(app: &App, action: MenuAction) -> Option<String> {
    app.world().iter_entities().find_map(|entity| {
        if entity.get::<MenuAction>() != Some(&action) {
            return None;
        }
        if let Some(label) = entity.get::<crate::presentation::render::ControlLabel>() {
            return Some(label.0.clone());
        }
        entity
            .get::<Text>()
            .is_some()
            .then(|| flattened_text(app, entity.id()))
    })
}

/// Build the title/overlay tree through the real presentation system.
pub(super) fn build_menu_ui(app: &mut App) {
    app.world_mut()
        .run_system_once(crate::presentation::sync_menu_ui)
        .expect("sync_menu_ui runs");
}

/// The built `EditableText` field's entity, if the row built one.
pub(super) fn key_field_entity(app: &App) -> Option<Entity> {
    app.world()
        .iter_entities()
        .find_map(|entity| entity.get::<KeyField>().map(|_| entity.id()))
}

/// Put `text` into the field directly, the way typing would.
pub(super) fn set_field_text(app: &mut App, text: &str) {
    let entity = key_field_entity(app).expect("the key field is built");
    let mut entity = app.world_mut().entity_mut(entity);
    let mut editable = entity.get_mut::<EditableText>().expect("EditableText");
    editable.editor_mut().set_text(text);
}

/// Open Settings and build the UI tree, so the field exists.
pub(super) fn open_settings_with_ui(app: &mut App) {
    open_settings(app);
    app.init_resource::<crate::presentation::MenuBuilt>();
    build_menu_ui(app);
}

/// Install a fake key check on a booted app.
pub(super) fn install_key_check(app: &mut App, checker: &FakeKeyCheck) {
    app.insert_resource(crate::KeyCheckChannel {
        checker: Some(Arc::new(checker.clone())),
    });
}

/// The keys recorded by [`recording_factory`].
pub(super) type RecordedKeys = Arc<Mutex<Vec<String>>>;

/// A resolver builder that records the key it was built from and returns one
/// fixed resolver, so a rebuild is observable and provably keyed.
pub(super) fn recording_factory(
    resolver: Arc<dyn DecisionResolver>,
) -> (RecordedKeys, crate::DecisionFactory) {
    let keys: RecordedKeys = Arc::new(Mutex::new(Vec::new()));
    let log = keys.clone();
    let factory = crate::DecisionFactory {
        builder: Some(Arc::new(move |key: &str| {
            log.lock().unwrap().push(key.to_string());
            resolver.clone()
        })),
    };
    (keys, factory)
}

/// Focus the key row, enter edit mode, type `key`, and commit with `Enter`.
pub(super) fn commit_key_at(app: &mut App, key: &str) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::KeyRow;
    press_and_update(app, KeyCode::Enter);
    assert!(menu(app).key_editing, "Enter enters the field");
    set_field_text(app, key);
    press_and_update(app, KeyCode::Enter);
    assert!(
        !menu(app).key_editing,
        "Enter commits and returns to the walk"
    );
}

/// A booted, play-entered app with the Settings view open, a temp credential
/// directory, a fake key check and a recording resolver factory.
pub(super) struct KeyCheckRig {
    pub(super) app: App,
    pub(super) _pack: tempfile::TempDir,
    pub(super) config_dir: tempfile::TempDir,
    pub(super) path: PathBuf,
    pub(super) checker: FakeKeyCheck,
    pub(super) keys: RecordedKeys,
}

pub(super) fn key_check_rig(resolver: Arc<dyn DecisionResolver>) -> KeyCheckRig {
    let pack = temp_pack();
    let config_dir = tempfile::tempdir().unwrap();
    let path = credentials::credentials_path_in(config_dir.path());
    let checker = FakeKeyCheck::new();
    let (keys, factory) = recording_factory(resolver);
    let mut app = app_for_pack(pack.path().join("pack"));
    app.insert_resource(CredentialsState {
        config_dir: Some(config_dir.path().to_path_buf()),
        path: Some(path.clone()),
        row: KeyRow::NotSet,
    });
    install_key_check(&mut app, &checker);
    app.insert_resource(factory);
    open_settings_with_ui(&mut app);
    KeyCheckRig {
        app,
        _pack: pack,
        config_dir,
        path,
        checker,
        keys,
    }
}

/// The bundled face a text entity names. Panics on Bevy's default handle or a
/// family-name source, so a stray default font fails the test
/// (`docs/title-and-menu.md` § Type system §5).
pub(super) fn face_handle(app: &App, entity: Entity) -> Handle<bevy::text::Font> {
    match &app
        .world()
        .get::<TextFont>(entity)
        .unwrap_or_else(|| panic!("{entity:?} has no TextFont"))
        .font
    {
        bevy::text::FontSource::Handle(handle) => handle.clone(),
        other => panic!("{entity:?} names a non-bundled font source: {other:?}"),
    }
}

/// The first entity whose `Text` or `TextSpan` is exactly `text`.
pub(super) fn text_entity(app: &App, text: &str) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| {
            let matches = entity.get::<Text>().is_some_and(|t| t.0 == text)
                || entity.get::<TextSpan>().is_some_and(|t| t.0 == text);
            matches.then_some(entity.id())
        })
        .unwrap_or_else(|| panic!("no text entity {text:?}"))
}

/// The running app's read-log sibling path.
pub(super) fn read_log_path(app: &App) -> PathBuf {
    let runtime = app.world().resource::<Runtime>();
    unmei_engine::read_log::read_log_path_for(runtime.runner.pack())
}

/// The in-memory read-log state.
pub(super) fn read_state(app: &App) -> &crate::read_log::ReadLogState {
    app.world().resource::<crate::read_log::ReadLogState>()
}

/// The log decoded from disk, or `None` when no file exists.
pub(super) fn read_log_on_disk(app: &App) -> Option<unmei_engine::read_log::ReadLog> {
    let path = read_log_path(app);
    path.exists()
        .then(|| unmei_engine::read_log::load_read_log(&path).0)
}

/// Seed the sibling file with a hand-built log before the app boots.
pub(super) fn seed_read_log(pack_root: &Path, log: &unmei_engine::read_log::ReadLog) {
    let mut pack = load_pack(pack_root).expect("pack loads").pack;
    pack.root = pack_root.to_path_buf();
    let path = unmei_engine::read_log::read_log_path_for(&pack);
    unmei_engine::read_log::write_read_log(&path, log).unwrap();
}

/// Whether the seen skip is engaged.
pub(super) fn skip_on(app: &App) -> bool {
    app.world().resource::<crate::core::SkipState>().is_on()
}

/// The one entity carrying component `T`.
pub(super) fn entity_with<T: bevy::prelude::Component>(app: &App) -> Entity {
    app.world()
        .iter_entities()
        .find_map(|entity| entity.get::<T>().map(|_| entity.id()))
        .expect("the component exists")
}

/// Replace the demo pack's entry scene with a say-only script. The first say
/// sits on source line 2 (line 1 is the leading `bg`).
pub(super) fn pack_with_entry_says(says: &[&str]) -> tempfile::TempDir {
    let pack = temp_pack();
    let mut script = String::from("bg school_gate\n");
    for say in says {
        script.push_str(say);
        script.push('\n');
    }
    std::fs::write(pack.path().join("pack/scenes/first_meeting.unmei"), script).unwrap();
    pack
}

/// Seed every say-line of `scene_id` as arrival-read.
pub(super) fn seed_scene_read(pack_root: &Path, scene_id: &str) {
    let pack = load_pack(pack_root).expect("pack loads").pack;
    let scene = pack.scenes.get(scene_id).expect("scene exists");
    let mut lines = std::collections::BTreeSet::new();
    for (index, line) in scene.lines.iter().enumerate() {
        if matches!(line, unmei_engine::content::Line::Say { .. }) {
            lines.insert(scene.source_line(index));
        }
    }
    let mut log = unmei_engine::read_log::ReadLog::default();
    log.mark_visited(scene_id);
    for line in lines {
        log.mark_read(scene_id, line);
    }
    seed_read_log(pack_root, &log);
}

/// Boot into play with every entry-scene say-line already known.
pub(super) fn app_with_known_entry(pack: &tempfile::TempDir) -> App {
    let pack_root = pack.path().join("pack");
    seed_scene_read(&pack_root, "first_meeting");
    app_for_pack(pack_root)
}

/// Whether the force hold is effective this frame.
pub(super) fn skip_force(app: &App) -> bool {
    app.world()
        .resource::<crate::core::SkipState>()
        .is_force_active()
}

/// A booted app whose key state comes from the real [`bevy::input::InputPlugin`]:
/// a held key is a genuine multi-frame `ButtonInput` hold built from
/// `KeyboardInput` press/release messages, not a `press_and_update`/`reset_all`
/// simulation. `Time` is advanced only through [`advance_time`], so the dwell is
/// deterministic.
pub(super) fn boot_app_with_input(
    pack_root: PathBuf,
    decisions: Option<Arc<dyn DecisionResolver>>,
) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::input::InputPlugin)
        .insert_resource(RuntimeConfig::new(pack_root, decisions).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    attach_text_faces(&mut app);
    app.update(); // Startup: load the pack, then show the title
                  // Zero the real boot delta so every later `step_input` frame is controlled
                  // only through `advance_time`.
    advance_time(&mut app, 0.0);
    app
}

pub(super) fn send_key_message(
    app: &mut App,
    key: KeyCode,
    state: bevy::input::ButtonState,
    repeat: bool,
) {
    use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
    app.world_mut().write_message(KeyboardInput {
        key_code: key,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        text: None,
        repeat,
        window: Entity::PLACEHOLDER,
    });
}

pub(crate) fn press_key(app: &mut App, key: KeyCode) {
    send_key_message(app, key, bevy::input::ButtonState::Pressed, false);
}

pub(super) fn release_key(app: &mut App, key: KeyCode) {
    send_key_message(app, key, bevy::input::ButtonState::Released, false);
}

/// One production-faithful frame: the real `PreUpdate` keyboard system settles
/// `ButtonInput`, then the app's `Update` schedule runs. Deliberately not
/// `app.update()`, which would let the real `TimePlugin` overwrite the test's
/// simulated delta.
pub(crate) fn step_input(app: &mut App) {
    app.world_mut().run_schedule(PreUpdate);
    app.world_mut().run_schedule(Update);
}

/// A tap through the real keyboard chain: press, one frame, release, one frame.
pub(crate) fn tap_key(app: &mut App, key: KeyCode) {
    press_key(app, key);
    step_input(app);
    release_key(app, key);
    step_input(app);
}

/// Cross the title's New Game through `KeyboardInput` messages.
pub(crate) fn enter_play_with_input(app: &mut App) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press_key(app, KeyCode::Enter);
    step_input(app);
    assert!(!menu(app).open, "input enter_play must start play");
    release_key(app, KeyCode::Enter);
    step_input(app);
}

/// Click a control and run one production-faithful frame.
pub(super) fn click_input(app: &mut App, action: MenuAction) {
    app.world_mut().spawn((action, Interaction::Pressed));
    step_input(app);
}

/// A copied demo pack (so `state.ron`, `npcs/` and assets survive) whose scene
/// files and manifest are replaced by the given scenes. `entry` need not be in
/// `order`, so an unlisted entry can be the only visited scene.
pub(super) fn temp_custom_pack(
    scenes: &[(&str, &str)],
    order: Option<&[&str]>,
    entry: &str,
) -> tempfile::TempDir {
    let dir = temp_pack();
    let root = dir.path().join("pack");
    for scene in std::fs::read_dir(root.join("scenes")).unwrap() {
        std::fs::remove_file(scene.unwrap().path()).unwrap();
    }
    for (id, body) in scenes {
        std::fs::write(root.join(format!("scenes/{id}.unmei")), body).unwrap();
    }
    let scenes_field = match order {
        None => String::new(),
        Some(list) => format!(
            "    scenes: [{}],\n",
            list.iter()
                .map(|id| format!("\"{id}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    std::fs::write(
        root.join("pack.ron"),
        format!("(\n    name: \"timeline test\",\n    entry: \"{entry}\",\n{scenes_field})\n"),
    )
    .unwrap();
    dir
}

/// Seed the sibling read log with the given visited scenes before boot.
pub(super) fn seed_visited(pack_root: &Path, visited: &[&str]) {
    let mut log = unmei_engine::read_log::ReadLog::default();
    for id in visited {
        log.mark_visited(id);
    }
    seed_read_log(pack_root, &log);
}

// The seam is one resource (`InputOwnership`) read by `input_unclaimed`. These
// tests drive the real Bevy input/focus/text/clipboard pipeline end to end —
// `KeyboardInput` messages through focus dispatch, `EditableText.pending_edits`
// through the PostUpdate text apply, a propagated `Pointer<Press>` observer and
// a Settings `Interaction` press — rather than a reset-all `ButtonInput` flag,
// so a claim is proven to suppress (and release to restore) the production
// paths, not a mock.

/// A production-shaped full-frame app: the real Bevy input, focus dispatch,
/// clipboard and editable-text plugins over [`RuntimeCorePlugin`], plus the
/// app's own queued-edit drain, outside-press observer and Settings drag. The
/// returned `Entity` is the primary window for `KeyboardInput` messages.
pub(crate) fn boot_widget_input_app(pack_root: PathBuf) -> (App, Entity) {
    use bevy::clipboard::ClipboardPlugin;
    use bevy::input_focus::{InputDispatchPlugin, InputFocusPlugin};
    use bevy::picking::events::{Pointer, Release};
    use bevy::text::{apply_text_edits, FontCx, LayoutCx};
    use bevy::ui::UiScale;
    use bevy::ui_widgets::EditableTextInputPlugin;
    use bevy::window::Ime;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::input::InputPlugin)
        .add_plugins(InputFocusPlugin)
        .add_plugins(InputDispatchPlugin)
        .add_plugins(ClipboardPlugin)
        .add_plugins(EditableTextInputPlugin)
        .init_resource::<ClipboardPort>()
        .init_resource::<FontCx>()
        .init_resource::<LayoutCx>()
        .init_resource::<UiScale>()
        .add_message::<Ime>()
        .add_message::<Pointer<Release>>()
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(RuntimeCorePlugin);
    attach_text_faces(&mut app);
    crate::presentation::render::register(&mut app);
    app.add_observer(crate::presentation::settings_view::discard_key_edit_on_outside_press)
        .add_systems(
            PostUpdate,
            apply_text_edits.in_set(bevy::text::EditableTextSystems),
        );
    // The production ownership gates (Settings drag, trimmed paste, the
    // queued-edit drain), registered by the same function `UnmeiPlugin` uses, so
    // this harness cannot drift from the shipped wiring.
    crate::register_ownership_gated_presentation(&mut app);
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    app.update(); // Startup: load the pack, then show the title
    (app, window)
}

/// Spawn a focused editable key field, the production `KeyField` widget shape.
pub(super) fn spawn_key_field(app: &mut App) -> Entity {
    use bevy::input_focus::{FocusCause, InputFocus};
    use bevy::text::EditableTextFilter;

    let field = app
        .world_mut()
        .spawn((
            crate::credentials::KeyField,
            EditableText::new(""),
            Node::default(),
            TextFont::default(),
            TextColor::default(),
            EditableTextFilter::new(|character| character != '\n' && character != '\r'),
        ))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(field, FocusCause::Navigated);
    field
}

/// The focused field's committed text, read from the entity under test rather
/// than by scanning for a built `KeyField`.
pub(super) fn field_value(app: &App, field: Entity) -> String {
    app.world()
        .entity(field)
        .get::<EditableText>()
        .expect("the field is an EditableText")
        .value()
        .to_string()
}

/// Type one character through the real keyboard chain and run a full frame,
/// then release it on the next. The press and release are separate frames so an
/// `EditableText` observer sees them exactly as production would.
pub(super) fn type_character(app: &mut App, window: Entity, character: char) {
    use bevy::input::keyboard::{Key, KeyboardInput};
    use bevy::input::ButtonState;

    for (state, text) in [
        (ButtonState::Pressed, Some(character.to_string())),
        (ButtonState::Released, None),
    ] {
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::KeyA,
            logical_key: Key::Character(character.to_string().into()),
            state,
            text: text.map(Into::into),
            repeat: false,
            window,
        });
        app.update();
    }
}

/// Fire a propagated `Pointer<Press>` at `target` through the real observer
/// path and run a full frame.
pub(super) fn outside_press(app: &mut App, target: Entity) {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::backend::HitData;
    use bevy::picking::events::{Pointer, Press};
    use bevy::picking::pointer::{Location, PointerButton, PointerId};

    let event = Pointer::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::None {
                width: 1280,
                height: 720,
            },
            position: Vec2::ZERO,
        },
        Press {
            button: PointerButton::Primary,
            hit: HitData {
                camera: Entity::PLACEHOLDER,
                depth: 0.0,
                position: None,
                normal: None,
                extra: None,
            },
            count: 1,
        },
        target,
    );
    app.world_mut()
        .entity_mut(target)
        .trigger(|_| event.clone());
    app.update();
}

/// Paste through the real keyboard chain and the in-process clipboard, running
/// a full frame for the press and the release.
pub(super) fn paste(app: &mut App, window: Entity) {
    use bevy::input::keyboard::{Key, KeyboardInput};
    use bevy::input::ButtonState;

    app.world_mut()
        .resource_mut::<ButtonInput<Key>>()
        .press(Key::Control);
    for (state, text) in [
        (ButtonState::Pressed, Some("v".to_string())),
        (ButtonState::Released, None),
    ] {
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::KeyV,
            logical_key: Key::Character("v".into()),
            state,
            text: text.map(Into::into),
            repeat: false,
            window,
        });
        app.update();
    }
    app.world_mut()
        .resource_mut::<ButtonInput<Key>>()
        .release(Key::Control);
    app.update();
}

pub(super) fn drawer_panel(app: &App) -> &drawer::MonitorPanel {
    app.world()
        .get_resource::<drawer::MonitorPanel>()
        .expect("dev mode installs the drawer state")
}

pub(crate) fn drawer_open(app: &App) -> bool {
    drawer_panel(app).is_open()
}

pub(crate) fn drawer_view(app: &App) -> MonitorView {
    drawer_panel(app).view()
}

pub(super) fn drawer_focus(app: &App) -> MonitorControl {
    drawer_panel(app).focus()
}

pub(super) fn drawer_control_entity(app: &mut App, control: MonitorControl) -> Entity {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &drawer::MonitorControlTag)>();
    query
        .iter(world)
        .find(|(_, tag)| tag.0 == control)
        .map(|(entity, _)| entity)
        .expect("the drawer control exists")
}

/// Activate a control through the component the production `Interaction` write
/// lands on. Real cursor hit-testing belongs to the rendered walk; this
/// drives the same activation branch without a UI camera.
pub(crate) fn activate_control(app: &mut App, control: MonitorControl) {
    let entity = drawer_control_entity(app, control);
    app.world_mut()
        .entity_mut(entity)
        .insert(Interaction::Pressed);
    step_input(app);
    // `Close` despawns the tree on that frame; the release is only meaningful
    // while the control still exists.
    if let Ok(mut control) = app.world_mut().get_entity_mut(entity) {
        control.insert(Interaction::None);
    }
    step_input(app);
}

pub(super) fn test_monitor_line(
    sequence: u64,
    target: &str,
    level: log::Level,
    message: &str,
) -> MonitorLine {
    let text = format!("{sequence:010}ms {level:<5} {target} {message}");
    let body_start = text.len() - message.len();
    MonitorLine {
        sequence,
        elapsed_ms: sequence,
        level,
        target: target.to_string(),
        target_display: target.to_string(),
        text,
        body_start,
        truncated: false,
    }
}

pub(super) fn seed_monitor_history(app: &mut App, lines: Vec<MonitorLine>) {
    app.world_mut().init_resource::<MonitorHistory>();
    app.world_mut()
        .resource_mut::<MonitorHistory>()
        .seed_lines(lines);
}

/// Every materialized row's part texts, oldest row first — the display the
/// developer actually sees.
pub(super) fn drawer_rows(app: &mut App) -> Vec<(u64, MonitorRowPart, String)> {
    let world = app.world_mut();
    let rows: Vec<(u64, Entity)> = {
        let mut query = world.query::<(Entity, &MonitorRow)>();
        query.iter(world).map(|(e, row)| (row.0, e)).collect()
    };
    let parts: Vec<(Entity, MonitorRowPart, String)> = {
        let mut query = world.query::<(Entity, &MonitorRowPart, &Text)>();
        query
            .iter(world)
            .map(|(e, part, text)| (e, *part, text.as_str().to_string()))
            .collect()
    };
    let children: Vec<(Entity, Entity)> = {
        let mut query = world.query::<(Entity, &ChildOf)>();
        query
            .iter(world)
            .map(|(child, parent)| (parent.parent(), child))
            .collect()
    };
    let mut out = Vec::new();
    for (sequence, entity) in rows {
        for (part_entity, part, text) in &parts {
            if children
                .iter()
                .any(|(parent, child)| *parent == entity && child == part_entity)
            {
                out.push((sequence, *part, text.clone()));
            }
        }
    }
    out.sort_by_key(|(sequence, part, _)| (*sequence, *part as u8));
    out
}

pub(super) fn drawer_status(app: &mut App) -> String {
    let world = app.world_mut();
    let mut query = world.query_filtered::<&Text, With<MonitorStatusText>>();
    query
        .iter(world)
        .map(|text| text.as_str().to_string())
        .next()
        .expect("the drawer status line exists")
}

pub(super) fn has_component<T: Component>(app: &mut App) -> bool {
    let world = app.world_mut();
    let mut query = world.query_filtered::<Entity, With<T>>();
    query.iter(world).next().is_some()
}

// The reader is driven through the production paths: real `KeyboardInput`
// press/release/repeat for paging and Home/End, real `MouseWheel` messages
// hit-tested against the log body's own laid-out rect, and the real widget
// `Interaction` that starts a scrollbar drag. Assertions are on the rendered
// row window, the stable event anchor and the panel-local status.

/// A drawer harness with a real primary window, so cursor-driven wheel and
/// scrollbar hit-testing have a window to address. The world is otherwise the
/// same `RuntimeCorePlugin` one the other drawer tests use.
pub(super) fn boot_reader_app(pack_root: PathBuf) -> App {
    let mut app = boot_app_with_input(pack_root, None);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    app
}

/// Step frames until the current-predicate projection pass has settled.
pub(super) fn settle_reader(app: &mut App) {
    for _ in 0..20 {
        step_input(app);
        if !app
            .world()
            .resource::<crate::drawer::MonitorProjection>()
            .is_updating()
        {
            return;
        }
    }
    panic!("the projection never settled");
}

pub(super) fn seed_numbered_monitor(app: &mut App, range: std::ops::Range<u64>, target: &str) {
    seed_monitor_history(
        app,
        range
            .map(|i| test_monitor_line(i, target, log::Level::Info, &format!("event {i}")))
            .collect(),
    );
}

/// Admit numbered `unmei::seq` lines through the production capture layer and
/// let the real ingest path hand them to retained history.
///
/// The capture assigns the admission identities, so a range that starts where
/// the previous call stopped keeps them consecutive. Each chunk is ingested
/// before the next is admitted: admitting more than
/// [`crate::monitor::INGEST_PER_UPDATE`] at once would overflow the pending
/// budget and lose lines that the caller meant to retain.
pub(super) fn capture_numbered_monitor(
    app: &mut App,
    capture: &crate::monitor::MonitorCapture,
    range: std::ops::Range<u64>,
) {
    use bevy::log::tracing;
    use bevy::log::tracing_subscriber::prelude::*;

    let mut next = range.start;
    while next < range.end {
        let chunk_end = (next + crate::monitor::INGEST_PER_UPDATE as u64).min(range.end);
        let subscriber =
            bevy::log::tracing_subscriber::registry().with(crate::monitor::capture_layer(capture));
        tracing::subscriber::with_default(subscriber, || {
            for index in next..chunk_end {
                tracing::info!(target: "unmei::seq", "event {index}");
            }
        });
        step_input(app);
        next = chunk_end;
    }
}

/// The materialized rows' admission sequences in display order, top row first
/// — the window the developer actually sees.
pub(super) fn drawer_window(app: &mut App) -> Vec<u64> {
    let world = app.world_mut();
    let content = {
        let mut query = world.query_filtered::<Entity, With<drawer::MonitorRowsContent>>();
        query.iter(world).next().expect("the rows content exists")
    };
    let children: Vec<Entity> = world
        .get::<Children>(content)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    children
        .iter()
        .filter_map(|entity| world.get::<MonitorRow>(*entity).map(|row| row.0))
        .collect()
}
