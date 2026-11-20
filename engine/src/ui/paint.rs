//! The paint a node draws: fill, border, corner radius, shadows, opacity and
//! transform, and the state overrides a control applies over them (spec §4,
//! "Paint").
//!
//! A paint property is read in one place whether it belongs to a node, a
//! component or a control state, so a token reference resolves and an unknown
//! property is reported identically in all three. The types are the resolved
//! values the renderer reads: no `{group.name}` text survives parsing, and a
//! state override keeps every property optional because it changes only what it
//! names.

use super::tokens::{self, Color, Tokens};
use super::value::{self, Fields, Report};

/// What fills a node: a solid colour, or a linear or radial gradient of colour
/// stops.
#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    Color(Color),
    Linear {
        angle_deg: f32,
        stops: Vec<GradientStop>,
    },
    Radial {
        stops: Vec<GradientStop>,
    },
}

/// One colour stop of a gradient, at a position from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub at: f32,
    pub color: Color,
}

/// A border's width and colour. The corner radius is [`Paint::radius`]'s,
/// because one radius rounds the node and there is no second corner shape for a
/// border to have.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}

/// A corner radius: the same on every corner, or one per corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Radius {
    Uniform(f32),
    Corners {
        top_left: f32,
        top_right: f32,
        bottom_right: f32,
        bottom_left: f32,
    },
}

/// One shadow. A glow is a shadow whose offset is zero (spec §4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

/// A node's offset, scale and rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rotation_deg: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rotation_deg: 0.0,
        }
    }
}

/// The paint a node draws with, with the format's defaults already applied.
#[derive(Clone, Debug, PartialEq)]
pub struct Paint {
    pub fill: Option<Fill>,
    pub border: Option<Border>,
    /// The corner radius, which rounds the node and not only its border: a
    /// filled box that draws no border rounds the same way.
    pub radius: Option<Radius>,
    pub shadows: Vec<Shadow>,
    pub opacity: f32,
    pub transform: Transform,
}

impl Default for Paint {
    fn default() -> Self {
        Paint {
            fill: None,
            border: None,
            radius: None,
            shadows: Vec::new(),
            opacity: 1.0,
            transform: Transform::default(),
        }
    }
}

/// The paint a control's state changes. A property left out keeps the value
/// the node already has.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaintOverrides {
    pub fill: Option<Fill>,
    pub border: Option<Border>,
    pub radius: Option<Radius>,
    pub shadows: Option<Vec<Shadow>>,
    pub opacity: Option<f32>,
    pub transform: Option<Transform>,
}

/// Read every paint property the map holds, leaving the rest for its caller.
pub(super) fn parse(
    fields: &mut Fields,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<PaintOverrides> {
    let mut paint = PaintOverrides::default();
    let mut ok = true;

    if let Some(value) = fields.take("fill") {
        let property = fields.path("fill");
        match fill(value, &property, tokens, report) {
            Some(value) => paint.fill = Some(value),
            None => ok = false,
        }
    }

    if let Some(value) = fields.take("border") {
        let property = fields.path("border");
        match border(value, &property, tokens, report) {
            Some(value) => paint.border = Some(value),
            None => ok = false,
        }
    }

    if let Some(value) = fields.take("radius") {
        let property = fields.path("radius");
        match radius(value, &property, tokens, report) {
            Some(value) => paint.radius = Some(value),
            None => ok = false,
        }
    }

    if let Some(value) = fields.take("shadow") {
        let property = fields.path("shadow");
        match shadows(value, &property, tokens, report) {
            Some(value) => paint.shadows = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("opacity") {
        let property = fields.path("opacity");
        match opacity(value, &property, report) {
            Some(value) => paint.opacity = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("transform") {
        let property = fields.path("transform");
        match transform(value, &property, tokens, report) {
            Some(value) => paint.transform = Some(value),
            None => ok = false,
        }
    }

    ok.then_some(paint)
}

/// A fill: a literal colour, a `{colors.name}` reference, or a `linear` or
/// `radial` gradient map.
pub(super) fn fill(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Fill> {
    match value {
        ron::Value::String(text) => {
            tokens::color(ron::Value::String(text), path, tokens, report).map(Fill::Color)
        }
        ron::Value::Map(_) => {
            let mut fields = Fields::new(value, path, report)?;
            let fill = if let Some(value) = fields.take("linear") {
                let property = fields.path("linear");
                linear(value, &property, tokens, report)
                    .map(|(angle_deg, stops)| Fill::Linear { angle_deg, stops })
            } else if let Some(value) = fields.take("radial") {
                let property = fields.path("radial");
                radial(value, &property, tokens, report).map(|stops| Fill::Radial { stops })
            } else if fields.has("r") {
                tokens::color_fields(&mut fields, report).map(Fill::Color)
            } else {
                report.claim(
                    path,
                    "a fill is a colour, a `{colors.name}` reference, or a `linear`/`radial` gradient",
                );
                return None;
            };
            let fill = fill?;
            if !fields.finish(report, " in a fill") {
                return None;
            }
            Some(fill)
        }
        other => {
            report.claim(
                path,
                format!(
                    "expected a colour or a gradient, found {}",
                    value::kind_name(&other)
                ),
            );
            None
        }
    }
}

fn linear(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<(f32, Vec<GradientStop>)> {
    let mut fields = Fields::new(value, path, report)?;
    let angle = value::require(
        fields.take("angle_deg"),
        fields.path("angle_deg"),
        "angle_deg",
        report,
    )?;
    let angle = value::number(angle, &fields.path("angle_deg"), report)? as f32;
    let items = value::require(fields.take("stops"), fields.path("stops"), "stops", report)?;
    let stops = stops(items, &fields.path("stops"), tokens, report)?;
    if !fields.finish(report, " in a linear gradient") {
        return None;
    }
    Some((angle, stops))
}

fn radial(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Vec<GradientStop>> {
    let mut fields = Fields::new(value, path, report)?;
    let items = value::require(fields.take("stops"), fields.path("stops"), "stops", report)?;
    let stops = stops(items, &fields.path("stops"), tokens, report)?;
    if !fields.finish(report, " in a radial gradient") {
        return None;
    }
    Some(stops)
}

fn stops(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Vec<GradientStop>> {
    let items = value::seq(value, path, report)?;
    let mut stops = Vec::with_capacity(items.len());
    let mut ok = true;
    for (index, item) in items.into_iter().enumerate() {
        let stop_path = format!("{path}[{index}]");
        match stop(item, &stop_path, tokens, report) {
            Some(stop) => stops.push(stop),
            None => ok = false,
        }
    }
    ok.then_some(stops)
}

fn stop(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<GradientStop> {
    let mut fields = Fields::new(value, path, report)?;
    let at = value::require(fields.take("at"), fields.path("at"), "at", report)?;
    let at = value::number(at, &fields.path("at"), report)?;
    if !(0.0..=1.0).contains(&at) {
        report.claim(
            &fields.path("at"),
            format!("a gradient stop sits from 0 to 1, found {at}"),
        );
        return None;
    }
    let color = value::require(fields.take("color"), fields.path("color"), "color", report)?;
    let color = tokens::color(color, &fields.path("color"), tokens, report)?;
    if !fields.finish(report, " in a gradient stop") {
        return None;
    }
    Some(GradientStop {
        at: at as f32,
        color,
    })
}

fn border(value: ron::Value, path: &str, tokens: &Tokens, report: &mut Report) -> Option<Border> {
    let mut fields = Fields::new(value, path, report)?;
    let width = value::require(fields.take("width"), fields.path("width"), "width", report)?;
    let width = length(width, &fields.path("width"), tokens, report)?;
    let color = value::require(fields.take("color"), fields.path("color"), "color", report)?;
    let color = tokens::color(color, &fields.path("color"), tokens, report)?;
    if !fields.finish(report, " in a border") {
        return None;
    }
    Some(Border { width, color })
}

/// A radius: a number, a `{rounded.name}` reference, or a map with one value
/// per corner.
fn radius(value: ron::Value, path: &str, tokens: &Tokens, report: &mut Report) -> Option<Radius> {
    match value {
        ron::Value::Map(_) => {
            let mut fields = Fields::new(value, path, report)?;
            let top_left = corner(&mut fields, "top_left", tokens, report)?;
            let top_right = corner(&mut fields, "top_right", tokens, report)?;
            let bottom_right = corner(&mut fields, "bottom_right", tokens, report)?;
            let bottom_left = corner(&mut fields, "bottom_left", tokens, report)?;
            if !fields.finish(report, " in a radius") {
                return None;
            }
            Some(Radius::Corners {
                top_left,
                top_right,
                bottom_right,
                bottom_left,
            })
        }
        other => radius_value(other, path, tokens, report).map(Radius::Uniform),
    }
}

fn corner(fields: &mut Fields, name: &str, tokens: &Tokens, report: &mut Report) -> Option<f32> {
    let path = fields.path(name);
    let value = value::require(fields.take(name), path.clone(), name, report)?;
    radius_value(value, &path, tokens, report)
}

fn radius_value(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<f32> {
    match value {
        ron::Value::Number(_) => Some(value::number(value, path, report)? as f32),
        ron::Value::String(text) => {
            let Some((group, name)) = value::reference(&text) else {
                if value::is_reference(&text) {
                    report.unknown_token(path, &text, "rounded");
                } else {
                    report.claim(
                        path,
                        "expected a number or a `{rounded.name}` or `{spacing.name}` reference",
                    );
                }
                return None;
            };
            match group {
                "rounded" => match tokens.rounded.get(name) {
                    Some(radius) => Some(*radius),
                    None => {
                        report.unknown_token(path, &text, "rounded");
                        None
                    }
                },
                // A radius is a length, and a length may name a spacing token;
                // the rounded group is where a named radius lives.
                "spacing" => match tokens.spacing.get(name) {
                    Some(radius) => Some(*radius),
                    None => {
                        report.unknown_token(path, &text, "spacing");
                        None
                    }
                },
                _ => {
                    report.unknown_token(path, &text, "rounded or spacing");
                    None
                }
            }
        }
        other => {
            report.claim(
                path,
                format!("expected a radius, found {}", value::kind_name(&other)),
            );
            None
        }
    }
}

fn shadows(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Vec<Shadow>> {
    let items = value::seq(value, path, report)?;
    let mut shadows = Vec::with_capacity(items.len());
    let mut ok = true;
    for (index, item) in items.into_iter().enumerate() {
        let shadow_path = format!("{path}[{index}]");
        match shadow(item, &shadow_path, tokens, report) {
            Some(shadow) => shadows.push(shadow),
            None => ok = false,
        }
    }
    ok.then_some(shadows)
}

fn shadow(value: ron::Value, path: &str, tokens: &Tokens, report: &mut Report) -> Option<Shadow> {
    let mut fields = Fields::new(value, path, report)?;
    let x = field(&mut fields, "x", tokens, report)?;
    let y = field(&mut fields, "y", tokens, report)?;
    let blur = field(&mut fields, "blur", tokens, report)?;
    let spread = field(&mut fields, "spread", tokens, report)?;
    let color = value::require(fields.take("color"), fields.path("color"), "color", report)?;
    let color = tokens::color(color, &fields.path("color"), tokens, report)?;
    if !fields.finish(report, " in a shadow") {
        return None;
    }
    Some(Shadow {
        x,
        y,
        blur,
        spread,
        color,
    })
}

/// A length: a number of pixels, or a `{spacing.name}` reference.
pub(super) fn length(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<f32> {
    match value {
        ron::Value::Number(_) => Some(value::number(value, path, report)? as f32),
        ron::Value::String(text) => {
            let Some((group, name)) = value::reference(&text) else {
                if value::is_reference(&text) {
                    report.unknown_token(path, &text, "spacing");
                } else {
                    report.claim(path, "expected a number or a `{spacing.name}` reference");
                }
                return None;
            };
            if group != "spacing" {
                report.unknown_token(path, &text, "spacing");
                return None;
            }
            match tokens.spacing.get(name) {
                Some(length) => Some(*length),
                None => {
                    report.unknown_token(path, &text, "spacing");
                    None
                }
            }
        }
        other => {
            report.claim(
                path,
                format!("expected a length, found {}", value::kind_name(&other)),
            );
            None
        }
    }
}

/// A named length field of the map, such as one shadow offset.
pub(super) fn field(
    fields: &mut Fields,
    name: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<f32> {
    let path = fields.path(name);
    let value = value::require(fields.take(name), path.clone(), name, report)?;
    length(value, &path, tokens, report)
}

pub(super) fn opacity(value: ron::Value, path: &str, report: &mut Report) -> Option<f32> {
    let number = value::number(value, path, report)?;
    if !(0.0..=1.0).contains(&number) {
        report.claim(path, format!("opacity runs from 0 to 1, found {number}"));
        return None;
    }
    Some(number as f32)
}

fn transform(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Transform> {
    let mut fields = Fields::new(value, path, report)?;
    let default = Transform::default();
    let x = match fields.take("x") {
        Some(value) => length(value, &fields.path("x"), tokens, report)?,
        None => default.x,
    };
    let y = match fields.take("y") {
        Some(value) => length(value, &fields.path("y"), tokens, report)?,
        None => default.y,
    };
    let scale = match fields.take("scale") {
        Some(value) => value::number(value, &fields.path("scale"), report)? as f32,
        None => default.scale,
    };
    let rotation_deg = match fields.take("rotation_deg") {
        Some(value) => value::number(value, &fields.path("rotation_deg"), report)? as f32,
        None => default.rotation_deg,
    };
    if !fields.finish(report, " in a transform") {
        return None;
    }
    Some(Transform {
        x,
        y,
        scale,
        rotation_deg,
    })
}
