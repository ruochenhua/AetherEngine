//! 3D Worley-octave texture generator.
//!
//! One-to-one port of NadirRoGue/RenderEngine's shaders/clouds/generation/worley.comp.
//! Outputs a 32^3 RGBA8 texture with 3 Worley-octave channels:
//!   R = 1x cell frequency
//!   G = 2x cell frequency
//!   B = 4x cell frequency
//!   A = 1.0 (unused, preserved for RGBA layout compatibility)

@group(0) @binding(0)
var output_tex: texture_storage_3d<rgba8unorm, write>;

fn hash(n: f32) -> f32 {
    return fract(sin(n + 1.951) * 43758.5453123);
}

fn noise(x: vec3<f32>) -> f32 {
    let p = floor(x);
    var f = fract(x);
    f = f * f * (vec3<f32>(3.0) - vec3<f32>(2.0) * f);
    let n = p.x + p.y * 57.0 + 113.0 * p.z;
    return mix(
        mix(
            mix(hash(n + 0.0), hash(n + 1.0), f.x),
            mix(hash(n + 57.0), hash(n + 58.0), f.x),
            f.y),
        mix(
            mix(hash(n + 113.0), hash(n + 114.0), f.x),
            mix(hash(n + 170.0), hash(n + 171.0), f.x),
            f.y),
        f.z);
}

fn mod_v3(x: vec3<f32>, y: vec3<f32>) -> vec3<f32> {
    return x - floor(x / y) * y;
}

fn cells(p: vec3<f32>, cell_count: f32) -> f32 {
    let p_cell = p * cell_count;
    var d: f32 = 1.0e10;
    for (var xo: i32 = -1; xo <= 1; xo = xo + 1) {
        for (var yo: i32 = -1; yo <= 1; yo = yo + 1) {
            for (var zo: i32 = -1; zo <= 1; zo = zo + 1) {
                var tp = floor(p_cell) + vec3<f32>(f32(xo), f32(yo), f32(zo));
                tp = p_cell - tp - noise(mod_v3(tp, vec3<f32>(cell_count)));
                d = min(d, dot(tp, tp));
            }
        }
    }
    return clamp(d, 0.0, 1.0);
}

fn worley_noise_3d(p: vec3<f32>, cell_count: f32) -> f32 {
    return cells(p, cell_count);
}

fn sample_worley_octaves(coord: vec3<f32>) -> vec4<f32> {
    let cell_count = 2.0;
    let w0 = 1.0 - worley_noise_3d(coord, cell_count * 1.0);
    let w1 = 1.0 - worley_noise_3d(coord, cell_count * 2.0);
    let w2 = 1.0 - worley_noise_3d(coord, cell_count * 4.0);
    let w3 = 1.0 - worley_noise_3d(coord, cell_count * 8.0);

    let worley_fbm0 = w0 * 0.625 + w1 * 0.25 + w2 * 0.125;
    let worley_fbm1 = w1 * 0.625 + w2 * 0.25 + w3 * 0.125;
    let worley_fbm2 = w2 * 0.75 + w3 * 0.25;

    return vec4<f32>(worley_fbm0, worley_fbm1, worley_fbm2, 1.0);
}

@compute @workgroup_size(4, 4, 4)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let pixel = vec3<i32>(global_id);
    let coord = vec3<f32>(pixel) / f32(textureDimensions(output_tex).x);
    textureStore(output_tex, pixel, sample_worley_octaves(coord));
}
