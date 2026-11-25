//! `ui/tokens.ron`: the five token groups, `{group.name}` resolution, and the
//! rules an unknown reference or a reference cycle breaks (spec §3).
//!
//! A colour and a typography token are literal. Rounded and spacing values may
//! name another token in their own group, so resolution is depth-first with a
//! visiting chain, and a chain that returns to its own start is reported at the
//! property that closes the loop. A component is a named style that may name
//! another component; its chain resolves the same way. Every group is optional,
//! and so is the file: a plugin with no tokens is the empty table.

use std::collections::BTreeMap;

use crate::finding::{rules, Finding};

use super::file;
use super::source::Source;
use super::style::Component;
use super::value::{self, Fields, Report};

/// The token file, relative to the plugin folder.
pub(super) const FILE: &str = "tokens.ron";

/// A resolved sRGB colour with alpha, each channel from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// A resolved typography token: a face, a weight, a size, a line height and the
/// tracking each letter takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Typography {
    pub face: String,
    pub weight: u16,
    pub size_px: f32,
    pub line_height_px: f32,
    /// The space in px added after each letter: 0 when the token sets none, and
    /// negative to tighten (spec §3).
    pub tracking_px: f32,
}

/// The plugin's tokens, every reference resolved. The renderer and the later
/// plugin passes read these values; no `{group.name}` text survives loading.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tokens {
    pub colors: BTreeMap<String, Color>,
    pub typography: BTreeMap<String, Typography>,
    /// Corner radii in px, by token name.
    pub rounded: BTreeMap<String, f32>,
    /// Lengths in px, by token name.
    pub spacing: BTreeMap<String, f32>,
    /// A component is its parent's properties with its own laid over them.
    pub components: BTreeMap<String, Component>,
}

/// Read `ui/tokens.ron`. `None` means the file failed and its findings were
/// added; the surface trees are not read on.
pub(super) fn check(source: &dyn Source, findings: &mut Vec<Finding>) -> Option<Tokens> {
    if !source.paths().iter().any(|path| path == FILE) {
        return Some(Tokens::default());
    }
    let value = file::read_ron(source, FILE, "token file", findings)?;
    let file = source.display(FILE);
    let mut report = Report::new(&file, findings);
    parse(value, &mut report)
}

fn parse(value: ron::Value, report: &mut Report) -> Option<Tokens> {
    let mut fields = Fields::new(value, "", report)?;
    let mut tokens = Tokens::default();
    let mut ok = true;

    if let Some(value) = fields.take("colors") {
        match colors(value, "colors", report) {
            Some(value) => tokens.colors = value,
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("typography") {
        match typography(value, "typography", report) {
            Some(value) => tokens.typography = value,
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("rounded") {
        match lengths(value, "rounded", report) {
            Some(value) => tokens.rounded = value,
            None => ok = false,
        }
    }
    if let Some(value) = fields.take("spacing") {
        match lengths(value, "spacing", report) {
            Some(value) => tokens.spacing = value,
            None => ok = false,
        }
    }

    // A component applies colours, typography and lengths, so it is read only
    // once those groups resolved; otherwise its findings would feed on a table
    // that failed. The group is taken either way, so a failed colour does not
    // also report `components` as an unknown property.
    let component_group = fields.take("components");
    if ok {
        if let Some(value) = component_group {
            match components(value, &tokens, report) {
                Some(value) => tokens.components = value,
                None => ok = false,
            }
        }
    }
    if !fields.finish(report, " in the token file") {
        ok = false;
    }
    ok.then_some(tokens)
}

fn colors(value: ron::Value, group: &str, report: &mut Report) -> Option<BTreeMap<String, Color>> {
    let entries = token_map(value, group, report)?;
    let mut colors = BTreeMap::new();
    let mut ok = true;
    for (name, value) in entries {
        let path = format!("{group}.{name}");
        match color_literal(value, &path, report) {
            Some(color) => {
                colors.insert(name, color);
            }
            None => ok = false,
        }
    }
    ok.then_some(colors)
}

/// A colour token's value, which the format makes literal: a `(r, g, b, a)`
/// map, never another reference.
fn color_literal(value: ron::Value, path: &str, report: &mut Report) -> Option<Color> {
    let mut fields = Fields::new(value, path, report)?;
    color_fields(&mut fields, report)
}

/// A colour used as a value: a literal map, or a `{colors.name}` reference.
pub(super) fn color(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<Color> {
    match value {
        ron::Value::String(text) => color_ref(&text, path, tokens, report),
        ron::Value::Map(_) => {
            let mut fields = Fields::new(value, path, report)?;
            color_fields(&mut fields, report)
        }
        other => {
            report.claim(
                path,
                format!("expected a colour, found {}", value::kind_name(&other)),
            );
            None
        }
    }
}

fn color_ref(text: &str, path: &str, tokens: &Tokens, report: &mut Report) -> Option<Color> {
    let Some((group, name)) = value::reference(text) else {
        if value::is_reference(text) {
            report.unknown_token(path, text, "colour");
        } else {
            report.claim(path, "expected a colour or a `{colors.name}` reference");
        }
        return None;
    };
    if group != "colors" {
        report.unknown_token(path, text, "colour");
        return None;
    }
    match tokens.colors.get(name) {
        Some(color) => Some(*color),
        None => {
            report.unknown_token(path, text, "colour");
            None
        }
    }
}

pub(super) fn color_fields(fields: &mut Fields, report: &mut Report) -> Option<Color> {
    let mut ok = true;
    let r = channel(fields, "r", report, &mut ok);
    let g = channel(fields, "g", report, &mut ok);
    let b = channel(fields, "b", report, &mut ok);
    let a = channel(fields, "a", report, &mut ok);
    if !fields.finish(report, " in a colour") {
        ok = false;
    }
    match (r, g, b, a) {
        (Some(r), Some(g), Some(b), Some(a)) if ok => Some(Color { r, g, b, a }),
        _ => None,
    }
}

fn channel(fields: &mut Fields, name: &str, report: &mut Report, ok: &mut bool) -> Option<f32> {
    let path = fields.path(name);
    let value = value::require(fields.take(name), path.clone(), name, report)?;
    let number = value::number(value, &path, report)?;
    if !(0.0..=1.0).contains(&number) {
        report.claim(
            &path,
            format!("a colour channel runs from 0 to 1, found {number}"),
        );
        *ok = false;
        return None;
    }
    Some(number as f32)
}

fn typography(
    value: ron::Value,
    group: &str,
    report: &mut Report,
) -> Option<BTreeMap<String, Typography>> {
    let entries = token_map(value, group, report)?;
    let mut tokens = BTreeMap::new();
    let mut ok = true;
    for (name, value) in entries {
        let path = format!("{group}.{name}");
        match typography_token(value, &path, report) {
            Some(token) => {
                tokens.insert(name, token);
            }
            None => ok = false,
        }
    }
    ok.then_some(tokens)
}

fn typography_token(value: ron::Value, path: &str, report: &mut Report) -> Option<Typography> {
    let mut fields = Fields::new(value, path, report)?;
    let face = value::require(fields.take("face"), fields.path("face"), "face", report)?;
    let face = value::string(face, &fields.path("face"), report)?;
    let weight = value::require(
        fields.take("weight"),
        fields.path("weight"),
        "weight",
        report,
    )?;
    let weight = value::whole(weight, &fields.path("weight"), 0.0, 65535.0, report)? as u16;
    let size = value::require(
        fields.take("size_px"),
        fields.path("size_px"),
        "size_px",
        report,
    )?;
    let size = value::number(size, &fields.path("size_px"), report)? as f32;
    let line = value::require(
        fields.take("line_height_px"),
        fields.path("line_height_px"),
        "line_height_px",
        report,
    )?;
    let line = value::number(line, &fields.path("line_height_px"), report)? as f32;
    let tracking = match fields.take("tracking_px") {
        Some(value) => {
            let property = fields.path("tracking_px");
            value::number(value, &property, report)? as f32
        }
        None => 0.0,
    };
    if !fields.finish(report, " in a typography token") {
        return None;
    }
    Some(Typography {
        face,
        weight,
        size_px: size,
        line_height_px: line,
        tracking_px: tracking,
    })
}

/// One token group, as the name/value pairs authored in it.
fn token_map(
    value: ron::Value,
    group: &str,
    report: &mut Report,
) -> Option<Vec<(String, ron::Value)>> {
    let map = match value {
        ron::Value::Map(map) => map,
        // `()` is how an author writes an empty group, as it is for an empty
        // token file.
        ron::Value::Unit => return Some(Vec::new()),
        other => {
            report.claim(
                group,
                format!("expected a token group, found {}", value::kind_name(&other)),
            );
            return None;
        }
    };
    let mut tokens = Vec::with_capacity(map.len());
    let mut ok = true;
    for (key, value) in map {
        match key {
            ron::Value::String(name) => tokens.push((name, value)),
            other => {
                report.claim(
                    group,
                    format!(
                        "a token name must be a string, found {}",
                        value::kind_name(&other)
                    ),
                );
                ok = false;
            }
        }
    }
    ok.then_some(tokens)
}

enum RawLength {
    Px(f32),
    Reference(String),
}

struct RawToken {
    raw: RawLength,
    path: String,
}

fn lengths(value: ron::Value, group: &str, report: &mut Report) -> Option<BTreeMap<String, f32>> {
    let entries = token_map(value, group, report)?;
    let mut raw = BTreeMap::new();
    let mut ok = true;
    for (name, value) in entries {
        let path = format!("{group}.{name}");
        match raw_length(value, &path, group, report) {
            Some(length) => {
                raw.insert(name, RawToken { raw: length, path });
            }
            None => ok = false,
        }
    }
    if !ok {
        return None;
    }
    let mut resolved = BTreeMap::new();
    let mut visiting = Vec::new();
    for name in raw.keys() {
        resolve_length(group, name, &raw, &mut resolved, &mut visiting, report)?;
    }
    Some(resolved)
}

fn raw_length(
    value: ron::Value,
    path: &str,
    group: &str,
    report: &mut Report,
) -> Option<RawLength> {
    match value {
        ron::Value::Number(_) => Some(RawLength::Px(value::number(value, path, report)? as f32)),
        ron::Value::String(text) => {
            if !value::is_reference(&text) {
                report.claim(
                    path,
                    format!("expected a number or a `{{{group}.name}}` reference"),
                );
                return None;
            }
            match value::reference(&text) {
                Some((reference_group, name)) if reference_group == group => {
                    Some(RawLength::Reference(name.to_string()))
                }
                _ => {
                    report.unknown_token(path, &text, group);
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

fn resolve_length(
    group: &str,
    name: &str,
    raw: &BTreeMap<String, RawToken>,
    resolved: &mut BTreeMap<String, f32>,
    visiting: &mut Vec<String>,
    report: &mut Report,
) -> Option<f32> {
    if let Some(value) = resolved.get(name) {
        return Some(*value);
    }
    let token = raw.get(name)?;
    let target = match &token.raw {
        RawLength::Px(value) => {
            resolved.insert(name.to_string(), *value);
            return Some(*value);
        }
        RawLength::Reference(target) => target,
    };

    visiting.push(name.to_string());
    let value = if visiting.iter().any(|visiting| visiting == target) {
        report.cycle(&token.path, &chain(group, visiting, target));
        None
    } else if !raw.contains_key(target) {
        report.unknown_token(&token.path, &format!("{{{group}.{target}}}"), group);
        None
    } else {
        resolve_length(group, target, raw, resolved, visiting, report)
    };
    visiting.pop();
    if let Some(value) = value {
        resolved.insert(name.to_string(), value);
    }
    value
}

fn chain(group: &str, visiting: &[String], target: &str) -> String {
    visiting
        .iter()
        .map(|name| format!("{group}.{name}"))
        .chain([format!("{group}.{target}")])
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn components(
    value: ron::Value,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<BTreeMap<String, Component>> {
    let entries = token_map(value, "components", report)?;
    let mut raw = BTreeMap::new();
    for (name, value) in entries {
        let path = format!("components.{name}");
        let (parent, style) = component(value, &path, tokens, report)?;
        raw.insert(
            name,
            RawComponent {
                parent,
                style,
                path,
            },
        );
    }
    let mut resolved = BTreeMap::new();
    let mut visiting = Vec::new();
    for name in raw.keys() {
        resolve_component(name, &raw, &mut resolved, &mut visiting, report)?;
    }
    Some(resolved)
}

struct RawComponent {
    parent: Option<String>,
    style: Component,
    path: String,
}

fn component(
    value: ron::Value,
    path: &str,
    tokens: &Tokens,
    report: &mut Report,
) -> Option<(Option<String>, Component)> {
    let mut fields = Fields::new(value, path, report)?;
    let parent = match fields.take("component") {
        Some(value) => Some(value::string(value, &fields.path("component"), report)?),
        None => None,
    };
    let style = super::style::parse(&mut fields, tokens, report)?;
    if !fields.finish(report, " in a component") {
        return None;
    }
    Some((parent, style))
}

fn resolve_component(
    name: &str,
    raw: &BTreeMap<String, RawComponent>,
    resolved: &mut BTreeMap<String, Component>,
    visiting: &mut Vec<String>,
    report: &mut Report,
) -> Option<Component> {
    if let Some(style) = resolved.get(name) {
        return Some(style.clone());
    }
    let component = raw.get(name)?;

    visiting.push(name.to_string());
    let mut style = Component::default();
    if let Some(parent) = &component.parent {
        let property = format!("{}.component", component.path);
        if visiting.iter().any(|visiting| visiting == parent) {
            let chain = visiting
                .iter()
                .map(|name| format!("components.{name}"))
                .chain([format!("components.{parent}")])
                .collect::<Vec<_>>()
                .join(" -> ");
            report.cycle(&property, &chain);
            visiting.pop();
            return None;
        }
        if !raw.contains_key(parent) {
            report.rule(
                &property,
                rules::UI_TOKEN_UNKNOWN,
                format!("there is no component named `{parent}`"),
            );
            visiting.pop();
            return None;
        }
        let base = resolve_component(parent, raw, resolved, visiting, report);
        let Some(base) = base else {
            visiting.pop();
            return None;
        };
        style = base;
    }
    style = style.over(&component.style);
    visiting.pop();
    resolved.insert(name.to_string(), style.clone());
    Some(style)
}
