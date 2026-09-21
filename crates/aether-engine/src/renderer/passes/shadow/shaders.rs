// WGSL shader for cascaded shadow mapping.

pub(super) const SHADOW_SHADER_SRC: &str = r#"
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
    @location(0) world_position: vec3<f32>,
};
struct LightVP { light_view_proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> lvp: LightVP;
@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3);
    let world_position = (model * vec4<f32>(in.position, 1.0)).xyz;
    out.clip_position = lvp.light_view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) {
    // Faces nearly parallel to the light direction collapse to lines in the
    // orthographic shadow view. Metal can rasterize those edge-on triangles as
    // unstable depth strips, so discard only the degenerate projected faces.
    let light_axis = normalize(vec3<f32>(
        lvp.light_view_proj[0].z,
        lvp.light_view_proj[1].z,
        lvp.light_view_proj[2].z,
    ));
    let geometric_normal = normalize(cross(dpdx(in.world_position), dpdy(in.world_position)));
    if (abs(dot(geometric_normal, light_axis)) < 0.02) {
        discard;
    }
}
"#;
