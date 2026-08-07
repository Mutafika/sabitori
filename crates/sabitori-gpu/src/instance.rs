use bytemuck::{Pod, Zeroable};

/// GPU instance data for one arc / ring segment. Layout must match the
/// `RingInstance` struct in `arc.wgsl`. One instance renders both the
/// active ("fill") arc and the inactive ("track") arc as a single
/// composited draw — `value` controls the split between them.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RingInstance {
    /// Center x, center y, outer radius, inner radius (logical px).
    pub center_radii: [f32; 4],   // offset 0,  size 16
    /// Start angle (radians), total sweep (radians), value in [0, 1],
    /// padding.
    pub arc_params: [f32; 4],     // offset 16, size 16
    /// Active fill color (linear RGBA).
    pub fill_color: [f32; 4],     // offset 32, size 16
    /// Inactive track color (linear RGBA).
    pub track_color: [f32; 4],    // offset 48, size 16
    /// Per-instance scissor clip rect in logical pixels: x, y, w, h.
    /// `w == 0 || h == 0` → no clipping. See `RectInstance::clip_rect`.
    pub clip_rect: [f32; 4],      // offset 64, size 16
}
// Total: 80 bytes

impl RingInstance {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0, // center_radii
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 1, // arc_params
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 2, // fill_color
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 3, // track_color
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 64,
                shader_location: 4, // clip_rect
            },
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RingInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: ATTRS,
        }
    }
}

/// GPU instance data for one anti-aliased line segment, rendered as an
/// SDF capsule (the region within `half_width` of the segment, with
/// round end caps). Layout must match `LineInstance` in `line.wgsl`. A
/// polyline is drawn as N-1 of these; round caps keep thin joints
/// seamless.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LineInstance {
    /// Segment endpoints: x0, y0, x1, y1 (logical px).
    pub endpoints: [f32; 4],   // offset 0,  size 16
    /// half_width (px), aa edge softness (px), unused, unused.
    pub params: [f32; 4],      // offset 16, size 16
    /// Stroke color (linear RGBA, un-premultiplied).
    pub color: [f32; 4],       // offset 32, size 16
    /// Per-instance scissor clip rect (x, y, w, h) in logical px.
    /// `w == 0 || h == 0` → no clipping. See `RectInstance::clip_rect`.
    pub clip_rect: [f32; 4],   // offset 48, size 16
}
// Total: 64 bytes

impl LineInstance {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0, // endpoints
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 1, // params
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 2, // color
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 3, // clip_rect
            },
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: ATTRS,
        }
    }
}

/// GPU instance data for a single rounded rectangle.
/// Must match the layout in `rect.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RectInstance {
    /// Bounds: x, y, width, height (logical pixels)
    pub rect: [f32; 4],              // offset 0,  size 16
    /// Corner radii: top-left, top-right, bottom-right, bottom-left
    pub corner_radii: [f32; 4],      // offset 16, size 16
    /// Fill color (linear RGBA)
    pub fill_color: [f32; 4],        // offset 32, size 16
    /// Border color (linear RGBA)
    pub border_color: [f32; 4],      // offset 48, size 16
    /// Border width in logical pixels
    pub border_width: f32,           // offset 64, size 4
    /// Gradient angle in radians (0 = no gradient)
    pub gradient_angle: f32,         // offset 68, size 4
    /// Rotation angle in radians (rotates rect around its center)
    pub rotation: f32,               // offset 72, size 4
    pub _pad0: f32,                  // offset 76, size 4
    /// Shadow color (linear RGBA)
    pub shadow_color: [f32; 4],      // offset 80, size 16
    /// Shadow offset (x, y)
    pub shadow_offset: [f32; 2],     // offset 96, size 8
    /// Shadow params: blur radius, spread
    pub shadow_params: [f32; 2],     // offset 104, size 8
    /// Gradient end color (linear RGBA). Used when gradient_angle != 0.
    pub gradient_end_color: [f32; 4], // offset 112, size 16
    /// Per-instance scissor clip rect in logical pixels: x, y, w, h.
    /// Sentinel `w == 0 || h == 0` disables the test (no clipping).
    /// Walking the RenderCommand clip stack on the CPU side and writing
    /// the running intersection here lets the fragment shader discard
    /// pixels outside the active overflow_hidden / overflow_scroll
    /// container — the previous CPU-only `is_clipped` cull only handled
    /// the entirely-outside case so partial overflows leaked.
    ///
    /// IMPORTANT: because of the sentinel, the CPU side must NEVER write a
    /// genuinely degenerate clip (zero-area intersection) here — that would
    /// read as "unclipped" and leak the instance over the whole screen.
    /// Instances whose effective clip is degenerate are culled CPU-side
    /// (`is_clipped` in sabitori's bridge treats a zero-sized clip as
    /// "clips everything").
    pub clip_rect: [f32; 4],          // offset 128, size 16
}
// Total: 144 bytes

impl RectInstance {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0, // rect
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 1, // corner_radii
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 2, // fill_color
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 3, // border_color
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 64,
                shader_location: 4, // border_width
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 68,
                shader_location: 5, // gradient_angle
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 80,
                shader_location: 6, // shadow_color
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 96,
                shader_location: 7, // shadow_offset
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 104,
                shader_location: 8, // shadow_params
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 112,
                shader_location: 9, // gradient_end_color
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 72,
                shader_location: 10, // rotation
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 128,
                shader_location: 11, // clip_rect
            },
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RectInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: ATTRS,
        }
    }
}
