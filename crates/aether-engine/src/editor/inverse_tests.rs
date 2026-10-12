use super::{EditorContext, EditorHistory, EditorOperation};
use crate::ecs::components::{
    MaterialAssetRef, MaterialAssetStatus, Name, Selected, Transform, Visibility,
};
use crate::ecs::{Entity, World};
use crate::renderer::{
    extracted_material::ExtractedMaterial, light::LightingUniforms, renderable::MaterialUniform,
};
use crate::scene::MaterialConfig;
use std::fs;
use std::path::{Path, PathBuf};

fn paths(label: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "aether-editor-inverse-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    (root.join("scene.ron"), root.join("scene.editor.jsonl"))
}

fn apply_context<'a>(
    world: &'a mut World,
    lighting: &'a LightingUniforms,
    scene: &'a Path,
    log: &'a Path,
) -> EditorContext<'a> {
    EditorContext::new(world, lighting, "Inverse", scene, log)
}

fn selected_entities(world: &World) -> Vec<Entity> {
    world
        .query::<(Entity, &Selected)>()
        .iter()
        .map(|(entity, _)| entity)
        .collect()
}

#[test]
fn delete_undo_redo_restores_complete_material_runtime_state() {
    let (scene, log) = paths("material-roundtrip");
    let mut world = World::new();
    let material = MaterialConfig {
        albedo: [0.2, 0.3, 0.4, 1.0],
        roughness: 0.17,
        metallic: 0.73,
        albedo_texture: Some("textures/paint.png".into()),
        ..MaterialConfig::default()
    };
    let uniform = MaterialUniform {
        albedo: material.albedo,
        roughness: material.roughness,
        metallic: material.metallic,
        ..MaterialUniform::default()
    };
    let entity = world.spawn((
        Transform::default(),
        Visibility::default(),
        MaterialAssetRef("materials/paint.ron".into()),
        MaterialAssetStatus(Some("last reload warning".into())),
        material.clone(),
        uniform,
        ExtractedMaterial::from_legacy_uniform(uniform),
        Selected,
        Name("PaintedMesh".into()),
    ));
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    history
        .apply(
            &mut apply_context(&mut world, &lighting, &scene, &log),
            EditorOperation::Delete {
                entity_name: "PaintedMesh".into(),
            },
        )
        .unwrap();
    assert!(!world.contains(entity));

    for _ in 0..2 {
        history
            .undo(&mut apply_context(&mut world, &lighting, &scene, &log))
            .unwrap();
        let restored = world
            .query::<(Entity, &Name)>()
            .iter()
            .find_map(|(entity, name)| (name.0 == "PaintedMesh").then_some(entity))
            .unwrap();
        assert_eq!(
            world.query_one::<&MaterialConfig>(restored).get().unwrap(),
            &material
        );
        assert_eq!(
            world
                .query_one::<&MaterialAssetRef>(restored)
                .get()
                .unwrap()
                .0,
            "materials/paint.ron"
        );
        assert_eq!(
            world
                .query_one::<&MaterialAssetStatus>(restored)
                .get()
                .unwrap()
                .0
                .as_deref(),
            Some("last reload warning")
        );
        assert_eq!(
            world
                .query_one::<&ExtractedMaterial>(restored)
                .get()
                .unwrap(),
            &ExtractedMaterial::from_legacy_uniform(uniform)
        );
        assert_eq!(
            world.query_one::<&MaterialUniform>(restored).get().unwrap(),
            &uniform
        );
        assert!(world.query_one::<&Selected>(restored).get().is_ok());
        history
            .redo(&mut apply_context(&mut world, &lighting, &scene, &log))
            .unwrap();
        assert_eq!(world.query::<&Name>().iter().count(), 0);
    }
}

#[test]
fn copying_selected_entity_keeps_only_the_source_selected() {
    let (scene, log) = paths("copy-selection");
    let mut world = World::new();
    let source = world.spawn((
        Name("Source".into()),
        MaterialAssetRef("materials/paint.ron".into()),
        MaterialAssetStatus::default(),
        MaterialConfig::default(),
        MaterialUniform::default(),
        ExtractedMaterial::from_legacy_uniform(MaterialUniform::default()),
        Selected,
    ));
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();

    history
        .apply(
            &mut apply_context(&mut world, &lighting, &scene, &log),
            EditorOperation::Copy {
                source_name: "Source".into(),
                new_name: "Copy".into(),
            },
        )
        .unwrap();

    assert_eq!(selected_entities(&world), vec![source]);
    let copy = world
        .query::<(Entity, &Name)>()
        .iter()
        .find_map(|(entity, name)| (name.0 == "Copy").then_some(entity))
        .unwrap();
    assert_eq!(
        world.query_one::<&MaterialAssetRef>(copy).get().unwrap().0,
        "materials/paint.ron"
    );
    assert!(world.query_one::<&ExtractedMaterial>(copy).get().is_ok());
}

#[test]
fn undo_delete_preserves_a_newer_selection() {
    let (scene, log) = paths("delete-selection");
    let mut world = World::new();
    let deleted = world.spawn((Name("Deleted".into()), Selected));
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();

    history
        .apply(
            &mut apply_context(&mut world, &lighting, &scene, &log),
            EditorOperation::Delete {
                entity_name: "Deleted".into(),
            },
        )
        .unwrap();
    assert!(!world.contains(deleted));
    let newer_selection = world.spawn((Name("NewerSelection".into()), Selected));

    history
        .undo(&mut apply_context(&mut world, &lighting, &scene, &log))
        .unwrap();

    assert_eq!(selected_entities(&world), vec![newer_selection]);
}
