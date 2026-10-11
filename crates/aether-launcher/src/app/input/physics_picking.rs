//! Prefer physics hits for editor picking, retaining render-AABB fallback.

use aether_engine::{
    ecs::{
        components::{MeshHandle, Selected},
        Entity, World,
    },
    physics::{PhysicsRuntime, QueryFilter},
    renderer::picking::{pick_entity as pick_render_entity, Ray},
};

pub(super) fn pick_scene_entity(world: &mut World, physics: Option<&PhysicsRuntime>, ray: &Ray) {
    if let Some(runtime) = physics {
        if let Ok(Some(hit)) =
            runtime.cast_ray(ray, 10_000.0, QueryFilter::default().exclude_sensors())
        {
            if let Some(entity) =
                Entity::from_bits(hit.entity_bits).filter(|entity| world.contains(*entity))
            {
                let has_render_mesh = world.get::<&MeshHandle>(entity).is_ok();
                if has_render_mesh {
                    let selected = world
                        .query::<(Entity, &Selected)>()
                        .iter()
                        .map(|(entity, _)| entity)
                        .collect::<Vec<_>>();
                    for selected_entity in selected {
                        let _ = world.remove::<(Selected,)>(selected_entity);
                    }
                    let _ = world.insert(entity, (Selected,));
                    return;
                }
            }
        }
    }
    pick_render_entity(world, ray);
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_engine::{
        asset::mesh::{CpuMesh, GpuMesh},
        ecs::components::{MeshSource, Transform},
        physics::{Collider, ColliderShape, RigidBody},
    };
    use glam::Vec3;
    use std::sync::Arc;

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("physics picking test needs a headless GPU adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("physics picking test needs a headless GPU device")
            .0
    }

    fn add_renderable(world: &mut World, mesh: &Arc<GpuMesh>, z: f32) -> Entity {
        world.spawn((
            Transform {
                translation: Vec3::new(0.0, 0.0, z),
                ..Transform::default()
            },
            MeshHandle::new(Arc::clone(mesh), MeshSource::Builtin("cube".into()), "cube"),
        ))
    }

    fn spawn_collider(runtime: &mut PhysicsRuntime, entity: Entity, z: f32, radius: f32) {
        runtime
            .spawn_body(
                entity.to_bits().get(),
                &Transform {
                    translation: Vec3::new(0.0, 0.0, z),
                    ..Transform::default()
                },
                &RigidBody::default(),
                &Collider {
                    shape: ColliderShape::Sphere(radius),
                    ..Collider::default()
                },
            )
            .expect("picking collider should spawn");
    }

    fn ray_from_front() -> Ray {
        Ray {
            origin: Vec3::new(0.0, 0.0, 3.0),
            dir: -Vec3::Z,
        }
    }

    fn selected_entities(world: &World) -> Vec<Entity> {
        world
            .query::<(Entity, &Selected)>()
            .iter()
            .map(|(entity, _)| entity)
            .collect()
    }

    #[test]
    fn physics_pick_uses_nearest_collider_and_replaces_old_selection() {
        let device = headless_device();
        let mesh = Arc::new(GpuMesh::from_cpu(&device, &CpuMesh::cube()));
        let mut world = World::new();
        // The second visual AABB is closer, but its physics collider is small.
        let physics_hit = add_renderable(&mut world, &mesh, -3.0);
        let render_hit = add_renderable(&mut world, &mesh, 0.0);
        world.insert(render_hit, (Selected,)).unwrap();

        let mut runtime = PhysicsRuntime::default();
        spawn_collider(&mut runtime, physics_hit, -3.0, 3.5);
        spawn_collider(&mut runtime, render_hit, 0.0, 0.1);

        pick_scene_entity(&mut world, Some(&runtime), &ray_from_front());

        assert_eq!(selected_entities(&world), vec![physics_hit]);
    }

    #[test]
    fn physics_pick_ignores_sensor_and_selects_solid_behind_it() {
        let device = headless_device();
        let mesh = Arc::new(GpuMesh::from_cpu(&device, &CpuMesh::cube()));
        let mut world = World::new();
        let sensor = add_renderable(&mut world, &mesh, 1.0);
        let solid = add_renderable(&mut world, &mesh, 0.0);
        let mut runtime = PhysicsRuntime::default();
        let mut sensor_collider = Collider {
            shape: ColliderShape::Sphere(0.5),
            ..Collider::default()
        };
        sensor_collider.is_trigger = true;
        runtime
            .spawn_body(
                sensor.to_bits().get(),
                &Transform {
                    translation: Vec3::Z,
                    ..Transform::default()
                },
                &RigidBody::default(),
                &sensor_collider,
            )
            .unwrap();
        spawn_collider(&mut runtime, solid, 0.0, 0.5);

        pick_scene_entity(&mut world, Some(&runtime), &ray_from_front());

        assert_eq!(selected_entities(&world), vec![solid]);
    }

    #[test]
    fn missing_physics_hit_falls_back_to_render_bounds() {
        let device = headless_device();
        let mesh = Arc::new(GpuMesh::from_cpu(&device, &CpuMesh::cube()));
        let mut world = World::new();
        let target = add_renderable(&mut world, &mesh, 0.0);
        let runtime = PhysicsRuntime::default();

        pick_scene_entity(&mut world, Some(&runtime), &ray_from_front());

        assert_eq!(selected_entities(&world), vec![target]);
    }

    #[test]
    fn absent_physics_runtime_uses_render_bounds() {
        let device = headless_device();
        let mesh = Arc::new(GpuMesh::from_cpu(&device, &CpuMesh::cube()));
        let mut world = World::new();
        let target = add_renderable(&mut world, &mesh, 0.0);

        pick_scene_entity(&mut world, None, &ray_from_front());

        assert_eq!(selected_entities(&world), vec![target]);
    }
}
