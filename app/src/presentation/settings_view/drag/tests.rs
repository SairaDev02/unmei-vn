//! The Settings view's drag tests, beside the state they arm.

use super::*;
use crate::drawer::{MonitorControl, MonitorView};
use crate::input::InputQuarantine;
use crate::tests::{
    activate_control, boot_widget_input_app, drawer_open, drawer_view, enter_play_with_input,
    press_key, step_input, tap_key, temp_pack,
};
use bevy::window::PrimaryWindow;

/// Window focus loss cancels the quarantine and the drag, preserves the
/// drawer and adds no pause. Production cannot start a drag while the
/// drawer holds the frame, so the owner-local test arms the storage
/// directly — the same defensive path the focus-loss handler guards.
#[test]
fn window_focus_loss_cancels_transients_and_keeps_the_view() {
    let pack = temp_pack();
    let (mut app, _window) = boot_widget_input_app(pack.path().join("pack"));
    enter_play_with_input(&mut app);
    tap_key(&mut app, KeyCode::F3);
    activate_control(&mut app, MonitorControl::ShowEverything);
    assert_eq!(drawer_view(&app), MonitorView::Everything);
    // `X` is not a Monitor activation key, so it only exercises the quarantine.
    press_key(&mut app, KeyCode::KeyX);
    step_input(&mut app);
    assert!(
        !app.world()
            .get_resource::<InputQuarantine>()
            .expect("quarantine")
            .is_empty(),
        "the held key was quarantined"
    );
    app.world_mut().resource_mut::<SettingsDrag>().0 = Some(TrackControl::TextSpeed);

    {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = query.single_mut(app.world_mut()).expect("the window");
        window.focused = false;
    }
    step_input(&mut app);

    assert!(
        app.world()
            .get_resource::<InputQuarantine>()
            .expect("quarantine")
            .is_empty(),
        "focus loss cleared the quarantine"
    );
    assert_eq!(
        app.world().resource::<SettingsDrag>().control(),
        None,
        "focus loss cancelled the drag"
    );
    assert!(drawer_open(&app), "focus loss never closes Monitor");
    assert_eq!(
        drawer_view(&app),
        MonitorView::Everything,
        "the filter choice survived"
    );
}
