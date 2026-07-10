//! WGSL shader source for the volumetric cloud pass.
//!
//! Phase 3 reconstruction: embeds the asset shader at
//! `assets/shaders/clouds/volumetric_clouds.wgsl`.

/// Full-screen ray-marched cloud shader.
pub(crate) const SHADER: &str = include_str!(
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/shaders/clouds/volumetric_clouds.wgsl"
    )
);
