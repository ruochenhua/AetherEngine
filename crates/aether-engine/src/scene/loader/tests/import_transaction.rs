use super::*;
use crate::ecs::components::Name;

#[test]
fn failed_scene_import_rolls_back_objects_spawned_before_the_failure() {
    let device = headless_device();
    let registry = test_registry();
    let mut world = World::new();
    let original = world.spawn((Transform::default(), Name("ExistingCube".into())));
    let mut assets = test_assets();

    let dir = std::env::temp_dir().join("aether_test_import_scene_transaction");
    let _ = std::fs::create_dir(&dir);
    let path = dir.join("import_scene.ron");
    std::fs::write(
        &path,
        r#"SceneDescription(
    name: "Import",
    camera: (position: (0.0, 0.0, 0.0)),
    lights: [],
    objects: [
        (name: "ImportedBeforeFailure", mesh: Builtin("cube"), transform: (translation: (0.0, 0.0, 0.0)), material: (albedo: (1.0, 1.0, 1.0, 1.0))),
        (name: "InvalidMesh", mesh: Builtin("does-not-exist"), transform: (translation: (1.0, 0.0, 0.0)), material: (albedo: (1.0, 1.0, 1.0, 1.0))),
    ],
)"#,
    )
    .unwrap();

    let result = SceneLoader::import_scene(&path, &device, &registry, &mut assets, &mut world);

    assert!(
        result.is_err(),
        "the second object's unknown mesh must fail import"
    );
    assert!(
        world.contains(original),
        "existing entity identity must be preserved"
    );
    assert_eq!(
        world.len(),
        1,
        "entities imported before failure must be removed"
    );
    let mut name_query = world.query::<&Name>();
    let names: Vec<_> = name_query.iter().map(|name| name.0.clone()).collect();
    assert_eq!(names, vec!["ExistingCube".to_string()]);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}
