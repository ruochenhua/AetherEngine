use super::{FileOperationError, BYTE_LIMIT};
use std::path::Path;
fn invalid(message: impl ToString) -> FileOperationError {
    FileOperationError::Validation(message.to_string())
}

pub(super) fn bytes(bytes: &[u8], extension: &str) -> Result<(), FileOperationError> {
    if extension == "glb" {
        return glb(bytes);
    }
    let expected = if extension == "png" {
        image::ImageFormat::Png
    } else {
        image::ImageFormat::Jpeg
    };
    if image::guess_format(bytes).map_err(invalid)? != expected {
        return Err(invalid("Image content does not match its extension"));
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), expected);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(BYTE_LIMIT);
    reader.limits(limits);
    reader.decode().map_err(invalid)?;
    Ok(())
}
fn glb(bytes: &[u8]) -> Result<(), FileOperationError> {
    if bytes.len() < 20
        || &bytes[..4] != b"glTF"
        || bytes[4..8] != 2u32.to_le_bytes()
        || bytes[8..12] != (bytes.len() as u32).to_le_bytes()
    {
        return Err(invalid("Invalid GLB magic, version, or length"));
    }
    let mut offset = 12;
    let mut json = None;
    let mut chunks = 0;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or_else(|| invalid("Truncated GLB chunk header"))?;
        let length = u32::from_le_bytes(header[..4].try_into().map_err(invalid)?) as usize;
        if length % 4 != 0 {
            return Err(invalid("Unaligned GLB chunk"));
        }
        let end = offset
            .checked_add(8)
            .and_then(|o| o.checked_add(length))
            .ok_or_else(|| invalid("GLB chunk overflow"))?;
        let chunk = bytes
            .get(offset + 8..end)
            .ok_or_else(|| invalid("Truncated GLB chunk"))?;
        match (chunks, &header[4..8]) {
            (0, b"JSON") => {
                json = Some(serde_json::from_slice::<serde_json::Value>(chunk).map_err(invalid)?)
            }
            (1, b"BIN\0") => (),
            _ => return Err(invalid("Invalid GLB chunk sequence")),
        }
        offset = end;
        chunks += 1;
    }
    let json = json.ok_or_else(|| invalid("Missing GLB JSON"))?;
    for field in ["buffers", "images"] {
        if let Some(entries) = json.get(field).and_then(|v| v.as_array()) {
            for entry in entries {
                if let Some(uri) = entry.get("uri").and_then(|v| v.as_str()) {
                    if !uri.starts_with("data:") {
                        return Err(FileOperationError::ExternalDependency(uri.into()));
                    }
                    let (header, data) = uri
                        .split_once(',')
                        .ok_or_else(|| invalid("Malformed data URI"))?;
                    if !header.ends_with(";base64") {
                        return Err(invalid("Only base64 data URIs are supported"));
                    }
                    if data.len() as u64 > BYTE_LIMIT {
                        return Err(invalid("Data URI exceeds byte budget"));
                    }
                    let decoded = base64::decode(data).map_err(invalid)?;
                    if decoded.len() as u64 > BYTE_LIMIT {
                        return Err(invalid("Data URI exceeds byte budget"));
                    }
                }
            }
        }
    }
    gltf::Gltf::from_slice(bytes).map_err(invalid)?;
    Ok(())
}
pub(super) fn engine_mesh(path: &Path) -> Result<(), FileOperationError> {
    // Untrusted accessors can panic in mesh conversion; keep that failure at the import boundary.
    std::panic::catch_unwind(|| aether_engine::asset::loaders::gltf::load(path))
        .map_err(|_| invalid("Engine mesh loader rejected malformed mesh data"))?
        .map_err(invalid)?;
    Ok(())
}
