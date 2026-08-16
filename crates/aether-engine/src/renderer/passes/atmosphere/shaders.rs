// WGSL shader for atmosphere rendering.

pub(super) const ATMOSPHERE_SHADER_SRC: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(position, 0.0, 1.0);
    out.uv = vec2<f32>(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5);
    return out;
}

struct AtmosphereUniform {
    sun_direction: vec3<f32>,
    _pad0: f32,
    camera_pos: vec3<f32>,
    _pad1: f32,
    planet_radius: f32,
    atmosphere_height: f32,
    rayleigh_scale_height: f32,
    mie_scale_height: f32,
    rayleigh_scattering: vec3<f32>,
    _pad2: f32,
    mie_scattering: vec3<f32>,
    _pad3: f32,
    sun_intensity: f32,
    mie_asymmetry: f32,
    _pad4: f32,
    _pad5: f32,
    ozone_absorption: vec3<f32>,
    _pad6: f32,
    ozone_scale_height: f32,
    _pad7: f32,
    _pad8: f32,
    _pad9: f32,
    multi_scattering_factor: f32,
    _pad10: f32,
    _pad11: f32,
    _pad12: f32,
    inv_view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> atmos: AtmosphereUniform;
@group(1) @binding(0) var gbuffer_depth: texture_depth_2d;

const PI: f32 = 3.14159265359;

fn ray_sphere_intersect(r0: vec3<f32>, rd: vec3<f32>, center: vec3<f32>, radius: f32) -> vec2<f32> {
    let oc = r0 - center;
    let b = dot(oc, rd);
    let c = dot(oc, oc) - radius * radius;
    let d = b * b - c;
    if (d < 0.0) {
        return vec2<f32>(1e10, -1e10);
    }
    let sd = sqrt(d);
    return vec2<f32>(-b - sd, -b + sd);
}

fn rayleigh_phase(cos_theta: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + cos_theta * cos_theta);
}

fn mie_phase(cos_theta: f32, g: f32) -> f32 {
    let gg = g * g;
    let num = (1.0 - gg) * (1.0 + cos_theta * cos_theta);
    let denom = (2.0 + gg) * pow(1.0 + gg - 2.0 * g * cos_theta, 1.5);
    return num / denom;
}

fn sun_transmittance(origin: vec3<f32>, sun_dir: vec3<f32>, planet_center: vec3<f32>, atmo_radius: f32, ozone_center: f32) -> vec3<f32> {
    let sun_atmo = ray_sphere_intersect(origin, sun_dir, planet_center, atmo_radius);
    let sun_step = sun_atmo.y;
    let sun_samples = 16.0;
    let sun_step_size = sun_step / sun_samples;
    var sun_sample = origin + sun_dir * (sun_step_size * 0.5);
    var sun_od_r: f32 = 0.0;
    var sun_od_m: f32 = 0.0;
    var sun_od_o: f32 = 0.0;
    for (var j: f32 = 0.0; j < sun_samples; j = j + 1.0) {
        let sun_h = length(sun_sample - planet_center) - atmos.planet_radius;
        sun_od_r += exp(-sun_h / atmos.rayleigh_scale_height) * sun_step_size;
        sun_od_m += exp(-sun_h / atmos.mie_scale_height) * sun_step_size;
        sun_od_o += max(0.0, 1.0 - abs(sun_h - ozone_center) / atmos.ozone_scale_height) * sun_step_size;
        sun_sample += sun_dir * sun_step_size;
    }
    return exp(-(
        atmos.rayleigh_scattering * sun_od_r +
        atmos.mie_scattering * sun_od_m +
        atmos.ozone_absorption * sun_od_o
    ));
}

fn atmosphere_color(ray_origin: vec3<f32>, ray_dir: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    let planet_center = vec3<f32>(0.0, -atmos.planet_radius, 0.0);
    let atmo_radius = atmos.planet_radius + atmos.atmosphere_height;
    let ozone_center = 25.0;

    let atmo_hit = ray_sphere_intersect(ray_origin, ray_dir, planet_center, atmo_radius);
    var t0 = max(atmo_hit.x, 0.0);
    var t1 = atmo_hit.y;
    if (t1 <= t0) {
        return vec3<f32>(0.0);
    }

    let planet_hit = ray_sphere_intersect(ray_origin, ray_dir, planet_center, atmos.planet_radius);
    if (planet_hit.x > 0.0) {
        t1 = min(t1, planet_hit.x);
    }

    let ray_length = t1 - t0;
    if (ray_length <= 0.0) {
        return vec3<f32>(0.0);
    }

    let sample_count = 32.0;
    let step_size = ray_length / sample_count;
    var sample_point = ray_origin + ray_dir * (t0 + step_size * 0.5);

    var optical_depth_r: f32 = 0.0;
    var optical_depth_m: f32 = 0.0;
    var optical_depth_o: f32 = 0.0;
    var total_r: vec3<f32> = vec3<f32>(0.0);
    var total_m: vec3<f32> = vec3<f32>(0.0);

    for (var i: f32 = 0.0; i < sample_count; i = i + 1.0) {
        let h = length(sample_point - planet_center) - atmos.planet_radius;
        let density_r = exp(-h / atmos.rayleigh_scale_height) * step_size;
        let density_m = exp(-h / atmos.mie_scale_height) * step_size;
        // Ozone tent distribution centered at 25 km (Hillaire 2020).
        let density_o = max(0.0, 1.0 - abs(h - ozone_center) / atmos.ozone_scale_height) * step_size;

        optical_depth_r += density_r;
        optical_depth_m += density_m;
        optical_depth_o += density_o;

        // Sun ray optical depth from sample point to top of atmosphere.
        let sun_trans = sun_transmittance(sample_point, sun_dir, planet_center, atmo_radius, ozone_center);

        let extinction = exp(-(
            atmos.rayleigh_scattering * optical_depth_r +
            atmos.mie_scattering * optical_depth_m +
            atmos.ozone_absorption * optical_depth_o
        )) * sun_trans;

        total_r += density_r * extinction;
        total_m += density_m * extinction;

        sample_point += ray_dir * step_size;
    }

    let mu = dot(ray_dir, sun_dir);
    let phase_r = rayleigh_phase(mu);
    let phase_m = mie_phase(mu, atmos.mie_asymmetry);

    let single = atmos.sun_intensity * (
        atmos.rayleigh_scattering * total_r * phase_r +
        atmos.mie_scattering * total_m * phase_m
    );

    // Approximate multiple scattering as an isotropic contribution scaled by
    // the configured factor. This lifts the dark-back-sky artifact of pure
    // single scattering without the cost of a full multi-scattering LUT.
    let ms_phase = 1.0 / (4.0 * PI);
    let multi = atmos.multi_scattering_factor * (
        atmos.rayleigh_scattering * total_r +
        atmos.mie_scattering * total_m
    ) * ms_phase;

    return single + atmos.sun_intensity * multi;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Discard geometry pixels so the existing SceneColor (from LightingPass) is preserved.
    // The GBuffer depth was cleared to 1.0 for sky and written to < 1.0 for geometry.
    let dims = vec2<f32>(textureDimensions(gbuffer_depth, 0));
    let coord = vec2<i32>(in.uv * dims);
    let depth = textureLoad(gbuffer_depth, coord, 0);
    if (depth < 0.9999) {
        discard;
    }

    let clip = vec4<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, 0.0, 1.0);
    let world_ray = atmos.inv_view_proj * clip;
    let world_pos = world_ray.xyz / world_ray.w;
    let ray_dir = normalize(world_pos - atmos.camera_pos);
    let sun_dir = normalize(atmos.sun_direction);

    var color = atmosphere_color(atmos.camera_pos, ray_dir, sun_dir);

    // Soft sun disc with atmospheric extinction. The extinction makes the
    // sun fade and redden as it approaches the horizon.
    let cos_sun = dot(ray_dir, sun_dir);
    let planet_center = vec3<f32>(0.0, -atmos.planet_radius, 0.0);
    let atmo_radius = atmos.planet_radius + atmos.atmosphere_height;
    let ozone_center = 25.0;
    let sun_trans = sun_transmittance(atmos.camera_pos, sun_dir, planet_center, atmo_radius, ozone_center);

    // Real solar disc angular radius is ~0.27 deg. We use a slightly larger
    // soft disc so it remains visible while keeping a soft limb.
    let cos_outer = 0.9995;  // ~1.8 deg
    let cos_inner = 0.99995; // ~0.57 deg
    let sun_disc = smoothstep(cos_outer, cos_inner, cos_sun);
    // Keep the sun disc visible but avoid a multiplier that pushes it far
    // above the scattered sky radiance; the disc should fade with the same
    // extinction as the surrounding sky.
    color += sun_disc * sun_trans * atmos.sun_intensity;

    return vec4<f32>(color, 1.0);
}
"#;
