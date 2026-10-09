use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::context::{GpuContext, SceneRenderContext};
use crate::goo_renderer::{GooRenderer, GooSlot};
use crate::instance::{GooInstance, RectInstance};

/// Pick the surface composite alpha mode.
///
/// A normal (opaque) window MUST use `Opaque` so the OS compositor ignores the
/// framebuffer alpha channel. If we hand it `PreMultiplied`/`PostMultiplied`
/// instead, the platform layer becomes non-opaque and the whole window blends
/// against the desktop wherever the final alpha < 1.0 — i.e. a see-through
/// window. Only apps that explicitly opt into a transparent window
/// (`App::transparent() == true`, which sets `with_transparent(true)`) want a
/// premultiplied mode.
///
/// This is pure so it can be unit-tested without a GPU/surface — see the tests
/// at the bottom of this module.
pub(crate) fn choose_alpha_mode(
    available: &[wgpu::CompositeAlphaMode],
    transparent: bool,
) -> wgpu::CompositeAlphaMode {
    use wgpu::CompositeAlphaMode::{Opaque, PostMultiplied, PreMultiplied};
    if !transparent && available.contains(&Opaque) {
        Opaque
    } else if available.contains(&PreMultiplied) {
        PreMultiplied
    } else if available.contains(&PostMultiplied) {
        PostMultiplied
    } else {
        available[0]
    }
}

/// The limits sabitori genuinely needs from a GPU, whatever the platform
/// baseline says.
///
/// * 2048² is the glyph atlas ([`sabitori_text`]).
/// * rect.wgsl passes 34 inter-stage components — above the WebGL2 baseline's
///   own default of 31, so this one has to be asked for explicitly.
const MINIMUM_LIMITS: &[(&str, u32, fn(&wgpu::Limits) -> u32)] = &[
    ("max_texture_dimension_2d", 2048, |l| l.max_texture_dimension_2d),
    ("max_inter_stage_shader_components", 34, |l| {
        l.max_inter_stage_shader_components
    }),
];

/// Decide what to pass as `required_limits`, given the baseline we would like.
///
/// `request_device` treats these limits as *requirements*: if the adapter
/// reports less than any one of them, device creation fails outright. Both
/// baselines we start from claim headroom sabitori never uses —
/// `downlevel_webgl2_defaults()` asks for `max_color_attachments = 8` where
/// SwiftShader offers 6, and `Limits::default()` asks for an 8192² texture
/// where older integrated GPUs stop at 4096 — so asking for the baseline
/// wholesale means "this device cannot start sabitori at all" for reasons that
/// have nothing to do with what we draw (#72).
///
/// So: keep the baseline when the adapter can meet it (unchanged behaviour on
/// capable hardware, and the baseline still guards against quietly relying on
/// a strong GPU's headroom), and otherwise fall back to exactly what the
/// adapter reports, naming what came up short in the log.
fn resolve_limits(adapter: &wgpu::Adapter, baseline: wgpu::Limits) -> wgpu::Limits {
    let available = adapter.limits();

    if let Some(short) = first_unmet_minimum(&available) {
        let info = adapter.get_info();
        panic!(
            "This GPU cannot run sabitori: it reports {} = {}, and sabitori needs at least {} \
             (adapter: {} / {:?} / {:?}).",
            short.name,
            short.have,
            short.needed,
            info.name,
            info.device_type,
            info.backend,
        );
    }

    pick_limits(baseline, available)
}

/// A limit sabitori needs that the adapter does not offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnmetLimit {
    pub name: &'static str,
    pub needed: u32,
    pub have: u32,
}

/// First entry of [`MINIMUM_LIMITS`] the adapter cannot satisfy, if any.
///
/// Pure so it can be unit-tested without a GPU — see the tests at the bottom
/// of this module.
/// GPU を用意できなかった理由 ([#82])。
///
/// wasm では実際に起きる (WebGL2 も WebGPU も無い、上限が足りない)。panic に
/// すると canvas が真っ白なまま console にしか出ないので、**画面に出せる形**で
/// 返す。`Display` はそのまま利用者に見せられる日本語。
///
/// [#82]: https://github.com/Mutafika/sabitori/issues/82
#[derive(Debug)]
pub enum GpuInitError {
    /// 描画面 (surface) を作れなかった。canvas が無い / WebGL2 も WebGPU も無い。
    NoSurface(String),
    /// アダプタが 1 つも見つからない。
    NoAdapter,
    /// デバイスを作れなかった。`unmet` があれば、足りなかった上限の名前と値。
    DeviceRejected {
        unmet: Option<UnmetLimit>,
        source: String,
    },
}

impl std::fmt::Display for GpuInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSurface(e) => write!(
                f,
                "描画面を作れませんでした。この環境では WebGL2 / WebGPU が使えない可能性があります ({e})"
            ),
            Self::NoAdapter => write!(
                f,
                "GPU が見つかりませんでした。ハードウェアアクセラレーションが切られているか、この環境では使えません"
            ),
            Self::DeviceRejected { unmet: Some(u), .. } => write!(
                f,
                "GPU の上限が足りません: {} は {} 必要ですが、この環境は {} です",
                u.name, u.needed, u.have
            ),
            Self::DeviceRejected { unmet: None, source } => {
                write!(f, "GPU デバイスを作れませんでした ({source})")
            }
        }
    }
}

impl std::error::Error for GpuInitError {}

pub(crate) fn first_unmet_minimum(available: &wgpu::Limits) -> Option<UnmetLimit> {
    MINIMUM_LIMITS.iter().find_map(|(name, needed, get)| {
        let have = get(available);
        (have < *needed).then_some(UnmetLimit {
            name,
            needed: *needed,
            have,
        })
    })
}

/// The baseline when the adapter can meet it, otherwise the adapter's own
/// limits (and a log line naming every limit that came up short).
///
/// Pure so it can be unit-tested without a GPU.
pub(crate) fn pick_limits(baseline: wgpu::Limits, available: wgpu::Limits) -> wgpu::Limits {
    if baseline.check_limits(&available) {
        return baseline;
    }

    baseline.check_limits_with_fail_fn(&available, false, |name, wanted, allowed| {
        tracing::warn!(
            "GPU below the baseline limit {name} (baseline wants {wanted}, adapter has {allowed}); \
             requesting the adapter's own limits instead"
        );
    });
    available
}

/// Identifies which phase of layered rendering the draw callback is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderPhase {
    /// Draw base-layer text (after base rects, before overlay rects).
    BaseText,
    /// Draw overlay-layer text (after overlay rects).
    OverlayText,
}

/// Uniform buffer matching the `Globals` struct in rect.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    screen_size: [f32; 2],
    scale_factor: f32,
    _pad: f32,
}

/// Where one uploaded goo instance is drawn: before rect `at` (index into
/// the shared rect instance buffer), as goo buffer entry `index`.
#[derive(Clone, Copy, Debug)]
struct GooMark {
    at: u32,
    index: u32,
}

pub struct GpuRenderer {
    /// 直近の `get_current_texture` で描画先を待った時間 (ms)。フレームの計測用。
    pub last_acquire_ms: f32,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub rect_pipeline: wgpu::RenderPipeline,
    pub globals_buffer: wgpu::Buffer,
    pub globals_bind_group: wgpu::BindGroup,
    pub globals_bind_group_layout: wgpu::BindGroupLayout,
    pub instance_buffer: wgpu::Buffer,
    pub instance_capacity: usize,
    pub scale_factor: f32,
    /// Optional depth texture, created when a SceneApp requests depth testing.
    pub depth_texture: Option<wgpu::Texture>,
    pub depth_view: Option<wgpu::TextureView>,
    pub depth_format: wgpu::TextureFormat,
    /// Goo (smooth-union backgrounds) — drawn between rects, see [`GooRenderer`].
    goo_renderer: GooRenderer,
    /// Goo queued by [`GpuRenderer::set_goo`] for the next render call.
    /// 層ごと (`set_goo` なら [地, 上掛け]、`set_goo_layers` なら描く層の数だけ)。
    pending_goo: Vec<Vec<GooSlot>>,
    /// 次に描くフレームを読み戻す ([`GpuRenderer::request_capture`])。
    capture_pending: bool,
    /// 読み戻したフレーム ([`GpuRenderer::take_captured`] で取り出す)。
    captured: Option<CapturedFrame>,
}

/// 読み戻した 1 フレーム (RGBA8、上から下)。
///
/// **窓の枠は入らない** — 描画面そのもの。`SABITORI_SCREENSHOT` で使う
/// ([#69](https://github.com/Mutafika/sabitori/issues/69))。
#[derive(Clone, Debug)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// このプロセスはフレームを読み戻す予定があるか (`SABITORI_SCREENSHOT`)。
///
/// サーフェスの usage は**設定時に決まる**ので、起動時に知る必要がある。
pub fn capture_wanted() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var_os("SABITORI_SCREENSHOT").is_some()
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

/// サーフェスのフォーマットを選ぶ。sRGB を最優先する。
///
/// `Color` は linear を保持していて、sRGB サーフェスのハードウェアエンコードに
/// 依存している（`sabitori_core::Color` の doc を参照）。つまり sRGB を掴めないと、
/// linear がそのまま UNORM へ書かれて画面全体が明るく飛ぶ。
///
/// 現状フォールバックを止める手立ては無い（そのフォーマットしか無いのだから）。
/// せめて警告を出す：全部の色が同時におかしくなるので、これが無いと
/// 「どのコンポーネントのバグか」の切り分けすらできない。
fn pick_surface_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    if let Some(f) = caps.formats.iter().find(|f| f.is_srgb()) {
        return *f;
    }
    let fallback = caps.formats[0];
    tracing::warn!(
        "sRGB のサーフェスフォーマットが無い。{:?} を使うが、色は linear のまま \
         書かれるので全体が明るく飛ぶ。利用可能: {:?}",
        fallback,
        caps.formats,
    );
    fallback
}

impl GpuRenderer {
    /// Create a new GpuRenderer (native/desktop path).
    ///
    /// On WASM, use `GpuRenderer::new_async()` instead.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(window: Arc<winit::window::Window>) -> Self {
        Self::new_with_alpha(window, false)
    }

    /// Like [`new`](Self::new), but lets the caller request a transparent
    /// window surface. Pass `transparent = true` ONLY for windows created with
    /// `with_transparent(true)` (i.e. `App::transparent() == true`); otherwise
    /// the compositor blends the whole window against the desktop wherever the
    /// framebuffer alpha < 1.0.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_with_alpha(window: Arc<winit::window::Window>, transparent: bool) -> Self {
        pollster::block_on(Self::new_async_with_alpha(window, transparent))
    }

    /// Async initialization — works on both native and WASM. Defaults to an
    /// opaque surface; use [`new_async_with_alpha`](Self::new_async_with_alpha)
    /// for transparent windows.
    pub async fn new_async(window: Arc<winit::window::Window>) -> Self {
        Self::new_async_with_alpha(window, false).await
    }

    /// Async initialization with explicit surface transparency. See
    /// [`new_with_alpha`](Self::new_with_alpha) for when to pass `transparent`.
    pub async fn new_async_with_alpha(
        window: Arc<winit::window::Window>,
        transparent: bool,
    ) -> Self {
        match Self::try_new_async_with_alpha(window, transparent).await {
            Ok(r) => r,
            Err(e) => panic!("{e}"),
        }
    }

    /// [`GpuRenderer::new_async_with_alpha`] の、**落ちずに理由を返す**版
    /// ([#82](https://github.com/Mutafika/sabitori/issues/82))。
    ///
    /// wasm では「GPU が用意できない」が実際に起きる (WebGL2 も WebGPU も無い、
    /// 上限が足りない)。panic だと canvas が真っ白なまま console にしか出ないので、
    /// **画面にメッセージを出せる形**で返す。[`GpuInitError`] の `Display` は
    /// そのまま利用者に見せられる。
    pub async fn try_new_async_with_alpha(
        window: Arc<winit::window::Window>,
        transparent: bool,
    ) -> Result<Self, GpuInitError> {
        // iOS の `inner_size` はセーフエリア（ステータスバー等を除いた部分）で、描画面は窓全体
        // （`outer_size`）を覆う。inner で組むと最初の `Resized` が来るまで縦に伸びて描かれる。
        #[cfg(target_os = "ios")]
        let size = window.outer_size();
        #[cfg(not(target_os = "ios"))]
        let size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;

        #[cfg(target_arch = "wasm32")]
        let backends = wgpu::Backends::GL;
        #[cfg(not(target_arch = "wasm32"))]
        let backends = wgpu::Backends::all();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window)
            .map_err(|e| GpuInitError::NoSurface(e.to_string()))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(GpuInitError::NoAdapter)?;

        tracing::info!("GPU: {}", adapter.get_info().name);

        // Use downlevel limits on WASM for broader compatibility
        #[cfg(target_arch = "wasm32")]
        let baseline = {
            let mut l = wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter.limits());
            // rect.wgsl passes 34 inter-stage components; the conservative
            // webgl2 default caps this at 31, which fails rect_pipeline
            // validation. Raise to what the adapter actually reports
            // (desktop WebGL2 gives >= 60).
            l.max_inter_stage_shader_components =
                adapter.limits().max_inter_stage_shader_components;
            l
        };
        #[cfg(not(target_arch = "wasm32"))]
        let baseline = wgpu::Limits::default();
        let required_limits = resolve_limits(&adapter, baseline);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("sabitori_device"),
                    required_features: wgpu::Features::empty(),
                    required_limits,
                    ..Default::default()
                },
                None,
            )
            .await
            .map_err(|e| GpuInitError::DeviceRejected {
                unmet: first_unmet_minimum(&adapter.limits()),
                source: e.to_string(),
            })?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Surface configuration
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = pick_surface_format(&surface_caps);

        // Same max-dimension clamp as `resize`. Initial surface
        // creation typically falls within bounds, but cheap defensive
        // capping here keeps init / resize behavior identical and
        // catches any edge case (e.g. a display configured at
        // 8K + retina on a Metal device whose default limit is 8192).
        let max_dim = device.limits().max_texture_dimension_2d;
        // present_mode は Mailbox → Immediate → AutoVsync の順で選ぶ。
        // AutoVsync(Fifo) はディスプレイのリフレッシュ(例:60Hz)に蓋されて 120Hz パネルでも 60fps で
        // 頭打ちになる。Mailbox 不在の機種(Metal は報告が不安定)では Immediate に落として上限を外す
        // ＝高リフレッシュ環境で本来の fps を出す。両方無ければ従来どおり AutoVsync。
        let present_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else if surface_caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else {
            wgpu::PresentMode::AutoVsync
        };
        // 調査用の上書き: SABITORI_PRESENT_MODE=fifo|immediate|mailbox|autovsync
        let present_mode = match std::env::var("SABITORI_PRESENT_MODE").ok().as_deref() {
            Some("fifo") => wgpu::PresentMode::Fifo,
            Some("immediate") => wgpu::PresentMode::Immediate,
            Some("mailbox") => wgpu::PresentMode::Mailbox,
            Some("autovsync") => wgpu::PresentMode::AutoVsync,
            _ => present_mode,
        };
        // 調査用の上書き: SABITORI_FRAME_LATENCY=n (同時に描きかけでいられるフレーム数)
        let frame_latency: u32 =
            std::env::var("SABITORI_FRAME_LATENCY").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        tracing::info!("present_mode: {:?}, frame latency {}", present_mode, frame_latency);
        // 読み戻す予定があるときだけ COPY_SRC を足す (#69)。常に足さないのは、
        // WebGL2 のサーフェスがコピー元になれないため — wasm で無条件に付けると
        // **起動できない環境が出る**。
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if capture_wanted() {
            usage |= wgpu::TextureUsages::COPY_SRC;
        }
        let surface_config = wgpu::SurfaceConfiguration {
            usage,
            format: surface_format,
            width: size.width.max(1).min(max_dim),
            height: size.height.max(1).min(max_dim),
            present_mode,
            desired_maximum_frame_latency: frame_latency,
            alpha_mode: choose_alpha_mode(&surface_caps.alpha_modes, transparent),
            view_formats: vec![],
        };
        surface.configure(&device, &surface_config);

        // Globals uniform buffer
        let logical_size = [
            size.width as f32 / scale_factor,
            size.height as f32 / scale_factor,
        ];
        let globals = Globals {
            screen_size: logical_size,
            scale_factor,
            _pad: 0.0,
        };
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globals_buffer"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let globals_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("globals_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals_bind_group"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        // Shader
        let shader_source = include_str!("../../../shaders/rect.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        // Pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect_pipeline_layout"),
            bind_group_layouts: &[&globals_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Rect pipeline
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[RectInstance::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Instance buffer (pre-allocate for 256 rects)
        let instance_capacity = 256;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect_instance_buffer"),
            size: (instance_capacity * std::mem::size_of::<RectInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let goo_renderer = GooRenderer::new(&device, surface_config.format, &globals_bind_group_layout);

        Ok(Self {
            last_acquire_ms: 0.0,
            device,
            queue,
            surface,
            surface_config,
            rect_pipeline,
            globals_buffer,
            globals_bind_group,
            globals_bind_group_layout,
            instance_buffer,
            instance_capacity,
            scale_factor,
            depth_texture: None,
            depth_view: None,
            depth_format: wgpu::TextureFormat::Depth32Float,
            goo_renderer,
            pending_goo: Vec::new(),
            capture_pending: false,
            captured: None,
        })
    }

    /// プラグインウィンドウ等の外部ハンドルから GpuRenderer を生成。
    /// winit を使わず、wgpu の unsafe surface 生成を行う。
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_from_raw(
        surface_target: wgpu::SurfaceTargetUnsafe,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> Self {
        pollster::block_on(Self::new_from_raw_async(surface_target, width, height, scale_factor))
    }

    /// Async 版の raw handle 初期化。
    pub async fn new_from_raw_async(
        surface_target: wgpu::SurfaceTargetUnsafe,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> Self {
        let backends = wgpu::Backends::all();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        // SAFETY: 呼び出し元が有効なウィンドウハンドルを保証する
        let surface = unsafe {
            instance
                .create_surface_unsafe(surface_target)
                .expect("Failed to create surface from raw handle")
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find a suitable GPU adapter");

        let required_limits = resolve_limits(&adapter, wgpu::Limits::default());

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("sabitori_device"),
                    required_features: wgpu::Features::empty(),
                    required_limits,
                    ..Default::default()
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = pick_surface_format(&surface_caps);

        let max_dim = device.limits().max_texture_dimension_2d;
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if capture_wanted() {
            usage |= wgpu::TextureUsages::COPY_SRC;
        }
        let surface_config = wgpu::SurfaceConfiguration {
            usage,
            format: surface_format,
            width: width.max(1).min(max_dim),
            height: height.max(1).min(max_dim),
            present_mode: if surface_caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
                wgpu::PresentMode::Mailbox
            } else {
                wgpu::PresentMode::AutoVsync
            },
            desired_maximum_frame_latency: 1,
            // Raw-surface path has no `transparent()` signal; default to opaque
            // (correct for normal windows). Add a `_with_alpha` variant if a
            // transparent raw surface is ever needed.
            alpha_mode: choose_alpha_mode(&surface_caps.alpha_modes, false),
            view_formats: vec![],
        };
        surface.configure(&device, &surface_config);

        let logical_size = [
            width as f32 / scale_factor,
            height as f32 / scale_factor,
        ];
        let globals = Globals {
            screen_size: logical_size,
            scale_factor,
            _pad: 0.0,
        };
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globals_buffer"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let globals_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("globals_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals_bind_group"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let shader_source = include_str!("../../../shaders/rect.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect_pipeline_layout"),
            bind_group_layouts: &[&globals_bind_group_layout],
            push_constant_ranges: &[],
        });

        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[RectInstance::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let instance_capacity = 256;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect_instance_buffer"),
            size: (instance_capacity * std::mem::size_of::<RectInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let goo_renderer = GooRenderer::new(&device, surface_config.format, &globals_bind_group_layout);

        Self {
            last_acquire_ms: 0.0,
            device,
            queue,
            surface,
            surface_config,
            rect_pipeline,
            globals_buffer,
            globals_bind_group,
            globals_bind_group_layout,
            instance_buffer,
            instance_capacity,
            scale_factor,
            depth_texture: None,
            depth_view: None,
            depth_format: wgpu::TextureFormat::Depth32Float,
            goo_renderer,
            pending_goo: Vec::new(),
            capture_pending: false,
            captured: None,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f64) {
        if width == 0 || height == 0 {
            return;
        }
        // Clamp to the device's max texture dimension. macOS occasionally
        // forwards inflated physical sizes during sleep / wake / scale
        // factor swaps (e.g. a 5120×2160 surface re-reported at
        // 10240×4320 = 2× backing scale), which exceeds Metal's
        // 8192-pixel limit on M-series GPUs and would panic
        // `Surface::configure` with a validation error.
        let max_dim = self.device.limits().max_texture_dimension_2d;
        let width = width.min(max_dim);
        let height = height.min(max_dim);
        self.scale_factor = scale_factor as f32;
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);

        let logical_size = [
            width as f32 / self.scale_factor,
            height as f32 / self.scale_factor,
        ];
        let globals = Globals {
            screen_size: logical_size,
            scale_factor: self.scale_factor,
            _pad: 0.0,
        };
        self.queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        // Recreate depth texture if it was previously created
        if self.depth_texture.is_some() {
            self.create_depth_texture();
        }
    }

    /// Acquire the next drawable, retrying once on `Outdated`/`Lost`, then
    /// reconcile our sizing to the texture we actually got.
    ///
    /// On macOS the CAMetalLayer can resize its drawable *before* winit
    /// delivers the matching `Resized` event, so for one frame the color
    /// target (this drawable) and the depth target (sized from
    /// `surface_config`) disagree. A render pass with mismatched attachment
    /// sizes is a wgpu validation error, and with no uncaptured-error handler
    /// installed that takes the whole process down — i.e. the app crashes mid
    /// window-resize. Reconciling here keeps color + depth (and the globals
    /// uniform) in lockstep with the real drawable for every frame.
    /// **次に描くフレームを読み戻す** ([#69])。
    ///
    /// 読み戻せるのは、起動時に `SABITORI_SCREENSHOT` が立っていたときだけ
    /// (サーフェスの usage は設定時に決まるため)。立っていなければ何も起きない。
    ///
    /// [#69]: https://github.com/Mutafika/sabitori/issues/69
    pub fn request_capture(&mut self) {
        self.capture_pending = capture_wanted();
    }

    /// 読み戻したフレームを取り出す (1 回だけ返る)。
    pub fn take_captured(&mut self) -> Option<CapturedFrame> {
        self.captured.take()
    }

    /// 描き終わったサーフェスを読み戻す。`present()` の**前**に呼ぶこと。
    ///
    /// wgpu は行の先頭を 256 バイトに揃えることを要求するので、幅によっては
    /// 余白が挟まる。揃えずに読むと画像が斜めにずれる。サーフェスは
    /// BGRA のことがあるので、その場合は並べ替えて RGBA で返す。
    fn capture_if_requested(&mut self, texture: &wgpu::Texture) {
        if !self.capture_pending {
            return;
        }
        self.capture_pending = false;

        let (width, height) = (texture.width(), texture.height());
        const ALIGN: u32 = 256;
        let unpadded = width * 4;
        let padded = unpadded.div_ceil(ALIGN) * ALIGN;

        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sabitori_capture"),
            size: (padded * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::Maintain::Wait);
        if !matches!(rx.recv(), Ok(Ok(()))) {
            tracing::warn!("フレームを読み戻せなかった");
            return;
        }

        let bgra = matches!(
            self.surface_config.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        let mapped = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity((unpadded * height) as usize);
        for row in 0..height {
            let start = (row * padded) as usize;
            let line = &mapped[start..start + unpadded as usize];
            if bgra {
                for px in line.chunks_exact(4) {
                    rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
                }
            } else {
                rgba.extend_from_slice(line);
            }
        }
        drop(mapped);
        buffer.unmap();

        self.captured = Some(CapturedFrame { width, height, rgba });
    }

    fn acquire_drawable(&mut self) -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError> {
        // wasm には std の Instant が無いので計らない (0 のまま)
        #[cfg(not(target_arch = "wasm32"))]
        let t0 = std::time::Instant::now();
        let output = match self.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.surface_config);
                self.surface.get_current_texture()?
            }
            Err(e) => return Err(e),
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.last_acquire_ms = t0.elapsed().as_secs_f32() * 1000.0;
        }
        self.sync_to_drawable(&output);
        Ok(output)
    }

    /// Align `surface_config`, the globals uniform, and the depth texture to the
    /// actual drawable size. No-op when they already match (the common case).
    fn sync_to_drawable(&mut self, output: &wgpu::SurfaceTexture) {
        let (tw, th) = (output.texture.width(), output.texture.height());
        if tw == self.surface_config.width && th == self.surface_config.height {
            return;
        }
        self.surface_config.width = tw;
        self.surface_config.height = th;
        let logical_size = [tw as f32 / self.scale_factor, th as f32 / self.scale_factor];
        let globals = Globals {
            screen_size: logical_size,
            scale_factor: self.scale_factor,
            _pad: 0.0,
        };
        self.queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
        if self.depth_texture.is_some() {
            self.create_depth_texture();
        }
    }

    /// Queue goo for the next render call, per layer (base, overlay). Each
    /// slot's `before_rect` indexes into the rect slice of its own layer as
    /// passed to that call. Consumed by the call; single-layer render
    /// functions (`render_with`, `render_scene_then_ui`) draw only `base`.
    pub fn set_goo(&mut self, base: Vec<GooSlot>, overlay: Vec<GooSlot>) {
        self.pending_goo = vec![base, overlay];
    }

    /// [`GpuRenderer::render_layers`] 用: 層ごとの goo (`layers[i]` が層 i)。
    /// 各 `before_rect` はその層の矩形の並びの中の位置。
    pub fn set_goo_layers(&mut self, layers: Vec<Vec<GooSlot>>) {
        self.pending_goo = layers;
    }

    /// Upload the queued goo and return each layer's draw marks, with
    /// overlay positions shifted by `overlay_rect_base` into the shared
    /// rect buffer's index space.
    fn upload_goo(&mut self, overlay_rect_base: u32) -> (Vec<GooMark>, Vec<GooMark>) {
        let mut marks = self.upload_goo_layers(&[0, overlay_rect_base]).into_iter();
        (marks.next().unwrap_or_default(), marks.next().unwrap_or_default())
    }

    /// 積んである goo を上げ、層ごとの描く位置を返す。`rect_bases[i]` が層 i の矩形が
    /// 共有バッファのどこから始まるか。返す `Vec` は層の数だけ (goo の無い層は空)。
    fn upload_goo_layers(&mut self, rect_bases: &[u32]) -> Vec<Vec<GooMark>> {
        let layers = std::mem::take(&mut self.pending_goo);
        let (instances, marks) = goo_marks(&layers, rect_bases);
        if !instances.is_empty() {
            self.goo_renderer.upload(&self.device, &self.queue, &instances);
        }
        marks
    }

    /// Draw rect instances `range` from the shared instance buffer,
    /// switching to the goo pipeline at each mark so goo paints in tree
    /// order between the rects around it.
    fn draw_rects(&self, pass: &mut wgpu::RenderPass<'_>, range: std::ops::Range<u32>, goo: &[GooMark]) {
        let mut cur = range.start;
        for mark in goo {
            let at = mark.at.clamp(cur, range.end);
            self.draw_rect_run(pass, cur..at);
            cur = at;
            self.goo_renderer.draw(pass, &self.globals_bind_group, mark.index..mark.index + 1);
        }
        self.draw_rect_run(pass, cur..range.end);
    }

    fn draw_rect_run(&self, pass: &mut wgpu::RenderPass<'_>, range: std::ops::Range<u32>) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.rect_pipeline);
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, range);
    }

    pub fn render(&mut self, rects: &[RectInstance]) -> Result<(), wgpu::SurfaceError> {
        self.render_with(rects, |_, _| {})
    }

    /// Render rectangles, then call `extra_draw` with the render pass for additional drawing
    /// (e.g., text glyphs).
    pub fn render_with(
        &mut self,
        rects: &[RectInstance],
        extra_draw: impl FnOnce(&mut wgpu::RenderPass<'_>, &wgpu::BindGroup),
    ) -> Result<(), wgpu::SurfaceError> {
        let count = rects.len();

        // Grow instance buffer if needed
        if count > self.instance_capacity {
            self.instance_capacity = count.max(1).next_power_of_two();
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rect_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<RectInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        if count > 0 {
            // Upload instance data
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(rects));
        }

        // Single-layer path: there is no overlay layer to put overlay goo in.
        let (goo_base, _) = self.upload_goo(0);
        let output = self.acquire_drawable()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sabitori_encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sabitori_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Draw rects
            self.draw_rects(&mut pass, 0..count as u32, &goo_base);

            // Draw extra (text, etc.)
            extra_draw(&mut pass, &self.globals_bind_group);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.capture_if_requested(&output.texture);
        output.present();

        Ok(())
    }

    /// 層を下から順に描く (層ごとに 1 回出す: 文字・画像などの書き込みは 1 回の提出に 1 回しか効かないため)。
    /// `layers[i]` は (その層の矩形, 矩形以外も描くか)。`draw_fn(i, ..)` が層 i の矩形以外を描く。
    /// 層 0 は必ず描く (画面を消す)。goo は [`GpuRenderer::set_goo_layers`] で層ごとに渡す
    /// (`set_goo` で渡すと、上掛けの分は層 1 に描かれる)。
    pub fn render_layers(
        &mut self,
        layers: &[(&[RectInstance], bool)],
        mut draw_fn: impl FnMut(usize, &mut wgpu::RenderPass<'_>, &wgpu::BindGroup),
    ) -> Result<(), wgpu::SurfaceError> {
        let total: usize = layers.iter().map(|(r, _)| r.len()).sum();
        if total > self.instance_capacity {
            self.instance_capacity = total.max(1).next_power_of_two();
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rect_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<RectInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        // 全部の層の矩形を 1 本のバッファに並べる (starts[i] が層 i の先頭)
        let mut starts = Vec::with_capacity(layers.len());
        let mut at = 0usize;
        for (rects, _) in layers {
            starts.push(at);
            if !rects.is_empty() {
                let offset = (at * std::mem::size_of::<RectInstance>()) as u64;
                self.queue.write_buffer(&self.instance_buffer, offset, bytemuck::cast_slice(rects));
            }
            at += rects.len();
        }
        let bases: Vec<u32> = starts.iter().map(|&s| s as u32).collect();
        let goo_marks = self.upload_goo_layers(&bases);
        let output = self.acquire_drawable()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        for (i, (rects, has_content)) in layers.iter().enumerate() {
            let goo: &[GooMark] = &goo_marks[i];
            if i > 0 && rects.is_empty() && !has_content && goo.is_empty() {
                continue;
            }
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sabitori_layer_encoder"),
            });
            {
                let load = if i == 0 { wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }) } else { wgpu::LoadOp::Load };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sabitori_layer_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                let s = starts[i] as u32;
                self.draw_rects(&mut pass, s..s + rects.len() as u32, goo);
                draw_fn(i, &mut pass, &self.globals_bind_group);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }
        self.capture_if_requested(&output.texture);
        output.present();
        Ok(())
    }

    /// Render with two layers: base and overlay.
    ///
    /// Draw order within a single render pass:
    ///   1. base rects (instanced draw)
    ///   2. caller draws base text (via `draw_fn`, phase `RenderPhase::BaseText`)
    ///   3. overlay rects (instanced draw, same pipeline)
    ///   4. caller draws overlay text (via `draw_fn`, phase `RenderPhase::OverlayText`)
    ///
    /// Both rect slices are uploaded to the same instance buffer (overlay
    /// appended after base) so only one buffer is needed.
    ///
    /// The `draw_fn` closure is called twice with different [`RenderPhase`]
    /// values, so the caller can use a single `&mut TextRenderer` without
    /// borrow-checker issues.
    /// `overlay_has_content` says whether the overlay layer draws anything
    /// *other than* rects — images, rings, lines, glyphs. It cannot be derived
    /// here: those are drawn by `draw_fn`, which is opaque to the renderer.
    ///
    /// Gating the overlay pass on `overlay_rects` alone loses whole layers.
    /// An undecorated `div` emits no rect, so an overlay holding only an image
    /// (a drag ghost) or only text never opened its pass and vanished — with
    /// every other signal, hit regions and callbacks included, still correct.
    /// Tooltips and context menus survived only because both happen to set a
    /// background ([#44](https://github.com/Mutafika/sabitori/issues/44)).
    pub fn render_layered(
        &mut self,
        base_rects: &[RectInstance],
        overlay_rects: &[RectInstance],
        overlay_has_content: bool,
        mut draw_fn: impl FnMut(RenderPhase, &mut wgpu::RenderPass<'_>, &wgpu::BindGroup),
    ) -> Result<(), wgpu::SurfaceError> {
        let base_count = base_rects.len();
        let overlay_count = overlay_rects.len();
        let total_count = base_count + overlay_count;

        // Grow instance buffer if needed
        if total_count > self.instance_capacity {
            self.instance_capacity = total_count.max(1).next_power_of_two();
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rect_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<RectInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Upload base rects
        if base_count > 0 {
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(base_rects));
        }

        // Upload overlay rects (appended after base)
        if overlay_count > 0 {
            let offset = (base_count * std::mem::size_of::<RectInstance>()) as u64;
            self.queue
                .write_buffer(&self.instance_buffer, offset, bytemuck::cast_slice(overlay_rects));
        }

        let (goo_base, goo_overlay) = self.upload_goo(base_count as u32);
        let output = self.acquire_drawable()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Pass 1: base layer — submit immediately so glyph buffer writes are flushed
        {
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sabitori_base_encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sabitori_base_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0, g: 0.0, b: 0.0, a: 0.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                self.draw_rects(&mut pass, 0..base_count as u32, &goo_base);

                draw_fn(RenderPhase::BaseText, &mut pass, &self.globals_bind_group);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // Pass 2: overlay layer — separate encoder + submit
        if overlay_count > 0 || overlay_has_content || !goo_overlay.is_empty() {
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sabitori_overlay_encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sabitori_overlay_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                self.draw_rects(&mut pass, base_count as u32..(base_count + overlay_count) as u32, &goo_overlay);

                draw_fn(RenderPhase::OverlayText, &mut pass, &self.globals_bind_group);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        self.capture_if_requested(&output.texture);
        output.present();

        Ok(())
    }

    /// Create (or recreate) the depth texture matching the current surface size.
    pub fn create_depth_texture(&mut self) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sabitori_depth"),
            size: wgpu::Extent3d {
                width: self.surface_config.width.max(1),
                height: self.surface_config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.depth_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.depth_texture = Some(texture);
        self.depth_view = Some(view);
    }

    /// Build a GpuContext snapshot for passing to SceneApp lifecycle methods.
    pub fn gpu_context(&self) -> GpuContext {
        GpuContext {
            device: self.device.clone(),
            queue: self.queue.clone(),
            surface_format: self.surface_config.format,
            depth_format: self.depth_format,
            surface_width: self.surface_config.width,
            surface_height: self.surface_config.height,
            scale_factor: self.scale_factor,
        }
    }

    /// Render with a custom scene pass followed by a UI overlay pass.
    ///
    /// 1. Acquires the surface texture
    /// 2. Calls `scene_fn` with a SceneRenderContext — the app draws its 3D scene
    /// 3. Submits the scene commands
    /// 4. Draws the 2D UI overlay (rects + text) using LoadOp::Load
    /// 5. Presents
    pub fn render_scene_then_ui(
        &mut self,
        scene_fn: impl FnOnce(&mut SceneRenderContext),
        ui_rects: &[RectInstance],
        ui_draw: impl FnOnce(&mut wgpu::RenderPass<'_>, &wgpu::BindGroup),
    ) -> Result<(), wgpu::SurfaceError> {
        // Single-layer path: there is no overlay layer to put overlay goo in.
        let (goo_base, _) = self.upload_goo(0);
        let output = self.acquire_drawable()?;
        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // === Pass 1: Custom scene ===
        {
            let mut encoder = self.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("sabitori_scene_encoder"),
                },
            );

            let depth_view = self.depth_view.as_ref()
                .expect("depth texture must be created before render_scene_then_ui");

            let mut scene_ctx = SceneRenderContext {
                device: &self.device,
                queue: &self.queue,
                encoder: &mut encoder,
                surface_view: &surface_view,
                depth_view,
                surface_format: self.surface_config.format,
                depth_format: self.depth_format,
                width: self.surface_config.width,
                height: self.surface_config.height,
                scale_factor: self.scale_factor,
            };
            scene_fn(&mut scene_ctx);

            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // === Pass 2: UI overlay (no depth, LoadOp::Load to preserve scene) ===
        {
            let count = ui_rects.len();
            if count > self.instance_capacity {
                self.instance_capacity = count.max(1).next_power_of_two();
                self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("rect_instance_buffer"),
                    size: (self.instance_capacity * std::mem::size_of::<RectInstance>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            if count > 0 {
                self.queue
                    .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(ui_rects));
            }

            let mut encoder = self.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("sabitori_ui_overlay_encoder"),
                },
            );

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sabitori_ui_overlay_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &surface_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                self.draw_rects(&mut pass, 0..count as u32, &goo_base);

                ui_draw(&mut pass, &self.globals_bind_group);
            }

            self.queue.submit(std::iter::once(encoder.finish()));
        }

        self.capture_if_requested(&output.texture);
        output.present();
        Ok(())
    }

    /// Like [`Self::render_scene_then_ui`], but with a separate overlay UI
    /// layer on top of the base UI (tooltips, `overlay_view()`, drag ghosts,
    /// auto-hoisted `.overlay()` subtrees).
    ///
    /// Draw order:
    ///   1. custom scene (`scene_fn`, with depth)
    ///   2. base UI rects, then base UI text (`draw_fn` / `RenderPhase::BaseText`)
    ///   3. overlay UI rects, then overlay UI text (`draw_fn` / `RenderPhase::OverlayText`)
    ///
    /// Steps 2 and 3 are distinct passes (both `LoadOp::Load`) so the overlay
    /// occludes base *text* — a single appended buffer would let base glyphs
    /// paint over an overlay's background. Base and overlay rects share one
    /// instance buffer (overlay appended after base), matching
    /// [`Self::render_layered`].
    /// `overlay_has_content` carries the same meaning as in
    /// [`render_layered`](Self::render_layered): whether the overlay layer
    /// draws anything other than rects. Same trap, same reason it cannot be
    /// derived here.
    pub fn render_scene_then_ui_layered(
        &mut self,
        scene_fn: impl FnOnce(&mut SceneRenderContext),
        base_rects: &[RectInstance],
        overlay_rects: &[RectInstance],
        overlay_has_content: bool,
        mut draw_fn: impl FnMut(RenderPhase, &mut wgpu::RenderPass<'_>, &wgpu::BindGroup),
    ) -> Result<(), wgpu::SurfaceError> {
        let base_count = base_rects.len();
        let overlay_count = overlay_rects.len();
        let total_count = base_count + overlay_count;

        // Grow the shared instance buffer if needed, then upload base rects at
        // offset 0 and overlay rects appended after.
        if total_count > self.instance_capacity {
            self.instance_capacity = total_count.max(1).next_power_of_two();
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rect_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<RectInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if base_count > 0 {
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(base_rects));
        }
        if overlay_count > 0 {
            let offset = (base_count * std::mem::size_of::<RectInstance>()) as u64;
            self.queue
                .write_buffer(&self.instance_buffer, offset, bytemuck::cast_slice(overlay_rects));
        }

        let (goo_base, goo_overlay) = self.upload_goo(base_count as u32);
        let output = self.acquire_drawable()?;
        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // === Pass 1: custom scene (with depth) ===
        {
            let mut encoder = self.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("sabitori_scene_encoder"),
                },
            );
            let depth_view = self.depth_view.as_ref()
                .expect("depth texture must be created before render_scene_then_ui_layered");
            let mut scene_ctx = SceneRenderContext {
                device: &self.device,
                queue: &self.queue,
                encoder: &mut encoder,
                surface_view: &surface_view,
                depth_view,
                surface_format: self.surface_config.format,
                depth_format: self.depth_format,
                width: self.surface_config.width,
                height: self.surface_config.height,
                scale_factor: self.scale_factor,
            };
            scene_fn(&mut scene_ctx);
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // === Pass 2: base UI (no depth, LoadOp::Load to preserve scene) ===
        {
            let mut encoder = self.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("sabitori_scene_ui_base_encoder"),
                },
            );
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sabitori_scene_ui_base_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &surface_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                self.draw_rects(&mut pass, 0..base_count as u32, &goo_base);
                draw_fn(RenderPhase::BaseText, &mut pass, &self.globals_bind_group);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // === Pass 3: overlay UI (no depth, LoadOp::Load) ===
        if overlay_count > 0 || overlay_has_content || !goo_overlay.is_empty() {
            let mut encoder = self.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("sabitori_scene_ui_overlay_encoder"),
                },
            );
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sabitori_scene_ui_overlay_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &surface_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                self.draw_rects(&mut pass, base_count as u32..(base_count + overlay_count) as u32, &goo_overlay);
                draw_fn(RenderPhase::OverlayText, &mut pass, &self.globals_bind_group);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        self.capture_if_requested(&output.texture);
        output.present();
        Ok(())
    }
}

/// 層ごとの goo を 1 本の並びにし、層ごとの描く位置を返す。`rect_bases[i]` が層 i の
/// 矩形の先頭。`layers` が `rect_bases` より短ければ、残りの層は goo 無し。
fn goo_marks(layers: &[Vec<GooSlot>], rect_bases: &[u32]) -> (Vec<GooInstance>, Vec<Vec<GooMark>>) {
    let mut instances = Vec::new();
    let mut out = Vec::with_capacity(rect_bases.len());
    for (i, &at_base) in rect_bases.iter().enumerate() {
        let slots = layers.get(i).map(Vec::as_slice).unwrap_or(&[]);
        let index_base = instances.len() as u32;
        let mut marks: Vec<GooMark> = slots
            .iter()
            .enumerate()
            .map(|(j, g)| GooMark { at: at_base + g.before_rect, index: index_base + j as u32 })
            .collect();
        // Stable: goo queued at the same position keeps tree order.
        marks.sort_by_key(|m| m.at);
        instances.extend(slots.iter().map(|g| g.instance));
        out.push(marks);
    }
    (instances, out)
}

#[cfg(test)]
mod tests {
    use super::{choose_alpha_mode, first_unmet_minimum, pick_limits, UnmetLimit};
    use wgpu::CompositeAlphaMode::{Inherit, Opaque, PostMultiplied, PreMultiplied};

    // Regression guard for the "see-through window" bug: a non-transparent app
    // must get an Opaque surface even when PreMultiplied is also offered.
    // Previously the selection unconditionally preferred PreMultiplied, which
    // made the whole macOS/Metal window composite against the desktop.
    #[test]
    fn opaque_app_gets_opaque_even_when_premultiplied_available() {
        // Typical macOS/Metal capability set, Opaque first or not — order must
        // not matter.
        assert_eq!(
            choose_alpha_mode(&[Opaque, PreMultiplied, PostMultiplied], false),
            Opaque
        );
        assert_eq!(
            choose_alpha_mode(&[PreMultiplied, PostMultiplied, Opaque], false),
            Opaque
        );
    }

    #[test]
    fn transparent_app_avoids_opaque() {
        // A window created with `with_transparent(true)` needs a blending mode
        // so its own alpha reaches the compositor.
        assert_eq!(
            choose_alpha_mode(&[Opaque, PreMultiplied, PostMultiplied], true),
            PreMultiplied
        );
        assert_eq!(
            choose_alpha_mode(&[Opaque, PostMultiplied], true),
            PostMultiplied
        );
    }

    #[test]
    fn falls_back_when_opaque_unavailable() {
        // If the surface can't be Opaque, an opaque app still has to pick
        // *something*; prefer a premultiplied mode, else whatever is offered.
        assert_eq!(choose_alpha_mode(&[PreMultiplied], false), PreMultiplied);
        assert_eq!(choose_alpha_mode(&[PostMultiplied], false), PostMultiplied);
        // Last-resort: none of Opaque/Pre/PostMultiplied on offer — fall through
        // to `available[0]` and take whatever the surface exposes (e.g. an
        // Inherit-only capability set). This is the branch the earlier cases
        // never reach.
        assert_eq!(choose_alpha_mode(&[Inherit], false), Inherit);
        assert_eq!(choose_alpha_mode(&[Inherit], true), Inherit);
    }

    /// SwiftShader などの上限の低い WebGL2 環境。baseline を丸ごと「必須」として
    /// 要求すると request_device が LimitsExceeded で落ちて画面が出ない (#72)。
    /// baseline に届かない項目が 1 つでもあれば、アダプタの実値で要求すること。
    #[test]
    fn a_weak_adapter_gets_its_own_limits_instead_of_the_baseline() {
        let baseline = wgpu::Limits::downlevel_webgl2_defaults();
        // 実測値: SwiftShader は max_color_attachments = 6、baseline は 8。
        let available = wgpu::Limits {
            max_color_attachments: 6,
            ..wgpu::Limits::downlevel_webgl2_defaults()
        };

        let picked = pick_limits(baseline.clone(), available.clone());
        assert_eq!(picked.max_color_attachments, 6);
        assert!(
            picked.check_limits(&available),
            "要求がアダプタの上限を超えていたら request_device が失敗する"
        );
    }

    /// 足りているアダプタでは baseline のまま = 既存機種の挙動を 1 つも変えない。
    #[test]
    fn a_capable_adapter_keeps_the_baseline() {
        let baseline = wgpu::Limits::downlevel_webgl2_defaults();
        let available = wgpu::Limits {
            max_inter_stage_shader_components: 60,
            ..wgpu::Limits::default()
        };

        assert_eq!(pick_limits(baseline.clone(), available), baseline);
    }

    /// baseline より寛容なアダプタでも、baseline を超える要求に引き上げない
    /// (強い GPU の余裕に黙って寄りかからないための下限であって、上限ではない)。
    #[test]
    fn a_strong_adapter_is_not_asked_for_more_than_the_baseline() {
        let baseline = wgpu::Limits::downlevel_webgl2_defaults();
        let picked = pick_limits(baseline.clone(), wgpu::Limits::default());
        assert_eq!(picked.max_texture_dimension_2d, baseline.max_texture_dimension_2d);
    }

    /// sabitori が本当に必要とする下限は、どの項目が足りないかを名前で返す
    /// (request_device の「最初に引っかかった項目」より原因が分かる)。
    #[test]
    fn the_minimum_sabitori_needs_is_reported_by_name() {
        assert_eq!(first_unmet_minimum(&wgpu::Limits::default()), None);
        assert_eq!(
            first_unmet_minimum(&wgpu::Limits::downlevel_webgl2_defaults()),
            Some(UnmetLimit {
                name: "max_inter_stage_shader_components",
                needed: 34,
                have: wgpu::Limits::downlevel_webgl2_defaults().max_inter_stage_shader_components,
            }),
            "webgl2 の既定 31 は rect.wgsl の 34 に足りない"
        );

        let tiny = wgpu::Limits {
            max_texture_dimension_2d: 1024,
            ..wgpu::Limits::default()
        };
        assert_eq!(
            first_unmet_minimum(&tiny).map(|u| u.name),
            Some("max_texture_dimension_2d"),
            "グリフアトラスは 2048² を張る"
        );
    }

    /// 層 0 と上掛けの間の層の goo も描く位置を持つ (以前は層 0 と最後の層だけで、
    /// 間の層に置いた goo は黙って消えていた)。位置は共有バッファでの通し番号。
    #[test]
    fn goo_in_a_middle_layer_is_drawn() {
        use super::{goo_marks, GooSlot};
        use bytemuck::Zeroable;
        let slot = |before_rect| GooSlot { before_rect, instance: crate::GooInstance::zeroed() };
        let layers = vec![vec![slot(1)], vec![slot(0), slot(2)], vec![], vec![slot(0)]];
        let (instances, marks) = goo_marks(&layers, &[0, 5, 9, 9]);
        assert_eq!(instances.len(), 4);
        let at: Vec<Vec<(u32, u32)>> =
            marks.iter().map(|l| l.iter().map(|m| (m.at, m.index)).collect()).collect();
        assert_eq!(at, vec![vec![(1, 0)], vec![(5, 1), (7, 2)], vec![], vec![(9, 3)]]);
    }

    /// 2 層用の `set_goo` (地・上掛け) で渡した分は、層 0 と層 1 に描かれる。
    #[test]
    fn two_layer_goo_keeps_its_old_marks() {
        use super::{goo_marks, GooSlot};
        use bytemuck::Zeroable;
        let slot = |before_rect| GooSlot { before_rect, instance: crate::GooInstance::zeroed() };
        let (_, marks) = goo_marks(&[vec![slot(0)], vec![slot(1)]], &[0, 4]);
        let at: Vec<Vec<(u32, u32)>> =
            marks.iter().map(|l| l.iter().map(|m| (m.at, m.index)).collect()).collect();
        assert_eq!(at, vec![vec![(0, 0)], vec![(5, 1)]]);
    }
}
