use super::*;

#[test]
fn prepared_material_accepts_missing_config_and_undo_restores_absence() {
    let (scene, log) = paths("missing-config");
    let device = headless_device();
    let mut world = World::new();
    let entity = spawn_mesh(&mut world, &device, "Cube");
    world.remove::<(MaterialConfig,)>(entity).unwrap();
    let old_uniform = *world.query_one::<&MaterialUniform>(entity).get().unwrap();
    let old_extracted = world
        .query_one::<&ExtractedMaterial>(entity)
        .get()
        .unwrap()
        .clone();
    let old_asset = world
        .query_one::<&MaterialAssetRef>(entity)
        .get()
        .unwrap()
        .clone();
    let old_status = world
        .query_one::<&MaterialAssetStatus>(entity)
        .get()
        .unwrap()
        .clone();
    let old_transparent = world
        .query_one::<&TransparentMaterial>(entity)
        .get()
        .unwrap()
        .clone();
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    let material = MaterialConfig {
        albedo_texture: Some("textures/new.png".into()),
        ..MaterialConfig::default()
    };
    history
        .apply_prepared_material(
            &mut context(&mut world, &lighting, &scene, &log),
            inline_operation("Cube", material.clone()),
            prepared(material.clone(), None, 0.85),
        )
        .unwrap();
    assert_eq!(
        world.query_one::<&MaterialConfig>(entity).get().unwrap(),
        &material
    );

    history
        .undo(&mut context(&mut world, &lighting, &scene, &log))
        .unwrap();
    assert!(world.query_one::<&MaterialConfig>(entity).get().is_err());
    assert_eq!(
        world.query_one::<&MaterialUniform>(entity).get().unwrap(),
        &old_uniform
    );
    assert_eq!(
        world.query_one::<&ExtractedMaterial>(entity).get().unwrap(),
        &old_extracted
    );
    assert_eq!(
        world.query_one::<&MaterialAssetRef>(entity).get().unwrap(),
        &old_asset
    );
    assert_eq!(
        world
            .query_one::<&MaterialAssetStatus>(entity)
            .get()
            .unwrap(),
        &old_status
    );
    assert_eq!(
        world
            .query_one::<&TransparentMaterial>(entity)
            .get()
            .unwrap(),
        &old_transparent
    );

    history
        .redo(&mut context(&mut world, &lighting, &scene, &log))
        .unwrap();
    assert_eq!(
        world.query_one::<&MaterialConfig>(entity).get().unwrap(),
        &material
    );
}

#[test]
fn material_undo_resolves_restored_entity_after_delete_undo_changes_generation() {
    let (scene, log) = paths("delete-undo-remap");
    let device = headless_device();
    let mut world = World::new();
    let original_entity = spawn_mesh(&mut world, &device, "Cube");
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    let original_material = world
        .query_one::<&MaterialConfig>(original_entity)
        .get()
        .unwrap()
        .clone();
    let assigned = MaterialConfig {
        albedo_texture: Some("textures/assigned.png".into()),
        ..MaterialConfig::default()
    };

    history
        .apply_prepared_material(
            &mut context(&mut world, &lighting, &scene, &log),
            inline_operation("Cube", assigned.clone()),
            prepared(assigned, None, 0.75),
        )
        .unwrap();
    history
        .apply(
            &mut context(&mut world, &lighting, &scene, &log),
            EditorOperation::Delete {
                entity_name: "Cube".into(),
            },
        )
        .unwrap();
    assert!(!world.contains(original_entity));

    history
        .undo(&mut context(&mut world, &lighting, &scene, &log))
        .unwrap();
    let restored_entity = world
        .query::<(Entity, &Name)>()
        .iter()
        .find_map(|(entity, name)| (name.0 == "Cube").then_some(entity))
        .unwrap();
    assert_ne!(restored_entity, original_entity);
    assert_eq!(
        world
            .query_one::<&MaterialConfig>(restored_entity)
            .get()
            .unwrap()
            .albedo_texture,
        Some("textures/assigned.png".into())
    );

    history
        .undo(&mut context(&mut world, &lighting, &scene, &log))
        .unwrap();
    assert_eq!(
        world
            .query_one::<&MaterialConfig>(restored_entity)
            .get()
            .unwrap(),
        &original_material
    );
    assert_eq!(history.redo_len(), 2);
}

#[test]
fn rename_delete_undo_order_keeps_material_inverse_unambiguous() {
    let (scene, log) = paths("rename-delete-undo-order");
    let device = headless_device();
    let mut world = World::new();
    let original_entity = spawn_mesh(&mut world, &device, "Cube");
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    let original_material = world
        .query_one::<&MaterialConfig>(original_entity)
        .get()
        .unwrap()
        .clone();
    let assigned = MaterialConfig {
        albedo_texture: Some("textures/assigned.png".into()),
        ..MaterialConfig::default()
    };
    history
        .apply_prepared_material(
            &mut context(&mut world, &lighting, &scene, &log),
            inline_operation("Cube", assigned.clone()),
            prepared(assigned, None, 0.75),
        )
        .unwrap();
    history
        .apply(
            &mut context(&mut world, &lighting, &scene, &log),
            EditorOperation::Rename {
                entity_name: "Cube".into(),
                new_name: "RenamedCube".into(),
            },
        )
        .unwrap();
    history
        .apply(
            &mut context(&mut world, &lighting, &scene, &log),
            EditorOperation::Delete {
                entity_name: "RenamedCube".into(),
            },
        )
        .unwrap();
    history
        .undo(&mut context(&mut world, &lighting, &scene, &log))
        .unwrap();
    let restored_entity = world
        .query::<(Entity, &Name)>()
        .iter()
        .find_map(|(entity, name)| (name.0 == "RenamedCube").then_some(entity))
        .unwrap();
    assert_ne!(restored_entity, original_entity);

    history
        .undo(&mut context(&mut world, &lighting, &scene, &log))
        .unwrap();
    assert_eq!(
        world.query_one::<&Name>(restored_entity).get().unwrap().0,
        "Cube"
    );
    history
        .undo(&mut context(&mut world, &lighting, &scene, &log))
        .unwrap();
    assert_eq!(
        world
            .query_one::<&MaterialConfig>(restored_entity)
            .get()
            .unwrap(),
        &original_material
    );
    assert_eq!(history.redo_len(), 3);
}
