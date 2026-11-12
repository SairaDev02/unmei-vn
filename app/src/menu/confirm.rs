//! Which confirmation a card raises, and what each one does when it is accepted: the
//! one destructive-action gate, branching on the [`ConfirmKind`] the dialog named.

use bevy::prelude::*;
use unmei_engine::save::SlotStatus;

use crate::core::{DecisionChannel, PendingKeyCheck};
use crate::credentials::{self, CredentialsState, KeyRow};
use crate::session::{RestartOutcome, SessionInput};
use crate::settings::SettingsState;
use crate::RuntimeConfig;

use super::cards::{card_action, SlotAction};
use super::persistence::{load_slot, save_slot};
use super::state::{ConfirmKind, MenuFocus, MenuState};
use super::transition::{close, enter_play};

#[allow(clippy::too_many_arguments)]
pub(super) fn accept_confirm(
    menu: &mut MenuState,
    session: &mut SessionInput,
    settings: &mut SettingsState,
    exit: &mut MessageWriter<AppExit>,
    credentials: &mut CredentialsState,
    runtime_config: &mut RuntimeConfig,
    decision_channel: &mut DecisionChannel,
    pending_key_check: &mut PendingKeyCheck,
) {
    let Some(confirm) = menu.confirm.take() else {
        return;
    };
    match confirm {
        ConfirmKind::Overwrite(slot) => save_slot(menu, session, slot),
        ConfirmKind::Load(slot) => load_slot(menu, session, slot),
        ConfirmKind::ResetAudio => {
            settings.reset_audio();
            menu.focus = MenuFocus::AudioReset;
        }
        ConfirmKind::ResetPlay => {
            settings.reset_play();
            menu.focus = MenuFocus::PlayReset;
        }
        ConfirmKind::NewGame => {
            session.transitions().new_game();
            enter_play(menu);
        }
        ConfirmKind::Quit => {
            exit.write(AppExit::Success);
        }
        ConfirmKind::ExitToTitle => menu.enter_title(session.pack(), session.scene_id()),
        ConfirmKind::ClearKey => {
            let removed = match credentials.config_dir.as_deref() {
                Some(config_dir) => credentials::clear(config_dir).is_ok(),
                // No directory means no file to delete; the row is already Not
                // set in every reachable state.
                None => true,
            };
            if removed {
                credentials.row = KeyRow::NotSet;
                // The stored key is gone, so any ping still in flight is
                // cancelled and its verdict (and the key inside it) dropped.
                pending_key_check.task = None;
                // No key is stored any more, so play degrades to the authored
                // fallback exactly as an offline launch does. The play path
                // reads `DecisionChannel`, not `RuntimeConfig`, so both are
                // cleared.
                runtime_config.decisions = None;
                decision_channel.resolver = None;
            } else {
                // The bytes are still on disk, so the row and the resolver must
                // stay as they were; the next Settings open re-reads them.
                log::warn!("unmei: could not delete the credential file");
            }
            menu.focus = MenuFocus::KeyRow;
        }
        ConfirmKind::TimelineJump(index) => {
            // The menu resolves the confirmed index against the pack's declared
            // order; the session recipe neither looks up a label nor derives
            // scene order. A defensive miss is the existing no-op, and only a
            // `Restarted` outcome closes the Timeline.
            let Some(scene_id) = session.pack().timeline_entries().get(index).cloned() else {
                return;
            };
            let outcome = session.transitions().restart_scene(&scene_id);
            if outcome == RestartOutcome::Restarted {
                close(menu);
            }
        }
    }
}

pub(super) fn activate_card(index: usize, menu: &mut MenuState, session: &mut SessionInput) {
    let Some(info) = menu.slots.get(index) else {
        return;
    };
    let action = card_action(info, menu.mode);
    match (action.action, &info.status) {
        // Save-over-empty is immediate: no second prompt.
        (Some(SlotAction::Write), SlotStatus::Empty) => {
            save_slot(menu, session, info.slot);
        }
        (Some(SlotAction::Write), _) => {
            menu.confirm = Some(ConfirmKind::Overwrite(info.slot));
            menu.focus = MenuFocus::ConfirmYes;
        }
        (Some(SlotAction::Load), _) => {
            menu.confirm = Some(ConfirmKind::Load(info.slot));
            menu.focus = MenuFocus::ConfirmYes;
        }
        (None, _) => {}
    }
}
