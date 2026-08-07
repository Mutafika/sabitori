//! Arc / ring rendering pipeline.
//!
//! Each [`RingInstance`] (see `instance.rs`) describes a sectored donut
//! (start angle, total sweep, fill fraction, outer/inner radii). The
//! shader in `arc.wgsl` SDF-rasterizes the active "fill" arc and the
//! inactive "track" arc as a single composited fragment — one quad
//! per ring, no overdraw between fill and track.
//!
//! Modeled on [`ImageRenderer`] minus the texture cache, since rings
//! don't bind any image. Caller owns its own `Vec<RingInstance>`,
//! passes it to [`RingRenderer::render_rings`] from inside a
//! `render_pass`-borrowing closure (same pattern image text use).

use crate::instance::RingInstance;

pub struct RingRenderer {
    pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
}

impl RingRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader_source = include_str!("../../../shaders/arc.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("arc_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("arc_pipeline_layout"),
            bind_group_layouts: &[globals_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("arc_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[RingInstance::layout()],
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

        let instance_capacity = 32;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("arc_instance_buffer"),
            size: (instance_capacity * std::mem::size_of::<RingInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            instance_buffer,
            instance_capacity,
        }
    }

    /// Draw a batch of rings into the given render pass. Uploads
    /// `instances` to the internal vertex buffer first; safe to call
    /// once per pass (subsequent calls in the same pass would clobber
    /// each other via `queue.write_buffer`, same caveat as
    /// `ImageRenderer::render_images`).
    pub fn render_rings(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[RingInstance],
        render_pass: &mut wgpu::RenderPass<'_>,
        globals_bind_group: &wgpu::BindGroup,
    ) {
        if instances.is_empty() {
            return;
        }
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("arc_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<RingInstance>()) as u64,
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
