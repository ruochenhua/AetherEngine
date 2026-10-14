use crate::project_index::ResourceKind;
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Thumbnail {
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
    pub dimensions: String,
}

#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub file_bytes: u64,
    pub image_edge: u32,
    pub image_pixels: u64,
    pub vertices: usize,
    pub triangles: usize,
    pub thumbnail_edge: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            file_bytes: 16 * 1024 * 1024,
            image_edge: 8192,
            image_pixels: 16 * 1024 * 1024,
            vertices: 100_000,
            triangles: 20_000,
            thumbnail_edge: 384,
        }
    }
}

pub(crate) fn load(
    root: &Path,
    path: &str,
    kind: ResourceKind,
    limits: Limits,
) -> Result<Thumbnail, String> {
    let path = files::contained(root, Path::new(path))?;
    let bytes = files::read_bounded(&path, limits.file_bytes)?;
    match kind {
        ResourceKind::Image => decode_image(&bytes, limits),
        ResourceKind::Obj => obj::load(root, &path, &bytes, limits),
        ResourceKind::Gltf => gltf_mesh::load(root, &path, &bytes, limits),
        _ => Err("This resource type has no thumbnail".into()),
    }
}

fn decode_image(bytes: &[u8], limits: Limits) -> Result<Thumbnail, String> {
    use std::io::Cursor;
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("Unreadable image: {e}"))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|e| format!("Malformed image: {e}"))?;
    if width == 0
        || height == 0
        || width > limits.image_edge
        || height > limits.image_edge
        || u64::from(width) * u64::from(height) > limits.image_pixels
    {
        return Err("Image dimension or pixel limit exceeded".into());
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("Unreadable image: {e}"))?;
    let mut allocation = image::Limits::default();
    allocation.max_image_width = Some(limits.image_edge);
    allocation.max_image_height = Some(limits.image_edge);
    allocation.max_alloc = Some(limits.image_pixels.saturating_mul(16));
    reader.limits(allocation);
    let decoded = reader
        .decode()
        .map_err(|e| format!("Malformed image: {e}"))?;
    let thumbnail = decoded
        .thumbnail(limits.thumbnail_edge, limits.thumbnail_edge)
        .to_rgba8();
    Ok(Thumbnail {
        size: [thumbnail.width() as usize, thumbnail.height() as usize],
        rgba: thumbnail.into_raw(),
        dimensions: format!("Image: {width} × {height} px"),
    })
}

mod cache;
mod files;
mod gltf_mesh;
mod mesh;
mod obj;
mod worker;
pub(crate) use cache::PreviewState;
pub(crate) use worker::PreviewWorker;
#[cfg(test)]
#[path = "project_preview/tests.rs"]
mod tests;
