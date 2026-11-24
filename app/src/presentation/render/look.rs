//! The look in force: the plugin the renderer draws, its shipped fonts and
//! images as asset handles, and its copy overrides (spec §11).
//!
//! Boot already loads the pack's `ui/` folder — [`load_pack`] carries the
//! outcome — so this module only chooses what to draw with. A pack with no
//! plugin, or one that failed, draws in the reference look; the reference plugin
//! is the embedded copy under `app/reference-plugin/`, loaded through the same
//! [`ui::load_from`] a pack's folder meets.
//!
//! Fonts and images are the one thing the engine hands over as bytes rather than
//! as a path: [`Source::read`] is the confined reader, and the handles are made
//! here so nothing downstream re-opens the folder.
//!
//! [`load_pack`]: unmei_engine::load_pack

use std::collections::BTreeMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use unmei_engine::ui::{self, DirSource, Node, PluginOutcome, Source, UiPlugin};

use crate::core::PackPlugin;
use crate::reference_plugin;
use crate::Runtime;

use super::tree::Words;

/// The plugin a boot draws with, its fonts and images, and its copy overrides.
#[derive(Resource)]
pub(crate) struct UiLook {
    /// The plugin in force, which restyles some or all of the ten surfaces.
    plugin: UiPlugin,
    /// The reference look, whose tree stands in for every surface the plugin
    /// leaves alone (spec §11, rule 18).
    reference: UiPlugin,
    fonts: BTreeMap<(String, u16), Handle<Font>>,
    images: BTreeMap<String, (Handle<Image>, Vec2)>,
}

impl UiLook {
    /// The resolved tree of `surface`: the plugin's own, or the reference look's
    /// for a surface the plugin does not restyle (spec §11, rule 18).
    pub(crate) fn surface(&self, surface: &str) -> Option<&Node> {
        self.plugin
            .surfaces
            .get(surface)
            .or_else(|| self.reference.surfaces.get(surface))
    }

    /// The shipped face `family` at `weight`, when the plugin ships one.
    pub(crate) fn shipped_face(&self, family: &str, weight: u16) -> Option<Handle<Font>> {
        self.fonts.get(&(family.to_string(), weight)).cloned()
    }

    /// The image `file` names and its own pixel size, when the plugin declares
    /// and ships it.
    pub(crate) fn image(&self, file: &str) -> Option<(Handle<Image>, Vec2)> {
        self.images.get(file).cloned()
    }

    /// The plugin's words for the copy key `id`: its override when the manifest
    /// holds one, else the copy table's own text (spec §5).
    pub(crate) fn copy(&self, id: &str) -> &str {
        self.plugin
            .manifest
            .copy
            .get(id)
            .map(String::as_str)
            .unwrap_or_else(|| unmei_engine::copy::text(id))
    }

    /// The words `word` names, with each placeholder filled.
    pub(crate) fn words(&self, word: &Words) -> String {
        let values: Vec<(&str, &str)> = word
            .values
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        unmei_engine::copy::fill(self.copy(word.key), &values)
    }
}

/// Pick the look a boot draws with, and make its fonts and images, once.
///
/// It waits for the pack — a pack that failed to load shows the error screen and
/// never draws a surface, and the plugin outcome arrives with the load — and
/// runs until the resource is in place, so the frame a surface first draws on
/// already has its look.
pub(crate) fn load_look(
    mut commands: Commands,
    runtime: Option<Res<Runtime>>,
    pack: Option<Res<PackPlugin>>,
    fonts: Option<ResMut<Assets<Font>>>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let (Some(runtime), Some(pack)) = (runtime, pack) else {
        return;
    };
    let (plugin, source) = choose(&runtime.pack_root, &pack.0);
    let reference = match ui::load_from(&reference_plugin::source()) {
        PluginOutcome::Loaded(plugin) => plugin,
        _ => unreachable!("the embedded reference plugin passes every rule"),
    };
    let mut shipped = BTreeMap::new();
    if let Some(mut assets) = fonts {
        shipped = shipped_fonts(&plugin, source.as_ref(), &mut assets);
    }
    let mut decoded = BTreeMap::new();
    if let Some(mut assets) = images {
        decoded = shipped_images(&plugin, source.as_ref(), &mut assets);
    }
    commands.insert_resource(UiLook {
        plugin,
        reference,
        fonts: shipped,
        images: decoded,
    });
}

/// The plugin to draw with and the source its files come from: the pack's own
/// when it loaded one, else the embedded reference plugin (spec §11, rule 17).
fn choose(pack_root: &std::path::Path, pack: &PluginOutcome) -> (UiPlugin, Box<dyn Source>) {
    if let PluginOutcome::Loaded(plugin) = pack {
        if let Ok(source) = DirSource::open(&pack_root.join("ui")) {
            return (plugin.clone(), Box::new(source));
        }
    }
    match ui::load_from(&reference_plugin::source()) {
        PluginOutcome::Loaded(plugin) => (plugin, Box::new(reference_plugin::source())),
        _ => unreachable!("the embedded reference plugin passes every rule"),
    }
}

/// Every declared font the plugin ships, as an asset handle. A file that cannot
/// be read or parsed leaves its face out; the renderer then draws that face with
/// an embedded one rather than panicking mid-frame.
fn shipped_fonts(
    plugin: &UiPlugin,
    source: &dyn Source,
    fonts: &mut Assets<Font>,
) -> BTreeMap<(String, u16), Handle<Font>> {
    let mut handles = BTreeMap::new();
    for declaration in &plugin.manifest.fonts {
        let face = (declaration.family.clone(), declaration.weight);
        match source.read(&declaration.file) {
            Ok(bytes) => {
                handles.insert(face, fonts.add(Font::from_bytes(bytes)));
            }
            Err(message) => warn!(
                "the plugin font `{}` is not drawn: {message}",
                declaration.file
            ),
        }
    }
    handles
}

/// Every image the plugin declares, as an asset handle, decoded from its bytes.
fn shipped_images(
    plugin: &UiPlugin,
    source: &dyn Source,
    images: &mut Assets<Image>,
) -> BTreeMap<String, (Handle<Image>, Vec2)> {
    let mut handles = BTreeMap::new();
    for declaration in &plugin.manifest.images {
        let bytes = match source.read(&declaration.file) {
            Ok(bytes) => bytes,
            Err(message) => {
                warn!(
                    "the plugin image `{}` is not drawn: {message}",
                    declaration.file
                );
                continue;
            }
        };
        match decode(&declaration.file, &bytes) {
            Ok(image) => {
                let size = image.size_f32();
                handles.insert(declaration.file.clone(), (images.add(image), size));
            }
            Err(message) => warn!(
                "the plugin image `{}` is not drawn: {message}",
                declaration.file
            ),
        }
    }
    handles
}

/// One image file as a texture. The format accepts five extensions (spec §2);
/// one this build cannot decode is logged and skipped rather than drawn as a
/// wrong-size texture.
fn decode(file: &str, bytes: &[u8]) -> Result<Image, String> {
    let extension = file.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    Image::from_buffer(
        bytes,
        ImageType::Extension(&extension),
        CompressedImageFormats::all(),
        true,
        ImageSampler::Default,
        RenderAssetUsages::RENDER_WORLD,
    )
    .map_err(|error| error.to_string())
}
