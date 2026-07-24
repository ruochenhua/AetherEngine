//! GPU procedural noise generation for volumetric clouds.
//!
//! Runs three compute passes to build the noise textures consumed by
//! NadirRoGue/RenderEngine's volumetric cloud pass:
//!   - Perlin-Worley 3D (128³/64³/32³ by quality, RGBA8)
//!   - Worley 3D        (32³/32³/16³ by quality, RGBA8)
//!   - Weather 2D       (2048²/1024²/512² by quality, RGBA8)
//!
//! Note: the compute shaders are externalized via `include_str!` under
//! `assets/shaders/clouds/generation/`. This is the sanctioned exception to
//! the "WGSL inlined in the pass module" convention — this module is an
//! asset-generation tool, not a render pass.

pub(crate) const PERLIN_WORLEY_SHADER: &str = include_str!(
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/shaders/clouds/generation/perlinworley.wgsl"
    )
);

pub(crate) const WORLEY_SHADER: &str = include_str!(
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/shaders/clouds/generation/worley.wgsl"
    )
);

pub(crate) const WEATHER_SHADER: &str = include_str!(
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/shaders/clouds/generation/weather.wgsl"
    )
);

/// GPU textures, sampler, and pre-built bind group for the cloud pass.
pub struct CloudNoiseTextures {
    /// RGBA8 3D Perlin-Worley base-shape noise.
    pub perlinworley_texture: wgpu::Texture,
    /// View of `perlinworley_texture`.
    pub perlinworley_view: wgpu::TextureView,
    /// RGBA8 3D Worley detail noise.
    pub worley_texture: wgpu::Texture,
    /// View of `worley_texture`.
    pub worley_view: wgpu::TextureView,
    /// RGBA8 2D weather coverage map.
    pub weather_texture: wgpu::Texture,
    /// View of `weather_texture`.
    pub weather_view: wgpu::TextureView,
    /// Shared repeat sampler for all cloud noise textures.
    pub sampler: wgpu::Sampler,
    /// Bind group matching the volumetric cloud shader's group 2.
    pub bind_group: wgpu::BindGroup,
}

/// Generate all procedural cloud noise textures on the GPU.
///
/// Texture resolutions scale with `quality`:
/// - High:   Perlin-Worley 128³, Worley 32³, weather 2048²
/// - Medium: Perlin-Worley 64³,  Worley 32³, weather 1024²
/// - Low:    Perlin-Worley 32³,  Worley 16³, weather 512²
pub fn generate_cloud_noise_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    quality: crate::scene::config::CloudQuality,
) -> CloudNoiseTextures {
    let (perlin_size, worley_size, weather_size) = match quality {
        crate::scene::config::CloudQuality::High => (128, 32, 2048),
        crate::scene::config::CloudQuality::Medium => (64, 32, 1024),
        crate::scene::config::CloudQuality::Low => (32, 16, 512),
    };

    let perlinworley = create_storage_texture_3d(device, perlin_size, "Cloud Perlin-Worley");
    let worley = create_storage_texture_3d(device, worley_size, "Cloud Worley");
    let weather = create_storage_texture_2d(device, weather_size, "Cloud Weather");

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Cloud Noise Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        ..Default::default()
    });

    run_compute_pass(
        device,
        queue,
        PERLIN_WORLEY_SHADER,
        &perlinworley.0,
        perlin_size / 4,
        perlin_size / 4,
        perlin_size / 4,
        "Perlin-Worley",
    );
    run_compute_pass(
        device,
        queue,
        WORLEY_SHADER,
        &worley.0,
        worley_size / 4,
        worley_size / 4,
        worley_size / 4,
        "Worley",
    );
    run_compute_pass(
        device,
        queue,
        WEATHER_SHADER,
        &weather.0,
        weather_size / 8,
        weather_size / 8,
        1,
        "Weather",
    );

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Cloud Noise Bind Group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&perlinworley.1),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&worley.1),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&weather.1),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    CloudNoiseTextures {
        perlinworley_texture: perlinworley.0,
        perlinworley_view: perlinworley.1,
        worley_texture: worley.0,
        worley_view: worley.1,
        weather_texture: weather.0,
        weather_view: weather.1,
        sampler,
        bind_group,
    }
}

fn create_storage_texture_3d(
    device: &wgpu::Device,
    size: u32,
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
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_storage_texture_2d(
    device: &wgpu::Device,
    size: u32,
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
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn run_compute_pass(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    shader_src: &str,
    output: &wgpu::Texture,
    groups_x: u32,
    groups_y: u32,
    groups_z: u32,
    label: &str,
) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_src)),
    });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(&format!("{} BGL", label)),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::Rgba8Unorm,
                view_dimension: match output.dimension() {
                    wgpu::TextureDimension::D2 => wgpu::TextureViewDimension::D2,
                    wgpu::TextureDimension::D3 => wgpu::TextureViewDimension::D3,
                    _ => wgpu::TextureViewDimension::D3,
                },
            },
            count: None,
        }],
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{} Pipeline Layout", label)),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&format!("{} Bind Group", label)),
        layout: &bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some(&format!("{} Compute Pass", label)),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label),
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(groups_x, groups_y, groups_z);
    }
    queue.submit(std::iter::once(encoder.finish()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::headless_device_queue;

    /// Run a generation compute shader into a `COPY_SRC`-enabled storage
    /// texture and read the RGBA8 texels back into CPU memory.
    fn run_shader_and_read_back(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shader_src: &str,
        size: wgpu::Extent3d,
        groups: (u32, u32, u32),
        label: &str,
    ) -> Vec<u8> {
        let is_3d = size.depth_or_array_layers > 1;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: if is_3d {
                wgpu::TextureDimension::D3
            } else {
                wgpu::TextureDimension::D2
            },
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        run_compute_pass(
            device, queue, shader_src, &texture, groups.0, groups.1, groups.2, label,
        );

        // copy_texture_to_buffer requires 256-byte-aligned rows; read the
        // padded buffer back and strip the padding per row.
        let row_bytes = size.width as usize * 4;
        let aligned_bpr = (row_bytes as u32).div_ceil(256) * 256;
        let rows_total = (size.height * size.depth_or_array_layers) as u64;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("noise readback"),
            size: aligned_bpr as u64 * rows_total,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("noise readback encoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(aligned_bpr),
                    rows_per_image: Some(size.height),
                },
            },
            size,
        );
        queue.submit(std::iter::once(encoder.finish()));

        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| {
            result.expect("failed to map noise readback buffer");
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        let padded = slice.get_mapped_range();
        let mut data = Vec::with_capacity(row_bytes * rows_total as usize);
        for row in 0..rows_total as usize {
            let start = row * aligned_bpr as usize;
            data.extend_from_slice(&padded[start..start + row_bytes]);
        }
        data
    }

    /// Assert the texels are not degenerate: not all zero (the compute pass
    /// wrote something) and not a single uniform value (it wrote real noise).
    fn assert_non_degenerate(data: &[u8], label: &str) {
        assert!(
            data.iter().any(|&b| b != 0),
            "{label}: texture is entirely zero — compute pass wrote nothing"
        );
        let min = *data.iter().min().unwrap();
        let max = *data.iter().max().unwrap();
        assert!(
            min < max,
            "{label}: texture is a single uniform value ({min}) — no noise variation"
        );
        let distinct: std::collections::HashSet<u8> = data.iter().copied().collect();
        assert!(
            distinct.len() > 8,
            "{label}: only {} distinct texel values, expected varied noise",
            distinct.len()
        );
    }

    #[test]
    fn cloud_noise_perlin_worley_is_non_degenerate() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        // Low-quality resolution: 32³, one workgroup of 4³ per 4 texels.
        let data = run_shader_and_read_back(
            &device,
            &queue,
            PERLIN_WORLEY_SHADER,
            wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 32,
            },
            (8, 8, 8),
            "Perlin-Worley Test",
        );
        assert_eq!(data.len(), 32 * 32 * 32 * 4);
        assert_non_degenerate(&data, "Perlin-Worley");
    }

    #[test]
    fn cloud_noise_worley_is_non_degenerate() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        // Low-quality resolution: 16³.
        let data = run_shader_and_read_back(
            &device,
            &queue,
            WORLEY_SHADER,
            wgpu::Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 16,
            },
            (4, 4, 4),
            "Worley Test",
        );
        assert_eq!(data.len(), 16 * 16 * 16 * 4);
        assert_non_degenerate(&data, "Worley");
    }

    #[test]
    fn cloud_noise_weather_is_non_degenerate() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        // Low-quality resolution: 512², one workgroup of 8² per 8 texels.
        let data = run_shader_and_read_back(
            &device,
            &queue,
            WEATHER_SHADER,
            wgpu::Extent3d {
                width: 512,
                height: 512,
                depth_or_array_layers: 1,
            },
            (64, 64, 1),
            "Weather Test",
        );
        assert_eq!(data.len(), 512 * 512 * 4);
        assert_non_degenerate(&data, "Weather");
    }
}
