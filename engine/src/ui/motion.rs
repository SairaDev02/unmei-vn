//! A node's loop — its period, easing and keyframes — and the rules that decide
//! where a loop may appear and how fast it may flash (spec §6).
//!
//! The parser accepts a loop on any node because the format's syntax does;
//! [`check`] is where §6 judges one, once per surface tree and with the tree's
//! node paths in hand. A keyframe states what it changes and leaves the rest at
//! the node's neutral value, which is what makes the format's opacity-only
//! example a complete keyframe.

use crate::finding::{rules, Finding, Location};
use crate::keiko::is_control_id;

use super::node::{Node, NodeKind};
use super::paint;
use super::surface::ORNAMENT_SLOTS;
use super::tokens::Tokens;
use super::value::{self, Fields, Report};

/// The shortest period a loop may have (spec §6). The parser accepts less,
/// because the floor is a rule about the loop rather than its syntax.
pub const MIN_PERIOD_MS: u32 = 400;

/// How many flashes one second of a loop may hold (spec §6).
pub const MAX_FLASHES_PER_SECOND: u64 = 3;

/// The opacity change a flash needs on each side of its turn (spec §6).
const MIN_FLASH_STEP: f32 = 0.2;

/// How far under [`MIN_FLASH_STEP`] a step may land and still count.
///
/// Opacity is f32, so the authored decimals subtract to just under their
/// decimal difference: 1.0 - 0.8 is 0.19999999. The spec says "at least 0.2",
/// and an author writing 1.0 and 0.8 means the boundary, not a miss.
const STEP_TOLERANCE: f32 = 1e-6;

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

/// Judge every loop in one surface tree: where it sits, how long its period is
/// and how fast it flashes (spec §6). Returns whether the tree kept all of them.
pub(super) fn check(surface: &str, tree: &Node, file: &str, findings: &mut Vec<Finding>) -> bool {
    let before = findings.len();
    walk(tree, "root", surface, &Place::default(), file, findings);
    findings.len() == before
}

/// The three facts a loop's placement depends on, carried down the walk.
#[derive(Clone, Copy, Default)]
struct Place<'a> {
    inside_list_item: bool,
    /// The innermost slot the node sits in. The two ornament slots hold the
    /// plugin's own nodes, so a loop inside them is ornament; every other slot
    /// holds engine content, which stays still.
    slot: Option<&'a str>,
}

fn walk(
    node: &Node,
    path: &str,
    surface: &str,
    place: &Place<'_>,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    if let Some(motion) = &node.style.motion {
        if let Some(reason) = refusal(surface, &node.kind, place) {
            report(findings, file, path, rules::UI_MOTION_FORBIDDEN, reason);
        }
        if let Some(reason) = short_period(motion) {
            report(findings, file, path, rules::UI_MOTION_FORBIDDEN, reason);
        }
        if let Some(reason) = fast_flash(motion) {
            report(findings, file, path, rules::UI_MOTION_FLASH, reason);
        }
    }
    match &node.kind {
        NodeKind::Box(box_node) => {
            children(&box_node.children, path, surface, place, file, findings)
        }
        NodeKind::Slot(slot) => children(
            &slot.children,
            path,
            surface,
            &Place {
                slot: Some(&slot.slot),
                ..*place
            },
            file,
            findings,
        ),
        NodeKind::List(list) => walk(
            &list.item,
            &format!("{path}.item"),
            surface,
            &Place {
                inside_list_item: true,
                ..*place
            },
            file,
            findings,
        ),
        NodeKind::Image(_) | NodeKind::Glyph(_) | NodeKind::Text(_) => {}
    }
}

fn children(
    nodes: &[Node],
    path: &str,
    surface: &str,
    place: &Place<'_>,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    for (index, node) in nodes.iter().enumerate() {
        walk(
            node,
            &format!("{path}.children[{index}]"),
            surface,
            place,
            file,
            findings,
        );
    }
}

/// Why a loop may not sit on this node, or `None` when it may (spec §6).
fn refusal(surface: &str, kind: &NodeKind, place: &Place<'_>) -> Option<String> {
    if !matches!(surface, "title" | "play") {
        return Some(format!("the `{surface}` surface stays static"));
    }
    if place.inside_list_item {
        return Some("a list item takes no loop".to_string());
    }
    if let Some(slot) = place.slot {
        if !ORNAMENT_SLOTS.contains(&slot) {
            return Some(format!("`{slot}` holds engine content, which stays still"));
        }
    }
    match kind {
        NodeKind::Box(_) | NodeKind::Image(_) | NodeKind::Glyph(_) => None,
        NodeKind::Slot(slot) if is_control_id(&slot.slot) => Some(format!(
            "`{}` is a control, and a loop cannot run on a control",
            slot.slot
        )),
        NodeKind::Slot(slot) => Some(format!(
            "`{}` is a slot, and a loop cannot run on a slot",
            slot.slot
        )),
        NodeKind::Text(_) | NodeKind::List(_) => {
            Some("only `box`, `image` and `glyph` nodes take a loop".to_string())
        }
    }
}

/// Why the loop's period is below the floor, or `None` when it is at or above.
fn short_period(motion: &Motion) -> Option<String> {
    (motion.period_ms < MIN_PERIOD_MS).then(|| {
        format!(
            "the loop's period is {} ms; the floor is {MIN_PERIOD_MS} ms",
            motion.period_ms
        )
    })
}

/// Why the loop flashes too fast, or `None` when it stays under the cap.
///
/// A flash is two opposing opacity changes of at least [`MIN_FLASH_STEP`] in a
/// row, so it takes three consecutive keyframes. The list is a ring: a loop
/// repeats forever, so the step from the last keyframe back to the first ends
/// the period and the turn that step completes sits at `keyframes[0]`. Each
/// keyframe is a turn's centre once, so the count is turns per period and not
/// triples visited. The rate compares as integers: that keeps the cap exact at
/// its boundary and leaves a sub-floor period to [`short_period`], which
/// already reported it.
fn fast_flash(motion: &Motion) -> Option<String> {
    if motion.period_ms < MIN_PERIOD_MS {
        return None;
    }
    let flashes = turns_in_one_period(&motion.keyframes) as u64;
    if flashes * 1000 <= MAX_FLASHES_PER_SECOND * u64::from(motion.period_ms) {
        return None;
    }
    let rate = flashes as f64 * 1000.0 / f64::from(motion.period_ms);
    Some(format!(
        "the loop flashes {flashes} times in its {} ms period, {rate:.1} a second; \
         the cap is {MAX_FLASHES_PER_SECOND}",
        motion.period_ms
    ))
}

/// The turns in one period of `keyframes`, walked as a ring. A loop of fewer
/// than two keyframes has no step, and so no turn.
fn turns_in_one_period(keyframes: &[Keyframe]) -> usize {
    let count = keyframes.len();
    if count < 2 {
        return 0;
    }
    (0..count)
        .filter(|&index| {
            is_flash(
                keyframes[(index + count - 1) % count],
                keyframes[index],
                keyframes[(index + 1) % count],
            )
        })
        .count()
}

fn is_flash(a: Keyframe, b: Keyframe, c: Keyframe) -> bool {
    let out = b.opacity - a.opacity;
    let back = c.opacity - b.opacity;
    out.abs() >= MIN_FLASH_STEP - STEP_TOLERANCE
        && back.abs() >= MIN_FLASH_STEP - STEP_TOLERANCE
        && out.signum() != back.signum()
}

fn report(findings: &mut Vec<Finding>, file: &str, path: &str, rule: &str, message: String) {
    findings.push(Finding::new(
        file,
        Location::Field(path.to_string()),
        rule,
        message,
    ));
}
