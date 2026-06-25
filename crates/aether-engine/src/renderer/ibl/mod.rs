//! Image-Based Lighting loader.
//!
//! Follows LearnOpenGL PBR/IBL tutorials:
//! - Diffuse irradiance: https://learnopengl.com/PBR/IBL/Diffuse-irradiance
//! - Specular IBL: https://learnopengl.com/PBR/IBL/Specular-IBL
//!
//! Uses render-to-cubemap (fragment shader) for equirect→cubemap,
//! irradiance convolution, and prefiltering. BRDF LUT uses compute shader.

pub(crate) mod config;
pub(crate) mod generate;
pub(crate) mod resources;

pub use config::IblConfig;
pub use generate::CpuCubemap;
pub use resources::IblResources;
