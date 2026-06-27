//! Wavefront OBJ loader.
//!
//! Uses the `tobj` crate to parse `.obj` files (and optional `.mtl` material
//! libraries). Geometry from all objects/groups in the file is merged into a
//! single shared vertex buffer, while index ranges are kept per material so
//! that each material can use its own diffuse texture.
//!
//! [`CpuMesh`]: crate::asset::mesh::CpuMesh

use std::path::Path;

use crate::asset::loaders::{compute_smooth_normals, compute_tangents, fill_missing_uvs};
use crate::asset::mesh::{CpuMaterial, CpuMesh, CpuSubmesh};

/// Load an OBJ file into a `CpuMesh`.
///
/// Missing normals are computed as smooth per-vertex normals; missing UVs are set
/// to `(0, 0)`; tangents are always generated so normal mapping can be used.
///
/// When the OBJ/MTL defines more than one textured material, the returned mesh
/// contains [`CpuSubmesh`] ranges so the scene loader can bind the correct
/// diffuse texture per part.
pub fn load(path: &Path) -> anyhow::Result<CpuMesh> {
    let (models, materials) = tobj::load_obj(path, &tobj::GPU_LOAD_OPTIONS)
        .map_err(|e| anyhow::anyhow!("Failed to load OBJ '{}': {}", path.display(), e))?;

    let materials = materials
        .map_err(|e| anyhow::anyhow!("Failed to load MTL for '{}': {}", path.display(), e))?;

    let base_dir = path.parent().unwrap_or_else(|| Path::new(""));

    // Convert tobj materials into our simplified CpuMaterial.
    let cpu_materials: Vec<CpuMaterial> = materials
        .iter()
        .map(|m| {
            let albedo_texture = m
                .diffuse_texture
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| base_dir.join(s).to_string_lossy().to_string());
            let diffuse = m.diffuse.unwrap_or([0.64, 0.64, 0.64]);
            CpuMaterial {
                name: m.name.clone(),
                base_color: [diffuse[0], diffuse[1], diffuse[2], 1.0],
                // MTL uses a specular exponent; map it loosely to roughness.
                roughness: (1.0 - m.shininess.unwrap_or(0.0) / 1000.0).clamp(0.0, 1.0),
                metallic: 0.0,
                albedo_texture,
            }
        })
        .collect();

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut missing_normals = false;

    // Map material id -> list of indices (in the merged vertex buffer).
    let mut buckets: std::collections::HashMap<Option<usize>, Vec<u32>> =
        std::collections::HashMap::new();

    for model in models {
        let mesh = model.mesh;
        let base_index = positions.len();
        let vertex_count = mesh.positions.len() / 3;

        positions.extend(mesh.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]));

        if mesh.normals.is_empty() {
            normals.resize(positions.len(), [0.0, 0.0, 0.0]);
            missing_normals = true;
        } else {
            normals.extend(mesh.normals.chunks_exact(3).map(|c| [c[0], c[1], c[2]]));
        }

        if mesh.texcoords.is_empty() {
            uvs.resize(positions.len(), [0.0, 0.0]);
        } else {
            // wgpu texture coordinates have (0,0) at the top-left of the image,
            // while OBJ conventionally treats v=0 as the bottom-left. Flip V so
            // external OBJ assets display right-side up.
            uvs.extend(mesh.texcoords.chunks_exact(2).map(|c| [c[0], 1.0 - c[1]]));
        }

        let bucket = buckets.entry(mesh.material_id).or_default();
        for idx in &mesh.indices {
            bucket.push((base_index + *idx as usize) as u32);
        }

        debug_assert_eq!(positions.len(), base_index + vertex_count);
        debug_assert_eq!(positions.len(), normals.len());
        debug_assert_eq!(positions.len(), uvs.len());
    }

    // Build submeshes only when there is more than one material part, or when
    // the single material actually references a texture. This keeps simple
    // untextured OBJs as a single mesh that can be styled by the scene.
    let has_textured_material = cpu_materials.iter().any(|m| m.albedo_texture.is_some());
    let build_submeshes = buckets.len() > 1 || (buckets.len() == 1 && has_textured_material);

    let mut submeshes = Vec::new();
    let indices: Vec<u32> = if build_submeshes {
        // Preserve deterministic order by material id.
        let mut material_ids: Vec<Option<usize>> = buckets.keys().copied().collect();
        material_ids.sort_by_key(|id| id.unwrap_or(usize::MAX));

        let mut merged = Vec::new();
        for id in material_ids {
            let bucket_indices = buckets.remove(&id).unwrap();
            if bucket_indices.is_empty() {
                continue;
            }

            let material = id
                .and_then(|i| cpu_materials.get(i))
                .cloned()
                .unwrap_or_default();

            submeshes.push(CpuSubmesh {
                name: material.name.clone(),
                index_offset: merged.len(),
                index_count: bucket_indices.len(),
                material,
            });

            merged.extend_from_slice(&bucket_indices);
        }
        merged
    } else {
        // Single material / untextured: flatten in model order.
        buckets.into_values().flatten().collect()
    };

    if missing_normals {
        compute_smooth_normals(&positions, &indices, &mut normals);
    }

    fill_missing_uvs(&mut uvs, positions.len());

    let tangents = compute_tangents(&positions, &normals, &uvs, &indices);

    Ok(CpuMesh {
        positions,
        normals,
        uvs,
        tangents,
        indices,
        submeshes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_simple_quad_obj() {
        let dir = std::env::temp_dir().join("aether_test_obj_loader");
        let _ = std::fs::create_dir(&dir);
        let path = dir.join("quad.obj");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "v -1.0 -1.0 0.0").unwrap();
        writeln!(file, "v  1.0 -1.0 0.0").unwrap();
        writeln!(file, "v  1.0  1.0 0.0").unwrap();
        writeln!(file, "v -1.0  1.0 0.0").unwrap();
        writeln!(file, "vt 0.0 1.0").unwrap();
        writeln!(file, "vt 1.0 1.0").unwrap();
        writeln!(file, "vt 1.0 0.0").unwrap();
        writeln!(file, "vt 0.0 0.0").unwrap();
        writeln!(file, "vn 0.0 0.0 -1.0").unwrap();
        writeln!(file, "f 1/1/1 3/3/1 2/2/1").unwrap();
        writeln!(file, "f 1/1/1 4/4/1 3/3/1").unwrap();
        drop(file);

        let mesh = load(&path).expect("should load quad.obj");
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
        assert_eq!(mesh.normals.len(), 4);
        assert_eq!(mesh.uvs.len(), 4);
        assert_eq!(mesh.tangents.len(), 4);
        assert!(mesh.submeshes.is_empty());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
