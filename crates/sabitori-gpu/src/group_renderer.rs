//! 要素とその子孫を 1 枚の絵として貼る描画器。
//!
//! 木の順に描く経路 (`sabitori::bridge::OrderedFrame`) は、回す要素 (グループ) の中身を
//! 先に別のテクスチャへ描き、本番のパスではそのテクスチャを回して貼る。中身を
//! 箱・文字・画像…ごとに回すのと違い、切り抜き (`overflow_hidden`) や goo も含めて
//! 丸ごと同じように回る。
//!
//! テクスチャは窓 (描き先) と同じ大きさ・同じ形式なので、中身を描く描画器は
//! 本番と同じ物をそのまま使える。

use crate::instance::GroupInstance;

struct Slot {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

pub struct GroupRenderer {
    pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    slots: Vec<Slot>,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
}

impl GroupRenderer {
    /// `format` は本番の描き先と同じ形式 (中身を描く描画器のパイプラインと合わせる)。
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("group_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/group.wgsl").into()),
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("group_texture_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("group_pipeline_layout"),
            bind_group_layouts: &[globals_bind_group_layout, &texture_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("group_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[GroupInstance::layout()],
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("group_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let instance_capacity = 4;
        let instance_buffer = Self::instance_buffer(device, instance_capacity);
        Self {
            pipeline,
            texture_layout,
            sampler,
            format,
            size: (0, 0),
            slots: Vec::new(),
            instance_buffer,
            instance_capacity,
        }
    }

    fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("group_instance_buffer"),
            size: (capacity * std::mem::size_of::<GroupInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// グループ `instances.len()` 個ぶんの描き先を `width × height` (物理 px、描き先と
    /// 同じ大きさ) でそろえ、貼り方を上げる。パスを開く前に呼ぶ。描き先は使い回す。
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        instances: &[GroupInstance],
    ) {
        if instances.is_empty() {
            return;
        }
        let size = (width.max(1), height.max(1));
        if size != self.size {
            self.slots.clear();
            self.size = size;
        }
        while self.slots.len() < instances.len() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("group_target"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("group_texture_bg"),
                layout: &self.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            self.slots.push(Slot { _texture: texture, view, bind_group });
        }
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = Self::instance_buffer(device, self.instance_capacity);
        }
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
    }

    /// グループ `slot` の中身を描く先。[`Self::prepare`] の後で使う。
    pub fn target(&self, slot: usize) -> &wgpu::TextureView {
        &self.slots[slot].view
    }

    /// グループ `slot` を、上げた貼り方で貼る。
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, globals_bind_group: &wgpu::BindGroup, slot: u32) {
        let Some(s) = self.slots.get(slot as usize) else { return };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, globals_bind_group, &[]);
        pass.set_bind_group(1, &s.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, slot..slot + 1);
    }
}
