//! GPU texture creation helpers for the volumetric cloud pass.

use super::pipeline;
use crate::scene::config::CloudQuality;

/// All GPU resources created for a given cloud noise quality preset.
pub(crate) struct NoiseResources {
    pub worley_texture: wgpu::Texture,
    pub worley_view: wgpu::TextureView,
    pub perlin_worley_texture: wgpu::Texture,
    pub perlin_worley_view: wgpu::TextureView,
    pub curl_texture: wgpu::Texture,
    pub curl_view: wgpu::TextureView,
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
    let sizes = pipeline::NoiseSizes::from(&quality);

    let (worley_texture, worley_view) = create_texture_3d(
        device,
        sizes.worley,
        wgpu::TextureFormat::R8Unorm,
        "Cloud Worley Texture",
    );
    let (perlin_worley_texture, perlin_worley_view) = create_texture_3d(
        device,
        sizes.worley,
        wgpu::TextureFormat::R8Unorm,
        "Cloud Perlin-Worley Texture",
    );
    let (curl_texture, curl_view) = create_texture_3d(
        device,
        sizes.curl,
        wgpu::TextureFormat::Rg8Snorm,
        "Cloud Curl Texture",
    );
    let (weather_texture, weather_view) = create_texture_2d(
        device,
        sizes.weather,
        wgpu::TextureFormat::R8Unorm,
        "Cloud Weather Texture",
    );

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Cloud Multi-Noise Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        ..Default::default()
    });

    // Generate noise data. Worley is reused as the base for Perlin-Worley
    // so the expensive cellular pass is only evaluated once.
    let worley_data = crate::renderer::clouds::worley::worley_noise_3d(sizes.worley);
    let perlin_worley_data =
        crate::renderer::clouds::perlin_worley::perlin_worley_from_worley(
            &worley_data,
            sizes.worley,
        );
    let curl_data = crate::renderer::clouds::curl::curl_noise_3d(sizes.curl);
    let weather_data =
        crate::renderer::clouds::weather::generate_weather_map_2d(sizes.weather);

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &worley_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &worley_data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(sizes.worley),
            rows_per_image: Some(sizes.worley),
        },
        wgpu::Extent3d {
            width: sizes.worley,
            height: sizes.worley,
            depth_or_array_layers: sizes.worley,
        },
    );

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &perlin_worley_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &perlin_worley_data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(sizes.worley),
            rows_per_image: Some(sizes.worley),
        },
        wgpu::Extent3d {
            width: sizes.worley,
            height: sizes.worley,
            depth_or_array_layers: sizes.worley,
        },
    );

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &curl_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&curl_data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(sizes.curl * 2),
            rows_per_image: Some(sizes.curl),
        },
        wgpu::Extent3d {
            width: sizes.curl,
            height: sizes.curl,
            depth_or_array_layers: sizes.curl,
        },
    );

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &weather_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &weather_data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(sizes.weather),
            rows_per_image: Some(sizes.weather),
        },
        wgpu::Extent3d {
            width: sizes.weather,
            height: sizes.weather,
            depth_or_array_layers: 1,
        },
    );

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Cloud Noise Bind Group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&worley_view,
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    &perlin_worley_view,
                ),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(
                    &curl_view,
                ),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(
                    &weather_view,
                ),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(
                    &sampler,
                ),
            },
        ],
    });

    NoiseResources {
        worley_texture,
        worley_view,
        perlin_worley_texture,
        perlin_worley_view,
        curl_texture,
        curl_view,
        weather_texture,
        weather_view,
        sampler,
        bind_group,
    }
}

/// Create a 3D texture suitable for noise data.
pub(crate) fn create_texture_3d(
    device: &wgpu::Device,
    size: u32,
    format: wgpu::TextureFormat,
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: size,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Create a 2D texture suitable for weather data.
pub(crate) fn create_texture_2d(
    device: &wgpu::Device,
    size: u32,
    format: wgpu::TextureFormat,
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
