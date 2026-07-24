//! Context-aware entity inspector for the Launcher editor.
//!
//! Mirrors UE's Details panel: the Inspector renders a different set of
//! editable fields depending on which Actor is selected (Mesh, Light, Terrain,
//! Water, Atmosphere, Clouds, GodRay).

mod apply;
mod helpers;
mod render;

pub(crate) use apply::{apply, apply_undo, snapshot_entity};

use aether_engine::ecs::components::{
    Atmosphere, Camera, Clouds, GodRay, Light, Terrain, Transform, Water,
};
use aether_engine::ecs::{Entity, World};
use aether_engine::renderer::renderable::MaterialUniform;

/// Snapshot of a despawned entity's components, used to undo deletions.
///
/// Every field mirrors one component type the editor can spawn; `None` means
/// the entity did not have that component. Restoring re-spawns a fresh entity
/// (hecs does not reuse despawned ids) and re-inserts the captured components.
#[derive(Clone)]
pub(crate) struct DeletedEntity {
    /// Entity id at snapshot time. Stale after a restore — always re-captured.
    pub(crate) entity: Entity,
    pub(crate) transform: Option<Transform>,
    pub(crate) mesh: Option<aether_engine::ecs::components::MeshHandle>,
    pub(crate) material: Option<MaterialUniform>,
    pub(crate) visibility: Option<aether_engine::ecs::components::Visibility>,
    pub(crate) name: Option<aether_engine::ecs::components::Name>,
    pub(crate) light: Option<Light>,
    pub(crate) camera: Option<Camera>,
    pub(crate) terrain: Option<Terrain>,
    pub(crate) water: Option<Water>,
    pub(crate) atmosphere: Option<Atmosphere>,
    pub(crate) clouds: Option<Clouds>,
    pub(crate) god_ray: Option<GodRay>,
    /// Whether the entity had the `Selected` marker when deleted.
    pub(crate) selected: bool,
}

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
        old_material: MaterialUniform,
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
    /// Restore a Clouds to a previous value.
    Clouds { entity: Entity, old_clouds: Clouds },
    /// Restore a GodRay to a previous value.
    GodRay { entity: Entity, old_god_ray: GodRay },
    /// Restore a Camera to a previous value.
    Camera { entity: Entity, old_camera: Camera },
    /// Entities deleted by the user. Applying the undo re-spawns them from
    /// the snapshots; the returned inverse command is `Restore`.
    Delete { entities: Vec<DeletedEntity> },
    /// Inverse of `Delete`: entities that were just restored by an undo.
    /// Applying it re-deletes them and returns a fresh `Delete` snapshot.
    Restore { entities: Vec<DeletedEntity> },
}

/// Editable snapshot of the currently selected entity.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub(crate) enum InspectorTarget {
    /// Renderable mesh object.
    Mesh {
        entity: Entity,
        transform: Transform,
        material: MaterialUniform,
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
}

impl InspectorTarget {
    #[allow(dead_code)] // used by unit tests
    pub(crate) fn entity(&self) -> Entity {
        match *self {
            InspectorTarget::Mesh { entity, .. } => entity,
            InspectorTarget::Light { entity, .. } => entity,
            InspectorTarget::Terrain { entity, .. } => entity,
            InspectorTarget::Water { entity, .. } => entity,
            InspectorTarget::Atmosphere { entity, .. } => entity,
            InspectorTarget::Clouds { entity, .. } => entity,
            InspectorTarget::GodRay { entity, .. } => entity,
            InspectorTarget::Camera { entity, .. } => entity,
        }
    }
}

/// Extract an inspector target from the first `Selected` entity, if any.
#[allow(dead_code)] // used by unit tests; the UI anchors via `extract_entity`
pub(crate) fn extract(world: &World) -> Option<InspectorTarget> {
    let (entity, _) = world
        .query::<(Entity, &aether_engine::ecs::components::Selected)>()
        .iter()
        .next()?;
    extract_entity(world, entity)
}

/// Extract an inspector target for a specific entity.
///
/// Used with multi-selection: the inspector shows the selection's anchor
/// (most recently selected entity) regardless of query iteration order.
pub(crate) fn extract_entity(world: &World, entity: Entity) -> Option<InspectorTarget> {
    // Mesh object: Transform + MeshHandle + MaterialUniform.
    let mut q = world.query_one::<(
        &Transform,
        &aether_engine::ecs::components::MeshHandle,
        &MaterialUniform,
    )>(entity);
    if let Ok((transform, _mesh, material)) = q.get() {
        let (ex, ey, ez) = transform.rotation.to_euler(glam::EulerRot::XYZ);
        return Some(InspectorTarget::Mesh {
            entity,
            transform: transform.clone(),
            material: *material,
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

    None
}

/// Render the inspector UI for the given target.
pub(crate) fn render(ui: &mut egui::Ui, target: &mut InspectorTarget) {
    render::render(ui, target);
}

#[cfg(test)]
mod tests {
    use super::helpers::{light_direction_to_rotation, light_rotation_to_direction};
    use super::*;
    use aether_engine::ecs::components::{Light, Selected, Terrain, Transform};
    use aether_engine::ecs::World;
    use aether_engine::renderer::light::LightType;
    use aether_engine::renderer::renderable::MaterialUniform;
    use glam::Vec3;

    fn world_with_light() -> (World, Entity) {
        let mut world = World::new();
        let entity = world.spawn((
            Transform::default(),
            Light {
                light_type: LightType::Directional,
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
                cast_shadow: true,
            },
            Selected,
        ));
        (world, entity)
    }

    #[test]
    fn extract_returns_light_for_selected_light_entity() {
        let (world, entity) = world_with_light();
        let target = extract(&world).expect("should extract light target");
        assert_eq!(target.entity(), entity);
        match target {
            InspectorTarget::Light { direction, .. } => {
                assert!(
                    (direction[1] + 1.0).abs() < 1e-4,
                    "default direction should be -Y"
                );
            }
            _ => panic!("expected Light target"),
        }
    }

    #[test]
    fn light_direction_roundtrip_through_rotation() {
        let direction = Vec3::new(0.2, -0.6, -0.8).normalize();
        let rotation = light_direction_to_rotation(direction);
        let recovered = light_rotation_to_direction(rotation);
        assert!((direction - recovered).length() < 1e-4);
    }

    #[test]
    fn apply_light_updates_transform_rotation() {
        let (mut world, entity) = world_with_light();
        let mut target = extract(&world).unwrap();
        match &mut target {
            InspectorTarget::Light {
                direction, light, ..
            } => {
                *direction = [0.0, 0.0, -1.0];
                light.intensity = 2.5;
            }
            _ => panic!("expected Light target"),
        }
        let mut undo = Vec::new();
        let mut redo = Vec::new();
        apply(&target, &mut world, &mut undo, &mut redo);

        let transform = world.query_one_mut::<&Transform>(entity).unwrap();
        let recovered = light_rotation_to_direction(transform.rotation);
        assert!((recovered - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-4);

        let light = world.query_one_mut::<&Light>(entity).unwrap();
        assert_eq!(light.intensity, 2.5);
    }

    #[test]
    fn apply_terrain_rebuilds_material_layers() {
        let mut world = World::new();
        let terrain = Terrain {
            source: aether_engine::scene::TerrainSource::Procedural {
                seed: 1,
                frequency: 0.05,
                amplitude: 32.0,
            },
            geometry: aether_engine::scene::TerrainGeometry::default(),
            material: aether_engine::asset::terrain_material::TerrainMaterial::default(),
            splatmap_path: None,
            layer_configs: vec![aether_engine::scene::TerrainLayerConfig {
                albedo: [1.0, 0.0, 0.0, 1.0],
                roughness: 0.1,
                metallic: 0.2,
                ..Default::default()
            }],
        };
        let entity = world.spawn((Transform::default(), terrain, Selected));

        let mut target = extract(&world).unwrap();
        match &mut target {
            InspectorTarget::Terrain { terrain, .. } => {
                terrain.layer_configs[0].albedo = [0.0, 1.0, 0.0, 1.0];
            }
            _ => panic!("expected Terrain target"),
        }
        let mut undo = Vec::new();
        let mut redo = Vec::new();
        apply(&target, &mut world, &mut undo, &mut redo);

        let terrain = world.query_one_mut::<&Terrain>(entity).unwrap();
        assert_eq!(terrain.material.layers[0].albedo, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(terrain.material.layers[0].roughness, 0.1);
    }

    fn world_with_camera() -> (World, Entity) {
        let mut world = World::new();
        let entity = world.spawn((
            Transform::default(),
            Camera {
                fov: 60.0f32.to_radians(),
                near: 0.1,
                far: 500.0,
                speed: 8.0,
            },
            Selected,
        ));
        (world, entity)
    }

    #[test]
    fn extract_returns_camera_for_selected_camera_entity() {
        let (world, entity) = world_with_camera();
        let target = extract(&world).expect("should extract camera target");
        assert_eq!(target.entity(), entity);
        match target {
            InspectorTarget::Camera {
                camera,
                fov_degrees,
                ..
            } => {
                assert!((camera.fov - 60.0f32.to_radians()).abs() < 1e-4);
                assert!((fov_degrees - 60.0).abs() < 1e-4);
                assert_eq!(camera.speed, 8.0);
                assert_eq!(camera.near, 0.1);
                assert_eq!(camera.far, 500.0);
            }
            _ => panic!("expected Camera target"),
        }
    }

    #[test]
    fn apply_camera_updates_component() {
        let (mut world, entity) = world_with_camera();
        let mut target = extract(&world).unwrap();
        match &mut target {
            InspectorTarget::Camera {
                camera,
                fov_degrees,
                ..
            } => {
                *fov_degrees = 90.0;
                camera.fov = 90.0f32.to_radians();
                camera.speed = 16.0;
                camera.near = 0.5;
                camera.far = 2000.0;
            }
            _ => panic!("expected Camera target"),
        }
        let mut undo = Vec::new();
        let mut redo = Vec::new();
        apply(&target, &mut world, &mut undo, &mut redo);

        let camera = world.query_one_mut::<&Camera>(entity).unwrap();
        assert!((camera.fov - 90.0f32.to_radians()).abs() < 1e-4);
        assert_eq!(camera.speed, 16.0);
        assert_eq!(camera.near, 0.5);
        assert_eq!(camera.far, 2000.0);
    }

    #[test]
    fn apply_camera_undo_restores_previous_values() {
        let (mut world, entity) = world_with_camera();
        let mut target = extract(&world).unwrap();
        match &mut target {
            InspectorTarget::Camera {
                camera,
                fov_degrees,
                ..
            } => {
                *fov_degrees = 90.0;
                camera.fov = 90.0f32.to_radians();
                camera.speed = 16.0;
            }
            _ => panic!("expected Camera target"),
        }
        let mut undo = Vec::new();
        let mut redo = Vec::new();
        apply(&target, &mut world, &mut undo, &mut redo);
        assert_eq!(undo.len(), 1);

        let redo = apply_undo(&mut world, &undo.pop().unwrap());
        let camera = world.query_one_mut::<&Camera>(entity).unwrap();
        assert!((camera.fov - 60.0f32.to_radians()).abs() < 1e-4);
        assert_eq!(camera.speed, 8.0);

        match redo {
            EditorCommand::Camera { old_camera, .. } => {
                assert!((old_camera.fov - 90.0f32.to_radians()).abs() < 1e-4);
                assert_eq!(old_camera.speed, 16.0);
            }
            _ => panic!("expected Camera undo command"),
        }
    }

    fn headless_device() -> Option<wgpu::Device> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;
        Some(
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("need device")
                .0,
        )
    }

    #[test]
    fn extract_prefers_mesh_over_light() {
        let Some(device) = headless_device() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let mut world = World::new();
        let cpu_mesh = aether_engine::asset::mesh::CpuMesh::cube();
        let gpu_mesh = std::sync::Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(
            &device, &cpu_mesh,
        ));
        let entity = world.spawn((
            Transform::default(),
            aether_engine::ecs::components::MeshHandle::new(
                gpu_mesh,
                aether_engine::ecs::components::MeshSource::Builtin("cube".into()),
                "cube",
            ),
            MaterialUniform {
                albedo: [0.8, 0.3, 0.2, 1.0],
                roughness: 0.5,
                metallic: 0.0,
                _pad: [0.0, 0.0],
                albedo_texture_id: 0,
            },
            Selected,
        ));
        let target = extract(&world).expect("should extract mesh target");
        assert_eq!(target.entity(), entity);
        match target {
            InspectorTarget::Mesh { .. } => {}
            _ => panic!("expected Mesh target"),
        }
    }

    #[test]
    fn extract_entity_uses_given_anchor_among_multiple_selected() {
        use aether_engine::ecs::components::Name;
        let mut world = World::new();
        let first = world.spawn((
            Transform::default(),
            Light {
                light_type: LightType::Directional,
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
                cast_shadow: true,
            },
            Name("First".into()),
            Selected,
        ));
        let second = world.spawn((
            Transform::default(),
            Light {
                light_type: LightType::Point,
                color: [1.0, 0.0, 0.0],
                intensity: 3.0,
                cast_shadow: false,
            },
            Name("Second".into()),
            Selected,
        ));

        // The inspector follows the explicit anchor, not query order.
        let target = extract_entity(&world, second).expect("should extract");
        assert_eq!(target.entity(), second);
        match target {
            InspectorTarget::Light { light, .. } => {
                assert_eq!(light.light_type, LightType::Point);
            }
            _ => panic!("expected Light target"),
        }
        let target = extract_entity(&world, first).expect("should extract");
        assert_eq!(target.entity(), first);
    }

    #[test]
    fn delete_undo_restores_multiple_entities_and_redo_deletes_again() {
        use aether_engine::ecs::components::Name;
        let mut world = World::new();
        let a = world.spawn((Transform::default(), Name("A".into()), Selected));
        let b = world.spawn((
            Transform {
                translation: Vec3::new(1.0, 2.0, 3.0),
                ..Default::default()
            },
            Name("B".into()),
            Selected,
        ));

        // Simulate the Delete key: snapshot every selected entity, despawn all.
        let cmd = EditorCommand::Delete {
            entities: vec![snapshot_entity(&world, a), snapshot_entity(&world, b)],
        };
        world.despawn(a).unwrap();
        world.despawn(b).unwrap();
        assert_eq!(world.len(), 0);

        // Undo: both come back with fresh ids, components and Selected intact.
        let redo_cmd = apply_undo(&mut world, &cmd);
        assert_eq!(world.len(), 2);
        let names: Vec<String> = world
            .query::<(&Name, &Selected)>()
            .iter()
            .map(|(name, _)| name.0.clone())
            .collect();
        assert!(names.contains(&"A".to_string()) && names.contains(&"B".to_string()));
        let restored_b_transform = world
            .query::<(&Name, &Transform)>()
            .iter()
            .find(|(name, _)| name.0 == "B")
            .map(|(_, t)| t.clone())
            .expect("B should be restored");
        assert_eq!(restored_b_transform.translation, Vec3::new(1.0, 2.0, 3.0));

        // Redo: deletes the restored entities again.
        let undo_cmd2 = apply_undo(&mut world, &redo_cmd);
        assert_eq!(world.len(), 0);

        // Undo again: restores once more (fresh snapshots were captured).
        let _ = apply_undo(&mut world, &undo_cmd2);
        assert_eq!(world.len(), 2);
    }

    #[test]
    fn undo_component_edit_with_stale_id_after_delete_restore_is_noop() {
        use aether_engine::ecs::components::Name;
        let mut world = World::new();
        let entity = world.spawn((Transform::default(), Name("A".into()), Selected));

        // Component-level edit: move the entity, recording the pre-edit
        // value in an old-style undo command referencing the original id.
        let moved = Transform {
            translation: Vec3::new(5.0, 0.0, 0.0),
            ..Default::default()
        };
        *world.query_one_mut::<&mut Transform>(entity).unwrap() = moved;
        let component_cmd = EditorCommand::Transform {
            entity,
            old_transform: Transform::default(),
        };

        // Delete the entity, then undo the deletion: it comes back with a
        // fresh id, so `component_cmd` now references a dead id.
        let delete_cmd = EditorCommand::Delete {
            entities: vec![snapshot_entity(&world, entity)],
        };
        world.despawn(entity).unwrap();
        let _restore_cmd = apply_undo(&mut world, &delete_cmd);
        assert_eq!(world.len(), 1);
        assert!(!world.contains(entity));

        // Undoing the older component edit must be a no-op, not a panic.
        let redo_cmd = apply_undo(&mut world, &component_cmd);

        // The restored entity keeps the transform from the delete snapshot.
        let restored = world
            .query::<&Transform>()
            .iter()
            .next()
            .expect("restored entity should exist")
            .clone();
        assert_eq!(restored.translation, Vec3::new(5.0, 0.0, 0.0));
        // The inverse command is the stale command returned unchanged.
        match redo_cmd {
            EditorCommand::Transform {
                entity: cmd_entity,
                old_transform,
            } => {
                assert_eq!(cmd_entity, entity);
                assert_eq!(old_transform, Transform::default());
            }
            _ => panic!("expected Transform command"),
        }
    }

    #[test]
    fn undo_component_edit_for_despawned_entity_is_noop() {
        let mut world = World::new();
        let entity = world.spawn((Transform::default(), Selected));

        let component_cmd = EditorCommand::Transform {
            entity,
            old_transform: Transform {
                translation: Vec3::new(1.0, 2.0, 3.0),
                ..Default::default()
            },
        };
        world.despawn(entity).unwrap();

        // The entity was never restored; undoing the edit must be a no-op,
        // not a panic.
        let redo_cmd = apply_undo(&mut world, &component_cmd);
        assert_eq!(world.len(), 0);
        match redo_cmd {
            EditorCommand::Transform {
                entity: cmd_entity,
                old_transform,
            } => {
                assert_eq!(cmd_entity, entity);
                assert_eq!(old_transform.translation, Vec3::new(1.0, 2.0, 3.0));
            }
            _ => panic!("expected Transform command"),
        }
    }

    #[test]
    fn undo_light_edit_still_swaps_light_and_transform() {
        let (mut world, entity) = world_with_light();

        // Current state: intensity 9.0 and a rotated transform. The undo
        // command restores intensity 1.0 and the default transform.
        let rotated = Transform {
            rotation: light_direction_to_rotation(Vec3::new(0.0, 0.0, -1.0)),
            ..Default::default()
        };
        *world.query_one_mut::<&mut Transform>(entity).unwrap() = rotated.clone();
        world.query_one_mut::<&mut Light>(entity).unwrap().intensity = 9.0;

        let cmd = EditorCommand::Light {
            entity,
            old_light: Light {
                light_type: LightType::Directional,
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
                cast_shadow: true,
            },
            old_transform: Transform::default(),
        };
        let redo_cmd = apply_undo(&mut world, &cmd);

        assert_eq!(world.query_one_mut::<&Light>(entity).unwrap().intensity, 1.0);
        assert_eq!(
            *world.query_one_mut::<&Transform>(entity).unwrap(),
            Transform::default()
        );
        match redo_cmd {
            EditorCommand::Light {
                old_light,
                old_transform,
                ..
            } => {
                assert_eq!(old_light.intensity, 9.0);
                assert_eq!(old_transform, rotated);
            }
            _ => panic!("expected Light command"),
        }
    }
}
