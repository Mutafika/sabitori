// Sabitori Image Rendering Shader
// Renders textured quads with rounded corner clipping and opacity.

struct Globals {
    screen_size: vec2<f32>,
    scale_factor: f32,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(1) @binding(0)
var image_texture: texture_2d<f32>;
@group(1) @binding(1)
var image_sampler: sampler;

struct ImageInstance {
    // Destination rect: x, y, w, h in logical pixels
    @location(0) rect: vec4<f32>,
    // UV rect: u, v, u_size, v_size
    @location(1) uv_rect: vec4<f32>,
    // Corner radii: TL, TR, BR, BL
    @location(2) corner_radii: vec4<f32>,
    // Opacity + padding
    @location(3) params: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) half_size: vec2<f32>,
    @location(3) corner_radii: vec4<f32>,
    @location(4) opacity: f32,
}

var<private> QUAD_VERTICES: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: ImageInstance,
) -> VertexOutput {
    var out: VertexOutput;

    let uv = QUAD_VERTICES[vertex_index];
    let pixel_pos = instance.rect.xy + uv * instance.rect.zw;

    let ndc = vec2<f32>(
        pixel_pos.x / globals.screen_size.x * 2.0 - 1.0,
        1.0 - pixel_pos.y / globals.screen_size.y * 2.0,
    );

    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = instance.uv_rect.xy + uv * instance.uv_rect.zw;

    let center = instance.rect.xy + instance.rect.zw * 0.5;
    out.local_pos = pixel_pos - center;
    out.half_size = instance.rect.zw * 0.5;
    out.corner_radii = instance.corner_radii;
    out.opacity = instance.params.x;

    return out;
}

// SDF rounded rectangle (same as rect.wgsl)
fn sdf_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    var r: vec2<f32>;
    if p.x > 0.0 {
        r = radii.yz; // TR, BR
    } else {
        r = radii.xw; // TL, BL
    }
    var radius: f32;
    if p.y > 0.0 {
        radius = r.y; // bottom
    } else {
        radius = r.x; // top
    }
    let q = abs(p) - half_size + radius;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(image_texture, image_sampler, in.uv);
    var alpha = sample.a * in.opacity;

    // SDF rounded corner clipping
    let max_radius = max(
        max(in.corner_radii.x, in.corner_radii.y),
        max(in.corner_radii.z, in.corner_radii.w)
    );
    if max_radius > 0.0 {
        let dist = sdf_rounded_rect(in.local_pos, in.half_size, in.corner_radii);
        let clip = 1.0 - smoothstep(-0.75, 0.75, dist);
        alpha *= clip;
    }

    // Premultiplied alpha output
    return vec4<f32>(sample.rgb * alpha, alpha);
}
