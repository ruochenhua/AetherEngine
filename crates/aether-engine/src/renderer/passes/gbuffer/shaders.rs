// WGSL shader for G-Buffer rendering.

pub(super) const GBUFFER_SHADER_SRC: &str = r#"
struct VertexInput { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) tangent: vec4<f32>, };
struct InstanceInput {
    @location(4) model_matrix_0: vec4<f32>,
    @location(5) model_matrix_1: vec4<f32>,
    @location(6) model_matrix_2: vec4<f32>,
    @location(7) model_matrix_3: vec4<f32>,
    @location(8) entity_id: u32,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) world_tangent: vec3<f32>,
    @location(3) tangent_sign: f32,
    @location(4) uv: vec2<f32>,
};
struct ViewProjUniform { view: mat4x4<f32>, proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> vp: ViewProjUniform;

struct ObjectData {
    albedo: vec4<f32>,
    roughness: f32,
    metallic: f32,
    unlit: u32,
    normal_scale: f32,
    occlusion_strength: f32,
    emissive_r: f32,
    emissive_g: f32,
    emissive_b: f32,
    emissive_intensity: f32,
    normal_present: u32,
    orm_present: u32,
    emissive_present: u32,
    _material_pad: u32,
};
@group(1) @binding(0) var<uniform> obj: ObjectData;
@group(2) @binding(0) var albedo_texture: texture_2d<f32>;
@group(2) @binding(1) var albedo_sampler: sampler;
@group(2) @binding(2) var normal_texture: texture_2d<f32>;
@group(2) @binding(3) var normal_sampler: sampler;
@group(2) @binding(4) var orm_texture: texture_2d<f32>;
@group(2) @binding(5) var orm_sampler: sampler;
@group(2) @binding(6) var emissive_texture: texture_2d<f32>;
@group(2) @binding(7) var emissive_sampler: sampler;

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3);
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = vp.proj * vp.view * world_pos;
    out.world_pos = world_pos.xyz;
    let nm = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = normalize(nm * in.normal);
    out.world_tangent = normalize(nm * in.tangent.xyz);
    out.tangent_sign = in.tangent.w;
    out.uv = in.uv;
    return out;
}

struct FragmentOutput {
    @location(0) position: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) albedo: vec4<f32>,
    @location(3) material: vec2<f32>,
    @location(4) emissive: vec4<u32>,
}
@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    out.position = vec4<f32>(in.world_pos, 1.0);
    let tex_color = textureSample(albedo_texture, albedo_sampler, in.uv);
    var N = normalize(in.world_normal);
    if (obj.normal_present != 0u) {
        let T = normalize(in.world_tangent - N * dot(N, in.world_tangent));
        let B = normalize(cross(N, T)) * select(-1.0, 1.0, in.tangent_sign >= 0.0);
        let tangent_normal = textureSample(normal_texture, normal_sampler, in.uv).xyz * 2.0 - 1.0;
        let scaled_normal = vec3<f32>(tangent_normal.xy * obj.normal_scale, tangent_normal.z);
        N = normalize(T * scaled_normal.x + B * scaled_normal.y + N * scaled_normal.z);
    }
    // Alpha is reserved as a compact deferred flag; regular materials write 0,
    // while unlit markers write 1 and bypass the lighting pass.
    out.albedo = vec4<f32>((obj.albedo * tex_color).rgb, f32(obj.unlit));
    var roughness = obj.roughness;
    var metallic = obj.metallic;
    var ao = 1.0;
    if (obj.orm_present != 0u) {
        let orm = textureSample(orm_texture, orm_sampler, in.uv).rgb;
        roughness = roughness * orm.g;
        metallic = metallic * orm.b;
        ao = mix(1.0, orm.r, obj.occlusion_strength);
    }
    out.normal = vec4<f32>(N * 0.5 + 0.5, ao);
    out.material = vec2<f32>(clamp(roughness, 0.045, 1.0), clamp(metallic, 0.0, 1.0));
    let emissive_texel = select(
        vec3<f32>(1.0),
        textureSample(emissive_texture, emissive_sampler, in.uv).rgb,
        obj.emissive_present != 0u,
    );
    let emissive_linear = clamp(
        vec4<f32>(obj.emissive_r, obj.emissive_g, obj.emissive_b, 1.0)
            * vec4<f32>(emissive_texel * obj.emissive_intensity, 1.0),
        vec4<f32>(0.0),
        vec4<f32>(1.0),
    );
    out.emissive = vec4<u32>(emissive_linear * 255.0);
    return out;
}
"#;
