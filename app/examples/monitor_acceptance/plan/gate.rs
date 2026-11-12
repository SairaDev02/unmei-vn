//! The dev-mode gate plan: the build's effective value, F3, F5 and F9.

use crate::prelude::*;

pub(crate) fn gate_plan() -> Vec<Step> {
    vec![
        Step::new("gate: the effective dev mode follows cfg!(debug_assertions)")
            .settle(12)
            .check(|world, harness| {
                let dev = world.resource::<RuntimeConfig>().dev_mode;
                println!(
                    "  build: debug_assertions={} dev_mode={dev}",
                    cfg!(debug_assertions)
                );
                match harness.explicit_dev {
                    Some(forced) => harness.report.check(
                        format!("the explicit construction override is honored ({forced})"),
                        dev == forced,
                    ),
                    None => harness.report.check(
                        "the effective dev mode is the build's debug-assertions default",
                        dev == cfg!(debug_assertions),
                    ),
                }
                let menu = world.resource::<MenuState>();
                harness
                    .report
                    .check("the title is up", menu.open && menu.view == MenuView::Title);
                // Off really installs no Monitor-only plumbing, while the
                // existing session-log sink stays live.
                harness.report.check(
                    format!("the Monitor capture layer matches the gate ({dev})"),
                    world
                        .get_resource::<unmei_app::monitor::MonitorCapture>()
                        .is_some()
                        == dev,
                );
                let root = world.resource::<RuntimeConfig>().pack_root.clone();
                let log = root
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join(unmei_app::failure::session_log_file_name(&root));
                let live = std::fs::metadata(&log).is_ok_and(|metadata| metadata.len() > 0);
                harness.report.check(
                    format!("the session log sink is live ({})", log.display()),
                    live,
                );
            }),
        Step::new("gate: F3 installs or ignores the drawer accordingly")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::F3))
            .settle(8)
            .check(|world, harness| {
                let dev = world.resource::<RuntimeConfig>().dev_mode;
                let open = crate::harness::snap(world).drawer_open();
                harness.report.check(
                    format!(
                        "F3 {} the drawer (dev_mode={dev})",
                        if dev { "opened" } else { "ignored" }
                    ),
                    open == dev,
                );
            }),
        Step::new("gate: enter play and exercise the F5/F9 shortcuts")
            .at(0, |world, harness| {
                if crate::harness::snap(world).drawer_open() {
                    close_drawer(world, harness);
                }
            })
            .at(6, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .settle(14)
            .check(|world, harness| {
                harness.report.check(
                    "New Game reached live play",
                    !world.resource::<MenuState>().open
                        && world.get_resource::<Runtime>().is_some(),
                );
            })
            .settle(14),
        Step::new("gate: F5 writes only when the gate is on")
            .at(0, |world, harness| {
                if let Some(runtime) = world.get_resource::<Runtime>() {
                    let path = unmei_engine::save::save_path_for(
                        runtime.runner.pack(),
                        unmei_engine::Slot::Manual(1),
                    );
                    println!("  manual slot 1: {}", path.display());
                    let _ = std::fs::remove_file(&path);
                }
                tap_key(world, harness, KeyCode::F5);
            })
            .settle(10)
            .check(|world, harness| {
                let dev = world.resource::<RuntimeConfig>().dev_mode;
                let Some(runtime) = world.get_resource::<Runtime>() else {
                    harness
                        .report
                        .fail("live play exists for the F5 shortcut check");
                    return;
                };
                let path = unmei_engine::save::save_path_for(
                    runtime.runner.pack(),
                    unmei_engine::Slot::Manual(1),
                );
                harness.report.check(
                    format!(
                        "F5 {} manual slot 1 (dev_mode={dev})",
                        if dev { "wrote" } else { "did not write" }
                    ),
                    path.exists() == dev,
                );
            }),
        Step::new("gate: a fresh Space advances after the shortcut attempt")
            .at(0, |world, harness| {
                harness.wait_before = wait_of(world);
                tap_key(world, harness, KeyCode::Space);
            })
            .settle(12)
            .check(|world, harness| {
                let now = wait_of(world);
                harness.report.check(
                    format!("a fresh Space advanced the runner ({now:?})"),
                    now != harness.wait_before,
                );
                harness.after_advance = now;
            }),
        Step::new("gate: F9 restores only when the gate is on")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::F9))
            .settle(20)
            .check(|world, harness| {
                let dev = world.resource::<RuntimeConfig>().dev_mode;
                let now = wait_of(world);
                let expected = if dev {
                    harness.wait_before.clone()
                } else {
                    harness.after_advance.clone()
                };
                harness.report.check(
                    format!(
                        "F9 {} the manual slot (dev_mode={dev})",
                        if dev { "loaded" } else { "ignored" }
                    ),
                    now == expected,
                );
            }),
    ]
}
