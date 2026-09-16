use super::shaders::SSAO_SHADER_SRC;

#[test]
fn ssao_shader_parses_without_a_gpu() {
    naga::front::wgsl::parse_str(SSAO_SHADER_SRC).expect("SSAO shader should remain valid WGSL");
}
