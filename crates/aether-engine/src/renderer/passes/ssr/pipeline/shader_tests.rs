use super::trace_shader::SSR_TRACE_SHADER_SRC;
use super::upsample_shader::SSR_UPSAMPLE_SHADER_SRC;

#[test]
fn ssr_shaders_parse_without_a_gpu() {
    naga::front::wgsl::parse_str(SSR_TRACE_SHADER_SRC)
        .expect("SSR trace shader should remain valid WGSL");
    naga::front::wgsl::parse_str(SSR_UPSAMPLE_SHADER_SRC)
        .expect("SSR upsample shader should remain valid WGSL");
}

#[test]
fn trace_uses_linear_depth_and_stable_roughness_sampling() {
    assert!(SSR_TRACE_SHADER_SRC.contains("var sample_count = i32(settings.linear_steps)"));
    assert!(SSR_TRACE_SHADER_SRC.contains("let ray_depth = ray_clip.w"));
    assert!(SSR_TRACE_SHADER_SRC.contains("roughness * roughness"));
    assert!(!SSR_TRACE_SHADER_SRC.contains("let z_tolerance = 0.002"));
}

#[test]
fn trace_fades_rough_surfaces_more_aggressively() {
    assert!(SSR_TRACE_SHADER_SRC.contains("roughness_factor * roughness_factor"));
}

#[test]
fn upsample_uses_normal_guidance() {
    assert!(SSR_UPSAMPLE_SHADER_SRC.contains("@binding(1) var gbuffer_normal"));
    assert!(SSR_UPSAMPLE_SHADER_SRC.contains("normal_weight"));
}

#[test]
fn upsample_samples_the_center_of_each_half_resolution_texel() {
    assert!(SSR_UPSAMPLE_SHADER_SRC.contains("floor(vec2<f32>(tap_px_clamped) * 0.5)"));
    assert!(SSR_UPSAMPLE_SHADER_SRC.contains("+ vec2<f32>(0.5)"));
}
