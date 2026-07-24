//! Precomputed IBL resources.

use super::config::IblConfig;
use super::generate::{create_cubemap, load_hdr_texture, CpuCubemap, CubeMesh};

/// Precomputed IBL resources.
pub struct IblResources {
    /// Full-resolution environment cubemap (512×512, 1 mip, Rgba16Float). Used for skybox.
    pub env_view: wgpu::TextureView,
    /// Diffuse irradiance cubemap (32×32, Rgba16Float).
    pub irradiance_view: wgpu::TextureView,
    /// Prefiltered specular cubemap (128×128, 5 mips, Rgba16Float).
    pub prefiltered_view: wgpu::TextureView,
    /// BRDF integration LUT (256×256, Rgba16Float, RG channels).
    pub brdf_lut_view: wgpu::TextureView,
    /// Shared sampler (trilinear, clamp-to-edge) for all IBL textures.
    pub ibl_sampler: wgpu::Sampler,
    _env_texture: wgpu::Texture,
    _irradiance_texture: wgpu::Texture,
    _prefiltered_texture: wgpu::Texture,
    _brdf_lut_texture: wgpu::Texture,
}

impl IblResources {
    /// Debug: get the raw irradiance texture (for direct write testing).
    #[doc(hidden)]
    pub fn irradiance_texture(&self) -> &wgpu::Texture {
        &self._irradiance_texture
    }
    /// Debug: get the raw prefiltered texture.
    #[doc(hidden)]
    pub fn prefiltered_texture(&self) -> &wgpu::Texture {
        &self._prefiltered_texture
    }
    /// Debug: get the raw BRDF LUT texture.
    #[doc(hidden)]
    pub fn brdf_lut_texture(&self) -> &wgpu::Texture {
        &self._brdf_lut_texture
    }
    /// Generate all IBL resources. Pass `None` for queue in tests.
    pub fn generate(
        device: &wgpu::Device,
        queue: Option<&wgpu::Queue>,
        config: &IblConfig,
    ) -> Self {
        let ibl_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("IBL Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let env_tex = create_cubemap(device, config.env_size, 1, "Env");
        let irradiance_tex = create_cubemap(device, config.irradiance_size, 1, "Irr");
        let prefiltered_tex =
            create_cubemap(device, config.prefilter_size, config.prefilter_mips, "Pref");
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
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });

        let env_view = env_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let irradiance_view = irradiance_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let prefiltered_view = prefiltered_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let brdf_lut_view = brdf_lut_tex.create_view(&wgpu::TextureViewDescriptor::default());

        if let Some(queue) = queue {
            let (hdr_tex, hdr_view, hdr_sampler) = load_hdr_texture(device, queue, config);
            let cube_mesh = CubeMesh::new(device);

            // 1. Equirect → Cubemap
            CpuCubemap::equirect_to_cubemap(
                device,
                queue,
                &hdr_view,
                &hdr_sampler,
                &env_tex,
                config.env_size,
                &cube_mesh,
            );

            // 2. Irradiance convolution
            CpuCubemap::irradiance_convolution(
                device,
                queue,
                &env_view,
                &irradiance_tex,
                config.irradiance_size,
                &cube_mesh,
            );

            // 3. Prefilter (one pass per mip)
            CpuCubemap::prefiltration(
                device,
                queue,
                &env_view,
                &prefiltered_tex,
                config.prefilter_size,
                config.prefilter_mips,
                &cube_mesh,
            );

            // 4. BRDF LUT (compute)
            CpuCubemap::brdf_integration(device, queue, &brdf_lut_tex, config.brdf_lut_size);

            drop((hdr_tex, hdr_view, hdr_sampler));
        }

        Self {
            env_view,
            irradiance_view,
            prefiltered_view,
            brdf_lut_view,
            ibl_sampler,
            _env_texture: env_tex,
            _irradiance_texture: irradiance_tex,
            _prefiltered_texture: prefiltered_tex,
            _brdf_lut_texture: brdf_lut_tex,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::IblConfig;
    use super::IblResources;
    use crate::test_utils::headless_device_queue;

    #[test]
    fn ibl_resources_created_with_correct_sizes() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let config = IblConfig::default();
        let ibl = IblResources::generate(&device, None, &config);
        assert_eq!(ibl._irradiance_texture.size().width, 32);
        assert_eq!(ibl._irradiance_texture.depth_or_array_layers(), 6);
        assert_eq!(ibl._prefiltered_texture.mip_level_count(), 5);
        assert_eq!(ibl._brdf_lut_texture.size().width, 256);
    }

    #[test]
    fn ibl_texture_formats_are_correct() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let config = IblConfig::default();
        let ibl = IblResources::generate(&device, None, &config);
        assert_eq!(
            ibl._irradiance_texture.format(),
            wgpu::TextureFormat::Rgba16Float
        );
        assert_eq!(
            ibl._prefiltered_texture.format(),
            wgpu::TextureFormat::Rgba16Float
        );
        assert_eq!(
            ibl._brdf_lut_texture.format(),
            wgpu::TextureFormat::Rgba16Float
        );
    }
}
