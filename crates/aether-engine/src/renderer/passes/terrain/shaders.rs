//! WGSL shader source for the terrain pass.

/// Terrain GBuffer pass shader: transforms instances and writes layered PBR
/// material data into the deferred GBuffer.
pub const TERRAIN: &str = r#"
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
struct ViewProjUniform { view: mat4x4<f32>, proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> vp: ViewProjUniform;

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
    out.clip_position = vp.proj * vp.view * world_pos;
    out.world_pos = world_pos.xyz;
    let nm = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = normalize(nm * in.normal);
    // Use world-space XZ as UV so textures tile continuously across chunks.
    out.uv = world_pos.xz;
    return out;
}

struct FragmentOutput {
    @location(0) position: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) albedo: vec4<f32>,
    @location(3) material: vec2<f32>,
}
@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    out.position = vec4<f32>(in.world_pos, 1.0);
    out.normal = vec4<f32>(in.world_normal * 0.5 + 0.5, 1.0);

    // Compute UVs from world-space XZ so adjacent chunks share continuous texture
    // space. The splat map covers the terrain once; albedo textures tile.
    let splat_uv = in.uv * terrain.splat_uv_scale + vec2<f32>(0.5);
    let albedo_uv = in.uv * terrain.albedo_uv_scale;

    // Sample splat map if present; otherwise default to the first layer.
    var weights: vec4<f32>;
    if (terrain.has_splat_map != 0u) {
        let splat = textureSample(splat_map, terrain_sampler, splat_uv);
        weights = splat;
    } else {
        weights = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }

    // Sample each layer's albedo texture with its own UV scale and blend using
    // the splat weights. Different scales keep the layers from tiling in sync.
    let uv0 = albedo_uv * terrain.layer_uv_scale.x;
    let uv1 = albedo_uv * terrain.layer_uv_scale.y;
    let uv2 = albedo_uv * terrain.layer_uv_scale.z;
    let uv3 = albedo_uv * terrain.layer_uv_scale.w;
    let c0 = terrain.layer_color_0 * textureSample(layer_albedo_0, terrain_sampler, uv0);
    let c1 = terrain.layer_color_1 * textureSample(layer_albedo_1, terrain_sampler, uv1);
    let c2 = terrain.layer_color_2 * textureSample(layer_albedo_2, terrain_sampler, uv2);
    let c3 = terrain.layer_color_3 * textureSample(layer_albedo_3, terrain_sampler, uv3);

    let albedo = c0 * weights.x + c1 * weights.y + c2 * weights.z + c3 * weights.w;
    let roughness = dot(terrain.layer_roughness, weights);
    let metallic = dot(terrain.layer_metallic, weights);

    out.albedo = albedo;
    out.material = vec2<f32>(roughness, metallic);
    return out;
}
"#;
