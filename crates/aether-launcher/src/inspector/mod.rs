//! Context-aware entity inspector for the Launcher editor.
//!
//! Mirrors UE's Details panel: the Inspector renders a different set of
//! editable fields depending on which Actor is selected (Mesh, Light, Terrain,
//! Water, Atmosphere, Clouds, GodRay).

mod apply;
mod helpers;
mod material;
mod material_render;
mod particle;
mod render;

pub(crate) use apply::{apply, apply_undo};
use material::config_from_uniform;

use aether_engine::ecs::components::{
    Atmosphere, Camera, Clouds, GodRay, Light, PrefabAssetStatus, PrefabInstanceRoot, Terrain,
    Transform, Water,
};
use aether_engine::ecs::{Entity, World};
use aether_engine::particles::ParticleEmitterConfig;
use aether_engine::renderer::renderable::MaterialUniform;
use aether_engine::scene::MaterialConfig;

/// A reversible editor action.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub(crate) enum EditorCommand {
    /// Restore a Transform to a previous value.
    Transform {
        entity: Entity,
        old_transform: Transform,
    },
    /// Restore a Material to a previous value.
    Material {
        entity: Entity,
        old_config: Option<MaterialConfig>,
        old_material: MaterialUniform,
        old_asset_ref: Option<String>,
    },
    /// Restore a Light and its Transform to previous values.
    Light {
        entity: Entity,
        old_light: Light,
        old_transform: Transform,
    },
    /// Restore a Terrain to a previous value.
    Terrain {
        entity: Entity,
        old_terrain: Terrain,
    },
    /// Restore a Water to a previous value.
    Water { entity: Entity, old_water: Water },
    /// Restore an Atmosphere to a previous value.
    Atmosphere {
        entity: Entity,
        old_atmosphere: Atmosphere,
    },
    /// Restore a Clouds actor to a previous value.
    Clouds { entity: Entity, old_clouds: Clouds },
    /// Restore a GodRay actor to a previous value.
    GodRay { entity: Entity, old_god_ray: GodRay },
    /// Restore a Camera to a previous value.
    Camera { entity: Entity, old_camera: Camera },
    /// Restore a particle emitter configuration.
    ParticleEmitter {
        entity: Entity,
        old_config: ParticleEmitterConfig,
    },
}

/// Editable snapshot of the currently selected entity.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub(crate) enum InspectorTarget {
    /// Renderable mesh object.
    Mesh {
        entity: Entity,
        transform: Transform,
        material: MaterialConfig,
        material_asset: Option<String>,
        material_asset_error: Option<String>,
        detach_material: bool,
        euler: [f32; 3],
    },
    /// Directional/point/spot light.
    Light {
        entity: Entity,
        transform: Transform,
        light: Light,
        direction: [f32; 3],
    },
    /// Terrain actor.
    Terrain { entity: Entity, terrain: Terrain },
    /// Water actor.
    Water { entity: Entity, water: Water },
    /// Atmosphere actor.
    Atmosphere {
        entity: Entity,
        atmosphere: Atmosphere,
    },
    /// Volumetric clouds actor.
    Clouds { entity: Entity, clouds: Clouds },
    /// God ray actor.
    GodRay { entity: Entity, god_ray: GodRay },
    /// Scene camera.
    Camera {
        entity: Entity,
        camera: Camera,
        fov_degrees: f32,
    },
    /// CPU particle emitter controls.
    ParticleEmitter {
        entity: Entity,
        config: ParticleEmitterConfig,
        restart: bool,
    },
    /// A Prefab instance root with a hot-reload diagnostic.
    PrefabAssetError {
        entity: Entity,
        path: String,
        error: String,
    },
}

impl InspectorTarget {
    pub(crate) fn entity(&self) -> Entity {
        match *self {
            InspectorTarget::Mesh { entity, .. }
            | InspectorTarget::Light { entity, .. }
            | InspectorTarget::Terrain { entity, .. }
            | InspectorTarget::Water { entity, .. }
            | InspectorTarget::Atmosphere { entity, .. }
            | InspectorTarget::Clouds { entity, .. }
            | InspectorTarget::GodRay { entity, .. }
            | InspectorTarget::Camera { entity, .. }
            | InspectorTarget::ParticleEmitter { entity, .. }
            | InspectorTarget::PrefabAssetError { entity, .. } => entity,
        }
    }
}

/// Extract an inspector target from the single `Selected` entity, if any.
pub(crate) fn extract(world: &World) -> Option<InspectorTarget> {
    let (entity, _) = world
        .query::<(Entity, &aether_engine::ecs::components::Selected)>()
        .iter()
        .next()?;

    if let Ok((root, status)) = world
        .query_one::<(&PrefabInstanceRoot, &PrefabAssetStatus)>(entity)
        .get()
    {
        if let Some(error) = status.0.clone() {
            return Some(InspectorTarget::PrefabAssetError {
                entity,
                path: root.0.prefab_asset.clone(),
                error,
            });
        }
    }

    // Mesh object: Transform + MeshHandle + MaterialUniform.
    let mut q = world.query_one::<(
        &Transform,
        &aether_engine::ecs::components::MeshHandle,
        &MaterialUniform,
    )>(entity);
    if let Ok((transform, _mesh, material)) = q.get() {
        let (ex, ey, ez) = transform.rotation.to_euler(glam::EulerRot::XYZ);
        let config = world
            .query_one::<&MaterialConfig>(entity)
            .get()
            .ok()
            .cloned()
            .unwrap_or_else(|| config_from_uniform(material));
        let material_asset = world
            .query_one::<&aether_engine::ecs::components::MaterialAssetRef>(entity)
            .get()
            .ok()
            .map(|reference| reference.0.clone());
        let material_asset_error = world
            .query_one::<&aether_engine::ecs::components::MaterialAssetStatus>(entity)
            .get()
            .ok()
            .and_then(|status| status.0.clone());
        let config = material_asset
            .as_deref()
            .map(|path| rebase_material_textures(config.clone(), path))
            .unwrap_or(config);
        return Some(InspectorTarget::Mesh {
            entity,
            transform: transform.clone(),
            material: config,
            material_asset,
            material_asset_error,
            detach_material: false,
            euler: [ex, ey, ez],
        });
    }

    // Light: Transform + Light.
    let mut q = world.query_one::<(&Transform, &Light)>(entity);
    if let Ok((transform, light)) = q.get() {
        let direction = helpers::light_rotation_to_direction(transform.rotation).to_array();
        return Some(InspectorTarget::Light {
            entity,
            transform: transform.clone(),
            light: light.clone(),
            direction,
        });
    }

    // Terrain.
    let mut q = world.query_one::<(&Transform, &Terrain)>(entity);
    if let Ok((_transform, terrain)) = q.get() {
        return Some(InspectorTarget::Terrain {
            entity,
            terrain: terrain.clone(),
        });
    }

    // Water.
    let mut q = world.query_one::<(&Transform, &Water)>(entity);
    if let Ok((_transform, water)) = q.get() {
        return Some(InspectorTarget::Water {
            entity,
            water: water.clone(),
        });
    }

    // Atmosphere.
    let mut q = world.query_one::<(&Transform, &Atmosphere)>(entity);
    if let Ok((_transform, atmosphere)) = q.get() {
        return Some(InspectorTarget::Atmosphere {
            entity,
            atmosphere: atmosphere.clone(),
        });
    }

    // Clouds.
    let mut q = world.query_one::<(&Transform, &Clouds)>(entity);
    if let Ok((_transform, clouds)) = q.get() {
        return Some(InspectorTarget::Clouds {
            entity,
            clouds: clouds.clone(),
        });
    }

    // GodRay.
    let mut q = world.query_one::<(&Transform, &GodRay)>(entity);
    if let Ok((_transform, god_ray)) = q.get() {
        return Some(InspectorTarget::GodRay {
            entity,
            god_ray: god_ray.clone(),
        });
    }

    // Camera.
    let mut q = world.query_one::<&Camera>(entity);
    if let Ok(camera) = q.get() {
        return Some(InspectorTarget::Camera {
            entity,
            camera: *camera,
            fov_degrees: camera.fov.to_degrees(),
        });
    }

    let mut q = world.query_one::<&ParticleEmitterConfig>(entity);
    if let Ok(config) = q.get() {
        return Some(InspectorTarget::ParticleEmitter {
            entity,
            config: config.clone(),
            restart: false,
        });
    }

    None
}

fn rebase_material_textures(mut config: MaterialConfig, material_path: &str) -> MaterialConfig {
    let parent = std::path::Path::new(material_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    for path in [
        &mut config.albedo_texture,
        &mut config.normal_texture,
        &mut config.orm_texture,
        &mut config.emissive_texture,
    ]
    .into_iter()
    .flatten()
    {
        *path = parent.join(&*path).to_string_lossy().replace('\\', "/");
    }
    if let Some(path) = config
        .transparent
        .as_mut()
        .and_then(|transparent| transparent.texture.as_mut())
    {
        *path = parent.join(&*path).to_string_lossy().replace('\\', "/");
    }
    config
}

/// Render the inspector UI for the given target.
pub(crate) fn render(ui: &mut egui::Ui, target: &mut InspectorTarget) {
    render::render(ui, target);
}

#[cfg(test)]
mod tests;
