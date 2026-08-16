// WGSL shader for water rendering.

pub(super) const WATER_SHADER_SRC: &str = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct WaterUniform {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    water_color: vec4<f32>,
    deep_color: vec4<f32>,
    wave_direction: vec2<f32>,
    wave_amplitude: f32,
    wave_wavelength: f32,
    wave_speed: f32,
    wave_steepness: f32,
    time: f32,
    level: f32,
    fresnel_power: f32,
    refraction_scale: f32,
    reflectivity: f32,
    texture_scale: f32,
    dudv_strength: f32,
    has_dudv: u32,
    has_normal: u32,
    normal_strength: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
    sun_direction: vec4<f32>,
    sun_color: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
    depth_scale: f32,
    specular_power: f32,
    secondary_scale: f32,
    _pad4: f32,
    flow_speed: vec2<f32>,
    flow_speed_2: vec2<f32>,
    reflection_enabled: u32,
    reflection_resolution_scale: f32,
    _pad5: f32,
    _pad6: f32,
};

@group(0) @binding(0) var<uniform> water: WaterUniform;
@group(1) @binding(0) var scene_color: texture_2d<f32>;
@group(1) @binding(1) var reflection_texture: texture_2d<f32>;
@group(1) @binding(2) var tex_sampler: sampler;
@group(1) @binding(3) var depth_tex: texture_depth_2d;
@group(1) @binding(4) var planar_reflection: texture_2d<f32>;
@group(2) @binding(0) var dudv_map: texture_2d<f32>;
@group(2) @binding(1) var normal_map: texture_2d<f32>;
@group(2) @binding(2) var water_sampler: sampler;

const PI: f32 = 3.14159265359;

fn gerstner_displacement(position: vec2<f32>, time: f32) -> vec3<f32> {
    var dir = normalize(water.wave_direction);
    if (length(dir) < 0.0001) {
        dir = vec2<f32>(1.0, 0.0);
    }
    let w = 2.0 * PI / max(water.wave_wavelength, 0.001);
    let phi = water.wave_speed * w * time;
    let q = water.wave_steepness;
    let theta = w * dot(dir, position) + phi;
    let c = cos(theta);
    let s = sin(theta);
    let amp = water.wave_amplitude;
    return vec3<f32>(
        q * amp * dir.x * c,
        amp * s,
        q * amp * dir.y * c
    );
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let base_pos = in.position;
    let displaced = base_pos + gerstner_displacement(base_pos.xz, water.time);
    let world_pos = vec3<f32>(displaced.x, displaced.y + water.level, displaced.z);
    out.clip_position = water.view_proj * vec4<f32>(world_pos, 1.0);
    out.world_pos = world_pos;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(scene_color, 0));
    let screen_uv = in.clip_position.xy / dims;
    let screen_coord = vec2<i32>(screen_uv * dims);

    // Manual depth test: discard this water fragment if opaque geometry is in front.
    let surface_depth = textureLoad(depth_tex, screen_coord, 0);
    if (surface_depth < in.clip_position.z - 0.0001) {
        discard;
    }

    // Two layers of animated texture coordinates for dudv / normal maps.
    let flow0 = water.time * water.flow_speed;
    let flow1 = water.time * water.flow_speed_2;
    let uv0 = in.uv * water.texture_scale + flow0;
    let uv1 = in.uv * water.texture_scale * water.secondary_scale + flow1;

    // Sample dudv map when configured; otherwise no distortion.
    var distortion = vec2<f32>(0.0);
    if (water.has_dudv != 0u) {
        let dudv0 = textureSample(dudv_map, water_sampler, uv0).rg;
        let dudv1 = textureSample(dudv_map, water_sampler, uv1).rg;
        let dudv = (dudv0 + dudv1) * 0.5;
        distortion = (dudv * 2.0 - 1.0) * water.dudv_strength;
    }

    // Reconstruct surface normal from position derivatives.
    let ddx = dpdx(in.world_pos);
    let ddy = dpdy(in.world_pos);
    var normal = normalize(cross(ddx, ddy));

    // Perturb normal using the normal map when configured.
    if (water.has_normal != 0u) {
        let normal0 = textureSample(normal_map, water_sampler, uv0).rgb;
        let normal1 = textureSample(normal_map, water_sampler, uv1).rgb;
        let normal_sample = normalize((normal0 + normal1) * 0.5);
        let tangent_normal = normalize(normal_sample * 2.0 - 1.0);

        // Build a robust tangent frame around the geometry normal.
        let up = select(
            vec3<f32>(1.0, 0.0, 0.0),
            vec3<f32>(0.0, 1.0, 0.0),
            abs(normal.y) < 0.9999
        );
        let tangent = normalize(cross(up, normal));
        let bitangent = cross(normal, tangent);
        let mapped_normal = tangent_normal.x * tangent
                          + tangent_normal.y * bitangent
                          + tangent_normal.z * normal;
        normal = normalize(mix(normal, mapped_normal, clamp(water.normal_strength, 0.0, 1.0)));
    }

    let view_dir = normalize(water.camera_pos.xyz - in.world_pos);
    let n_dot_v = max(dot(normal, view_dir), 0.0);
    let fresnel = pow(1.0 - n_dot_v, water.fresnel_power);

    // Refraction: sample the lit scene behind the water surface, distorted by dudv + normal.
    let refract_uv = clamp(
        screen_uv + distortion + normal.xz * water.refraction_scale,
        vec2<f32>(0.0),
        vec2<f32>(1.0)
    );
    let refract_color = textureSample(scene_color, tex_sampler, refract_uv).rgb;

    // Reconstruct the underwater hit point from the distorted refract UV and depth buffer.
    let refract_coord = clamp(
        vec2<i32>(refract_uv * dims),
        vec2<i32>(0),
        vec2<i32>(dims) - vec2<i32>(1)
    );
    let refract_depth = textureLoad(depth_tex, refract_coord, 0);
    var depth_blend = 0.0;
    if (refract_depth < 0.9999) {
        let ndc = vec4<f32>(
            refract_uv.x * 2.0 - 1.0,
            1.0 - refract_uv.y * 2.0,
            refract_depth,
            1.0
        );
        let underwater_h = water.inv_view_proj * ndc;
        let underwater_pos = underwater_h.xyz / underwater_h.w;
        let thickness = max(in.world_pos.y - underwater_pos.y, 0.0);
        depth_blend = 1.0 - exp(-thickness * water.depth_scale);
    }

    // Depth-aware water color: deeper water is more tinted toward deep_color,
    // but still retains the refracted scene so underwater detail stays visible.
    let water_tint = mix(water.water_color, water.deep_color, depth_blend).rgb;
    let tinted = refract_color * water_tint;
    let refracted = mix(refract_color, tinted, depth_blend);

    // Reflection: sample planar reflection when enabled, otherwise fall back to SSR.
    let reflect_uv = clamp(
        screen_uv + normal.xz * water.refraction_scale * 0.5,
        vec2<f32>(0.0),
        vec2<f32>(1.0)
    );
    var reflection = textureSample(reflection_texture, tex_sampler, reflect_uv).rgb;
    if (water.reflection_enabled != 0u) {
        reflection = textureSample(planar_reflection, tex_sampler, reflect_uv).rgb;
    }

    // Specular highlight from the directional light, using the perturbed normal.
    let sun_dir = normalize(water.sun_direction.xyz);
    let sun_color = water.sun_color.rgb;
    let half_vec = normalize(sun_dir + view_dir);
    let n_dot_h = max(dot(normal, half_vec), 0.0);
    let specular = pow(n_dot_h, water.specular_power) * sun_color;

    // Sky gradient fallback for grazing angles (visible when SSR misses).
    let sky_color = mix(vec3<f32>(0.45, 0.65, 0.9), vec3<f32>(0.9, 0.95, 1.0), max(normal.y, 0.0));
    let reflected = mix(reflection, sky_color, 0.25);

    var final_color = mix(refracted, reflected, fresnel * water.reflectivity);
    final_color = final_color + specular * (1.0 - fresnel * 0.3);

    let alpha = mix(mix(0.6, 0.8, depth_blend), 0.95, fresnel);
    return vec4<f32>(final_color, alpha);
}
"#;
