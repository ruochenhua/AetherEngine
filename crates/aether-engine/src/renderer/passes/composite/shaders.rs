// WGSL shader for composite pass.

pub(super) const COMPOSITE_SHADER_SRC: &str = r#"
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

@group(0) @binding(0) var scene_color: texture_2d<f32>;
@group(0) @binding(1) var reflection_texture: texture_2d<f32>;
@group(0) @binding(2) var water_color: texture_2d<f32>;
@group(0) @binding(3) var cloud_color: texture_2d<f32>;
@group(0) @binding(4) var god_ray_color: texture_2d<f32>;
@group(0) @binding(5) var tex_sampler: sampler;
@group(0) @binding(6) var gbuffer_position: texture_2d<f32>;
@group(0) @binding(7) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(8) var gbuffer_albedo: texture_2d<f32>;
@group(0) @binding(9) var gbuffer_material: texture_2d<f32>;

struct CompositeUniforms {
    camera_pos: vec3<f32>,
    _pad0: f32,
};

@group(1) @binding(0) var<uniform> uniforms: CompositeUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let scene = textureSample(scene_color, tex_sampler, uv);
    let refl  = textureSample(reflection_texture, tex_sampler, uv);
    let water = textureSample(water_color, tex_sampler, uv);
    let cloud = textureSample(cloud_color, tex_sampler, uv);
    let god_ray = textureSample(god_ray_color, tex_sampler, uv);

    // Decode G-Buffer to compute Fresnel reflectance.
    let world_pos = textureSample(gbuffer_position, tex_sampler, uv).xyz;
    let normal_sample = textureSample(gbuffer_normal, tex_sampler, uv);
    let albedo = textureSample(gbuffer_albedo, tex_sampler, uv).rgb;
    let material_sample = textureSample(gbuffer_material, tex_sampler, uv);

    var lit: vec3<f32>;
    if (normal_sample.r == 0.0 && normal_sample.g == 0.0 && normal_sample.b == 0.0) {
        // Sky / background: bypass SSR blend.
        lit = scene.rgb;
    } else {
        let N = normalize(normal_sample.xyz * 2.0 - 1.0);
        let V = normalize(uniforms.camera_pos - world_pos);
        let NdotV = max(dot(N, V), 0.0);
        let roughness = material_sample.r;
        let metallic = material_sample.g;
        let F0 = mix(vec3<f32>(0.04), albedo, metallic);
        let fresnel = F0 + (vec3<f32>(1.0) - F0) * pow(1.0 - NdotV, 5.0);
        // Mask low-roughness surfaces: SSR pass already encodes roughness in refl.a,
        // but re-apply here so non-reflective pixels are unchanged.
        let reflectance = fresnel * refl.a;
        lit = mix(scene.rgb, refl.rgb, reflectance);
    }

    // Volumetric clouds are integrated as background * transmittance + in-scattered light.
    // cloud.rgb already contains the accumulated scattered light; cloud.a = 1 - transmittance.
    let with_clouds = lit * (1.0 - cloud.a) + cloud.rgb;
    let with_god_rays = with_clouds + god_ray.rgb;
    let final_color = mix(with_god_rays, water.rgb, water.a);
    return vec4<f32>(final_color, 1.0);
}
"#;
