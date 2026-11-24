//! The six node kinds, the tree they form, and the findings a node carries
//! (spec §4). A tree file's value is its root, which must be a `box`, and every
//! node's finding path is its place in that tree: `root.children[2].children[0]`.
//!
//! The parser is deliberately shallow about what a node means and strict about
//! what it holds: a node kind that is not one of the six, a property that kind
//! does not take, or a property the format does not name fails with `ui.parse`
//! at the field. The node-count and depth caps are enforced here because this
//! is where the tree is built, and they stop the walk rather than measure a
//! tree the plugin has already lost.

use crate::finding::{rules, Finding};

use super::caps::{MAX_DEPTH, MAX_NODES};
use super::paint::{self, StateOverrides};
use super::style::{
    self, AlignItems, Component, Direction, JustifyContent, Padding, Style, TextStyle,
};
use super::tokens::{self, Tokens};
use super::value::{self, Fields, Report};

/// The most characters a glyph may hold (spec §4, rule 11).
pub const MAX_GLYPH_CHARS: usize = 8;

/// One node of a surface tree with every token resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub style: Style,
    pub kind: NodeKind,
    /// The state overrides the node carries. A control slot and a list always
    /// may; every node inside a list item may, and follows the item's control
    /// (spec §4, "States inside a list item").
    pub states: ControlStates,
}

/// What the node is and what it holds besides its style.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    Box(BoxNode),
    Image(ImageNode),
    Glyph(GlyphNode),
    Text(TextNode),
    Slot(SlotNode),
    List(ListNode),
}

/// A flex container. It is the only kind that has children.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxNode {
    pub direction: Direction,
    pub gap: f32,
    pub padding: Padding,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
    pub children: Vec<Node>,
}

/// A file under the plugin folder, drawn with one fit.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageNode {
    pub file: String,
    pub fit: Fit,
}

/// How an image fills its node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    #[default]
    Cover,
    Contain,
    Stretch,
    None,
}

/// Decorative text of at most [`MAX_GLYPH_CHARS`] characters. It is ornament,
/// never copy.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphNode {
    pub glyph: String,
}

/// The text of one copy key. Ticket #107 checks that the key exists.
#[derive(Clone, Debug, PartialEq)]
pub struct TextNode {
    pub text: String,
}

/// One named piece of engine content, or one control.
///
/// A slot takes children because the two ornament slots are carriers: the
/// plugin authors the `advance-marker` and `deciding-marker` nodes itself and
/// the engine only decides when they show (spec §5). Which slots may hold
/// content is ticket #107's interface rule, so the parser accepts it on any
/// slot and reads it the way a box's children are read.
///
/// A slot that shows text takes the face and the tone the engine's own text is
/// drawn in, because the words are the engine's and only their look is the
/// plugin's (spec §4, "Engine text"). [`super::text`] judges that, and the
/// resolved pair lives on [`Node::style`].
#[derive(Clone, Debug, PartialEq)]
pub struct SlotNode {
    pub slot: String,
    pub children: Vec<Node>,
}

/// One engine list and its item template. The engine supplies the items in
/// their specified order; the template places each item's slots and controls.
#[derive(Clone, Debug, PartialEq)]
pub struct ListNode {
    pub list: String,
    pub item: Box<Node>,
}

/// The paint a control or a list item applies in each state. A state that was
/// not authored keeps the node's own paint.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlStates {
    pub focused: Option<StateOverrides>,
    pub disabled: Option<StateOverrides>,
    pub lit: Option<StateOverrides>,
    pub current: Option<StateOverrides>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Box,
    Image,
    Glyph,
    Text,
    Slot,
    List,
}

impl Shape {
    fn parse(name: &str) -> Option<Shape> {
        match name {
            "box" => Some(Shape::Box),
            "image" => Some(Shape::Image),
            "glyph" => Some(Shape::Glyph),
            "text" => Some(Shape::Text),
            "slot" => Some(Shape::Slot),
            "list" => Some(Shape::List),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Shape::Box => "box",
            Shape::Image => "image",
            Shape::Glyph => "glyph",
            Shape::Text => "text",
            Shape::Slot => "slot",
            Shape::List => "list",
        }
    }
}

#[derive(Default)]
struct Budget {
    nodes: usize,
    capped: bool,
}

pub(super) fn parse_tree(
    value: ron::Value,
    tokens: &Tokens,
    file: &str,
    findings: &mut Vec<Finding>,
) -> Option<Node> {
    let mut report = Report::new(file, findings);
    let mut budget = Budget::default();
    let node = parse_node(
        value,
        "root",
        1,
        &mut budget,
        tokens,
        &mut report,
        &TextStyle::default(),
        false,
    )?;
    if !matches!(node.kind, NodeKind::Box(_)) {
        report.claim("root.kind", "a tree's root is a `box`");
        return None;
    }
    Some(node)
}

#[allow(
    clippy::too_many_arguments,
    reason = "one node walk passes its distinct facts straight through: the value, its path, the depth, the node budget, the token table, the findings report, the text style the enclosing boxes set and whether the node sits in a list item"
)]
fn parse_node(
    value: ron::Value,
    path: &str,
    depth: usize,
    budget: &mut Budget,
    tokens: &Tokens,
    report: &mut Report,
    inherited: &TextStyle,
    inside_item: bool,
) -> Option<Node> {
    if depth > MAX_DEPTH {
        report.cap(format!(
            "the tree reaches {depth} nodes deep; the cap is {MAX_DEPTH}"
        ));
        return None;
    }
    budget.nodes += 1;
    if budget.nodes > MAX_NODES {
        if !budget.capped {
            report.cap(format!(
                "the tree reaches {} nodes; the cap is {MAX_NODES}",
                budget.nodes
            ));
            budget.capped = true;
        }
        return None;
    }

    let mut fields = Fields::new(value, path, report)?;
    let kind_value = value::require(fields.take("kind"), fields.path("kind"), "kind", report)?;
    let kind_name = value::string(kind_value, &fields.path("kind"), report)?;
    let Some(shape) = Shape::parse(&kind_name) else {
        report.claim(
            &fields.path("kind"),
            format!("`{kind_name}` is not one of `box`, `image`, `glyph`, `text`, `slot`, `list`"),
        );
        return None;
    };

    let base = match fields.take("component") {
        Some(value) => {
            let property = fields.path("component");
            let name = value::string(value, &property, report)?;
            match tokens.components.get(&name) {
                Some(component) => component.clone(),
                None => {
                    report.rule(
                        &property,
                        rules::UI_TOKEN_UNKNOWN,
                        format!("there is no component named `{name}`"),
                    );
                    return None;
                }
            }
        }
        None => Component::default(),
    };
    let own = style::parse(&mut fields, tokens, report)?;
    let merged = base.over(&own);
    let text = inherited.over(&TextStyle {
        typography: merged.typography.clone(),
        color: merged.color,
    });

    let kind = match shape {
        Shape::Box => {
            let children = parse_children(
                &mut fields,
                depth,
                budget,
                tokens,
                report,
                &text,
                inside_item,
            )?;
            NodeKind::Box(BoxNode {
                direction: merged.direction.unwrap_or_default(),
                gap: merged.gap.unwrap_or_default(),
                padding: merged.padding.unwrap_or_default(),
                align_items: merged.align_items.unwrap_or_default(),
                justify_content: merged.justify_content.unwrap_or_default(),
                children,
            })
        }
        Shape::Image => {
            let file = take_string(&mut fields, "file", report)?;
            if let Err(reason) = super::source::check_relative(&file) {
                report.rule(&fields.path("file"), rules::UI_PATH_ESCAPE, reason);
                return None;
            }
            let fit = match fields.take("fit") {
                Some(value) => value::variant(
                    value,
                    &fields.path("fit"),
                    &[
                        ("cover", Fit::Cover),
                        ("contain", Fit::Contain),
                        ("stretch", Fit::Stretch),
                        ("none", Fit::None),
                    ],
                    report,
                )?,
                None => Fit::default(),
            };
            NodeKind::Image(ImageNode { file, fit })
        }
        Shape::Glyph => {
            let glyph = take_string(&mut fields, "glyph", report)?;
            let count = glyph.chars().count();
            if count > MAX_GLYPH_CHARS {
                report.claim(
                    &fields.path("glyph"),
                    format!("a glyph is {count} characters; the limit is {MAX_GLYPH_CHARS}"),
                );
                return None;
            }
            NodeKind::Glyph(GlyphNode { glyph })
        }
        Shape::Text => {
            let key = take_string(&mut fields, "text", report)?;
            NodeKind::Text(TextNode { text: key })
        }
        Shape::Slot => {
            let slot = take_string(&mut fields, "slot", report)?;
            let children = parse_children(
                &mut fields,
                depth,
                budget,
                tokens,
                report,
                &text,
                inside_item,
            )?;
            NodeKind::Slot(SlotNode { slot, children })
        }
        Shape::List => {
            let list = take_string(&mut fields, "list", report)?;
            let item = value::require(fields.take("item"), fields.path("item"), "item", report)?;
            let item = parse_node(
                item,
                &fields.path("item"),
                depth + 1,
                budget,
                tokens,
                report,
                inherited,
                true,
            )?;
            NodeKind::List(ListNode {
                list,
                item: Box::new(item),
            })
        }
    };

    let states = if inside_item || matches!(shape, Shape::Slot | Shape::List) {
        parse_states(&mut fields, path, tokens, report)?
    } else {
        ControlStates::default()
    };
    if !fields.finish(report, &format!(" on a `{}` node", shape.name())) {
        return None;
    }
    if !check_kind(&merged, shape, path, report) {
        return None;
    }
    Some(Node {
        style: Style::resolve(&merged, text),
        kind,
        states,
    })
}

fn parse_children(
    fields: &mut Fields,
    depth: usize,
    budget: &mut Budget,
    tokens: &Tokens,
    report: &mut Report,
    inherited: &TextStyle,
    inside_item: bool,
) -> Option<Vec<Node>> {
    let mut children = Vec::new();
    if let Some(value) = fields.take("children") {
        let property = fields.path("children");
        let items = value::seq(value, &property, report)?;
        for (index, item) in items.into_iter().enumerate() {
            let child_path = format!("{property}[{index}]");
            let child = parse_node(
                item,
                &child_path,
                depth + 1,
                budget,
                tokens,
                report,
                inherited,
                inside_item,
            )?;
            children.push(child);
        }
    }
    Some(children)
}

fn take_string(fields: &mut Fields, name: &str, report: &mut Report) -> Option<String> {
    let path = fields.path(name);
    let value = value::require(fields.take(name), path.clone(), name, report)?;
    value::string(value, &path, report)
}

fn parse_states(
    fields: &mut Fields,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<ControlStates> {
    let focused = state(fields, "focused", path, tokens, report)?;
    let disabled = state(fields, "disabled", path, tokens, report)?;
    let lit = state(fields, "lit", path, tokens, report)?;
    let current = state(fields, "current", path, tokens, report)?;
    Some(ControlStates {
        focused,
        disabled,
        lit,
        current,
    })
}

fn state(
    fields: &mut Fields,
    name: &str,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Option<StateOverrides>> {
    let Some(value) = fields.take(name) else {
        return Some(None);
    };
    let property = format!("{path}.{name}");
    let mut state = Fields::new(value, &property, report)?;
    let mut paint = paint::parse(&mut state, tokens, report)?;
    if let Some(value) = state.take("typography") {
        let property = state.path("typography");
        paint.typography = Some(style::typography(value, &property, tokens, report)?);
    }
    if let Some(value) = state.take("color") {
        let property = state.path("color");
        paint.color = Some(tokens::color(value, &property, tokens, report)?);
    }
    if !state.finish(report, " in a state override") {
        return None;
    }
    Some(Some(paint))
}

/// The properties a node kind accepts. A component is applied before this
/// check, so a property the kind cannot use is reported at the node that named
/// the component, which is where the mistake shows.
fn check_kind(component: &Component, shape: Shape, path: &str, report: &mut Report) -> bool {
    let kind = shape.name();
    let mut ok = true;
    {
        let mut reject = |is_set: bool, property: &str| {
            if is_set {
                report.claim(
                    &format!("{path}.{property}"),
                    format!("a `{kind}` node does not take `{property}`"),
                );
            }
            !is_set
        };
        if shape != Shape::Box {
            ok &= reject(component.direction.is_some(), "direction");
            ok &= reject(component.gap.is_some(), "gap");
            ok &= reject(component.padding.is_some(), "padding");
            ok &= reject(component.align_items.is_some(), "align_items");
            ok &= reject(component.justify_content.is_some(), "justify_content");
        }
        if !matches!(shape, Shape::Text | Shape::Glyph | Shape::Slot | Shape::Box) {
            ok &= reject(component.typography.is_some(), "typography");
        }
        if !matches!(shape, Shape::Slot | Shape::Box) {
            ok &= reject(component.color.is_some(), "color");
        }
        match shape {
            Shape::Text => {
                ok &= reject(component.border.is_some(), "border");
                ok &= reject(component.radius.is_some(), "radius");
                ok &= reject(component.shadows.is_some(), "shadow");
                ok &= reject(component.opacity.is_some(), "opacity");
                ok &= reject(component.transform.is_some(), "transform");
            }
            Shape::Slot | Shape::List => {
                ok &= reject(component.fill.is_some(), "fill");
                ok &= reject(component.border.is_some(), "border");
                ok &= reject(component.radius.is_some(), "radius");
                ok &= reject(component.shadows.is_some(), "shadow");
                ok &= reject(component.opacity.is_some(), "opacity");
                ok &= reject(component.transform.is_some(), "transform");
            }
            Shape::Box | Shape::Image | Shape::Glyph => {}
        }
    }
    ok
}
