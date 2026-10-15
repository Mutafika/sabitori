//! 要素とその子孫を 1 枚の絵として貼る描画器。
//!
//! 木の順に描く経路 (`sabitori::bridge::OrderedFrame`) は、回す要素 (グループ) の中身を
//! 先に別のテクスチャへ描き、本番のパスではそのテクスチャを回して貼る。中身を
//! 箱・文字・画像…ごとに回すのと違い、切り抜き (`overflow_hidden`) や goo も含めて
//! 丸ごと同じように回る。
//!
//! テクスチャは窓 (描き先) と同じ大きさ・同じ形式なので、中身を描く描画器は
//! 本番と同じ物をそのまま使える。
//!
//! ぼかし (`.blur(..)`) も貼る前にここで掛ける: 横に ぼかして作業用の絵へ、縦に
//! ぼかして元の絵へ戻す 2 回のパス (`group_blur.wgsl`)。

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
    blur_pipeline: wgpu::RenderPipeline,
    blur_params_layout: wgpu::BindGroupLayout,
    /// ぼかしの設定。グループ `i` の横が `2i`、縦が `2i + 1` 番目 ([`BLUR_STRIDE`] 刻み)。
    blur_params: wgpu::Buffer,
    blur_params_bind_group: wgpu::BindGroup,
    blur_params_capacity: usize,
    /// グループ `i` のぼかしの標準偏差 (物理 px)。0 なら掛けない。
    blur_sigma: Vec<f32>,
    /// 横にぼかした途中の絵。
    scratch: Option<Slot>,
}

/// ぼかしの設定 1 つぶんの間隔。動的オフセットは 256 バイト揃えが要る。
const BLUR_STRIDE: u64 = 256;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurParams {
    step: [f32; 2],
    sigma: f32,
    _pad: f32,
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

        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("group_blur_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/group_blur.wgsl").into()),
        });
        let blur_params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("group_blur_params_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<BlurParams>() as u64),
                },
                count: None,
            }],
        });
        let blur_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("group_blur_pipeline_layout"),
            bind_group_layouts: &[&blur_params_layout, &texture_layout],
            push_constant_ranges: &[],
        });
        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("group_blur_pipeline"),
            layout: Some(&blur_layout),
            vertex: wgpu::VertexState {
                module: &blur_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blur_shader,
                entry_point: Some("fs_main"),
                // 置き換える (混ぜない)。
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
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
        let blur_params_capacity = 4;
        let (blur_params, blur_params_bind_group) =
            Self::blur_params_buffer(device, &blur_params_layout, blur_params_capacity);
        Self {
            pipeline,
            texture_layout,
            sampler,
            format,
            size: (0, 0),
            slots: Vec::new(),
            instance_buffer,
            instance_capacity,
            blur_pipeline,
            blur_params_layout,
            blur_params,
            blur_params_bind_group,
            blur_params_capacity,
            blur_sigma: Vec::new(),
            scratch: None,
        }
    }

    /// ぼかしの設定 `groups` 個ぶん (横と縦で 2 つずつ) の置き場。
    fn blur_params_buffer(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        groups: usize,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("group_blur_params"),
            size: groups as u64 * 2 * BLUR_STRIDE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("group_blur_params_bg"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<BlurParams>() as u64),
                }),
            }],
        });
        (buffer, bind_group)
    }

    fn make_slot(&self, device: &wgpu::Device) -> Slot {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("group_target"),
            size: wgpu::Extent3d { width: self.size.0, height: self.size.1, depth_or_array_layers: 1 },
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
        Slot { _texture: texture, view, bind_group }
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
    /// 同じ大きさ) でそろえ、貼り方とぼかしの設定を上げる。パスを開く前に呼ぶ。
    /// 描き先は使い回す。`scale` は論理 px → 物理 px の倍率 (ぼかしの幅に掛ける)。
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        scale: f32,
        instances: &[GroupInstance],
    ) {
        if instances.is_empty() {
            return;
        }
        let size = (width.max(1), height.max(1));
        if size != self.size {
            self.slots.clear();
            self.scratch = None;
            self.size = size;
        }
        while self.slots.len() < instances.len() {
            let slot = self.make_slot(device);
            self.slots.push(slot);
        }
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = Self::instance_buffer(device, self.instance_capacity);
        }
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));

        self.blur_sigma = instances.iter().map(|g| (g.effect[0] * scale).max(0.0)).collect();
        if self.blur_sigma.iter().all(|&s| s <= 0.0) {
            return;
        }
        if self.scratch.is_none() {
            self.scratch = Some(self.make_slot(device));
        }
        if instances.len() > self.blur_params_capacity {
            self.blur_params_capacity = instances.len().next_power_of_two();
            (self.blur_params, self.blur_params_bind_group) =
                Self::blur_params_buffer(device, &self.blur_params_layout, self.blur_params_capacity);
        }
        // 1 回の書き込みで全部 (write_buffer は提出ごとに最後の書き込みしか効かない)。
        let stride = BLUR_STRIDE as usize;
        let mut bytes = vec![0u8; instances.len() * 2 * stride];
        let step = [1.0 / size.0 as f32, 1.0 / size.1 as f32];
        for (i, &sigma) in self.blur_sigma.iter().enumerate() {
            for (k, dir) in [[step[0], 0.0], [0.0, step[1]]].into_iter().enumerate() {
                let p = BlurParams { step: dir, sigma, _pad: 0.0 };
                let at = (i * 2 + k) * stride;
                bytes[at..at + std::mem::size_of::<BlurParams>()].copy_from_slice(bytemuck::bytes_of(&p));
            }
        }
        queue.write_buffer(&self.blur_params, 0, &bytes);
    }

    /// グループ `slot` にぼかしが要るなら、中身を描いた後・貼る前に掛ける
    /// (横: 絵 → 作業用、縦: 作業用 → 絵)。要らなければ何もしない。
    pub fn apply_effects(&self, encoder: &mut wgpu::CommandEncoder, slot: usize) {
        let (Some(&sigma), Some(scratch)) = (self.blur_sigma.get(slot), self.scratch.as_ref()) else {
            return;
        };
        if sigma <= 0.0 {
            return;
        }
        let target = &self.slots[slot];
        for (k, (src, dst)) in [(target, scratch), (scratch, target)].into_iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("group_blur_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst.view,
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
            let offset = ((slot * 2 + k) as u64 * BLUR_STRIDE) as u32;
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, &self.blur_params_bind_group, &[offset]);
            pass.set_bind_group(1, &src.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
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
