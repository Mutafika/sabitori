//! パスの塗り (`path().fill(..)`) を、三角形に割った頂点のまま描く描画器。
//!
//! 三角形に割るのは描く側 (`sabitori::bridge`)。ここは 1 枚ぶんの頂点を 1 回で上げ、
//! 範囲で描くだけ (他の描画器と同じ形)。

use crate::instance::FillVertex;

pub struct FillRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    capacity: usize,
}

impl FillRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fill_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/fill.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fill_pipeline_layout"),
            bind_group_layouts: &[globals_bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fill_pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[FillVertex::layout()],
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let capacity = 256;
        Self { pipeline, vertex_buffer: Self::buffer(device, capacity), capacity }
    }

    fn buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fill_vertex_buffer"),
            size: (capacity * std::mem::size_of::<FillVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// 1 枚ぶんの頂点 (3 つで三角形 1 つ) を 1 回で上げる。続く [`Self::draw`] は範囲で描く。
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, vertices: &[FillVertex]) {
        if vertices.is_empty() {
            return;
        }
        if vertices.len() > self.capacity {
            self.capacity = vertices.len().next_power_of_two();
            self.vertex_buffer = Self::buffer(device, self.capacity);
        }
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(vertices));
    }

    /// 上げた頂点の `range` を描く。
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, globals_bind_group: &wgpu::BindGroup, range: std::ops::Range<u32>) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, globals_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(range, 0..1);
    }
}
