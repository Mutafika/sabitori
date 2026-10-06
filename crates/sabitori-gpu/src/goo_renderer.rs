//! Goo rendering pipeline — two rounded rects fused by an SDF smooth
//! union (`goo.wgsl`).
//!
//! Unlike rings and lines, goo is a *background*: it has to sit under the
//! rects that follow it in tree order (a popup's rows on top of the goo
//! that replaced its backdrop). So it is not drawn in the post-rect phase.
//! [`GpuRenderer`](crate::GpuRenderer) uploads every goo of the frame once
//! and splits its rect draw call around each [`GooSlot::before_rect`],
//! switching to this pipeline in between.

use crate::instance::GooInstance;

/// One goo plus where it sits in its layer's rect sequence: it is drawn
/// after the first `before_rect` rects of that layer and before the rest.
#[derive(Clone, Copy, Debug)]
pub struct GooSlot {
    pub before_rect: u32,
    pub instance: GooInstance,
}

pub struct GooRenderer {
    pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
}

impl GooRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("goo_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/goo.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("goo_pipeline_layout"),
            bind_group_layouts: &[globals_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("goo_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[GooInstance::layout()],
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

        let instance_capacity = 8;
        let instance_buffer = Self::create_buffer(device, instance_capacity);
        Self { pipeline, instance_buffer, instance_capacity }
    }

    fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("goo_instance_buffer"),
            size: (capacity * std::mem::size_of::<GooInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Upload every goo of the frame (all layers). Call once per frame —
    /// `queue.write_buffer` only takes effect once per submit, so a second
    /// upload would clobber instances the earlier pass still reads.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, instances: &[GooInstance]) {
        if instances.is_empty() {
            return;
        }
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = Self::create_buffer(device, self.instance_capacity);
        }
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
    }

    /// Draw uploaded instances `range` into the pass. Leaves the goo
    /// pipeline bound — callers switching back to rects rebind theirs.
    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        globals_bind_group: &wgpu::BindGroup,
        range: std::ops::Range<u32>,
    ) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, globals_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, range);
    }
}
