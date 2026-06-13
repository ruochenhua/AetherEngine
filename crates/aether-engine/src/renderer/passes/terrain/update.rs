//! Terrain state update helpers.

use crate::asset::mesh::GpuMesh;
use crate::asset::terrain_material::TerrainMaterial;
use crate::ecs::components::Terrain;
use crate::ecs::World;
use crate::math::{Frustum, Mat4, Vec3};
use crate::renderer::renderable::ViewProjUniform;
use crate::terrain::{build_chunk_grid, cull_and_select_lod, height_function_from_source};
use std::sync::Arc;

use super::{ChunkInstanceData, TerrainPass, TerrainUniform};

pub(super) fn read_terrain(world: &World) -> Option<Terrain> {
    world.query::<&Terrain>().iter().next().cloned()
}

impl TerrainPass {
    pub(super) fn update_terrain(
        &mut self,
        terrain: Terrain,
        _world: &World,
        view: &Mat4,
        proj: &Mat4,
        queue: &wgpu::Queue,
    ) {
        // If the terrain configuration changed, invalidate cached geometry.
        if self.last_terrain.as_ref() != Some(&terrain) {
            self.chunks.clear();
            self.chunk_meshes.clear();
            self.last_terrain = Some(terrain.clone());
        }

        // Build chunks and meshes if they don't exist or if terrain config changed.
        if self.chunks.is_empty() {
            let height_fn = height_function_from_source(&terrain.source);
            self.chunks = build_chunk_grid(
                terrain.geometry.extent,
                terrain.geometry.chunk_size as f32,
                terrain.geometry.max_lod,
            );
            self.chunk_meshes = self
                .chunks
                .iter()
                .map(|chunk| {
                    let cpu_meshes = crate::terrain::generate_chunk_lod_meshes(
                        terrain.geometry.max_lod,
                        chunk.size,
                        height_fn.as_ref(),
                    );
                    cpu_meshes
                        .into_iter()
                        .map(|cpu| Arc::new(GpuMesh::from_cpu(&self.device, &cpu)))
                        .collect()
                })
                .collect();
        }

        // Update view/proj uniform.
        let vp = ViewProjUniform {
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
        };
        queue.write_buffer(&self.view_proj_buffer, 0, bytemuck::cast_slice(&[vp]));

        // Update terrain material uniform.
        write_terrain_uniforms(
            &self.terrain_buffer,
            &terrain.material,
            terrain.splatmap_path.is_some(),
            queue,
        );

        // Cull and select LOD.
        let camera_pos = self.camera_position_from_view(view);
        let frustum = Frustum::from_view_projection(*proj * *view);
        let height_estimate = self.estimate_height_range();
        self.visible_chunk_indices = cull_and_select_lod(
            &mut self.chunks,
            camera_pos,
            &frustum,
            height_estimate.0,
            height_estimate.1,
            2.0,
        );

        // Build instance data.
        self.chunk_instance_data.clear();
        for idx in &self.visible_chunk_indices {
            let chunk = &self.chunks[*idx];
            let model = Mat4::from_translation(chunk.center);
            self.chunk_instance_data.push(ChunkInstanceData {
                model_matrix: model.to_cols_array_2d(),
                lod: chunk.lod,
                _pad: [0; 3],
            });
        }

        if !self.chunk_instance_data.is_empty() {
            queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.chunk_instance_data),
            );
        }
    }

    fn camera_position_from_view(&self, view: &Mat4) -> Vec3 {
        // view = R * T(-eye) => eye = -R^T * view[3].xyz
        let inv = view.inverse();
        inv.transform_point3(Vec3::ZERO)
    }

    fn estimate_height_range(&self) -> (f32, f32) {
        // Conservative estimate; in production use actual height bounds per chunk.
        (-128.0, 128.0)
    }
}

pub(super) fn write_terrain_uniforms(
    buffer: &wgpu::Buffer,
    material: &TerrainMaterial,
    has_splat_map: bool,
    queue: &wgpu::Queue,
) {
    let uniform = terrain_uniform_from_material(material, has_splat_map);
    queue.write_buffer(buffer, 0, bytemuck::cast_slice(&[uniform]));
}

fn terrain_uniform_from_material(
    material: &TerrainMaterial,
    has_splat_map: bool,
) -> TerrainUniform {
    let mut colors = [[0.5, 0.5, 0.5, 1.0]; 4];
    let mut roughness = [0.8; 4];
    let mut metallic = [0.0; 4];
    for (i, layer) in material.layers.iter().enumerate() {
        colors[i] = layer.albedo;
        roughness[i] = layer.roughness;
        metallic[i] = layer.metallic;
    }
    TerrainUniform {
        layer_color_0: colors[0],
        layer_color_1: colors[1],
        layer_color_2: colors[2],
        layer_color_3: colors[3],
        layer_roughness: roughness,
        layer_metallic: metallic,
        has_splat_map: if has_splat_map { 1 } else { 0 },
        _pad: [0; 3],
    }
}
