//! Held-input quarantine: keys and buttons held across an ownership transition
//! stay inert until they are physically released.

use std::collections::HashSet;

use bevy::input::ButtonInput;
use bevy::prelude::*;

use super::ownership::{input_claimed, input_unclaimed};

/// Input held across a claim's lifetime (or its transition frames), quarantined
/// until it is physically released.
///
/// A claim already stops the app's ordinary action systems while it is held, but
/// two production paths keep running and can replay the held input after the
/// claim drops:
///
/// - Bevy's focused-widget dispatch and editable-text observers read raw
///   `KeyboardInput` messages rather than `just_pressed`, so an OS repeat from a
///   `Space`/`Enter` held across the transition would still type into a resumed
///   key field.
/// - `Interaction` is a sticky component, so a pointer press held across the
///   transition is still `Pressed` when an overlay system that was frozen by the
///   claim runs again, which would start a Settings drag or pick a choice.
///
/// This resource closes both paths without freezing physical input tracking:
/// every key or button down while ordinary input is not live is remembered, and
/// [`app_input_ready`] stays false until each of those is observed released.
/// Releasing (not resetting) the physical state is deliberate — the force-hold
/// release mirror must still see a held `Ctrl` go up, and the already-armed
/// pre-Monitor hold keeps running while it stays down.
#[derive(Resource, Debug, Default)]
pub(crate) struct InputQuarantine {
    keys: HashSet<KeyCode>,
    buttons: HashSet<MouseButton>,
}

impl InputQuarantine {
    /// Whether nothing is being quarantined, i.e. ordinary input may resume.
    pub(crate) fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.buttons.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn holds_key(&self, key: KeyCode) -> bool {
        self.keys.contains(&key)
    }

    #[cfg(test)]
    pub(crate) fn holds_button(&self, button: MouseButton) -> bool {
        self.buttons.contains(&button)
    }

    /// Whether any remembered key or button is still physically down. The
    /// resource set is the frame's snapshot; this reads the live physical state
    /// so a caller that runs before [`track_quarantine`]'s drain (the drawer's
    /// closed-frame restore) still sees the release frame correctly.
    pub(crate) fn has_live_hold(
        &self,
        keys: &ButtonInput<KeyCode>,
        mouse: &ButtonInput<MouseButton>,
    ) -> bool {
        self.keys.iter().any(|key| keys.pressed(*key))
            || self.buttons.iter().any(|button| mouse.pressed(*button))
    }

    /// Forget every quarantined input. Called on window focus loss, where the
    /// platform may never deliver the releases of keys/buttons it already
    /// considers gone.
    pub(crate) fn clear(&mut self) {
        self.keys.clear();
        self.buttons.clear();
    }
}

/// Record every key/button that is down while ordinary app input is not live
/// (a claim is held, or the quarantine tail is still draining), and drop each
/// entry once it is observed released.
///
/// The tail case is why the test is `!app_input_ready` rather than just
/// `input_claimed`: a button first pressed after the claim dropped but before
/// the tail cleared is still held across the transition and must be quarantined
/// too, or its sticky `Interaction` could activate on release.
pub(crate) fn track_quarantine(
    ownership: Option<Res<super::InputOwnership>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    mut quarantine: ResMut<InputQuarantine>,
) {
    let claimed = input_claimed(ownership.as_deref());
    let draining = !quarantine.is_empty();
    if claimed || draining {
        if let Some(keys) = keys.as_deref() {
            for key in keys.get_pressed() {
                quarantine.keys.insert(*key);
            }
        }
        if let Some(mouse) = mouse.as_deref() {
            for button in mouse.get_pressed() {
                quarantine.buttons.insert(*button);
            }
        }
    }
    // Drop the entries whose physical key/button has come up. A re-press after
    // that is a fresh press and may act once the set is empty.
    if let Some(keys) = keys.as_deref() {
        quarantine.keys.retain(|key| keys.pressed(*key));
    }
    if let Some(mouse) = mouse.as_deref() {
        quarantine.buttons.retain(|button| mouse.pressed(*button));
    }
}

/// The run condition every production app-input path actually uses: input is
/// live only while no claimant owns the frame **and** the held-input quarantine
/// has drained. A world without the resource cannot have a quarantined key, so
/// a missing [`InputQuarantine`] answers `true` — the same answer a world with
/// an empty set gives.
pub(crate) fn app_input_ready(
    ownership: Option<Res<super::InputOwnership>>,
    quarantine: Option<Res<InputQuarantine>>,
) -> bool {
    if !input_unclaimed(ownership) {
        return false;
    }
    !quarantine.is_some_and(|quarantine| !quarantine.is_empty())
}
