//! File format loaders for external 3D models.
//!
//! This module isolates model parsing (OBJ, glTF) from the engine's main asset
//! flow. Each format lives in its own submodule and exposes a single `load`
//! function that converts a file path into a [`CpuMesh`].
//!
//! [`CpuMesh`]: crate::asset::mesh::CpuMesh

pub mod gltf;
pub mod obj;

use crate::asset::mesh::CpuMesh;
use std::path::Path;

/// Load a mesh from a file by dispatching on its extension.
///
/// Supported extensions:
/// - `.obj`  → Wavefront OBJ (via `tobj`)
/// - `.gltf` / `.glb` → glTF 2.0 (via `gltf`)
pub fn load_mesh(path: &Path) -> anyhow::Result<CpuMesh> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "obj" => obj::load(path),
        "gltf" | "glb" => gltf::load(path),
        _ => anyhow::bail!("Unsupported mesh format: {}", ext),
    }
}

// --- shared geometry helpers ------------------------------------------------

use glam::{Vec2, Vec3};

/// Compute smooth per-vertex normals by averaging face normals.
pub(crate) fn compute_smooth_normals(
    positions: &[[f32; 3]],
    indices: &[u32],
    normals: &mut Vec<[f32; 3]>,
) {
    normals.clear();
    normals.resize(positions.len(), [0.0, 0.0, 0.0]);

    let mut accum = vec![Vec3::ZERO; positions.len()];

    for tri in indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        let p0 = Vec3::from_array(positions[i0]);
        let p1 = Vec3::from_array(positions[i1]);
        let p2 = Vec3::from_array(positions[i2]);

        let face_normal = (p1 - p0).cross(p2 - p0);
        accum[i0] += face_normal;
        accum[i1] += face_normal;
        accum[i2] += face_normal;
    }

    for (i, acc) in accum.iter().enumerate() {
        let n = acc.normalize_or_zero();
        normals[i] = if n.length_squared() > 0.0 {
            n.to_array()
        } else {
            // Degenerate geometry: fall back to a predictable up vector.
            [0.0, 1.0, 0.0]
        };
    }
}

/// Fill missing UV coordinates with `[0, 0]`.
pub(crate) fn fill_missing_uvs(uvs: &mut Vec<[f32; 2]>, count: usize) {
    if uvs.len() < count {
        uvs.resize(count, [0.0, 0.0]);
    }
}

/// Compute per-vertex tangents using the standard Mikktspace-style algorithm.
///
/// Returns `(tangent_x, tangent_y, tangent_z, handedness)` for each vertex.
pub(crate) fn compute_tangents(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
) -> Vec<[f32; 4]> {
    let vertex_count = positions.len();
    let mut tangents = vec![Vec3::ZERO; vertex_count];
    let mut bitangents = vec![Vec3::ZERO; vertex_count];

    for tri in indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        let p0 = Vec3::from_array(positions[i0]);
        let p1 = Vec3::from_array(positions[i1]);
        let p2 = Vec3::from_array(positions[i2]);

        let uv0 = Vec2::from_array(uvs[i0]);
        let uv1 = Vec2::from_array(uvs[i1]);
        let uv2 = Vec2::from_array(uvs[i2]);

        let edge1 = p1 - p0;
        let edge2 = p2 - p0;

        let duv1 = uv1 - uv0;
        let duv2 = uv2 - uv0;

        let denom = duv1.x * duv2.y - duv2.x * duv1.y;
        if denom.abs() < 1e-8 {
            // Degenerate UVs; skip this triangle rather than producing NaNs.
            continue;
        }
        let r = 1.0 / denom;

        let tangent = (edge1 * duv2.y - edge2 * duv1.y) * r;
        let bitangent = (edge2 * duv1.x - edge1 * duv2.x) * r;

        tangents[i0] += tangent;
        tangents[i1] += tangent;
        tangents[i2] += tangent;
        bitangents[i0] += bitangent;
        bitangents[i1] += bitangent;
        bitangents[i2] += bitangent;
    }

    let mut result = Vec::with_capacity(vertex_count);
    for i in 0..vertex_count {
        let n = Vec3::from_array(normals[i]);
        let t = tangents[i];
        let b = bitangents[i];

        // Gram-Schmidt orthogonalize against the normal.
        let mut tangent = (t - n * n.dot(t)).normalize_or_zero();
        if tangent.length_squared() == 0.0 {
            // If the tangent collapsed, pick any vector orthogonal to the normal.
            let fallback = if n.abs_diff_eq(Vec3::Y, 1e-4) {
                Vec3::X
            } else {
                Vec3::Y
            };
            tangent = (fallback - n * n.dot(fallback)).normalize();
        }

        // Recompute handedness from the accumulated bitangent.
        let handedness = if n.cross(tangent).dot(b) < 0.0 {
            -1.0
        } else {
            1.0
        };
        result.push([tangent.x, tangent.y, tangent.z, handedness]);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_normals_for_quad_face_up() {
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        // Counter-clockwise winding when viewed from +Y so the normal faces up.
        let indices = vec![0, 2, 1, 0, 3, 2];
        let mut normals = vec![[0.0, 0.0, 0.0]; 4];

        compute_smooth_normals(&positions, &indices, &mut normals);

        for n in &normals {
            assert!(n[1] > 0.9, "quad should face +Y, got {:?}", n);
        }
    }

    #[test]
    fn tangents_for_simple_plane_point_along_u() {
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        let normals = [[0.0, 1.0, 0.0]; 4];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let indices = vec![0, 2, 1, 0, 3, 2];

        let tangents = compute_tangents(&positions, &normals, &uvs, &indices);

        for t in &tangents {
            assert!(t[0] > 0.9, "tangent should point +X, got {:?}", t);
            assert!((t[3].abs() - 1.0).abs() < 1e-4, "handedness must be +/-1");
        }
    }

    #[test]
    fn fill_uvs_pads_to_requested_count() {
        let mut uvs = vec![[0.5, 0.5]];
        fill_missing_uvs(&mut uvs, 4);
        assert_eq!(uvs.len(), 4);
        assert_eq!(uvs[0], [0.5, 0.5]);
        assert_eq!(uvs[3], [0.0, 0.0]);
    }
}
