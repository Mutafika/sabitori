// scene3d.wgsl — 3D card rendering with SDF rounded rectangles + glow + floor grid

struct Camera {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye_pos: vec3<f32>,
    time: f32,
    warp_progress: f32,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0) var card_texture: texture_2d<f32>;
@group(1) @binding(1) var card_sampler: sampler;

// ════════════════════════════════════════════════════
//  Card instances
// ════════════════════════════════════════════════════

struct CardInstance {
    @location(0) model_0: vec4<f32>,
    @location(1) model_1: vec4<f32>,
    @location(2) model_2: vec4<f32>,
    @location(3) model_3: vec4<f32>,
    @location(4) size: vec2<f32>,
    @location(5) corner_radius: f32,
    @location(6) glow_intensity: f32,
    @location(7) fill_color: vec4<f32>,
    @location(8) border_color: vec4<f32>,
    @location(9) glow_color: vec4<f32>,
    @location(10) kind: f32,
    @location(11) has_texture: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) corner_radius: f32,
    @location(3) fill_color: vec4<f32>,
    @location(4) border_color: vec4<f32>,
    @location(5) glow_color: vec4<f32>,
    @location(6) glow_intensity: f32,
    @location(7) world_pos: vec3<f32>,
    @location(8) kind: f32,
    @location(9) has_texture: f32,
    @location(10) tex_uv: vec2<f32>,
}

var<private> QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-0.5, -0.5),
    vec2<f32>( 0.5, -0.5),
    vec2<f32>( 0.5,  0.5),
    vec2<f32>(-0.5, -0.5),
    vec2<f32>( 0.5,  0.5),
    vec2<f32>(-0.5,  0.5),
);

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    inst: CardInstance,
) -> VertexOutput {
    var out: VertexOutput;

    let model = mat4x4<f32>(inst.model_0, inst.model_1, inst.model_2, inst.model_3);
    let expand = inst.glow_intensity * 0.3 + 0.08;
    let expanded_size = inst.size + vec2<f32>(expand * 2.0);
    let local = QUAD[vi] * expanded_size;
    let world = model * vec4<f32>(local.x, local.y, 0.0, 1.0);

    out.clip_position = camera.view_proj * world;
    out.uv = QUAD[vi] * expanded_size / inst.size;
    out.size = inst.size;
    out.corner_radius = inst.corner_radius;
    out.fill_color = inst.fill_color;
    out.border_color = inst.border_color;
    out.glow_color = inst.glow_color;
    out.glow_intensity = inst.glow_intensity;
    out.world_pos = world.xyz;
    out.kind = inst.kind;
    out.has_texture = inst.has_texture;
    out.tex_uv = QUAD[vi] + 0.5; // [0,1] range for texture sampling

    return out;
}

fn sdf_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    let q = abs(p) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// Night sky fog color
const FOG_COLOR: vec3<f32> = vec3<f32>(0.01, 0.015, 0.04);

// Distance fog — fades objects into night atmosphere
fn fog(color: vec4<f32>, world_pos: vec3<f32>) -> vec4<f32> {
    let dist = length(world_pos - camera.eye_pos);
    let fog_start = 4.0;
    let fog_end = 18.0;
    let f = clamp((dist - fog_start) / (fog_end - fog_start), 0.0, 1.0);
    return vec4<f32>(mix(color.rgb, FOG_COLOR, f * f), color.a * (1.0 - f * 0.4));
}

// Noise for portal interior
fn portal_noise(p: vec2<f32>, t: f32) -> f32 {
    let q = p * 3.0;
    let a = sin(q.x * 1.7 + t * 2.3) * cos(q.y * 2.1 - t * 1.8);
    let b = sin(q.x * 3.1 - t * 1.5 + q.y * 1.3) * 0.5;
    let c = cos(q.y * 4.7 + t * 3.0 + q.x * 0.8) * 0.3;
    return (a + b + c) * 0.5 + 0.5;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let local = in.uv * in.size;
    let half = in.size * 0.5;
    let dist = sdf_rounded_rect(local, half, in.corner_radius);
    let aa = 1.5;
    let t = camera.time;

    if in.kind > 0.5 {
        // ── Portal / Gate ──
        let outer_alpha = 1.0 - smoothstep(-aa, aa, dist);

        // Frame thickness (world-space relative)
        let frame_w = min(in.size.x, in.size.y) * 0.08;
        let inner_half = half - vec2<f32>(frame_w);
        let inner_dist = sdf_rounded_rect(local, inner_half, max(in.corner_radius - frame_w * 0.5, 0.0));
        let inner_alpha = 1.0 - smoothstep(-aa, aa, inner_dist);

        // Frame mask (outer minus inner)
        let frame_mask = outer_alpha * (1.0 - inner_alpha);

        // Frame color with pulse
        let pulse = 0.7 + sin(t * 3.0) * 0.3;
        let frame_color = in.border_color * frame_mask * pulse;

        // Interior energy — swirling glow inside the portal
        let uv_norm = local / half;
        let noise = portal_noise(uv_norm, t);
        let energy_base = 0.15 + noise * 0.25 + in.glow_intensity * 0.2;

        // Bright center, darker edges
        let center_dist = length(uv_norm);
        let center_glow = exp(-center_dist * 1.5) * 0.4;
        let energy = energy_base + center_glow;

        // Vertical scan line
        let scan = sin(uv_norm.y * 12.0 - t * 4.0) * 0.5 + 0.5;
        let scan_add = scan * 0.08;

        let interior_color = in.glow_color * (energy + scan_add) * inner_alpha;

        // Outer glow
        let glow_falloff = 0.1;
        let glow_alpha = exp(-max(dist, 0.0) * glow_falloff) * in.glow_intensity;
        let glow = in.glow_color * glow_alpha;

        // Top arch highlight
        let arch_t = smoothstep(-half.y * 0.3, -half.y * 0.8, local.y);
        let arch_highlight = in.glow_color * arch_t * frame_mask * 0.3;

        // Composite: glow behind + interior + frame + arch
        var color = glow * (1.0 - interior_color.a);
        color = color + interior_color;
        color = color + frame_color * (1.0 - color.a * 0.5);
        color = color + arch_highlight;

        if color.a < 0.002 { discard; }
        return fog(color, in.world_pos);

    } else {
        // ── Normal card ──
        let fill_alpha = 1.0 - smoothstep(-aa, aa, dist);

        // Texture sampling (when has_texture > 0.5, blend texture with fill)
        var base_fill = in.fill_color;
        if in.has_texture > 0.5 {
            let tex = textureSample(card_texture, card_sampler, in.tex_uv);
            base_fill = vec4<f32>(tex.rgb * tex.a, tex.a);
        }

        var color = base_fill * fill_alpha;

        // Border
        let bw = 2.0;
        let inner = dist + bw;
        let border_mask = fill_alpha * (1.0 - (1.0 - smoothstep(-aa, aa, inner)));
        color = mix(color, in.border_color * fill_alpha, border_mask);

        // Glow
        if in.glow_intensity > 0.001 {
            let glow_falloff = 0.15;
            let glow_alpha = exp(-max(dist, 0.0) * glow_falloff) * in.glow_intensity;
            let glow = in.glow_color * glow_alpha;
            color = color + glow * (1.0 - color.a);
        }

        if color.a < 0.002 { discard; }
        return fog(color, in.world_pos);
    }
}

// ════════════════════════════════════════════════════
//  Background + Floor grid
// ════════════════════════════════════════════════════

struct BgVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) ray_dir: vec3<f32>,
}

var<private> FULLSCREEN: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
    vec2<f32>(-1.0,  1.0),
);

@vertex
fn vs_bg(@builtin(vertex_index) vi: u32) -> BgVertexOutput {
    var out: BgVertexOutput;
    let pos = FULLSCREEN[vi];
    out.position = vec4<f32>(pos, 0.999, 1.0);
    out.uv = pos * 0.5 + 0.5;

    // Unproject screen position to world ray direction
    let near = camera.inv_view_proj * vec4<f32>(pos.x, pos.y, 0.0, 1.0);
    let far  = camera.inv_view_proj * vec4<f32>(pos.x, pos.y, 1.0, 1.0);
    out.ray_dir = normalize(far.xyz / far.w - near.xyz / near.w);

    return out;
}

fn hash(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

fn hash2(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453),
        fract(sin(dot(p, vec2<f32>(269.5, 183.3))) * 43758.5453)
    );
}

fn warp_hash(p: f32) -> f32 {
    return fract(sin(p * 127.1) * 43758.5453);
}

// Simplex-ish value noise for clouds
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    for (var i = 0; i < 4; i++) {
        val += amp * vnoise(pos);
        pos *= 2.1;
        amp *= 0.5;
    }
    return val;
}

@fragment
fn fs_bg(in: BgVertexOutput) -> @location(0) vec4<f32> {
    let t = camera.time;
    let warp = camera.warp_progress;
    let uv = in.uv;
    let rd = normalize(in.ray_dir);

    // Spherical coordinates from ray direction (world-space sky dome)
    let sky_yaw = atan2(rd.x, -rd.z);     // horizontal angle
    let sky_pitch = asin(clamp(rd.y, -1.0, 1.0)); // vertical angle
    let sky_u = sky_yaw / 6.28318 + 0.5;  // [0, 1]
    let sky_v = sky_pitch / 3.14159 + 0.5; // [0, 1], 0=down, 1=up

    // ── Night sky gradient ──
    let horizon_t = smoothstep(-0.05, 0.5, sky_pitch);
    let sky_bottom = vec3<f32>(0.04, 0.02, 0.08);
    let sky_top = vec3<f32>(0.005, 0.008, 0.025);
    var col = mix(sky_bottom, sky_top, horizon_t);

    // Horizon glow
    let horizon_glow = exp(-pow(sky_pitch * 6.0, 2.0));
    col += vec3<f32>(0.06, 0.025, 0.04) * horizon_glow;

    // ── Stars (sky-dome based) ──
    for (var layer = 0; layer < 3; layer++) {
        let scale = 40.0 + f32(layer) * 60.0;
        let drift = t * (0.002 + f32(layer) * 0.001);
        let star_uv = vec2<f32>(sky_u, sky_v) * scale + vec2<f32>(drift, f32(layer) * 50.0);
        let cell = floor(star_uv);
        let local = fract(star_uv) - 0.5;

        let h = hash2(cell);
        let star_pos = h - 0.5;
        let d = length(local - (star_pos * 0.6));

        let brightness = h.x * h.y;
        // Stars visible above horizon
        let sky_mask = smoothstep(-0.02, 0.15, sky_pitch);
        let twinkle = 0.7 + 0.3 * sin(t * (2.0 + h.x * 4.0) + h.y * 6.28);

        let star_size = 0.008 + brightness * 0.015;
        let star = smoothstep(star_size, 0.0, d) * brightness * sky_mask * twinkle;

        let star_color = mix(
            vec3<f32>(0.7, 0.8, 1.0),
            vec3<f32>(1.0, 0.9, 0.75),
            h.x
        );
        col += star_color * star * (0.3 + f32(layer) * 0.15);
    }

    // ── Atmospheric haze / subtle nebula (sky-dome based) ──
    let cloud_uv = vec2<f32>(sky_u, sky_v) * 2.0 + vec2<f32>(t * 0.01, t * 0.005);
    let cloud = fbm(cloud_uv) * fbm(cloud_uv * 1.5 + 3.0);
    let haze_mask = smoothstep(-0.02, 0.2, sky_pitch) * (1.0 - smoothstep(0.4, 0.8, sky_pitch));
    col += vec3<f32>(0.03, 0.015, 0.05) * cloud * haze_mask;

    // ── Subtle vignette (screen-space, this is fine) ──
    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 0.8;
    col *= clamp(vignette, 0.5, 1.0);

    // ── Warp effect (screen-space, intentional) ──
    if warp > 0.001 {
        let uv_c = uv - 0.5;
        let warp_i = sin(warp * 3.14159);

        let angle = atan2(uv_c.y, uv_c.x);
        let radius = length(uv_c);

        let line_count = 100.0;
        let line_angle = fract(angle * line_count / 6.28318);
        let line_hash = warp_hash(floor(angle * line_count / 6.28318));

        let line_bright = smoothstep(0.42, 0.46, line_angle) * (1.0 - smoothstep(0.54, 0.58, line_angle));
        let radial_fade = smoothstep(0.02, 0.12, radius) * (1.0 - smoothstep(0.35, 0.65, radius));
        let streak = line_bright * radial_fade * warp_i;

        let streak_color = mix(
            vec3<f32>(0.4, 0.5, 1.0),
            vec3<f32>(0.9, 0.92, 1.0),
            line_hash * 0.7
        );
        col += streak_color * streak * 2.0;

        let center_glow = exp(-radius * 5.0) * warp_i * 0.6;
        col += vec3<f32>(0.5, 0.6, 1.0) * center_glow;

        let flash = exp(-pow((warp - 0.5) * 6.0, 2.0));
        col += vec3<f32>(0.8, 0.85, 1.0) * flash * 0.4;

        let tunnel_vig = smoothstep(0.25, 0.6, radius) * warp_i;
        col *= 1.0 - tunnel_vig * 0.6;
    }

    return vec4<f32>(col, 1.0);
}

// ════════════════════════════════════════════════════
//  Floor grid (perspective ground plane)
// ════════════════════════════════════════════════════

struct FloorVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_xz: vec2<f32>,
    @location(1) world_pos: vec3<f32>,
}

// Large ground quad vertices (xz plane at y = floor_y)
var<private> FLOOR_QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
    vec2<f32>(-1.0,  1.0),
);

@vertex
fn vs_floor(@builtin(vertex_index) vi: u32) -> FloorVertexOutput {
    var out: FloorVertexOutput;
    let floor_y = -1.5;
    let extent = 30.0;
    let xz = FLOOR_QUAD[vi] * extent;
    let world = vec3<f32>(xz.x, floor_y, xz.y);
    out.clip_position = camera.view_proj * vec4<f32>(world, 1.0);
    out.world_xz = xz;
    out.world_pos = world;
    return out;
}

@fragment
fn fs_floor(in: FloorVertexOutput) -> @location(0) vec4<f32> {
    let t = camera.time;
    let dist = length(in.world_pos - camera.eye_pos);
    let fade = exp(-dist * 0.1);

    // Fine grid
    let grid_scale = 1.0;
    let gx = abs(fract(in.world_xz.x * grid_scale + 0.5) - 0.5);
    let gy = abs(fract(in.world_xz.y * grid_scale + 0.5) - 0.5);
    let line_w = 0.015;
    let grid = 1.0 - min(
        smoothstep(0.0, line_w, gx),
        smoothstep(0.0, line_w, gy)
    );

    // Cool-toned grid — subtle blue-silver
    let grid_color = vec3<f32>(0.12, 0.14, 0.22) * grid * fade;

    // Axis highlight — slightly brighter
    let ax = 1.0 - smoothstep(0.0, 0.03, abs(in.world_xz.x));
    let az = 1.0 - smoothstep(0.0, 0.03, abs(in.world_xz.y));
    let axis = max(ax, az) * fade;
    let axis_color = vec3<f32>(0.2, 0.25, 0.45) * axis;

    // Ground surface base — very faint, gives a "ground" feeling
    let surface = 0.012 * fade;
    let surface_color = vec3<f32>(0.04, 0.045, 0.07) * surface;

    // Subtle reflection of sky near camera
    let reflect_fade = exp(-dist * 0.3) * 0.03;
    let reflect_color = vec3<f32>(0.05, 0.04, 0.08) * reflect_fade;

    let col = grid_color + axis_color + surface_color + reflect_color;
    let alpha = (grid * fade + axis * 0.4 + surface) * 0.85;

    if alpha < 0.003 { discard; }

    return vec4<f32>(col, alpha);
}
