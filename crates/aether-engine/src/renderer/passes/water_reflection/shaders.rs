// WGSL shaders for planar water reflection.

pub(super) const REFLECTION_SHADER_SRC: &str = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};
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
    @location(2) uv: vec2<f32>,
};
struct ReflectionUniform {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    ambient: vec4<f32>,
};
@group(0) @binding(0) var<uniform> ru: ReflectionUniform;

struct ObjectData { albedo: vec4<f32>, roughness: f32, metallic: f32, };
@group(1) @binding(0) var<uniform> obj: ObjectData;
@group(2) @binding(0) var albedo_texture: texture_2d<f32>;
@group(2) @binding(1) var albedo_sampler: sampler;

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3);
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = ru.proj * ru.view * world_pos;
    out.world_pos = world_pos.xyz;
    let nm = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = normalize(nm * in.normal);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(albedo_texture, albedo_sampler, in.uv);
    let albedo = obj.albedo.rgb * tex_color.rgb;

    let n = normalize(in.world_normal);
    let l = normalize(ru.light_dir.xyz);
    let v = normalize(-in.world_pos);
    let h = normalize(l + v);

    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);

    let diffuse = albedo * n_dot_l * ru.light_color.rgb;
    let specular = pow(n_dot_h, 64.0) * ru.light_color.rgb;
    let ambient = albedo * ru.ambient.rgb;

    return vec4<f32>(ambient + diffuse + specular * 0.3, 1.0);
}
"#;

pub(super) const TERRAIN_REFLECTION_SHADER_SRC: &str = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};
struct InstanceInput {
    @location(4) model_matrix_0: vec4<f32>,
    @location(5) model_matrix_1: vec4<f32>,
    @location(6) model_matrix_2: vec4<f32>,
    @location(7) model_matrix_3: vec4<f32>,
    @location(8) lod: u32,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};
struct ReflectionUniform {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    ambient: vec4<f32>,
};
@group(0) @binding(0) var<uniform> ru: ReflectionUniform;

struct TerrainUniform {
    layer_color_0: vec4<f32>,
    layer_color_1: vec4<f32>,
    layer_color_2: vec4<f32>,
    layer_color_3: vec4<f32>,
    layer_roughness: vec4<f32>,
    layer_metallic: vec4<f32>,
    has_splat_map: u32,
    _pad0: u32,
    splat_uv_scale: f32,
    albedo_uv_scale: f32,
    layer_uv_scale: vec4<f32>,
};
@group(1) @binding(0) var<uniform> terrain: TerrainUniform;
@group(1) @binding(1) var splat_map: texture_2d<f32>;
@group(1) @binding(2) var terrain_sampler: sampler;
@group(1) @binding(3) var layer_albedo_0: texture_2d<f32>;
@group(1) @binding(4) var layer_albedo_1: texture_2d<f32>;
@group(1) @binding(5) var layer_albedo_2: texture_2d<f32>;
@group(1) @binding(6) var layer_albedo_3: texture_2d<f32>;

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3);
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = ru.proj * ru.view * world_pos;
    out.world_pos = world_pos.xyz;
    let nm = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = normalize(nm * in.normal);
    out.uv = world_pos.xz;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let splat_uv = in.uv * terrain.splat_uv_scale + vec2<f32>(0.5);
    let albedo_uv = in.uv * terrain.albedo_uv_scale;

    var weights: vec4<f32>;
    if (terrain.has_splat_map != 0u) {
        weights = textureSample(splat_map, terrain_sampler, splat_uv);
    } else {
        weights = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }

    let uv0 = albedo_uv * terrain.layer_uv_scale.x;
    let uv1 = albedo_uv * terrain.layer_uv_scale.y;
    let uv2 = albedo_uv * terrain.layer_uv_scale.z;
    let uv3 = albedo_uv * terrain.layer_uv_scale.w;
    let c0 = terrain.layer_color_0 * textureSample(layer_albedo_0, terrain_sampler, uv0);
    let c1 = terrain.layer_color_1 * textureSample(layer_albedo_1, terrain_sampler, uv1);
    let c2 = terrain.layer_color_2 * textureSample(layer_albedo_2, terrain_sampler, uv2);
    let c3 = terrain.layer_color_3 * textureSample(layer_albedo_3, terrain_sampler, uv3);

    let albedo = c0 * weights.x + c1 * weights.y + c2 * weights.z + c3 * weights.w;

    let n = normalize(in.world_normal);
    let l = normalize(ru.light_dir.xyz);
    let v = normalize(-in.world_pos);
    let h = normalize(l + v);

    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);

    let diffuse = albedo.rgb * n_dot_l * ru.light_color.rgb;
    let specular = pow(n_dot_h, 64.0) * ru.light_color.rgb;
    let ambient = albedo.rgb * ru.ambient.rgb;

    return vec4<f32>(ambient + diffuse + specular * 0.3, 1.0);
}
"#;
