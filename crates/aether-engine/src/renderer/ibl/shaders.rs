// WGSL shaders used by IBL cubemap generation.

// ── WGSL Shaders ─────────────────────────────────────────────────────

pub(super) const EQUIRECT_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec3<f32>,
};
struct Uniforms { proj: mat4x4<f32>, };
struct Flip { flip_x: u32, flip_y: u32, flip_z: u32, _pad: u32, };

@group(0) @binding(0) var<uniform> proj: Uniforms;
@group(0) @binding(1) var<uniform> view: Uniforms;

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.local_pos = pos;
    out.clip_position = proj.proj * view.proj * vec4<f32>(pos, 1.0);
    return out;
}

const invAtan: vec2<f32> = vec2<f32>(0.1591, 0.3183);
fn sample_spherical_map(v: vec3<f32>, flip_x: u32, flip_y: u32, flip_z: u32) -> vec2<f32> {
    let vx = select(v.x, -v.x, flip_x == 1u);
    let vy = select(v.y, -v.y, flip_y == 1u);
    let vz = select(v.z, -v.z, flip_z == 1u);
    var uv = vec2<f32>(atan2(vz, vx), asin(vy));
    uv = uv * invAtan;
    uv = uv + vec2<f32>(0.5);
    return uv;
}

@group(0) @binding(2) var<uniform> flip: Flip;

@group(1) @binding(0) var equirect_map: texture_2d<f32>;
@group(1) @binding(1) var equirect_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.local_pos);
    let uv = sample_spherical_map(dir, flip.flip_x, flip.flip_y, flip.flip_z);
    return textureSample(equirect_map, equirect_sampler, uv);
}
"#;

pub(super) const IRRADIANCE_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec3<f32>,
};
struct Uniforms { proj: mat4x4<f32>, };

@group(0) @binding(0) var<uniform> proj: Uniforms;
@group(0) @binding(1) var<uniform> view: Uniforms;

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.local_pos = pos;
    out.clip_position = proj.proj * view.proj * vec4<f32>(pos, 1.0);
    return out;
}

const PI: f32 = 3.14159265359;
const sample_delta: f32 = 0.025;

@group(1) @binding(0) var environment_map: texture_cube<f32>;
@group(1) @binding(1) var env_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.local_pos);
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(N.y) > 0.999) { up = vec3<f32>(1.0, 0.0, 0.0); }
    let right = normalize(cross(up, N));
    up = cross(N, right);

    var irradiance = vec3<f32>(0.0);
    var nr_samples: f32 = 0.0;

    var phi: f32 = 0.0;
    while (phi < 2.0 * PI) {
        var theta: f32 = 0.0;
        while (theta < 0.5 * PI) {
            let sin_theta = sin(theta);
            let tangent_sample = vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), cos(theta));
            let sample_vec = tangent_sample.x * right + tangent_sample.y * up + tangent_sample.z * N;
            irradiance += textureSample(environment_map, env_sampler, sample_vec).rgb * cos(theta) * sin_theta;
            nr_samples += 1.0;
            theta += sample_delta;
        }
        phi += sample_delta;
    }
    irradiance = PI * irradiance / nr_samples;
    return vec4<f32>(irradiance, 1.0);
}
"#;

pub(super) const PREFILTER_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec3<f32>,
};
struct Uniforms { proj: mat4x4<f32>, };

@group(0) @binding(0) var<uniform> proj: Uniforms;
@group(0) @binding(1) var<uniform> view: Uniforms;

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.local_pos = pos;
    out.clip_position = proj.proj * view.proj * vec4<f32>(pos, 1.0);
    return out;
}

const PI: f32 = 3.14159265359;
const SAMPLE_COUNT: u32 = 1024u;

fn radical_inverse_vdc(bits_in: u32) -> f32 {
    var bits = bits_in;
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10;
}
fn hammersley(i: u32, N: u32) -> vec2<f32> {
    return vec2<f32>(f32(i) / f32(N), radical_inverse_vdc(i));
}
fn importance_sample_ggx(Xi: vec2<f32>, N: vec3<f32>, roughness: f32) -> vec3<f32> {
    let a = roughness * roughness;
    let phi = 2.0 * PI * Xi.x;
    let cos_theta = sqrt((1.0 - Xi.y) / (1.0 + (a * a - 1.0) * Xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
    let H = vec3<f32>(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(N.z) < 0.999);
    let tangent = normalize(cross(up, N));
    let bitangent = cross(N, tangent);
    return normalize(tangent * H.x + bitangent * H.y + N * H.z);
}

struct Roughness { roughness: f32, _pad: f32, _pad2: f32, _pad3: f32, };

@group(1) @binding(0) var environment_map: texture_cube<f32>;
@group(1) @binding(1) var env_sampler: sampler;

@group(2) @binding(0) var<uniform> u_roughness: Roughness;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.local_pos);
    let R = N;
    let V = R;
    let roughness = u_roughness.roughness;

    var prefiltered_color = vec3<f32>(0.0);
    var total_weight: f32 = 0.0;

    for (var i: u32 = 0u; i < SAMPLE_COUNT; i = i + 1u) {
        let Xi = hammersley(i, SAMPLE_COUNT);
        let H = importance_sample_ggx(Xi, N, roughness);
        let L = normalize(2.0 * dot(V, H) * H - V);
        let NdotL = max(dot(N, L), 0.0);
        if (NdotL > 0.0) {
            prefiltered_color += textureSample(environment_map, env_sampler, L).rgb * NdotL;
            total_weight += NdotL;
        }
    }
    prefiltered_color = prefiltered_color / total_weight;
    return vec4<f32>(prefiltered_color, 1.0);
}
"#;

pub(super) const BRDF_LUT_SHADER: &str = r#"
@group(0) @binding(0) var output_lut: texture_storage_2d<rgba16float, write>;

const PI: f32 = 3.14159265359;
const SAMPLE_COUNT: u32 = 1024u;

fn radical_inverse_vdc(bits_in: u32) -> f32 {
    var bits = bits_in;
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10;
}
fn hammersley(i: u32, N: u32) -> vec2<f32> { return vec2<f32>(f32(i) / f32(N), radical_inverse_vdc(i)); }
fn importance_sample_ggx(Xi: vec2<f32>, N: vec3<f32>, roughness: f32) -> vec3<f32> {
    let a = roughness * roughness;
    let phi = 2.0 * PI * Xi.x;
    let cos_theta = sqrt((1.0 - Xi.y) / (1.0 + (a * a - 1.0) * Xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
    let H = vec3<f32>(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(N.z) < 0.999);
    let tangent = normalize(cross(up, N));
    let bitangent = cross(N, tangent);
    return normalize(tangent * H.x + bitangent * H.y + N * H.z);
}
fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;  // Schlick-GGX (matches LearnOpenGL tutorial)
    return (NdotV / (NdotV * (1.0 - k) + k)) * (NdotL / (NdotL * (1.0 - k) + k));
}
fn integrate_brdf(NdotV: f32, roughness: f32) -> vec2<f32> {
    let V = vec3<f32>(sqrt(1.0 - NdotV * NdotV), 0.0, NdotV);
    let N = vec3<f32>(0.0, 0.0, 1.0);
    var A: f32 = 0.0;
    var B: f32 = 0.0;
    for (var i: u32 = 0u; i < SAMPLE_COUNT; i = i + 1u) {
        let Xi = hammersley(i, SAMPLE_COUNT);
        let H = importance_sample_ggx(Xi, N, roughness);
        let L = normalize(2.0 * dot(V, H) * H - V);
        let NdotL = max(L.z, 0.0);
        let NdotH = max(H.z, 0.0);
        let VdotH = max(dot(V, H), 0.0);
        if (NdotL > 0.0) {
            let G = geometry_smith(NdotV, NdotL, roughness);
            let G_vis = (G * VdotH) / (NdotH * NdotV);
            let Fc = pow(1.0 - VdotH, 5.0);
            A += (1.0 - Fc) * G_vis;
            B += Fc * G_vis;
        }
    }
    return vec2<f32>(A, B) / f32(SAMPLE_COUNT);
}
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= $size || id.y >= $size) { return; }
    let NdotV = (f32(id.x) + 0.5) / f32($size);
    let roughness = (f32(id.y) + 0.5) / f32($size);
    let result = integrate_brdf(NdotV, roughness);
    textureStore(output_lut, vec2<i32>(id.xy), vec4<f32>(result, 0.0, 1.0));
}
"#;
