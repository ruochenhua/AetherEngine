//! Context-aware entity inspector for the Launcher editor.
//!
//! Mirrors UE's Details panel: the Inspector renders a different set of
//! editable fields depending on which Actor is selected (Mesh, Light, Terrain,
//! Water, Atmosphere, Clouds, GodRay).

mod apply;
mod helpers;
mod render;

pub(crate) use apply::{apply, apply_undo};

use aether_engine::ecs::components::{
    Atmosphere, Clouds, GodRay, Light, Terrain, Transform, Water,
};
use aether_engine::ecs::{Entity, World};
use aether_engine::renderer::renderable::MaterialUniform;

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
}

impl InspectorTarget {
    pub(crate) fn entity(&self) -> Entity {
        match *self {
            InspectorTarget::Mesh { entity, .. } => entity,
            InspectorTarget::Light { entity, .. } => entity,
            InspectorTarget::Terrain { entity, .. } => entity,
            InspectorTarget::Water { entity, .. } => entity,
            InspectorTarget::Atmosphere { entity, .. } => entity,
            InspectorTarget::Clouds { entity, .. } => entity,
            InspectorTarget::GodRay { entity, .. } => entity,
        }
    }
}

/// Extract an inspector target from the single `Selected` entity, if any.
pub(crate) fn extract(world: &World) -> Option<InspectorTarget> {
    let (entity, _) = world
        .query::<(Entity, &aether_engine::ecs::components::Selected)>()
        .iter()
        .next()?;

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

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
            .0
    }

    #[test]
    fn extract_prefers_mesh_over_light() {
        let device = headless_device();
        let mut world = World::new();
        let cpu_mesh = aether_engine::asset::mesh::CpuMesh::cube();
        let gpu_mesh = std::sync::Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(
            &device, &cpu_mesh,
        ));
        let entity = world.spawn((
            Transform::default(),
            aether_engine::ecs::components::MeshHandle::new(gpu_mesh, "cube"),
            MaterialUniform {
                albedo: [0.8, 0.3, 0.2, 1.0],
                roughness: 0.5,
                metallic: 0.0,
                _pad: [0.0, 0.0],
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
}
