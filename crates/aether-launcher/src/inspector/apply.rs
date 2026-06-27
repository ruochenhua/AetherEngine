//! Apply inspector changes back to the ECS world and undo/redo support.

use super::helpers::{light_direction_to_rotation, rebuild_terrain_material};
use super::{EditorCommand, InspectorTarget};
use aether_engine::ecs::components::{
    Atmosphere, Camera, Clouds, GodRay, Light, Terrain, Transform, Water,
};
use aether_engine::ecs::World;
use aether_engine::renderer::renderable::MaterialUniform;
use glam::{Quat, Vec3};

/// Apply any changes in the inspector target back to the ECS world, pushing
/// undo commands for changed components.
pub(crate) fn apply(
    target: &InspectorTarget,
    world: &mut World,
    undo_stack: &mut Vec<EditorCommand>,
    redo_stack: &mut Vec<EditorCommand>,
) {
    match target {
        InspectorTarget::Mesh {
            entity,
            transform,
            material,
            euler,
        } => {
            let mut desired_transform = transform.clone();
            desired_transform.rotation =
                Quat::from_euler(glam::EulerRot::XYZ, euler[0], euler[1], euler[2]);
            if let Ok(current) = world.query_one_mut::<&mut Transform>(*entity) {
                if *current != desired_transform {
                    undo_stack.push(EditorCommand::Transform {
                        entity: *entity,
                        old_transform: current.clone(),
                    });
                    redo_stack.clear();
                    *current = desired_transform;
                }
            }
            if let Ok(current) = world.query_one_mut::<&mut MaterialUniform>(*entity) {
                if *current != *material {
                    undo_stack.push(EditorCommand::Material {
                        entity: *entity,
                        old_material: *current,
                    });
                    redo_stack.clear();
                    *current = *material;
                }
            }
        }
        InspectorTarget::Light {
            entity,
            transform,
            light,
            direction,
        } => {
            let desired_rotation = light_direction_to_rotation(Vec3::from_array(*direction));
            let mut desired_transform = transform.clone();
            desired_transform.rotation = desired_rotation;
            if let Ok(current) = world.query_one_mut::<&mut Transform>(*entity) {
                if *current != desired_transform {
                    undo_stack.push(EditorCommand::Light {
                        entity: *entity,
                        old_light: light.clone(),
                        old_transform: current.clone(),
                    });
                    redo_stack.clear();
                    *current = desired_transform.clone();
                }
            }
            if let Ok(current) = world.query_one_mut::<&mut Light>(*entity) {
                if *current != *light {
                    undo_stack.push(EditorCommand::Light {
                        entity: *entity,
                        old_light: current.clone(),
                        old_transform: desired_transform.clone(),
                    });
                    redo_stack.clear();
                    *current = light.clone();
                }
            }
        }
        InspectorTarget::Terrain { entity, terrain } => {
            let mut desired = terrain.clone();
            rebuild_terrain_material(&mut desired);
            if let Ok(current) = world.query_one_mut::<&mut Terrain>(*entity) {
                if *current != desired {
                    undo_stack.push(EditorCommand::Terrain {
                        entity: *entity,
                        old_terrain: current.clone(),
                    });
                    redo_stack.clear();
                    *current = desired;
                }
            }
        }
        InspectorTarget::Water { entity, water } => {
            if let Ok(current) = world.query_one_mut::<&mut Water>(*entity) {
                if *current != *water {
                    undo_stack.push(EditorCommand::Water {
                        entity: *entity,
                        old_water: current.clone(),
                    });
                    redo_stack.clear();
                    *current = water.clone();
                }
            }
        }
        InspectorTarget::Atmosphere { entity, atmosphere } => {
            if let Ok(current) = world.query_one_mut::<&mut Atmosphere>(*entity) {
                if *current != *atmosphere {
                    undo_stack.push(EditorCommand::Atmosphere {
                        entity: *entity,
                        old_atmosphere: current.clone(),
                    });
                    redo_stack.clear();
                    *current = atmosphere.clone();
                }
            }
        }
        InspectorTarget::Clouds { entity, clouds } => {
            if let Ok(current) = world.query_one_mut::<&mut Clouds>(*entity) {
                if *current != *clouds {
                    undo_stack.push(EditorCommand::Clouds {
                        entity: *entity,
                        old_clouds: current.clone(),
                    });
                    redo_stack.clear();
                    *current = clouds.clone();
                }
            }
        }
        InspectorTarget::GodRay { entity, god_ray } => {
            if let Ok(current) = world.query_one_mut::<&mut GodRay>(*entity) {
                if *current != *god_ray {
                    undo_stack.push(EditorCommand::GodRay {
                        entity: *entity,
                        old_god_ray: current.clone(),
                    });
                    redo_stack.clear();
                    *current = god_ray.clone();
                }
            }
        }
        InspectorTarget::Camera { entity, camera, .. } => {
            if let Ok(current) = world.query_one_mut::<&mut Camera>(*entity) {
                if *current != *camera {
                    undo_stack.push(EditorCommand::Camera {
                        entity: *entity,
                        old_camera: *current,
                    });
                    redo_stack.clear();
                    *current = *camera;
                }
            }
        }
    }
}

/// Apply an undo command, returning the command that would redo the change.
pub(crate) fn apply_undo(world: &mut World, cmd: &EditorCommand) -> EditorCommand {
    match cmd.clone() {
        EditorCommand::Transform {
            entity,
            old_transform,
        } => {
            let current = world
                .query_one_mut::<&mut Transform>(entity)
                .unwrap()
                .clone();
            *world.query_one_mut::<&mut Transform>(entity).unwrap() = old_transform;
            EditorCommand::Transform {
                entity,
                old_transform: current,
            }
        }
        EditorCommand::Material {
            entity,
            old_material,
        } => {
            let current = *world.query_one_mut::<&mut MaterialUniform>(entity).unwrap();
            *world.query_one_mut::<&mut MaterialUniform>(entity).unwrap() = old_material;
            EditorCommand::Material {
                entity,
                old_material: current,
            }
        }
        EditorCommand::Light {
            entity,
            old_light,
            old_transform,
        } => {
            let current_light = world.query_one_mut::<&mut Light>(entity).unwrap().clone();
            let current_transform = world
                .query_one_mut::<&mut Transform>(entity)
                .unwrap()
                .clone();
            *world.query_one_mut::<&mut Light>(entity).unwrap() = old_light;
            *world.query_one_mut::<&mut Transform>(entity).unwrap() = old_transform;
            EditorCommand::Light {
                entity,
                old_light: current_light,
                old_transform: current_transform,
            }
        }
        EditorCommand::Terrain {
            entity,
            old_terrain,
        } => {
            let current = world.query_one_mut::<&mut Terrain>(entity).unwrap().clone();
            *world.query_one_mut::<&mut Terrain>(entity).unwrap() = old_terrain;
            EditorCommand::Terrain {
                entity,
                old_terrain: current,
            }
        }
        EditorCommand::Water { entity, old_water } => {
            let current = world.query_one_mut::<&mut Water>(entity).unwrap().clone();
            *world.query_one_mut::<&mut Water>(entity).unwrap() = old_water;
            EditorCommand::Water {
                entity,
                old_water: current,
            }
        }
        EditorCommand::Atmosphere {
            entity,
            old_atmosphere,
        } => {
            let current = world
                .query_one_mut::<&mut Atmosphere>(entity)
                .unwrap()
                .clone();
            *world.query_one_mut::<&mut Atmosphere>(entity).unwrap() = old_atmosphere;
            EditorCommand::Atmosphere {
                entity,
                old_atmosphere: current,
            }
        }
        EditorCommand::Clouds { entity, old_clouds } => {
            let current = world.query_one_mut::<&mut Clouds>(entity).unwrap().clone();
            *world.query_one_mut::<&mut Clouds>(entity).unwrap() = old_clouds;
            EditorCommand::Clouds {
                entity,
                old_clouds: current,
            }
        }
        EditorCommand::GodRay {
            entity,
            old_god_ray,
        } => {
            let current = world.query_one_mut::<&mut GodRay>(entity).unwrap().clone();
            *world.query_one_mut::<&mut GodRay>(entity).unwrap() = old_god_ray;
            EditorCommand::GodRay {
                entity,
                old_god_ray: current,
            }
        }
        EditorCommand::Camera { entity, old_camera } => {
            let current = *world.query_one_mut::<&mut Camera>(entity).unwrap();
            *world.query_one_mut::<&mut Camera>(entity).unwrap() = old_camera;
            EditorCommand::Camera {
                entity,
                old_camera: current,
            }
        }
    }
}
