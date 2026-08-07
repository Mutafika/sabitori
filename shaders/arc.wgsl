// Sabitori SDF Arc / Ring Shader
//
// Renders smooth anti-aliased arc segments (filled donut sectors) with
// independent track + fill colors. Both share the same outer / inner
// radii and total sweep; the fill subtends `value * sweep` of the
// total, and the track fills the remainder.
//
// Layout: one quad per RingInstance, sized to the bounding box of the
// outer circle. The fragment shader runs an arc-SDF in local
// (center-origin) space and shades the two sub-arcs separately.

struct Globals {
    screen_size: vec2<f32>,
    scale_factor: f32,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

struct RingInstance {
    // Center (xy) + outer radius (z) + inner radius (w), all px.
    @location(0) center_radii: vec4<f32>,
    // Start angle (x), total sweep (y), fill fraction in [0,1] (z),
    // unused pad (w).
    @location(1) arc_params: vec4<f32>,
    // Active fill color (linear RGBA).
    @location(2) fill_color: vec4<f32>,
    // Inactive track color (linear RGBA).
    @location(3) track_color: vec4<f32>,
    // Per-instance scissor in logical px (x, y, w, h). zw==0 → no clip.
    @location(4) clip_rect: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) outer_radius: f32,
    @location(2) inner_radius: f32,
    @location(3) start_angle: f32,
    @location(4) sweep: f32,
    @location(5) value: f32,
    @location(6) fill_color: vec4<f32>,
    @location(7) track_color: vec4<f32>,
    @location(8) clip_rect: vec4<f32>,
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
    instance: RingInstance,
) -> VertexOutput {
    var out: VertexOutput;

    let center = instance.center_radii.xy;
    let outer_r = instance.center_radii.z;
    let inner_r = instance.center_radii.w;

    // Quad covers the outer circle's bounding box, with a small AA
    // halo so the SDF anti-aliasing has room to fade at the edges.
    let pad = 1.5;
    let half_size = vec2<f32>(outer_r + pad, outer_r + pad);
    let uv = QUAD_VERTICES[vertex_index];
    let local_offset = (uv - vec2<f32>(0.5, 0.5)) * half_size * 2.0;
    let pixel_pos = center + local_offset;

    let ndc = vec2<f32>(
        pixel_pos.x / globals.screen_size.x * 2.0 - 1.0,
        1.0 - pixel_pos.y / globals.screen_size.y * 2.0,
    );

    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.local_pos = local_offset;
    out.outer_radius = outer_r;
    out.inner_radius = inner_r;
    out.start_angle = instance.arc_params.x;
    out.sweep = instance.arc_params.y;
    out.value = instance.arc_params.z;
    out.fill_color = instance.fill_color;
    out.track_color = instance.track_color;
    out.clip_rect = instance.clip_rect;
    return out;
}

const PI: f32 = 3.14159265358979;
const TAU: f32 = 6.28318530717958;

// Wrap dtheta into [-PI, PI].
fn wrap_pi(x: f32) -> f32 {
    var v = x;
    while (v > PI) { v -= TAU; }
    while (v < -PI) { v += TAU; }
    return v;
}

// Distance from point `p` (in arc-center-local frame) to the band
// {`inner_r..outer_r`} restricted to angular range
// `[start_angle, start_angle + sweep]`. Negative inside.
//
// `sweep` is assumed positive. The arc is rendered going clockwise in
// screen coords (y grows down) — but our SDF math uses standard math
// convention, so the caller passes `start_angle` already adjusted.
fn sd_arc_segment(
    p: vec2<f32>,
    start_angle: f32,
    sweep: f32,
    outer_r: f32,
    inner_r: f32,
) -> f32 {
    if (sweep <= 0.0) {
        // Empty arc — push pixels well outside.
        return outer_r * 4.0;
    }
    let mid_angle = start_angle + sweep * 0.5;
    let half_aperture = sweep * 0.5;
    let theta = atan2(p.y, p.x); // [-PI, PI]
    let dt = wrap_pi(theta - mid_angle);
    let r = length(p);
    let band_dist = max(r - outer_r, inner_r - r);
    if (abs(dt) <= half_aperture) {
        return band_dist;
    }
    // Outside the angular sweep — distance is to whichever end cap is
    // closer (a half-disc of diameter = ring thickness, centered on
    // the radial midline at the boundary angle).
    let cap_angle = mid_angle + sign(dt) * half_aperture;
    let cap_dir = vec2<f32>(cos(cap_angle), sin(cap_angle));
    let cap_r_mid = (outer_r + inner_r) * 0.5;
    let cap_thickness = (outer_r - inner_r) * 0.5;
    let cap_pos = cap_dir * cap_r_mid;
    return length(p - cap_pos) - cap_thickness;
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
    let aa = 0.75;
    let p = in.local_pos;
    let value = clamp(in.value, 0.0, 1.0);
    let fill_sweep = in.sweep * value;
    let track_sweep = in.sweep * (1.0 - value);
    let track_start = in.start_angle + fill_sweep;

    let fill_d = sd_arc_segment(
        p,
        in.start_angle,
        fill_sweep,
        in.outer_radius,
        in.inner_radius,
    );
    let track_d = sd_arc_segment(
        p,
        track_start,
        track_sweep,
        in.outer_radius,
        in.inner_radius,
    );

    let fill_a = 1.0 - smoothstep(-aa, aa, fill_d);
    let track_a = 1.0 - smoothstep(-aa, aa, track_d);

    // Match the rect pipeline's convention: input colors are
    // un-premultiplied; multiply by SDF coverage to get the source
    // contribution, then premultiplied-alpha composite layer-by-layer.
    let fill = in.fill_color * fill_a;
    let track = in.track_color * track_a;
    // Fill paints over track wherever both regions overlap — should
    // be near-zero overlap given the angles are disjoint, but the
    // AA halo can produce a 1-pixel seam without explicit ordering.
    let combined = fill + track * (1.0 - fill.a);
    return combined;
}
