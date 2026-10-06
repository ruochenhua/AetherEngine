mod asset;
use std::path::Path;

/// CPU-side texture data.
#[derive(Debug, Clone, PartialEq)]
pub struct CpuTexture {
    /// Raw pixel data.
    pub data: Vec<u8>,
    /// Texture width.
    pub width: u32,
    /// Texture height.
    pub height: u32,
    /// Number of channels.
    pub channels: u32,
    /// Format.
    pub format: TextureFormat,
}

/// Texture format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    /// Single channel 8-bit.
    R8,
    /// Two channel 8-bit.
    Rg8,
    /// Three channel 8-bit (padded to RGBA on GPU).
    Rgb8,
    /// Four channel 8-bit.
    Rgba8,
    /// Four channel 16-bit float.
    Rgba16F,
    /// Four channel 32-bit float.
    Rgba32F,
}

impl CpuTexture {
    /// Load a texture from a file.
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let img = image::open(path)?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();

        Ok(Self {
            data: rgba.into_raw(),
            width,
            height,
            channels: 4,
            format: TextureFormat::Rgba8,
        })
    }

    /// Create a 1x1 solid color texture.
    pub fn from_color(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            data: vec![r, g, b, a],
            width: 1,
            height: 1,
            channels: 4,
            format: TextureFormat::Rgba8,
        }
    }
}

/// GPU texture representation.
#[derive(Debug)]
pub struct GpuTexture {
    /// Texture handle.
    pub texture: wgpu::Texture,
    /// Texture view.
    pub view: wgpu::TextureView,
    /// Sampler.
    pub sampler: wgpu::Sampler,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Format.
    pub format: wgpu::TextureFormat,
}

impl GpuTexture {
    /// Create a GPU texture from CPU data.
    pub fn from_cpu(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cpu: &CpuTexture,
        label: Option<&str>,
    ) -> Self {
        Self::from_cpu_with_srgb(device, queue, cpu, label, false)
    }

    /// Create a GPU texture and select an sRGB format for color slots when supported.
    pub fn from_cpu_with_srgb(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cpu: &CpuTexture,
        label: Option<&str>,
        srgb: bool,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: cpu.width,
            height: cpu.height,
            depth_or_array_layers: 1,
        };

        let format = match (cpu.format, srgb) {
            (TextureFormat::R8, _) => wgpu::TextureFormat::R8Unorm,
            (TextureFormat::Rg8, _) => wgpu::TextureFormat::Rg8Unorm,
            (TextureFormat::Rgb8, true) => wgpu::TextureFormat::Rgba8UnormSrgb,
            (TextureFormat::Rgb8, false) => wgpu::TextureFormat::Rgba8Unorm,
            (TextureFormat::Rgba8, true) => wgpu::TextureFormat::Rgba8UnormSrgb,
            (TextureFormat::Rgba8, false) => wgpu::TextureFormat::Rgba8Unorm,
            (TextureFormat::Rgba16F, _) => wgpu::TextureFormat::Rgba16Float,
            (TextureFormat::Rgba32F, _) => wgpu::TextureFormat::Rgba32Float,
        };
        let padded_rgb = (cpu.format == TextureFormat::Rgb8).then(|| {
            let mut rgba = Vec::with_capacity((cpu.width * cpu.height * 4) as usize);
            for pixel in cpu.data.chunks_exact(3) {
                rgba.extend_from_slice(pixel);
                rgba.push(255);
            }
            rgba
        });
        let upload_data = padded_rgb.as_deref().unwrap_or(&cpu.data);
        let upload_channels = if padded_rgb.is_some() {
            4
        } else {
            cpu.channels
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            upload_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(cpu.width * upload_channels),
                rows_per_image: Some(cpu.height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            width: cpu.width,
            height: cpu.height,
            format,
        }
    }
}
