//! Procedural noise generation for volumetric clouds.
//!
//! Phase 3 reconstruction: CPU-side generators that produce the same
//! RGBA8 noise textures that RenderEngine builds on the GPU.

pub mod generate;

pub use generate::generate_cloud_noise_textures;
