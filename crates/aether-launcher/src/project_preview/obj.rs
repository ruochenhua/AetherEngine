use super::{files, mesh, Limits, Thumbnail};
use std::path::Path;

pub(super) fn load(
    root: &Path,
    path: &Path,
    bytes: &[u8],
    limits: Limits,
) -> Result<Thumbnail, String> {
    let source = std::str::from_utf8(bytes).map_err(|_| "Malformed OBJ text")?;
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for line in source.lines() {
        let mut words = line
            .split('#')
            .next()
            .unwrap_or_default()
            .split_whitespace();
        match words.next() {
            Some("v") => {
                if vertices.len() >= limits.vertices {
                    return Err("Mesh vertex limit exceeded".into());
                }
                let mut position = [0.; 3];
                for coordinate in &mut position {
                    *coordinate = words
                        .next()
                        .ok_or("Malformed OBJ vertex")?
                        .parse::<f32>()
                        .map_err(|_| "Malformed OBJ vertex")?;
                }
                if position.iter().any(|p| !p.is_finite()) {
                    return Err("OBJ vertex must be finite".into());
                }
                vertices.push(position);
            }
            Some("f") => {
                let mut face = Vec::new();
                for word in words {
                    if face.len() > limits.triangles.saturating_add(2) {
                        return Err("Generated triangle limit exceeded".into());
                    }
                    let value = word
                        .split('/')
                        .next()
                        .ok_or("Malformed OBJ index")?
                        .parse::<i64>()
                        .map_err(|_| "Malformed OBJ index")?;
                    let index = if value < 0 {
                        (vertices.len() as i64).checked_add(value)
                    } else {
                        value.checked_sub(1)
                    }
                    .ok_or("Malformed OBJ index")?;
                    if index < 0 || index as usize >= vertices.len() {
                        return Err("Malformed OBJ index".into());
                    }
                    face.push(index as usize);
                }
                if face.len() < 3 {
                    return Err("Malformed OBJ face".into());
                }
                if triangles.len().saturating_add(face.len() - 2) > limits.triangles {
                    return Err("Generated triangle limit exceeded".into());
                }
                for i in 1..face.len() - 1 {
                    triangles.push([face[0], face[i], face[i + 1]]);
                }
            }
            Some("mtllib") => {
                for dependency in words {
                    let path = files::dependency(root, path, dependency)?;
                    if std::fs::metadata(path)
                        .map_err(|e| format!("Unreadable dependency: {e}"))?
                        .len()
                        > limits.file_bytes
                    {
                        return Err("Dependency file limit exceeded".into());
                    }
                }
            }
            _ => {}
        }
    }
    mesh::project(&vertices, &triangles, limits)
}
