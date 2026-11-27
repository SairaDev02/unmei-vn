//! The Settings-panel scroll and focus steps.

use crate::prelude::*;

pub(crate) fn settings_scroll_body(world: &mut World) -> Option<Entity> {
    settings_scroll_body_of(world, MenuAction::BackToSlots)
}

/// The scroll body around the control that opens `action`: the nearest ancestor
/// whose node scrolls.
pub(crate) fn settings_scroll_body_of(world: &mut World, action: MenuAction) -> Option<Entity> {
    let mut cursor = control_entity(world, action)?;
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

pub(crate) fn focus_text_speed(world: &mut World, _harness: &mut Harness) {
    world.resource_mut::<MenuState>().focus = MenuFocus::TextSpeed;
}

/// Every Settings row whose control is laid over its row covers the panel: the
/// stepped row and the three switches ring the row the way the audio rows' rings
/// do, with the label on the row's own line
/// (`docs/title-and-menu.md` § Title layout §5).
pub(crate) fn check_settings_rows_ring_their_panels(world: &mut World, harness: &mut Harness) {
    for action in [
        MenuAction::TextSpeed,
        MenuAction::AudioMute,
        MenuAction::PlayAuto,
        MenuAction::PlayFullscreen,
    ] {
        let Some((origin, size)) = control_rect(world, action) else {
            harness
                .report
                .fail(format!("{action:?}'s control is drawn"));
            continue;
        };
        let Some(row) = control_entity(world, action)
            .and_then(|control| world.get::<ChildOf>(control).map(ChildOf::parent))
        else {
            harness
                .report
                .fail(format!("{action:?}'s control sits in a row"));
            continue;
        };
        let (Some(node), Some(transform)) = (
            world.get::<ComputedNode>(row).copied(),
            world.get::<UiGlobalTransform>(row).copied(),
        ) else {
            harness.report.fail(format!("{action:?}'s row is laid out"));
            continue;
        };
        let border = match world.get::<bevy::ui::Node>(row).map(|node| node.border.top) {
            Some(Val::Px(px)) => px,
            _ => 0.0,
        };
        let row_origin = transform.translation - node.size() / 2.0 + Vec2::splat(border);
        let row_size = node.size() - Vec2::splat(border * 2.0);
        harness.report.check(
            format!(
                "{action:?}'s ring frames its row ({origin:?} {size:?} against {row_origin:?} {row_size:?})"
            ),
            (origin - row_origin).length() < 1.0 && (size - row_size).length() < 1.0,
        );
    }
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

/// The overlay door's viewport reaches its last control: Default look §4 moved
/// the way out to the contents page's left column, so the body ends at `Clear
/// key` and the wheel must bring it fully into view.
pub(crate) fn check_settings_wheel_reaches_last_control(world: &mut World, harness: &mut Harness) {
    let Some(body) = settings_scroll_body_of(world, MenuAction::ClearKey) else {
        harness
            .report
            .fail("the contents page's Settings body is a scroll body");
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
    let back_drawn = control_entity(world, MenuAction::BackToSlots)
        .and_then(|entity| world.get::<Node>(entity))
        .is_some_and(|node| node.display != Display::None);
    harness.report.check(
        "the contents page's Settings body draws no back control",
        !back_drawn,
    );
    let body_top = world
        .get::<UiGlobalTransform>(body)
        .expect("the body is laid out")
        .translation
        .y
        - size.y / 2.0;
    let Some((position, control)) = control_rect(world, MenuAction::ClearKey) else {
        harness.report.fail("Clear key is rendered");
        return;
    };
    let top = position.y - body_top + scroll;
    let bottom = top + control.y;
    harness.report.check(
        format!(
            "Clear key is inside the viewport after wheeling ({top}..{bottom} in {scroll}..{})",
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
