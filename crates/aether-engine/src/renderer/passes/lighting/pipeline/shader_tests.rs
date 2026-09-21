use super::shaders::LIGHTING_SHADER_SRC;

#[test]
fn lighting_shader_parses_and_declares_local_light_storage() {
    naga::front::wgsl::parse_str(LIGHTING_SHADER_SRC)
        .expect("lighting shader should remain valid WGSL");
    assert!(LIGHTING_SHADER_SRC.contains("@group(3) @binding(5) var<storage, read> local_lights"));
    assert!(LIGHTING_SHADER_SRC.contains("local_light_params.count"));
    assert!(LIGHTING_SHADER_SRC.contains("evaluate_direct_light"));
    assert!(LIGHTING_SHADER_SRC.contains("albedo_sample.a > 0.5"));
    assert!(LIGHTING_SHADER_SRC.contains("normal_sample.a"));
}

#[test]
fn lighting_shader_consumes_gbuffer_emissive() {
    assert!(LIGHTING_SHADER_SRC.contains("gbuffer_emissive"));
    assert!(LIGHTING_SHADER_SRC.contains("emissive_sample"));
    assert!(LIGHTING_SHADER_SRC.contains("+ emissive_sample"));
}
