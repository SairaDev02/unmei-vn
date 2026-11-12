//! Everything the acceptance walk's steps and checks share, in one place.
//!
//! The plans are read as this gate's specification, so they import this module
//! instead of repeating the same engine, app and harness names in every file.

pub(crate) use std::collections::HashMap;
pub(crate) use std::sync::{Arc, Mutex};

pub(crate) use bevy::input::keyboard::{Key, KeyboardInput};
pub(crate) use bevy::input::mouse::MouseButtonInput;
pub(crate) use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
pub(crate) use bevy::input::touch::TouchPhase;
pub(crate) use bevy::input::ButtonState;
pub(crate) use bevy::prelude::*;
pub(crate) use bevy::ui::{ComputedNode, UiGlobalTransform};
pub(crate) use bevy::window::{PrimaryWindow, WindowFocused, WindowResolution};

pub(crate) use unmei_app::credentials::{CredentialsState, KeyRow};
pub(crate) use unmei_app::failure::StartupFailure;
pub(crate) use unmei_app::menu::{ConfirmKind, MenuAction, MenuFocus, MenuState, MenuView};
pub(crate) use unmei_app::{DefensiveHold, Runtime, RuntimeConfig};
pub(crate) use unmei_engine::{
    DecisionConfig, DecisionError, DecisionResolver, ResolvedDecision, StateValue, Wait,
};

pub(crate) use crate::acts::*;
pub(crate) use crate::anchor::*;
pub(crate) use crate::checks::*;
pub(crate) use crate::fixture;
pub(crate) use crate::harness::*;
pub(crate) use crate::input::*;
pub(crate) use crate::mode::{GatedResolver, Mode};
pub(crate) use crate::plan::plan;
pub(crate) use crate::rows::*;
pub(crate) use crate::settings::*;
pub(crate) use crate::status::*;
pub(crate) use crate::style::*;
pub(crate) use crate::walk::{DraftEdit, Harness, Queued, Step};
