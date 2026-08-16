// WGSL shader for SSAO.

pub(super) const SSAO_SHADER_SRC: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = vec2<f32>(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
    return out;
}

struct SSAOParams {
    radius: f32,
    bias: f32,
    intensity: f32,
    _pad0: f32,
    screen_size: vec2<f32>,
    _pad1: vec2<f32>,
};

@group(0) @binding(0) var gbuffer_depth: texture_depth_2d;
@group(0) @binding(1) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuffer_sampler: sampler;

struct FrameUniforms {
    params: SSAOParams,
    proj_mat: mat4x4<f32>,
    inv_proj_mat: mat4x4<f32>,
    view_mat: mat4x4<f32>,
};

@group(1) @binding(0) var<uniform> frame: FrameUniforms;

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let h = fract(sin(vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)))) * 43758.5453);
    return h * 2.0 - 1.0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let norm_sample = textureSample(gbuffer_normal, gbuffer_sampler, uv);

    // Sky check: GBuffer normal is (0,0,0) after clear
    if (norm_sample.r == 0.0 && norm_sample.g == 0.0 && norm_sample.b == 0.0) {
        return vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }

    // Reconstruct view-space position from depth + UV
    let depth_sample = textureSample(gbuffer_depth, gbuffer_sampler, uv);
    let clip_uv = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let clip_pos = vec4<f32>(clip_uv.x, clip_uv.y, depth_sample, 1.0);
    let view_pos4 = frame.inv_proj_mat * clip_pos;
    let view_pos = view_pos4.xyz / view_pos4.w;
    let frag_view_z = view_pos.z;

    // Normal: read from GBuffer, transform to view space
    let world_N = normalize(norm_sample.xyz * 2.0 - 1.0);
    let view_N4 = frame.view_mat * vec4<f32>(world_N, 0.0);
    let view_N = normalize(view_N4.xyz);

    // Random rotation (hash-based)
    let rot = hash2(uv * 1024.0);
    let rvec = vec3<f32>(rot.x, rot.y, 0.0);

    // TBN in VIEW space
    let tangent = normalize(rvec - view_N * dot(rvec, view_N));
    let bitangent = cross(view_N, tangent);

    var occlusion: f32 = 0.0;

    // ── 16-sample hemisphere kernel ───
    const KERNEL: array<vec3<f32>, 16> = array<vec3<f32>, 16>(
        vec3<f32>( 0.5381,  0.1856,  0.4319),
        vec3<f32>( 0.1379,  0.1967,  0.8544),
        vec3<f32>(-0.3476,  0.4789,  0.5346),
        vec3<f32>(-0.4791,  0.3456,  0.4765),
        vec3<f32>( 0.3883, -0.2607,  0.5915),
        vec3<f32>(-0.4340, -0.4412,  0.5311),
        vec3<f32>( 0.2380, -0.3947,  0.6678),
        vec3<f32>(-0.2238, -0.5672,  0.4478),
        vec3<f32>( 0.3065,  0.0922,  0.1805),
        vec3<f32>( 0.1244,  0.6114,  0.2616),
        vec3<f32>( 0.4086,  0.4489,  0.2630),
        vec3<f32>(-0.4856,  0.3076,  0.1789),
        vec3<f32>(-0.3801, -0.3943,  0.1389),
        vec3<f32>(-0.1088, -0.6461,  0.0838),
        vec3<f32>( 0.2919, -0.4453,  0.3042),
        vec3<f32>( 0.3513, -0.1539,  0.0345),
    );

    let view_radius = frame.params.radius;
    let view_bias = frame.params.bias;

    for (var i: u32 = 0u; i < 16u; i = i + 1u) {
        let ks = KERNEL[i];
        let sd = tangent * ks.x + bitangent * ks.y + view_N * ks.z;
        let sv = view_pos + sd * view_radius;
        let sc = frame.proj_mat * vec4<f32>(sv, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_depth = textureSample(gbuffer_depth, gbuffer_sampler, suv);
            let occ_clip = vec4<f32>(suv.x * 2.0 - 1.0, 1.0 - suv.y * 2.0, occ_depth, 1.0);
            let occ_view = frame.inv_proj_mat * occ_clip;
            let occ_view_z = occ_view.z / occ_view.w;
            if (occ_view_z >= sv.z + view_bias) {
                let z_delta = abs(frag_view_z - occ_view_z);
                // Range falloff: attenuate occlusion when the occluder is farther
                // than `radius` away in view-space Z. This prevents distant surfaces
                // from casting dark halos.
                let range_attenuation = 1.0 - smoothstep(0.0, view_radius, z_delta);
                occlusion += range_attenuation;
            }
        }
    }

    // Final AO value (per LearnOpenGL: invert, power)
    occlusion = 1.0 - occlusion / 16.0;
    occlusion = pow(occlusion, frame.params.intensity);

    // TODO: Add a separate AO blur pass. Inline bilateral blur was removed
    // because a fragment shader cannot sample its own output texture.
    // The previous code did `blurred += occlusion * w` which always evaluates
    // to `occlusion` (center value only, no actual neighbor sampling).

    return vec4<f32>(occlusion, 0.0, 0.0, 1.0);
}
"#;
