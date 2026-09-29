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
