//! The single app-input ownership boundary: one claim, one ordering point.

use bevy::prelude::*;

/// The single app-input ownership boundary.
///
/// The ordinary app surfaces own input unless a topmost claimant holds it. A
/// claim is deliberately generic — a temporary overlay, the Monitor or any
/// future surface that must take every app input path — and shipping installs
/// none, so the default is unchanged. Everything the app *does* with input
/// (play/menu/shortcut dispatch, focused-widget dispatch, propagated pointer and
/// focus observers, queued text/paste application, startup-error quit and
/// Settings drag) reads [`input_unclaimed`], so one claim suppresses them all
/// without a per-surface rewrite.
///
/// Physical input **tracking** deliberately does not read this resource:
/// [`crate::core::force_release_mirror`] must mirror a released `Ctrl` even while
/// another claimant owns the frame, so an already-armed force hold can keep
/// running and its release still clears the latch (`docs/runtime-hardening.md`
/// § Text skip §5). A claimant is expected to set or clear the claim in
/// `PreUpdate` at [`InputOwnershipSet`], or in whatever schedule runs before the
/// app input systems it means to suppress; the run conditions read the live
/// value, so a claim taken early in a frame covers the rest of that frame.
///
/// This seam covers the app's own paths and the shared text-apply set. The
/// engine-internal input producers (`bevy::input_focus` dispatch, the
/// `EditableText` keyboard/pointer/focus observers, `Interaction`/picking
/// mutation and IME) are not reachable by a `run_if`; they still run while a
/// claimant holds the frame, so the app gates `EditableTextSystems` and drains
/// the edits those observers queue
/// ([`crate::presentation::settings_view::discard_queued_key_edits_when_blocked`])
/// so no text
/// mutation applies. The focus tree, `Interaction` and IME state stay the later
/// Monitor drawer ticket's to isolate.
#[derive(Resource, Debug, Default)]
pub struct InputOwnership {
    claim: Option<InputClaim>,
}

impl InputOwnership {
    /// Take app input for `claim`. The claim is live immediately, so a claimant
    /// that runs before the app's input systems suppresses them on this frame too.
    ///
    /// This is the seam's test entry point; shipping claimants use
    /// [`Self::claim_if_free`] / [`Self::release_claim`] so a second claimant
    /// cannot silently displace the first.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn claim(&mut self, claim: InputClaim) {
        self.claim = Some(claim);
    }

    /// Drop the claim, returning app input to the ordinary surfaces.
    /// The test counterpart of [`Self::claim`].
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn release(&mut self) {
        self.claim = None;
    }

    /// The current claimant, if any.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn claimed_by(&self) -> Option<InputClaim> {
        self.claim
    }

    /// Take app input for `claim` unless a **different** claimant already owns
    /// it. Re-claiming as the same claimant is idempotent, so a surface that
    /// owns input for several frames (the Monitor drawer) can assert its claim
    /// every frame without fighting itself. Returns whether `claim` owns input
    /// after the call.
    pub fn claim_if_free(&mut self, claim: InputClaim) -> bool {
        if self.claim.is_none() || self.claim == Some(claim) {
            self.claim = Some(claim);
            true
        } else {
            false
        }
    }

    /// Release the claim only if `claim` owns it, so one surface can never drop
    /// another's protection. Returns whether the claim was released.
    pub fn release_claim(&mut self, claim: InputClaim) -> bool {
        if self.claim == Some(claim) {
            self.claim = None;
            true
        } else {
            false
        }
    }
}

/// An opaque, named claim on app input. The label is diagnostic only; the
/// boundary does not branch on it. It exists so a claimant can be identified in
/// tests and, later, in status surfaces without the app learning its semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputClaim(pub &'static str);

/// Whether app input is unclaimed, ignoring the held-input quarantine.
///
/// [`crate::input::app_input_ready`] composes this with the quarantine; it stays public
/// so a future claimant can ask only whether another surface owns the frame.
/// Tolerant of a world that never added [`InputOwnership`] (the headless core
/// tests), which can only mean no claim.
pub(crate) fn input_unclaimed(ownership: Option<Res<InputOwnership>>) -> bool {
    !input_claimed(ownership.as_deref())
}

/// Whether a claimant owns app input in this world. A world without the resource
/// cannot have a claimant, so a missing resource means unclaimed — the same
/// answer [`input_unclaimed`] gives.
pub(crate) fn input_claimed(ownership: Option<&InputOwnership>) -> bool {
    ownership.is_some_and(|ownership| ownership.claim.is_some())
}

/// The resolved-ownership point in `PreUpdate`.
///
/// It sits before physical `bevy::input::InputSystems` and focused input
/// dispatch, and so before every `Update` app action and `PostUpdate` widget
/// application in the same frame. A claimant (the later Monitor drawer)
/// schedules its decision to claim or release here so an opening/closing frame
/// is covered whole. Shipping schedules nothing in it; it is the seam's ordering
/// anchor.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct InputOwnershipSet;
