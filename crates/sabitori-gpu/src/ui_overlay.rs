//! UI overlay renderer — 既存の wgpu device / queue / surface を持つホストアプリに
//! sabitori UI を重ね描きするための軽量レンダラー。
//!
//! [`GpuRenderer`](crate::GpuRenderer) と違い、surface・device を**所有しない**。
//! ホスト（自前の winit ループ + 自前の 3D パイプラインを持つアプリ）が
//! 既に獲得した surface texture への render pass に、sabitori の rect 群を
//! 描き込む。テキストは `TextRenderer::render_glyphs` に
//! [`globals_bind_group`](Self::globals_bind_group) を渡して同じ pass で描く。
//!
//! ## 使い方（ホスト側の 1 フレーム）
//!
//! ```ignore
//! // 1. ビルド結果から rects を作る（base = view、overlay = ドロップダウン等）
//! ui.update_globals(&queue, logical_w, logical_h, scale_factor);
//! ui.upload_rects(&device, &queue, &base_rects, &overlay_rects);
//!
//! // 2. 3D シーン submit 後、LoadOp::Load の pass で base を描く
//! ui.draw_base(&mut pass);
//! text_renderer.render_glyphs(&device, &queue, &base_glyphs, &mut pass, ui.globals_bind_group());
//! // （pass を閉じて submit）
//!
//! // 3. overlay があれば 2 つ目の pass + submit で重ねる
//! //    （TextRenderer の glyph buffer は submit 単位で 1 回しか書けないため、
//! //      base / overlay は別 submit にすること）
//! ui.draw_overlay(&mut pass2);
//! text_renderer.render_glyphs(&device, &queue, &overlay_glyphs, &mut pass2, ui.globals_bind_group());
//! ```

use wgpu::util::DeviceExt;

use crate::instance::RectInstance;

/// rect.wgsl の `Globals` uniform と同レイアウト（renderer.rs と同一）。
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    screen_size: [f32; 2],
    scale_factor: f32,
    _pad: f32,
}

/// surface 非所有の sabitori UI レンダラー。
///
/// base レイヤー（通常 UI）と overlay レイヤー（ドロップダウン / モーダル等）の
/// 2 層の rect 群を 1 つの instance buffer に同居させ、
/// [`draw_base`](Self::draw_base) / [`draw_overlay`](Self::draw_overlay) で
/// それぞれの範囲を描く。
pub struct UiOverlayRenderer {
    rect_pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    globals_bind_group_layout: wgpu::BindGroupLayout,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    base_count: u32,
    overlay_count: u32,
}

impl UiOverlayRenderer {
    /// 既存 device + 描画先フォーマットからレンダラーを構築する。
    /// `target_format` はホストの surface フォーマット
    /// （`surface_config.format`）を渡すこと。
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let globals = Globals {
            screen_size: [1.0, 1.0],
            scale_factor: 1.0,
            _pad: 0.0,
        };
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sabitori_ui_overlay_globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let globals_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sabitori_ui_overlay_globals_layout"),
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
            label: Some("sabitori_ui_overlay_globals_bg"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let shader_source = include_str!("../../../shaders/rect.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sabitori_ui_overlay_rect_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sabitori_ui_overlay_pipeline_layout"),
            bind_group_layouts: &[&globals_bind_group_layout],
            push_constant_ranges: &[],
        });

        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sabitori_ui_overlay_rect_pipeline"),
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
                    format: target_format,
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
            label: Some("sabitori_ui_overlay_instances"),
            size: (instance_capacity * std::mem::size_of::<RectInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            rect_pipeline,
            globals_buffer,
            globals_bind_group,
            globals_bind_group_layout,
            instance_buffer,
            instance_capacity,
            base_count: 0,
            overlay_count: 0,
        }
    }

    /// rect.wgsl 系シェーダー共通の globals bind group。
    /// `TextRenderer::render_glyphs` にそのまま渡せる。
    pub fn globals_bind_group(&self) -> &wgpu::BindGroup {
        &self.globals_bind_group
    }

    /// `TextRenderer::new` に渡す bind group layout。
    pub fn globals_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.globals_bind_group_layout
    }

    /// 現在確保している instance buffer の容量（要素数）。テスト/診断用。
    pub fn instance_capacity(&self) -> usize {
        self.instance_capacity
    }

    /// 画面サイズ（論理ピクセル）と scale factor を更新する。
    /// リサイズ時、または毎フレーム呼んでよい（uniform 1 本の write のみ）。
    pub fn update_globals(
        &self,
        queue: &wgpu::Queue,
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
    ) {
        let globals = Globals {
            screen_size: [logical_width, logical_height],
            scale_factor,
            _pad: 0.0,
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
    }

    /// base / overlay の rect 群を instance buffer にアップロードする。
    /// render pass を begin する**前**に呼ぶこと（buffer 再確保があり得るため）。
    pub fn upload_rects(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        base: &[RectInstance],
        overlay: &[RectInstance],
    ) {
        let total = base.len() + overlay.len();
        if total > self.instance_capacity {
            self.instance_capacity = total.max(1).next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sabitori_ui_overlay_instances"),
                size: (self.instance_capacity * std::mem::size_of::<RectInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !base.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(base));
        }
        if !overlay.is_empty() {
            let offset = (base.len() * std::mem::size_of::<RectInstance>()) as u64;
            queue.write_buffer(&self.instance_buffer, offset, bytemuck::cast_slice(overlay));
        }
        self.base_count = base.len() as u32;
        self.overlay_count = overlay.len() as u32;
    }

    /// base レイヤーの rect 群を pass に描く。rect が 0 件なら何もしない。
    pub fn draw_base(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.base_count == 0 {
            return;
        }
        pass.set_pipeline(&self.rect_pipeline);
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, 0..self.base_count);
    }

    /// overlay レイヤーの rect 群を pass に描く。rect が 0 件なら何もしない。
    pub fn draw_overlay(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.overlay_count == 0 {
            return;
        }
        pass.set_pipeline(&self.rect_pipeline);
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, self.base_count..self.base_count + self.overlay_count);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// surface なしの headless device を作る。GPU が無い環境では None。
    fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))?;
        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("ui_overlay_test_device"),
                ..Default::default()
            },
            None,
        ))
        .ok()
    }

    fn test_rect(x: f32, y: f32) -> RectInstance {
        let mut r: RectInstance = bytemuck::Zeroable::zeroed();
        r.rect = [x, y, 50.0, 30.0];
        r.fill_color = [0.5, 0.2, 0.8, 1.0];
        r
    }

    /// offscreen テクスチャに base → overlay の 2 pass を描いて完走することを確認。
    #[test]
    fn renders_base_and_overlay_into_offscreen_texture() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("skip: GPU adapter not available");
            return;
        };
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let mut ui = UiOverlayRenderer::new(&device, format);
        ui.update_globals(&queue, 320.0, 240.0, 2.0);
        ui.upload_rects(
            &device,
            &queue,
            &[test_rect(0.0, 0.0), test_rect(60.0, 0.0)],
            &[test_rect(10.0, 10.0)],
        );

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui_overlay_test_target"),
            size: wgpu::Extent3d { width: 640, height: 480, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        // pass 1: base（クリアして描く）
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("base_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            ui.draw_base(&mut pass);
        }
        queue.submit(std::iter::once(encoder.finish()));

        // pass 2: overlay（Load で重ねる）
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay_pass"),
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
            ui.draw_overlay(&mut pass);
        }
        queue.submit(std::iter::once(encoder.finish()));
        device.poll(wgpu::Maintain::Wait);
    }

    /// instance buffer が base + overlay 合計で成長すること。
    #[test]
    fn instance_buffer_grows_for_combined_count() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("skip: GPU adapter not available");
            return;
        };
        let mut ui = UiOverlayRenderer::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
        assert_eq!(ui.instance_capacity(), 256);
        let base: Vec<RectInstance> = (0..300).map(|i| test_rect(i as f32, 0.0)).collect();
        let overlay: Vec<RectInstance> = (0..300).map(|i| test_rect(0.0, i as f32)).collect();
        ui.upload_rects(&device, &queue, &base, &overlay);
        assert!(ui.instance_capacity() >= 600);
        // 2 のべき乗に丸められる
        assert_eq!(ui.instance_capacity(), 1024);
    }

    /// 空アップロード + 空描画が no-op として完走すること。
    #[test]
    fn empty_layers_are_noop() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("skip: GPU adapter not available");
            return;
        };
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let mut ui = UiOverlayRenderer::new(&device, format);
        ui.upload_rects(&device, &queue, &[], &[]);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: 4, height: 4, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            ui.draw_base(&mut pass);
            ui.draw_overlay(&mut pass);
        }
        queue.submit(std::iter::once(encoder.finish()));
        device.poll(wgpu::Maintain::Wait);
    }
}
