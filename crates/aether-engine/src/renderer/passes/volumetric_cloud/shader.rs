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
@group(1) @binding(1) var noise_tex: texture_3d<f32>;
@group(1) @binding(2) var noise_sampler: sampler;

@group(2) @binding(0) var worley_tex: texture_3d<f32>;
@group(2) @binding(1) var perlin_worley_tex: texture_3d<f32>;
@group(2) @binding(2) var curl_tex: texture_3d<f32>;
@group(2) @binding(3) var weather_tex: texture_2d<f32>;
@group(2) @binding(4) var multi_noise_sampler: sampler;

const PI: f32 = 3.14159265359;

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
    let coverage = bounds.z;
    let density_scale = bounds.w;

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

    // Stop marching at the reconstructed geometry depth.
    let geo_dist = length(world_pos - clouds.camera_pos.xyz);
    t_exit = min(t_exit, geo_dist);
    if (t_enter >= t_exit) {
        return vec4<f32>(0.0);
    }

    let steps = 32.0;
    let dt = (t_exit - t_enter) / steps;
    let sun_dir = normalize(clouds.sun_direction.xyz);
    let wind = clouds.wind_time.xyz * clouds.wind_time.w;

    var transmittance = 1.0;
    var light_energy = 0.0;

    for (var i = 0.0; i < steps; i += 1.0) {
        let t = t_enter + (i + 0.5) * dt;
        let pos = clouds.camera_pos.xyz + ray_dir * t;
        let noise_uvw = pos * 0.005 + wind * 0.01;
        let n = textureSample(noise_tex, noise_sampler, noise_uvw).r;
        let density = max(n - (1.0 - coverage), 0.0) * density_scale;

        if (density > 0.0) {
            let extinction = density * 0.15;
            let sample_trans = exp(-extinction * dt);
            transmittance *= sample_trans;

            let light = max(dot(ray_dir, sun_dir), 0.0) * 0.5 + 0.5;
            light_energy += density * dt * transmittance * light;
        }
    }

    let alpha = 1.0 - transmittance;
    let cloud_color = vec3<f32>(1.0, 0.98, 0.95);
    return vec4<f32>(light_energy * cloud_color, alpha);
}
"#;
