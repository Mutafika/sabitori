// Sabitori Separable Gaussian Blur Compute Shader
// Two-pass: horizontal then vertical.

@group(0) @binding(0)
var input_texture: texture_2d<f32>;

@group(0) @binding(1)
var output_texture: texture_storage_2d<rgba8unorm, write>;

struct BlurParams {
    direction: vec2<f32>,  // (1,0) for horizontal, (0,1) for vertical
    radius: f32,
    _pad: f32,
}

@group(0) @binding(2)
var<uniform> params: BlurParams;

// Gaussian weight
fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(2.0 * 3.14159265) * sigma);
}

@compute @workgroup_size(16, 16)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(input_texture);
    if id.x >= dims.x || id.y >= dims.y {
        return;
    }

    let sigma = params.radius * 0.5;
    let kernel_size = i32(ceil(params.radius * 2.0));
    let coord = vec2<i32>(i32(id.x), i32(id.y));

    var color = vec4<f32>(0.0);
    var total_weight = 0.0;

    for (var i = -kernel_size; i <= kernel_size; i++) {
        let offset = vec2<i32>(params.direction * f32(i));
        let sample_coord = clamp(
            coord + offset,
            vec2<i32>(0),
            vec2<i32>(i32(dims.x) - 1, i32(dims.y) - 1)
        );
        let weight = gaussian(f32(i), sigma);
        color += textureLoad(input_texture, sample_coord, 0) * weight;
        total_weight += weight;
    }

    color /= total_weight;
    textureStore(output_texture, vec2<i32>(id.xy), color);
}
