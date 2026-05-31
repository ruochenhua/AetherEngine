//! Image-Based Lighting loader.
//!
//! Loads an HDR equirectangular environment map and generates:
//! - Diffuse irradiance cubemap (convolved with cosine lobe)
//! - Pre-filtered specular cubemap (mipmapped, roughness levels)
//! - BRDF integration LUT (2D texture)

/// Configuration for IBL precomputation.
pub struct IblConfig {
    /// Size of irradiance cubemap per face (default: 32).
    pub irradiance_size: u32,
    /// Size of prefiltered cubemap base mip per face (default: 128).
    pub prefilter_size: u32,
    /// Number of mip levels for prefiltered cubemap (default: 5, max roughness 0.5 at mip 4).
    pub prefilter_mips: u32,
    /// Size of BRDF LUT (default: 256).
    pub brdf_lut_size: u32,
    /// Path to HDR environment map.
    pub environment_path: Option<String>,
}

impl Default for IblConfig {
    fn default() -> Self {
        Self {
            irradiance_size: 32,
            prefilter_size: 128,
            prefilter_mips: 5,
            brdf_lut_size: 256,
            environment_path: None,
        }
    }
}

/// Precomputed IBL resources.
pub struct IblResources {
    /// Irradiance cubemap view.
    pub irradiance_view: wgpu::TextureView,
    /// Prefiltered specular cubemap view.
    pub prefiltered_view: wgpu::TextureView,
    /// BRDF integration LUT view.
    pub brdf_lut_view: wgpu::TextureView,

    /// Irradiance cubemap sampler.
    pub irradiance_sampler: wgpu::Sampler,
    /// Prefiltered cubemap sampler (with mipmapping, trilinear).
    pub prefiltered_sampler: wgpu::Sampler,
    /// BRDF LUT sampler (clamp to edge).
    pub brdf_lut_sampler: wgpu::Sampler,

    _irradiance_texture: wgpu::Texture,
    _prefiltered_texture: wgpu::Texture,
    _brdf_lut_texture: wgpu::Texture,
}

impl IblResources {
    /// Generate IBL resources from an equirectangular HDR image.
    ///
    /// Currently uses a placeholder white environment map.
    pub fn generate(device: &wgpu::Device, queue: &wgpu::Queue, config: &IblConfig) -> Self {
        // For now, create placeholder textures (solid white for diffuse, solid black for specular).
        // Compute shader generation will come in the GREEN phase.

        let irradiance_tex = create_cubemap(
            device,
            config.irradiance_size,
            1,
            wgpu::TextureFormat::Rgba16Float,
            Some("IBL Irradiance"),
        );

        let prefiltered_tex = create_cubemap(
            device,
            config.prefilter_size,
            config.prefilter_mips,
            wgpu::TextureFormat::Rgba16Float,
            Some("IBL Prefiltered"),
        );

        let brdf_lut_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("BRDF LUT"),
            size: wgpu::Extent3d {
                width: config.brdf_lut_size,
                height: config.brdf_lut_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let irradiance_view = irradiance_tex.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Irradiance View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        let prefiltered_view = prefiltered_tex.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Prefiltered View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        let brdf_lut_view = brdf_lut_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let irradiance_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Irradiance Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let prefiltered_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Prefiltered Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let brdf_lut_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("BRDF LUT Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let _ = queue; // Used by compute dispatch in GREEN phase

        Self {
            irradiance_view,
            prefiltered_view,
            brdf_lut_view,
            irradiance_sampler,
            prefiltered_sampler,
            brdf_lut_sampler,
            _irradiance_texture: irradiance_tex,
            _prefiltered_texture: prefiltered_tex,
            _brdf_lut_texture: brdf_lut_tex,
        }
    }
}

/// Create a cubemap texture with mip levels.
fn create_cubemap(
    device: &wgpu::Device,
    size: u32,
    mip_levels: u32,
    format: wgpu::TextureFormat,
    label: Option<&str>,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label,
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        mip_level_count: mip_levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless_device_and_queue() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        )
        .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
            .expect("need device")
    }

    #[test]
    fn ibl_resources_created_with_correct_sizes() {
        let (device, queue) = headless_device_and_queue();
        let config = IblConfig::default();
        let ibl = IblResources::generate(&device, &queue, &config);

        // Irradiance cubemap: 32×32 × 6 faces
        assert_eq!(ibl._irradiance_texture.size().width, 32);
        assert_eq!(ibl._irradiance_texture.size().height, 32);
        assert_eq!(ibl._irradiance_texture.depth_or_array_layers(), 6);
        assert_eq!(ibl._irradiance_texture.mip_level_count(), 1);

        // Prefiltered cubemap: 128×128 × 6 faces, 5 mips
        assert_eq!(ibl._prefiltered_texture.size().width, 128);
        assert_eq!(ibl._prefiltered_texture.size().height, 128);
        assert_eq!(ibl._prefiltered_texture.depth_or_array_layers(), 6);
        assert_eq!(ibl._prefiltered_texture.mip_level_count(), 5);

        // BRDF LUT: 256×256 2D
        assert_eq!(ibl._brdf_lut_texture.size().width, 256);
        assert_eq!(ibl._brdf_lut_texture.size().height, 256);
        assert_eq!(ibl._brdf_lut_texture.depth_or_array_layers(), 1);
    }

    #[test]
    fn ibl_texture_formats_are_correct() {
        let (device, queue) = headless_device_and_queue();
        let config = IblConfig::default();
        let ibl = IblResources::generate(&device, &queue, &config);

        assert_eq!(ibl._irradiance_texture.format(), wgpu::TextureFormat::Rgba16Float);
        assert_eq!(ibl._prefiltered_texture.format(), wgpu::TextureFormat::Rgba16Float);
        assert_eq!(ibl._brdf_lut_texture.format(), wgpu::TextureFormat::Rgba16Float);
    }
}
