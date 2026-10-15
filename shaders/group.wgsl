// Sabitori group compositor.
//
// 1 つの要素とその子孫を、窓と同じ大きさの別のテクスチャに先に描き
// (`OrderedFrame` のグループ)、ここでそれを 1 枚の絵として貼る。貼るときに
// 回す・薄くするので、中身の箱・文字・画像・切り抜きがまとめて同じように回る。
//
// テクスチャは前乗算の線形の色 (sRGB 形式なので読むと線形に戻る) を持つ。

struct Globals {
    screen_size: vec2<f32>,
    scale_factor: f32,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(1) @binding(0)
var group_tex: texture_2d<f32>;
@group(1) @binding(1)
var group_sampler: sampler;

// マスク (`fs_masked` だけが使う)。不透明度だけを見る。
@group(2) @binding(0)
var mask_tex: texture_2d<f32>;
@group(2) @binding(1)
var mask_sampler: sampler;

struct GroupInstance {
    // 回す中心 x, y (論理 px)、回転 (ラジアン、画面上で時計回り)、不透明度。
    @location(0) pivot_rot: vec4<f32>,
    // 祖先の切り抜き (論理 px の x, y, w, h)。w == 0 || h == 0 → 切らない。
    @location(1) clip_rect: vec4<f32>,
    // ぼかし, 重ね方, 倍率 (0 は 1), 空き。ここで読むのは倍率だけ。
    @location(2) effect: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
    @location(2) clip_rect: vec4<f32>,
}

var<private> QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: GroupInstance) -> VertexOutput {
    let uv = QUAD[vi];
    let pivot = inst.pivot_rot.xy;
    let angle = inst.pivot_rot.z;
    let c = cos(angle);
    let s = sin(angle);
    let zoom = select(1.0, inst.effect.z, inst.effect.z > 0.0);
    let d = (uv * globals.screen_size - pivot) * zoom;
    let p = pivot + vec2<f32>(d.x * c - d.y * s, d.x * s + d.y * c);
    var out: VertexOutput;
    out.position = vec4<f32>(
        p.x / globals.screen_size.x * 2.0 - 1.0,
        1.0 - p.y / globals.screen_size.y * 2.0,
        0.0,
        1.0,
    );
    out.uv = uv;
    out.opacity = inst.pivot_rot.w;
    out.clip_rect = inst.clip_rect;
    return out;
}

fn clipped(in: VertexOutput) -> bool {
    if in.clip_rect.z <= 0.0 || in.clip_rect.w <= 0.0 {
        return false;
    }
    let p = in.position.xy / globals.scale_factor;
    let lo = in.clip_rect.xy;
    let hi = in.clip_rect.xy + in.clip_rect.zw;
    return p.x < lo.x || p.x > hi.x || p.y < lo.y || p.y > hi.y;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if clipped(in) {
        discard;
    }
    return textureSample(group_tex, group_sampler, in.uv) * in.opacity;
}

// マスクはグループと同じ座標で描いてあるので、同じ uv で読む (一緒に回る)。
@fragment
fn fs_masked(in: VertexOutput) -> @location(0) vec4<f32> {
    if clipped(in) {
        discard;
    }
    let m = textureSample(mask_tex, mask_sampler, in.uv).a;
    return textureSample(group_tex, group_sampler, in.uv) * (in.opacity * m);
}
