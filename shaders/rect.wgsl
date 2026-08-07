// Sabitori SDF Rounded Rectangle Shader
// Renders anti-aliased rounded rectangles with borders, box shadows,
// and linear gradients using Signed Distance Fields.

struct Globals {
    screen_size: vec2<f32>,
    scale_factor: f32,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

struct RectInstance {
    @location(0) rect: vec4<f32>,
    @location(1) corner_radii: vec4<f32>,
    @location(2) fill_color: vec4<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) border_width: f32,
    @location(5) gradient_angle: f32,
    @location(6) shadow_color: vec4<f32>,
    @location(7) shadow_offset: vec2<f32>,
    @location(8) shadow_params: vec2<f32>,
    @location(9) gradient_end_color: vec4<f32>,
    @location(10) rotation: f32,
    // Per-instance scissor in logical px (x, y, w, h). zw==0 → no clip.
    @location(11) clip_rect: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    @location(2) fill_color: vec4<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) border_width: f32,
    @location(5) corner_radii: vec4<f32>,
    @location(6) shadow_color: vec4<f32>,
    @location(7) shadow_offset: vec2<f32>,
    @location(8) shadow_blur: f32,
    @location(9) shadow_spread: f32,
    @location(10) gradient_angle: f32,
    @location(11) gradient_end_color: vec4<f32>,
    // Per-instance clip rect in logical px (x, y, w, h). zw==0 → no
    // clip. fs_main derives the fragment's logical screen position from
    // @builtin(position) / scale_factor, so we don't need a varying for
    // it.
    @location(12) clip_rect: vec4<f32>,
}

var<private> QUAD_VERTICES: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

fn rotate2d(p: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(p.x * c - p.y * s, p.x * s + p.y * c);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: RectInstance,
) -> VertexOutput {
    var out: VertexOutput;

    let shadow_blur = instance.shadow_params.x;
    let shadow_spread = instance.shadow_params.y;

    let shadow_expand = shadow_blur * 3.0 + shadow_spread;
    let expand = max(shadow_expand, instance.border_width);

    let rect_center = instance.rect.xy + instance.rect.zw * 0.5;
    let rect_size_expanded = instance.rect.zw + expand * 2.0;

    let uv = QUAD_VERTICES[vertex_index];
    // quad local offset (unrotated, centered)
    let local_offset_pre = (uv - vec2<f32>(0.5, 0.5)) * rect_size_expanded;
    // rotation around rect center
    let rotated_offset = rotate2d(local_offset_pre, instance.rotation);
    let pixel_pos = rect_center + rotated_offset;

    let ndc = vec2<f32>(
        pixel_pos.x / globals.screen_size.x * 2.0 - 1.0,
        1.0 - pixel_pos.y / globals.screen_size.y * 2.0,
    );

    out.position = vec4<f32>(ndc, 0.0, 1.0);

    // SDF is evaluated in the rect's local (unrotated) frame
    out.local_pos = local_offset_pre;
    out.half_size = instance.rect.zw * 0.5;

    out.fill_color = instance.fill_color;
    out.border_color = instance.border_color;
    out.border_width = instance.border_width;
    out.corner_radii = instance.corner_radii;
    out.shadow_color = instance.shadow_color;
    out.shadow_offset = instance.shadow_offset;
    out.shadow_blur = shadow_blur;
    out.shadow_spread = shadow_spread;
    out.gradient_angle = instance.gradient_angle;
    out.gradient_end_color = instance.gradient_end_color;
    out.clip_rect = instance.clip_rect;

    return out;
}

fn sdf_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    var r: vec2<f32>;
    if p.x > 0.0 {
        r = radii.yz;
    } else {
        r = radii.xw;
    }
    var radius: f32;
    if p.y > 0.0 {
        radius = r.y;
    } else {
        radius = r.x;
    }
    let q = abs(p) - half_size + radius;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

fn shadow_alpha(dist: f32, blur: f32) -> f32 {
    if blur < 0.001 {
        return select(1.0, 0.0, dist > 0.0);
    }
    let sigma = blur * 0.5;
    return 1.0 - smoothstep(-sigma * 2.0, sigma * 2.0, dist);
}

// Discard fragments outside the active overflow clip rect. zw==0 sentinel
// means the instance is unclipped and the test is skipped entirely.
// `frag_pos` is `@builtin(position).xy` (physical px), divided by
// scale_factor inside fs_main so the comparison happens in the same
// logical-pixel space the CPU uses for clip_rect.
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
    let screen_pos = in.position.xy / globals.scale_factor;
    if clip_discard(screen_pos, in.clip_rect) {
        discard;
    }
    let aa_radius = 0.75;

    // -- Box Shadow --
    var shadow = vec4<f32>(0.0);
    if in.shadow_color.a > 0.001 {
        let shadow_pos = in.local_pos - in.shadow_offset;
        let shadow_half = in.half_size + in.shadow_spread;
        let shadow_dist = sdf_rounded_rect(shadow_pos, shadow_half, in.corner_radii);
        let alpha = shadow_alpha(shadow_dist, in.shadow_blur);
        shadow = in.shadow_color * alpha;
    }

    // -- Fill color: solid or gradient --
    var base_color: vec4<f32>;
    if abs(in.gradient_angle) > 0.001 {
        // Reconstruct UV from local_pos and half_size
        let uv = in.local_pos / (in.half_size * 2.0) + 0.5;
        let angle = in.gradient_angle;
        let dir = vec2<f32>(cos(angle), sin(angle));
        let centered = uv - 0.5;
        let t = clamp(dot(centered, dir) + 0.5, 0.0, 1.0);
        base_color = mix(in.fill_color, in.gradient_end_color, t);
    } else {
        base_color = in.fill_color;
    }

    // -- Main Rectangle --
    let dist = sdf_rounded_rect(in.local_pos, in.half_size, in.corner_radii);
    let fill_alpha = 1.0 - smoothstep(-aa_radius, aa_radius, dist);
    var fill = base_color * fill_alpha;

    // Border
    if in.border_width > 0.0 {
        let inner_dist = dist + in.border_width;
        let border_mask = fill_alpha * (1.0 - (1.0 - smoothstep(-aa_radius, aa_radius, inner_dist)));
        fill = mix(fill, in.border_color * fill_alpha, border_mask);
    }

    // Composite
    let result = fill + shadow * (1.0 - fill.a);
    return result;
}
