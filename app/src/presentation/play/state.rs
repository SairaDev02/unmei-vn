//! The play surface's node markers, its two audio channels and the applied
//! caches that make each sync write only on change.

use bevy::prelude::*;
use std::collections::BTreeMap;
use unmei_engine::runner::Wait;

#[derive(Component)]
pub(crate) struct NoticeText;

#[derive(Component)]
pub(crate) struct ErrorScreen;

#[derive(Component)]
pub(crate) struct BackgroundSprite;

#[derive(Component)]
pub(crate) struct NpcSprite {
    #[allow(
        dead_code,
        reason = "the sprite's staged NPC id; readers match the component by entity"
    )]
    pub(super) npc_id: String,
}

#[derive(Component)]
pub(crate) struct ChoiceButton {
    pub(crate) index: usize,
}

// Audio channels: BGM and one-shot SFX stay independent so a choice sound never
// cuts the music.
#[derive(Resource)]
pub(crate) struct Bgm;

#[derive(Resource)]
pub(crate) struct Sfx;

/// What the audio layer last applied, so `play` is issued on change only. The
/// fields are private to this module ([ADR 0013](../../../../docs/adr/0013-presentation-surface-partition.md)):
/// callers request a replay through [`request_replay`] and never touch the
/// cache.
#[derive(Resource)]
pub(crate) struct AppliedAudio {
    pub(super) bgm: Option<String>,
    /// Force a stop+play even when the id is unchanged (after a load).
    pub(super) force: bool,
}

impl FromWorld for AppliedAudio {
    fn from_world(_: &mut World) -> Self {
        AppliedAudio {
            bgm: None,
            force: false,
        }
    }
}

/// The channel volumes last pushed to kira, in decibels, so `set_volume` is
/// called on change only. The default `0.0` is kira's identity, matching a
/// first launch at full volume.
#[derive(Resource)]
pub(crate) struct AppliedVolume {
    pub(super) bgm: f32,
    pub(super) sfx: f32,
}

impl FromWorld for AppliedVolume {
    fn from_world(_: &mut World) -> Self {
        AppliedVolume { bgm: 0.0, sfx: 0.0 }
    }
}

/// What sprite entities currently exist, so they are rebuilt only on change.
#[derive(Resource)]
pub(crate) struct AppliedVisuals {
    pub(super) bg: Option<String>,
    pub(super) sprites: BTreeMap<String, String>,
    pub(super) force: bool,
}

impl FromWorld for AppliedVisuals {
    fn from_world(_: &mut World) -> Self {
        AppliedVisuals {
            bg: None,
            sprites: BTreeMap::new(),
            force: false,
        }
    }
}

/// What the dialogue/choice UI last showed, so it is rebuilt only on change.
#[derive(Resource)]
pub(crate) struct AppliedUi {
    pub(super) wait: Option<Wait>,
    pub(super) ready: bool,
    /// The say-line the player last read, kept so a choice-wait can show the
    /// question its options answer (Default look §2). A load clears it with the
    /// rest of the replay caches, so a save restored onto a choice-wait shows an
    /// empty box.
    pub(super) say: Option<(String, String)>,
}

impl FromWorld for AppliedUi {
    fn from_world(_: &mut World) -> Self {
        AppliedUi {
            wait: None,
            ready: false,
            say: None,
        }
    }
}

// The app's integration tests read these caches to assert the replay and
// title-silence behavior; their fields are written only by this module's own
// syncs and [`request_replay`].
#[cfg(test)]
impl AppliedAudio {
    pub(crate) fn current_bgm(&self) -> Option<&str> {
        self.bgm.as_deref()
    }

    pub(crate) fn is_forced(&self) -> bool {
        self.force
    }
}

#[cfg(test)]
impl AppliedVisuals {
    pub(crate) fn is_forced(&self) -> bool {
        self.force
    }
}

#[cfg(test)]
impl AppliedUi {
    pub(crate) fn is_ready(&self) -> bool {
        self.ready
    }
}

/// One replay request: force audio, visuals and the dialogue/choice UI to
/// replay from the top exactly as a load does. Shared by the load path, New
/// Game's reset and the Timeline restart, so the three call sites cannot
/// drift; the caller never sets a cache field itself. The reveal reset stays
/// behind [`TextReveal`]'s own methods.
pub(crate) fn request_replay(
    audio: &mut AppliedAudio,
    visuals: &mut AppliedVisuals,
    ui: &mut AppliedUi,
) {
    *audio = AppliedAudio {
        bgm: None,
        force: true,
    };
    *visuals = AppliedVisuals {
        bg: None,
        sprites: BTreeMap::new(),
        force: true,
    };
    *ui = AppliedUi {
        wait: None,
        ready: false,
        say: None,
    };
}
