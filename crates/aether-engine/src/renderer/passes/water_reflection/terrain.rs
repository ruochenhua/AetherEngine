// Terrain material update for water reflection rendering.

use super::WaterReflectionPass;
use crate::ecs::components::Terrain;
use crate::terrain::{create_terrain_material_bind_group, write_terrain_uniforms};
use std::sync::Arc;

impl WaterReflectionPass {
    pub(super) fn update_terrain_material(
        &mut self,
        terrain: &Terrain,
        queue: &wgpu::Queue,
        texture_cache: &crate::asset::texture_cache::GpuTextureCache,
        asset_manager: &crate::asset::AssetManager,
    ) {
        write_terrain_uniforms(
            &self.terrain_buffer,
            &terrain.material,
            terrain.splatmap_path.is_some(),
            terrain.geometry.extent,
            terrain.geometry.albedo_tiling,
            queue,
        );

        let splat =
            texture_cache.get_or_upload_optional(terrain.material.splat_map.clone(), asset_manager);
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

        let needs_rebuild = match (
            &self.terrain_last_splat,
            &self.terrain_last_layer0,
            &self.terrain_last_layer1,
            &self.terrain_last_layer2,
            &self.terrain_last_layer3,
        ) {
            (Some(last_splat), Some(last_l0), Some(last_l1), Some(last_l2), Some(last_l3)) => {
                !Arc::ptr_eq(last_splat, &splat)
                    || !Arc::ptr_eq(last_l0, &layer0)
                    || !Arc::ptr_eq(last_l1, &layer1)
                    || !Arc::ptr_eq(last_l2, &layer2)
                    || !Arc::ptr_eq(last_l3, &layer3)
            }
            _ => true,
        };

        if needs_rebuild {
            self.terrain_bind_group = create_terrain_material_bind_group(
                &self.device,
                &self.terrain_bind_group_layout,
                &self.terrain_buffer,
                &splat,
                &layer0,
                &layer1,
                &layer2,
                &layer3,
            );
            self.terrain_last_splat = Some(splat);
            self.terrain_last_layer0 = Some(layer0);
            self.terrain_last_layer1 = Some(layer1);
            self.terrain_last_layer2 = Some(layer2);
            self.terrain_last_layer3 = Some(layer3);
        }
    }
}
