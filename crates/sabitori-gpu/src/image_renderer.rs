//! Image rendering pipeline for Sabitori.
//! Each unique image gets its own wgpu::Texture + bind group.

use std::collections::HashMap;
use bytemuck::{Pod, Zeroable};

/// GPU instance for a single image quad.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ImageInstance {
    /// Destination rect: x, y, w, h in logical pixels.
    pub rect: [f32; 4],
    /// UV rect: u, v, u_size, v_size.
    pub uv_rect: [f32; 4],
    /// Corner radii: TL, TR, BR, BL.
    pub corner_radii: [f32; 4],
    /// params.x = opacity, rest = padding.
    pub params: [f32; 4],
}

impl ImageInstance {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // rect
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                // uv_rect
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 1,
                },
                // corner_radii
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 2,
                },
                // params (opacity + pad)
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 3,
                },
            ],
        }
    }
}

struct CachedImage {
    bind_group: wgpu::BindGroup,
    _texture: wgpu::Texture,
}

pub struct ImageRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    textures: HashMap<String, CachedImage>,
    /// バイト予算と LRU の帳簿。実際のテクスチャは `textures` 側にあり、
    /// こちらは「どれを追い出すか」だけを決める (#43)。
    budget: crate::TextureBudget,
}

impl ImageRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("image_bind_group_layout"),
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shader_source = include_str!("../../../shaders/image.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image_pipeline_layout"),
            bind_group_layouts: &[globals_bind_group_layout, &bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[ImageInstance::layout()],
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
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let instance_capacity = 64;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image_instance_buffer"),
            size: (instance_capacity * std::mem::size_of::<ImageInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            instance_buffer,
            instance_capacity,
            textures: HashMap::new(),
            budget: crate::TextureBudget::default(),
        }
    }

    /// Ensure an image texture is uploaded to GPU. Cached by key.
    pub fn ensure_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &str,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) {
        if self.textures.contains_key(key) {
            // 今フレーム使ったことにする。これを飛ばすと、今から描く物が
            // 追い出しの候補に残ってしまう。
            self.budget.touch(key);
            return;
        }

        // 予算に収まるまで古い物を捨ててから入れる。捨てた物は次に必要になった
        // フレームで上げ直される — `ensure_texture` は毎フレーム、rgba 付きで
        // 呼ばれるので、追い出しても絵は欠けない。
        let bytes = (width as usize) * (height as usize) * 4;
        for victim in self.budget.admit(key, bytes) {
            self.textures.remove(&victim);
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(key),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(key),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.textures.insert(key.to_string(), CachedImage {
            bind_group,
            _texture: texture,
        });
    }

    // ---------------------------------------------------------------
    // テクスチャ寿命の制御 (#43)
    // ---------------------------------------------------------------

    /// このフレームの描画が終わったことをキャッシュに伝える。
    ///
    /// **フレームにつき 1 回、全パスを描き終えてから**呼ぶこと。パスごとに
    /// 呼ぶと、base パスで使った画像が overlay パスの途中で「古い」と見なされ、
    /// これから描く物を追い出しかねない。ランタイム (`run_declarative` /
    /// `run_scene`) が呼ぶので、普通のアプリが自分で呼ぶ必要はない。
    pub fn end_frame(&mut self) {
        self.budget.end_frame();
    }

    /// テクスチャに使ってよい GPU メモリの上限を変える (既定 256 MiB)。
    ///
    /// 枚数ではなくバイト数なのは、240px のサムネイルと 832×1216 の原寸で
    /// 1 枚あたり 18 倍違うため。枚数で切ると、片方に対して必ずずれる。
    pub fn set_texture_budget_bytes(&mut self, bytes: usize) {
        self.budget.set_budget_bytes(bytes);
    }

    /// 今テクスチャが占めている bytes。
    pub fn texture_bytes(&self) -> usize {
        self.budget.used_bytes()
    }

    /// 今持っているテクスチャの枚数。
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// テクスチャを 1 枚捨てる。持っていれば `true`。
    ///
    /// どれが要らないかはアプリが知っているので、予算に当たる前に自分で捨てたい
    /// 場合に使う。捨てた鍵が再び描かれれば、次のフレームで上げ直される。
    pub fn drop_texture(&mut self, key: &str) -> bool {
        self.budget.remove(key);
        self.textures.remove(key).is_some()
    }

    /// `f` が `false` を返した鍵のテクスチャを捨てる。
    ///
    /// 「今開いているフォルダの物だけ残す」のように、残す条件を式で書ける。
    pub fn retain_textures(&mut self, f: impl FnMut(&str) -> bool) {
        for key in self.budget.retain(f) {
            self.textures.remove(&key);
        }
    }

    /// テクスチャを全部捨てる。
    pub fn clear_textures(&mut self) {
        self.textures.clear();
        self.budget.clear();
    }

    /// Render a batch of image instances that use the same texture.
    ///
    /// NOTE: this API is kept for compatibility but is only safe to call
    /// ONCE per render pass. `queue.write_buffer` is applied before all
    /// commands in the submit, so calling this method multiple times per
    /// pass causes the last write to clobber earlier ones — every batch
    /// then draws the last batch's instance data. Use [`render_many`] when
    /// you have more than one batch in the same pass.
    pub fn render_images(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &str,
        instances: &[ImageInstance],
        render_pass: &mut wgpu::RenderPass<'_>,
        globals_bind_group: &wgpu::BindGroup,
    ) {
        if instances.is_empty() {
            return;
        }
        if !self.textures.contains_key(key) {
            return;
        }
        let cached = self.textures.get(key).unwrap();

        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("image_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<ImageInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, globals_bind_group, &[]);
        render_pass.set_bind_group(1, &cached.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..6, 0..instances.len() as u32);
    }

    /// Render multiple image batches in one render pass. Packs every batch's
    /// instances into the shared instance buffer with per-batch offsets, then
    /// draws each batch with the correct offset slice and bind group.
    ///
    /// This is the correct path when multiple different-keyed images appear
    /// in the same frame; using [`render_images`] in a loop silently produces
    /// corrupt renders because `queue.write_buffer` only applies once per
    /// submit.
    pub fn render_many<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batches: impl IntoIterator<Item = (&'a str, &'a [ImageInstance])>,
        render_pass: &mut wgpu::RenderPass<'_>,
        globals_bind_group: &wgpu::BindGroup,
    ) {
        // Filter to batches whose textures are uploaded, and flatten their
        // instances into a single Vec with offsets.
        let mut packed: Vec<ImageInstance> = Vec::new();
        let mut draws: Vec<(String, u32, u32)> = Vec::new(); // (key, offset_instances, count)
        for (key, instances) in batches {
            if instances.is_empty() {
                continue;
            }
            if !self.textures.contains_key(key) {
                continue;
            }
            let offset = packed.len() as u32;
            packed.extend_from_slice(instances);
            draws.push((key.to_string(), offset, instances.len() as u32));
        }

        if draws.is_empty() {
            return;
        }

        let total = packed.len();
        if total > self.instance_capacity {
            self.instance_capacity = total.next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("image_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<ImageInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&packed));

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, globals_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));

        for (key, offset, count) in &draws {
            let cached = match self.textures.get(key) {
                Some(c) => c,
                None => continue,
            };
            render_pass.set_bind_group(1, &cached.bind_group, &[]);
            let start = *offset;
            render_pass.draw(0..6, start..(start + *count));
        }
    }
}
