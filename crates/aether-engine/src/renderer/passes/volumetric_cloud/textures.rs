//! GPU texture creation helpers for the volumetric cloud pass.

use crate::clouds::generate::{generate_cloud_noise_textures, CloudNoiseTextures};
use crate::scene::config::CloudQuality;

/// All GPU resources created for a given cloud noise quality preset.
pub(crate) struct NoiseResources {
    pub worley_texture: wgpu::Texture,
    pub worley_view: wgpu::TextureView,
    pub perlin_worley_texture: wgpu::Texture,
    pub perlin_worley_view: wgpu::TextureView,
    pub weather_texture: wgpu::Texture,
    pub weather_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
}

/// Create (or recreate) all noise textures, the shared sampler, and the bind
/// group for the requested quality preset.
pub(crate) fn create_noise_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    quality: CloudQuality,
    layout: &wgpu::BindGroupLayout,
) -> NoiseResources {
    let CloudNoiseTextures {
        perlinworley_texture,
        perlinworley_view,
        worley_texture,
        worley_view,
        weather_texture,
        weather_view,
        sampler,
        bind_group,
    } = generate_cloud_noise_textures(device, queue, layout, quality);

    NoiseResources {
        worley_texture,
        worley_view,
        perlin_worley_texture: perlinworley_texture,
        perlin_worley_view: perlinworley_view,
        weather_texture,
        weather_view,
        sampler,
        bind_group,
    }
}
