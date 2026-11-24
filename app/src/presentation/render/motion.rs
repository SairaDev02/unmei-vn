//! A node's loop: an ornament that repeats forever from the moment its surface
//! appears, on real time, so it keeps running under the presentation pause
//! (spec §6).
//!
//! The engine decides where a loop may sit and how fast it may flash; the
//! renderer plays the keyframed opacity. A keyframe states what it changes and
//! leaves the rest neutral, the keyframes are evenly spaced, and the last one
//! closes the ring back to the first.

use bevy::prelude::*;
use bevy::ui::BoxShadow;

use unmei_engine::ui::{Easing, Motion};

/// One node's loop and the paint it scales every frame: the node's own tone and
/// fills, kept so each frame dims the same colours rather than compounding the
/// last frame's.
#[derive(Component, Clone, Debug)]
pub(crate) struct UiLoop {
    motion: Motion,
    elapsed_ms: f32,
    text: Option<Color>,
    background: Option<Color>,
    border: Option<Color>,
    shadows: Vec<Color>,
}

impl UiLoop {
    /// The loop `motion` playing on a node whose paint is these colours.
    pub(super) fn new(
        motion: &Motion,
        text: Option<Color>,
        background: Option<Color>,
        border: Option<Color>,
        shadows: Vec<Color>,
    ) -> Self {
        Self {
            motion: motion.clone(),
            elapsed_ms: 0.0,
            text,
            background,
            border,
            shadows,
        }
    }

    /// The opacity at `progress` through the period, in 0..1. Keyframes are
    /// evenly spaced and the ring closes from the last back to the first.
    fn opacity(&self, progress: f32) -> f32 {
        let frames = &self.motion.keyframes;
        let count = frames.len();
        match count {
            0 => 1.0,
            1 => frames[0].opacity,
            _ => {
                let scaled = progress * count as f32;
                let index = (scaled.floor() as usize).min(count - 1);
                let from = frames[index].opacity;
                let to = frames[(index + 1) % count].opacity;
                from + (to - from) * eased(scaled - index as f32, self.motion.easing)
            }
        }
    }
}

/// The easing curve at `t` in 0..1: `linear` keeps the pace, `ease-in-out`
/// starts and ends slowly.
fn eased(t: f32, easing: Easing) -> f32 {
    match easing {
        Easing::Linear => t,
        Easing::EaseInOut => t * t * (3.0 - 2.0 * t),
    }
}

/// Play every loop: age it by the frame's real time and write the opacity it is
/// on, from the colours it started with.
#[allow(
    clippy::type_complexity,
    reason = "the four components a loop dims are read as they come, each optional"
)]
pub(crate) fn sync_loops(
    time: Res<Time>,
    mut loops: Query<(
        &mut UiLoop,
        Option<&mut TextColor>,
        Option<&mut BackgroundColor>,
        Option<&mut BorderColor>,
        Option<&mut BoxShadow>,
    )>,
) {
    for (mut motion, text, background, border, shadow) in &mut loops {
        motion.elapsed_ms += time.delta_secs() * 1000.0;
        let period = motion.motion.period_ms.max(1) as f32;
        let opacity = motion.opacity((motion.elapsed_ms % period) / period);
        if let (Some(mut tone), Some(base)) = (text, motion.text) {
            tone.0 = base.with_alpha(base.alpha() * opacity);
        }
        if let (Some(mut fill), Some(base)) = (background, motion.background) {
            fill.0 = base.with_alpha(base.alpha() * opacity);
        }
        if let (Some(mut edge), Some(base)) = (border, motion.border) {
            *edge = BorderColor::all(base.with_alpha(base.alpha() * opacity));
        }
        if let Some(mut drawn) = shadow {
            for (index, colour) in motion.shadows.iter().enumerate() {
                if let Some(style) = drawn.0.get_mut(index) {
                    style.color = colour.with_alpha(colour.alpha() * opacity);
                }
            }
        }
    }
}
