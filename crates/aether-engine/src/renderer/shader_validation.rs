//! CPU-only naga validation for every WGSL shader the engine compiles at
//! runtime. No GPU is required: these tests parse and fully validate the
//! sources with the same naga version wgpu 29 uses internally.
//!
//! # Convention (mirrored in the repository `AGENTS.md`)
//!
//! Every production `create_shader_module` call site in this crate must have
//! its WGSL source registered in [`SHADER_MANIFEST`] below. When adding a
//! shader:
//! 1. expose its WGSL as a `pub(crate) const` in the pass module (lift it out
//!    of the function body if it was an inline `let`), and
//! 2. add one `("<pass>/<purpose>", <CONST>)` entry here per source.
//!
//! Shaders that only exist in test code (e.g. the god-ray occluder test
//! shader) are intentionally not covered.

/// (pass/shader purpose, WGSL source) pairs covering every production
/// `create_shader_module` call site in the engine. Keep this in sync with the
/// call sites — see the module-level convention above.
const SHADER_MANIFEST: &[(&str, &str)] = &[
    (
        "clouds/perlin-worley noise (compute)",
        crate::clouds::generate::PERLIN_WORLEY_SHADER,
    ),
    (
        "clouds/worley noise (compute)",
        crate::clouds::generate::WORLEY_SHADER,
    ),
    (
        "clouds/weather map (compute)",
        crate::clouds::generate::WEATHER_SHADER,
    ),
    (
        "ibl/equirectangular to cubemap",
        crate::renderer::ibl::generate::EQUIRECT_SHADER,
    ),
    (
        "ibl/irradiance convolution",
        crate::renderer::ibl::generate::IRRADIANCE_SHADER,
    ),
    (
        "ibl/specular prefilter",
        crate::renderer::ibl::generate::PREFILTER_SHADER,
    ),
    (
        "ibl/brdf lut integration (compute)",
        crate::renderer::ibl::generate::BRDF_LUT_SHADER,
    ),
    (
        "gbuffer/geometry + material",
        crate::renderer::passes::gbuffer::GBUFFER_SHADER,
    ),
    (
        "debug/lines + gizmos",
        crate::renderer::passes::debug::DEBUG_LINE_SHADER,
    ),
    (
        "shadow/cascaded depth",
        crate::renderer::passes::shadow::SHADOW_SHADER,
    ),
    (
        "ao_blur/bilateral blur",
        crate::renderer::passes::ao_blur::AO_BLUR_SHADER,
    ),
    (
        "fxaa/anti-aliasing",
        crate::renderer::passes::fxaa::FXAA_SHADER,
    ),
    (
        "composite/hdr combine",
        crate::renderer::passes::composite::COMPOSITE_SHADER,
    ),
    (
        "atmosphere/sky + aerial perspective",
        crate::renderer::passes::atmosphere::ATMOSPHERE_SHADER,
    ),
    (
        "god_ray/screen-space shafts",
        crate::renderer::passes::god_ray::GOD_RAY_SHADER,
    ),
    (
        "tone_mapping/hdr to ldr",
        crate::renderer::passes::tone_mapping::TONE_MAPPING_SHADER,
    ),
    ("ssao/occlusion", crate::renderer::passes::ssao::SSAO_SHADER),
    (
        "terrain/displacement + splatting",
        crate::renderer::passes::terrain::shaders::TERRAIN,
    ),
    (
        "lighting/deferred pbr + ibl",
        crate::renderer::passes::lighting::pipeline::LIGHTING_SHADER,
    ),
    (
        "water/surface shading",
        crate::renderer::passes::water::pipeline::WATER_SHADER,
    ),
    (
        "ssr/trace",
        crate::renderer::passes::ssr::pipeline::SSR_SHADER,
    ),
    (
        "ssr/upsample",
        crate::renderer::passes::ssr::pipeline::SSR_UPSAMPLE_SHADER,
    ),
    (
        "water_reflection/scene objects",
        crate::renderer::passes::water_reflection::WATER_REFLECTION_SHADER,
    ),
    (
        "water_reflection/terrain",
        crate::renderer::passes::water_reflection::WATER_REFLECTION_TERRAIN_SHADER,
    ),
    (
        "bloom/extract",
        crate::renderer::passes::bloom::shaders::EXTRACT,
    ),
    (
        "bloom/blur",
        crate::renderer::passes::bloom::shaders::BLUR,
    ),
    (
        "bloom/composite",
        crate::renderer::passes::bloom::shaders::COMPOSITE,
    ),
    (
        "volumetric_cloud/raymarch",
        crate::renderer::passes::volumetric_cloud::shader::SHADER,
    ),
];

/// Parse and fully validate one WGSL source with naga (all stages, all
/// capabilities). Panics with the manifest name plus naga's location-aware
/// error report (line/column) on failure.
fn validate_shader_source(name: &str, source: &str) {
    use naga::error::ShaderError;
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|parse_error| {
        let error = ShaderError {
            source: source.to_string(),
            label: Some(name.to_string()),
            inner: Box::new(parse_error),
        };
        panic!("{error}");
    });

    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    validator.validate(&module).unwrap_or_else(|validation_error| {
        let error = ShaderError {
            source: source.to_string(),
            label: Some(name.to_string()),
            inner: Box::new(validation_error),
        };
        panic!("{error}");
    });
}

/// Every shader in the manifest must parse and pass full naga validation.
#[test]
fn all_shaders_pass_naga_validation() {
    for (name, source) in SHADER_MANIFEST {
        // The BRDF LUT source is a template: the runtime replaces `$size`
        // with the LUT dimension before compiling (see ibl/generate.rs).
        let source = source.replace("$size", "512");
        validate_shader_source(name, &source);
    }
}

/// Positive control: the validator must reject a broken shader, proving the
/// harness above actually has teeth (an always-passing check would be worse
/// than none).
#[test]
fn validator_rejects_bad_shader() {
    const BAD_WGSL: &str = r#"
@vertex
fn vs_main() -> @builtin(position) vec4<f32> {
    // vec4<f32> constructed from 3 scalars: invalid.
    return vec4<f32>(0.0, 0.0, 0.0);
}
"#;

    let rejected = match naga::front::wgsl::parse_str(BAD_WGSL) {
        Err(_) => true,
        Ok(module) => {
            let mut validator = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            );
            validator.validate(&module).is_err()
        }
    };
    assert!(
        rejected,
        "naga must reject the intentionally broken shader (parse or validation failure)"
    );
}
