//! Line / polyline rendering pipeline.
//!
//! Each [`LineInstance`](crate::instance::LineInstance) describes one
//! anti-aliased segment (two endpoints, half width, color). The shader
//! in `line.wgsl` SDF-rasterizes it as a capsule with round caps — one
//! quad per segment. A polyline is just a batch of these; round caps
//! keep thin joints seamless without miter math.
//!
//! Modeled verbatim on [`RingRenderer`](crate::ring_renderer::RingRenderer):
//! caller owns its own `Vec<LineInstance>` and passes it to
//! [`LineRenderer::render_lines`] from inside a `render_pass`-borrowing
//! closure.

use crate::instance::LineInstance;

pub struct LineRenderer {
    pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
}

impl LineRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader_source = include_str!("../../../shaders/line.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("line_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("line_pipeline_layout"),
            bind_group_layouts: &[globals_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("line_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[LineInstance::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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

        let instance_capacity = 64;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("line_instance_buffer"),
            size: (instance_capacity * std::mem::size_of::<LineInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            instance_buffer,
            instance_capacity,
        }
    }

    /// Draw a batch of line segments into the given render pass. Uploads
    /// `instances` to the internal vertex buffer first; safe to call once
    /// per pass (subsequent calls in the same pass would clobber each
    /// other via `queue.write_buffer`, same caveat as `render_rings`).
    pub fn render_lines(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[LineInstance],
        render_pass: &mut wgpu::RenderPass<'_>,
        globals_bind_group: &wgpu::BindGroup,
    ) {
        if instances.is_empty() {
            return;
        }
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("line_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<LineInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, globals_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..6, 0..instances.len() as u32);
    }
}

#[cfg(test)]
mod verify {
    use super::*;
    use crate::instance::LineInstance;

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
            &wgpu::DeviceDescriptor { label: Some("line_test"), ..Default::default() },
            None,
        ))
        .ok()
    }

    #[test]
    fn renders_polyline_to_png() {
        let Some((device, queue)) = headless_device() else {
            eprintln!("skip: no GPU");
            return;
        };
        let (w, h): (u32, u32) = (512, 256); // 512*4 = 2048, 256-aligned (no row padding)
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals: [f32; 4] = [w as f32, h as f32, 1.0, 0.0];
        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&globals_buf, 0, bytemuck::cast_slice(&globals));
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals_bg"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() }],
        });

        let mut lr = LineRenderer::new(&device, format, &globals_layout);

        // A sine wave (cyan) + a diagonal (magenta).
        let mut pts: Vec<(f32, f32)> = Vec::new();
        for i in 0..=90 {
            let t = i as f32 / 90.0;
            let x = 24.0 + t * (w as f32 - 48.0);
            let y = h as f32 * 0.5 - 92.0 * (t * std::f32::consts::PI * 3.0).sin();
            pts.push((x, y));
        }
        let mut instances: Vec<LineInstance> = pts
            .windows(2)
            .map(|s| LineInstance {
                endpoints: [s[0].0, s[0].1, s[1].0, s[1].1],
                params: [2.5, 1.0, 0.0, 0.0],
                color: [0.30, 0.85, 0.92, 1.0],
                clip_rect: [0.0; 4],
            })
            .collect();
        instances.push(LineInstance {
            endpoints: [24.0, 24.0, w as f32 - 24.0, h as f32 - 24.0],
            params: [1.5, 1.0, 0.0, 0.0],
            color: [0.95, 0.35, 0.75, 1.0],
            clip_rect: [0.0; 4],
        });

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("line_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.10, g: 0.10, b: 0.17, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            lr.render_lines(&device, &queue, &instances, &mut pass, &globals_bg);
        }

        let bpr = w * 4;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (bpr * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(encoder.finish()));

        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);
        let data = slice.get_mapped_range();
        let out = std::env::temp_dir().join("polyline_verify.png");
        image::save_buffer(&out, &data[..], w, h, image::ExtendedColorType::Rgba8).unwrap();
        eprintln!("WROTE {}", out.display());
    }
}
