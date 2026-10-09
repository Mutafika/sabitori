//! # sabitori-reel
//!
//! Programmatic motion-graphics video for [Sabitori]. This is to Sabitori what
//! Remotion is to React: a [`Scene`] returns a **declarative `Element` tree**
//! (the same UI you'd build for a window), Sabitori lays it out and renders it
//! on the GPU **offscreen** (headless, no window), and each frame is read back
//! and written as a PNG sequence.
//!
//! A video is a **pure function of the frame**: the scene reads a [`FrameCtx`]
//! and returns an `Element`, animating props (`opacity`, `.ty(..)`,
//! `Color::lerp`, positions, sizes) off `ctx.t()` / `ctx.frame`.
//!
//! ```no_run
//! use sabitori_reel::{div, text, Color, Element, FrameCtx, Px, Reel};
//!
//! let reel = Reel::new(1280, 720, 30, 90);
//! let scene = |ctx: FrameCtx| -> Element {
//!     div()
//!         .w(Px(ctx.width)).h(Px(ctx.height))
//!         .flex_col().items_center().justify_center().gap(16.0)
//!         .bg(Color::from_hex("#070910"))
//!         .child(text("EnchuDB").font_size(96.0).bold().color(Color::from_hex("#5eead4")))
//!         .child(text("円柱 database").font_size(30.0).color(Color::from_hex("#818da6")))
//! };
//! reel.render_pngs(&scene, std::path::Path::new("/tmp/out")).unwrap();
//! ```
//!
//! [Sabitori]: https://github.com/Mutafika/sabitori

use std::cell::RefCell;
use std::process::Command;
use std::sync::Arc;

use sabitori::bridge::{
    extract_image_batches, render_list_to_gpu_with_rings, MeasureCache, TextRendererMeasurer,
};
use sabitori::{DeclarativeApp, InputEvent, Key};
#[cfg(not(feature = "three-d"))]
use sabitori::run_declarative;
#[cfg(feature = "three-d")]
use sabitori::{run_scene, GpuContext, SceneApp, SceneRenderContext};
use sabitori_core::{build_tree_measured, ViewContext};
use sabitori_gpu::{ImageRenderer, LineRenderer, RingRenderer, UiOverlayRenderer};
use sabitori_text::TextRenderer;

// Re-export the Sabitori declarative builder surface so a scene file only needs
// `use sabitori_reel::*` to author a tree.
pub use sabitori_core::element::polyline;
pub use sabitori_core::{
    arc, button, div, image, text, Auto, Color, Dimension, Element, ImageData, ObjectFit, Percent,
    Px, Typography,
};

/// A 3D stage drawn *beneath* the UI overlay, rendered by `seimei` (PBR,
/// shadow maps) on the reel's own offscreen device. Requires the `three-d`
/// feature.
///
/// A stage stays a pure function of the frame, like the rest of a reel:
/// [`Scene::setup_stage`] registers meshes and lights once, and
/// [`Scene::stage`] returns the camera and per-instance transforms for a single
/// frame — so scrubbing backwards lands on the identical image.
#[cfg(feature = "three-d")]
pub mod three {
    pub use seimei::math::Vec3D;
    pub use seimei::procedural;
    pub use seimei::{
        Camera, InstanceData, Light, LightKind, LineVertex, MsaaSamples, Point3, QualityPreset,
        QualitySettings, RenderMesh, Renderer, MODEL_GLASS, MODEL_JELLY, MODEL_STANDARD,
        MODEL_WATER,
    };

    /// One frame's worth of 3D: where the camera is, and what is drawn.
    pub struct Stage {
        pub camera: Camera,
        /// `(mesh id registered in `setup_stage`, per-instance transform)`.
        pub instances: Vec<(String, InstanceData)>,
        /// How many leading entries of `instances` are opaque. Transparent
        /// instances must come last and be sorted back-to-front by the scene;
        /// `None` treats every instance as opaque.
        pub opaque_count: Option<usize>,
        /// Free-standing 3D line segments, as **pairs** of vertices — the
        /// pipeline is a line list, so `lines[0]`–`lines[1]` is one segment,
        /// `lines[2]`–`lines[3]` the next. Drawn with the scene, no mesh
        /// registration needed.
        pub lines: Vec<LineVertex>,
    }

    impl Stage {
        /// A stage whose instances are all opaque.
        pub fn new(camera: Camera, instances: Vec<(String, InstanceData)>) -> Self {
            Self {
                camera,
                instances,
                opaque_count: None,
                lines: Vec::new(),
            }
        }

        /// One line segment from `a` to `b`, pushed onto a line list.
        pub fn segment(out: &mut Vec<LineVertex>, a: [f32; 3], b: [f32; 3], color: [f32; 4]) {
            out.push(LineVertex::new(a, color));
            out.push(LineVertex::new(b, color));
        }

        /// Model matrix helper: translate + uniform scale, no rotation.
        pub fn at(pos: [f32; 3], scale: f32) -> [[f32; 4]; 4] {
            [
                [scale, 0.0, 0.0, 0.0],
                [0.0, scale, 0.0, 0.0],
                [0.0, 0.0, scale, 0.0],
                [pos[0], pos[1], pos[2], 1.0],
            ]
        }

        /// Model matrix that stretches a **unit mesh along +Y** (radius 1,
        /// height 1, centred on the origin — e.g. `procedural::cylinder(1.0,
        /// 1.0, n)`) so it spans `a` to `b` with the given radius.
        ///
        /// Use it to draw a connector as real geometry rather than a line: a
        /// [`LineVertex`] segment is always one pixel wide however close the
        /// camera gets, and picks up no light.
        pub fn between(a: [f32; 3], b: [f32; 3], radius: f32) -> [[f32; 4]; 4] {
            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if len < 1e-6 {
                return Self::at(a, radius);
            }
            let y = [d[0] / len, d[1] / len, d[2] / len];
            // Any reference vector not parallel to the segment works; swap it
            // near the poles so the cross product stays well conditioned.
            let r = if y[1].abs() > 0.99 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let cross = |u: [f32; 3], v: [f32; 3]| {
                [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ]
            };
            let norm = |v: [f32; 3]| {
                let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
                [v[0] / n, v[1] / n, v[2] / n]
            };
            let x = norm(cross(r, y));
            let z = cross(y, x);
            [
                [x[0] * radius, x[1] * radius, x[2] * radius, 0.0],
                [y[0] * len, y[1] * len, y[2] * len, 0.0],
                [z[0] * radius, z[1] * radius, z[2] * radius, 0.0],
                [
                    (a[0] + b[0]) * 0.5,
                    (a[1] + b[1]) * 0.5,
                    (a[2] + b[2]) * 0.5,
                    1.0,
                ],
            ]
        }

        /// Model matrix helper: uniform scale, then spin `yaw` radians about Y,
        /// then translate. The order matters — this is the one a turntable
        /// shot wants.
        pub fn spun(pos: [f32; 3], scale: f32, yaw: f32) -> [[f32; 4]; 4] {
            let (s, c) = yaw.sin_cos();
            [
                [c * scale, 0.0, -s * scale, 0.0],
                [0.0, scale, 0.0, 0.0],
                [s * scale, 0.0, c * scale, 0.0],
                [pos[0], pos[1], pos[2], 1.0],
            ]
        }
    }
}

/// Per-frame context handed to a [`Scene`] — everything it needs to be a pure
/// function of the frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameCtx {
    /// Frame index, starting at 0.
    pub frame: u32,
    /// Frames per second of the reel.
    pub fps: u32,
    /// Canvas width in logical pixels.
    pub width: f32,
    /// Canvas height in logical pixels.
    pub height: f32,
}

impl FrameCtx {
    /// Time in seconds since frame 0.
    pub fn t(&self) -> f32 {
        self.frame as f32 / self.fps as f32
    }

    /// Progress through the whole reel in `[0, 1)`.
    pub fn progress(&self, total_frames: u32) -> f32 {
        if total_frames == 0 {
            0.0
        } else {
            self.frame as f32 / total_frames as f32
        }
    }
}

/// Frame-based animation helpers — the Remotion `interpolate` / `spring` /
/// `<Sequence>` surface adapted to Sabitori's `Scene = fn(FrameCtx) -> Element`
/// model. Every one is a **pure function of time**, so a scrubbed or reversed
/// playhead always reproduces the same value (unlike a stateful UI spring that
/// integrates across frames).
pub mod anim {
    use crate::FrameCtx;

    /// Easing curves, re-exported so a scene needs only `use sabitori_reel::*`.
    /// Variants include `Linear`, `EaseIn`, `EaseOut`, `EaseInOut`,
    /// `EaseOutCubic`, `EaseOutBack`, `EaseOutElastic`, `CubicBezier(..)`.
    pub use sabitori::EasingFunction as Easing;
    /// Spring physics config with presets: [`Spring::default`] (stiff, no
    /// overshoot), [`Spring::gentle`], [`Spring::snappy`], [`Spring::bouncy`],
    /// and [`Spring::critical`].
    pub use sabitori::Spring;

    /// Map `x` from the input range onto the output range, **clamped** to the
    /// output ends (Remotion's `extrapolate: 'clamp'` — the sane default).
    /// Ideal for entrances: `opacity = interpolate(t, [0.0, 0.5], [0.0, 1.0])`.
    pub fn interpolate(x: f32, in_range: [f32; 2], out_range: [f32; 2]) -> f32 {
        interpolate_eased(x, in_range, out_range, Easing::Linear)
    }

    /// Like [`interpolate`], but the normalized progress is shaped by `easing`
    /// before it maps onto the output range.
    pub fn interpolate_eased(
        x: f32,
        in_range: [f32; 2],
        out_range: [f32; 2],
        easing: Easing,
    ) -> f32 {
        let [i0, i1] = in_range;
        let [o0, o1] = out_range;
        if (i1 - i0).abs() < f32::EPSILON {
            return o0;
        }
        let p = ((x - i0) / (i1 - i0)).clamp(0.0, 1.0);
        o0 + (o1 - o0) * easing.eval(p)
    }

    /// Interpolate a [`Color`](sabitori_core::Color) from `a` to `b` as `x`
    /// crosses the input range (clamped at both ends).
    pub fn interpolate_color(
        x: f32,
        in_range: [f32; 2],
        a: sabitori_core::Color,
        b: sabitori_core::Color,
    ) -> sabitori_core::Color {
        a.lerp(b, interpolate(x, in_range, [0.0, 1.0]))
    }

    /// Spring value at time `t` seconds, simulated from rest (0) toward a
    /// target of 1 at `fps`. **Pure**: the same `t` always yields the same
    /// value, so scrubbing and reverse playback work. Bouncy configs overshoot
    /// past 1. Drive a scale / translate pop with it.
    pub fn spring(t: f32, fps: u32) -> f32 {
        spring_with(t, fps, Spring::default())
    }

    /// [`spring`] with an explicit [`Spring`] config (use the presets).
    pub fn spring_with(t: f32, fps: u32, cfg: Spring) -> f32 {
        if fps == 0 || t <= 0.0 {
            return 0.0;
        }
        let dt = 1.0 / fps as f32;
        let steps = (t * fps as f32).round() as u32;
        let (mut value, mut velocity) = (0.0f32, 0.0f32);
        for _ in 0..steps {
            let (v, vel, settled) = cfg.step(value, velocity, 1.0, dt);
            value = v;
            velocity = vel;
            if settled {
                return 1.0;
            }
        }
        value
    }

    /// A slice of the timeline in seconds — the `<Sequence>` analog. Gate and
    /// time-shift part of a scene so it only exists during its window and sees
    /// its own local clock starting at 0.
    ///
    /// ```ignore
    /// if let Some(local) = Seq::new(1.0, 2.0).enter(ctx) {
    ///     // shown for t in [1.0, 3.0); local.t() runs 0 → 2.0
    ///     kids.push(text("act two").opacity(interpolate(local.t(), [0.0, 0.4], [0.0, 1.0])));
    /// }
    /// ```
    #[derive(Clone, Copy, Debug)]
    pub struct Seq {
        /// Start time on the reel timeline, in seconds.
        pub start: f32,
        /// Length of the window, in seconds.
        pub dur: f32,
    }

    impl Seq {
        pub fn new(start: f32, dur: f32) -> Self {
            Self { start, dur }
        }

        /// Is the reel's playhead inside `[start, start + dur)`?
        pub fn active(&self, ctx: FrameCtx) -> bool {
            let t = ctx.t();
            t >= self.start && t < self.start + self.dur
        }

        /// Progress through the window in `[0, 1]` (0 before it, 1 after).
        pub fn progress(&self, ctx: FrameCtx) -> f32 {
            if self.dur <= 0.0 {
                return 0.0;
            }
            ((ctx.t() - self.start) / self.dur).clamp(0.0, 1.0)
        }

        /// [`progress`](Seq::progress) shaped by `easing` — 0 before the
        /// window, eased through it, and **held at 1 afterwards**. This is the
        /// one an entrance wants: fade a title in, then leave it up.
        pub fn eased(&self, ctx: FrameCtx, easing: Easing) -> f32 {
            easing.eval(self.progress(ctx))
        }

        /// If active, return a [`FrameCtx`] whose clock is re-based so the
        /// window starts at frame 0 / t 0; otherwise `None`. Mount the element
        /// only when `Some` and animate it off the returned local time.
        ///
        /// Note this is a *window*: it also returns `None` once the window has
        /// passed, so anything mounted through it disappears again. For an
        /// entrance that stays on screen, use [`since`](Seq::since).
        pub fn enter(&self, ctx: FrameCtx) -> Option<FrameCtx> {
            if !self.active(ctx) {
                return None;
            }
            let local = ((ctx.t() - self.start) * ctx.fps as f32).round() as u32;
            Some(FrameCtx { frame: local, ..ctx })
        }

        /// Like [`enter`](Seq::enter) but open-ended: `None` only *before*
        /// `start`, and afterwards a local clock that keeps running past
        /// `dur`. Drive a spring with it and the element pops in, settles, and
        /// stays put.
        pub fn since(&self, ctx: FrameCtx) -> Option<FrameCtx> {
            if ctx.t() < self.start {
                return None;
            }
            let local = ((ctx.t() - self.start) * ctx.fps as f32).round() as u32;
            Some(FrameCtx { frame: local, ..ctx })
        }
    }
}

pub use anim::{
    interpolate, interpolate_color, interpolate_eased, spring, spring_with, Easing, Seq, Spring,
};

/// A scene is a **pure function of the frame**: the same [`FrameCtx`] must
/// always produce the same [`Element`] tree, so every render is deterministic.
pub trait Scene {
    fn view(&self, ctx: FrameCtx) -> Element;

    /// Register the 3D meshes and lights this scene draws. Called once, before
    /// the first frame. Default: no 3D.
    #[cfg(feature = "three-d")]
    fn setup_stage(&self, _renderer: &mut three::Renderer) {}

    /// The 3D stage for one frame, drawn beneath [`Scene::view`]'s tree.
    /// Default: none, and the reel takes its 2D-only path.
    #[cfg(feature = "three-d")]
    fn stage(&self, _ctx: FrameCtx) -> Option<three::Stage> {
        None
    }

    /// Renderer quality for the 3D stage.
    ///
    /// Default is `Medium` **plus 4× MSAA**: an offscreen render has no frame
    /// budget, so there is no reason to ship aliased edges. Post-process
    /// effects stay off — enabling any of them switches `seimei` to its HDR
    /// path, which the reel's single-sampled target does not feed.
    #[cfg(feature = "three-d")]
    fn stage_quality(&self) -> three::QualitySettings {
        three::QualitySettings {
            msaa: three::MsaaSamples::X4,
            ..three::QualitySettings::from_preset(three::QualityPreset::Medium)
        }
    }
}

// A plain closure is a Scene, so callers can pass `|ctx| div()...`.
impl<F: Fn(FrameCtx) -> Element> Scene for F {
    fn view(&self, ctx: FrameCtx) -> Element {
        self(ctx)
    }
}

/// A reel: a fixed canvas size, frame rate, and frame count. Lays out and
/// renders a [`Scene`]'s `Element` tree offscreen to a PNG sequence.
pub struct Reel {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: u32,
    /// Clear color behind the scene, linear RGBA in `0.0..=1.0`.
    pub clear: [f64; 4],
    /// Font files (TTF/OTF bytes) put ahead of the built-in and system fonts,
    /// e.g. a brand face in Regular + Bold. Empty = the defaults.
    pub fonts: Vec<Vec<u8>>,
    /// Family name used for ordinary (non-mono) text, e.g. `"HackGen35"` after
    /// loading it through [`Reel::fonts`]. `None` = the system sans-serif.
    pub family: Option<String>,
    /// Pixels per logical px. The scene is laid out at `width × height`
    /// logical px either way; `2.0` renders and writes a frame twice as large
    /// in each direction (e.g. 1920×1080 → 3840×2160), with text rasterized at
    /// that size rather than upscaled. Default `1.0`.
    pub scale: f32,
    /// x264 quality for [`Reel::render_mp4`] (lower = better, bigger). Flat
    /// motion graphics compress well, so going below the default 18 costs
    /// little. Default `18`.
    pub crf: u8,
}

impl Reel {
    pub fn new(width: u32, height: u32, fps: u32, frames: u32) -> Self {
        Self {
            width,
            height,
            fps,
            frames,
            clear: [0.0, 0.0, 0.0, 1.0],
            fonts: Vec::new(),
            family: None,
            scale: 1.0,
            crf: 18,
        }
    }

    /// Render every frame to `dir/frame_00000.png`, `frame_00001.png`, … and
    /// return the written paths in order.
    pub fn render_pngs(
        &self,
        scene: &dyn Scene,
        dir: &std::path::Path,
    ) -> std::io::Result<Vec<std::path::PathBuf>> {
        std::fs::create_dir_all(dir)?;
        let mut gpu = Gpu::new(self.width, self.height, self.scale, &self.fonts, self.family.clone());
        let frame0 = FrameCtx {
            frame: 0,
            fps: self.fps,
            width: self.width as f32,
            height: self.height as f32,
        };
        // Only stand up the 3D renderer if this scene actually draws a stage.
        #[cfg(feature = "three-d")]
        if scene.stage(frame0).is_some() {
            gpu.init_stage(scene);
        }
        let mut paths = Vec::with_capacity(self.frames as usize);
        for frame in 0..self.frames {
            let ctx = FrameCtx { frame, ..frame0 };
            let root = scene.view(ctx);
            let rgba = gpu.render_frame(&root, self.clear, frame_stage(scene, ctx));
            let path = dir.join(format!("frame_{frame:05}.png"));
            image::save_buffer(
                &path,
                &rgba,
                gpu.width,
                gpu.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(std::io::Error::other)?;
            paths.push(path);
        }
        Ok(paths)
    }

    /// Render every frame and encode it to an H.264 MP4 at `out` via `ffmpeg`,
    /// returning `out`. The PNG sequence goes to a scratch dir first, then
    /// `ffmpeg` stitches it (`libx264`, `yuv420p`, CRF 18, `+faststart`).
    ///
    /// `ffmpeg` is taken from `PATH`. Override the binary with the
    /// `SABITORI_REEL_FFMPEG` env var (e.g. a self-contained build); for a
    /// custom binary its own directory is added to `DYLD_LIBRARY_PATH` so it
    /// can resolve sibling dynamic libraries.
    pub fn render_mp4(
        &self,
        scene: &dyn Scene,
        out: &std::path::Path,
    ) -> std::io::Result<std::path::PathBuf> {
        // `ffmpeg` will not create the output's directory, and fails with a
        // bare "No such file or directory" if it is missing.
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Clear the scratch dir first so a shorter reel can't inherit stale
        // frames from a previous, longer run.
        let dir = std::env::temp_dir().join("sabitori-reel-encode");
        let _ = std::fs::remove_dir_all(&dir);
        self.render_pngs(scene, &dir)?;

        let ffmpeg = std::env::var("SABITORI_REEL_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_string());
        let mut cmd = Command::new(&ffmpeg);
        // Let a self-contained ffmpeg find its bundled dylibs (Remotion's build
        // needs this on macOS). Skipped for a bare "ffmpeg" from PATH.
        if let Some(parent) =
            std::path::Path::new(&ffmpeg).parent().filter(|p| !p.as_os_str().is_empty())
        {
            cmd.env("DYLD_LIBRARY_PATH", parent);
        }
        let fps = self.fps.to_string();
        let crf = self.crf.to_string();
        let pattern = dir.join("frame_%05d.png");
        // NB: H.264 + yuv420p needs even width/height; reel sizes are expected
        // to be even (720p/1080p). We avoid a scale/pad filter so a minimal
        // ffmpeg build (e.g. Remotion's) without those filters still works.
        cmd.args(["-y", "-framerate", fps.as_str(), "-i"])
            .arg(&pattern)
            .args([
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-crf",
                crf.as_str(),
                "-preset",
                "slow",
                "-movflags",
                "+faststart",
            ])
            .arg(out);

        let status = cmd.status().map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!(
                    "failed to launch ffmpeg ({ffmpeg:?}): {e} — install ffmpeg or set \
                     SABITORI_REEL_FFMPEG to a binary"
                ),
            )
        })?;
        if !status.success() {
            return Err(std::io::Error::other(format!(
                "ffmpeg exited unsuccessfully ({status})"
            )));
        }
        Ok(out.to_path_buf())
    }
}

/// Owns the headless GPU device, the offscreen render target, Sabitori's four
/// 2D renderers (rects / rings / lines / text — all sharing one globals
/// uniform), a persistent text-measure cache, and one reusable readback buffer.
/// One frame's 3D stage as passed down to the GPU. Collapses to `()` when the
/// `three-d` feature is off, so the render path keeps a single signature.
#[cfg(feature = "three-d")]
type StageArg = Option<three::Stage>;
#[cfg(not(feature = "three-d"))]
type StageArg = ();

#[cfg(feature = "three-d")]
fn frame_stage(scene: &dyn Scene, ctx: FrameCtx) -> StageArg {
    scene.stage(ctx)
}
#[cfg(not(feature = "three-d"))]
fn frame_stage(_scene: &dyn Scene, _ctx: FrameCtx) -> StageArg {}

/// The `seimei` renderer and its depth buffer, live only while a scene draws
/// 3D. Shares the reel's device and queue, so the 3D pass writes straight into
/// the same offscreen texture the UI overlay is composited onto — no readback
/// or CPU round-trip between the two.
#[cfg(feature = "three-d")]
struct StageGpu {
    renderer: three::Renderer,
    depth: wgpu::TextureView,
}

/// Depth attachment for a 3D pass. The sample count has to come from the
/// renderer rather than the target: with MSAA on, `seimei` resolves into a
/// single-sampled colour view but still wants a multisampled depth beside it.
#[cfg(feature = "three-d")]
fn stage_depth(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    sample_count: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("reel_stage_depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

struct Gpu {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    ui: UiOverlayRenderer,
    text: TextRenderer,
    ring: RingRenderer,
    line: LineRenderer,
    image: ImageRenderer,
    measure_cache: RefCell<MeasureCache>,
    texture: wgpu::Texture,
    /// Size of the target in physical pixels (logical size × scale).
    width: u32,
    height: u32,
    /// Layout size in logical px.
    logical: (f32, f32),
    unpadded_bpr: u32,
    padded_bpr: u32,
    readback: wgpu::Buffer,
    #[cfg(feature = "three-d")]
    stage: Option<StageGpu>,
}

impl Gpu {
    fn new(width: u32, height: u32, scale: f32, fonts: &[Vec<u8>], family: Option<String>) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("sabitori-reel: no GPU adapter available");
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("sabitori-reel"),
                ..Default::default()
            },
            None,
        ))
        .expect("sabitori-reel: failed to create device");
        let (device, queue) = (Arc::new(device), Arc::new(queue));

        let format = wgpu::TextureFormat::Rgba8UnormSrgb;

        // rects; all other renderers share its globals (screen size / scale).
        let ui = UiOverlayRenderer::new(&device, format);
        let logical = (width as f32, height as f32);
        ui.update_globals(&queue, logical.0, logical.1, scale);
        // From here on `width` / `height` are the target's physical pixels.
        let width = (width as f32 * scale).round() as u32;
        let height = (height as f32 * scale).round() as u32;
        let layout = ui.globals_bind_group_layout();

        let mut text = TextRenderer::new(&device, format, layout);
        text.set_scale_factor(scale);
        if !fonts.is_empty() {
            text.prefer_user_fonts(fonts);
        }
        text.set_preferred_family(family);
        let ring = RingRenderer::new(&device, format, layout);
        let line = LineRenderer::new(&device, format, layout);
        let image = ImageRenderer::new(&device, format, layout);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("reel_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let unpadded_bpr = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bpr = unpadded_bpr.div_ceil(align) * align;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reel_readback"),
            size: (padded_bpr as u64) * (height as u64),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            device,
            queue,
            ui,
            text,
            ring,
            line,
            image,
            measure_cache: RefCell::new(MeasureCache::new()),
            texture,
            width,
            height,
            logical,
            unpadded_bpr,
            padded_bpr,
            readback,
            #[cfg(feature = "three-d")]
            stage: None,
        }
    }

    /// Stand up the 3D renderer on this device and let `scene` register its
    /// meshes and lights. Called once, and only when the scene actually draws
    /// a stage — a 2D reel never pays for it.
    ///
    /// Quality is pinned to `Medium`: MSAA off and post-process off, which
    /// keeps `scene_sample_count() == 1` so the 3D pass can target the reel's
    /// single-sampled offscreen texture directly.
    #[cfg(feature = "three-d")]
    fn init_stage(&mut self, scene: &dyn Scene) {
        let mut renderer = three::Renderer::new(
            self.device.clone(),
            self.queue.clone(),
            self.texture.format(),
            self.width,
            self.height,
        )
        .expect("sabitori-reel: failed to create the seimei renderer");
        renderer
            .set_quality(scene.stage_quality())
            .expect("sabitori-reel: failed to apply 3D quality settings");
        scene.setup_stage(&mut renderer);

        let depth = stage_depth(
            &self.device,
            self.width,
            self.height,
            renderer.scene_sample_count(),
        );

        self.stage = Some(StageGpu { renderer, depth });
    }

    /// Lay out the Element tree, convert it to GPU primitives, draw them into
    /// the offscreen texture, and read the pixels back as tightly-packed RGBA8.
    fn render_frame(&mut self, root: &Element, clear: [f64; 4], stage: StageArg) -> Vec<u8> {
        let (w, h) = self.logical;

        // Layout (needs a text measurer over the persistent TextRenderer).
        let mut build = {
            let measurer = TextRendererMeasurer::new(&mut self.text, &self.measure_cache);
            build_tree_measured(root, w, h, &measurer)
        };
        // 1 枚に描く: 層 1 以上は層 0 に寄せる
        build.flatten_layers();
        // RenderList -> GPU instance vecs (also shapes text into the glyph atlas).
        let (rects, glyphs, rings, lines) =
            render_list_to_gpu_with_rings(&build.render_list, &mut self.text);
        // Images live on a separate command path; group them into per-key
        // batches and upload their pixels to GPU textures (cached by key).
        let image_batches = extract_image_batches(&build.render_list);

        self.ui.upload_rects(&self.device, &self.queue, &rects, &[]);
        for batch in &image_batches {
            self.image.ensure_texture(
                &self.device,
                &self.queue,
                &batch.key,
                &batch.data.rgba,
                batch.data.width,
                batch.data.height,
            );
        }

        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("reel_encoder"),
            });

        // 3D first, straight into the same offscreen texture. It clears the
        // target itself (with the reel's clear color), so when a stage runs the
        // UI pass below loads instead of clearing and lands on top of it.
        #[cfg(feature = "three-d")]
        let drew_3d = match (stage, self.stage.as_mut()) {
            (Some(stage), Some(gpu)) => {
                gpu.renderer.set_clear_rgba(wgpu::Color {
                    r: clear[0],
                    g: clear[1],
                    b: clear[2],
                    a: clear[3],
                });
                let opaque = stage.opaque_count.unwrap_or(stage.instances.len());
                gpu.renderer.update_lines(&stage.lines);
                gpu.renderer.render_to_view(
                    &mut encoder,
                    &view,
                    &gpu.depth,
                    &stage.camera,
                    &stage.instances,
                    opaque,
                );
                true
            }
            _ => false,
        };
        #[cfg(not(feature = "three-d"))]
        let drew_3d = {
            let _ = stage;
            false
        };

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("reel_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if drew_3d {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(wgpu::Color {
                                r: clear[0],
                                g: clear[1],
                                b: clear[2],
                                a: clear[3],
                            })
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // Painter order (all share ui's globals): rects, images, rings,
            // lines, text — text last so it sits on top.
            let globals = self.ui.globals_bind_group();
            self.ui.draw_base(&mut pass);
            self.image.render_many(
                &self.device,
                &self.queue,
                image_batches
                    .iter()
                    .map(|b| (b.key.as_str(), b.instances.as_slice())),
                &mut pass,
                globals,
            );
            self.ring
                .render_rings(&self.device, &self.queue, &rings, &mut pass, globals);
            self.line
                .render_lines(&self.device, &self.queue, &lines, &mut pass, globals);
            self.text
                .render_glyphs(&self.device, &self.queue, &glyphs, &mut pass, globals);
        }

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bpr),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = self.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);

        let unpadded = self.unpadded_bpr as usize;
        let padded = self.padded_bpr as usize;
        let mut out = Vec::with_capacity(unpadded * self.height as usize);
        {
            let data = slice.get_mapped_range();
            for row in 0..self.height as usize {
                let start = row * padded;
                out.extend_from_slice(&data[start..start + unpadded]);
            }
        }
        self.readback.unmap();
        out
    }
}

// ---------------------------------------------------------------------------
// Native preview — a Sabitori window that plays a Scene like a video player.
//
// The preview *is* a Sabitori app: its `view()` returns the scene's current
// frame (a real `Element` tree, laid out on the GPU exactly like the offscreen
// render) stacked above a transport bar. A playhead clock, advanced in `tick`,
// makes it play; input scrubs it. This reuses the same rendering path as
// `render_pngs`, so what you preview is what you export.
// ---------------------------------------------------------------------------

/// Height of the transport bar drawn beneath the stage, in logical pixels.
const BAR_H: f32 = 56.0;

impl Reel {
    /// Open a native Sabitori window that plays `scene` in real time.
    ///
    /// Controls: **Space** toggles play/pause, **←/→** step one frame, click or
    /// drag the transport bar to scrub, click the stage to toggle play. The
    /// playhead loops at the end. Blocks until the window is closed.
    /// With the `three-d` feature the window runs through [`run_scene`], so the
    /// scene's [`Scene::stage`] is drawn by `seimei` underneath the overlay —
    /// the same two-layer composite the offscreen render produces.
    pub fn preview<S: Scene + 'static>(&self, scene: S) {
        let duration = if self.fps == 0 {
            0.0
        } else {
            self.frames as f32 / self.fps as f32
        };
        let app = PreviewApp {
            scene,
            width: self.width as f32,
            height: self.height as f32,
            fps: self.fps,
            frames: self.frames,
            duration,
            #[cfg(feature = "three-d")]
            clear: self.clear,
            playhead: 0.0,
            playing: true,
            dragging: false,
            mouse_x: 0.0,
            #[cfg(feature = "three-d")]
            gpu: None,
        };
        #[cfg(feature = "three-d")]
        run_scene(app);
        #[cfg(not(feature = "three-d"))]
        run_declarative(app);
    }
}

/// A [`DeclarativeApp`] that turns a [`Scene`] into a scrubbable player.
struct PreviewApp<S: Scene> {
    scene: S,
    width: f32,
    height: f32,
    fps: u32,
    frames: u32,
    duration: f32,
    /// Clear colour behind the scene, carried over from the [`Reel`] so the
    /// window and the export start from the same background. Only the 3D pass
    /// owns a clear; without it the overlay renderer clears for itself.
    #[cfg(feature = "three-d")]
    clear: [f64; 4],
    /// Playhead in seconds, kept in `[0, duration)`.
    playhead: f32,
    playing: bool,
    dragging: bool,
    /// Last known cursor x (logical px), so a scrub-bar click can seek to it.
    mouse_x: f32,
    /// The 3D stage, stood up on the *window's* device in [`SceneApp::setup`].
    /// Separate from the offscreen renderer's copy — a preview and an export
    /// never share a device.
    #[cfg(feature = "three-d")]
    gpu: Option<StageGpu>,
}

impl<S: Scene> PreviewApp<S> {
    fn current_frame(&self) -> u32 {
        ((self.playhead * self.fps as f32) as u32).min(self.frames.saturating_sub(1))
    }

    /// The frame the playhead is on, in the form both [`Scene::view`] and
    /// [`Scene::stage`] take — at the *reel* resolution, so the scene sees the
    /// same numbers it will see during an export.
    fn frame_ctx(&self) -> FrameCtx {
        FrameCtx {
            frame: self.current_frame(),
            fps: self.fps,
            width: self.width,
            height: self.height,
        }
    }

    /// Map a window-x (logical px) onto the timeline and seek there.
    fn seek_to_x(&mut self, x: f32) {
        let p = if self.width > 0.0 {
            (x / self.width).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.playhead = p * self.duration;
    }

    /// Pause and jump `delta` frames.
    fn step(&mut self, delta: i64) {
        self.playing = false;
        let cur = (self.playhead * self.fps as f32).round() as i64 + delta;
        let max = self.frames.saturating_sub(1) as i64;
        self.playhead = cur.clamp(0, max) as f32 / self.fps.max(1) as f32;
    }

    fn transport_bar(&self) -> Element {
        let progress = if self.duration > 0.0 {
            (self.playhead / self.duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fill_w = progress * self.width;
        let glyph = if self.playing { "❚❚ pause" } else { "▶ play" };
        let time = format!("{:.2}s / {:.2}s", self.playhead, self.duration);
        let frame = format!("frame {} / {}", self.current_frame(), self.frames);

        div()
            .id("scrub")
            .w(Px(self.width))
            .h(Px(BAR_H))
            .bg(Color::from_hex("#0b0e16"))
            .border(1.0, Color::from_hex("#1b2130"))
            .flex_col()
            .gap(10.0)
            .children([
                // Full-width scrub track with a teal fill showing progress.
                div()
                    .w(Px(self.width))
                    .h(Px(6.0))
                    .bg(Color::from_hex("#232a3a"))
                    .child(
                        div()
                            .w(Px(fill_w))
                            .h(Px(6.0))
                            .bg(Color::from_hex("#5eead4")),
                    ),
                div()
                    .flex_row()
                    .items_center()
                    .gap(16.0)
                    .p_px(12.0)
                    .children([
                        text(glyph)
                            .font_size(15.0)
                            .bold()
                            .color(Color::from_hex("#5eead4")),
                        text(time).font_size(14.0).color(Color::from_hex("#8b95ad")),
                        text(frame).font_size(14.0).color(Color::from_hex("#4b5568")),
                    ]),
            ])
    }
}

impl<S: Scene + 'static> DeclarativeApp for PreviewApp<S> {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let overlay = self.scene.view(self.frame_ctx());
        // The window is *exactly* the reel resolution and the transport bar is
        // laid over the foot of the frame, rather than the bar sitting below a
        // shorter stage. `render_scene` draws the 3D across the whole surface —
        // seimei has no viewport, so it always fills its attachment — and a
        // window even 56px taller than the reel would stretch that pass
        // vertically. Same window aspect, same image: what you preview stays
        // what you export. The bar covers y > height-56, which is below where
        // the copy sits.
        div()
            .w(Px(self.width))
            .h(Px(self.height))
            .children([
                div()
                    .id("stage")
                    .w(Px(self.width))
                    .h(Px(self.height))
                    .child(overlay),
                self.transport_bar().pos(0.0, self.height - BAR_H),
            ])
    }

    fn tick(&mut self, dt: f32) {
        if self.playing && self.duration > 0.0 {
            self.playhead += dt;
            while self.playhead >= self.duration {
                self.playhead -= self.duration;
            }
        }
    }

    fn is_animating(&self) -> bool {
        self.playing
    }

    fn on_input(&mut self, event: &InputEvent) -> bool {
        if let InputEvent::KeyInput {
            key, pressed: true, ..
        } = event
        {
            match key {
                Key::Space => {
                    self.playing = !self.playing;
                    return true;
                }
                Key::Left => {
                    self.step(-1);
                    return true;
                }
                Key::Right => {
                    self.step(1);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn on_pointer_move(&mut self, x: f32, _y: f32) {
        self.mouse_x = x;
        if self.dragging {
            self.seek_to_x(x);
        }
    }

    fn on_click(&mut self, id: &str) {
        match id {
            "scrub" => {
                self.playing = false;
                self.dragging = true;
                let x = self.mouse_x;
                self.seek_to_x(x);
            }
            "stage" => self.playing = !self.playing,
            _ => {}
        }
    }

    fn on_pointer_up(&mut self) {
        self.dragging = false;
    }

    fn title(&self) -> &str {
        "sabitori-reel preview"
    }

    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn min_size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    // 30fps reels don't need a 120Hz redraw; 60fps keeps scrubbing smooth and
    // sidesteps the ProMotion path. Playback speed is unaffected (playhead
    // advances by real `dt`).
    fn target_frame_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(16)
    }
}

/// The 3D half of the preview. `render_scene` runs before the overlay pass and
/// owns the clear, exactly as it does in the offscreen path — the runtime's UI
/// pass is `LoadOp::Load`, so whatever is drawn here survives underneath.
#[cfg(feature = "three-d")]
impl<S: Scene + 'static> SceneApp for PreviewApp<S> {
    fn setup(&mut self, ctx: &GpuContext) {
        let mut renderer = three::Renderer::new(
            ctx.device.clone(),
            ctx.queue.clone(),
            ctx.surface_format,
            ctx.surface_width,
            ctx.surface_height,
        )
        .expect("sabitori-reel: failed to create the seimei renderer");
        renderer
            .set_quality(self.scene.stage_quality())
            .expect("sabitori-reel: failed to apply 3D quality settings");
        self.scene.setup_stage(&mut renderer);

        let depth = stage_depth(
            &ctx.device,
            ctx.surface_width,
            ctx.surface_height,
            renderer.scene_sample_count(),
        );
        self.gpu = Some(StageGpu { renderer, depth });
    }

    fn on_resize(&mut self, ctx: &GpuContext) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.renderer.resize(ctx.surface_width, ctx.surface_height);
            gpu.depth = stage_depth(
                &ctx.device,
                ctx.surface_width,
                ctx.surface_height,
                gpu.renderer.scene_sample_count(),
            );
        }
    }

    fn render_scene(&mut self, ctx: &mut SceneRenderContext) {
        let clear = wgpu::Color {
            r: self.clear[0],
            g: self.clear[1],
            b: self.clear[2],
            a: self.clear[3],
        };
        let stage = self.scene.stage(self.frame_ctx());

        match (stage, self.gpu.as_mut()) {
            (Some(stage), Some(gpu)) => {
                gpu.renderer.set_clear_rgba(clear);
                let opaque = stage.opaque_count.unwrap_or(stage.instances.len());
                gpu.renderer.update_lines(&stage.lines);
                gpu.renderer.render_to_view(
                    ctx.encoder,
                    ctx.surface_view,
                    &gpu.depth,
                    &stage.camera,
                    &stage.instances,
                    opaque,
                );
            }
            // A frame with no stage still has to put something in the surface:
            // the overlay pass loads, it never clears, so skipping this would
            // leave the previous frame (or garbage) showing through a 2D reel.
            _ => {
                ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("reel_preview_clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: ctx.surface_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GPU が取れない環境 (CI の runner) では描く試験を飛ばす。
    fn gpu_available() -> bool {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let ok = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .is_some();
        if !ok {
            eprintln!("skip: GPU が無い");
        }
        ok
    }

    fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * width + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    /// `scale` は**組み方を変えずに**画素だけを増やす。40×30 の論理 px で組んだ
    /// 左右 2 色の箱が、2 倍なら 80×60 の画像の左右にそのまま出る。
    #[test]
    fn scale_renders_the_same_layout_at_more_pixels() {
        if !gpu_available() {
            return;
        }
        let root = div()
            .w(Px(40.0))
            .h(Px(30.0))
            .flex_row()
            .child(div().w(Px(20.0)).h(Px(30.0)).bg(Color::from_hex("#ff0000")))
            .child(div().w(Px(20.0)).h(Px(30.0)).bg(Color::from_hex("#0000ff")));
        let mut gpu = Gpu::new(40, 30, 2.0, &[], None);
        assert_eq!((gpu.width, gpu.height), (80, 60));
        let rgba = gpu.render_frame(&root, [0.0, 0.0, 0.0, 1.0], Default::default());
        assert_eq!(rgba.len(), 80 * 60 * 4);

        let left = pixel(&rgba, 80, 20, 30);
        let right = pixel(&rgba, 80, 60, 30);
        assert!(left[0] > 200 && left[2] < 60, "左半分が赤でない: {left:?}");
        assert!(right[2] > 200 && right[0] < 60, "右半分が青でない: {right:?}");
    }

    /// 開始色が透明なグラデーション (動画の文字の下に敷くスクリム) が描かれる。
    /// 落ちると、画面の下を暗くする帯が黙って消える。
    #[test]
    fn a_scrim_from_transparent_darkens_the_bottom() {
        if !gpu_available() {
            return;
        }
        let root = div().w(Px(64.0)).h(Px(64.0)).bg(Color::WHITE).child(
            div()
                .w(Px(64.0))
                .h(Px(64.0))
                .gradient(Color::TRANSPARENT, Color::BLACK, std::f32::consts::FRAC_PI_2),
        );
        let mut gpu = Gpu::new(64, 64, 1.0, &[], None);
        let rgba = gpu.render_frame(&root, [1.0, 1.0, 1.0, 1.0], Default::default());
        let top = pixel(&rgba, 64, 32, 2)[0];
        let bottom = pixel(&rgba, 64, 32, 61)[0];
        assert!(bottom + 100 < top, "下が暗くなっていない (上 {top}, 下 {bottom})");
    }
}
