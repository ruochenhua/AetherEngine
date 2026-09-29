use crate::scene::{PhysicsColliderShapeConfig, SceneDescription};

#[test]
fn object_physics_config_parses_body_and_collider_for_fixture_scenes() {
    let scene = SceneDescription::from_ron(
        r#"SceneDescription(
            name: "Physics Fixture",
            camera: (position: (0.0, 2.0, 8.0)),
            objects: [(
                name: "Falling Sphere",
                mesh: Builtin("sphere"),
                physics: Some((
                    body: (mass: 2.0, velocity: (0.0, -1.0, 0.0)),
                    colliders: [(
                        shape: Sphere(radius: 0.5),
                        friction: 0.8,
                    )],
                )),
            )],
        )"#,
    )
    .expect("physics fixture scene should parse");

    let physics = scene.objects[0]
        .physics
        .as_ref()
        .expect("object should have physics configuration");
    assert_eq!(physics.body.mass, 2.0);
    assert_eq!(physics.body.velocity, [0.0, -1.0, 0.0]);
    assert!(!physics.body.is_static);
    assert_eq!(physics.colliders.len(), 1);
    assert_eq!(physics.colliders[0].friction, 0.8);
}

#[test]
fn t6_physics_fixture_scenes_parse_with_expected_physics_objects() {
    for scene_file in [
        "t6_physics_stack.ron",
        "t6_physics_ramp.ron",
        "t6_physics_debug.ron",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenes")
            .join(scene_file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let scene = SceneDescription::from_ron(&source)
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
        let physics_objects = scene
            .objects
            .iter()
            .filter(|object| object.physics.is_some())
            .count();
        assert!(
            physics_objects >= 4,
            "{scene_file} needs visible physics cases"
        );
    }
}

#[test]
fn visible_physics_fixture_spheres_match_builtin_sphere_mesh_radius() {
    // CpuMesh::sphere is generated with a 0.5-unit radius. Physics applies
    // the object's uniform Transform scale to both that mesh and the collider.
    for scene_file in ["t6_physics_stack.ron", "t6_physics_ramp.ron"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenes")
            .join(scene_file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let scene = SceneDescription::from_ron(&source)
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));

        for object in scene.objects.iter().filter(|object| {
            object.name.starts_with("Sphere") || object.name.starts_with("RampSphere")
        }) {
            let physics = object.physics.as_ref().expect("fixture sphere has physics");
            let crate::scene::config::PhysicsColliderShapeConfig::Sphere { radius } =
                &physics.colliders[0].shape
            else {
                panic!("{} should use a sphere collider", object.name);
            };
            assert_eq!(*radius, 0.5, "{} collider radius", object.name);
            assert_eq!(object.transform.scale[0], object.transform.scale[1]);
            assert_eq!(object.transform.scale[1], object.transform.scale[2]);
        }
    }
}

#[test]
fn t6_physics_debug_floor_matches_visible_plane_bounds_and_height() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenes/t6_physics_debug.ron");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let scene = SceneDescription::from_ron(&source)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    let visual = scene
        .objects
        .iter()
        .find(|object| object.name == "GroundVisual")
        .expect("visible floor should exist");
    let collider = scene
        .objects
        .iter()
        .find(|object| object.name == "GroundCollider")
        .expect("physics floor should exist");
    let physics = collider.physics.as_ref().expect("floor has physics config");
    let PhysicsColliderShapeConfig::Box { half_extents } = &physics.colliders[0].shape else {
        panic!("floor collider should be a box");
    };

    assert!((visual.transform.translation[1] + 0.05).abs() < 1.0e-6);
    assert_eq!(visual.transform.scale[0], half_extents[0]);
    assert_eq!(visual.transform.scale[2], half_extents[2]);
    assert!((collider.transform.translation[1] + half_extents[1] + 0.05).abs() < 1.0e-6);
}
