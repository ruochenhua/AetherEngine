//! Weather map generator for volumetric clouds.
//!
//! Ported from RenderEngine's shaders/clouds/generation/weather.comp.

@group(0) @binding(0)
var output_tex: texture_storage_2d<rgba8unorm, write>;

const PERLIN_AMPLITUDE: f32 = 0.5;
const PERLIN_FREQUENCY: f32 = 0.92;
const PERLIN_SCALE: f32 = 50.0;
const PERLIN_OCTAVES: i32 = 8;

fn random2d(st: vec2<f32>) -> f32 {
    return fract(sin(dot(st.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453123);
}

fn noise_interpolation(coord: vec2<f32>, size: f32) -> f32 {
    let grid = coord * size;
    let random_input = floor(grid);
    var weights = fract(grid);

    let p0 = random2d(random_input);
    let p1 = random2d(random_input + vec2<f32>(1.0, 0.0));
    let p2 = random2d(random_input + vec2<f32>(0.0, 1.0));
    let p3 = random2d(random_input + vec2<f32>(1.0, 1.0));

    weights = smoothstep(vec2<f32>(0.0), vec2<f32>(1.0), weights);

    return p0 +
        (p1 - p0) * weights.x +
        (p2 - p0) * weights.y * (1.0 - weights.x) +
        (p3 - p1) * weights.y * weights.x;
}

fn perlin_noise(uv: vec2<f32>, sc: f32, f: f32, a: f32, o: i32) -> f32 {
    var noise_value = 0.0;
    var local_amplitude = a;
    var local_frequency = f;

    for (var index: i32 = 0; index < o; index = index + 1) {
        noise_value += noise_interpolation(uv, sc * local_frequency) * local_amplitude;
        local_amplitude *= 0.5;
        local_frequency *= 2.0;
    }

    return (noise_value - 0.2) / 0.8;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let pixel = vec2<i32>(global_id.xy);
    let dx = 1.0 / 2048.0;
    let dy = 1.0 / 2048.0;
    let uv = vec2<f32>(f32(pixel.x) * dx, f32(pixel.y) * dy);

    let coverage = perlin_noise(uv, PERLIN_SCALE, PERLIN_FREQUENCY, PERLIN_AMPLITUDE, PERLIN_OCTAVES);

    let weather = vec4<f32>(clamp(coverage, 0.0, 1.0), 0.75, 0.0, 1.0);
    textureStore(output_tex, pixel, weather);
}
