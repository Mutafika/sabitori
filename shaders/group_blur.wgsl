// Sabitori group blur.
//
// グループ (要素とその子孫を先に描いた絵) を、貼る前にぼかす。縦横に分けた
// ガウスのぼかしを 2 回のパスで掛ける (横: 絵 → 作業用、縦: 作業用 → 絵)。
// 計算シェーダを使わないのは WebGL でも動かすため。
//
// 絵は前乗算の線形の色 (sRGB 形式なので読むと線形に戻る)。前乗算のまま混ぜれば、
// 透明な所との境目が黒ずまない。

struct BlurParams {
    // 1 画素ぶんの uv の歩み (横なら (1/w, 0)、縦なら (0, 1/h))。
    step: vec2<f32>,
    // 標準偏差 (物理 px)。
    sigma: f32,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> params: BlurParams;

@group(1) @binding(0)
var src_tex: texture_2d<f32>;
@group(1) @binding(1)
var src_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// 画面を覆う大きな三角形 1 枚。
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    var out: VertexOutput;
    out.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sigma = max(params.sigma, 0.001);
    // 3σ まで。広いぼかしは標本の間を空け、間は線形補間に任せる (64 回まで)。
    let reach = 3.0 * sigma;
    let count = min(ceil(reach), 64.0);
    let spacing = reach / max(count, 1.0);
    var sum = textureSampleLevel(src_tex, src_sampler, in.uv, 0.0);
    var weight = 1.0;
    for (var i = 1.0; i <= count; i += 1.0) {
        let d = i * spacing;
        let w = exp(-(d * d) / (2.0 * sigma * sigma));
        let off = params.step * d;
        sum += (textureSampleLevel(src_tex, src_sampler, in.uv + off, 0.0)
              + textureSampleLevel(src_tex, src_sampler, in.uv - off, 0.0)) * w;
        weight += 2.0 * w;
    }
    return sum / weight;
}
