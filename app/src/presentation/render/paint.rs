//! The paint a resolved style draws with, as the Bevy UI components that carry
//! it: a fill, a border, a corner radius, shadows, an opacity and a transform
//! (spec §4).
//!
//! One conversion per property, so a state override repaints through the same
//! functions its base paint was built with.

use bevy::prelude::*;
use bevy::ui::{
    BackgroundGradient, BorderRadius, BoxShadow, ColorStop, Gradient, LinearGradient,
    RadialGradient, ShadowStyle, UiTransform, Val2,
};

use unmei_engine::ui::{Border, Fill, GradientStop, Paint, Radius, Shadow, Transform};

/// A token colour as a Bevy colour: one sRGB value with the opacity applied.
pub(super) fn color(color: unmei_engine::ui::Color, opacity: f32) -> Color {
    Color::srgba(color.r, color.g, color.b, color.a * opacity)
}

/// The background a fill draws: a solid colour, or the gradient that stands in
/// its place.
pub(super) fn background(
    fill: &Fill,
    opacity: f32,
) -> (BackgroundColor, Option<BackgroundGradient>) {
    match fill {
        Fill::Color(value) => (BackgroundColor(color(*value, opacity)), None),
        Fill::Linear { angle_deg, stops } => (
            BackgroundColor(Color::NONE),
            Some(BackgroundGradient(vec![Gradient::Linear(LinearGradient {
                // The format's angle runs left to right at 0 and top to bottom
                // at 90; Bevy's runs bottom to top at 0, clockwise.
                angle: std::f32::consts::TAU * (angle_deg + 90.0) / 360.0,
                stops: stops_of(stops, opacity),
                ..default()
            })])),
        ),
        Fill::Radial { stops } => (
            BackgroundColor(Color::NONE),
            Some(BackgroundGradient(vec![Gradient::Radial(RadialGradient {
                stops: stops_of(stops, opacity),
                ..default()
            })])),
        ),
    }
}

fn stops_of(stops: &[GradientStop], opacity: f32) -> Vec<ColorStop> {
    stops
        .iter()
        .map(|stop| ColorStop::new(color(stop.color, opacity), Val::Percent(stop.at * 100.0)))
        .collect()
}

/// A border's colour and width, and the radius that rounds both the fill and the
/// border.
pub(super) fn border(edge: Border, opacity: f32) -> (BorderColor, UiRect) {
    (
        BorderColor::all(color(edge.color, opacity)),
        UiRect::all(Val::Px(edge.width)),
    )
}

/// The corner radius, as the same rounding on every corner or one per corner.
pub(super) fn radius(radius: Radius) -> BorderRadius {
    match radius {
        Radius::Uniform(value) => BorderRadius::all(Val::Px(value)),
        Radius::Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } => BorderRadius {
            top_left: Val::Px(top_left),
            top_right: Val::Px(top_right),
            bottom_right: Val::Px(bottom_right),
            bottom_left: Val::Px(bottom_left),
        },
    }
}

/// The node's shadows, in the order the tree lists them.
pub(super) fn shadows(shadows: &[Shadow], opacity: f32) -> BoxShadow {
    BoxShadow(
        shadows
            .iter()
            .map(|shadow| ShadowStyle {
                color: color(shadow.color, opacity),
                x_offset: Val::Px(shadow.x),
                y_offset: Val::Px(shadow.y),
                spread_radius: Val::Px(shadow.spread),
                blur_radius: Val::Px(shadow.blur),
            })
            .collect(),
    )
}

/// The node's offset, scale and rotation.
pub(super) fn transform(transform: Transform) -> UiTransform {
    UiTransform {
        translation: Val2::px(transform.x, transform.y),
        scale: Vec2::splat(transform.scale),
        rotation: Rot2::radians(transform.rotation_deg.to_radians()),
    }
}

/// Every paint component of a style, in one place so a state override and the
/// base paint cannot drift.
pub(super) fn paint(paint: &Paint) -> PaintBundle {
    let mut bundle = PaintBundle::default();
    if let Some(fill) = &paint.fill {
        let (background, gradient) = background(fill, paint.opacity);
        bundle.background = Some(background);
        bundle.gradient = gradient;
    }
    if let Some(edge) = paint.border {
        let (border_color, width) = border(edge, paint.opacity);
        bundle.border_color = Some(border_color);
        bundle.border_width = Some(width);
    }
    if let Some(corners) = paint.radius {
        bundle.radius = Some(radius(corners));
    }
    if !paint.shadows.is_empty() {
        bundle.shadow = Some(shadows(&paint.shadows, paint.opacity));
    }
    if paint.transform != Transform::default() {
        bundle.transform = Some(transform(paint.transform));
    }
    bundle
}

/// The paint components one style sets, each present only when the style does.
#[derive(Default)]
pub(super) struct PaintBundle {
    pub background: Option<BackgroundColor>,
    pub gradient: Option<BackgroundGradient>,
    pub border_color: Option<BorderColor>,
    pub border_width: Option<UiRect>,
    pub radius: Option<BorderRadius>,
    pub shadow: Option<BoxShadow>,
    pub transform: Option<UiTransform>,
}
