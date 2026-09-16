//! WGSL shader for the SSAO bilateral blur.

pub(super) const AO_BLUR_SHADER_SRC: &str = r#"
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

struct BlurParams {
    depth_sigma: f32,
    texel_size: vec2<f32>,
    camera_pos: vec3<f32>,
    camera_forward: vec3<f32>,
};

@group(0) @binding(0) var ao_tex: texture_2d<f32>;
@group(0) @binding(1) var gbuffer_position: texture_2d<f32>;
@group(0) @binding(2) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(3) var ao_sampler: sampler;
@group(0) @binding(4) var gbuffer_sampler: sampler;

@group(1) @binding(0) var<uniform> params: BlurParams;

// Precomputed 3x3 gaussian kernel weights (sigma = 0.85)
const KERNEL_SIZE: i32 = 1;
const KERNEL_WEIGHTS: array<f32, 9> = array<f32, 9>(
    0.077847, 0.123317, 0.077847,
    0.123317, 0.195346, 0.123317,
    0.077847, 0.123317, 0.077847,
);

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;

    // Sky check: AO texture is R8Unorm, cleared to WHITE (1.0).
    // GBuffer position alpha: 1.0 = geometry, 0.0 = sky.
    let center_pos = textureSample(gbuffer_position, gbuffer_sampler, uv);
    if (center_pos.a < 0.5) {
        return vec4<f32>(textureSample(ao_tex, ao_sampler, uv).r, 0.0, 0.0, 1.0);
    }

    let center_normal = normalize(textureSample(gbuffer_normal, gbuffer_sampler, uv).xyz * 2.0 - 1.0);
    let center_depth = dot(center_pos.xyz - params.camera_pos, params.camera_forward);
    let texel_x = params.texel_size.x;
    let texel_y = params.texel_size.y;

    var blurred_ao: f32 = 0.0;
    var total_weight: f32 = 0.0;

    for (var y: i32 = -KERNEL_SIZE; y <= KERNEL_SIZE; y = y + 1) {
        for (var x: i32 = -KERNEL_SIZE; x <= KERNEL_SIZE; x = x + 1) {
            let idx = (y + KERNEL_SIZE) * 3 + (x + KERNEL_SIZE);
            let gaussian_w = KERNEL_WEIGHTS[idx];

            let sample_uv = uv + vec2<f32>(f32(x) * texel_x, f32(y) * texel_y);
            let sample_ao = textureSample(ao_tex, ao_sampler, sample_uv).r;

            // Bilateral weight: reduce contribution across depth edges.
            let sample_pos = textureSample(gbuffer_position, gbuffer_sampler, sample_uv);
            if (sample_pos.a < 0.5) {
                continue;
            }
            let sample_normal = normalize(textureSample(gbuffer_normal, gbuffer_sampler, sample_uv).xyz * 2.0 - 1.0);
            let sample_depth = dot(sample_pos.xyz - params.camera_pos, params.camera_forward);
            let depth_diff = abs(center_depth - sample_depth);
            let bilateral_w = exp(-depth_diff / params.depth_sigma);
            let normal_w = pow(max(dot(center_normal, sample_normal), 0.0), 8.0);

            let w = gaussian_w * bilateral_w * normal_w;
            blurred_ao = blurred_ao + sample_ao * w;
            total_weight = total_weight + w;
        }
    }

    let result = blurred_ao / max(total_weight, 0.0001);
    return vec4<f32>(result, 0.0, 0.0, 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::AO_BLUR_SHADER_SRC;

    #[test]
    fn ao_blur_shader_parses_without_a_gpu() {
        naga::front::wgsl::parse_str(AO_BLUR_SHADER_SRC)
            .expect("AO blur shader should remain valid WGSL");
    }
}
