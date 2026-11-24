//! The confirm surface: one tree for every kind, drawn from the plugin in force
//! (spec §11).
//!
//! The kind decides the words and nothing else. Its question fills
//! `confirm.prompt`, its consequence fills `confirm.detail`, and the accepting
//! control's label fills `confirm.accept-label` — all through the copy table, so a
//! plugin's overrides show with their placeholders filled (spec §5). The words
//! the engine draws inside a control are the same keys, which is why
//! [`Bind::words`] answers `confirm.yes`/`confirm.no` as well as the slots.
//!
//! Focus, the walk and both actions stay the menu's: [`Bind::action`] names the
//! two Keiko controls and the coordinator repaints their state.

use bevy::prelude::*;

use super::super::render::{Bind, Surface, UiLook, Words};
use super::super::style::TextFaces;
use crate::credentials::CredentialsState;
use crate::menu::{self, MenuAction};
use crate::Runtime;

/// The confirm's binding: the kind's copy keys, its two controls and no state a
/// confirm can be in beyond the focus the repaint reads.
pub(super) struct Confirm<'a> {
    kind: menu::ConfirmKind,
    credentials: Option<&'a CredentialsState>,
    runtime: &'a Runtime,
}

impl Bind for Confirm<'_> {
    fn words(&self, name: &str) -> Option<Words> {
        match name {
            "confirm.prompt" => Some(self.question()),
            "confirm.detail" => Some(self.detail()),
            "confirm.accept-label" | "confirm.yes" => Some(Words::new(accept(self.kind))),
            "confirm.no" => Some(Words::new("confirm.button.cancel")),
            _ => None,
        }
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        match name {
            "confirm.yes" => Some(MenuAction::ConfirmAccept),
            "confirm.no" => Some(MenuAction::ConfirmCancel),
            _ => None,
        }
    }
}

impl Confirm<'_> {
    /// What the confirm asks: the kind's question line, with the scene a
    /// Timeline jump names.
    fn question(&self) -> Words {
        match self.kind {
            menu::ConfirmKind::TimelineJump(index) => Words::with(
                "confirm.timeline-jump.title",
                &[("scene", &self.scene(index))],
            ),
            kind => Words::new(title(kind)),
        }
    }

    /// The consequence the confirm states, with the slot or path it names.
    fn detail(&self) -> Words {
        match self.kind {
            menu::ConfirmKind::Overwrite(slot) | menu::ConfirmKind::Load(slot) => {
                Words::with(prompt(self.kind), &[("slot", menu::slot_label(slot))])
            }
            menu::ConfirmKind::ClearKey => {
                Words::with("confirm.clear-key.prompt", &[("path", &self.key_path())])
            }
            menu::ConfirmKind::TimelineJump(_) => Words::new("confirm.timeline-jump.detail"),
            kind => Words::new(prompt(kind)),
        }
    }

    /// The path the stored key lives in, or the em dash when there is none: the
    /// prompt still says where the key would be deleted from.
    fn key_path(&self) -> String {
        self.credentials
            .and_then(|state| state.path.as_deref())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| unmei_engine::copy::text("confirm.clear-key.none").to_string())
    }

    /// The scene a Timeline jump would land on.
    fn scene(&self, index: usize) -> String {
        self.runtime
            .runner
            .pack()
            .timeline_entries()
            .get(index)
            .cloned()
            .unwrap_or_default()
    }
}

/// The copy key of a kind's question line.
fn title(kind: menu::ConfirmKind) -> &'static str {
    match kind {
        menu::ConfirmKind::Overwrite(_) => "confirm.overwrite.title",
        menu::ConfirmKind::Load(_) => "confirm.load.title",
        menu::ConfirmKind::ResetAudio => "confirm.reset-audio.title",
        menu::ConfirmKind::ResetPlay => "confirm.reset-play.title",
        menu::ConfirmKind::NewGame => "confirm.new-game.title",
        menu::ConfirmKind::Quit => "confirm.quit.title",
        menu::ConfirmKind::ExitToTitle => "confirm.exit-to-title.title",
        menu::ConfirmKind::ClearKey => "confirm.clear-key.title",
        menu::ConfirmKind::TimelineJump(_) => "confirm.timeline-jump.title",
    }
}

/// The copy key of a kind's consequence line.
fn prompt(kind: menu::ConfirmKind) -> &'static str {
    match kind {
        menu::ConfirmKind::Overwrite(_) => "confirm.overwrite.prompt",
        menu::ConfirmKind::Load(_) => "confirm.load.prompt",
        menu::ConfirmKind::ResetAudio => "confirm.reset-audio.prompt",
        menu::ConfirmKind::ResetPlay => "confirm.reset-play.prompt",
        menu::ConfirmKind::NewGame => "confirm.new-game.prompt",
        menu::ConfirmKind::Quit => "confirm.quit.prompt",
        menu::ConfirmKind::ExitToTitle => "confirm.exit-to-title.prompt",
        menu::ConfirmKind::ClearKey => "confirm.clear-key.prompt",
        menu::ConfirmKind::TimelineJump(_) => "confirm.timeline-jump.detail",
    }
}

/// The copy key of the accepting control's label.
fn accept(kind: menu::ConfirmKind) -> &'static str {
    match kind {
        menu::ConfirmKind::NewGame => "confirm.button.start",
        menu::ConfirmKind::Quit => "confirm.button.quit",
        menu::ConfirmKind::ExitToTitle => "confirm.button.exit",
        menu::ConfirmKind::TimelineJump(_) => "confirm.button.jump",
        _ => "confirm.button.yes",
    }
}

/// Draw the open confirm as the plugin in force declares it, under `parent`.
pub(in crate::presentation) fn confirm_dialog(
    parent: &mut ChildSpawnerCommands,
    look: Option<&UiLook>,
    faces: &TextFaces,
    confirm: menu::ConfirmKind,
    credentials: Option<&CredentialsState>,
    runtime: &Runtime,
) {
    let Some(look) = look else {
        return;
    };
    let bind = Confirm {
        kind: confirm,
        credentials,
        runtime,
    };
    let surface = Surface {
        look,
        faces,
        bind: &bind,
    };
    surface.spawn("confirm", parent);
}
