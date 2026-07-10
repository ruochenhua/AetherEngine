//! Volumetric cloud raymarch shader.
//!
//! Phase 3 reconstruction: flat cloud slab with RenderEngine-style density
//! sampling and lighting. The noise is sampled in world space so the clouds
//! keep their natural 3D thickness as the camera moves through the layer.

struct CloudUniform {
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_direction: vec4<f32>, // xyz = direction toward sun, w = light intensity
    cloud_bounds: vec4<f32>, // x = bottom altitude, y = top altitude, z = coverage, w = density scale
    wind_time: vec4<f32>, // xyz = wind direction, w = wind speed * accumulated time
    render_params: vec4<f32>, // x = max_render_dist, y = weather_scale, z = base_noise_scale, w = high_freq_noise_scale
    detail_params: vec4<f32>, // x = high_freq_uv_scale, y = high_freq_h_scale, z = cloud_type, w = cloud_top_offset
    cloud_color_low: vec4<f32>,
    cloud_color_high: vec4<f32>,
    light_color: vec4<f32>, // rgb = light color, a = light intensity factor
};

@group(0) @binding(0) var<uniform> clouds: CloudUniform;

@group(1) @binding(0) var depth_tex: texture_depth_2d;

@group(2) @binding(0) var perlinworley_tex: texture_3d<f32>;
@group(2) @binding(1) var worley_tex: texture_3d<f32>;
@group(2) @binding(2) var weather_tex: texture_2d<f32>;
@group(2) @binding(3) var noise_sampler: sampler;

// ---------------------------------------------------------------------------
// Vertex
// ---------------------------------------------------------------------------

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn remap_value(original: f32, o_min: f32, o_max: f32, n_min: f32, n_max: f32) -> f32 {
    return n_min + ((original - o_min) / (o_max - o_min)) * (n_max - n_min);
}

fn get_height_fraction(p: vec3<f32>) -> f32 {
    return (p.y - clouds.cloud_bounds.x) / (clouds.cloud_bounds.y - clouds.cloud_bounds.x);
}

fn get_weather_data(p: vec3<f32>) -> vec3<f32> {
    let uv = p.xz * clouds.render_params.y;
    return textureSample(weather_tex, noise_sampler, uv).rgb;
}

// ---------------------------------------------------------------------------
// Cloud type gradients (from RenderEngine)
// ---------------------------------------------------------------------------

const STRATUS_GRADIENT: vec4<f32> = vec4<f32>(0.0, 0.1, 0.2, 0.3);
const STRATOCUMULUS_GRADIENT: vec4<f32> = vec4<f32>(0.02, 0.2, 0.48, 0.625);
const CUMULUS_GRADIENT: vec4<f32> = vec4<f32>(0.00, 0.1625, 0.88, 0.98);

fn get_density_for_cloud(height_fraction: f32, cloud_type: f32) -> f32 {
    let stratus_factor = 1.0 - clamp(cloud_type * 2.0, 0.0, 1.0);
    let strato_factor = 1.0 - abs(cloud_type - 0.5) * 2.0;
    let cumulus_factor = clamp(cloud_type - 0.5, 0.0, 1.0) * 2.0;

    let base_gradient =
        stratus_factor * STRATUS_GRADIENT +
        strato_factor * STRATOCUMULUS_GRADIENT +
        cumulus_factor * CUMULUS_GRADIENT;

    var result =
        remap_value(height_fraction, base_gradient.x, base_gradient.y, 0.0, 1.0) *
        remap_value(height_fraction, base_gradient.z, base_gradient.w, 1.0, 0.0);
    return result;
}

// ---------------------------------------------------------------------------
// Density sampling (RenderEngine's sampleCloudDensity, world-space variant)
// ---------------------------------------------------------------------------

fn sample_cloud_density(p: vec3<f32>, expensive: bool, height_fraction: f32) -> f32 {
    let wind_dir = clouds.wind_time.xyz;
    let wind_speed_time = clouds.wind_time.w;
    let cloud_top_offset = clouds.detail_params.w;

    var pos = p;
    pos += height_fraction * wind_dir * cloud_top_offset;
    pos += wind_dir * wind_speed_time;

    let weather_data = get_weather_data(pos);

    // Sample base shape noise in world space.
    let base_scale = clouds.render_params.z;
    let base_cloud_noise = textureSampleLevel(perlinworley_tex, noise_sampler, pos * base_scale, 0.0);

    let low_freq_fbm = base_cloud_noise.g * 0.625 + base_cloud_noise.b * 0.25 + base_cloud_noise.a * 0.125;
    var base_cloud_shape = remap_value(base_cloud_noise.r, -(1.0 - low_freq_fbm), 1.0, 0.0, 1.0);

    // Density gradient by cloud type
    let density_gradient = get_density_for_cloud(height_fraction, weather_data.g);
    base_cloud_shape *= density_gradient;

    // Coverage
    let coverage = clamp(weather_data.r, 0.0, 1.0) * clouds.cloud_bounds.z;
    var coveraged_cloud = remap_value(base_cloud_shape, coverage, 1.0, 0.0, 1.0);
    coveraged_cloud *= coverage;
    coveraged_cloud *= mix(1.0, 0.0, clamp(height_fraction / max(coverage, 0.0001), 0.0, 1.0));

    var final_cloud = coveraged_cloud;

    if (expensive) {
        // Detail erosion in world space.
        let detail_scale = clouds.render_params.w;
        let erode_noise = textureSampleLevel(worley_tex, noise_sampler, pos * detail_scale, 0.0).rgb;
        let high_freq_fbm = erode_noise.r * 0.625 + erode_noise.g * 0.25 + erode_noise.b * 0.125;

        let hf_modifier = mix(high_freq_fbm, 1.0 - high_freq_fbm, clamp(get_height_fraction(pos) * 8.5, 0.0, 1.0));
        final_cloud = remap_value(coveraged_cloud, hf_modifier * 0.8, 1.0, 0.0, 1.0);
    }

    return clamp(final_cloud * clouds.cloud_bounds.w, 0.0, 1.0);
}

// ---------------------------------------------------------------------------
// Lighting (RenderEngine)
// ---------------------------------------------------------------------------

const NOISE_KERNEL: array<vec3<f32>, 6> = array<vec3<f32>, 6>(
    vec3<f32>( 0.38051305,  0.92453449, -0.02111345),
    vec3<f32>(-0.50625799, -0.03590792, -0.86163418),
    vec3<f32>(-0.32509218, -0.94557439,  0.01428793),
    vec3<f32>( 0.09026238, -0.27376545,  0.95755165),
    vec3<f32>( 0.28128598,  0.42443639, -0.86065785),
    vec3<f32>(-0.16852403,  0.14748697,  0.97460106)
);

const CONE_STEP: f32 = 0.1666666;

fn henyey_greenstein(l: vec3<f32>, v: vec3<f32>, g: f32, ca: f32) -> f32 {
    let g2 = g * g;
    return ((1.0 - g2) / pow((1.0 + g2 - 2.0 * g * ca), 1.5)) * (1.0 / (4.0 * 3.1415));
}

fn beer(density: f32) -> f32 {
    return exp(-density);
}

fn powder(density: f32, ca: f32) -> f32 {
    let f = 1.0 - exp(-density * 2.0);
    return mix(1.0, f, clamp(-ca * 0.5 + 0.5, 0.0, 1.0));
}

fn light_energy(l: vec3<f32>, v: vec3<f32>, ca: f32, cone_density: f32) -> f32 {
    return 8.0 * beer(cone_density) * powder(cone_density, ca) * henyey_greenstein(l, v, 0.2, ca);
}

fn ambient_light(light_factor: f32) -> vec3<f32> {
    let zenit_color = vec3<f32>(0.05, 0.30, 0.65);
    let horizon_color = vec3<f32>(0.55, 0.62, 0.72);
    let ambient_color = mix(horizon_color, zenit_color, 0.15);
    return mix(clouds.light_color.rgb, ambient_color, 0.65) * light_factor * 0.65;
}

fn raymarch_to_light(pos: vec3<f32>, view_dir: vec3<f32>, step_size: f32) -> f32 {
    let light_dir = normalize(clouds.sun_direction.xyz);
    let slab_thickness = clouds.cloud_bounds.y - clouds.cloud_bounds.x;
    let ray_step = light_dir * max(step_size * 3.0, slab_thickness / 6.0);
    var cone_radius = 1.0;
    let inv_depth = 1.0 / (length(ray_step) * 6.0);
    var density = 0.0;
    var cone_density = 0.0;

    for (var i: i32 = 0; i < 6; i = i + 1) {
        let pos_in_cone = pos + ray_step * f32(i) + cone_radius * NOISE_KERNEL[i] * f32(i);
        let height_fraction = get_height_fraction(pos_in_cone);
        if (height_fraction >= 0.0 && height_fraction <= 1.0) {
            let cloud_density = sample_cloud_density(pos_in_cone, cone_density < 0.3, height_fraction);
            if (cloud_density > 0.0) {
                density += cloud_density;
                let transmittance = 1.0 - (density * inv_depth);
                cone_density += cloud_density * transmittance;
            }
        }
        cone_radius += CONE_STEP;
    }

    // Far sample for shadowing
    let far_pos = pos + ray_step * 8.0;
    let far_height = get_height_fraction(far_pos);
    let far_density = sample_cloud_density(far_pos, false, far_height);
    if (far_density > 0.0) {
        density += far_density;
        let transmittance = 1.0 - (density * inv_depth);
        cone_density += far_density * transmittance;
    }

    let ca = dot(light_dir, view_dir);
    return light_energy(light_dir, view_dir, ca, cone_density);
}

// ---------------------------------------------------------------------------
// Front-to-back raymarch (RenderEngine, flat slab variant)
// ---------------------------------------------------------------------------

fn front_to_back_raymarch(start_pos: vec3<f32>, end_pos: vec3<f32>) -> vec4<f32> {
    let path = end_pos - start_pos;
    let slab_thickness = clouds.cloud_bounds.y - clouds.cloud_bounds.x;
    let sample_count = i32(ceil(mix(48.0, 96.0, clamp(length(path) / slab_thickness, 0.0, 1.0))));

    let step_vector = path / f32(sample_count - 1);
    let step_size = length(step_vector);
    let view_dir = normalize(path);

    var pos = start_pos;
    var result = vec4<f32>(0.0);

    // Bayer-style dithering based on pixel coordinates to reduce banding.
    let a = i32(frag_coord.x) % 4;
    let b = i32(frag_coord.y) % 4;
    let bayer_filter = array<f32, 16>(
        0.0, 8.0, 2.0, 10.0,
        12.0, 4.0, 14.0, 6.0,
        3.0, 11.0, 1.0, 9.0,
        15.0, 7.0, 13.0, 5.0
    );
    pos += step_vector * (bayer_filter[a * 4 + b] / 16.0);

    let light_dir = normalize(clouds.sun_direction.xyz);
    let light_factor = clamp(dot(vec3<f32>(0.0, 1.0, 0.0), light_dir), 0.0, 1.0);
    let lc = clouds.light_color.rgb * light_factor * clouds.cloud_color_low.rgb;
    let ambient_l = ambient_light(light_factor);

    for (var i: i32 = 0; i < sample_count; i = i + 1) {
        let height_fraction = get_height_fraction(pos);
        if (height_fraction < 0.0 || height_fraction > 1.0) {
            break;
        }

        let cloud_density = sample_cloud_density(pos, true, height_fraction);
        if (cloud_density > 0.0) {
            let le = raymarch_to_light(pos, view_dir, step_size);
            var src = vec4<f32>(lc * le + ambient_l, cloud_density);
            src = vec4<f32>(src.rgb * src.a, src.a);
            result = (1.0 - result.a) * src + result;

            if (result.a >= 0.95) {
                break;
            }
        }

        pos += step_vector;
    }

    return result;
}

// ---------------------------------------------------------------------------
// Fragment
// ---------------------------------------------------------------------------

var<private> frag_coord: vec4<f32>;

@fragment
fn fs_main(@builtin(position) in_frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    frag_coord = in_frag_coord;
    let dims = vec2<f32>(textureDimensions(depth_tex, 0));
    let uv = frag_coord.xy / dims;
    let coord = vec2<i32>(frag_coord.xy);

    let depth = textureLoad(depth_tex, coord, 0);

    // Skip cloud rendering if the pixel is occluded by geometry.
    if (depth < 1.0) {
        return vec4<f32>(0.0);
    }

    // Reconstruct world-space ray direction
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world_h = clouds.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;
    let ray_dir = normalize(world_pos - clouds.camera_pos.xyz);

    let min_y = clouds.cloud_bounds.x;
    let max_y = clouds.cloud_bounds.y;

    // Slab entry/exit
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

    let geo_dist = length(world_pos - clouds.camera_pos.xyz);
    t_exit = min(t_exit, geo_dist);

    if (t_enter >= t_exit) {
        return vec4<f32>(0.0);
    }

    let start_pos = clouds.camera_pos.xyz + ray_dir * t_enter;
    let end_pos = clouds.camera_pos.xyz + ray_dir * t_exit;
    let cloud = front_to_back_raymarch(start_pos, end_pos);
    return cloud;
}
