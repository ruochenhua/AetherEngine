//! Apply inspector changes back to the ECS world and undo/redo support.

use super::helpers::{light_direction_to_rotation, rebuild_terrain_material};
use super::{DeletedEntity, EditorCommand, InspectorTarget};
use aether_engine::ecs::components::{
    Atmosphere, Camera, Clouds, GodRay, Light, MeshHandle, Name, Selected, Terrain, Transform,
    Visibility, Water,
};
use aether_engine::ecs::{Entity, World};
use aether_engine::renderer::renderable::MaterialUniform;
use glam::{Quat, Vec3};
use tracing::warn;

/// Capture all editor-relevant components of an entity into a snapshot.
pub(crate) fn snapshot_entity(world: &World, entity: Entity) -> DeletedEntity {
    // hecs::Component is blanket-implemented for any Send + Sync + 'static
    // type; the launcher has no direct hecs dependency, so bound on that.
    fn get<T: Clone + Send + Sync + 'static>(world: &World, entity: Entity) -> Option<T> {
        world.query_one::<&T>(entity).get().ok().cloned()
    }
    DeletedEntity {
        entity,
        transform: get::<Transform>(world, entity),
        mesh: get::<MeshHandle>(world, entity),
        material: get::<MaterialUniform>(world, entity),
        visibility: get::<Visibility>(world, entity),
        name: get::<Name>(world, entity),
        light: get::<Light>(world, entity),
        camera: get::<Camera>(world, entity),
        terrain: get::<Terrain>(world, entity),
        water: get::<Water>(world, entity),
        atmosphere: get::<Atmosphere>(world, entity),
        clouds: get::<Clouds>(world, entity),
        god_ray: get::<GodRay>(world, entity),
        selected: world.query_one::<&Selected>(entity).get().is_ok(),
    }
}

/// Re-spawn an entity from a snapshot, returning the new entity id.
///
/// hecs does not recycle despawned ids, so the restored entity always gets a
/// fresh id; callers must propagate it into the inverse undo command.
fn respawn_snapshot(world: &mut World, snapshot: &DeletedEntity) -> Entity {
    let entity = world.spawn(());
    if let Some(transform) = &snapshot.transform {
        let _ = world.insert(entity, (transform.clone(),));
    }
    if let Some(mesh) = &snapshot.mesh {
        let _ = world.insert(entity, (mesh.clone(),));
    }
    if let Some(material) = &snapshot.material {
        let _ = world.insert(entity, (*material,));
    }
    if let Some(visibility) = &snapshot.visibility {
        let _ = world.insert(entity, (*visibility,));
    }
    if let Some(name) = &snapshot.name {
        let _ = world.insert(entity, (name.clone(),));
    }
    if let Some(light) = &snapshot.light {
        let _ = world.insert(entity, (light.clone(),));
    }
    if let Some(camera) = &snapshot.camera {
        let _ = world.insert(entity, (*camera,));
    }
    if let Some(terrain) = &snapshot.terrain {
        let _ = world.insert(entity, (terrain.clone(),));
    }
    if let Some(water) = &snapshot.water {
        let _ = world.insert(entity, (water.clone(),));
    }
    if let Some(atmosphere) = &snapshot.atmosphere {
        let _ = world.insert(entity, (atmosphere.clone(),));
    }
    if let Some(clouds) = &snapshot.clouds {
        let _ = world.insert(entity, (clouds.clone(),));
    }
    if let Some(god_ray) = &snapshot.god_ray {
        let _ = world.insert(entity, (god_ray.clone(),));
    }
    if snapshot.selected {
        let _ = world.insert(entity, (Selected,));
    }
    entity
}

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
///
/// Component-level commands reference an entity id that may be stale: the
/// entity can be deleted after the command was recorded, and hecs never
/// recycles ids — even undoing that delete respawns a *new* id. A stale
/// command is skipped as a no-op (and returned unchanged) instead of
/// panicking.
pub(crate) fn apply_undo(world: &mut World, cmd: &EditorCommand) -> EditorCommand {
    match cmd.clone() {
        EditorCommand::Transform {
            entity,
            old_transform,
        } => {
            let Ok(current) = world.query_one_mut::<&mut Transform>(entity) else {
                warn!(
                    ?entity,
                    "skipping Transform undo: entity or component no longer exists"
                );
                return EditorCommand::Transform {
                    entity,
                    old_transform,
                };
            };
            let current = std::mem::replace(current, old_transform);
            EditorCommand::Transform {
                entity,
                old_transform: current,
            }
        }
        EditorCommand::Material {
            entity,
            old_material,
        } => {
            let Ok(current) = world.query_one_mut::<&mut MaterialUniform>(entity) else {
                warn!(
                    ?entity,
                    "skipping Material undo: entity or component no longer exists"
                );
                return EditorCommand::Material {
                    entity,
                    old_material,
                };
            };
            let current = std::mem::replace(current, old_material);
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
            // A light actor always carries both components; fetch them in one
            // query so a stale command stays an all-or-nothing no-op.
            let Ok((light, transform)) =
                world.query_one_mut::<(&mut Light, &mut Transform)>(entity)
            else {
                warn!(
                    ?entity,
                    "skipping Light undo: entity or component no longer exists"
                );
                return EditorCommand::Light {
                    entity,
                    old_light,
                    old_transform,
                };
            };
            let current_light = std::mem::replace(light, old_light);
            let current_transform = std::mem::replace(transform, old_transform);
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
            let Ok(current) = world.query_one_mut::<&mut Terrain>(entity) else {
                warn!(
                    ?entity,
                    "skipping Terrain undo: entity or component no longer exists"
                );
                return EditorCommand::Terrain {
                    entity,
                    old_terrain,
                };
            };
            let current = std::mem::replace(current, old_terrain);
            EditorCommand::Terrain {
                entity,
                old_terrain: current,
            }
        }
        EditorCommand::Water { entity, old_water } => {
            let Ok(current) = world.query_one_mut::<&mut Water>(entity) else {
                warn!(
                    ?entity,
                    "skipping Water undo: entity or component no longer exists"
                );
                return EditorCommand::Water { entity, old_water };
            };
            let current = std::mem::replace(current, old_water);
            EditorCommand::Water {
                entity,
                old_water: current,
            }
        }
        EditorCommand::Atmosphere {
            entity,
            old_atmosphere,
        } => {
            let Ok(current) = world.query_one_mut::<&mut Atmosphere>(entity) else {
                warn!(
                    ?entity,
                    "skipping Atmosphere undo: entity or component no longer exists"
                );
                return EditorCommand::Atmosphere {
                    entity,
                    old_atmosphere,
                };
            };
            let current = std::mem::replace(current, old_atmosphere);
            EditorCommand::Atmosphere {
                entity,
                old_atmosphere: current,
            }
        }
        EditorCommand::Clouds { entity, old_clouds } => {
            let Ok(current) = world.query_one_mut::<&mut Clouds>(entity) else {
                warn!(
                    ?entity,
                    "skipping Clouds undo: entity or component no longer exists"
                );
                return EditorCommand::Clouds {
                    entity,
                    old_clouds,
                };
            };
            let current = std::mem::replace(current, old_clouds);
            EditorCommand::Clouds {
                entity,
                old_clouds: current,
            }
        }
        EditorCommand::GodRay {
            entity,
            old_god_ray,
        } => {
            let Ok(current) = world.query_one_mut::<&mut GodRay>(entity) else {
                warn!(
                    ?entity,
                    "skipping GodRay undo: entity or component no longer exists"
                );
                return EditorCommand::GodRay {
                    entity,
                    old_god_ray,
                };
            };
            let current = std::mem::replace(current, old_god_ray);
            EditorCommand::GodRay {
                entity,
                old_god_ray: current,
            }
        }
        EditorCommand::Camera { entity, old_camera } => {
            let Ok(current) = world.query_one_mut::<&mut Camera>(entity) else {
                warn!(
                    ?entity,
                    "skipping Camera undo: entity or component no longer exists"
                );
                return EditorCommand::Camera {
                    entity,
                    old_camera,
                };
            };
            let current = std::mem::replace(current, old_camera);
            EditorCommand::Camera {
                entity,
                old_camera: current,
            }
        }
        EditorCommand::Delete { entities } => {
            // Undo of a deletion: re-spawn every entity from its snapshot.
            // The inverse command references the new entity ids so a redo
            // deletes exactly what was restored.
            let restored = entities
                .iter()
                .map(|snapshot| DeletedEntity {
                    entity: respawn_snapshot(world, snapshot),
                    ..snapshot.clone()
                })
                .collect();
            EditorCommand::Restore {
                entities: restored,
            }
        }
        EditorCommand::Restore { entities } => {
            // Redo of a deletion: despawn the restored entities again,
            // capturing fresh snapshots so a later undo restores the state
            // as it was just before this redo.
            let mut deleted = Vec::with_capacity(entities.len());
            for snapshot in entities {
                if world.contains(snapshot.entity) {
                    deleted.push(snapshot_entity(world, snapshot.entity));
                    let _ = world.despawn(snapshot.entity);
                }
            }
            EditorCommand::Delete { entities: deleted }
        }
    }
}
