// Sabitori SDF Line Shader
//
// Renders one anti-aliased line segment as an SDF capsule: the region
// within `half_width` of the segment [p0, p1], with round end caps. A
// polyline is drawn as N-1 of these instances; the round caps keep the
// joints seamless for thin strokes (no miter math needed).
//
// Layout: one quad per LineInstance, sized to the axis-aligned bounding
// box of the segment inflated by half_width + an AA halo. The fragment
// shader evaluates the capsule SDF in logical-pixel space. Modeled on
// `arc.wgsl`.

struct Globals {
    screen_size: vec2<f32>,
    scale_factor: f32,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

struct LineInstance {
    // Segment endpoints: p0 = xy, p1 = zw (logical px).
    @location(0) endpoints: vec4<f32>,
    // half_width (x), aa edge softness (y), unused (z, w) — logical px.
    @location(1) params: vec4<f32>,
    // Stroke color (linear RGBA, un-premultiplied).
    @location(2) color: vec4<f32>,
    // Per-instance scissor in logical px (x, y, w, h). zw==0 → no clip.
    @location(3) clip_rect: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) frag_px: vec2<f32>,   // this fragment's logical-px position
    @location(1) p0: vec2<f32>,
    @location(2) p1: vec2<f32>,
    @location(3) half_width: f32,
    @location(4) aa: f32,
    @location(5) color: vec4<f32>,
    @location(6) clip_rect: vec4<f32>,
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
    instance: LineInstance,
) -> VertexOutput {
    var out: VertexOutput;

    let p0 = instance.endpoints.xy;
    let p1 = instance.endpoints.zw;
    let half_width = instance.params.x;
    let aa = instance.params.y;

    // Quad = the segment's bounding box, inflated by the stroke's half
    // width plus an AA halo so the SDF has room to fade at the edges.
    let pad = half_width + aa + 1.0;
    let lo = min(p0, p1) - vec2<f32>(pad, pad);
    let hi = max(p0, p1) + vec2<f32>(pad, pad);

    let uv = QUAD_VERTICES[vertex_index];
    let px = lo + (hi - lo) * uv;

    let ndc = vec2<f32>(
        px.x / globals.screen_size.x * 2.0 - 1.0,
        1.0 - px.y / globals.screen_size.y * 2.0,
    );

    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.frag_px = px;
    out.p0 = p0;
    out.p1 = p1;
    out.half_width = half_width;
    out.aa = aa;
    out.color = instance.color;
    out.clip_rect = instance.clip_rect;
    return out;
}

// Unsigned distance from point `p` to the segment [a, b].
fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let denom = max(dot(ba, ba), 1e-6);
    let h = clamp(dot(pa, ba) / denom, 0.0, 1.0);
    return length(pa - ba * h);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Per-instance scissor (logical px), same convention as rect/arc.
    if in.clip_rect.z > 0.0 && in.clip_rect.w > 0.0 {
        let cmin = in.clip_rect.xy;
        let cmax = in.clip_rect.xy + in.clip_rect.zw;
        if in.frag_px.x < cmin.x || in.frag_px.x > cmax.x
            || in.frag_px.y < cmin.y || in.frag_px.y > cmax.y {
            discard;
        }
    }

    // Capsule SDF: inside when distance-to-segment <= half_width.
    let d = sd_segment(in.frag_px, in.p0, in.p1) - in.half_width;
    let aa = max(in.aa, 0.5);
    let alpha = 1.0 - smoothstep(-aa, aa, d);

    // Premultiplied-alpha output: input color is un-premultiplied, so
    // multiply by SDF coverage (matches the rect/arc pipelines, which
    // use PREMULTIPLIED_ALPHA_BLENDING).
    return in.color * alpha;
}
