//! Terrain state update helpers.

use crate::asset::mesh::GpuMesh;
use crate::asset::terrain_material::TerrainMaterial;
use crate::asset::texture_cache::GpuTextureCache;
use crate::asset::AssetManager;
use crate::ecs::components::Terrain;
use crate::math::{Frustum, Mat4, Vec3};
use crate::renderer::renderable::ViewProjUniform;
use crate::terrain::{build_chunk_grid, cull_and_select_lod, height_function_from_source};
use std::sync::Arc;

use super::{ChunkInstanceData, TerrainPass, TerrainUniform};

impl TerrainPass {
    pub(super) fn update_terrain(
        &mut self,
        terrain: Terrain,
        view: &Mat4,
        proj: &Mat4,
        queue: &wgpu::Queue,
        texture_cache: &GpuTextureCache,
        asset_manager: &AssetManager,
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
                        (chunk.center.x, chunk.center.z),
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

        // Resolve GPU textures and rebuild the material bind group.
        let splat = texture_cache.get_or_upload_optional(terrain.material.splat_map, asset_manager);
        let layer0 = texture_cache.get_or_upload_optional(
            terrain.material.layers[0].albedo_texture.clone(),
            asset_manager,
        );
        let layer1 = texture_cache.get_or_upload_optional(
            terrain.material.layers[1].albedo_texture.clone(),
            asset_manager,
        );
        let layer2 = texture_cache.get_or_upload_optional(
            terrain.material.layers[2].albedo_texture.clone(),
            asset_manager,
        );
        let layer3 = texture_cache.get_or_upload_optional(
            terrain.material.layers[3].albedo_texture.clone(),
            asset_manager,
        );
        self.terrain_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Terrain Material BG"),
            layout: &self.terrain_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.terrain_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&splat.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&splat.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&layer0.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&layer1.view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&layer2.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&layer3.view),
                },
            ],
        });

        // Cull and select LOD.
        let camera_pos = self.camera_position_from_view(view);
        let frustum = Frustum::from_view_projection(*proj * *view);
        let height_estimate = self.estimate_height_range(&terrain.source);
        self.visible_chunk_indices = cull_and_select_lod(
            &mut self.chunks,
            camera_pos,
            &frustum,
            height_estimate.0,
            height_estimate.1,
            2.0,
        );
        tracing::debug!(
            target: "aether_engine::renderer::passes::terrain",
            "TerrainPass::update_terrain: chunks={}, visible={}, camera_pos={:?}",
            self.chunks.len(),
            self.visible_chunk_indices.len(),
            camera_pos
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

    fn estimate_height_range(&self, source: &crate::scene::TerrainSource) -> (f32, f32) {
        // Conservative estimate based on the source's configured amplitude.
        // Procedural sine waves and FBM Perlin can both exceed the nominal
        // amplitude when multiple octaves/layers combine, so leave headroom.
        let amplitude = match source {
            crate::scene::TerrainSource::Heightmap(_) => 128.0,
            crate::scene::TerrainSource::Procedural { amplitude, .. } => *amplitude,
            crate::scene::TerrainSource::Perlin { amplitude, .. } => *amplitude,
        };
        let half_range = amplitude * 1.5;
        (-half_range, half_range)
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
