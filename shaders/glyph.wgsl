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
    // 字の下に見える地の色 (linear RGBA)。a == 1.0 = 分かっている、0.0 = 分からない。
    @location(7) backdrop: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) clip_rect: vec4<f32>,
    @location(3) is_color: f32,
    @location(4) backdrop: vec4<f32>,
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
    out.backdrop = instance.backdrop;

    return out;
}

// sRGB の値 ↔ 光の量 (linear)。
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// 地 `ground` の上に、覆い率 `a` の字の縁を **sRGB の値のまま** 重ねたときと同じ
// 濃さになる覆い率 (#165)。明るさはどちらも光の量。
//
// 面は sRGB なので、GPU は光の量で混ぜる。字の縁 (覆い率が半端な画素) をそのまま
// 光の量で混ぜると、暗い地の明るい字は縁が明るく太ってにじみ、明るい地の暗い字は
// 縁が薄くやせる。CoreText などの普通の描き方は sRGB の値のまま混ぜるので、縁の
// 濃さをそちらに合わせる。覆い率 0 と 1 はそのまま (字の芯の色は変わらない)。
fn gamma_space_coverage(a: f32, text_y: f32, ground_y: f32) -> f32 {
    let d = text_y - ground_y;
    if abs(d) < 1e-4 {
        return a;
    }
    let want = srgb_to_linear(mix(linear_to_srgb(ground_y), linear_to_srgb(text_y), a));
    return clamp((want - ground_y) / d, 0.0, 1.0);
}

// 字の色 `color` (`opacity` は芯の不透明度) の覆い率 `a` を、地に合わせて付け替える。
fn glyph_coverage(a: f32, color: vec3<f32>, opacity: f32, backdrop: vec4<f32>) -> f32 {
    let text_y = luminance(color);
    if backdrop.a > 0.5 {
        // 半透明の字は、芯が地と混ざった色を字の色とみなす。
        let ground_y = luminance(backdrop.rgb);
        return gamma_space_coverage(a, mix(ground_y, text_y, opacity), ground_y);
    }
    // 地が分からない: 明るい字は暗い地の上、暗い字は明るい地の上とみなす。
    // 中間の灰色はどちらとも言えないので、両方の間を取る。
    let lit = smoothstep(0.35, 0.65, linear_to_srgb(text_y));
    return mix(gamma_space_coverage(a, text_y, 1.0), gamma_space_coverage(a, text_y, 0.0), lit);
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
    let coverage = glyph_coverage(atlas_sample.a, in.color.rgb, in.color.a, in.backdrop);
    let alpha = in.color.a * coverage;
    // Premultiplied alpha output
    return vec4<f32>(in.color.rgb * alpha, alpha);
}
