use super::shaders::LIGHTING_SHADER_SRC;

#[test]
fn lighting_shader_parses_and_declares_local_light_storage() {
    naga::front::wgsl::parse_str(LIGHTING_SHADER_SRC)
        .expect("lighting shader should remain valid WGSL");
    assert!(LIGHTING_SHADER_SRC.contains("@group(4) @binding(0) var<storage, read> local_lights"));
    assert!(LIGHTING_SHADER_SRC.contains("local_light_params.count"));
    assert!(LIGHTING_SHADER_SRC.contains("evaluate_direct_light"));
    assert!(LIGHTING_SHADER_SRC.contains("albedo_sample.a > 0.5"));
    assert!(LIGHTING_SHADER_SRC.contains("normal_sample.a"));
}

#[test]
fn directional_only_shader_parses_without_local_light_bindings() {
    let source = super::shader_variant::shader_source(false);
    naga::front::wgsl::parse_str(&source)
        .expect("directional-only lighting shader should remain valid WGSL");
    assert!(!source.contains("@group(4)"));
    assert!(source.contains("let local_direct = vec3<f32>(0.0);"));
}

#[test]
fn local_light_bindings_are_isolated_in_group_four() {
    let source = super::shader_variant::shader_source(true);
    assert!(source.contains("@group(4) @binding(0) var<storage, read> local_lights"));
    assert!(source.contains("@group(4) @binding(1) var<uniform> local_light_params"));
    assert!(!source.contains("@group(3) @binding(5)"));
}

#[test]
fn lighting_shader_consumes_gbuffer_emissive() {
    assert!(LIGHTING_SHADER_SRC.contains("gbuffer_emissive"));
    assert!(LIGHTING_SHADER_SRC.contains("emissive_sample"));
    assert!(LIGHTING_SHADER_SRC.contains("+ emissive_sample"));
}
