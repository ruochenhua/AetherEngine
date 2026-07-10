//! Volumetric cloud raymarch shader.
//!
//! One-to-one port of NadirRoGue/RenderEngine's
//! shaders/clouds/volumetricclouds.frag from GLSL to WGSL.
//!
//! The cloud layer is a spherical shell centered below the camera. Noise is
//! sampled with spherical UV coordinates plus a height fraction, exactly as
//! RenderEngine does. Pixels without cloud intersection remain transparent so
//! the atmosphere pass provides the sky background.

struct CloudUniform {
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_direction: vec4<f32>,
    sphere_center_inner: vec4<f32>, // xyz = sphere center, w = inner radius
    sphere_outer_params: vec4<f32>, // x = outer radius, y = max render dist, z = cloud top offset, w = 0
    wind_time: vec4<f32>,           // xyz = wind direction, w = time * wind speed
    noise_scales: vec4<f32>,        // x = weather scale, y = base noise scale, z = high freq noise scale, w = high freq UV scale
    detail_params: vec4<f32>,       // x = high freq H scale, y = cloud type, z = coverage multiplier, w = 0
    light_color: vec4<f32>,         // rgb = real light color, a = light factor
    horizon_color: vec4<f32>,
    zenit_color: vec4<f32>,
    cloud_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> clouds: CloudUniform;

@group(1) @binding(0) var depth_tex: texture_depth_2d;

@group(2) @binding(0) var perlinworley_tex: texture_3d<f32>;
@group(2) @binding(1) var worley_tex: texture_3d<f32>;
@group(2) @binding(2) var weather_tex: texture_2d<f32>;
@group(2) @binding(3) var noise_sampler: sampler;

// -----------------------------------------------------------------------------
// Vertex
// -----------------------------------------------------------------------------

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

const STRATUS_GRADIENT: vec4<f32> = vec4<f32>(0.0, 0.1, 0.2, 0.3);
const STRATOCUMULUS_GRADIENT: vec4<f32> = vec4<f32>(0.02, 0.2, 0.48, 0.625);
const CUMULUS_GRADIENT: vec4<f32> = vec4<f32>(0.00, 0.1625, 0.88, 0.98);

const CONE_STEP: f32 = 0.1666666;

const NOISE_KERNEL: array<vec3<f32>, 6> = array<vec3<f32>, 6>(
    vec3<f32>( 0.38051305,  0.92453449, -0.02111345),
    vec3<f32>(-0.50625799, -0.03590792, -0.86163418),
    vec3<f32>(-0.32509218, -0.94557439,  0.01428793),
    vec3<f32>( 0.09026238, -0.27376545,  0.95755165),
    vec3<f32>( 0.28128598,  0.42443639, -0.86065785),
    vec3<f32>(-0.16852403,  0.14748697,  0.97460106)
);

// -----------------------------------------------------------------------------
// Utilities
// -----------------------------------------------------------------------------

fn remap_value(original: f32, o_min: f32, o_max: f32, n_min: f32, n_max: f32) -> f32 {
    return n_min + ((original - o_min) / (o_max - o_min)) * (n_max - n_min);
}

fn sphere_center() -> vec3<f32> {
    return clouds.sphere_center_inner.xyz;
}

fn inner_radius() -> f32 {
    return clouds.sphere_center_inner.w;
}

fn outer_radius() -> f32 {
    return clouds.sphere_outer_params.x;
}

fn max_render_dist() -> f32 {
    return clouds.sphere_outer_params.y;
}

fn cloud_top_offset() -> f32 {
    return clouds.sphere_outer_params.z;
}

fn get_height_fraction(p: vec3<f32>) -> f32 {
    return (length(p - sphere_center()) - inner_radius()) / (outer_radius() - inner_radius());
}

fn spherical_uv_proj(p: vec3<f32>) -> vec2<f32> {
    let dir = normalize(p - sphere_center());
    return (dir.xz + 1.0) / 2.0;
}

fn get_weather_data(p: vec3<f32>) -> vec3<f32> {
    let uv = spherical_uv_proj(p) * clouds.noise_scales.x;
    return textureSample(weather_tex, noise_sampler, uv).rgb;
}

fn get_density_for_cloud(height_fraction: f32, cloud_type: f32) -> f32 {
    let stratus_factor = 1.0 - clamp(cloud_type * 2.0, 0.0, 1.0);
    let strato_factor = 1.0 - abs(cloud_type - 0.5) * 2.0;
    let cumulus_factor = clamp(cloud_type - 0.5, 0.0, 1.0) * 2.0;

    let base_gradient =
        stratus_factor * STRATUS_GRADIENT +
        strato_factor * STRATOCUMULUS_GRADIENT +
        cumulus_factor * CUMULUS_GRADIENT;

    let result =
        remap_value(height_fraction, base_gradient.x, base_gradient.y, 0.0, 1.0) *
        remap_value(height_fraction, base_gradient.z, base_gradient.w, 1.0, 0.0);
    return result;
}

// -----------------------------------------------------------------------------
// Density sampling
// -----------------------------------------------------------------------------

fn sample_cloud_density(p: vec3<f32>, lod: f32, expensive: bool, height_fraction: f32) -> f32 {
    var pos = p;
    pos += height_fraction * clouds.wind_time.xyz * cloud_top_offset();
    pos += clouds.wind_time.xyz * clouds.wind_time.w;

    let weather_data = get_weather_data(pos);
    let uv = spherical_uv_proj(pos);

    let base_scale = clouds.noise_scales.y;
    let base_cloud_noise = textureSampleLevel(
        perlinworley_tex,
        noise_sampler,
        vec3<f32>(uv * base_scale, height_fraction),
        lod,
    );

    let low_freq_fbm = base_cloud_noise.g * 0.625 + base_cloud_noise.b * 0.25 + base_cloud_noise.a * 0.125;
    var base_cloud_shape = remap_value(base_cloud_noise.r, -(1.0 - low_freq_fbm), 1.0, 0.0, 1.0);

    let density_gradient = get_density_for_cloud(height_fraction, weather_data.g);
    base_cloud_shape *= density_gradient;

    let coverage = clamp(weather_data.r, 0.0, 1.0) * clouds.detail_params.z;
    var coveraged_cloud = remap_value(base_cloud_shape, coverage, 1.0, 0.0, 1.0);
    coveraged_cloud *= coverage;
    coveraged_cloud *= mix(1.0, 0.0, clamp(height_fraction / coverage, 0.0, 1.0));

    var final_cloud = coveraged_cloud;

    if (expensive) {
        let detail_scale = clouds.noise_scales.z;
        let uv_scale = clouds.noise_scales.w;
        let h_scale = clouds.detail_params.x;
        let erode_noise = textureSampleLevel(
            worley_tex,
            noise_sampler,
            vec3<f32>(uv * uv_scale, height_fraction * h_scale) * detail_scale,
            lod,
        ).rgb;

        let high_freq_fbm = erode_noise.r * 0.625 + erode_noise.g * 0.25 + erode_noise.b * 0.125;

        let hf_height_fraction = get_height_fraction(pos);
        let high_freq_noise_modifier = mix(high_freq_fbm, 1.0 - high_freq_fbm, clamp(hf_height_fraction * 8.5, 0.0, 1.0));
        final_cloud = remap_value(coveraged_cloud, high_freq_noise_modifier * 0.8, 1.0, 0.0, 1.0);
    }

    return clamp(final_cloud, 0.0, 1.0);
}

// -----------------------------------------------------------------------------
// Lighting
// -----------------------------------------------------------------------------

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
    return 20.0 * beer(cone_density) * powder(cone_density, ca) * henyey_greenstein(l, v, 0.2, ca);
}

fn ambient_light() -> vec3<f32> {
    let light_color = clouds.light_color.rgb;
    let ambient_color = mix(clouds.horizon_color.rgb, clouds.zenit_color.rgb, 0.15);
    return mix(light_color, ambient_color, 0.65) * clouds.light_color.a * 0.65;
}

fn raymarch_to_light(pos: vec3<f32>, view_dir: vec3<f32>, step_size: f32) -> f32 {
    let light_dir = normalize(clouds.sun_direction.xyz);
    var start_pos = pos;
    let ray_step = light_dir * step_size * 0.7;
    var cone_radius = 1.0;
    let inv_depth = 1.0 / (step_size * 6.0);
    var density = 0.0;
    var cone_density = 0.0;

    for (var i: i32 = 0; i < 6; i = i + 1) {
        let pos_in_cone = start_pos + light_dir + cone_radius * NOISE_KERNEL[i] * f32(i);

        let height_fraction = get_height_fraction(pos_in_cone);
        if (height_fraction <= 1.0) {
            let cloud_density = sample_cloud_density(pos_in_cone, f32(i + 1), cone_density < 0.3, height_fraction);
            if (cloud_density > 0.0) {
                density += cloud_density;
                let transmittance = 1.0 - (density * inv_depth);
                cone_density += cloud_density * transmittance;
            }
        }

        start_pos += ray_step;
        cone_radius += CONE_STEP;
    }

    var far_pos = pos + ray_step * 8.0;
    let far_height_fraction = get_height_fraction(far_pos);
    let far_density = sample_cloud_density(far_pos, 6.0, false, far_height_fraction);
    if (far_density > 0.0) {
        density += far_density;
        let transmittance = 1.0 - (density * inv_depth);
        cone_density += far_density * transmittance;
    }

    let ca = dot(light_dir, view_dir);
    return light_energy(light_dir, view_dir, ca, cone_density);
}

// -----------------------------------------------------------------------------
// Front-to-back raymarch
// -----------------------------------------------------------------------------

fn front_to_back_raymarch(start_pos: vec3<f32>, end_pos: vec3<f32>) -> vec4<f32> {
    let path = end_pos - start_pos;
    let shell_thickness = outer_radius() - inner_radius();
    let sample_count = i32(ceil(mix(48.0, 96.0, clamp(length(path) / shell_thickness, 0.0, 1.0))));

    let step_vector = path / f32(sample_count - 1);
    let step_size = length(step_vector);
    let view_dir = normalize(path);

    var pos = start_pos;
    var result = vec4<f32>(0.0);

    let lod_alpha = clamp(length(start_pos - clouds.camera_pos.xyz) / max_render_dist(), 0.0, 1.0);
    let sampling_lod = mix(0.0, 6.0, lod_alpha);

    let a = i32(frag_coord.x) % 4;
    let b = i32(frag_coord.y) % 4;
    let bayer_filter = array<f32, 16>(
        0.0, 8.0, 2.0, 10.0,
        12.0, 4.0, 14.0, 6.0,
        3.0, 11.0, 1.0, 9.0,
        15.0, 7.0, 13.0, 5.0
    );
    pos += step_vector * (bayer_filter[a * 4 + b] / 16.0);

    let light_factor = clouds.light_color.a;
    let lc = clouds.light_color.rgb * light_factor * clouds.cloud_color.rgb;
    let ambient_l = ambient_light();

    for (var i: i32 = 0; i < sample_count; i = i + 1) {
        let height_fraction = get_height_fraction(pos);
        if (height_fraction < 0.0 || height_fraction > 1.0) {
            break;
        }

        let cloud_density = sample_cloud_density(pos, sampling_lod, true, height_fraction);
        if (cloud_density > 0.0) {
            let le = raymarch_to_light(pos, view_dir, step_size);
            // Lower absorption so clouds stay translucent and ground shadows are softer.
            let alpha = cloud_density * 0.75;
            var src = vec4<f32>(lc * le + ambient_l, alpha);
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

// -----------------------------------------------------------------------------
// Sphere intersection
// -----------------------------------------------------------------------------

fn intersect_sphere(
    o: vec3<f32>,
    d: vec3<f32>,
    min_t: ptr<function, vec3<f32>>,
    max_t: ptr<function, vec3<f32>>,
) -> bool {
    let sphere_to_origin = o - sphere_center();
    let b = dot(d, sphere_to_origin);
    let c = dot(sphere_to_origin, sphere_to_origin);

    let inner_r = inner_radius();
    let sqrt_op_inner = b * b - (c - inner_r * inner_r);
    if (sqrt_op_inner < 0.0) {
        return false;
    }

    let de_inner = sqrt(sqrt_op_inner);
    let sol_a_inner = -b - de_inner;
    let sol_b_inner = -b + de_inner;
    var max_s_inner = max(sol_a_inner, sol_b_inner);
    if (max_s_inner < 0.0) {
        return false;
    }

    let outer_r = outer_radius();
    let sqrt_op_outer = b * b - (c - outer_r * outer_r);
    if (sqrt_op_outer < 0.0) {
        return false;
    }

    let de_outer = sqrt(sqrt_op_outer);
    let sol_a_outer = -b - de_outer;
    let sol_b_outer = -b + de_outer;
    let max_s_outer = max(sol_a_outer, sol_b_outer);
    if (max_s_outer < 0.0) {
        return false;
    }

    let min_sol = min(max_s_inner, max_s_outer);
    if (min_sol > max_render_dist()) {
        return false;
    }

    let max_sol = max(max_s_inner, max_s_outer);
    *min_t = o + d * min_sol;
    *max_t = o + d * max_sol;
    return true;
}

// -----------------------------------------------------------------------------
// Fragment
// -----------------------------------------------------------------------------

var<private> frag_coord: vec4<f32>;

@fragment
fn fs_main(@builtin(position) in_frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    frag_coord = in_frag_coord;
    let dims = vec2<f32>(textureDimensions(depth_tex, 0));
    let uv = frag_coord.xy / dims;
    let coord = vec2<i32>(frag_coord.xy);

    let depth = textureLoad(depth_tex, coord, 0);
    if (depth < 1.0) {
        return vec4<f32>(0.0);
    }

    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world_h = clouds.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;
    let ray_dir = normalize(world_pos - clouds.camera_pos.xyz);

    var start_pos: vec3<f32>;
    var end_pos: vec3<f32>;
    let intersect = intersect_sphere(clouds.camera_pos.xyz, ray_dir, &start_pos, &end_pos);

    if (!intersect) {
        return vec4<f32>(0.0);
    }

    let geo_dist = length(world_pos - clouds.camera_pos.xyz);
    let t_start = length(start_pos - clouds.camera_pos.xyz);
    let t_end = min(length(end_pos - clouds.camera_pos.xyz), geo_dist);

    if (t_start >= t_end) {
        return vec4<f32>(0.0);
    }

    start_pos = clouds.camera_pos.xyz + ray_dir * t_start;
    end_pos = clouds.camera_pos.xyz + ray_dir * t_end;

    return front_to_back_raymarch(start_pos, end_pos);
}
