//! Real-Bevy rendered smoke for the Monitor drawer.
//!
//! This opens the shipping [`UnmeiPlugin`] on a real window and renderer at the
//! 1280×720 reference size, emits real diagnostics into the dev capture (40
//! numbered events with some long wrapped bodies, plus short, long-message and
//! long-target fixtures), presses F3 through the real input path, then measures
//! the laid-out drawer: the panel rect (`x 24–1256`, `y 212–572`), the
//! header/log/status split, the row grid, whether long text wraps inside its
//! column, and whether the underlying HUD moved. It clicks **Show everything**
//! through the real cursor/hit-test path, then walks the older-line reader with
//! a real `MouseWheel` over the log body, a fresh PgUp, a filter change while
//! reading and a real scrollbar press-drag-release to the bottom — asserting on
//! the status footer and the laid-out top visible row. Finally it pages to a
//! mid-row anchor, goes borderless fullscreen through the shipping Settings
//! toggle, and proves the same event stayed on top through the rewrap with the
//! fixed-px chrome unchanged.
//!
//! Every recorded reading step compares the reader's **own stored anchor** (the
//! deliberate reading position, [`MonitorPanel::anchor`]) with the **drawn**
//! top-visible row (the `MonitorRow` admission identity of the row the layout
//! actually put on top). That oracle is stable in a slow, software-rendered
//! environment: a paging key is held for exactly one frame, so the drawer's held
//! key repeat cadence (`PAGE_REPEAT_SECONDS`) has no frame in which it could add
//! an unaccounted page, and a step is measured one frame after it applied,
//! because the drawn rows only reflect the reader's state after the frame that
//! synced them has ended. An arrival is admitted under the held anchor before
//! the rewrap, so the walk also covers the row-set change once suspected of
//! moving the reader's anchor, and the whole walk is cadence-independent — it
//! was re-run at 150 ms and 400 ms per frame, past the CI ~130 ms frames that
//! exposed the earlier harness.
//!
//! Set `MONITOR_SMOKE_FRAME_MS` (e.g. `150`, the CI Xvfb cadence) to delay every
//! frame and check that independence on a slow machine:
//!
//! ```sh
//! MONITOR_SMOKE_FRAME_MS=150 cargo run -p unmei-app --example monitor_smoke
//! ```
//!
//! It is the production geometry, hit-testing and reading check the browser
//! prototype cannot give. Run it from the workspace root (a display/GPU is
//! required):
//!
//! ```sh
//! cargo run -p unmei-app --example monitor_smoke
//! ```
//!
//! It exits non-zero when a reference number or a reader outcome drifts. The
//! screenshot-based human walk over every surface remains the human rendered
//! gate, which this walk cannot replace.
//!
//! The harness deliberately walks the UI tree by shape (stacking index, header/
//! body/footer sizes, control labels, the four-column row grid) instead of
//! naming the drawer's crate-internal component types. The only drawer types it
//! names are the public reading seam the walk exists to check: the reader's
//! stored anchor and the drawn row's admission identity.

mod events;
mod hud;
mod prelude;
mod probe;
mod query;
mod report;
mod state;

use crate::prelude::*;

pub(crate) fn main() {
    let pack_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
    let pack_root = pack_root.canonicalize().expect("the demo pack exists");

    let mut app = App::new();
    app.insert_resource(RuntimeConfig::new(pack_root.clone(), None).with_dev_mode(true))
        .add_plugins(
            DefaultPlugins
                .set(LogPlugin {
                    custom_layer: unmei_app::failure::app_log_layer,
                    ..default()
                })
                .set(bevy::asset::AssetPlugin {
                    file_path: pack_root.join("assets").to_string_lossy().into_owned(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        resolution: WindowResolution::new(1280, 720)
                            .with_scale_factor_override(1.0),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(bevy_kira_audio::AudioPlugin)
        .add_plugins(UnmeiPlugin)
        .add_systems(Update, probe::probe);

    app.run();
}
