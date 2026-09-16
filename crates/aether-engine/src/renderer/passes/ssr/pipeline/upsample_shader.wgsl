
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

@group(0) @binding(1) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(3) var gbuffer_depth: texture_depth_2d;
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

fn normal_weight(normal_p: vec3<f32>, normal_q: vec3<f32>) -> f32 {
    let p_is_sky = normal_p.x == 0.0 && normal_p.y == 0.0 && normal_p.z == 0.0;
    let q_is_sky = normal_q.x == 0.0 && normal_q.y == 0.0 && normal_q.z == 0.0;
    if (p_is_sky || q_is_sky) {
        return 0.0;
    }
    let p = normalize(normal_p * 2.0 - 1.0);
    let q = normalize(normal_q * 2.0 - 1.0);
    return pow(max(dot(p, q), 0.0), 8.0);
}

fn bilateral_weight(
    depth_p: f32,
    depth_q: f32,
    normal_p: vec3<f32>,
    normal_q: vec3<f32>,
    center_dist: f32,
) -> f32 {
    let depth_diff = abs(depth_p - depth_q);
    let depth_w = exp(-depth_diff * 20.0);
    let spatial_w = exp(-center_dist * 2.0);
    let normal_w = normal_weight(normal_p, normal_q);
    return depth_w * spatial_w * normal_w;
}

@fragment
fn fs_main(in: VSOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let dims = vec2<i32>(settings.screen_size);
    let center_px = vec2<i32>(uv * settings.screen_size);
    let center_px_clamped = clamp(center_px, vec2<i32>(0), dims - vec2<i32>(1));
    let depth_center = textureLoad(gbuffer_depth, center_px_clamped, 0);
    let normal_center = textureLoad(gbuffer_normal, center_px_clamped, 0).xyz;

    // 3x3 bilateral upsample using textureLoad for depth (non-filterable)
    var total: vec4<f32> = vec4<f32>(0.0);
    var sum_weights: f32 = 0.0;

    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let tap_px = center_px + vec2<i32>(dx, dy);
            let tap_px_clamped = clamp(tap_px, vec2<i32>(0), dims - vec2<i32>(1));
            let tap_depth = textureLoad(gbuffer_depth, tap_px_clamped, 0);
            let tap_normal = textureLoad(gbuffer_normal, tap_px_clamped, 0).xyz;
            let offset_len = sqrt(f32(dx * dx + dy * dy));
            let w = bilateral_weight(
                depth_center,
                tap_depth,
                normal_center,
                tap_normal,
                offset_len,
            );
            // Trace texture at half resolution: map each full-res tap to the
            // center of its corresponding half-res texel. Sampling at the
            // full-res coordinate would land between trace texels and amplify
            // low-resolution block edges.
            let trace_px = floor(vec2<f32>(tap_px_clamped) * 0.5);
            let trace_size = settings.screen_size * 0.5;
            let trace_uv = (trace_px + vec2<f32>(0.5)) / trace_size;
            let trace_sample = textureSampleLevel(ssr_trace, tex_sampler, trace_uv, 0.0);
            total += trace_sample * w;
            sum_weights += w;
        }
    }

    let result = total / max(sum_weights, 0.001);
    return result;
}
