// Sabitori Glyph Rendering Shader
// Renders text glyphs from an atlas texture via instanced quads.

struct Globals {
    screen_size: vec2<f32>,
    scale_factor: f32,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;
@group(1) @binding(1)
var atlas_sampler: sampler;

struct GlyphInstance {
    // Screen position (x, y) in logical pixels
    @location(0) position: vec2<f32>,
    // Size in pixels (width, height)
    @location(1) size: vec2<f32>,
    // UV rect in atlas: (u, v, u_size, v_size)
    @location(2) uv_rect: vec4<f32>,
    // Color (linear RGBA)
    @location(3) color: vec4<f32>,
    // Per-instance scissor in logical px (x, y, w, h). zw==0 → no clip.
    @location(4) clip_rect: vec4<f32>,
    // 1.0 = color (emoji) glyph, 0.0 = alpha-mask glyph.
    @location(5) is_color: f32,
    // Quad rotation in radians around `position` (the glyph's top-left).
    // 0.0 = axis-aligned. The CPU (`rotate_glyphs`) has already swung
    // `position` itself along the same arc; this only turns the bitmap.
    @location(6) rotation: f32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) clip_rect: vec4<f32>,
    @location(3) is_color: f32,
}

var<private> QUAD_VERTICES: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

// Y-down screen space, so a positive angle turns clockwise on screen.
// Identical to `rect.wgsl`'s rotate2d — keep the two in sync so a rotated
// box and its rotated label lean the same way.
fn rotate2d(p: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(p.x * c - p.y * s, p.x * s + p.y * c);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: GlyphInstance,
) -> VertexOutput {
    var out: VertexOutput;

    let uv = QUAD_VERTICES[vertex_index];
    // Pivot is the quad's own top-left (uv == 0 stays put), matching the
    // pivot `rotate_glyphs` used to place `position`. rotation == 0 gives
    // cos=1 / sin=0, i.e. exactly `position + uv * size` as before.
    let pixel_pos = instance.position + rotate2d(uv * instance.size, instance.rotation);

    let ndc = vec2<f32>(
        pixel_pos.x / globals.screen_size.x * 2.0 - 1.0,
        1.0 - pixel_pos.y / globals.screen_size.y * 2.0,
    );

    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = instance.uv_rect.xy + uv * instance.uv_rect.zw;
    out.color = instance.color;
    out.clip_rect = instance.clip_rect;
    out.is_color = instance.is_color;

    return out;
}

// Brightness-aware contrast enhancement.
//
// Light strokes on a dark background appear perceptually thinner than
// dark strokes on a light background, even at the same alpha mask coverage.
// Compensate by boosting alpha for high-luma text colors via a gamma curve
// driven by REC.601 luma. Dark text is left untouched.
fn enhance_contrast(alpha: f32, color_rgb: vec3<f32>) -> f32 {
    let luma = dot(color_rgb, vec3<f32>(0.299, 0.587, 0.114));
    // luma=0 (black) -> gamma=1.0 (no change)
    // luma=1 (white) -> gamma=0.6 (boost: pow(a, 0.6) > a for a in (0,1))
    let gamma = 1.0 - 0.4 * luma;
    return pow(alpha, gamma);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.clip_rect.z > 0.0 && in.clip_rect.w > 0.0 {
        let screen_pos = in.position.xy / globals.scale_factor;
        let cmin = in.clip_rect.xy;
        let cmax = in.clip_rect.xy + in.clip_rect.zw;
        if screen_pos.x < cmin.x || screen_pos.x > cmax.x
            || screen_pos.y < cmin.y || screen_pos.y > cmax.y {
            discard;
        }
    }
    let atlas_sample = textureSample(atlas_texture, atlas_sampler, in.uv);

    // Color (emoji) glyph: the atlas holds the glyph's own straight RGBA.
    // Emit it directly — DON'T tint by in.color.rgb and DON'T run the
    // monochrome stroke-contrast curve. in.color.a is kept as an overall
    // opacity so dimmed/faded emoji still attenuate. The pipeline blends
    // premultiplied, so multiply rgb (and the glyph alpha) by that opacity.
    if in.is_color > 0.5 {
        let a = atlas_sample.a * in.color.a;
        return vec4<f32>(atlas_sample.rgb * a, a);
    }

    // Alpha-mask glyph (normal text): tint the coverage by the text color.
    let enhanced = enhance_contrast(atlas_sample.a, in.color.rgb);
    let alpha = in.color.a * enhanced;
    // Premultiplied alpha output
    return vec4<f32>(in.color.rgb * alpha, alpha);
}
