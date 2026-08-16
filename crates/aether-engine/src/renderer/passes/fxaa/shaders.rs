// WGSL shader for FXAA.

pub(super) const FXAA_SHADER_SRC: &str = r#"
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

struct FXAAUniforms {
    edge_threshold: f32,
    edge_threshold_min: f32,
    subpixel_quality: f32,
    enabled: u32,
};

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: FXAAUniforms;

fn luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
}

fn fxaa(uv: vec2<f32>) -> vec3<f32> {
    let texel_size = 1.0 / vec2<f32>(textureDimensions(input_texture));
    let uv_n = uv + vec2<f32>(0.0, texel_size.y);
    let uv_s = uv - vec2<f32>(0.0, texel_size.y);
    let uv_e = uv + vec2<f32>(texel_size.x, 0.0);
    let uv_w = uv - vec2<f32>(texel_size.x, 0.0);

    let rgb_c = textureSample(input_texture, tex_sampler, uv).rgb;
    let rgb_n = textureSample(input_texture, tex_sampler, uv_n).rgb;
    let rgb_s = textureSample(input_texture, tex_sampler, uv_s).rgb;
    let rgb_e = textureSample(input_texture, tex_sampler, uv_e).rgb;
    let rgb_w = textureSample(input_texture, tex_sampler, uv_w).rgb;

    let luma_c = luma(rgb_c);
    let luma_n = luma(rgb_n);
    let luma_s = luma(rgb_s);
    let luma_e = luma(rgb_e);
    let luma_w = luma(rgb_w);

    let luma_min = min(luma_c, min(min(luma_n, luma_s), min(luma_e, luma_w)));
    let luma_max = max(luma_c, max(max(luma_n, luma_s), max(luma_e, luma_w)));

    let luma_range = luma_max - luma_min;

    if (luma_range < max(uniforms.edge_threshold_min, luma_max * uniforms.edge_threshold)) {
        return rgb_c;
    }

    let luma_nw = luma(textureSample(input_texture, tex_sampler, uv + vec2<f32>(-texel_size.x, texel_size.y)).rgb);
    let luma_ne = luma(textureSample(input_texture, tex_sampler, uv + vec2<f32>( texel_size.x, texel_size.y)).rgb);
    let luma_sw = luma(textureSample(input_texture, tex_sampler, uv + vec2<f32>(-texel_size.x, -texel_size.y)).rgb);
    let luma_se = luma(textureSample(input_texture, tex_sampler, uv + vec2<f32>( texel_size.x, -texel_size.y)).rgb);

    let luma_ns = luma_n + luma_s;
    let luma_ew = luma_e + luma_w;
    let luma_nesw = luma_ne + luma_sw;
    let luma_nwse = luma_nw + luma_se;

    let edge_horizontal = abs(-2.0 * luma_n + luma_nesw)
                        + abs(-2.0 * luma_c + luma_nwse) * 2.0
                        + abs(-2.0 * luma_s + luma_nesw);
    let edge_vertical   = abs(-2.0 * luma_e + luma_nwse)
                        + abs(-2.0 * luma_c + luma_nesw) * 2.0
                        + abs(-2.0 * luma_w + luma_nwse);

    let is_horizontal = edge_horizontal >= edge_vertical;

    let luma1 = select(luma_e, luma_n, is_horizontal);
    let luma2 = select(luma_w, luma_s, is_horizontal);
    let gradient1 = luma1 - luma_c;
    let gradient2 = luma2 - luma_c;

    let is_1_steepest = abs(gradient1) >= abs(gradient2);
    let gradient_scaled = 0.25 * max(abs(gradient1), abs(gradient2));

    let step_length = select(texel_size.x, texel_size.y, is_horizontal);
    var uv_offset = vec2<f32>(0.0);
    if (is_horizontal) {
        uv_offset.y = step_length * 0.5;
    } else {
        uv_offset.x = step_length * 0.5;
    }

    var luma_local_average = 0.0;
    if (is_1_steepest) {
        uv_offset = -uv_offset;
        luma_local_average = 0.5 * (luma1 + luma_c);
    } else {
        luma_local_average = 0.5 * (luma2 + luma_c);
    }

    var uv_p = uv + uv_offset;
    var uv_m = uv - uv_offset;

    var luma_end_p = 0.0;
    var luma_end_m = 0.0;
    var done_p = false;
    var done_m = false;

    // Search along edge direction (simplified: 4 steps)
    for (var i: i32 = 0; i < 4; i = i + 1) {
        if (!done_p) {
            luma_end_p = luma(textureSample(input_texture, tex_sampler, uv_p).rgb);
            done_p = abs(luma_end_p - luma_local_average) >= gradient_scaled;
            if (!done_p) {
                uv_p += uv_offset;
            }
        }
        if (!done_m) {
            luma_end_m = luma(textureSample(input_texture, tex_sampler, uv_m).rgb);
            done_m = abs(luma_end_m - luma_local_average) >= gradient_scaled;
            if (!done_m) {
                uv_m -= uv_offset;
            }
        }
    }

    let distance_p = select(abs(uv_p.x - uv.x), abs(uv_p.y - uv.y), is_horizontal);
    let distance_m = select(abs(uv_m.x - uv.x), abs(uv_m.y - uv.y), is_horizontal);

    let edge_thickness = distance_p + distance_m;
    // Standard FXAA 3.11 offset: positive → shift toward positive, negative → toward negative
    let pixel_offset = 0.5 * (distance_m - distance_p) / edge_thickness;

    let luma_average = (1.0 / 12.0) * (2.0 * (luma_n + luma_s + luma_e + luma_w) + luma_nw + luma_ne + luma_sw + luma_se);
    let subpixel_offset = abs(luma_average - luma_c) / luma_range;
    let subpixel_offset_clamped = max(-2.0 * subpixel_offset + 1.0, 0.0);
    let subpixel_offset_final = subpixel_offset_clamped * subpixel_offset_clamped * uniforms.subpixel_quality;

    // Pick the larger magnitude; preserve pixel_offset sign for subpixel case
    let final_offset = select(
        sign(pixel_offset) * subpixel_offset_final,
        pixel_offset,
        abs(pixel_offset) >= subpixel_offset_final,
    );

    var uv_final = uv;
    if (is_horizontal) {
        uv_final.y += final_offset * step_length;
    } else {
        uv_final.x += final_offset * step_length;
    }

    return textureSample(input_texture, tex_sampler, uv_final).rgb;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (uniforms.enabled == 0u) {
        return vec4<f32>(textureSample(input_texture, tex_sampler, in.uv).rgb, 1.0);
    }
    return vec4<f32>(fxaa(in.uv), 1.0);
}
"#;
