//! Shared presentation support: the embedded interface faces, the shared
//! tones and the cover/contain fit maths every surface draws with.

use bevy::prelude::*;

/// The five interface faces, embedded in the binary. The asset root is the
/// *pack's* `assets/` directory (`app/src/main.rs`), so the app's own faces
/// cannot be loose assets in that source; `include_bytes!` into
/// `Assets<Font>` is the delivery path. One distinct asset per face, held here
/// so the app's text builders name them through the [`TextFaces`] accessors
/// (`docs/title-and-menu.md` § Type system §1).
#[derive(Resource, Debug, Clone)]
pub(crate) struct TextFaces {
    pub(crate) sans_regular: Handle<Font>,
    pub(crate) sans_semibold: Handle<Font>,
    pub(crate) sans_bold: Handle<Font>,
    /// Fira Mono Regular ships available but no rule currently uses it
    /// (`docs/title-and-menu.md` § Type system §3).
    #[allow(dead_code)]
    pub(crate) mono_regular: Handle<Font>,
    pub(crate) mono_medium: Handle<Font>,
}

/// IBM Plex Sans Regular — `app/assets/fonts/IBMPlexSans-Regular.ttf`.
pub(crate) const IBMPLEXSANS_REGULAR: &[u8] =
    include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf");
/// IBM Plex Sans SemiBold.
pub(crate) const IBMPLEXSANS_SEMIBOLD: &[u8] =
    include_bytes!("../../assets/fonts/IBMPlexSans-SemiBold.ttf");
/// IBM Plex Sans Bold.
pub(crate) const IBMPLEXSANS_BOLD: &[u8] =
    include_bytes!("../../assets/fonts/IBMPlexSans-Bold.ttf");
/// Fira Mono Regular — `app/assets/fonts/FiraMono-Regular.ttf`.
pub(crate) const FIRAMONO_REGULAR: &[u8] =
    include_bytes!("../../assets/fonts/FiraMono-Regular.ttf");
/// Fira Mono Medium.
pub(crate) const FIRAMONO_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/FiraMono-Medium.ttf");

impl TextFaces {
    /// Insert the five embedded faces into `fonts` and return their handles.
    /// [`embed_text_faces`] calls this once `Assets<Font>` exists; no asset
    /// root, pack file or source directory is read.
    pub(crate) fn embed(fonts: &mut Assets<Font>) -> Self {
        Self {
            sans_regular: fonts.add(Font::from_bytes(IBMPLEXSANS_REGULAR.to_vec())),
            sans_semibold: fonts.add(Font::from_bytes(IBMPLEXSANS_SEMIBOLD.to_vec())),
            sans_bold: fonts.add(Font::from_bytes(IBMPLEXSANS_BOLD.to_vec())),
            mono_regular: fonts.add(Font::from_bytes(FIRAMONO_REGULAR.to_vec())),
            mono_medium: fonts.add(Font::from_bytes(FIRAMONO_MEDIUM.to_vec())),
        }
    }

    /// Plex Sans Regular — the prose and control-label face.
    pub(crate) fn sans(&self, size: f32) -> TextFont {
        self.font(self.sans_regular.clone(), size)
    }

    /// Plex Sans SemiBold — the sans headings' weight
    /// (`docs/title-and-menu.md` § Type system §3).
    pub(crate) fn sans_semibold(&self, size: f32) -> TextFont {
        self.font(self.sans_semibold.clone(), size)
    }

    /// Plex Sans Bold — the wordmark's weight.
    pub(crate) fn sans_bold(&self, size: f32) -> TextFont {
        self.font(self.sans_bold.clone(), size)
    }

    /// Fira Mono Medium — the annotation and machine-value face. Fira Mono
    /// Regular ships available but is deliberately unused.
    pub(crate) fn mono(&self, size: f32) -> TextFont {
        self.font(self.mono_medium.clone(), size)
    }

    fn font(&self, face: Handle<Font>, size: f32) -> TextFont {
        TextFont {
            font: face.into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

/// Embed the five faces and install the [`TextFaces`] resource.
/// `UnmeiPlugin::build` calls this once `Assets<Font>` exists (the
/// `TextPlugin` in `DefaultPlugins` adds it). It panics only when the font
/// store is absent, i.e. no `TextPlugin` was added.
pub(crate) fn embed_text_faces(app: &mut App) {
    let faces = {
        let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
        TextFaces::embed(&mut fonts)
    };
    app.insert_resource(faces);
}

pub(super) const CARD_BG: Color = Color::srgb(0.12, 0.13, 0.17);
pub(super) const PANEL_BG: Color = Color::srgb(0.10, 0.11, 0.15);
pub(super) const BUTTON_BG: Color = Color::srgb(0.16, 0.17, 0.22);

pub(crate) const FOCUS: Color = Color::srgb(0.90, 0.62, 0.36);
pub(super) const TEXT: Color = Color::srgb(0.90, 0.90, 0.92);
pub(super) const DIM: Color = Color::srgba(0.80, 0.80, 0.85, 0.62);
pub(super) const FAINT: Color = Color::srgba(0.80, 0.80, 0.85, 0.35);

/// The size that scales `image` to cover `window`: both axes filled, aspect
/// preserved, the overflowing axis cropped. A degenerate dimension (a
/// zero-sized image or window) returns the window unchanged, so nothing divides
/// by zero.
pub(crate) fn cover_size(image: Vec2, window: Vec2) -> Vec2 {
    if image.x <= 0.0 || image.y <= 0.0 || window.x <= 0.0 || window.y <= 0.0 {
        return window;
    }
    image * (window.x / image.x).max(window.y / image.y)
}

/// The size that scales `image` to fit inside `box`: both axes inside the box,
/// aspect preserved, the free axis letterboxed. It is [`cover_size`]'s inverse —
/// the same computation with `min` instead of `max` — so a card and the still
/// scale identically. A degenerate dimension (a zero-sized image or box)
/// returns the box unchanged, so nothing divides by zero. Centring the result
/// in the box is the caller's, exactly as [`fit_title_backdrop`](super::title::fit_title_backdrop) centres
/// [`cover_size`]'s.
// The gallery's card art and still view call this; the play path and the title
// do not.
pub(crate) fn contain_size(image: Vec2, box_size: Vec2) -> Vec2 {
    if image.x <= 0.0 || image.y <= 0.0 || box_size.x <= 0.0 || box_size.y <= 0.0 {
        return box_size;
    }
    image * (box_size.x / image.x).min(box_size.y / image.y)
}
