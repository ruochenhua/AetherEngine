//! Perlin-Worley + 3 Worley octaves 3D texture generator.
//!
//! Ported from RenderEngine's shaders/clouds/generation/perlinworley.comp,
//! which is based on Sebastien Hillarie's TileableVolumeNoise.

const FREQUENCE_MUL: array<f32, 6> = array<f32, 6>(2.0, 8.0, 14.0, 20.0, 26.0, 32.0);

@group(0) @binding(0)
var output_tex: texture_storage_3d<rgba8unorm, write>;

// =====================================================================================
// TileableVolumeNoise primitives

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

// =====================================================================================
// GLM classic 4D Perlin

fn mod_v4(x: vec4<f32>, y: vec4<f32>) -> vec4<f32> {
    return x - floor(x / y) * y;
}

fn mod289_v4(x: vec4<f32>) -> vec4<f32> {
    return x - floor(x * (1.0 / 289.0)) * 289.0;
}

fn permute_v4(x: vec4<f32>) -> vec4<f32> {
    return mod289_v4(((x * 34.0) + vec4<f32>(1.0)) * x);
}

fn taylor_inv_sqrt(r: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(1.7928429) - vec4<f32>(0.8537347) * r;
}

fn fade_v4(t: vec4<f32>) -> vec4<f32> {
    return (t * t * t) * (t * (t * vec4<f32>(6.0) - vec4<f32>(15.0)) + vec4<f32>(10.0));
}

fn glm_perlin_4d(position: vec4<f32>, rep: vec4<f32>) -> f32 {
    var pi0 = mod_v4(floor(position), rep);
    var pi1 = mod_v4(pi0 + vec4<f32>(1.0), rep);

    let pf0 = fract(position);
    let pf1 = pf0 - vec4<f32>(1.0);

    let ix = vec4<f32>(pi0.x, pi1.x, pi0.x, pi1.x);
    let iy = vec4<f32>(pi0.y, pi0.y, pi1.y, pi1.y);
    let iz0 = vec4<f32>(pi0.z);
    let iz1 = vec4<f32>(pi1.z);
    let iw0 = vec4<f32>(pi0.w);
    let iw1 = vec4<f32>(pi1.w);

    let ixy = permute_v4(permute_v4(ix) + iy);
    let ixy0 = permute_v4(ixy + iz0);
    let ixy1 = permute_v4(ixy + iz1);
    let ixy00 = permute_v4(ixy0 + iw0);
    let ixy01 = permute_v4(ixy0 + iw1);
    let ixy10 = permute_v4(ixy1 + iw0);
    let ixy11 = permute_v4(ixy1 + iw1);

    var gx00 = ixy00 / vec4<f32>(7.0);
    var gy00 = floor(gx00) / vec4<f32>(7.0);
    var gz00 = floor(gy00) / vec4<f32>(6.0);
    gx00 = fract(gx00) - vec4<f32>(0.5);
    gy00 = fract(gy00) - vec4<f32>(0.5);
    gz00 = fract(gz00) - vec4<f32>(0.5);
    var gw00 = vec4<f32>(0.75) - abs(gx00) - abs(gy00) - abs(gz00);
    let sw00 = step(gw00, vec4<f32>(0.0));
    gx00 -= sw00 * (step(vec4<f32>(0.0), gx00) - vec4<f32>(0.5));
    gy00 -= sw00 * (step(vec4<f32>(0.0), gy00) - vec4<f32>(0.5));

    var gx01 = ixy01 / vec4<f32>(7.0);
    var gy01 = floor(gx01) / vec4<f32>(7.0);
    var gz01 = floor(gy01) / vec4<f32>(6.0);
    gx01 = fract(gx01) - vec4<f32>(0.5);
    gy01 = fract(gy01) - vec4<f32>(0.5);
    gz01 = fract(gz01) - vec4<f32>(0.5);
    var gw01 = vec4<f32>(0.75) - abs(gx01) - abs(gy01) - abs(gz01);
    let sw01 = step(gw01, vec4<f32>(0.0));
    gx01 -= sw01 * (step(vec4<f32>(0.0), gx01) - vec4<f32>(0.5));
    gy01 -= sw01 * (step(vec4<f32>(0.0), gy01) - vec4<f32>(0.5));

    var gx10 = ixy10 / vec4<f32>(7.0);
    var gy10 = floor(gx10) / vec4<f32>(7.0);
    var gz10 = floor(gy10) / vec4<f32>(6.0);
    gx10 = fract(gx10) - vec4<f32>(0.5);
    gy10 = fract(gy10) - vec4<f32>(0.5);
    gz10 = fract(gz10) - vec4<f32>(0.5);
    var gw10 = vec4<f32>(0.75) - abs(gx10) - abs(gy10) - abs(gz10);
    let sw10 = step(gw10, vec4<f32>(0.0));
    gx10 -= sw10 * (step(vec4<f32>(0.0), gx10) - vec4<f32>(0.5));
    gy10 -= sw10 * (step(vec4<f32>(0.0), gy10) - vec4<f32>(0.5));

    var gx11 = ixy11 / vec4<f32>(7.0);
    var gy11 = floor(gx11) / vec4<f32>(7.0);
    var gz11 = floor(gy11) / vec4<f32>(6.0);
    gx11 = fract(gx11) - vec4<f32>(0.5);
    gy11 = fract(gy11) - vec4<f32>(0.5);
    gz11 = fract(gz11) - vec4<f32>(0.5);
    var gw11 = vec4<f32>(0.75) - abs(gx11) - abs(gy11) - abs(gz11);
    let sw11 = step(gw11, vec4<f32>(0.0));
    gx11 -= sw11 * (step(vec4<f32>(0.0), gx11) - vec4<f32>(0.5));
    gy11 -= sw11 * (step(vec4<f32>(0.0), gy11) - vec4<f32>(0.5));

    let g0000 = vec4<f32>(gx00.x, gy00.x, gz00.x, gw00.x);
    let g1000 = vec4<f32>(gx00.y, gy00.y, gz00.y, gw00.y);
    let g0100 = vec4<f32>(gx00.z, gy00.z, gz00.z, gw00.z);
    let g1100 = vec4<f32>(gx00.w, gy00.w, gz00.w, gw00.w);
    let g0010 = vec4<f32>(gx10.x, gy10.x, gz10.x, gw10.x);
    let g1010 = vec4<f32>(gx10.y, gy10.y, gz10.y, gw10.y);
    let g0110 = vec4<f32>(gx10.z, gy10.z, gz10.z, gw10.z);
    let g1110 = vec4<f32>(gx10.w, gy10.w, gz10.w, gw10.w);
    let g0001 = vec4<f32>(gx01.x, gy01.x, gz01.x, gw01.x);
    let g1001 = vec4<f32>(gx01.y, gy01.y, gz01.y, gw01.y);
    let g0101 = vec4<f32>(gx01.z, gy01.z, gz01.z, gw01.z);
    let g1101 = vec4<f32>(gx01.w, gy01.w, gz01.w, gw01.w);
    let g0011 = vec4<f32>(gx11.x, gy11.x, gz11.x, gw11.x);
    let g1011 = vec4<f32>(gx11.y, gy11.y, gz11.y, gw11.y);
    let g0111 = vec4<f32>(gx11.z, gy11.z, gz11.z, gw11.z);
    let g1111 = vec4<f32>(gx11.w, gy11.w, gz11.w, gw11.w);

    let norm00 = taylor_inv_sqrt(vec4<f32>(
        dot(g0000, g0000), dot(g0100, g0100), dot(g1000, g1000), dot(g1100, g1100)));
    let g0000n = g0000 * norm00.x;
    let g0100n = g0100 * norm00.y;
    let g1000n = g1000 * norm00.z;
    let g1100n = g1100 * norm00.w;

    let norm01 = taylor_inv_sqrt(vec4<f32>(
        dot(g0001, g0001), dot(g0101, g0101), dot(g1001, g1001), dot(g1101, g1101)));
    let g0001n = g0001 * norm01.x;
    let g0101n = g0101 * norm01.y;
    let g1001n = g1001 * norm01.z;
    let g1101n = g1101 * norm01.w;

    let norm10 = taylor_inv_sqrt(vec4<f32>(
        dot(g0010, g0010), dot(g0110, g0110), dot(g1010, g1010), dot(g1110, g1110)));
    let g0010n = g0010 * norm10.x;
    let g0110n = g0110 * norm10.y;
    let g1010n = g1010 * norm10.z;
    let g1110n = g1110 * norm10.w;

    let norm11 = taylor_inv_sqrt(vec4<f32>(
        dot(g0011, g0011), dot(g0111, g0111), dot(g1011, g1011), dot(g1111, g1111)));
    let g0011n = g0011 * norm11.x;
    let g0111n = g0111 * norm11.y;
    let g1011n = g1011 * norm11.z;
    let g1111n = g1111 * norm11.w;

    let n0000 = dot(g0000n, vec4<f32>(pf0.x, pf0.y, pf0.z, pf0.w));
    let n1000 = dot(g1000n, vec4<f32>(pf1.x, pf0.y, pf0.z, pf0.w));
    let n0100 = dot(g0100n, vec4<f32>(pf0.x, pf1.y, pf0.z, pf0.w));
    let n1100 = dot(g1100n, vec4<f32>(pf1.x, pf1.y, pf0.z, pf0.w));
    let n0010 = dot(g0010n, vec4<f32>(pf0.x, pf0.y, pf1.z, pf0.w));
    let n1010 = dot(g1010n, vec4<f32>(pf1.x, pf0.y, pf1.z, pf0.w));
    let n0110 = dot(g0110n, vec4<f32>(pf0.x, pf1.y, pf1.z, pf0.w));
    let n1110 = dot(g1110n, vec4<f32>(pf1.x, pf1.y, pf1.z, pf0.w));
    let n0001 = dot(g0001n, vec4<f32>(pf0.x, pf0.y, pf0.z, pf1.w));
    let n1001 = dot(g1001n, vec4<f32>(pf1.x, pf0.y, pf0.z, pf1.w));
    let n0101 = dot(g0101n, vec4<f32>(pf0.x, pf1.y, pf0.z, pf1.w));
    let n1101 = dot(g1101n, vec4<f32>(pf1.x, pf1.y, pf0.z, pf1.w));
    let n0011 = dot(g0011n, vec4<f32>(pf0.x, pf0.y, pf1.z, pf1.w));
    let n1011 = dot(g1011n, vec4<f32>(pf1.x, pf0.y, pf1.z, pf1.w));
    let n0111 = dot(g0111n, vec4<f32>(pf0.x, pf1.y, pf1.z, pf1.w));
    let n1111 = dot(g1111n, pf1);

    let fade_xyzw = fade_v4(pf0);
    let n_0w = mix(
        vec4<f32>(n0000, n1000, n0100, n1100),
        vec4<f32>(n0001, n1001, n0101, n1101),
        fade_xyzw.w);
    let n_1w = mix(
        vec4<f32>(n0010, n1010, n0110, n1110),
        vec4<f32>(n0011, n1011, n0111, n1111),
        fade_xyzw.w);
    let n_zw = mix(n_0w, n_1w, fade_xyzw.z);
    let n_yzw = mix(
        vec2<f32>(n_zw.x, n_zw.y),
        vec2<f32>(n_zw.z, n_zw.w),
        fade_xyzw.y);
    let n_xyzw = mix(n_yzw.x, n_yzw.y, fade_xyzw.x);
    return 2.2 * n_xyzw;
}

fn remap(original: f32, original_min: f32, original_max: f32, new_min: f32, new_max: f32) -> f32 {
    return new_min + (((original - original_min) / (original_max - original_min)) * (new_max - new_min));
}

fn worley_noise_3d(p: vec3<f32>, cell_count: f32) -> f32 {
    return cells(p, cell_count);
}

fn perlin_noise_3d(p_in: vec3<f32>, frequency: f32, octave_count: i32) -> f32 {
    let octave_frequency_factor = 2.0;
    var sum = 0.0;
    var weight_sum = 0.0;
    var weight = 0.5;
    var freq = frequency;
    for (var oct: i32 = 0; oct < octave_count; oct = oct + 1) {
        let p = vec3<f32>(freq) * p_in;
        let val = glm_perlin_4d(vec4<f32>(p, 0.0), vec4<f32>(freq));
        sum += val * weight;
        weight_sum += weight;
        weight *= weight;
        freq *= octave_frequency_factor;
    }
    var n = sum / weight_sum;
    return clamp(n, 0.0, 1.0);
}

fn stackable_3d_noise(pixel: vec3<i32>) -> vec4<f32> {
    let coord = vec3<f32>(pixel) / f32(textureDimensions(output_tex).x);

    let perlin_noise_val = perlin_noise_3d(coord, 8.0, 3);

    var perlin_worley_noise = 0.0;
    {
        let cell_count = 4.0;
        let worley_noise0 = 1.0 - worley_noise_3d(coord, cell_count * FREQUENCE_MUL[0]);
        let worley_noise1 = 1.0 - worley_noise_3d(coord, cell_count * FREQUENCE_MUL[1]);
        let worley_noise2 = 1.0 - worley_noise_3d(coord, cell_count * FREQUENCE_MUL[2]);

        let worley_fbm = worley_noise0 * 0.625 + worley_noise1 * 0.25 + worley_noise2 * 0.125;
        perlin_worley_noise = remap(perlin_noise_val, 0.0, 1.0, worley_fbm, 1.0);
    }

    let cell_count = 4.0;
    let worley_noise0 = 1.0 - worley_noise_3d(coord, cell_count * 1.0);
    let worley_noise1 = 1.0 - worley_noise_3d(coord, cell_count * 2.0);
    let worley_noise2 = 1.0 - worley_noise_3d(coord, cell_count * 4.0);
    let worley_noise3 = 1.0 - worley_noise_3d(coord, cell_count * 8.0);
    let worley_noise4 = 1.0 - worley_noise_3d(coord, cell_count * 16.0);

    let worley_fbm0 = worley_noise1 * 0.625 + worley_noise2 * 0.25 + worley_noise3 * 0.125;
    let worley_fbm1 = worley_noise2 * 0.625 + worley_noise3 * 0.25 + worley_noise4 * 0.125;
    let worley_fbm2 = worley_noise3 * 0.75 + worley_noise4 * 0.25;

    return clamp(vec4<f32>(perlin_worley_noise * perlin_worley_noise, worley_fbm0, worley_fbm1, worley_fbm2), vec4<f32>(0.0), vec4<f32>(1.0));
}

@compute @workgroup_size(4, 4, 4)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let pixel = vec3<i32>(global_id);
    textureStore(output_tex, pixel, stackable_3d_noise(pixel));
}
