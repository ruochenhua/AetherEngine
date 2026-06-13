//! Terrain splatting fragment shader — compile-only foundation for Phase 5.
//!
//! Samples a splat map and blends up to four material layers. In Phase 5 this
//! shader will be wired into `TerrainPass` with real layer texture arrays.

@group(0) @binding(0) var splat_sampler: sampler;
@group(0) @binding(1) var splat_map: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct FragmentOutput {
    @location(0) albedo: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) material: vec4<f32>,
};

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    let weights = textureSample(splat_map, splat_sampler, in.uv);

    // Placeholder layer colors; Phase 5 will sample a texture array per layer.
    let layer_colors = array<vec3<f32>, 4>(
        vec3<f32>(0.8, 0.2, 0.2),
        vec3<f32>(0.2, 0.8, 0.2),
        vec3<f32>(0.2, 0.2, 0.8),
        vec3<f32>(0.8, 0.8, 0.2)
    );

    var albedo = vec3<f32>(0.0);
    for (var i = 0; i < 4; i = i + 1) {
        albedo = albedo + layer_colors[i] * weights[i];
    }

    var out: FragmentOutput;
    out.albedo = vec4<f32>(albedo, 1.0);
    out.normal = vec4<f32>(0.0, 1.0, 0.0, 1.0);
    // material: roughness, metallic, ao, emissive
    out.material = vec4<f32>(0.8, 0.0, 1.0, 0.0);
    return out;
}
