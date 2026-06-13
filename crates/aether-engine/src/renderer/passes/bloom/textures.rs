//! Bloom intermediate texture allocation.
//!
//! Creates the bright texture, mip-chain, and final bloom texture used
//! during the multi-pass bloom effect.

use super::BloomPass;

/// Recreate all intermediate bloom textures at the pass's current screen size.
pub(super) fn create_intermediate_textures(pass: &mut BloomPass, device: &wgpu::Device) {
    let w = pass.screen_width;
    let h = pass.screen_height;

    pass.bright_texture = Some(create_texture(device, "Bloom Bright", w, h));
    pass.mip0 = Some(create_texture(device, "Bloom Mip0", w / 2, h / 2));
    pass.mip1 = Some(create_texture(device, "Bloom Mip1", w / 4, h / 4));
    pass.mip2 = Some(create_texture(device, "Bloom Mip2", w / 8, h / 8));
    pass.bloom_texture = Some(create_texture(device, "Bloom Texture", w, h));
}

fn create_texture(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
