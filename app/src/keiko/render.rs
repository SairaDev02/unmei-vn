//! Slice 2's one render path (spec §13): the player's own composition drawn at
//! the 1280×720 reference size on wgpu's fallback adapter, read back as a PNG,
//! and the pointer a `click` step drives.
//!
//! The path is built on the player's regular app: the same plugins, the same
//! camera and the same UI. Only the camera's target changes while a frame is
//! captured, which is the one thing a windowless machine cannot supply. The
//! shipping player gains no hook (K19); a take retargets the camera it finds.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::menu::{MenuAction, MenuState};
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::ui::IsDefaultUiCamera;
use bevy::window::{PrimaryWindow, Window, WindowRef};

use super::steps::{ActivationError, FrameRun};
use unmei_engine::keiko::{Case, Click, Step};

/// The reference frame size every take renders at (spec §13).
pub const WIDTH: u32 = 1280;
/// The reference frame height (spec §13).
pub const HEIGHT: u32 = 720;

/// Whether this machine has the software rasterizer the slice needs, probed
/// once per process. Bevy's renderer panics when no adapter matches, so a take
/// asks this before it builds a rendered app at all.
pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| adapter().is_some())
}

/// The fallback (software) adapter the probe found, if any.
pub fn adapter() -> Option<wgpu::AdapterInfo> {
    // The same selection Bevy makes: `WGPU_BACKEND` chooses the backends, and
    // an unknown or absent backend for this platform therefore finds nothing.
    let backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::all());
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle_from_env()
    });
    bevy::tasks::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: true,
    }))
    .ok()
    .map(|adapter| adapter.get_info())
}

/// One take's rendered frame: the image it draws into, the stand-in window its
/// pointer moves on, and the folder its PNGs land in.
pub struct Rendered {
    image: Handle<Image>,
    window: Entity,
    camera: Option<Entity>,
    frames: PathBuf,
    count: u32,
}

impl Rendered {
    /// Set up the take's render target, the stand-in window the pointer moves
    /// on, and the frames folder `<pack-parent>/<pack-name>.keiko/frames/<case>/`
    /// beside the real pack (spec §13).
    pub fn new(app: &mut App, pack_parent: &Path, pack_name: &str, case: &str) -> Self {
        // A window entity with no raw handle is never presented: the renderer
        // creates no surface for it, so it exists only to give the pointer and
        // the UI focus path a cursor (spec §13).
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: bevy::window::WindowResolution::new(WIDTH, HEIGHT),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        let mut image =
            Image::new_target_texture(WIDTH, HEIGHT, TextureFormat::Rgba8UnormSrgb, None);
        // The readback copies the texture out; without `COPY_SRC` it cannot.
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        Self {
            image,
            window,
            camera: None,
            frames: pack_parent
                .join(format!("{pack_name}.keiko"))
                .join("frames")
                .join(sanitize(case)),
            count: 0,
        }
    }

    /// `frame [name]`: draw the current composition at 1280×720 and write it as
    /// a PNG. Extra frames run with a zero-length clock, so the observation and
    /// the take's clock are the ones the step began with (spec §4.3).
    pub fn capture(
        &mut self,
        app: &mut App,
        run: FrameRun,
        name: Option<&str>,
    ) -> Result<PathBuf, String> {
        let camera = self.camera(app)?;
        self.point_at(app, camera, true);
        run(app);
        run(app);

        let landed: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
        let sink = landed.clone();
        let entity = app
            .world_mut()
            .spawn(Readback::texture(self.image.clone()))
            .observe(move |event: On<ReadbackComplete>| {
                *sink.lock().unwrap() = Some(event.data.clone());
            })
            .id();
        for _ in 0..READBACK_FRAMES {
            run(app);
            if landed.lock().unwrap().is_some() {
                break;
            }
        }
        app.world_mut().entity_mut(entity).despawn();
        // The copies requested on the frames before the first one came back are
        // still in flight; two more frames let them land, so a take that ends
        // right after this step leaves no readback pending at teardown.
        for _ in 0..2 {
            run(app);
        }
        self.point_at(app, camera, false);

        let data = landed
            .lock()
            .unwrap()
            .take()
            .ok_or("the frame did not come back from the rasterizer")?;
        let expected = (WIDTH * HEIGHT * 4) as usize;
        if data.len() != expected {
            return Err(format!(
                "the frame came back with {} bytes, not {expected}",
                data.len()
            ));
        }
        self.count += 1;
        let path = self.frames.join(self.file_name(name));
        write_png(&path, &data)?;
        eprintln!("frame {}: {}", self.count, path.display());
        Ok(path)
    }

    /// Move the stand-in window's cursor to a reference point and click the left
    /// button: press in one frame, release in the next. The player's own hit test
    /// decides what, if anything, the click reaches (spec §13, K9).
    pub fn click(&mut self, app: &mut App, run: FrameRun, point: Vec2) -> Result<(), String> {
        let Some(mut window) = app.world_mut().get_mut::<Window>(self.window) else {
            return Err("the take has no stand-in window".to_string());
        };
        window.set_physical_cursor_position(Some(point.as_dvec2()));
        run(app);
        button(app, run, self.window, ButtonState::Pressed);
        button(app, run, self.window, ButtonState::Released);
        Ok(())
    }

    /// `click <control>` / `click <x> <y>`: move the stand-in window's cursor
    /// and press and release the left button, so the player's own hit test
    /// decides what the click reaches (spec §13, K9).
    pub fn click_control(
        &mut self,
        app: &mut App,
        run: FrameRun,
        click: &Click,
    ) -> Result<(), ActivationError> {
        let point = match click {
            Click::Position(x, y) => Vec2::new(*x, *y),
            Click::Control(id) => Self::control_centre(app, id)?,
        };
        self.click(app, run, point)
            .map_err(ActivationError::render_unavailable)
    }

    /// The centre of a control's laid-out node, in reference pixels. The topmost
    /// visible node carrying the control's action is the one a player sees, so it is
    /// the one clicked; a control with no laid-out node fails like an absent one.
    fn control_centre(app: &mut App, id: &str) -> Result<Vec2, ActivationError> {
        let Some(action) = super::controls::action_for_id(id) else {
            return Err(ActivationError::no_target(format!(
                "unknown control `{id}`"
            )));
        };
        let mut found: Option<((bool, u32), Vec2)> = None;
        let mut query = app.world_mut().query::<(
            &MenuAction,
            &bevy::ui::UiGlobalTransform,
            &bevy::camera::visibility::InheritedVisibility,
            Option<&bevy::ui::ComputedStackIndex>,
        )>();
        for (candidate, transform, visible, stack) in query.iter(app.world()) {
            if *candidate != action {
                continue;
            }
            let key = (visible.get(), stack.map(|stack| stack.0).unwrap_or(0));
            if found.as_ref().is_none_or(|(best, _)| key > *best) {
                found = Some((key, transform.transform_point2(Vec2::ZERO)));
            }
        }
        match found {
            Some(((true, _), centre)) => Ok(centre),
            Some(((false, _), _)) => Err(ActivationError::no_target(format!(
                "`{id}` has no visible node on the {} surface",
                super::steps::surface_name(&app.world().resource::<MenuState>().clone())
            ))),
            None => Err(ActivationError::no_target(format!("`{id}` has no node"))),
        }
    }

    /// The player's camera. It spawns with the play surface, so the first frame
    /// step finds it; marking it the UI's default camera keeps the player's
    /// nodes attached to it while it points at the image.
    fn camera(&mut self, app: &mut App) -> Result<Entity, String> {
        if let Some(camera) = self.camera {
            return Ok(camera);
        }
        let camera = app
            .world_mut()
            .query_filtered::<Entity, With<Camera2d>>()
            .iter(app.world())
            .next()
            .ok_or("the player's camera has not spawned")?;
        app.world_mut().entity_mut(camera).insert(IsDefaultUiCamera);
        self.camera = Some(camera);
        Ok(camera)
    }

    /// Point the camera at the image, or back at the window a player uses.
    fn point_at(&self, app: &mut App, camera: Entity, at_image: bool) {
        let target = if at_image {
            RenderTarget::Image(ImageRenderTarget {
                handle: self.image.clone(),
                scale_factor: 1.0,
            })
        } else {
            RenderTarget::Window(WindowRef::Primary)
        };
        app.world_mut().entity_mut(camera).insert(target);
    }

    /// The PNG's name: the frame's 1-based ordinal, plus the step's own name.
    fn file_name(&self, name: Option<&str>) -> String {
        match name {
            Some(name) => format!("{}-{}.png", self.count, sanitize(name)),
            None => format!("{}.png", self.count),
        }
    }
}

/// How many zero-length frames a readback may take before the step gives up.
const READBACK_FRAMES: usize = 120;

/// One left-button message for the stand-in window.
fn button(app: &mut App, run: FrameRun, window: Entity, state: ButtonState) {
    app.world_mut().write_message(MouseButtonInput {
        button: MouseButton::Left,
        state,
        window,
    });
    run(app);
}

/// Write the readback bytes as a PNG, creating the frames folder.
fn write_png(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    image::save_buffer(path, data, WIDTH, HEIGHT, image::ExtendedColorType::Rgba8)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// A path component that cannot leave the frames folder.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => character,
            _ => '-',
        })
        .collect()
}

/// Whether the case asks for the rendered composition: one `frame` or `click`
/// step builds it for the whole take, so the steps before it run as usual
/// (spec §13).
pub fn draws(case: &Case) -> bool {
    case.steps.iter().any(|line| {
        matches!(
            line.step,
            Step::Frame(_) | Step::Click(Click::Control(_)) | Step::Click(Click::Position(_, _))
        )
    })
}

/// The case's folder name: its id, never a path and never its extension.
pub fn case_name(case_file: &str) -> String {
    std::path::Path::new(case_file)
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| case_file.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> Rendered {
        Rendered {
            image: Handle::default(),
            window: Entity::PLACEHOLDER,
            camera: None,
            frames: PathBuf::new(),
            count: 0,
        }
    }

    /// A frame's name is its 1-based step ordinal, then the step's own name,
    /// which cannot leave the frames folder (spec §13).
    #[test]
    fn a_frames_name_is_its_ordinal_and_a_plain_name() {
        let mut rendered = rendered();
        rendered.count = 1;
        assert_eq!(rendered.file_name(None), "1.png");
        rendered.count = 2;
        assert_eq!(rendered.file_name(Some("titled")), "2-titled.png");
        assert_eq!(
            rendered.file_name(Some("../escape/here")),
            "2-..-escape-here.png"
        );
        assert_eq!(rendered.file_name(Some("a b")), "2-a-b.png");
    }
}
