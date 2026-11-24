//! The play surface's sync systems: staging visuals, the mix, audio playback,
//! the silent-degradation report and the reveal clock. The frame's words are
//! [`super::frame`]'s.

use bevy::prelude::*;
use bevy_kira_audio::{AudioChannel, AudioControl as AudioPlaybackControl};

use super::super::assets::{
    audio_asset_path, background_asset_path, sprite_asset_path, AssetWatch,
};
use super::reveal::TextReveal;
use super::state::{
    AppliedAudio, AppliedVisuals, AppliedVolume, BackgroundSprite, Bgm, NpcSprite, Sfx,
};
use crate::failure::{Failure, FailureRow, Failures};
use crate::menu::MenuState;
use crate::settings::{self, SettingsState};
use crate::Runtime;

/// Rebuild background + NPC sprite entities whenever the staging snapshot changes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_staging_visuals(
    mut commands: Commands,
    runtime: Res<Runtime>,
    menu: Option<Res<MenuState>>,
    mut applied: ResMut<AppliedVisuals>,
    mut watch: ResMut<AssetWatch>,
    asset_server: Res<AssetServer>,
    backgrounds: Query<Entity, With<BackgroundSprite>>,
    npcs: Query<Entity, With<NpcSprite>>,
) {
    // The title's surfaces draw a neutral backdrop and nothing of the play
    // frame: the world sprites are removed while one is up, and the applied
    // bookkeeping is reset so entering play rebuilds them.
    if menu.as_deref().is_some_and(MenuState::title_surface_up) {
        for entity in &backgrounds {
            commands.entity(entity).despawn();
        }
        for entity in &npcs {
            commands.entity(entity).despawn();
        }
        applied.bg = None;
        applied.sprites.clear();
        applied.force = false;
        return;
    }

    let want_bg = runtime.staging.background.clone();
    if applied.force || applied.bg != want_bg {
        for entity in &backgrounds {
            commands.entity(entity).despawn();
        }
        if let Some(id) = &want_bg {
            // Row 3: a missing or undecodable background leaves nothing drawn
            // and logs once; the next `bg` replaces it.
            if let Some(image) = watch.watch_image(
                &asset_server,
                FailureRow::BackgroundAsset,
                background_asset_path(id),
            ) {
                commands.spawn((
                    BackgroundSprite,
                    Sprite {
                        image,
                        custom_size: Some(Vec2::new(1280.0, 720.0)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, 0.0),
                ));
            }
        }
        applied.bg = want_bg;
    }

    if applied.force || applied.sprites != runtime.staging.sprites {
        for entity in &npcs {
            commands.entity(entity).despawn();
        }
        let count = runtime.staging.sprites.len();
        for (position, (npc_id, pose)) in runtime.staging.sprites.iter().enumerate() {
            let x = (position as f32 - (count.saturating_sub(1)) as f32 / 2.0) * 240.0;
            // Row 4: only the failed pose is absent; the other sprites stay.
            if let Some(image) = watch.watch_image(
                &asset_server,
                FailureRow::SpriteAsset,
                sprite_asset_path(npc_id, pose),
            ) {
                commands.spawn((
                    NpcSprite {
                        npc_id: npc_id.clone(),
                    },
                    Sprite::from_image(image),
                    Transform::from_xyz(x, -120.0, 1.0),
                ));
            }
        }
        applied.sprites = runtime.staging.sprites.clone();
    }

    applied.force = false;
}

/// Apply the stored mix to the two app channels. Master scales each channel
/// arithmetically, mute forces silence, and the amplitude maps to decibels with
/// `0.0`/non-finite special-cased to [`settings::SILENCE_DECIBELS`]. A
/// `set_volume` re-applies to sounds already playing *and* is remembered for the
/// next `play`, so a change is audible immediately without a stop/play.
pub(crate) fn sync_audio_volume(
    settings: Option<Res<SettingsState>>,
    bgm: Option<Res<AudioChannel<Bgm>>>,
    sfx: Option<Res<AudioChannel<Sfx>>>,
    mut applied: ResMut<AppliedVolume>,
) {
    let Some(settings) = settings else {
        return;
    };
    let audio = &settings.settings.audio;
    let bgm_db = settings::amplitude_to_decibels(settings::effective_amplitude(
        audio.master,
        audio.music,
        audio.muted,
    ));
    let sfx_db = settings::amplitude_to_decibels(settings::effective_amplitude(
        audio.master,
        audio.sfx,
        audio.muted,
    ));
    if applied.bgm == bgm_db && applied.sfx == sfx_db {
        return;
    }
    // Only advance the bookkeeping for a channel that was actually set, so a
    // channel created after this system first ran still receives the mix.
    if applied.bgm != bgm_db {
        if let Some(bgm) = bgm {
            bgm.set_volume(bgm_db);
            applied.bgm = bgm_db;
        }
    }
    if applied.sfx != sfx_db {
        if let Some(sfx) = sfx {
            sfx.set_volume(sfx_db);
            applied.sfx = sfx_db;
        }
    }
}

/// Keep the BGM channel in step with `staging.bgm`: every `play` loops, `stop`
/// silences, and a load forces a fresh (re)start from the top.
pub(crate) fn sync_audio(
    runtime: Res<Runtime>,
    menu: Option<Res<MenuState>>,
    mut applied: ResMut<AppliedAudio>,
    mut watch: ResMut<AssetWatch>,
    asset_server: Res<AssetServer>,
    bgm: Res<AudioChannel<Bgm>>,
) {
    // The title's surfaces are silent: the entry scene's `play` is staged but
    // does not sound. Stopping here and dropping the applied id makes entering
    // play start the track from the top, exactly as a load does.
    if menu.as_deref().is_some_and(MenuState::title_surface_up) {
        bgm.stop();
        applied.bgm = None;
        applied.force = false;
        return;
    }
    if !applied.force && applied.bgm == runtime.staging.bgm {
        return;
    }
    bgm.stop();
    if let Some(id) = &runtime.staging.bgm {
        // Row 5: a missing or undecodable track is silence — the channel is
        // stopped and nothing plays until the next `play`.
        if let Some(source) =
            watch.watch_audio(&asset_server, FailureRow::BgmAsset, audio_asset_path(id))
        {
            bgm.play(source).looped();
        }
    }
    applied.bgm = runtime.staging.bgm.clone();
    applied.force = false;
}

/// Report the silent asset degradations the load sites are watching (rows 3–6):
/// one `warn!` under `unmei::failure` per path, and never a Notice.
///
/// Each row is chosen at its load site; this system only turns each settled
/// failure into a [`Failure`] and hands it to the funnel. A failed path is
/// remembered, so a repeated `bg`/`sprite`/`play` command or a re-`load` of a
/// failed path logs exactly once; a loaded asset is dropped without a line.
pub(crate) fn report_silent_degradations(
    asset_server: Res<AssetServer>,
    mut watch: ResMut<AssetWatch>,
    mut failures: Failures,
) {
    for (row, path, error) in watch.settle(&asset_server) {
        failures.report(Failure::new(row, format!("`{path}`: {error}")));
    }
}

/// Age the reveal by the frame's unpaused time. Frozen while the overlay is
/// open: the clock resumes where it stopped on resume.
pub(crate) fn advance_reveal(
    time: Res<Time>,
    menu: Res<MenuState>,
    runtime: Res<Runtime>,
    settings: Option<Res<SettingsState>>,
    mut reveal: ResMut<TextReveal>,
) {
    if menu.open {
        return;
    }
    let speed = settings::effective_text_speed(settings.as_deref());
    let Some(wait) = runtime.runner.wait() else {
        return;
    };
    reveal.advance(time.delta_secs(), wait, speed);
}
