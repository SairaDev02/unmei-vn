//! The real input messages the walk writes and the widget reads it needs.

use crate::prelude::*;

pub(crate) fn tap_key(world: &mut World, harness: &mut Harness, code: KeyCode) {
    crate::harness::press(world, code);
    harness.queued.push(Queued::Key(code));
}

pub(crate) fn release_button(world: &mut World, button: MouseButton) {
    world.write_message(MouseButtonInput {
        button,
        state: ButtonState::Released,
        window: Entity::PLACEHOLDER,
    });
}

pub(crate) fn tap_control(world: &mut World, harness: &mut Harness, label: &str) {
    let snap = crate::harness::snap(world);
    let Some(center) = control_center(&snap, label) else {
        harness
            .report
            .fail(format!("the {label:?} control is rendered and hittable"));
        return;
    };
    crate::harness::move_cursor(world, center);
    world.write_message(MouseButtonInput {
        button: MouseButton::Left,
        state: ButtonState::Pressed,
        window: Entity::PLACEHOLDER,
    });
    harness.queued.push(Queued::Button(MouseButton::Left));
}

/// The physical centre of the rendered text node containing `label` — the real
/// UI hit-test target for a pointer activation.
pub(crate) fn control_center(snap: &Snap, label: &str) -> Option<Vec2> {
    let node = snap
        .nodes
        .iter()
        .filter(|node| {
            snap.text(node.entity)
                .is_some_and(|text| text.contains(label))
        })
        .min_by(|a, b| {
            a.size
                .x
                .partial_cmp(&b.size.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
    Some(node.position + node.size / 2.0)
}

/// Write a real `KeyboardInput` carrying `logical` (not `Unidentified`), so the
/// production `EditableText` widget's selection/navigation arm sees it. The
/// shared app key path deliberately writes an unidentified logical key.
pub(crate) fn send_logical(world: &mut World, logical: Key, code: KeyCode, state: ButtonState) {
    world.write_message(KeyboardInput {
        key_code: code,
        logical_key: logical,
        state,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

/// The `Interaction` on the title's New Game widget, if it is present.
pub(crate) fn new_game_interaction(world: &mut World) -> Option<Interaction> {
    let mut query = world.query::<(&MenuAction, &Interaction)>();
    query
        .iter(world)
        .find(|(action, _)| matches!(action, MenuAction::NewGame))
        .map(|(_, interaction)| *interaction)
}

/// The Settings key field's `(text, caret, selection)` read from its own
/// `parley` editor, or `None` when no field is present.
pub(crate) fn draft_selection(world: &mut World) -> Option<DraftEdit> {
    let mut query =
        world.query_filtered::<&bevy::text::EditableText, With<unmei_app::credentials::KeyField>>();
    query.iter(world).next().map(|field| {
        let selection = field.editor().raw_selection();
        (
            field.value().to_string(),
            selection.focus().index(),
            selection.text_range(),
        )
    })
}

pub(crate) fn wheel(world: &mut World, lines: f32) {
    world.write_message(MouseWheel {
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: lines,
        window: Entity::PLACEHOLDER,
        phase: TouchPhase::Moved,
    });
}

/// Park the real cursor over the log body and roll the wheel.
pub(crate) fn wheel_over_body(world: &mut World, lines: f32) {
    let snap = crate::harness::snap(world);
    if let Some(geometry) = snap.geometry() {
        let x = geometry.x + geometry.width / 2.0;
        let y = geometry.body_top + geometry.body.min(20.0) / 2.0;
        crate::harness::move_cursor(world, Vec2::new(x, y));
    }
    wheel(world, lines);
}

/// Park the real cursor on the scrollbar track at a fraction of its height.
pub(crate) fn drag_scrollbar(world: &mut World, fraction: f32) {
    let snap = crate::harness::snap(world);
    let Some(track) = snap.scrollbar_track() else {
        return;
    };
    let Some(node) = snap.node(track) else {
        return;
    };
    let center =
        node.position + Vec2::new(node.size.x / 2.0, node.size.y * fraction.clamp(0.0, 1.0));
    crate::harness::move_cursor(world, center);
}

// ---------------------------------------------------------------------------
// Settings scroll
// ---------------------------------------------------------------------------

/// One real OS clipboard paste (Ctrl+V) through the widget's own paste path.
pub(crate) fn paste(world: &mut World, harness: &mut Harness) {
    world.write_message(KeyboardInput {
        key_code: KeyCode::ControlLeft,
        logical_key: Key::Control,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
    harness.queued.push(Queued::Key(KeyCode::ControlLeft));
    world.write_message(KeyboardInput {
        key_code: KeyCode::KeyV,
        logical_key: Key::Character("v".into()),
        state: ButtonState::Pressed,
        text: Some("v".into()),
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
    world.write_message(KeyboardInput {
        key_code: KeyCode::KeyV,
        logical_key: Key::Character("v".into()),
        state: ButtonState::Released,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

/// The primary window's entity, for focus messages.
pub(crate) fn primary_window_entity(world: &mut World) -> Entity {
    let mut query = world.query_filtered::<Entity, With<PrimaryWindow>>();
    query.single(world).expect("the primary window entity")
}
