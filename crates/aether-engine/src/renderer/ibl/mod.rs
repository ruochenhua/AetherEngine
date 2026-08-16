//! Image-Based Lighting loader.
//!
//! Follows LearnOpenGL PBR/IBL tutorials:
//! - Diffuse irradiance: https://learnopengl.com/PBR/IBL/Diffuse-irradiance
//! - Specular IBL: https://learnopengl.com/PBR/IBL/Specular-IBL
//!
//! Uses render-to-cubemap (fragment shader) for equirect→cubemap,
//! irradiance convolution, and prefiltering. BRDF LUT uses compute shader.

pub(crate) mod brdf;
pub(crate) mod config;
pub(crate) mod cubemap;
pub(crate) mod equirect;
pub(crate) mod generate;
pub(crate) mod hdr;
pub(crate) mod irradiance;
pub(crate) mod prefilter;
pub(crate) mod resources;
pub(crate) mod shaders;

pub use config::IblConfig;
pub use cubemap::CpuCubemap;
pub use resources::IblResources;
