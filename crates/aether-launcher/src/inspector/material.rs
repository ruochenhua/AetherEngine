//! Material inspector state and atomic application helpers.

use aether_engine::asset::AssetManager;
use aether_engine::ecs::{Entity, World};
use aether_engine::renderer::renderable::MaterialUniform;
use aether_engine::scene::{material::MaterialResolver, MaterialConfig};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MaterialState {
    pub(crate) config: Option<MaterialConfig>,
    pub(crate) uniform: MaterialUniform,
}

pub(crate) fn config_from_uniform(material: &MaterialUniform) -> MaterialConfig {
    MaterialConfig {
        albedo: material.albedo,
        roughness: material.roughness,
        metallic: material.metallic,
        unlit: material.unlit != 0,
        ..MaterialConfig::default()
    }
}

pub(crate) fn apply_material_config(
    world: &mut World,
    entity: Entity,
    desired: &MaterialConfig,
    assets: &mut AssetManager,
) -> Result<MaterialState, String> {
    let resolution = MaterialResolver::new(".")
        .resolve(desired, assets)
        .map_err(|error| error.to_string())?;
    let next_uniform = MaterialUniform::from_resolution(&resolution);
    let old_uniform = *world
        .query_one::<&MaterialUniform>(entity)
        .get()
        .map_err(|error| format!("material uniform is unavailable: {error}"))?;
    let old_config = world
        .query_one::<&MaterialConfig>(entity)
        .get()
        .ok()
        .cloned();

    if let Ok(current) = world.query_one_mut::<&mut MaterialConfig>(entity) {
        *current = desired.clone();
    } else {
        world
            .insert(entity, (desired.clone(),))
            .map_err(|error| format!("failed to store material config: {error}"))?;
    }
    *world
        .query_one_mut::<&mut MaterialUniform>(entity)
        .map_err(|error| format!("material uniform is unavailable: {error}"))? = next_uniform;

    Ok(MaterialState {
        config: old_config,
        uniform: old_uniform,
    })
}

pub(crate) fn swap_material_state(
    world: &mut World,
    entity: Entity,
    desired_config: Option<&MaterialConfig>,
    desired_uniform: MaterialUniform,
) -> MaterialState {
    let current = MaterialState {
        config: world
            .query_one::<&MaterialConfig>(entity)
            .get()
            .ok()
            .cloned(),
        uniform: *world
            .query_one::<&MaterialUniform>(entity)
            .get()
            .expect("material undo requires a material uniform"),
    };

    match desired_config {
        Some(config) => {
            if let Ok(current_config) = world.query_one_mut::<&mut MaterialConfig>(entity) {
                *current_config = config.clone();
            } else {
                world
                    .insert(entity, (config.clone(),))
                    .expect("material undo entity must exist");
            }
        }
        None => {
            let _ = world.remove::<(MaterialConfig,)>(entity);
        }
    }
    *world
        .query_one_mut::<&mut MaterialUniform>(entity)
        .expect("material undo requires a material uniform") = desired_uniform;
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extended_config() -> MaterialConfig {
        MaterialConfig {
            albedo: [0.2, 0.4, 0.8, 1.0],
            roughness: 0.18,
            metallic: 0.72,
            unlit: false,
            albedo_texture: None,
            normal_texture: Some("textures/detail_normal.png".into()),
            orm_texture: Some("textures/detail_orm.png".into()),
            emissive_texture: Some("textures/glow.png".into()),
            normal_scale: 1.4,
            occlusion_strength: 0.65,
            emissive: [0.1, 0.3, 0.8],
            emissive_intensity: 2.5,
            orm_swizzle: Default::default(),
        }
    }

    #[test]
    fn invalid_material_is_rejected_without_mutating_world() {
        let mut world = World::new();
        let initial = MaterialConfig::default();
        let entity = world.spawn((initial.clone(), MaterialUniform::default()));
        let mut desired = initial.clone();
        desired.normal_scale = 4.0;

        let result = apply_material_config(&mut world, entity, &desired, &mut AssetManager::new());

        assert!(result.is_err());
        assert_eq!(
            world.query_one::<&MaterialConfig>(entity).get().unwrap(),
            &initial
        );
        assert_eq!(
            *world.query_one::<&MaterialUniform>(entity).get().unwrap(),
            MaterialUniform::default()
        );
    }

    #[test]
    fn valid_material_updates_config_and_gpu_adapter_together() {
        let mut world = World::new();
        let initial = MaterialConfig::default();
        let entity = world.spawn((initial.clone(), MaterialUniform::default()));
        let desired = extended_config();

        let previous =
            apply_material_config(&mut world, entity, &desired, &mut AssetManager::new())
                .expect("valid material should apply");

        assert_eq!(previous.config, Some(initial));
        assert_eq!(previous.uniform, MaterialUniform::default());
        assert_eq!(
            world.query_one::<&MaterialConfig>(entity).get().unwrap(),
            &desired
        );

        let expected = MaterialUniform::from_resolution(
            &MaterialResolver::new(".")
                .resolve(&desired, &mut AssetManager::new())
                .expect("fixture paths use fallback textures"),
        );
        assert_eq!(
            *world.query_one::<&MaterialUniform>(entity).get().unwrap(),
            expected
        );
    }
}
