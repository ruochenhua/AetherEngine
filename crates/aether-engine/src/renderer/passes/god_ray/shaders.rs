// WGSL shader for god ray rendering.

pub(super) const GOD_RAY_SHADER_SRC: &str = r#"
struct GodRayUniform {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_direction: vec4<f32>,
    params: vec4<f32>,
    exposure: f32,
};

@group(0) @binding(0) var<uniform> ray: GodRayUniform;
@group(1) @binding(0) var depth_tex: texture_depth_2d;

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
    let world_h = ray.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;

    // Compute screen-space sun position.
    let sun_world = ray.camera_pos.xyz + ray.sun_direction.xyz * 1000.0;
    let sun_clip = ray.view_proj * vec4<f32>(sun_world, 1.0);
    let sun_ndc = sun_clip.xyz / sun_clip.w;
    let sun_uv = vec2<f32>(sun_ndc.x * 0.5 + 0.5, 0.5 - sun_ndc.y * 0.5);
    let sun_screen = sun_uv * dims;

    let delta = sun_screen - frag_coord.xy;
    let ray_dir = delta / dims;

    let samples = u32(ray.params.x);
    let density = ray.params.y;
    let decay_rate = ray.params.z;
    let weight = ray.params.w;
    let exposure = ray.exposure;

    var illumination = 0.0;
    var decay = 1.0;
    let step_size = 1.0 / f32(samples);

    for (var i = 0u; i < samples; i = i + 1u) {
        let t = f32(i) * step_size * density;
        let sample_uv = uv + ray_dir * t;
        let sample_coord = clamp(vec2<i32>(sample_uv * dims), vec2<i32>(0), vec2<i32>(dims) - vec2<i32>(1));
        let sample_depth = textureLoad(depth_tex, sample_coord, 0);

        // Treat far-plane (sky) samples as lit; geometry samples are occluders.
        if (sample_depth > 0.9999) {
            illumination += decay * weight;
        }
        decay *= decay_rate;
    }

    let intensity = illumination * exposure;
    let color = vec3<f32>(1.0, 0.95, 0.8) * intensity;
    return vec4<f32>(color, intensity);
}
"#;
