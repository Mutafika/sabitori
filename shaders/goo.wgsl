// Sabitori SDF Goo Shader
//
// Fills two rounded rectangles as one shape: their signed distances are
// combined with a polynomial smooth-min, so while the gap between the edges
// is under `smooth / 2` it fills with a liquid "neck", and further apart
// they render as two plain rounded rects.
//
// Layout: one quad per GooInstance, covering both rects plus the
// smoothing margin the neck can bulge into. Everything is evaluated in
// logical px, the same space `rect.wgsl` uses.

struct Globals {
    screen_size: vec2<f32>,
    scale_factor: f32,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

struct GooInstance {
    // Shape A: x, y, w, h (logical px).
    @location(0) rect_a: vec4<f32>,
    // Shape B: x, y, w, h (logical px).
    @location(1) rect_b: vec4<f32>,
    // Corner radius A (x), corner radius B (y), smooth radius k (z), pad (w).
    @location(2) params: vec4<f32>,
    // Fill color (straight linear RGBA).
    @location(3) color: vec4<f32>,
    // Per-instance scissor in logical px (x, y, w, h). zw==0 → no clip.
    @location(4) clip_rect: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_pos: vec2<f32>,
    @location(1) rect_a: vec4<f32>,
    @location(2) rect_b: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) clip_rect: vec4<f32>,
}

var<private> QUAD_VERTICES: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

// Antialiasing half-width in logical px — matches rect.wgsl.
const AA: f32 = 0.75;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: GooInstance,
) -> VertexOutput {
    var out: VertexOutput;

    let k = instance.params.z;
    let margin = k + AA * 2.0;
    let lo = min(instance.rect_a.xy, instance.rect_b.xy) - margin;
    let hi = max(instance.rect_a.xy + instance.rect_a.zw, instance.rect_b.xy + instance.rect_b.zw) + margin;

    let pixel_pos = mix(lo, hi, QUAD_VERTICES[vertex_index]);
    let ndc = vec2<f32>(
        pixel_pos.x / globals.screen_size.x * 2.0 - 1.0,
        1.0 - pixel_pos.y / globals.screen_size.y * 2.0,
    );

    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.world_pos = pixel_pos;
    out.rect_a = instance.rect_a;
    out.rect_b = instance.rect_b;
    out.params = instance.params;
    out.color = instance.color;
    out.clip_rect = instance.clip_rect;
    return out;
}

// Signed distance to a rounded rect given as (x, y, w, h) with one corner
// radius, clamped to half the shorter side so it never inverts.
fn sdf_round_rect(p: vec2<f32>, rect: vec4<f32>, radius: f32) -> f32 {
    let half_size = rect.zw * 0.5;
    let r = clamp(radius, 0.0, min(half_size.x, half_size.y));
    let q = abs(p - (rect.xy + half_size)) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// Polynomial smooth-min (quadratic). Equals min(a, b) once the two
// distances differ by more than k; inside that band it pulls the surface
// outward by at most k/4, which is what bridges the gap into a neck.
fn smin(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return min(a, b);
    }
    let h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}

// See rect.wgsl — zw==0 means unclipped.
fn clip_discard(screen_pos: vec2<f32>, clip_rect: vec4<f32>) -> bool {
    if clip_rect.z <= 0.0 || clip_rect.w <= 0.0 {
        return false;
    }
    let cmin = clip_rect.xy;
    let cmax = clip_rect.xy + clip_rect.zw;
    return screen_pos.x < cmin.x || screen_pos.x > cmax.x
        || screen_pos.y < cmin.y || screen_pos.y > cmax.y;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if clip_discard(in.world_pos, in.clip_rect) {
        discard;
    }
    let da = sdf_round_rect(in.world_pos, in.rect_a, in.params.x);
    let db = sdf_round_rect(in.world_pos, in.rect_b, in.params.y);
    let d = smin(da, db, in.params.z);
    let alpha = 1.0 - smoothstep(-AA, AA, d);
    if alpha <= 0.0 {
        discard;
    }
    // Colors arrive straight; blending is premultiplied (see rect.wgsl).
    let a = in.color.a * alpha;
    return vec4<f32>(in.color.rgb * a, a);
}
