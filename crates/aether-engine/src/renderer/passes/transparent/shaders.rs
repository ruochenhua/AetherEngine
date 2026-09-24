//! WGSL for the general transparent overlay.

pub(super) const TRANSPARENT_SHADER_SRC: &str = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct ViewProjUniform {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> vp: ViewProjUniform;

struct ObjectUniform {
    model: mat4x4<f32>,
    color: vec4<f32>,
    alpha_cutoff: f32,
    cutoff_enabled: u32,
};
@group(1) @binding(0) var<uniform> obj: ObjectUniform;
@group(2) @binding(0) var albedo_texture: texture_2d<f32>;
@group(2) @binding(1) var albedo_sampler: sampler;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vp.proj * vp.view * obj.model * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(albedo_texture, albedo_sampler, in.uv);
    let alpha = clamp(obj.color.a * texel.a, 0.0, 1.0);
    if (obj.cutoff_enabled != 0u && alpha < obj.alpha_cutoff) {
        discard;
    }
    return vec4<f32>(obj.color.rgb * texel.rgb * alpha, alpha);
}
"#;

#[cfg(test)]
mod tests {
    use super::TRANSPARENT_SHADER_SRC;

    #[test]
    fn transparent_mesh_shader_samples_albedo_and_applies_cutoff_to_sampled_alpha() {
        assert!(TRANSPARENT_SHADER_SRC.contains("textureSample(albedo_texture"));
        assert!(TRANSPARENT_SHADER_SRC.contains("alpha < obj.alpha_cutoff"));
    }
}
