//! A node's loop: its period, easing and keyframes (spec §6), parsed here and
//! judged by ticket #109.
//!
//! The parser accepts a loop on any node because the format's syntax does; where
//! a loop may appear, how fast it may flash and whether it gates input are the
//! motion pass's rules, not the parser's. A keyframe states what it changes and
//! leaves the rest at the node's neutral value, which is what makes the format's
//! opacity-only example a complete keyframe.

use super::paint;
use super::tokens::Tokens;
use super::value::{self, Fields, Report};

/// How a loop interpolates between its keyframes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseInOut,
}

/// One keyframe. A property it does not name stays at the neutral value:
/// opacity 1, no offset, scale 1 and no rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    pub opacity: f32,
    pub offset: f32,
    pub scale: f32,
    pub rotation_deg: f32,
}

/// A loop that repeats forever from the moment its surface appears.
#[derive(Clone, Debug, PartialEq)]
pub struct Motion {
    pub period_ms: u32,
    pub easing: Easing,
    pub keyframes: Vec<Keyframe>,
}

pub(super) fn parse(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Motion> {
    let mut fields = Fields::new(value, path, report)?;
    let period = value::require(
        fields.take("period_ms"),
        fields.path("period_ms"),
        "period_ms",
        report,
    )?;
    let period = value::whole(
        period,
        &fields.path("period_ms"),
        0.0,
        f64::from(u32::MAX),
        report,
    )? as u32;
    let easing = value::require(
        fields.take("easing"),
        fields.path("easing"),
        "easing",
        report,
    )?;
    let easing = value::variant(
        easing,
        &fields.path("easing"),
        &[
            ("linear", Easing::Linear),
            ("ease-in-out", Easing::EaseInOut),
        ],
        report,
    )?;
    let frames = value::require(
        fields.take("keyframes"),
        fields.path("keyframes"),
        "keyframes",
        report,
    )?;
    let keyframes = keyframes(frames, &fields.path("keyframes"), tokens, report)?;
    if !fields.finish(report, " in a loop") {
        return None;
    }
    Some(Motion {
        period_ms: period,
        easing,
        keyframes,
    })
}

fn keyframes(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Vec<Keyframe>> {
    let items = value::seq(value, path, report)?;
    let mut keyframes = Vec::with_capacity(items.len());
    let mut ok = true;
    for (index, item) in items.into_iter().enumerate() {
        let frame_path = format!("{path}[{index}]");
        match keyframe(item, &frame_path, tokens, report) {
            Some(keyframe) => keyframes.push(keyframe),
            None => ok = false,
        }
    }
    ok.then_some(keyframes)
}

fn keyframe(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Keyframe> {
    let mut fields = Fields::new(value, path, report)?;
    let opacity = match fields.take("opacity") {
        Some(value) => paint::opacity(value, &fields.path("opacity"), report)?,
        None => 1.0,
    };
    let offset = match fields.take("offset") {
        Some(value) => paint::length(value, &fields.path("offset"), tokens, report)?,
        None => 0.0,
    };
    let scale = match fields.take("scale") {
        Some(value) => value::number(value, &fields.path("scale"), report)? as f32,
        None => 1.0,
    };
    let rotation_deg = match fields.take("rotation_deg") {
        Some(value) => value::number(value, &fields.path("rotation_deg"), report)? as f32,
        None => 0.0,
    };
    if !fields.finish(report, " in a keyframe") {
        return None;
    }
    Some(Keyframe {
        opacity,
        offset,
        scale,
        rotation_deg,
    })
}
