//! Scene serializer: ECS World → RON.
//!
//! Traverses the ECS World and produces a `SceneDescription` that can be
//! written to a `.ron` file.
//!
//! ## Known Pitfalls
//! - **Name Component 遗漏**: `serialize_world` 查询 `(Transform, MeshHandle,
//!   MaterialUniform, Visibility, Name)`。未附加 `Name` 的 entity 会被静默跳过。
//!   所有 spawn 路径必须附加 `Name`。
//! - **相机保存前同步**: 保存前必须调用 `write_camera_to_world` 将 FlyCamera
//!   的 position/rotation 写回 ECS Camera Component，否则 RON 使用 stale 数据。

use crate::ecs::components::{
    Atmosphere, Camera, Clouds, GodRay, Light, MeshHandle, Name, PrefabInstanceRoot,
    PrefabNodeInstance, Terrain, Transform, Visibility, Water,
};
use crate::ecs::{Entity, World};
use crate::physics::{ColliderList, ColliderShape, RigidBody};
use crate::renderer::light::LightingUniforms;
use crate::renderer::renderable::MaterialUniform;
use crate::scene::{
    AtmosphereConfig, CameraConfig, CloudConfig, GodRayConfig, LightConfig, MaterialConfig,
    MeshRef, ObjectConfig, PhysicsBodyConfig, PhysicsColliderConfig, PhysicsColliderShapeConfig,
    PhysicsConfig, SceneDescription, TerrainConfig, TransformConfig, WaterConfig,
};
use glam::Vec3;
use std::collections::HashSet;

/// Serialize the ECS World into a `SceneDescription`.
///
/// - Extracts camera from the first entity with `(Transform, Camera)`.
/// - Extracts lights from entities with `(Transform, Light)`.
/// - Extracts objects from entities with `(Transform, MeshHandle, MaterialUniform, Visibility, Name)`.
/// - Ignores entities with only editor components.
/// - Uses the provided lighting state for ambient.
pub fn serialize_world(
    world: &World,
    lighting: &LightingUniforms,
    scene_name: &str,
) -> SceneDescription {
    let prefab_entities = prefab_entities(world);
    let camera = extract_camera(world, &prefab_entities);
    let lights = extract_lights(world, &prefab_entities);
    let terrain = extract_terrain(world);
    let atmosphere = extract_atmosphere(world, &prefab_entities);
    let water = extract_water(world);
    let clouds = extract_clouds(world, &prefab_entities);
    let god_ray = extract_god_ray(world, &prefab_entities);
    let particle_emitters = extract_particle_emitters(world);
    let objects = extract_objects(world, &prefab_entities);

    SceneDescription {
        name: scene_name.to_string(),
        camera,
        lights,
        ambient: lighting.ambient_intensity,
        terrain,
        atmosphere,
        water,
        clouds,
        god_ray,
        particle_emitters,
        objects,
        prefab_instances: extract_prefab_instances(world),
    }
}

/// Serialize a `SceneDescription` to a pretty-printed RON string.
pub fn to_ron_string(desc: &SceneDescription) -> anyhow::Result<String> {
    let config = ron::ser::PrettyConfig::new()
        .depth_limit(4)
        .separate_tuple_members(true)
        .enumerate_arrays(false);
    let s = ron::ser::to_string_pretty(desc, config)?;
    Ok(s)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn extract_camera(world: &World, prefab_entities: &HashSet<Entity>) -> CameraConfig {
    let mut camera = CameraConfig::default();

    if let Some((transform, cam)) = world
        .query::<(Entity, &Transform, &Camera)>()
        .iter()
        .find_map(|(entity, transform, camera)| {
            (!prefab_entities.contains(&entity)).then_some((transform, camera))
        })
    {
        let (yaw, pitch, _roll) = transform.rotation.to_euler(glam::EulerRot::YXZ);
        camera = CameraConfig {
            position: transform.translation.to_array(),
            yaw,
            pitch,
            speed: cam.speed,
            fov: cam.fov.to_degrees(),
            near: cam.near,
            far: cam.far,
        };
    }
    camera
}

fn extract_lights(world: &World, prefab_entities: &HashSet<Entity>) -> Vec<LightConfig> {
    let mut lights = Vec::new();

    for (entity, transform, light) in world.query::<(Entity, &Transform, &Light)>().iter() {
        if prefab_entities.contains(&entity) {
            continue;
        }
        // Direction is derived from rotation: default light direction is -Y,
        // so rotated direction = rotation * -Y.
        let direction = (transform.rotation * Vec3::NEG_Y).normalize().to_array();
        lights.push(LightConfig {
            light_type: light.light_type,
            direction,
            position: transform.translation.to_array(),
            color: light.color,
            intensity: light.intensity,
            range: light.range,
            inner_cone_angle: light.inner_cone_angle,
            outer_cone_angle: light.outer_cone_angle,
        });
    }

    lights
}

fn extract_terrain(world: &World) -> Option<TerrainConfig> {
    world
        .query::<&Terrain>()
        .iter()
        .next()
        .map(|terrain| TerrainConfig {
            source: terrain.source.clone(),
            geometry: terrain.geometry.clone(),
            splatmap: terrain.splatmap_path.clone(),
            layers: if terrain.layer_configs.is_empty() {
                vec![
                    crate::scene::TerrainLayerConfig::default(),
                    crate::scene::TerrainLayerConfig::default(),
                    crate::scene::TerrainLayerConfig::default(),
                    crate::scene::TerrainLayerConfig::default(),
                ]
            } else {
                terrain.layer_configs.clone()
            },
        })
}

fn extract_atmosphere(
    world: &World,
    prefab_entities: &HashSet<Entity>,
) -> Option<AtmosphereConfig> {
    world
        .query::<(Entity, &Atmosphere)>()
        .iter()
        .find_map(|(entity, atmos)| {
            (!prefab_entities.contains(&entity)).then(|| atmos.config.clone())
        })
}

fn extract_water(world: &World) -> Option<WaterConfig> {
    world
        .query::<&Water>()
        .iter()
        .next()
        .map(|water| water.config.clone())
}

fn extract_clouds(world: &World, prefab_entities: &HashSet<Entity>) -> Option<CloudConfig> {
    world
        .query::<(Entity, &Clouds)>()
        .iter()
        .find_map(|(entity, clouds)| {
            (!prefab_entities.contains(&entity)).then(|| clouds.config.clone())
        })
}

fn extract_god_ray(world: &World, prefab_entities: &HashSet<Entity>) -> Option<GodRayConfig> {
    world
        .query::<(Entity, &GodRay)>()
        .iter()
        .find_map(|(entity, gr)| (!prefab_entities.contains(&entity)).then(|| gr.config.clone()))
}

fn extract_particle_emitters(world: &World) -> Vec<crate::particles::ParticleEmitterConfig> {
    let mut emitters: Vec<_> = world
        .query::<(crate::ecs::Entity, &crate::particles::ParticleEmitterConfig)>()
        .iter()
        .map(|(entity, config)| (entity.to_bits().get(), config.clone()))
        .collect();
    emitters.sort_by_key(|(entity_bits, _)| *entity_bits);
    emitters
        .into_iter()
        .map(|(_, mut config)| {
            // ECS IDs are runtime details; they are allocated again on load.
            config.entity_bits = 0;
            config
        })
        .collect()
}

fn extract_objects(world: &World, prefab_entities: &HashSet<Entity>) -> Vec<ObjectConfig> {
    let mut objects = Vec::new();
    for (
        entity,
        transform,
        mesh_handle,
        material,
        visibility,
        name,
        stored_config,
        material_asset,
        body,
        colliders,
    ) in world
        .query::<(
            Entity,
            &Transform,
            &MeshHandle,
            &MaterialUniform,
            &Visibility,
            &Name,
            Option<&MaterialConfig>,
            Option<&crate::ecs::components::MaterialAssetRef>,
            Option<&RigidBody>,
            Option<&ColliderList>,
        )>()
        .iter()
    {
        if prefab_entities.contains(&entity) {
            continue;
        }
        let mesh_ref = match &mesh_handle.source {
            crate::ecs::components::MeshSource::Builtin(name) => MeshRef::Builtin(name.clone()),
            crate::ecs::components::MeshSource::File(path) => MeshRef::File(path.clone()),
        };

        let obj = ObjectConfig {
            name: name.0.clone(),
            mesh: mesh_ref,
            transform: TransformConfig {
                translation: transform.translation.to_array(),
                rotation: transform.rotation.to_array(),
                scale: transform.scale.to_array(),
            },
            material: if material_asset.is_some() {
                MaterialConfig::default()
            } else {
                stored_config.cloned().unwrap_or_else(|| MaterialConfig {
                    albedo: material.albedo,
                    roughness: material.roughness,
                    metallic: material.metallic,
                    unlit: material.unlit != 0,
                    albedo_texture: None,
                    ..MaterialConfig::default()
                })
            },
            material_asset: material_asset.map(|reference| reference.0.clone()),
            visible: visibility.0,
            physics: body.zip(colliders).and_then(|(body, colliders)| {
                let colliders = colliders
                    .0
                    .iter()
                    .map(|collider| {
                        let shape = match collider.shape {
                            ColliderShape::Sphere(radius) => {
                                PhysicsColliderShapeConfig::Sphere { radius }
                            }
                            ColliderShape::Box(half_extents) => PhysicsColliderShapeConfig::Box {
                                half_extents: half_extents.to_array(),
                            },
                            ColliderShape::Capsule(radius, height) => {
                                PhysicsColliderShapeConfig::Capsule { radius, height }
                            }
                            ColliderShape::Mesh => PhysicsColliderShapeConfig::Mesh,
                        };
                        Some(PhysicsColliderConfig {
                            shape,
                            is_trigger: collider.is_trigger,
                            friction: collider.friction,
                            restitution: collider.restitution,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(PhysicsConfig {
                    body: PhysicsBodyConfig {
                        mass: body.mass,
                        is_static: body.is_static,
                        velocity: body.velocity.to_array(),
                        angular_velocity: body.angular_velocity.to_array(),
                    },
                    colliders,
                })
            }),
        };
        objects.push(obj);
    }

    objects
}

fn prefab_entities(world: &World) -> HashSet<Entity> {
    world
        .query::<(Entity, &PrefabNodeInstance)>()
        .iter()
        .map(|(entity, _)| entity)
        .collect()
}

fn extract_prefab_instances(world: &World) -> Vec<crate::asset::prefab::PrefabInstanceConfig> {
    let mut instances = world
        .query::<&PrefabInstanceRoot>()
        .iter()
        .map(|root| root.0.clone())
        .collect::<Vec<_>>();
    instances.sort_by_key(|instance| instance.instance_id);
    instances
}

#[cfg(test)]
mod tests;
