use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::context::{GpuContext, SceneRenderContext};
use crate::instance::RectInstance;

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

pub struct GpuRenderer {
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

        let surface = instance.create_surface(window).expect("Failed to create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find a suitable GPU adapter");

        tracing::info!("GPU: {}", adapter.get_info().name);

        // Use downlevel limits on WASM for broader compatibility
        #[cfg(target_arch = "wasm32")]
        let required_limits = {
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
        let required_limits = wgpu::Limits::default();

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
        tracing::info!("present_mode: {:?}", present_mode);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1).min(max_dim),
            height: size.height.max(1).min(max_dim),
            present_mode,
            desired_maximum_frame_latency: 1,
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

        Self {
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
        }
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

        let required_limits = wgpu::Limits::default();

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
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
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

        Self {
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
    fn acquire_drawable(&mut self) -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError> {
        let output = match self.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.surface_config);
                self.surface.get_current_texture()?
            }
            Err(e) => return Err(e),
        };
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
            if count > 0 {
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.globals_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                pass.draw(0..6, 0..count as u32);
            }

            // Draw extra (text, etc.)
            extra_draw(&mut pass, &self.globals_bind_group);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
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
    pub fn render_layered(
        &mut self,
        base_rects: &[RectInstance],
        overlay_rects: &[RectInstance],
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

                if base_count > 0 {
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_bind_group(0, &self.globals_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                    pass.draw(0..6, 0..base_count as u32);
                }

                draw_fn(RenderPhase::BaseText, &mut pass, &self.globals_bind_group);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // Pass 2: overlay layer — separate encoder + submit
        if overlay_count > 0 {
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

                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.globals_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                pass.draw(0..6, base_count as u32..(base_count + overlay_count) as u32);

                draw_fn(RenderPhase::OverlayText, &mut pass, &self.globals_bind_group);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

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

                if count > 0 {
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_bind_group(0, &self.globals_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                    pass.draw(0..6, 0..count as u32);
                }

                ui_draw(&mut pass, &self.globals_bind_group);
            }

            self.queue.submit(std::iter::once(encoder.finish()));
        }

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
    pub fn render_scene_then_ui_layered(
        &mut self,
        scene_fn: impl FnOnce(&mut SceneRenderContext),
        base_rects: &[RectInstance],
        overlay_rects: &[RectInstance],
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
                if base_count > 0 {
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_bind_group(0, &self.globals_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                    pass.draw(0..6, 0..base_count as u32);
                }
                draw_fn(RenderPhase::BaseText, &mut pass, &self.globals_bind_group);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // === Pass 3: overlay UI (no depth, LoadOp::Load) ===
        if overlay_count > 0 {
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
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.globals_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                pass.draw(0..6, base_count as u32..(base_count + overlay_count) as u32);
                draw_fn(RenderPhase::OverlayText, &mut pass, &self.globals_bind_group);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        output.present();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::choose_alpha_mode;
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
}
