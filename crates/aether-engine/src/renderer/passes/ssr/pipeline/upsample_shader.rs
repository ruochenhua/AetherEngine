// SSR bilateral upsample WGSL shader.

pub(super) const SSR_UPSAMPLE_SHADER_SRC: &str = r#"
struct SSRSettings {
    camera_pos: vec3<f32>,
    _pad0: f32,
    view_proj: mat4x4<f32>,
    screen_size: vec2<f32>,
    _pad1: vec2<f32>,
    max_distance: f32,
    linear_steps: f32,
    thickness: f32,
    step_exponent: f32,
    jitter_amount: f32,
    min_roughness: f32,
    max_roughness: f32,
    edge_fade_start: f32,
    edge_fade_end: f32,
    ssr_debug_mode: u32,
    ssr_enabled: u32,
    frame_index: u32,
    _pad2: u32,
    _pad3: u32,
    _pad4: u32,
    _pad5: u32,
};

@group(0) @binding(3) var gbuffer_depth: texture_2d<f32>;
@group(0) @binding(4) var scene_color: texture_2d<f32>;
@group(0) @binding(5) var tex_sampler: sampler;
@group(0) @binding(6) var ssr_trace: texture_2d<f32>;

@group(1) @binding(0) var<uniform> settings: SSRSettings;

struct VSOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VSOutput {
    var out: VSOutput;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = vec2<f32>(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
    return out;
}

fn bilateral_weight(depth_p: f32, depth_q: f32, center_dist: f32) -> f32 {
    let depth_diff = abs(depth_p - depth_q);
    let depth_w = exp(-depth_diff * 20.0);
    let spatial_w = exp(-center_dist * 2.0);
    return depth_w * spatial_w;
}

@fragment
fn fs_main(in: VSOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let dims = vec2<i32>(settings.screen_size);
    let center_px = vec2<i32>(uv * settings.screen_size);
    let center_px_clamped = clamp(center_px, vec2<i32>(0), dims - vec2<i32>(1));
    let depth_center = textureLoad(gbuffer_depth, center_px_clamped, 0).r;

    // 3x3 bilateral upsample using textureLoad for depth (non-filterable)
    var total: vec4<f32> = vec4<f32>(0.0);
    var sum_weights: f32 = 0.0;

    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let tap_px = center_px + vec2<i32>(dx, dy);
            let tap_px_clamped = clamp(tap_px, vec2<i32>(0), dims - vec2<i32>(1));
            let tap_depth = textureLoad(gbuffer_depth, tap_px_clamped, 0).r;
            let offset_len = sqrt(f32(dx * dx + dy * dy));
            let w = bilateral_weight(depth_center, tap_depth, offset_len);
            // Trace texture at half resolution: convert full-res pixel to trace UV
            let trace_uv = vec2<f32>(tap_px_clamped) / settings.screen_size;
            let trace_sample = textureSampleLevel(ssr_trace, tex_sampler, trace_uv, 0.0);
            total += trace_sample * w;
            sum_weights += w;
        }
    }

    let result = total / max(sum_weights, 0.001);
    return result;
}
"#;
