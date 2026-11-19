use super::*;

#[test]
fn core_parks_on_the_entry_scene_say() {
    let app = headless_app();
    let runtime = app.world().resource::<Runtime>();
    assert!(matches!(runtime.runner.wait(), Some(Wait::Say { .. })));
    assert_eq!(runtime.runner.scene_id(), "first_meeting");
}

#[test]
fn staging_snapshot_tracks_the_branch_taken() {
    let mut app = headless_app();
    let mut runtime = app.world_mut().resource_mut::<Runtime>();
    drive_runner_to_decision(&mut runtime);

    let staging = &runtime.staging;
    assert_eq!(staging.background.as_deref(), Some("school_gate"));
    assert_eq!(
        staging.sprites.get("ace").map(String::as_str),
        Some("neutral")
    );
    // Both scenes `play music_*`; the stop+play pair leaves the reunion track.
    assert_eq!(staging.bgm.as_deref(), Some("music_evening"));
}

/// Walk the whole demo script with the decision resolved to `action`,
/// returning every asset path the run staged.
fn staged_asset_paths(action: &str) -> Vec<PathBuf> {
    use unmei_engine::runner::StageCommand;

    let mut app = headless_app();
    let mut runtime = app.world_mut().resource_mut::<Runtime>();
    let asset_root = runtime.asset_root.clone();
    let mut paths = Vec::new();

    // `headless_app`'s first update already consumed the entry scene's
    // `bg`/`play`, so seed from what it left staged before walking on.
    if let Some(id) = &runtime.staging.background {
        paths.push(asset_root.join(format!("backgrounds/{id}.png")));
    }
    for (npc_id, pose) in &runtime.staging.sprites {
        paths.push(asset_root.join(crate::presentation::assets::sprite_asset_path(npc_id, pose)));
    }
    if let Some(id) = &runtime.staging.bgm {
        paths.push(asset_root.join(format!("audio/{id}.ogg")));
    }

    loop {
        let outcome = runtime.runner.run().unwrap();
        runtime.staging.apply_all(&outcome.staged);
        for command in &outcome.staged {
            let relative = match command {
                StageCommand::Bg(id) => Some(format!("backgrounds/{id}.png")),
                StageCommand::Sprite { npc_id, pose } => {
                    Some(crate::presentation::assets::sprite_asset_path(npc_id, pose))
                }
                StageCommand::Play(id) => Some(format!("audio/{id}.ogg")),
                StageCommand::Hide(_) | StageCommand::Stop(_) => None,
            };
            if let Some(relative) = relative {
                paths.push(asset_root.join(relative));
            }
        }
        match outcome.wait {
            Some(Wait::Say { .. }) => runtime.runner.advance_say().unwrap(),
            Some(Wait::Choice { .. }) => runtime.runner.choose(0).unwrap(),
            Some(Wait::Decision { .. }) => runtime.runner.resolve_decision(action).unwrap(),
            None => break,
        }
    }
    paths
}

/// Positive asset check: a boot smoke test only shows the *absence* of
/// loader errors, so assert here that every path the runtime stages for the
/// demo actually resolves to a file on disk — including both decision
/// branches and the hardwired choice SFX the script never stages.
#[test]
fn every_asset_the_demo_script_stages_exists() {
    let mut paths = staged_asset_paths("cold");
    paths.extend(staged_asset_paths("warm"));
    paths.push(demo_pack_root().join("assets/audio/sfx_select.ogg"));
    paths.sort();
    paths.dedup();

    for path in &paths {
        assert!(
            path.exists(),
            "demo pack is missing a staged asset: {}",
            path.display()
        );
    }

    let names: Vec<String> = paths
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    for expected in [
        "school_gate.png",
        "neutral.png",
        "happy.png",
        "music_morning.ogg",
        "music_evening.ogg",
        "sfx_paper_rustle.ogg",
        "sfx_select.ogg",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "the demo walk never staged {expected} (got {names:?})"
        );
    }
}

/// The play path's sprite rule has one producer — `sprites/<npc_id>/<pose>.png`
/// — so the gallery cannot fork a second inline copy
/// (`docs/title-and-menu.md` § Gallery § Entries §4).
#[test]
fn sprite_asset_path_builds_the_play_rule() {
    assert_eq!(
        crate::presentation::assets::sprite_asset_path("ace", "happy"),
        "sprites/ace/happy.png"
    );
}
