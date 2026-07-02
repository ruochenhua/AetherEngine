//! WGSL shader source for the volumetric cloud pass.

/// Full-screen ray-marched cloud shader.
pub(crate) const SHADER: &str = r#"
struct CloudUniform {
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_direction: vec4<f32>,
    cloud_bounds: vec4<f32>,
    wind_time: vec4<f32>,
    quality_params: vec4<f32>,
    cloud_color_low: vec4<f32>,
    cloud_color_high: vec4<f32>,
};

@group(0) @binding(0) var<uniform> clouds: CloudUniform;
@group(1) @binding(0) var depth_tex: texture_depth_2d;
@group(1) @binding(1) var noise_tex: texture_3d<f32>; // legacy, unused in new path
@group(1) @binding(2) var depth_sampler: sampler;     // unused
@group(2) @binding(0) var worley_tex: texture_3d<f32>;
@group(2) @binding(1) var perlin_worley_tex: texture_3d<f32>;
@group(2) @binding(2) var curl_tex: texture_3d<f32>;
@group(2) @binding(3) var weather_tex: texture_2d<f32>;
@group(2) @binding(4) var noise_sampler: sampler;

const PI: f32 = 3.14159265359;

fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let gg = g * g;
    return (1.0 - gg) / (4.0 * PI * pow(1.0 + gg - 2.0 * g * cos_theta, 1.5));
}

fn beer_transmittance(optical_depth: f32) -> f32 {
    return exp(-optical_depth);
}

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(depth_tex, 0));
    let uv = frag_coord.xy / dims;
    let coord = vec2<i32>(frag_coord.xy);

    let depth = textureLoad(depth_tex, coord, 0);
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world_h = clouds.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;

    let ray_dir = normalize(world_pos - clouds.camera_pos.xyz);
    let bounds = clouds.cloud_bounds;
    let min_y = bounds.x;
    let max_y = bounds.y;

    // --- Slab entry/exit ---
    if (abs(ray_dir.y) < 0.0001) {
        return vec4<f32>(0.0);
    }

    let t_min = (min_y - clouds.camera_pos.y) / ray_dir.y;
    let t_max = (max_y - clouds.camera_pos.y) / ray_dir.y;
    if (t_max < 0.0) {
        return vec4<f32>(0.0);
    }

    var t_enter = max(t_min, 0.0);
    var t_exit = t_max;
    if (t_enter > t_exit) {
        return vec4<f32>(0.0);
    }

    let geo_dist = length(world_pos - clouds.camera_pos.xyz);
    t_exit = min(t_exit, geo_dist);
    if (t_enter >= t_exit) {
        return vec4<f32>(0.0);
    }

    // --- Quality params ---
    let primary_steps = clouds.quality_params.x;
    let shadow_steps = clouds.quality_params.y;
    let g_forward = clouds.quality_params.z;
    let g_back = clouds.quality_params.w;
    let coverage = bounds.z;
    let density_scale = bounds.w;

    let sun_dir = normalize(clouds.sun_direction.xyz);
    let wind = clouds.wind_time.xyz * clouds.wind_time.w;
    let dt = (t_exit - t_enter) / primary_steps;

    var transmittance = 1.0;
    var light_energy = 0.0;

    for (var i: f32 = 0.0; i < primary_steps; i += 1.0) {
        let t = t_enter + (i + 0.5) * dt;
        let pos = clouds.camera_pos.xyz + ray_dir * t;

        // Sample weather map for coverage
        let weather_uv = pos.xz * 0.002;
        let weather_val = textureSampleLevel(weather_tex, noise_sampler, weather_uv, 0.0).r;

        // Height-based blend (0 at bottom, 1 at top)
        let height_norm = (pos.y - min_y) / (max_y - min_y);

        // Multi-noise density sampling
        let noise_pos = pos * 0.008 + wind * 0.01;
        let curl_sample = textureSample(curl_tex, noise_sampler, noise_pos * 0.02).rg;
        let warp = vec3<f32>(curl_sample.r, 0.0, curl_sample.g);
        let warped_pos = noise_pos + warp * 4.0;

        let worley_v = textureSample(worley_tex, noise_sampler, warped_pos * 1.0).r;
        let detail_v = textureSample(perlin_worley_tex, noise_sampler, warped_pos * 3.0).r;

        // Density model: coverage threshold + detail
        let base_density = max(worley_v - (1.0 - coverage), 0.0);
        let detail_density = detail_v * 0.4;

        // Height shaping: stronger clouds at mid-altitude
        let height_factor = 1.0 - abs(height_norm - 0.5) * 2.0;
        let density = (base_density + detail_density) * height_factor * weather_val * density_scale;

        if (density > 0.001) {
            // --- Self-shadowing: secondary march toward sun ---
            let shadow_dt = 20.0 / shadow_steps;
            var shadow_od: f32 = 0.0;
            for (var s: f32 = 0.0; s < shadow_steps; s += 1.0) {
                let sp = pos + sun_dir * ((s + 0.5) * shadow_dt);
                let s_noise = sp * 0.008 + wind * 0.01;
                let s_worley = textureSampleLevel(worley_tex, noise_sampler, s_noise, 0.0).r;
                let s_detail = textureSampleLevel(perlin_worley_tex, noise_sampler, s_noise * 3.0, 0.0).r;
                let s_base_d = max(s_worley - (1.0 - coverage), 0.0);
                let s_d = (s_base_d + s_detail * 0.4) * weather_val * density_scale;
                shadow_od += s_d * shadow_dt;
            }
            let sun_trans = beer_transmittance(shadow_od * 0.3);

            // --- Extinction ---
            let absorption = density * 0.15;
            transmittance *= exp(-absorption * dt);

            // --- Phase function (double-lobe HG) ---
            let cos_theta = dot(ray_dir, sun_dir);
            let phase_forward = henyey_greenstein(cos_theta, g_forward);
            let phase_back = henyey_greenstein(cos_theta, -g_back);
            let phase = phase_forward * 0.7 + phase_back * 0.3;

            light_energy += density * dt * sun_trans * phase * 3.0 * transmittance;
        }
    }

    let alpha = 1.0 - transmittance;

    // Cloud color: blend between low-altitude warm and high-altitude cool
    let height_norm = (0.5 * (min_y + max_y) - min_y) / (max_y - min_y); // mid-slab
    let cloud_color = mix(clouds.cloud_color_low.rgb, clouds.cloud_color_high.rgb, height_norm);

    return vec4<f32>(light_energy * cloud_color, alpha);
}
"#;
