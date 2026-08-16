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
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) world_pos: vec3<f32>, @location(1) world_normal: vec3<f32>, @location(2) uv: vec2<f32>, };
struct ViewProjUniform { view: mat4x4<f32>, proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> vp: ViewProjUniform;

struct ObjectData { albedo: vec4<f32>, roughness: f32, metallic: f32, };
@group(1) @binding(0) var<uniform> obj: ObjectData;
@group(2) @binding(0) var albedo_texture: texture_2d<f32>;
@group(2) @binding(1) var albedo_sampler: sampler;

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3);
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = vp.proj * vp.view * world_pos;
    out.world_pos = world_pos.xyz;
    let nm = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = normalize(nm * in.normal);
    out.uv = in.uv;
    return out;
}

struct FragmentOutput { @location(0) position: vec4<f32>, @location(1) normal: vec4<f32>, @location(2) albedo: vec4<f32>, @location(3) material: vec2<f32>, }
@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    out.position = vec4<f32>(in.world_pos, 1.0);
    out.normal = vec4<f32>(in.world_normal * 0.5 + 0.5, 1.0);
    let tex_color = textureSample(albedo_texture, albedo_sampler, in.uv);
    out.albedo = obj.albedo * tex_color;
    out.material = vec2<f32>(obj.roughness, obj.metallic);
    return out;
}
"#;
