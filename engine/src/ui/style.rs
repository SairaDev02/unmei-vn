//! The style properties a node and a component share: the paint, the sizing,
//! the position, the typography and the loop (spec §4).
//!
//! [`Component`] keeps every property optional because a component is a set of
//! properties a node may borrow and override, and a node's own properties are
//! read into the same shape so one merge handles both. [`Style`] is the
//! resolved form a node draws with: every default is applied and there is no
//! `{group.name}` left. The syntax of each property is unchanged from the node
//! that writes it, so a component's typo is reported where it is written.

use super::motion::{self, Motion};
use super::paint::{self, Border, Fill, Paint, Radius, Shadow, Transform};
use super::tokens::{self, Color, Tokens, Typography};
use super::value::{self, Fields, Report};

/// The properties a component sets and a node may override. Nothing here is
/// structural: `kind`, `children`, `item`, the content fields and a control's
/// states stay on the node, because a component is a style, not a template.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Component {
    pub fill: Option<Fill>,
    pub border: Option<Border>,
    pub radius: Option<Radius>,
    pub shadows: Option<Vec<Shadow>>,
    pub opacity: Option<f32>,
    pub transform: Option<Transform>,
    pub direction: Option<Direction>,
    pub gap: Option<f32>,
    pub padding: Option<Padding>,
    pub align_items: Option<AlignItems>,
    pub justify_content: Option<JustifyContent>,
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub position: Option<Position>,
    pub left: Option<f32>,
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub z_index: Option<i32>,
    pub typography: Option<Typography>,
    /// A text slot's own tone (spec §4, "Engine text").
    pub color: Option<Color>,
    pub motion: Option<Motion>,
}

impl Component {
    /// `self` with `winner`'s properties laid over it: a node's own value beats
    /// the component it names, which beats that component's parent.
    pub(super) fn over(&self, winner: &Component) -> Component {
        Component {
            fill: winner.fill.clone().or_else(|| self.fill.clone()),
            border: winner.border.or(self.border),
            radius: winner.radius.or(self.radius),
            shadows: winner.shadows.clone().or_else(|| self.shadows.clone()),
            opacity: winner.opacity.or(self.opacity),
            transform: winner.transform.or(self.transform),
            direction: winner.direction.or(self.direction),
            gap: winner.gap.or(self.gap),
            padding: winner.padding.or(self.padding),
            align_items: winner.align_items.or(self.align_items),
            justify_content: winner.justify_content.or(self.justify_content),
            width: winner.width.or(self.width),
            height: winner.height.or(self.height),
            position: winner.position.or(self.position),
            left: winner.left.or(self.left),
            top: winner.top.or(self.top),
            right: winner.right.or(self.right),
            bottom: winner.bottom.or(self.bottom),
            z_index: winner.z_index.or(self.z_index),
            typography: winner
                .typography
                .clone()
                .or_else(|| self.typography.clone()),
            color: winner.color.or(self.color),
            motion: winner.motion.clone().or_else(|| self.motion.clone()),
        }
    }
}

/// A node's resolved style: the values the renderer and the layout read.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub paint: Paint,
    pub width: Dimension,
    pub height: Dimension,
    pub position: Position,
    pub offsets: Offsets,
    pub z_index: i32,
    pub motion: Option<Motion>,
    pub text: TextStyle,
}

impl Style {
    pub(super) fn resolve(component: &Component, text: TextStyle) -> Style {
        Style {
            paint: Paint {
                fill: component.fill.clone(),
                border: component.border,
                radius: component.radius,
                shadows: component.shadows.clone().unwrap_or_default(),
                opacity: component.opacity.unwrap_or(1.0),
                transform: component.transform.unwrap_or_default(),
            },
            width: component.width.unwrap_or_default(),
            height: component.height.unwrap_or_default(),
            position: component.position.unwrap_or_default(),
            offsets: Offsets {
                left: component.left,
                top: component.top,
                right: component.right,
                bottom: component.bottom,
            },
            z_index: component.z_index.unwrap_or(0),
            motion: component.motion.clone(),
            text,
        }
    }
}

/// The face and tone a node's text draws with: the node's own values over the
/// nearest enclosing `Box`'s (spec §4, "Engine text").
///
/// Either half may be absent on a node that shows no text, because the value
/// still flows to its children; a node that shows text must resolve both, which
/// [`super::text`] judges.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextStyle {
    pub typography: Option<Typography>,
    pub color: Option<Color>,
}

impl TextStyle {
    /// `self` with `winner`'s values laid over it: a node's own face and tone
    /// beat the enclosing `Box`'s, which beats the one outside it.
    pub(super) fn over(&self, winner: &TextStyle) -> TextStyle {
        TextStyle {
            typography: winner
                .typography
                .clone()
                .or_else(|| self.typography.clone()),
            color: winner.color.or(self.color),
        }
    }
}

/// A child's main axis inside a box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Row,
    Column,
}

/// How a box places its children across the main axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignItems {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

/// How a box distributes its children along the main axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JustifyContent {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Whether a node takes part in its parent's flow or is placed by its offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Position {
    #[default]
    Relative,
    Absolute,
}

/// A width or height: sized by content, filled into the parent, or a length.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Dimension {
    #[default]
    Auto,
    Fill,
    Px(f32),
}

/// A box's space around its children.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Padding {
    Uniform(f32),
    Sides {
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    },
}

impl Default for Padding {
    fn default() -> Self {
        Padding::Uniform(0.0)
    }
}

/// A node's four offsets, each present only when it is authored.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Offsets {
    pub left: Option<f32>,
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
}

/// Read the style properties the map holds, leaving content and children for
/// the node's own parser.
pub(super) fn parse(
    fields: &mut Fields,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Component> {
    let paint = paint::parse(fields, tokens, report)?;
    let mut component = Component {
        fill: paint.fill,
        border: paint.border,
        radius: paint.radius,
        shadows: paint.shadows,
        opacity: paint.opacity,
        transform: paint.transform,
        ..Component::default()
    };
    let mut ok = true;

    if let Some(value) = fields.take("direction") {
        let property = fields.path("direction");
        match value::variant(
            value,
            &property,
            &[("row", Direction::Row), ("column", Direction::Column)],
            report,
        ) {
            Some(value) => component.direction = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("gap") {
        let property = fields.path("gap");
        match paint::length(value, &property, tokens, report) {
            Some(value) => component.gap = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("padding") {
        let property = fields.path("padding");
        match padding(value, &property, tokens, report) {
            Some(value) => component.padding = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("align_items") {
        let property = fields.path("align_items");
        match value::variant(
            value,
            &property,
            &[
                ("start", AlignItems::Start),
                ("center", AlignItems::Center),
                ("end", AlignItems::End),
                ("stretch", AlignItems::Stretch),
            ],
            report,
        ) {
            Some(value) => component.align_items = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("justify_content") {
        let property = fields.path("justify_content");
        match value::variant(
            value,
            &property,
            &[
                ("start", JustifyContent::Start),
                ("center", JustifyContent::Center),
                ("end", JustifyContent::End),
                ("space-between", JustifyContent::SpaceBetween),
            ],
            report,
        ) {
            Some(value) => component.justify_content = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("width") {
        let property = fields.path("width");
        match dimension(value, &property, tokens, report) {
            Some(value) => component.width = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("height") {
        let property = fields.path("height");
        match dimension(value, &property, tokens, report) {
            Some(value) => component.height = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("position") {
        let property = fields.path("position");
        match value::variant(
            value,
            &property,
            &[
                ("relative", Position::Relative),
                ("absolute", Position::Absolute),
            ],
            report,
        ) {
            Some(value) => component.position = Some(value),
            None => ok = false,
        }
    }
    for (name, slot) in [
        ("left", &mut component.left),
        ("top", &mut component.top),
        ("right", &mut component.right),
        ("bottom", &mut component.bottom),
    ] {
        if let Some(value) = fields.take(name) {
            let property = fields.path(name);
            match paint::length(value, &property, tokens, report) {
                Some(value) => *slot = Some(value),
                None => ok = false,
            }
        }
    }
    if let Some(value) = fields.take("z_index") {
        let property = fields.path("z_index");
        match value::whole(
            value,
            &property,
            f64::from(i32::MIN),
            f64::from(i32::MAX),
            report,
        ) {
            Some(value) => component.z_index = Some(value as i32),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("typography") {
        let property = fields.path("typography");
        match typography(value, &property, tokens, report) {
            Some(value) => component.typography = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("color") {
        let property = fields.path("color");
        match tokens::color(value, &property, tokens, report) {
            Some(value) => component.color = Some(value),
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("loop") {
        let property = fields.path("loop");
        match motion::parse(value, &property, tokens, report) {
            Some(value) => component.motion = Some(value),
            None => ok = false,
        }
    }

    ok.then_some(component)
}

fn dimension(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Dimension> {
    if let ron::Value::String(text) = &value {
        if !value::is_reference(text) {
            return match text.as_str() {
                "auto" => Some(Dimension::Auto),
                "fill" => Some(Dimension::Fill),
                _ => {
                    report.claim(
                        path,
                        format!("`{text}` is not `auto`, `fill` or a `{{spacing.name}}` reference"),
                    );
                    None
                }
            };
        }
    }
    paint::length(value, path, tokens, report).map(Dimension::Px)
}

fn padding(value: ron::Value, path: &str, tokens: &Tokens, report: &mut Report) -> Option<Padding> {
    if matches!(value, ron::Value::Map(_)) {
        let mut fields = Fields::new(value, path, report)?;
        let top = paint::field(&mut fields, "top", tokens, report)?;
        let right = paint::field(&mut fields, "right", tokens, report)?;
        let bottom = paint::field(&mut fields, "bottom", tokens, report)?;
        let left = paint::field(&mut fields, "left", tokens, report)?;
        if !fields.finish(report, " in padding") {
            return None;
        }
        return Some(Padding::Sides {
            top,
            right,
            bottom,
            left,
        });
    }
    paint::length(value, path, tokens, report).map(Padding::Uniform)
}

pub(super) fn typography(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Typography> {
    let text = value::string(value, path, report)?;
    let Some((group, name)) = value::reference(&text) else {
        if value::is_reference(&text) {
            report.unknown_token(path, &text, "typography");
        } else {
            report.claim(path, "expected a `{typography.name}` reference");
        }
        return None;
    };
    if group != "typography" {
        report.unknown_token(path, &text, "typography");
        return None;
    }
    match tokens.typography.get(name) {
        Some(typography) => Some(typography.clone()),
        None => {
            report.unknown_token(path, &text, "typography");
            None
        }
    }
}
