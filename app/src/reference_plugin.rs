//! The reference plugin: variant E written in the UI plugin format, embedded in
//! the player at build time (UI plugin spec §11, ticket #110).
//!
//! The plugin is authored as plain repository files under `app/reference-plugin/`
//! so it is readable, reviewable and exportable like a pack's `ui/` folder. This
//! module is that folder's build-time copy: [`FILES`] names every file, and
//! [`source`] hands the engine the in-memory source the embedded bundle and the
//! export both load through, so the reference look and a pack's plugin meet the
//! same rules (spec §8, R24).
//!
//! The copy is not a second owner of the files. `app/tests/ui_plugin_reference.rs`
//! compares [`FILES`] with the folder on disk, path for path and byte for byte,
//! so an edit that skips either side fails the test rather than drifting.
//!
//! The player draws from these trees too: the renderer takes the surface it is
//! drawing from the plugin in force, and the reference plugin's tree stands in
//! for every surface a pack's plugin leaves alone. The confirm is the first
//! surface drawn this way; the rest replace their hand-written trees ticket by
//! ticket, and each one deletes the tree it replaced.

use unmei_engine::ui::MemorySource;

/// Every file of the reference plugin as `(path, text)`, with paths relative to
/// the plugin folder exactly as a pack's `ui/` folder lists them.
///
/// The order is the folder's byte order, which is the order
/// [`MemorySource`] sorts to; keeping it here makes a diff of this list read
/// like a diff of the folder.
pub const FILES: &[(&str, &str)] = &[
    ("plugin.ron", include_str!("../reference-plugin/plugin.ron")),
    (
        "surfaces/confirm.ron",
        include_str!("../reference-plugin/surfaces/confirm.ron"),
    ),
    (
        "surfaces/contents.ron",
        include_str!("../reference-plugin/surfaces/contents.ron"),
    ),
    (
        "surfaces/gallery.ron",
        include_str!("../reference-plugin/surfaces/gallery.ron"),
    ),
    (
        "surfaces/legacy-picker.ron",
        include_str!("../reference-plugin/surfaces/legacy-picker.ron"),
    ),
    (
        "surfaces/play.ron",
        include_str!("../reference-plugin/surfaces/play.ron"),
    ),
    (
        "surfaces/settings.ron",
        include_str!("../reference-plugin/surfaces/settings.ron"),
    ),
    (
        "surfaces/still.ron",
        include_str!("../reference-plugin/surfaces/still.ron"),
    ),
    (
        "surfaces/timeline.ron",
        include_str!("../reference-plugin/surfaces/timeline.ron"),
    ),
    (
        "surfaces/title.ron",
        include_str!("../reference-plugin/surfaces/title.ron"),
    ),
    (
        "surfaces/title-settings.ron",
        include_str!("../reference-plugin/surfaces/title-settings.ron"),
    ),
    ("tokens.ron", include_str!("../reference-plugin/tokens.ron")),
];

/// The reference plugin as the engine's in-memory source, ready for
/// [`unmei_engine::ui::load_from`].
pub fn source() -> MemorySource {
    MemorySource::from_text(FILES)
}
