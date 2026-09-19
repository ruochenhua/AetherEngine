use super::*;

#[test]
fn serialize_world_preserves_object_visibility() {
    let device = headless_device();
    let registry = BuiltinMeshRegistry::new();
    let mut world = World::new();
    spawn_object_entity(&mut world, &device, &registry, "HiddenCube", "cube");
    let entity = world
        .query::<(crate::ecs::Entity, &Name)>()
        .iter()
        .find_map(|(entity, name)| (name.0 == "HiddenCube").then_some(entity))
        .unwrap();
    world.query_one_mut::<&mut Visibility>(entity).unwrap().0 = false;

    let desc = serialize_world(&world, &LightingUniforms::default(), "VisibilityScene");

    assert!(!desc.objects[0].visible);
}
