use bytemuck::{Pod, Zeroable};
use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache};
use sabitori_core::{Color, Typography};
use wgpu::util::DeviceExt;

use crate::atlas::GlyphAtlas;
use crate::shaper::{quantize_font_size, resolve_family, TextShaper};

/// Hit-test info for a single laid-out glyph. Pairs the glyph's logical-pixel
/// rect with its byte range in the source string and a 0-based line index.
/// Used by the runtime to map mouse points to text positions for selection.
///
/// The `byte_start` / `byte_end` are cosmic-text's `glyph.start` / `glyph.end`,
/// which are source-string byte offsets (UTF-8 aware). One source byte sequence
/// may produce multiple glyphs (ligatures) — handle by treating glyphs with the
/// same byte_start as a single hit target.
#[derive(Clone, Copy, Debug)]
pub struct GlyphHit {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line_index: u32,
}

/// GPU instance data for a single glyph quad.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GlyphInstance {
    /// Screen position in logical pixels.
    pub position: [f32; 2],
    /// Size in pixels.
    pub size: [f32; 2],
    /// UV rect in atlas: u, v, u_size, v_size.
    pub uv_rect: [f32; 4],
    /// Text color (linear RGBA).
    pub color: [f32; 4],
    /// Per-instance scissor clip rect in logical pixels: x, y, w, h.
    /// `w == 0 || h == 0` disables the test (no clipping). The bridge
    /// fills this in from the active overflow_hidden / overflow_scroll
    /// container so glyphs that straddle the container's edge get
    /// fragment-discarded instead of leaking past it.
    pub clip_rect: [f32; 4],
    /// `1.0` for a color (emoji) glyph, `0.0` for an alpha-mask glyph. The
    /// shader branches on this: color glyphs output the atlas RGBA directly
    /// (only `color.a` still attenuates them), mask glyphs tint by `color.rgb`.
    /// A float — not a uint — so the whole instance stays one Pod f32 block and
    /// no `@interpolate(flat)` is needed (it's per-instance constant anyway).
    pub is_color: f32,
    /// Quad rotation in radians, around this glyph's own `position` (top-left).
    /// `0.0` = axis-aligned (the default, bit-identical to the old behavior).
    ///
    /// This turns the glyph's *bitmap*; turning the whole run also requires
    /// moving each glyph's `position` along the same arc, which
    /// [`rotate_glyphs`] does on the CPU. Set it through that function rather
    /// than by hand — writing only this field spins each glyph in place and
    /// leaves the baseline horizontal.
    pub rotation: f32,
}

/// Rotate an already-laid-out glyph run around `origin` (logical px).
///
/// Sign convention matches `RectDraw::rotation` / `rect.wgsl`: screen space is
/// Y-down, so a positive angle turns the text **clockwise** on screen. Passing
/// `0.0` is a no-op.
///
/// `origin` is the text's own origin (its `TextDraw::position` — the top-left
/// of the laid-out box), not its center. `RectDraw` pivots around the rect
/// *center* instead, so a rotated element with both a background and a label
/// will not keep them glued together.
///
/// Two things have to turn for text to look rotated: each glyph's *placement*
/// (its top-left, done here) and each glyph's own *quad* (done in
/// `glyph.wgsl`, driven by the `rotation` field this writes). Splitting it that
/// way is what keeps the shaping cache angle-independent — the cache stores
/// origin-relative positions, so the same string at a new angle is still a
/// cache hit and never re-shapes.
///
/// Not idempotent: each call composes another rotation onto the placement
/// while *overwriting* the per-quad angle, so calling it twice bends the run.
/// Apply it once to a freshly prepared run.
pub fn rotate_glyphs(glyphs: &mut [GlyphInstance], origin: (f32, f32), radians: f32) {
    if radians == 0.0 {
        return;
    }
    let (sin, cos) = radians.sin_cos();
    for g in glyphs.iter_mut() {
        let dx = g.position[0] - origin.0;
        let dy = g.position[1] - origin.1;
        g.position = [
            origin.0 + dx * cos - dy * sin,
            origin.1 + dx * sin + dy * cos,
        ];
        g.rotation = radians;
    }
}

impl GlyphInstance {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0, // position
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1, // size
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 2, // uv_rect
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 3, // color
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 4, // clip_rect
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 64,
                shader_location: 5, // is_color
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 68,
                shader_location: 6, // rotation
            },
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: ATTRS,
        }
    }
}

/// Text rendering system.
pub struct TextRenderer {
    /// The font stack plus everything answerable without a GPU. Shared with
    /// headless callers via [`TextShaper`], so on-screen text and off-screen
    /// measurement resolve through the same faces, locale and quantization.
    pub shaper: TextShaper,
    pub swash_cache: SwashCache,
    pub atlas: GlyphAtlas,
    pub pipeline: wgpu::RenderPipeline,
    pub atlas_texture: wgpu::Texture,
    pub atlas_bind_group: wgpu::BindGroup,
    pub instance_buffer: wgpu::Buffer,
    pub instance_capacity: usize,
    /// Device scale factor (DPR) the glyph atlas rasterizes at. `1.0` = logical
    /// size (soft/upscaled on HiDPI); set to the window's scale (e.g. `2.0` on
    /// Retina) so glyph bitmaps are baked at physical resolution and stay crisp.
    /// Layout stays logical — only rasterization scales. The app updates this
    /// each frame from the renderer's `scale_factor`.
    pub scale_factor: f32,
    /// Per-frame-persistent shaping cache. cosmic-text "Advanced" shaping
    /// (`Buffer::new` + `set_text` + `shape_until_scroll`) is expensive and the
    /// scene loop re-tessellates the whole UI every frame, so an unchanged label
    /// would otherwise be re-shaped ~60×/s. Keyed by a 64-bit hash of the
    /// shaping inputs (text + size + width + bold/mono + family + max_lines);
    /// the stored glyphs are positioned **relative to the text origin** so a hit
    /// only re-adds `(x, y)` and overwrites the color — no reshape. Invalidated
    /// whenever scale / font family / loaded fonts change (those alter shaping).
    ///
    /// Serves **both** public entry points. `prepare_text_with_hits` used to
    /// bypass this entirely and re-shape every frame, which meant any app with
    /// text selection re-shaped its whole UI at frame rate — about half of
    /// draw time (#49). Both now go through [`TextRenderer::ensure_shaped`].
    glyph_cache: std::collections::HashMap<u64, ShapedRun>,
}

/// One shaped text run, stored **relative to the text origin** so it can be
/// replayed at any `(x, y)`.
///
/// `hits` is always populated even though `prepare_text_styled` discards it:
/// the data is already in hand while walking the shaped buffer, so building it
/// costs nothing, and keeping one cache (rather than one per entry point) means
/// a label drawn through either function warms the other.
///
/// `glyphs` and `hits` are **not** index-aligned. A hit is recorded for every
/// shaped glyph, while a glyph instance is emitted only if the atlas had room
/// for it and it survived the `max_width` clip — so `hits.len() >= glyphs.len()`.
struct ShapedRun {
    glyphs: Vec<GlyphInstance>,
    hits: Vec<GlyphHit>,
}

/// The CPU-side state shaping needs, borrowed out of [`TextRenderer`].
///
/// Shaping touches no GPU object — only the font system, the swash rasterizer
/// and the atlas *pixel buffer*, all of which are plain memory. Bundling them
/// as a borrow instead of reaching through `&mut self` is what lets the shaping
/// path and the cache in front of it be unit-tested: `TextRenderer::new`
/// demands a `wgpu::Device`, so anything written as a method on it is
/// unreachable from a test without an adapter.
struct ShapeCtx<'a> {
    font_system: &'a mut FontSystem,
    swash_cache: &'a mut SwashCache,
    atlas: &'a mut GlyphAtlas,
    scale_factor: f32,
    preferred_family: &'a Option<String>,
    preferred_monospace_family: &'a Option<String>,
}

/// Hash every input that changes the shaped result.
///
/// Deliberately excludes `x` / `y` / `color`: a [`ShapedRun`] is stored relative
/// to the text origin and recolored on the way out, so the same label at a new
/// position or in a new color is a cache *hit*. Anything that would alter glyph
/// selection, advances or wrapping must be in here — leaving a field out means
/// silently reusing stale glyphs.
fn run_cache_key(
    text: &str,
    font_size: f32,
    max_width: Option<f32>,
    bold: bool,
    monospace: bool,
    family_override: Option<&str>,
    max_lines: Option<u32>,
    typo: Typography,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    font_size.to_bits().hash(&mut h);
    match max_width {
        Some(w) => {
            1u8.hash(&mut h);
            w.to_bits().hash(&mut h);
        }
        None => 0u8.hash(&mut h),
    }
    bold.hash(&mut h);
    monospace.hash(&mut h);
    // Extended typography participates in the shaping key — otherwise a
    // weight/tracking/leading change would reuse stale cached glyphs.
    typo.weight.unwrap_or(0).hash(&mut h);
    typo.letter_spacing.to_bits().hash(&mut h);
    match typo.line_height {
        Some(m) => {
            1u8.hash(&mut h);
            m.to_bits().hash(&mut h);
        }
        None => 0u8.hash(&mut h),
    }
    match family_override {
        Some(f) => {
            1u8.hash(&mut h);
            f.hash(&mut h);
        }
        None => 0u8.hash(&mut h),
    }
    max_lines.hash(&mut h);
    h.finish()
}

/// Shape `text` into the cache under `key` if it is not already there.
///
/// Split out so both public entry points share one shaping path *and* one
/// cache. On a miss this shapes at the caller's `(x, y)` and stores the
/// result origin-relative; on a hit it does nothing.
///
/// `(x, y)` matters on a miss even though the stored run is origin-relative:
/// cosmic-text derives each glyph's sub-pixel bin (`x_bin` / `y_bin`) from
/// the pen position, so shaping at the real coordinates keeps the
/// rasterization identical to what the uncached path produced. Shaping at
/// `(0, 0)` would pin every run to bin 0 and change how text looks today.
///
/// The flip side, and the trade this fix accepts: the bin is now frozen at
/// whatever position the run was *first* shaped at, so text sliding across
/// fractional coordinates no longer re-bins every frame. That is already
/// how `prepare_text_styled` has always behaved, and it buys removing the
/// dominant per-frame CPU cost.
#[allow(clippy::too_many_arguments)]
fn ensure_shaped(
    cache: &mut std::collections::HashMap<u64, ShapedRun>,
    ctx: &mut ShapeCtx<'_>,
    key: u64,
    text: &str,
    x: f32,
    y: f32,
    font_size: f32,
    max_width: Option<f32>,
    bold: bool,
    monospace: bool,
    family_override: Option<&str>,
    max_lines: Option<u32>,
    typo: Typography,
) {
    if cache.contains_key(&key) {
        return;
    }
    let run = shape_run(
        ctx, text, x, y, font_size, max_width, bold, monospace, family_override, max_lines, typo,
    );
    // Bound memory: clock/scroll trickle unique keys; a flat cap + clear is
    // fine (the visible UI re-warms in one frame).
    if cache.len() >= 16_384 {
        cache.clear();
    }
    cache.insert(key, run);
}

/// Run cosmic-text over `text` and collect both the glyph quads and the
/// per-glyph hitboxes, positioned **relative to the text origin**.
///
/// This is the only place shaping happens. Callers go through
/// [`TextRenderer::ensure_shaped`] so the result lands in the cache.
#[allow(clippy::too_many_arguments)]
fn shape_run(
    ctx: &mut ShapeCtx<'_>,
    text: &str,
    x: f32,
    y: f32,
    font_size: f32,
    max_width: Option<f32>,
    bold: bool,
    monospace: bool,
    family_override: Option<&str>,
    max_lines: Option<u32>,
    typo: Typography,
) -> ShapedRun {
    let metrics = Metrics::new(font_size, typo.line_height_px(font_size));
    let mut buffer = Buffer::new(ctx.font_system, metrics);
    let width = max_width.unwrap_or(f32::MAX);
    buffer.set_size(ctx.font_system, Some(width), None);

    // Only log when max_lines is set OR text is long enough to
    // likely be a shelf/notif preview — clock numbers and bell
    // glyphs are noise.
    if std::env::var_os("SABITORI_TEXT_DEBUG").is_some()
        && (max_lines.is_some() || text.len() > 24)
    {
        eprintln!(
            "[sabitori-text] prepare width={} max_lines={:?} text_len={}",
            width,
            max_lines,
            text.len()
        );
    }

    let family = resolve_family(
        ctx.preferred_family,
        ctx.preferred_monospace_family,
        monospace,
        family_override,
    );
    let weight = cosmic_text::Weight(typo.resolved_weight(bold));
    let attrs = Attrs::new().family(family).weight(weight);

    // Reshape with line-clamp: drop trailing chars + append "…"
    // until the wrapped output fits within `max_lines`. The
    // shaper is cheap enough that a bounded iterative search is
    // fine in practice (most strings settle in ≤3 iterations).
    let owned;
    let final_text: &str = match max_lines {
        Some(n) if n > 0 => {
            buffer.set_text(ctx.font_system, text, attrs.clone(), Shaping::Advanced);
            buffer.shape_until_scroll(ctx.font_system, false);
            if buffer.layout_runs().count() <= n as usize {
                text
            } else {
                let n_lines = n as usize;
                // Seed truncation at the start of the (n+1)-th
                // layout run — that's the first byte we *don't*
                // want to render. Step back from there in char
                // increments until the shaped output (with "…"
                // appended) settles within `n` lines.
                let cutoff_byte = buffer
                    .layout_runs()
                    .nth(n_lines)
                    .and_then(|run| run.glyphs.iter().map(|g| g.start).min())
                    .unwrap_or(text.len());
                // cosmic-text の glyph.start は cluster 先頭でない場合がある (= 日本語等
                // multi-byte 中で line break が入ると mid-char になる) 。 char 境界まで
                // 巻き戻してから slice する (= UTF-8 panic 防止) 。
                let mut safe_cut = cutoff_byte.min(text.len());
                while safe_cut > 0 && !text.is_char_boundary(safe_cut) {
                    safe_cut -= 1;
                }
                let mut head = text[..safe_cut].trim_end().to_string();
                let mut iters = 0;
                let trimmed = loop {
                    let trial = format!("{}…", head);
                    buffer.set_text(
                        ctx.font_system,
                        &trial,
                        attrs.clone(),
                        Shaping::Advanced,
                    );
                    buffer.shape_until_scroll(ctx.font_system, false);
                    if buffer.layout_runs().count() <= n_lines || head.is_empty() {
                        break trial;
                    }
                    // Drop one grapheme cluster's worth of bytes
                    // from the tail. Char popping is good enough
                    // for the languages cosmic-text shapes well.
                    head.pop();
                    iters += 1;
                    if iters > 32 {
                        break format!("{}…", head);
                    }
                };
                owned = trimmed;
                owned.as_str()
            }
        }
        _ => text,
    };
    // Final shape pass with whatever the clamp logic decided on.
    buffer.set_text(ctx.font_system, final_text, attrs, Shaping::Advanced);
    buffer.shape_until_scroll(ctx.font_system, false);

    let mut glyphs: Vec<GlyphInstance> = Vec::new();
    let mut hits: Vec<GlyphHit> = Vec::new();

    for (line_idx, run) in buffer.layout_runs().enumerate() {
        let pen_y = y + run.line_y;
        let line_h = run.line_height;
        // Cumulative letter-spacing offset within the line: glyph N shifts
        // right by N*spacing. cosmic-text shapes with natural advances, so
        // the tracking is applied here at emit time.
        let mut ls_off = 0.0_f32;
        for glyph in run.glyphs.iter() {
            // Hit info — independent of atlas resolution。 advance box 全体で
            // 取る (= ligatures / 合成 glyph も byte 範囲が分かる粒度)。
            // letter-spacing shifts the hit box with the glyph so selection
            // stays aligned.
            //
            // Recorded *before* the atlas lookup on purpose: a glyph the
            // atlas had no room for, or one clipped by `max_width`, still
            // occupies its byte range for selection and caret math.
            hits.push(GlyphHit {
                x: glyph.x + ls_off,
                y: run.line_top,
                w: glyph.w,
                h: line_h,
                byte_start: glyph.start,
                byte_end: glyph.end,
                line_index: line_idx as u32,
            });

            // Pass the pen position as offset so cosmic-text encodes
            // sub-pixel alignment (x_bin / y_bin) into the cache key.
            // This yields per-alignment glyph rasterizations in the
            // atlas — sharper text when the pen sits between pixels.
            // Rasterize at the device scale factor (HiDPI: a 2× display
            // bakes a 2× bitmap), then emit the quad in *logical* coords so
            // layout is unchanged — the projection scales it back onto the
            // physical surface, sampling the hi-res bitmap 1:1. `scale == 1.0`
            // is a no-op (identical to the old behavior).
            let scale = ctx.scale_factor;
            // cosmic-text's `physical()` scales the glyph's own coords by
            // `scale` but adds `offset` as-is (physical px). The pen origin
            // here is LOGICAL, so scale it up; the `/ scale` below returns the
            // quad to logical. Passing a logical offset put every glyph at
            // origin/scale → text collapsed to the top-left on HiDPI (scale>1).
            let physical = glyph.physical(((x + ls_off) * scale, pen_y * scale), scale);
            ls_off += typo.letter_spacing;

            if let Some(entry) = ctx.atlas.get_or_insert(
                physical.cache_key,
                ctx.swash_cache,
                ctx.font_system,
            ) {
                let gx = (physical.x as f32 + entry.offset_x) / scale;
                let gy = (physical.y as f32 - entry.offset_y) / scale;

                // Clip glyphs that extend beyond max_width
                if gx - x > width {
                    continue;
                }

                glyphs.push(GlyphInstance {
                    // Origin-relative: the caller re-adds its own (x, y).
                    position: [gx - x, gy - y],
                    size: [entry.width / scale, entry.height / scale],
                    uv_rect: entry.uv_rect,
                    // Placeholder — every read path overwrites this with the
                    // caller's color, which is why color is not in the key.
                    color: [0.0; 4],
                    // Caller (bridge) overwrites with the active
                    // clip rect after this returns. Sentinel = no
                    // clip so direct callers (no overflow context)
                    // still render unclipped.
                    clip_rect: [0.0, 0.0, 0.0, 0.0],
                    is_color: if entry.color { 1.0 } else { 0.0 },
                    // Shaping is angle-independent, so the run is always
                    // laid out flat here (and cached that way). The caller
                    // applies rotation afterwards via `rotate_glyphs`.
                    rotation: 0.0,
                });
            }
        }
    }

    ShapedRun { glyphs, hits }
}

impl TextRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        // Font stack, locale normalization and preferred families all live in
        // `TextShaper` so a headless caller resolves text through exactly the
        // same rules — see `TextShaper::new`.
        let shaper = TextShaper::new();
        let swash_cache = SwashCache::new();
        let atlas = GlyphAtlas::new();

        // Atlas texture
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph_atlas"),
            size: wgpu::Extent3d {
                width: atlas.size,
                height: atlas.size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let atlas_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("atlas_bind_group_layout"),
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

        let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas_bind_group"),
            layout: &atlas_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        // Shader
        let shader_source = include_str!("../../../shaders/glyph.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("glyph_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glyph_pipeline_layout"),
            bind_group_layouts: &[globals_bind_group_layout, &atlas_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glyph_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[GlyphInstance::layout()],
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

        let instance_capacity = 4096;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph_instance_buffer"),
            size: (instance_capacity * std::mem::size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            shaper,
            swash_cache,
            atlas,
            pipeline,
            atlas_texture,
            atlas_bind_group,
            instance_buffer,
            instance_capacity,
            scale_factor: 1.0,
            glyph_cache: std::collections::HashMap::new(),
        }
    }

    /// Override the generic sans-serif family. When set, proportional text
    /// (non-monospace) is shaped with `Family::Name(&name)` instead of
    /// `Family::SansSerif`, which on macOS otherwise routes Japanese kanji
    /// through Chinese-styled system fonts.
    pub fn set_preferred_family(&mut self, family: Option<String>) {
        // The shaper owns the value and reports whether it moved; dropping the
        // shaped-glyph cache stays here because the shaper has no caches.
        if self.shaper.set_preferred_family(family) {
            self.glyph_cache.clear(); // resolved face changed → reshape
        }
    }

    /// Override the generic monospace family. When set, monospace text is shaped
    /// with `Family::Name(&name)` instead of `Family::Monospace`, letting an app
    /// switch the fixed-width face at runtime (font picker). Returns whether the
    /// value actually changed, so the caller can invalidate any size cache keyed
    /// on the old face (the measure cache doesn't include the family).
    pub fn set_preferred_monospace_family(&mut self, family: Option<String>) -> bool {
        if self.shaper.set_preferred_monospace_family(family) {
            self.glyph_cache.clear(); // resolved monospace face changed → reshape
            true
        } else {
            false
        }
    }

    /// Load a font from raw TTF/OTF data. Can be called multiple times
    /// to register additional fonts (e.g. Regular + Bold weights).
    ///
    /// ```ignore
    /// let font_data = include_bytes!("../assets/fonts/Hack-Regular.ttf");
    /// text_renderer.load_font(font_data.to_vec());
    /// ```
    pub fn load_font(&mut self, data: Vec<u8>) {
        self.shaper.load_font(data);
        self.glyph_cache.clear(); // new face may change fallback/shaping
    }

    /// 渡した user fonts を system fonts より先に DB に入れ直す。
    ///
    /// cosmic_text の script フォールバックは fontdb の挿入順で最初にグリフを
    /// 持つフォントを採用するため、user font を先に積むと macOS の Hiragino 等
    /// のシステム JP フォントよりバンドル済みの Noto などが優先される。
    pub fn prefer_user_fonts(&mut self, user_fonts: &[Vec<u8>]) {
        self.shaper.prefer_user_fonts(user_fonts);
        self.glyph_cache.clear(); // font_system rebuilt → reshape everything
    }

    /// Update the rasterization scale factor, flushing the glyph atlas when it
    /// actually changes. Bitmaps baked at the old scale are unreachable after
    /// the change (the scale is baked into each cache key), so without the
    /// flush a window moving between displays of different DPR (e.g. a 1×
    /// external ↔ a 2× Retina panel) accumulates dead glyphs until the atlas
    /// fills and new ones silently render as blanks. Use this instead of
    /// writing the `scale_factor` field directly.
    pub fn set_scale_factor(&mut self, scale: f32) {
        if scale != self.scale_factor {
            self.scale_factor = scale;
            self.atlas.clear();
            self.glyph_cache.clear(); // glyph size/pos baked at the old scale
        }
    }

    /// Self-heal after glyph-atlas exhaustion. The atlas is a fixed-size texture
    /// with no eviction; once it fills (a long session accumulates stale glyphs
    /// — old numbers, closed panels, sub-pixel bin variants, large color emoji),
    /// `get_or_insert` starts returning `None` and glyphs silently drop out.
    /// The shaping cache then bakes the incomplete result in permanently — the
    /// label stays broken until a scale/font change. Call this once per frame
    /// *before* shaping the frame's text: if the atlas overflowed last frame,
    /// evict everything and drop the shaping cache so the now-visible glyph set
    /// re-rasterizes into a fresh atlas (one glitch frame, then clean). Returns
    /// whether a flush happened. Cheap no-op when the atlas is healthy.
    pub fn maybe_recover_atlas(&mut self) -> bool {
        if self.atlas.exhausted {
            self.atlas.clear();
            self.glyph_cache.clear();
            true
        } else {
            false
        }
    }

    /// Whether the glyph atlas overflowed (dropped glyphs) during the last
    /// shaping pass. The runtime reads this after rendering: under
    /// `lazy_render` it must force one more frame so [`maybe_recover_atlas`]
    /// can flush + re-shape (otherwise the loop parks on the broken frame and
    /// the missing glyphs stay on screen until the user interacts).
    pub fn atlas_overflowed(&self) -> bool {
        self.atlas.exhausted
    }

    /// Measure text dimensions without generating glyph instances.
    ///
    /// `max_width` constrains the shaping pass. When `Some(w)`, text wraps at
    /// `w` and the returned height reflects the actual wrapped line count.
    /// When `None`, measures the natural (single-line) width.
    ///
    /// `max_lines`, when `Some(n)`, caps the reported height at `n` lines —
    /// the actual content may wrap to more lines but the layout treats only
    /// the first `n` as visible (subsequent lines get truncated at render
    /// time by `prepare_text_styled`).
    ///
    /// Line height is `font_size * 1.4` (matches `Metrics::new(font_size, font_size * 1.4)`
    /// used throughout for rendering).
    /// Measure text via the shared [`TextShaper`].
    ///
    /// Kept on the renderer for convenience; the shaper is public
    /// (`renderer.shaper`) and needs no GPU, so headless hosts call it directly.
    #[allow(clippy::too_many_arguments)]
    pub fn measure_text(
        &mut self,
        text: &str,
        font_size: f32,
        bold: bool,
        monospace: bool,
        family_override: Option<&str>,
        max_width: Option<f32>,
        max_lines: Option<u32>,
        typo: Typography,
    ) -> sabitori_core::TextMetrics {
        self.shaper.measure_text(
            text,
            font_size,
            bold,
            monospace,
            family_override,
            max_width,
            max_lines,
            typo,
        )
    }

    /// Shape and layout text, returning glyph instances ready for rendering.
    pub fn prepare_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        max_width: Option<f32>,
    ) -> Vec<GlyphInstance> {
        self.prepare_text_styled(
            text, x, y, font_size, color, max_width, false, false, None, None,
            Typography::default(),
        )
    }

    /// Shape and layout text with font style options.
    ///
    /// `max_lines`, when `Some(n)`, hard-caps the visible line count.
    /// If the natural wrap produces more than `n` lines, the source
    /// string is iteratively trimmed and `…` appended until it fits
    /// — Excel/Finder-style wrap with a configurable line cap.
    pub fn prepare_text_styled(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        max_width: Option<f32>,
        bold: bool,
        monospace: bool,
        family_override: Option<&str>,
        max_lines: Option<u32>,
        typo: Typography,
    ) -> Vec<GlyphInstance> {
        // Snap size *before* it feeds the cache key, the shaped Buffer, and
        // cosmic-text's physical/atlas key — a continuously-drifting size
        // otherwise misses every cache each frame (reshape + re-raster + full
        // atlas re-upload). See `quantize_font_size` / `FONT_SIZE_QUANTUM`.
        let font_size = quantize_font_size(font_size);
        let key = run_cache_key(
            text, font_size, max_width, bold, monospace, family_override, max_lines, typo,
        );
        // Borrowed field-by-field so `glyph_cache` stays independently borrowable
        // — the whole point of `ShapeCtx` over a `&mut self` method.
        let mut ctx = ShapeCtx {
            font_system: &mut self.shaper.font_system,
            swash_cache: &mut self.swash_cache,
            atlas: &mut self.atlas,
            scale_factor: self.scale_factor,
            preferred_family: &self.shaper.preferred_family,
            preferred_monospace_family: &self.shaper.preferred_monospace_family,
        };
        ensure_shaped(
            &mut self.glyph_cache, &mut ctx, key, text, x, y, font_size, max_width, bold,
            monospace, family_override, max_lines, typo,
        );

        let color_arr = color.to_array();
        let run = &self.glyph_cache[&key];
        let mut out = Vec::with_capacity(run.glyphs.len());
        for g in &run.glyphs {
            let mut gi = *g;
            gi.position = [g.position[0] + x, g.position[1] + y];
            gi.color = color_arr;
            out.push(gi);
        }
        out
    }

    /// `prepare_text_styled` の selection 用 sibling。 同じ shaping ロジックで
    /// GPU instance を生成しつつ、 各 glyph の hitbox (advance space, byte 範囲)
    /// を `GlyphHit` として並べて返す。 declarative runtime が hit_test に使う。
    ///
    /// Shares the shaping cache with `prepare_text_styled` — both are thin
    /// translate-and-recolor wrappers over the same [`ShapedRun`], so the two
    /// can no longer drift apart in layout (they used to be parallel copies of
    /// the same ~150 lines, held in sync only by a comment).
    pub fn prepare_text_with_hits(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        max_width: Option<f32>,
        bold: bool,
        monospace: bool,
        family_override: Option<&str>,
        max_lines: Option<u32>,
        typo: Typography,
    ) -> (Vec<GlyphInstance>, Vec<GlyphHit>) {
        // Snap to match the cached/measured paths so hit-test glyph advances stay
        // identical to what `prepare_text_styled` shapes for the same label.
        let font_size = quantize_font_size(font_size);
        let key = run_cache_key(
            text, font_size, max_width, bold, monospace, family_override, max_lines, typo,
        );
        // Borrowed field-by-field so `glyph_cache` stays independently borrowable
        // — the whole point of `ShapeCtx` over a `&mut self` method.
        let mut ctx = ShapeCtx {
            font_system: &mut self.shaper.font_system,
            swash_cache: &mut self.swash_cache,
            atlas: &mut self.atlas,
            scale_factor: self.scale_factor,
            preferred_family: &self.shaper.preferred_family,
            preferred_monospace_family: &self.shaper.preferred_monospace_family,
        };
        ensure_shaped(
            &mut self.glyph_cache, &mut ctx, key, text, x, y, font_size, max_width, bold,
            monospace, family_override, max_lines, typo,
        );

        let color_arr = color.to_array();
        let run = &self.glyph_cache[&key];
        let mut instances = Vec::with_capacity(run.glyphs.len());
        for g in &run.glyphs {
            let mut gi = *g;
            gi.position = [g.position[0] + x, g.position[1] + y];
            gi.color = color_arr;
            instances.push(gi);
        }
        let mut hits = Vec::with_capacity(run.hits.len());
        for h in &run.hits {
            let mut hit = *h;
            hit.x += x;
            hit.y += y;
            hits.push(hit);
        }
        (instances, hits)
    }

    /// Upload the atlas rows that changed since the last call.
    ///
    /// Copies only the pending row band (see [`GlyphAtlas::dirty_rows`]), not
    /// the whole texture. Adding one glyph used to cost a full 2048² × 4 B =
    /// 16 MiB transfer; now it costs that glyph's height in rows, which for a
    /// 12px glyph is ~96 KB.
    pub fn upload_atlas(&mut self, queue: &wgpu::Queue) {
        let Some((min_y, max_y)) = self.atlas.dirty_rows() else {
            return;
        };
        let size = self.atlas.size;
        let rows = max_y - min_y + 1;
        // `pixels` is row-major, so the band starts at a whole-row offset and
        // runs contiguously — one copy, no per-row striding.
        let row_bytes = size as u64 * 4;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: min_y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &self.atlas.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: min_y as u64 * row_bytes,
                bytes_per_row: Some(size * 4),
                rows_per_image: Some(rows),
            },
            wgpu::Extent3d {
                width: size,
                height: rows,
                depth_or_array_layers: 1,
            },
        );
        self.atlas.mark_uploaded();
    }

    /// Upload glyph instances and render them.
    pub fn render_glyphs(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[GlyphInstance],
        render_pass: &mut wgpu::RenderPass<'_>,
        globals_bind_group: &wgpu::BindGroup,
    ) {
        if instances.is_empty() {
            return;
        }

        // Grow buffer if needed
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("glyph_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<GlyphInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        self.upload_atlas(queue);

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, globals_bind_group, &[]);
        render_pass.set_bind_group(1, &self.atlas_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..6, 0..instances.len() as u32);
    }
}

#[cfg(test)]
mod rotate_tests {
    use super::{rotate_glyphs, GlyphInstance};

    fn glyph(x: f32, y: f32) -> GlyphInstance {
        GlyphInstance {
            position: [x, y],
            size: [10.0, 20.0],
            uv_rect: [0.0; 4],
            color: [1.0; 4],
            clip_rect: [0.0; 4],
            is_color: 0.0,
            rotation: 0.0,
        }
    }

    /// `0.0` must leave the run bit-identical — that's what keeps every
    /// existing (unrotated) app's pixels unchanged.
    #[test]
    fn zero_angle_is_a_no_op() {
        let mut gs = [glyph(100.0, 50.0), glyph(112.0, 50.0)];
        let before = gs;
        rotate_glyphs(&mut gs, (100.0, 50.0), 0.0);
        assert_eq!(gs[0].position, before[0].position);
        assert_eq!(gs[1].position, before[1].position);
        assert_eq!(gs[0].rotation, 0.0);
    }

    /// Screen space is Y-down, so +90° must sweep a glyph that sits to the
    /// RIGHT of the origin down BELOW it — clockwise on screen, same lean as
    /// `RectDraw::rotation`. Getting this backwards mirrors every rotated
    /// annotation, which is exactly the failure DXF import would show.
    #[test]
    fn positive_angle_turns_clockwise_on_screen() {
        let mut gs = [glyph(112.0, 50.0)]; // 12px right of the origin
        rotate_glyphs(&mut gs, (100.0, 50.0), std::f32::consts::FRAC_PI_2);
        assert!((gs[0].position[0] - 100.0).abs() < 1e-3, "{:?}", gs[0].position);
        assert!((gs[0].position[1] - 62.0).abs() < 1e-3, "{:?}", gs[0].position);
        assert_eq!(gs[0].rotation, std::f32::consts::FRAC_PI_2);
    }

    /// The pivot is the origin itself: a glyph sitting exactly on it never
    /// moves, at any angle.
    #[test]
    fn glyph_on_the_pivot_stays_put() {
        for angle in [0.3_f32, 1.0, -2.5, std::f32::consts::PI] {
            let mut gs = [glyph(80.0, 40.0)];
            rotate_glyphs(&mut gs, (80.0, 40.0), angle);
            assert!((gs[0].position[0] - 80.0).abs() < 1e-3, "angle {angle}");
            assert!((gs[0].position[1] - 40.0).abs() < 1e-3, "angle {angle}");
        }
    }

    /// Rotation is rigid: the baseline keeps its length and the glyphs keep
    /// their spacing, so a rotated run doesn't stretch or bunch up.
    #[test]
    fn preserves_distances_from_the_pivot() {
        let origin = (100.0, 50.0);
        let mut gs = [glyph(100.0, 50.0), glyph(112.0, 50.0), glyph(124.0, 50.0)];
        let dist = |g: &GlyphInstance| {
            ((g.position[0] - origin.0).powi(2) + (g.position[1] - origin.1).powi(2)).sqrt()
        };
        let before: Vec<f32> = gs.iter().map(dist).collect();
        rotate_glyphs(&mut gs, origin, 0.7);
        for (g, d0) in gs.iter().zip(&before) {
            assert!((dist(g) - d0).abs() < 1e-3, "{} vs {d0}", dist(g));
        }
        // Adjacent advance is unchanged (12px apart before and after).
        let gap = ((gs[1].position[0] - gs[0].position[0]).powi(2)
            + (gs[1].position[1] - gs[0].position[1]).powi(2))
        .sqrt();
        assert!((gap - 12.0).abs() < 1e-3, "{gap}");
    }
}

#[cfg(test)]
mod family_probe_tests {
    /// Ad-hoc probe (machine-dependent, run with --nocapture): does
    /// Family::Name resolve to the named face, for Latin AND Han glyphs?
    /// Mirrors the runtime `FontSystem` exactly: locale-normalized +
    /// user fonts loaded ahead of system fonts (`prefer_user_fonts`).
    #[test]
    fn probe_family_resolution() {
        let mut db = cosmic_text::fontdb::Database::new();
        if let Ok(hack) = std::fs::read("/Users/kubo/Desktop/mutafika/mearie/assets/fonts/Hack-Regular.ttf") {
            db.load_font_data(hack);
        }
        db.load_system_fonts();
        let mut font_system = cosmic_text::FontSystem::new_with_locale_and_db("ja".into(), db);
        let metrics = cosmic_text::Metrics::new(16.0, 22.4);
        for fam_name in ["Menlo", "Monaco", "HackGen", "HackGen Console", "Nonexistent"] {
            let mut buffer = cosmic_text::Buffer::new(&mut font_system, metrics);
            let attrs = cosmic_text::Attrs::new().family(cosmic_text::Family::Name(fam_name));
            buffer.set_text(&mut font_system, "a漢", attrs, cosmic_text::Shaping::Advanced);
            buffer.shape_until_scroll(&mut font_system, false);
            for run in buffer.layout_runs() {
                let names: Vec<String> = run
                    .glyphs
                    .iter()
                    .map(|g| {
                        font_system
                            .db()
                            .face(g.font_id)
                            .map(|i| i.families[0].0.clone())
                            .unwrap_or_else(|| "?".into())
                    })
                    .collect();
                println!("{fam_name:20} -> {names:?}");
            }
        }
    }
}

/// Shaping-cache behaviour (#49).
///
/// These drive [`ensure_shaped`] / [`shape_run`] directly rather than going
/// through [`TextRenderer`], because `TextRenderer::new` needs a `wgpu::Device`
/// and shaping needs no GPU at all. That split is why [`ShapeCtx`] exists.
#[cfg(test)]
mod shaping_cache_tests {
    use super::*;
    use std::collections::HashMap;

    struct Fixture {
        cache: HashMap<u64, ShapedRun>,
        font_system: FontSystem,
        swash_cache: SwashCache,
        atlas: GlyphAtlas,
        family: Option<String>,
        mono_family: Option<String>,
    }

    impl Fixture {
        fn new() -> Self {
            Self::with_atlas(GlyphAtlas::new())
        }

        fn with_atlas(atlas: GlyphAtlas) -> Self {
            Self {
                cache: HashMap::new(),
                font_system: FontSystem::new(),
                swash_cache: SwashCache::new(),
                atlas,
                family: None,
                mono_family: None,
            }
        }

        /// Mirror of what `prepare_text_with_hits` does: key, get-or-shape,
        /// then translate the stored origin-relative run to `(x, y)`.
        fn with_hits(
            &mut self,
            text: &str,
            x: f32,
            y: f32,
            font_size: f32,
            max_lines: Option<u32>,
            typo: Typography,
            bold: bool,
        ) -> (Vec<GlyphInstance>, Vec<GlyphHit>) {
            let font_size = quantize_font_size(font_size);
            let key = run_cache_key(text, font_size, None, bold, false, None, max_lines, typo);
            let mut ctx = ShapeCtx {
                font_system: &mut self.font_system,
                swash_cache: &mut self.swash_cache,
                atlas: &mut self.atlas,
                scale_factor: 1.0,
                preferred_family: &self.family,
                preferred_monospace_family: &self.mono_family,
            };
            ensure_shaped(
                &mut self.cache,
                &mut ctx,
                key,
                text,
                x,
                y,
                font_size,
                None,
                bold,
                false,
                None,
                max_lines,
                typo,
            );
            let run = &self.cache[&key];
            let glyphs = run
                .glyphs
                .iter()
                .map(|g| {
                    let mut gi = *g;
                    gi.position = [g.position[0] + x, g.position[1] + y];
                    gi
                })
                .collect();
            let hits = run
                .hits
                .iter()
                .map(|h| {
                    let mut hit = *h;
                    hit.x += x;
                    hit.y += y;
                    hit
                })
                .collect();
            (glyphs, hits)
        }

        fn plain(&mut self, text: &str, x: f32, y: f32) -> (Vec<GlyphInstance>, Vec<GlyphHit>) {
            self.with_hits(text, x, y, 14.0, None, Typography::default(), false)
        }
    }

    /// The bug this fixes: the hits path re-shaped from scratch every frame.
    /// A second identical request must add no cache entry and must not touch
    /// cosmic-text at all.
    #[test]
    fn second_identical_request_hits_the_cache() {
        let mut f = Fixture::new();
        let (g1, h1) = f.plain("選択できるラベル", 10.0, 20.0);
        assert_eq!(f.cache.len(), 1, "first call must populate the cache");
        assert!(!g1.is_empty() && !h1.is_empty());

        let (g2, h2) = f.plain("選択できるラベル", 10.0, 20.0);
        assert_eq!(f.cache.len(), 1, "a repeat must not add an entry");
        assert_eq!(g2.len(), g1.len());
        assert_eq!(h2.len(), h1.len());
        for (a, b) in g1.iter().zip(&g2) {
            assert_eq!(a.position, b.position);
            assert_eq!(a.uv_rect, b.uv_rect);
        }

        // Decisive: poison the stored run. If the third call re-shaped instead
        // of reading the cache, it would come back full of glyphs again. An
        // unchanged entry count alone would not prove where the answer came
        // from — this does.
        let key = *f.cache.keys().next().unwrap();
        f.cache.get_mut(&key).unwrap().glyphs.clear();
        let (g3, _) = f.plain("選択できるラベル", 10.0, 20.0);
        assert!(
            g3.is_empty(),
            "re-shaped instead of reading the cache ({} glyphs came back)",
            g3.len()
        );
    }

    /// Hits are stored origin-relative, so drawing the same string somewhere
    /// else must shift every box by exactly the origin delta and change
    /// nothing else. Getting this wrong shows up as selection landing on the
    /// wrong characters.
    #[test]
    fn cached_hits_translate_with_the_origin() {
        let mut f = Fixture::new();
        let (_, near) = f.plain("Selectable text", 10.0, 20.0);
        let (_, far) = f.plain("Selectable text", 100.0, 200.0);
        assert_eq!(f.cache.len(), 1, "same shaping inputs → one entry");
        assert_eq!(near.len(), far.len());

        for (a, b) in near.iter().zip(&far) {
            assert!((b.x - a.x - 90.0).abs() < 1e-3, "x: {} vs {}", a.x, b.x);
            assert!((b.y - a.y - 180.0).abs() < 1e-3, "y: {} vs {}", a.y, b.y);
            assert!((a.w - b.w).abs() < 1e-6, "width must not move with origin");
            assert!((a.h - b.h).abs() < 1e-6, "height must not move with origin");
            assert_eq!(a.byte_start, b.byte_start);
            assert_eq!(a.byte_end, b.byte_end);
            assert_eq!(a.line_index, b.line_index);
        }
    }

    /// Glyph quads translate the same way — the two output vectors have to
    /// stay consistent with each other, not just internally.
    #[test]
    fn cached_glyphs_translate_with_the_origin() {
        let mut f = Fixture::new();
        let (near, _) = f.plain("Selectable text", 10.0, 20.0);
        let (far, _) = f.plain("Selectable text", 100.0, 200.0);
        assert_eq!(near.len(), far.len());
        for (a, b) in near.iter().zip(&far) {
            assert!((b.position[0] - a.position[0] - 90.0).abs() < 1e-3);
            assert!((b.position[1] - a.position[1] - 180.0).abs() < 1e-3);
            assert_eq!(a.size, b.size);
            assert_eq!(a.uv_rect, b.uv_rect);
        }
    }

    /// A hit is recorded for every shaped glyph, an instance only for glyphs
    /// the atlas had room for. With a tiny atlas most glyphs get dropped, but
    /// selection must still know where every character sits — so the two
    /// vectors are deliberately not index-aligned.
    #[test]
    fn hits_outlive_glyphs_dropped_by_a_full_atlas() {
        // 32² holds almost nothing at 14px.
        let mut f = Fixture::with_atlas(GlyphAtlas::with_size(32));
        let (glyphs, hits) = f.plain("dropped glyphs still hit-test", 0.0, 0.0);
        assert!(
            hits.len() > glyphs.len(),
            "a full atlas dropped no glyphs ({} hits, {} glyphs) — test is not exercising the path",
            hits.len(),
            glyphs.len()
        );
        assert!(f.atlas.exhausted, "the tiny atlas should have overflowed");
    }

    /// Byte ranges must describe the string that is actually drawn. With
    /// `max_lines` the run is truncated and "…" appended, so no hit may point
    /// past the visible text.
    #[test]
    fn max_lines_truncation_is_reflected_in_hits() {
        let long = "This sentence is deliberately long enough that it wraps across several lines when the width is small.";
        let mut f = Fixture::new();
        let (_, hits) = f.with_hits(long, 0.0, 0.0, 14.0, Some(1), Typography::default(), false);
        assert!(!hits.is_empty());
        let lines = hits.iter().map(|h| h.line_index).max().unwrap();
        assert_eq!(lines, 0, "max_lines(1) must leave exactly one line");
    }

    /// Everything that changes shaping has to be in the key. If any of these
    /// collided, a bold or tracked label would silently reuse the plain run.
    #[test]
    fn shaping_inputs_are_separate_cache_entries() {
        let mut f = Fixture::new();
        let td = Typography::default();
        f.with_hits("Cache key", 0.0, 0.0, 14.0, None, td, false);
        assert_eq!(f.cache.len(), 1);

        f.with_hits("Cache key", 0.0, 0.0, 14.0, None, td, true); // bold
        assert_eq!(f.cache.len(), 2, "bold must not reuse the regular run");

        f.with_hits("Cache key", 0.0, 0.0, 28.0, None, td, false); // size
        assert_eq!(f.cache.len(), 3, "font size must not reuse");

        let mut spaced = Typography::default();
        spaced.letter_spacing = 2.0;
        f.with_hits("Cache key", 0.0, 0.0, 14.0, None, spaced, false);
        assert_eq!(f.cache.len(), 4, "letter-spacing must not reuse");

        // …but position is deliberately NOT in the key.
        f.with_hits("Cache key", 500.0, 500.0, 14.0, None, td, false);
        assert_eq!(f.cache.len(), 4, "position must never add an entry");
    }
}
