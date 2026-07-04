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
@group(2) @binding(0) var worley_tex: texture_3d<f32>;
@group(2) @binding(1) var perlin_worley_tex: texture_3d<f32>;
@group(2) @binding(2) var curl_tex: texture_3d<f32>;
@group(2) @binding(3) var weather_tex: texture_2d<f32>;
@group(2) @binding(4) var noise_sampler: sampler;

const PI: f32 = 3.14159265359;
const SUN_COLOR: vec3<f32> = vec3<f32>(1.0, 0.98, 0.95);
const SKY_AMBIENT: vec3<f32> = vec3<f32>(0.25, 0.35, 0.55);
const SUN_INTENSITY: f32 = 2.0;

fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let gg = g * g;
    return (1.0 - gg) / (4.0 * PI * pow(1.0 + gg - 2.0 * g * cos_theta, 1.5));
}

fn beer_transmittance(optical_depth: f32) -> f32 {
    return exp(-optical_depth);
}

/// Deterministic pseudo-random hash in [0, 1) for a 2D coordinate.
fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

/// Sample the multi-noise density model at `pos`.
///
/// Returns a raw density value (before the powder remapping) so the same
/// function can be used for the primary march and the self-shadow march.
fn sample_density(
    pos: vec3<f32>,
    coverage: f32,
    density_scale: f32,
    wind: vec3<f32>,
    min_y: f32,
    max_y: f32,
    with_detail: bool,
) -> f32 {
    // --- Weather mask: large-scale coverage regions ---
    let weather_uv = pos.xz * 0.0003;
    let weather_val = textureSampleLevel(weather_tex, noise_sampler, weather_uv, 0.0).r;
    // Broad cloud systems with large clear gaps in between.
    let weather_coverage = smoothstep(0.1, 0.55, weather_val);
    let effective_coverage = clamp(coverage * weather_coverage, 0.0, 0.95);

    // --- Height shaping: flat base, fluffy top ---
    let height_norm = (pos.y - min_y) / (max_y - min_y);
    let height_factor = pow(height_norm, 0.45) * pow(max(1.0 - height_norm, 0.0), 0.35);
    let shaped_height = clamp(height_factor * 2.8, 0.0, 1.0);

    // --- Noise sampling in texture-uv space ---
    let base_freq = 0.05;
    let noise_uv = pos * base_freq + wind * 0.05;

    // Curl / domain warp: distort the regular noise grid to hide the cellular structure.
    let curl_uv = noise_uv * 0.25;
    let curl_sample = textureSampleLevel(curl_tex, noise_sampler, curl_uv, 0.0).rg;
    let warp = vec3<f32>(curl_sample.r, 0.0, curl_sample.g) * 0.2;
    let warped_uv = noise_uv + warp;

    // --- Base shape from Perlin-Worley (smoother than raw Worley) ---
    let base_v = textureSampleLevel(perlin_worley_tex, noise_sampler, warped_uv, 0.0).r;
    var base_density = max(base_v - (1.0 - effective_coverage), 0.0);
    base_density = pow(base_density, 1.15);

    // --- Worley detail erosion ---
    if (with_detail) {
        let detail_v = textureSampleLevel(worley_tex, noise_sampler, warped_uv * 2.5, 0.0).r;
        let erosion = 1.0 - detail_v * 0.28;
        base_density *= erosion;
    }

    return max(base_density * shaped_height * density_scale, 0.0);
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

    let t1 = (min_y - clouds.camera_pos.y) / ray_dir.y;
    let t2 = (max_y - clouds.camera_pos.y) / ray_dir.y;
    let t_min = min(t1, t2);
    let t_max = max(t1, t2);
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

    // Subtle horizon fade: suppress distant cloud accumulation near the visual horizon.
    let horizon_fade = smoothstep(0.0, 0.05, abs(ray_dir.y));

    // Per-pixel ray offset to break aliasing / banding from regular sample placement.
    let ray_offset = hash2(uv * 437.0) - 0.5;

    var transmittance = 1.0;
    var light_energy = vec3<f32>(0.0);

    for (var i: f32 = 0.0; i < primary_steps; i += 1.0) {
        let t = t_enter + (i + 0.5 + ray_offset) * dt;
        let pos = clouds.camera_pos.xyz + ray_dir * t;

        let raw_density = sample_density(pos, coverage, density_scale, wind, min_y, max_y, true);
        if (raw_density <= 0.001) {
            continue;
        }

        // Powder effect: non-linear remapping that darkens thin edges and brightens dense cores.
        let density = (1.0 - exp(-raw_density * 1.2)) * horizon_fade;

        // --- Self-shadowing: secondary march toward sun ---
        let shadow_dt = 15.0 / shadow_steps;
        var shadow_od: f32 = 0.0;
        for (var s: f32 = 0.0; s < shadow_steps; s += 1.0) {
            let sp = pos + sun_dir * ((s + 0.5) * shadow_dt);
            shadow_od += sample_density(sp, coverage, density_scale, wind, min_y, max_y, false) * shadow_dt;
        }
        let sun_trans = beer_transmittance(shadow_od * 0.55);

        // --- Extinction ---
        let absorption = density * 0.45;
        transmittance *= exp(-absorption * dt);

        // --- Phase function (double-lobe HG + isotropic floor) ---
        let cos_theta = dot(ray_dir, sun_dir);
        let phase_forward = henyey_greenstein(cos_theta, g_forward);
        let phase_back = henyey_greenstein(cos_theta, -g_back);
        let phase = phase_forward * 0.5 + phase_back * 0.2 + 0.3;

        // --- Cloud color: per-sample height gradient ---
        let height_norm = (pos.y - min_y) / (max_y - min_y);
        let cloud_color = mix(clouds.cloud_color_low.rgb, clouds.cloud_color_high.rgb, height_norm);

        // --- Lighting: direct sun + blue sky ambient in shadowed parts ---
        let direct_light = SUN_COLOR * SUN_INTENSITY * sun_trans * phase * cloud_color;
        let ambient_light = SKY_AMBIENT * (1.0 - sun_trans * 0.65);
        let light = direct_light + ambient_light * 0.5;

        light_energy += density * dt * light * transmittance;
    }

    let alpha = (1.0 - transmittance) * horizon_fade;
    return vec4<f32>(light_energy, alpha);
}
"#;
