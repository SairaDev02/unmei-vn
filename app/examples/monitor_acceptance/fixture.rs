//! The controlled real-Bevy acceptance fixtures for the Monitor walk.
//!
//! Two fixture packs are generated under the gitignored `target/` directory:
//!
//! - [`write_pack`] — the healthy pack every surface walk runs on. It declares a
//!   three-scene order (so the Timeline has cards), a background and two sprites
//!   (so the Gallery has a grid and a still) and one `decide` point whose NPC
//!   answers through the walk's own gated resolver — **no live OpenRouter, no
//!   credentials and no network**.
//! - [`write_broken_pack`] — 25 unresolvable `jump`s plus an escaped one, so
//!   the startup error screen shows the capped 20-line list with its `… and 6
//!   more` pointer while Monitor retains all 26 for an in-window read.
//!
//! Generating them at run time keeps the repository free of acceptance-only
//! content while making the walk reproducible from the command line alone.

use std::path::{Path, PathBuf};

use bevy::prelude::*;

/// The acceptance working directory (gitignored, under `target/`).
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/monitor-acceptance")
}

/// Delete the pack-sibling files a previous acceptance run left behind: saves,
/// settings, the read log and the session log. Without this, a second run's
/// title would offer Continue from the first run's autosave and the walk would
/// load the end of the fixture script instead of starting a New Game.
///
/// Every target is named explicitly, one path at a time — the pack-sibling
/// naming is stable, and this never sweeps a directory.
pub fn clean_state() {
    for name in ["pack", "broken"] {
        for suffix in [
            "log",
            "read.ron",
            "settings.ron",
            "save.ron",
            "slot1.save.ron",
            "slot2.save.ron",
            "slot3.save.ron",
            "autosave.save.ron",
        ] {
            let path = root().join(format!("{name}.{suffix}"));
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Write (or refresh) the healthy fixture pack and return its root.
pub fn write_pack() -> PathBuf {
    clean_state();
    let pack = root().join("pack");
    write(
        &pack.join("pack.ron"),
        "(\n    name: \"acceptance\",\n    entry: \"entry\",\n    tagline: \"rendered monitor acceptance\",\n    title_backdrop: \"gate\",\n    scenes: [\"entry\", \"second\", \"third\"],\n)\n",
    );
    write(
        &pack.join("state.ron"),
        "(\n    affinity: \"low\",\n    met: false,\n)\n",
    );
    write(
        &pack.join("npcs/ace.ron"),
        "(\n    id: \"ace\",\n    name: \"Ace\",\n    model: \"typesafe/jev-1.13\",\n    decisions: {\n        \"greet\": (\n            instructions: \"How does Ace greet the player at the gate?\",\n            state_keys: [\"affinity\", \"met\"],\n            criteria: {\n                \"warm\": \"An affectionate, open greeting for a trusted player.\",\n                \"cold\": \"A distant, guarded greeting.\",\n                \"none\": \"Neither description fits.\",\n            },\n            fallback: \"cold\",\n        ),\n    },\n)\n",
    );
    write(
        &pack.join("scenes/entry.unmei"),
        "# Accept/entry — the walk's first scene.\n\
         bg gate\n\
         play theme\n\
         \n\
         Narrator: The acceptance fixture opens on a controlled say-line.\n\
         \n\
         choice\n  \"Speak first\" -> spoke\n  \"Wait\" -> waited\nend\n\
         \n\
         label spoke\nset met = true\nset affinity = \"high\"\nNarrator: The spoke branch is on screen.\njump decision\n\
         \n\
         label waited\nset met = false\nNarrator: The waited branch is on screen.\njump decision\n\
         \n\
         label decision\ndecide ace.greet -> reply\n\
         \n\
         label reply.warm\nsprite ace happy\nNarrator: The warm reply lands.\njump second\n\
         \n\
         label reply.cold\nsprite ace neutral\nNarrator: The cold reply lands.\njump second\n\
         \n\
         label reply.none\nNarrator: Neither reply fits.\njump second\n",
    );
    write(
        &pack.join("scenes/second.unmei"),
        "# Accept/second — the timeline's middle card.\n\
         bg gate\n\
         \n\
         Narrator: The second declared scene holds the timeline's middle card.\n\
         \n\
         jump third\n",
    );
    write(
        &pack.join("scenes/third.unmei"),
        "# Accept/third — the declared order's last card.\n\
         Narrator: The declared order ends here.\n",
    );
    for asset in [
        ("backgrounds/school_gate.png", "backgrounds/gate.png"),
        ("sprites/ace/happy.png", "sprites/ace/happy.png"),
        ("sprites/ace/neutral.png", "sprites/ace/neutral.png"),
        ("audio/music_morning.ogg", "audio/theme.ogg"),
        ("audio/sfx_select.ogg", "audio/select.ogg"),
    ] {
        copy_asset(asset.0, &pack.join("assets").join(asset.1));
    }
    pack
}

/// Write (or refresh) the broken fixture pack and return its root.
pub fn write_broken_pack() -> PathBuf {
    clean_state();
    let pack = root().join("broken");
    // Remove a fixture scene a previous run may have left behind, by explicit
    // path (never a directory sweep).
    let _ = std::fs::remove_file(pack.join("scenes/zz_oversized.unmei"));
    write(
        &pack.join("pack.ron"),
        "(\n    name: \"broken-acceptance\",\n    entry: \"entry\",\n)\n",
    );
    write(&pack.join("state.ron"), "(\n    affinity: \"low\",\n)\n");
    write(
        &pack.join("scenes/entry.unmei"),
        "Narrator: The broken fixture still parses.\n",
    );
    // 25 scenes, each with an unresolvable jump: the loader keeps every error,
    // while the screen shows the first 20 plus the pointer to the session log.
    for index in 0..25 {
        write(
            &pack.join(format!("scenes/broken_{index:02}.unmei")),
            &format!("Narrator: broken fixture {index}.\njump nowhere_{index}\n"),
        );
    }
    // One more distinctive startup failure that only the in-window Monitor can
    // show, because the on-screen list caps at 20: an escaped target (a bidi
    // control and a literal backslash-n, which Monitor must escape rather than
    // render as control text). It sorts after `broken_*`, so the numbered
    // messages stay inside the on-screen cap. The whole 26-error aggregate
    // stays under Monitor's 8,192-byte logical-line cap, so every message
    // survives and is readable in-window (the oversized/truncation case is the
    // walk mode's own fixture).
    write(
        &pack.join("scenes/zy_escaped.unmei"),
        &format!("Narrator: escaped fixture.\njump {ESCAPED_BROKEN_MARKER}\u{202e}bidi\\nmarker\n"),
    );
    pack
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the fixture directory is writable");
    }
    std::fs::write(path, contents).expect("the fixture file is writable");
}

fn copy_asset(name: &str, target: &Path) {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("the fixture asset directory is writable");
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/demo-pack/assets")
        .join(name);
    std::fs::copy(&source, target)
        .unwrap_or_else(|error| panic!("copying {}: {error}", source.display()));
}

// ---------------------------------------------------------------------------
// Controlled diagnostics
// ---------------------------------------------------------------------------

/// Real diagnostics emitted through the installed global logger and therefore
/// through the production Monitor capture layer.
pub fn emit_styles() {
    log::info!(target: "unmei::acceptance::short", "the short INFO fixture");
    log::warn!(target: "unmei::acceptance::tone", "the WARN fixture");
    log::error!(target: "unmei::acceptance::tone", "the ERROR fixture");
    // A message-less event: named fields only. Emitted through `tracing`
    // directly, because the `log` facade requires a message literal.
    bevy::log::tracing::info!(
        target: "unmei::acceptance::fields",
        answer = 42,
        detail = "fields without a message"
    );
    // Escapes: real newline/tab/CR, a Unicode line separator, a bidi control and
    // a literal backslash-n that must stay distinguishable from a real newline.
    log::info!(
        target: "unmei::acceptance::escaped",
        "line one\nline two\ttabbed\rcarriage\u{2028}separator\u{202e}bidi \\n literal"
    );
    // Ordinary Unicode stays intact.
    log::info!(target: "unmei::acceptance::unicode", "naïve café · 日本語 · Ωmega · ✨");
    // A long module path that must render whole and wrap inside the target
    // column.
    log::info!(
        target: "unmei::acceptance::a_deliberately_long_module_path_that_does_not_fit_the_target_column::tail_marker",
        "short body"
    );
    log::info!(
        target: "unmei::acceptance::wrapped",
        "{}",
        "A deliberately long wrapped body that must fold inside its flexible message column \
         instead of running past the log body's right edge. "
            .repeat(6)
    );
    // Oversized: past the 8,192-byte escaped cap, so the row collapses with the
    // visible `[truncated]` suffix.
    log::info!(
        target: "unmei::acceptance::oversized",
        "{}",
        "y".repeat(9_000)
    );
    // A target outside the default exact `unmei::` predicate: visible only under
    // Show everything, so a filter walk can prove the two views are distinct.
    log::info!(target: "bevy_acceptance_other", "a non-unmei target for Show everything");
}

/// The numbered overflow fixture: `count` real INFO events, numbered from
/// `start`. The walk keeps each call at or below 99 events so the production
/// 128-event ingestion and 256-slot pending budgets are never overloaded.
pub fn emit_sequence(start: u32, count: u32) {
    for index in 0..count {
        let number = start + index;
        log::info!(target: "unmei::acceptance::seq", "sequence event {number:04}");
    }
}

/// The fixture row the walk anchors on by text. `ESCAPED_FIXTURE` is the
/// *rendered* escape (literal backslash-n), not the source message.
pub const SHORT_FIXTURE: &str = "the short INFO fixture";
pub const WARN_FIXTURE: &str = "the WARN fixture";
pub const ERROR_FIXTURE: &str = "the ERROR fixture";
pub const ESCAPED_FIXTURE: &str = r"line one\nline two";
pub const UNICODE_FIXTURE: &str = "naïve café";
pub const TRUNCATED_SUFFIX: &str = "[truncated]";
pub const TARGET_TAIL: &str = "tail_marker";

/// The broken fixture's startup failures: the 25 numbered unresolvable jumps
/// plus the escaped one. The on-screen list caps at 20; Monitor retains and can
/// read all of them.
pub const BROKEN_ERRORS: usize = 26;
/// The ASCII prefix of the escaped broken target's marker — the bidi control
/// and literal backslash-n that follow it must render escaped.
pub const ESCAPED_BROKEN_MARKER: &str = "nowhere_escaped_";
