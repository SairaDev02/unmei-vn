use super::*;

/// Play reads every pack asset from [`RuntimeConfig`]'s one asset root, never
/// from `<pack_root>/assets` joined on the side (`docs/keiko-spec.md` §4.2).
///
/// The fixture moves the demo pack's whole `assets/` folder beside the pack and
/// points the config at it, so the pack folder holds no assets of its own: the
/// play frame's background and sprite can only resolve under the configured
/// root. Image loading needs a renderer in Bevy 0.19 — the headless
/// composition's image handles never leave `Loading` — so this asserts the
/// production reads the runtime makes (the runtime's own root and the file it
/// resolves), not a decoded GPU texture.
#[test]
fn a_configured_asset_root_supplies_the_background_and_the_sprite() {
    let dir = tempfile::tempdir().unwrap();
    let pack_root = dir.path().join("pack");
    let asset_root = dir.path().join("shared-assets");
    std::fs::create_dir_all(pack_root.join("scenes")).unwrap();
    std::fs::create_dir_all(pack_root.join("npcs")).unwrap();
    std::fs::write(
        pack_root.join("pack.ron"),
        "(name: \"asset root\", entry: \"start\")",
    )
    .unwrap();
    std::fs::write(pack_root.join("state.ron"), "(met: false,)").unwrap();
    std::fs::write(
        pack_root.join("scenes/start.unmei"),
        "bg school_gate
sprite ace neutral
Narrator: sourced elsewhere
",
    )
    .unwrap();
    for relative in [
        String::from("backgrounds/school_gate.png"),
        crate::presentation::assets::sprite_asset_path("ace", "neutral"),
    ] {
        let source = demo_pack_root().join("assets").join(&relative);
        let destination = asset_root.join(&relative);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(&source, &destination).unwrap();
    }
    assert!(
        !pack_root.join("assets").exists(),
        "the fixture must not keep an assets folder inside the pack"
    );

    // The production headless composition, so this covers the whole chain the
    // shipping player uses: the asset server's root, the loaded runtime's asset
    // root and the play frame's staging syncs.
    let mut app = crate::composition::schedule_app(
        RuntimeConfig::new(pack_root, None)
            .with_dev_mode(true)
            .with_asset_root(asset_root.clone()),
    );
    app.update(); // Startup: load the pack, then show the title
    assert_eq!(
        app.world().resource::<Runtime>().asset_root,
        asset_root,
        "the loaded runtime carries the configured asset root"
    );

    // The title surface draws no play frame. Close it — `enter_play`'s own
    // effect — so the real staging syncs draw the frame this pack opens on.
    app.world_mut().resource_mut::<MenuState>().open = false;
    app.update();

    let runtime = app.world().resource::<Runtime>();
    assert_eq!(runtime.staging.background.as_deref(), Some("school_gate"));
    assert_eq!(
        runtime.staging.sprites.get("ace").map(String::as_str),
        Some("neutral")
    );
    for relative in [
        "backgrounds/school_gate.png".to_string(),
        crate::presentation::assets::sprite_asset_path("ace", "neutral"),
    ] {
        assert!(
            runtime.asset_root.join(&relative).exists(),
            "{relative} must resolve under the configured asset root"
        );
    }
    let mut backgrounds = app
        .world_mut()
        .query_filtered::<Entity, With<crate::presentation::play::BackgroundSprite>>();
    assert_eq!(
        backgrounds.iter(app.world()).count(),
        1,
        "the play frame's staging sync draws the background"
    );
    let mut sprites = app
        .world_mut()
        .query_filtered::<Entity, With<crate::presentation::play::NpcSprite>>();
    assert_eq!(
        sprites.iter(app.world()).count(),
        1,
        "the play frame's staging sync draws the sprite"
    );
}

/// The default root is the pack's own `assets/` folder, so every existing caller
/// keeps today's behavior; the builder moves it without touching the pack.
#[test]
fn the_asset_root_defaults_beside_the_pack_and_the_builder_moves_it() {
    let config = RuntimeConfig::new(PathBuf::from("/packs/demo"), None);
    assert_eq!(config.asset_root, PathBuf::from("/packs/demo/assets"));

    let moved = config.with_asset_root(PathBuf::from("/shared/assets"));
    assert_eq!(moved.asset_root, PathBuf::from("/shared/assets"));
    assert_eq!(moved.pack_root, PathBuf::from("/packs/demo"));
}
