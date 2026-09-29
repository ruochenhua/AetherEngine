use super::*;
use crate::physics::{ColliderList, RigidBody};

#[test]
fn build_world_attaches_scene_physics_to_the_renderable_entity() {
    let device = headless_device();
    let registry = test_registry();
    let mut assets = AssetManager::new();
    let mut world = World::new();
    let mut desc = test_scene_desc();
    desc.objects.push(ObjectConfig {
        name: "Physics Cube".into(),
        mesh: MeshRef::Builtin("cube".into()),
        transform: Default::default(),
        material: Default::default(),
        visible: true,
        physics: Some(crate::scene::PhysicsConfig {
            body: crate::scene::PhysicsBodyConfig {
                mass: 2.0,
                velocity: [1.0, 0.0, 0.0],
                ..Default::default()
            },
            colliders: vec![crate::scene::PhysicsColliderConfig {
                shape: crate::scene::PhysicsColliderShapeConfig::Box {
                    half_extents: [0.5; 3],
                },
                is_trigger: false,
                friction: 0.7,
                restitution: 0.0,
            }],
        }),
    });

    SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world)
        .expect("physics object should load");

    let mut query = world.query::<(&RigidBody, &ColliderList)>();
    let (body, colliders) = query
        .iter()
        .next()
        .expect("physics components should be on the visible object");
    assert_eq!(body.mass, 2.0);
    assert_eq!(body.velocity.x, 1.0);
    assert_eq!(colliders.0.len(), 1);
    assert_eq!(colliders.0[0].friction, 0.7);
}

#[test]
fn build_world_rejects_invalid_physics_at_the_scene_boundary() {
    let device = headless_device();
    let registry = test_registry();
    let mut assets = AssetManager::new();
    let mut world = World::new();
    let mut desc = test_scene_desc();
    desc.objects[0].name = "InvalidBody".into();
    desc.objects[0].physics = Some(crate::scene::PhysicsConfig {
        body: crate::scene::PhysicsBodyConfig {
            mass: 0.0,
            ..Default::default()
        },
        colliders: vec![crate::scene::PhysicsColliderConfig {
            shape: crate::scene::PhysicsColliderShapeConfig::Sphere { radius: 0.5 },
            is_trigger: false,
            friction: 0.5,
            restitution: 0.0,
        }],
    });

    let error = SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world)
        .expect_err("invalid physics data must fail while loading");
    assert!(error.to_string().contains("InvalidBody"));
}
