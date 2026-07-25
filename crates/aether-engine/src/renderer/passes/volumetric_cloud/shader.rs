//! WGSL shader source for the volumetric cloud pass.
//!
//! The render-pass WGSL is kept in the shader asset so the Rust pass module
//! stays focused on pipeline wiring and remains within the module-size limit.

/// Full-screen ray-marched cloud shader.
pub(crate) const SHADER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/shaders/clouds/volumetric_clouds.wgsl"
));
