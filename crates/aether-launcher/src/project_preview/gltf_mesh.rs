use super::{files, mesh, Limits, Thumbnail};
use glam::Mat4;
use std::path::Path;

pub(super) fn load(
    root: &Path,
    path: &Path,
    bytes: &[u8],
    limits: Limits,
) -> Result<Thumbnail, String> {
    let asset = gltf::Gltf::from_slice(bytes).map_err(|e| format!("Malformed glTF: {e}"))?;
    let mut total_bytes = bytes.len() as u64;
    let mut buffers = Vec::new();
    for buffer in asset.buffers() {
        if buffer.length() as u64 > limits.file_bytes {
            return Err("glTF buffer limit exceeded".into());
        }
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => asset
                .blob
                .as_ref()
                .ok_or("Missing glTF binary buffer")?
                .clone(),
            gltf::buffer::Source::Uri(uri) => {
                if uri.starts_with("data:") {
                    embedded(uri, limits.file_bytes)?
                } else {
                    files::read_bounded(&files::dependency(root, path, uri)?, limits.file_bytes)?
                }
            }
        };
        total_bytes = total_bytes.saturating_add(data.len() as u64);
        if total_bytes > limits.file_bytes {
            return Err("glTF total input limit exceeded".into());
        }
        if data.len() < buffer.length() {
            return Err("Malformed glTF buffer length".into());
        }
        buffers.push(data);
    }
    // Materials are not rendered, but every dependency still passes containment policy.
    for image in asset.images() {
        if let gltf::image::Source::Uri { uri, .. } = image.source() {
            let size = if uri.starts_with("data:") {
                embedded(uri, limits.file_bytes)?.len() as u64
            } else {
                std::fs::metadata(files::dependency(root, path, uri)?)
                    .map_err(|e| format!("Unreadable dependency: {e}"))?
                    .len()
            };
            total_bytes = total_bytes.saturating_add(size);
            if total_bytes > limits.file_bytes {
                return Err("glTF total dependency limit exceeded".into());
            }
        }
    }
    validate_accessors(&asset, &buffers, limits)?;
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    let scene = asset
        .default_scene()
        .or_else(|| asset.scenes().next())
        .ok_or("glTF has no scene")?;
    let mut pending = scene
        .nodes()
        .map(|node| (node, Mat4::IDENTITY, 0))
        .collect::<Vec<_>>();
    let mut visited = 0usize;
    while let Some((node, parent, depth)) = pending.pop() {
        visited += 1;
        if depth > 64 || visited > limits.vertices {
            return Err("glTF node traversal limit exceeded".into());
        }
        let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if !transform.is_finite() {
            return Err("glTF transform must be finite".into());
        }
        if let Some(mesh) = node.mesh() {
            append_mesh(
                mesh,
                &buffers,
                transform,
                &mut positions,
                &mut triangles,
                limits,
            )?;
        }
        for child in node.children() {
            if pending.len() >= limits.vertices {
                return Err("glTF node traversal limit exceeded".into());
            }
            pending.push((child, transform, depth + 1));
        }
    }
    mesh::project(&positions, &triangles, limits)
}

fn embedded(uri: &str, limit: u64) -> Result<Vec<u8>, String> {
    let (header, encoded) = uri.split_once(',').ok_or("Malformed embedded dependency")?;
    if !header.ends_with(";base64") {
        return Err("Unsupported embedded dependency encoding".into());
    }
    if encoded.len() as u64 > limit.saturating_add(2).saturating_mul(4) / 3 + 4 {
        return Err("Embedded dependency limit exceeded".into());
    }
    let bytes = base64::decode(encoded).map_err(|_| "Malformed embedded dependency")?;
    if bytes.len() as u64 > limit {
        return Err("Embedded dependency limit exceeded".into());
    }
    Ok(bytes)
}

fn validate_accessors(
    asset: &gltf::Gltf,
    buffers: &[Vec<u8>],
    limits: Limits,
) -> Result<(), String> {
    for view in asset.views() {
        let buffer = buffers
            .get(view.buffer().index())
            .ok_or("Malformed glTF buffer index")?;
        let end = view
            .offset()
            .checked_add(view.length())
            .ok_or("Malformed glTF buffer view")?;
        if end > buffer.len() {
            return Err("Malformed glTF buffer view range".into());
        }
    }
    for accessor in asset.accessors() {
        if accessor.count() > limits.vertices.max(limits.triangles.saturating_mul(3)) {
            return Err("glTF accessor count limit exceeded".into());
        }
        if accessor.sparse().is_some() {
            return Err("Sparse glTF accessors are unsupported for previews".into());
        }
        let view = accessor.view().ok_or("Missing glTF accessor view")?;
        let stride = view.stride().unwrap_or(accessor.size());
        let last = accessor
            .count()
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|size| size.checked_add(accessor.offset()))
            .and_then(|size| size.checked_add(accessor.size()))
            .ok_or("Malformed glTF accessor range")?;
        if accessor.count() == 0 || stride < accessor.size() || last > view.length() {
            return Err("Malformed glTF accessor range".into());
        }
    }
    Ok(())
}

fn append_mesh(
    mesh: gltf::Mesh<'_>,
    buffers: &[Vec<u8>],
    transform: Mat4,
    positions: &mut Vec<[f32; 3]>,
    triangles: &mut Vec<[usize; 3]>,
    limits: Limits,
) -> Result<(), String> {
    for primitive in mesh.primitives() {
        if primitive.mode() != gltf::mesh::Mode::Triangles {
            return Err("Only triangle glTF primitives have previews".into());
        }
        let accessor = primitive
            .get(&gltf::Semantic::Positions)
            .ok_or("Missing glTF positions")?;
        if positions.len().saturating_add(accessor.count()) > limits.vertices {
            return Err("Mesh vertex limit exceeded".into());
        }
        let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
        let points = reader.read_positions().ok_or("Malformed glTF positions")?;
        let base = positions.len();
        for point in points {
            let point = transform.transform_point3(glam::Vec3::from_array(point));
            if !point.is_finite() {
                return Err("glTF points must be finite".into());
            }
            positions.push(point.to_array());
        }
        let count = primitive
            .indices()
            .map_or(accessor.count(), |indices| indices.count());
        if count % 3 != 0 {
            return Err("Malformed glTF triangle index count".into());
        }
        if triangles.len().saturating_add(count / 3) > limits.triangles {
            return Err("Generated triangle limit exceeded".into());
        }
        let indices: Vec<usize> = if primitive.indices().is_some() {
            reader
                .read_indices()
                .ok_or("Malformed glTF indices")?
                .into_u32()
                .map(|i| i as usize)
                .collect()
        } else {
            (0..accessor.count()).collect()
        };
        if indices.iter().any(|&i| i >= accessor.count()) {
            return Err("Malformed glTF index".into());
        }
        for triangle in indices.chunks_exact(3) {
            triangles.push([base + triangle[0], base + triangle[1], base + triangle[2]]);
        }
    }
    Ok(())
}
