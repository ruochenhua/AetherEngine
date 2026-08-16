// HDR environment texture loading for IBL.

use std::path::Path;

use super::config::IblConfig;

pub(super) fn load_hdr_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    config: &IblConfig,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler) {
    let (w, h, rgba) = if config.debug_checkerboard {
        // Generate a 16×8 magenta/cyan checkerboard for debugging
        let w = 16u32;
        let h = 8u32;
        let mut data: Vec<u8> = Vec::with_capacity((w * h * 8) as usize); // Rgba16Float = 8 bytes/pixel
        for y in 0..h {
            for x in 0..w {
                let is_magenta = ((x / 2) + (y / 2)) % 2 == 0;
                let (r, g, b) = if is_magenta {
                    (1.0f32, 0.0, 1.0)
                } else {
                    (0.0f32, 1.0, 1.0)
                };
                for c in [r, g, b, 1.0f32] {
                    data.extend_from_slice(&half::f16::from_f32(c).to_bits().to_le_bytes());
                }
            }
        }
        (w, h, data)
    } else {
        let path = config
            .environment_path
            .as_deref()
            .unwrap_or("assets/hdr/newport_loft.hdr");
        let img = image::open(Path::new(path))
            .unwrap_or_else(|e| panic!("Failed to load HDR '{}': {}", path, e))
            .to_rgb32f();

        let (iw, ih) = (img.width(), img.height());
        let mut data2: Vec<u8> = Vec::with_capacity((iw * ih * 8) as usize);
        for p in img.pixels() {
            for c in 0..3 {
                data2.extend_from_slice(&half::f16::from_f32(p.0[c]).to_bits().to_le_bytes());
            }
            data2.extend_from_slice(&half::f16::from_f32(1.0).to_bits().to_le_bytes());
        }
        (iw, ih, data2)
    };

    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("HDR"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8 * w), // Rgba16Float = 8 bytes/pixel
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );

    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("HDR Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (tex, view, sampler)
}
