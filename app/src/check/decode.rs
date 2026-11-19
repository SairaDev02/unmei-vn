//! The check's decoding rule (§5): every referenced asset file that exists is
//! loaded through the player's own loaders, in an app with no window and no
//! audio device, and one that fails is one `pack.asset.decode` finding at that
//! path's first reference.
//!
//! Existence is [`Pack::missing_asset_findings`]' rule, so the references are
//! filtered on it before any load and a missing file is never a decode finding
//! too. The loads go through [`AssetWatch`], play's one load path, so the
//! image settings, the audio decoder and the loader's error text are play's own
//! rather than a second, check-only load.
//!
//! [`Pack::missing_asset_findings`]: Pack::missing_asset_findings

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use unmei_engine::finding::{rules, Finding};
use unmei_engine::{AssetKind, AssetReference, Pack};

use crate::composition::check_app;
use crate::failure::FailureRow;
use crate::presentation::assets::AssetWatch;

/// The most the check waits for the watched loads, the same budget play's test
/// pumps give them. A load still running at the deadline is reported as
/// nothing: the check reports what the loaders said, never an invented failure.
const SETTLE_BUDGET: Duration = Duration::from_secs(10);

/// One `pack.asset.decode` finding per distinct referenced file that exists and
/// cannot be decoded, at its first reference (§5, rule 11).
pub(super) fn findings(pack: &Pack) -> Vec<Finding> {
    let references = loadable_references(pack);
    if references.is_empty() {
        return Vec::new();
    }
    let failures = load_and_settle(&pack.root, &references);
    references
        .iter()
        .filter_map(|reference| {
            let (_, error) = failures.iter().find(|(path, _)| *path == reference.path)?;
            let message = format!("{} cannot be decoded: {error}", reference.path);
            Some(reference_finding(reference, message))
        })
        .collect()
}

/// The first reference to each distinct referenced path that is a file on
/// disk, in [`Pack::asset_references`] order — so a path reported twice would
/// be reported at the reference §3 puts first.
fn loadable_references(pack: &Pack) -> Vec<AssetReference> {
    let mut seen = HashSet::new();
    pack.asset_references()
        .into_iter()
        .filter(|reference| seen.insert(reference.path.clone()))
        // The same file `Pack::missing_asset_findings` consults: one that is
        // not there is that rule's finding.
        .filter(|reference| pack.root.join(&reference.path).is_file())
        .collect()
}

/// Load every reference once in a fresh check app, then drive it until the
/// watch drains. The result is `(asset path, loader error)` for each load that
/// failed.
fn load_and_settle(pack_root: &Path, references: &[AssetReference]) -> Vec<(String, String)> {
    let mut app = check_app(pack_root);
    let server = app.world().resource::<AssetServer>().clone();
    {
        let mut watch = app.world_mut().resource_mut::<AssetWatch>();
        for reference in references {
            load(&mut watch, &server, reference);
        }
    }
    settle_all(&mut app)
}

/// The watch entry point for a reference's media, with the play row its kind
/// loads under. The row names a player-facing surface the check does not have,
/// so [`AssetWatch::settle`]'s row is dropped and only its error is read.
fn load(watch: &mut AssetWatch, server: &AssetServer, reference: &AssetReference) {
    let path = reference.path.clone();
    match reference.kind {
        AssetKind::Background => {
            watch.watch_image(server, FailureRow::BackgroundAsset, path);
        }
        AssetKind::Sprite => {
            watch.watch_image(server, FailureRow::SpriteAsset, path);
        }
        AssetKind::Sound => {
            watch.watch_audio(server, FailureRow::BgmAsset, path);
        }
    }
}

/// `app.update()` until every watched load has settled, collecting each failure
/// as it arrives. The loop stops at [`SETTLE_BUDGET`] so a load that never
/// settles delays the check instead of hanging it.
fn settle_all(app: &mut App) -> Vec<(String, String)> {
    let deadline = Instant::now() + SETTLE_BUDGET;
    let mut failed = Vec::new();
    let mut spins = 0u32;
    loop {
        let server = app.world().resource::<AssetServer>().clone();
        failed.extend(
            app.world_mut()
                .resource_mut::<AssetWatch>()
                .settle(&server)
                .into_iter()
                .map(|(_, path, error)| (path, error)),
        );
        if app.world().resource::<AssetWatch>().is_empty() || Instant::now() >= deadline {
            return failed;
        }
        app.update();
        spins += 1;
        if spins.is_multiple_of(64) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// A finding at the reference's own file and line. A manifest reference has no
/// line, so it names its file as a whole (§3).
fn reference_finding(reference: &AssetReference, message: String) -> Finding {
    match reference.line {
        Some(line) => Finding::at_line(&reference.file, line, rules::PACK_ASSET_DECODE, message),
        None => Finding::new(&reference.file, "", rules::PACK_ASSET_DECODE, message),
    }
}
