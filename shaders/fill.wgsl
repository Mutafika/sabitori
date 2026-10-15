// Sabitori path fill.
//
// パスの塗り (`path().fill(..)`) を、CPU で三角形に割った頂点のまま描く。色は
// straight で届くので、ここで前乗算する。縁は描く側が細い線を重ねて滑らかにする。

struct Globals {
    screen_size: vec2<f32>,
    scale_factor: f32,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

struct FillVertex {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    // 切り抜き (論理 px の x, y, w, h)。w == 0 || h == 0 → 切らない。
    @location(2) clip_rect: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) clip_rect: vec4<f32>,
}

@vertex
fn vs_main(v: FillVertex) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(
        v.position.x / globals.screen_size.x * 2.0 - 1.0,
        1.0 - v.position.y / globals.screen_size.y * 2.0,
        0.0,
        1.0,
    );
    out.color = v.color;
    out.clip_rect = v.clip_rect;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.clip_rect.z > 0.0 && in.clip_rect.w > 0.0 {
        let p = in.position.xy / globals.scale_factor;
        let lo = in.clip_rect.xy;
        let hi = in.clip_rect.xy + in.clip_rect.zw;
        if p.x < lo.x || p.x > hi.x || p.y < lo.y || p.y > hi.y {
            discard;
        }
    }
    return vec4<f32>(in.color.rgb * in.color.a, in.color.a);
}
