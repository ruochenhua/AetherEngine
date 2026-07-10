//! GPU procedural noise generation for volumetric clouds.
//!
//! Runs three compute passes to build the noise textures consumed by the
//! volumetric cloud pass:
//!   - Perlin-Worley 3D (128^3, RGBA8)
//!   - Worley 3D        (128^3, RGBA8)
//!   - Weather 2D       (2048^2, RGBA8)

const PERLIN_WORLEY_SHADER: &str = include_str!(
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/shaders/clouds/generation/perlinworley.wgsl"
    )
);

const WORLEY_SHADER: &str = include_str!(
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/shaders/clouds/generation/worley.wgsl"
    )
);

const WEATHER_SHADER: &str = include_str!(
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
/// The `quality` parameter is accepted for API compatibility but the Phase 3
/// reconstruction uses fixed-resolution RGBA8 textures (128^3 for 3D noise,
/// 2048^2 for weather) independent of quality.
pub fn generate_cloud_noise_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    _quality: crate::scene::config::CloudQuality,
) -> CloudNoiseTextures {
    let noise_size = 128u32;
    let weather_size = 2048u32;

    let perlinworley = create_storage_texture_3d(device, noise_size, "Cloud Perlin-Worley");
    let worley = create_storage_texture_3d(device, noise_size, "Cloud Worley");
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
        noise_size / 4,
        noise_size / 4,
        noise_size / 4,
        "Perlin-Worley",
    );
    run_compute_pass(
        device,
        queue,
        WORLEY_SHADER,
        &worley.0,
        noise_size / 4,
        noise_size / 4,
        noise_size / 4,
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
