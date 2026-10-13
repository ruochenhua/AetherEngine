//! Bound CPU work before accepting a synchronous host-side image drop.
use std::path::Path;

pub(super) fn validate_image(
    path: &Path,
) -> anyhow::Result<aether_engine::asset::texture::CpuTexture> {
    let (width, height) = image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_dimensions()?;
    anyhow::ensure!(
        width > 0
            && height > 0
            && width <= 8192
            && height <= 8192
            && u64::from(width) * u64::from(height) <= 16 * 1024 * 1024,
        "image dimensions exceed the limit (8192 per side, 16 Mi pixels)"
    );
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader.decode()?.to_rgba8();
    Ok(aether_engine::asset::texture::CpuTexture {
        width: rgba.width(),
        height: rgba.height(),
        channels: 4,
        data: rgba.into_raw(),
        format: aether_engine::asset::texture::TextureFormat::Rgba8,
    })
}
