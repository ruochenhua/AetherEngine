//! glTF 2.0 loader.
//!
//! Uses the `gltf` crate to import `.gltf` and `.glb` files. The default scene
//! is traversed, node transforms are applied, and all primitive geometry is
//! merged into a single [`CpuMesh`].
//!
//! [`CpuMesh`]: crate::asset::mesh::CpuMesh

use std::path::Path;

use glam::{Mat4, Vec3};

use crate::asset::loaders::{compute_smooth_normals, compute_tangents, fill_missing_uvs};
use crate::asset::mesh::{CpuMaterial, CpuMesh, CpuSubmesh};

/// Load a glTF file into a `CpuMesh`.
///
/// The default scene is used (falling back to the first scene if no default is
/// set). Geometry from all mesh primitives is merged into one indexed mesh.
/// Missing normals are computed as smooth per-vertex normals; missing UVs are
/// set to `(0, 0)`; tangents are generated when not authored.
pub fn load(path: &Path) -> anyhow::Result<CpuMesh> {
    let (document, buffers, _images) = gltf::import(path)
        .map_err(|e| anyhow::anyhow!("Failed to import glTF '{}': {}", path.display(), e))?;

    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .ok_or_else(|| anyhow::anyhow!("glTF '{}' contains no scenes", path.display()))?;

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut tangents = Vec::new();
    let mut indices = Vec::new();
    let mut submeshes = Vec::new();
    let mut missing_normals = false;
    let mut missing_tangents = false;

    let base_dir = path.parent().unwrap_or_else(|| Path::new(""));

    #[allow(clippy::too_many_arguments)]
    fn visit_node(
        node: gltf::Node,
        parent_transform: Mat4,
        buffers: &[gltf::buffer::Data],
        base_dir: &Path,
        positions: &mut Vec<[f32; 3]>,
        normals: &mut Vec<[f32; 3]>,
        uvs: &mut Vec<[f32; 2]>,
        tangents: &mut Vec<[f32; 4]>,
        indices: &mut Vec<u32>,
        submeshes: &mut Vec<CpuSubmesh>,
        missing_normals: &mut bool,
        missing_tangents: &mut bool,
    ) {
        let local_transform = Mat4::from_cols_array_2d(&node.transform().matrix());
        let transform = parent_transform * local_transform;

        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
                let base_index = positions.len();
                let index_offset = indices.len();

                let Some(positions_iter) = reader.read_positions() else {
                    continue;
                };

                for p in positions_iter {
                    positions.push(transform_point(transform, p));
                }
                let vertex_count = positions.len() - base_index;
                if vertex_count == 0 {
                    continue;
                }

                if let Some(iter) = reader.read_normals() {
                    for n in iter {
                        normals.push(transform_vector(transform, n));
                    }
                } else {
                    normals.resize(positions.len(), [0.0, 0.0, 0.0]);
                    *missing_normals = true;
                }

                if let Some(iter) = reader.read_tex_coords(0) {
                    for uv in iter.into_f32() {
                        uvs.push(uv);
                    }
                } else {
                    uvs.resize(positions.len(), [0.0, 0.0]);
                }

                if let Some(iter) = reader.read_tangents() {
                    for t in iter {
                        tangents.push(transform_tangent(transform, t));
                    }
                } else {
                    tangents.resize(positions.len(), [1.0, 0.0, 0.0, 1.0]);
                    *missing_tangents = true;
                }

                if let Some(index_reader) = reader.read_indices() {
                    for idx in index_reader.into_u32() {
                        indices.push((base_index + idx as usize) as u32);
                    }
                } else {
                    for i in 0..vertex_count {
                        indices.push((base_index + i) as u32);
                    }
                }

                let index_count = indices.len() - index_offset;
                if index_count > 0 {
                    submeshes.push(build_submesh_from_primitive(
                        primitive,
                        index_offset,
                        index_count,
                        base_dir,
                    ));
                }

                debug_assert_eq!(positions.len(), normals.len());
                debug_assert_eq!(positions.len(), uvs.len());
                debug_assert_eq!(positions.len(), tangents.len());
            }
        }

        for child in node.children() {
            visit_node(
                child,
                transform,
                buffers,
                base_dir,
                positions,
                normals,
                uvs,
                tangents,
                indices,
                submeshes,
                missing_normals,
                missing_tangents,
            );
        }
    }

    for node in scene.nodes() {
        visit_node(
            node,
            Mat4::IDENTITY,
            &buffers,
            base_dir,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut tangents,
            &mut indices,
            &mut submeshes,
            &mut missing_normals,
            &mut missing_tangents,
        );
    }

    if positions.is_empty() {
        anyhow::bail!("glTF '{}' contains no renderable geometry", path.display());
    }

    if missing_normals {
        compute_smooth_normals(&positions, &indices, &mut normals);
    }

    fill_missing_uvs(&mut uvs, positions.len());

    if missing_tangents {
        tangents = compute_tangents(&positions, &normals, &uvs, &indices);
    }

    Ok(CpuMesh {
        positions,
        normals,
        uvs,
        tangents,
        indices,
        submeshes,
    })
}

fn build_submesh_from_primitive(
    primitive: gltf::Primitive,
    index_offset: usize,
    index_count: usize,
    base_dir: &Path,
) -> CpuSubmesh {
    let material = primitive.material();
    let pbr = material.pbr_metallic_roughness();

    let albedo_texture = pbr.base_color_texture().and_then(|tex| {
        let source = tex.texture().source();
        match source.source() {
            gltf::image::Source::Uri { uri, .. } => {
                Some(base_dir.join(uri).to_string_lossy().to_string())
            }
            _ => None,
        }
    });

    let base_color = pbr.base_color_factor();
    let name = material
        .name()
        .map(String::from)
        .unwrap_or_else(|| format!("primitive_{}", primitive.index()));

    CpuSubmesh {
        name,
        index_offset,
        index_count,
        material: CpuMaterial {
            name: material.name().unwrap_or("").to_string(),
            base_color,
            metallic: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            albedo_texture,
        },
    }
}

fn transform_point(transform: Mat4, p: [f32; 3]) -> [f32; 3] {
    transform.transform_point3(Vec3::from_array(p)).to_array()
}

fn transform_vector(transform: Mat4, v: [f32; 3]) -> [f32; 3] {
    // Normalize after transformation so normals stay unit-length under scale.
    // Note: for non-uniform scale the mathematically correct transform is the
    // inverse-transpose of the upper 3x3; this simplified path is sufficient
    // for typical assets.
    transform
        .transform_vector3(Vec3::from_array(v))
        .normalize()
        .to_array()
}

fn transform_tangent(transform: Mat4, t: [f32; 4]) -> [f32; 4] {
    let v = Vec3::new(t[0], t[1], t[2]);
    let transformed = transform.transform_vector3(v).normalize();
    [transformed.x, transformed.y, transformed.z, t[3]]
}
