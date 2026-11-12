//! The defensive-hold plan.

use crate::prelude::*;

pub(crate) fn hold_plan() -> Vec<Step> {
    vec![
        Step::new("boot: the fixture pack loads and Monitor is closed")
            .settle(12)
            .check(check_title_up),
        Step::new("hold: enter play and engage a defensive hold")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Enter))
            .at(10, |world, _harness| {
                world.resource_mut::<DefensiveHold>().0 = true
            })
            .settle(16)
            .check(|world, harness| {
                harness.report.check(
                    "the defensive hold is engaged",
                    world.resource::<DefensiveHold>().0,
                );
            }),
        Step::new("hold: F3 opens over the held runner")
            .at(0, |world, harness| {
                open_drawer(world, harness);
                note_wait(world, harness);
            })
            .shot(6, "20-defensive-hold")
            .settle(10)
            .check(|world, harness| {
                harness.report.check(
                    "the defensive hold is engaged under the drawer",
                    world.resource::<DefensiveHold>().0,
                );
                check_panel(world, harness, "hold");
                check_hud_unchanged(world, harness);
            }),
        Step::new("hold: Space under Monitor never moves the held runner")
            .at(0, |world, harness| tap_key(world, harness, KeyCode::Space))
            .settle(8)
            .check(check_recorded_wait),
        Step::new("hold: Esc dismisses Monitor and the hold survives a fresh Space")
            .at(0, close_drawer)
            .at(6, |world, harness| {
                note_wait(world, harness);
                tap_key(world, harness, KeyCode::Space);
            })
            .settle(120)
            .check(|world, harness| {
                harness.report.check(
                    "the runner stayed held after Monitor closed",
                    wait_of(world) == harness.wait_before && world.resource::<DefensiveHold>().0,
                );
            }),
    ]
}
