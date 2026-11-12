//! The Settings-panel scroll and focus steps.

use crate::prelude::*;

pub(crate) fn settings_scroll_body(world: &mut World) -> Option<Entity> {
    let mut cursor = control_entity(world, MenuAction::BackToSlots)?;
    loop {
        let parent = world.get::<ChildOf>(cursor)?.parent();
        if world
            .get::<bevy::ui::Node>(parent)
            .is_some_and(|node| node.overflow.y == bevy::ui::OverflowAxis::Scroll)
        {
            return Some(parent);
        }
        cursor = parent;
    }
}

/// A control's laid-out top-left and size, in physical pixels.
pub(crate) fn control_rect(world: &mut World, action: MenuAction) -> Option<(Vec2, Vec2)> {
    let mut query = world.query::<(Entity, &MenuAction, &ComputedNode, &UiGlobalTransform)>();
    query
        .iter(world)
        .find(|(_, candidate, _, _)| **candidate == action)
        .map(|(_, _, node, transform)| (transform.translation - node.size() / 2.0, node.size()))
}

pub(crate) fn control_entity(world: &mut World, action: MenuAction) -> Option<Entity> {
    let mut query = world.query::<(Entity, &MenuAction)>();
    query
        .iter(world)
        .find(|(_, candidate)| **candidate == action)
        .map(|(entity, _)| entity)
}

/// Open the title door's Settings view from the title.
pub(crate) fn open_title_settings(world: &mut World, harness: &mut Harness) {
    world.resource_mut::<MenuState>().focus = MenuFocus::TitleSettings;
    tap_key(world, harness, KeyCode::Enter);
}

/// Park the real cursor on the Settings viewport, wherever it currently sits.
pub(crate) fn park_cursor_over_settings(world: &mut World, _harness: &mut Harness) {
    let Some(body) = settings_scroll_body(world) else {
        return;
    };
    let center = *world
        .get::<UiGlobalTransform>(body)
        .expect("the body is laid out");
    crate::harness::move_cursor(world, center.translation);
}

pub(crate) fn wheel_settings_to_bottom(world: &mut World, _harness: &mut Harness) {
    wheel(world, -20.0);
}

pub(crate) fn wheel_settings_to_top(world: &mut World, _harness: &mut Harness) {
    wheel(world, 20.0);
}

pub(crate) fn focus_clear_key(world: &mut World, _harness: &mut Harness) {
    world.resource_mut::<MenuState>().focus = MenuFocus::ClearKey;
}

/// The wheel reached the Settings panel's lower content: the viewport is
/// bounded with content taller than it, and the door-aware back control that the
/// 1280×720 clipping hid is inside the viewport at the bottom.
pub(crate) fn check_settings_wheel_reaches_back(world: &mut World, harness: &mut Harness) {
    let Some(body) = settings_scroll_body(world) else {
        harness.report.fail("the Settings panel is a scroll body");
        return;
    };
    let computed = *world
        .get::<ComputedNode>(body)
        .expect("the body is laid out");
    let size = computed.size();
    let content = computed.content_size();
    harness.report.check(
        format!(
            "the Settings viewport is bounded with content that overflows ({size:?} / {content:?})"
        ),
        content.y > size.y,
    );
    let scroll = world
        .get::<bevy::ui::ScrollPosition>(body)
        .map_or(0.0, |position| position.0.y);
    harness.report.check(
        format!("the wheel scrolled the panel ({scroll})"),
        scroll > 0.0,
    );
    let body_top = world
        .get::<UiGlobalTransform>(body)
        .expect("the body is laid out")
        .translation
        .y
        - size.y / 2.0;
    let (position, control) =
        control_rect(world, MenuAction::BackToSlots).expect("the back control is rendered");
    // The control's offset inside the scrolled content, from the body's own
    // top edge: its laid-out screen place plus the scroll the body applied.
    let top = position.y - body_top + scroll;
    let bottom = top + control.y;
    harness.report.check(
        format!("the back control started below the fold ({bottom} / {size:?})"),
        bottom > size.y,
    );
    harness.report.check(
        format!(
            "the back control is inside the viewport after wheeling ({top}..{bottom} in {scroll}..{})",
            scroll + size.y
        ),
        top >= scroll - 1.0 && bottom <= scroll + size.y + 1.0,
    );
}

/// The focus reveal: from the top of the body, focusing `Clear key` scrolls it
/// fully into view and its FOCUS ring is drawn while an unfocused control has
/// none (`docs/title-and-menu.md` § Decisions settings §6). The keyboard walk
/// itself is covered headlessly; this step proves the rendered geometry.
pub(crate) fn check_settings_focus_reveals_clear_key(world: &mut World, harness: &mut Harness) {
    let Some(body) = settings_scroll_body(world) else {
        harness.report.fail("the Settings panel is a scroll body");
        return;
    };
    let size = world
        .get::<ComputedNode>(body)
        .expect("the body is laid out")
        .size();
    let scroll = world
        .get::<bevy::ui::ScrollPosition>(body)
        .map_or(0.0, |position| position.0.y);
    let body_top = world
        .get::<UiGlobalTransform>(body)
        .expect("the body is laid out")
        .translation
        .y
        - size.y / 2.0;
    let (position, control) =
        control_rect(world, MenuAction::ClearKey).expect("the Clear key control is rendered");
    let top = position.y - body_top + scroll;
    let bottom = top + control.y;
    harness.report.check(
        format!("focusing Clear key scrolled the body ({scroll})"),
        scroll > 0.0,
    );
    harness.report.check(
        format!(
            "Clear key is fully inside the viewport ({top}..{bottom} in {scroll}..{})",
            scroll + size.y
        ),
        top >= scroll - 1.0 && bottom <= scroll + size.y + 1.0,
    );
    let focused_border = control_entity(world, MenuAction::ClearKey)
        .and_then(|entity| world.get::<BorderColor>(entity))
        .map(|border| border.top);
    let unfocused_border = control_entity(world, MenuAction::CopyKeyPath)
        .and_then(|entity| world.get::<BorderColor>(entity))
        .map(|border| border.top);
    harness.report.check(
        format!("the FOCUS ring is drawn on Clear key ({focused_border:?})"),
        focused_border.is_some_and(|color| color != Color::NONE),
    );
    harness.report.check(
        format!("an unfocused Settings control draws no ring ({unfocused_border:?})"),
        unfocused_border == Some(Color::NONE),
    );
}

// ---------------------------------------------------------------------------
// Common acts
// ---------------------------------------------------------------------------
