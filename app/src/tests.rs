use super::*;
use crate::credentials::{self, ClipboardPort, CopyPathLabel, CredentialsState, KeyField, KeyRow};
use crate::failure::{captures, LogOpenError, Notice, StartupFailure};
use crate::input::{InputClaim, InputOwnership};
use crate::menu::{
    self, ConfirmKind, LoadSurface, MenuAction, MenuFocus, MenuMode, MenuState, MenuView,
    SlotAction,
};
use crate::presentation::assets::AssetWatch;
use crate::presentation::play::{
    report_silent_degradations, sync_audio, sync_staging_visuals, AppliedAudio, AppliedUi,
    AppliedVisuals, Bgm, DialogueText, HelpLine, Sfx, TextReveal,
};
use crate::presentation::settings_view::{SettingsLabel, SettingsLabelKind};
use crate::session::{LoadOrigin, LoadOutcome, ReadOutcome, SessionInput};
use crate::settings::{self, AudioControl, SettingsState};
use bevy::app::AppExit;
use bevy::asset::{
    io::Reader, AssetApp, AssetLoader, AssetPlugin, LoadContext, LoadState, RenderAssetUsages,
};
use bevy::ecs::system::RunSystemOnce;
use bevy::ecs::world::FromWorld;
use bevy::reflect::TypePath;
use bevy::render::render_resource::Extent3d;
use bevy::text::{EditableText, LineBreak, LineHeight, TextLayout};
use bevy::ui::widget::NodeImageMode;
use bevy::ui_widgets::ScrollArea;
use bevy::window::{MonitorSelection, PrimaryWindow, Window, WindowMode};
use bevy::MinimalPlugins;
use bevy_kira_audio::AudioChannel;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use unmei_engine::decision::{DecisionError, DecisionResolver, ResolvedDecision};
use unmei_engine::runner::Wait;
use unmei_engine::save::{self, SaveFile, Slot, SlotStatus, StagingState};
use unmei_engine::{load_pack, DecisionConfig, KeyCheck, KeyVerdict, ScriptRunner, StateValue};

use crate::drawer::{
    self, MonitorControl, MonitorRow, MonitorRowPart, MonitorStatusText, MonitorView, MonitorWork,
    DISPLAY_ROWS, MONITOR_CLAIM, NEWEST_INSET, NEWEST_WASH, PROJECTION_SCAN_BUDGET,
    TEXT_WORK_BUDGET,
};
use crate::input::InputQuarantine;
use crate::monitor::{MonitorHistory, MonitorLine};

use support::*;
pub(crate) use support::{
    activate_control, app_for_pack, boot_widget_input_app, drawer_open, drawer_view,
    enter_play_with_input, press_and_update, press_key, step_input, tap_key, temp_pack,
};

mod asset_root;
mod comfort;
mod decisions;
mod failure_posture;
mod gallery;
mod input_ownership;
mod key_row;
mod monitor_drawer;
mod monitor_reading;
mod overlay;
mod play_path;
mod reading;
mod render;
mod save_slots;
mod settings_file;
mod support;
mod timeline;
mod title;
mod typography;
