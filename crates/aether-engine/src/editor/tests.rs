use super::*;
use crate::ecs::components::{Light, Name, Transform, Visibility};
use crate::ecs::{Entity, World};
use crate::renderer::light::LightingUniforms;
use glam::Vec3;
use std::fs;
use std::path::{Path, PathBuf};

fn temp_paths(label: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("aether-editor-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    (root.join("scene.ron"), root.join("operations.jsonl"))
}

fn context<'a>(
    world: &'a mut World,
    lighting: &'a LightingUniforms,
    scene_path: &'a Path,
    log_path: &'a Path,
) -> EditorContext<'a> {
    EditorContext::new(world, lighting, "EditorTest", scene_path, log_path)
}

fn named_world(name: &str) -> (World, Entity) {
    let mut world = World::new();
    let entity = world.spawn((
        Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            ..Default::default()
        },
        Visibility(true),
        Light::default(),
        Name(name.into()),
    ));
    (world, entity)
}

#[test]
fn snapshot_is_schema_v2_and_excludes_runtime_entity_bits() {
    let (world, entity) = named_world("Sun");

    let snapshot = capture_snapshot(&world, entity).unwrap();

    assert_eq!(snapshot.schema_version, EDITOR_SCHEMA_VERSION);
    assert_eq!(snapshot.entity_name, "Sun");
    assert!(snapshot
        .records
        .iter()
        .any(|record| matches!(record, ComponentRecord::Transform { .. })));
    assert!(snapshot
        .records
        .iter()
        .any(|record| matches!(record, ComponentRecord::Visibility { visible: true })));
    let json = serde_json::to_string(&snapshot).unwrap();
    assert!(!json.contains("Entity"));
    assert!(!json.contains("entity_bits"));
}

#[test]
fn rename_persists_scene_and_operation_jsonl() {
    let (scene_path, log_path) = temp_paths("rename");
    let (mut world, entity) = named_world("Before");
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();

    history
        .apply(
            &mut context(&mut world, &lighting, &scene_path, &log_path),
            EditorOperation::Rename {
                entity_name: "Before".into(),
                new_name: "After".into(),
            },
        )
        .unwrap();

    assert_eq!(world.query_one::<&Name>(entity).get().unwrap().0, "After");
    assert!(scene_path.is_file());
    let log = fs::read_to_string(&log_path).unwrap();
    assert!(log.contains("schema_version"));
    assert!(log.contains("rename"));
    assert!(log.contains("After"));
}

#[test]
fn failed_save_rolls_back_world_and_history() {
    let (mut world, entity) = named_world("Before");
    let lighting = LightingUniforms::default();
    let root = std::env::temp_dir().join(format!("aether-editor-fail-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let scene_path = root.join("missing-parent").join("scene.ron");
    let log_path = root.join("missing-parent").join("operations.jsonl");
    let mut history = EditorHistory::default();

    let result = history.apply(
        &mut context(&mut world, &lighting, &scene_path, &log_path),
        EditorOperation::Rename {
            entity_name: "Before".into(),
            new_name: "After".into(),
        },
    );

    assert!(matches!(result, Err(EditorError::Io { .. })));
    assert_eq!(world.query_one::<&Name>(entity).get().unwrap().0, "Before");
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn failed_undo_save_preserves_world_and_history() {
    let (scene_path, log_path) = temp_paths("undo-fail");
    let (mut world, entity) = named_world("Before");
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    history
        .apply(
            &mut context(&mut world, &lighting, &scene_path, &log_path),
            EditorOperation::Rename {
                entity_name: "Before".into(),
                new_name: "After".into(),
            },
        )
        .unwrap();

    let bad_root = std::env::temp_dir().join(format!(
        "aether-editor-undo-fail-bad-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&bad_root);
    let bad_scene = bad_root.join("missing-parent").join("scene.ron");
    let bad_log = bad_root.join("missing-parent").join("operations.jsonl");
    let result = history.undo(&mut context(&mut world, &lighting, &bad_scene, &bad_log));

    assert!(matches!(result, Err(EditorError::Io { .. })));
    assert_eq!(world.query_one::<&Name>(entity).get().unwrap().0, "After");
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.redo_len(), 0);

    history
        .undo(&mut context(&mut world, &lighting, &scene_path, &log_path))
        .unwrap();
    let result = history.redo(&mut context(&mut world, &lighting, &bad_scene, &bad_log));
    assert!(matches!(result, Err(EditorError::Io { .. })));
    assert_eq!(world.query_one::<&Name>(entity).get().unwrap().0, "Before");
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 1);
}

#[test]
fn copy_delete_component_add_and_replay_are_deterministic() {
    let (scene_path, log_path) = temp_paths("replay");
    let (mut world, _) = named_world("Source");
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();

    history
        .apply(
            &mut context(&mut world, &lighting, &scene_path, &log_path),
            EditorOperation::Copy {
                source_name: "Source".into(),
                new_name: "Copy".into(),
            },
        )
        .unwrap();
    history
        .apply(
            &mut context(&mut world, &lighting, &scene_path, &log_path),
            EditorOperation::AddComponent {
                entity_name: "Copy".into(),
                component: ComponentRecord::Visibility { visible: false },
            },
        )
        .unwrap_err();
    history
        .apply(
            &mut context(&mut world, &lighting, &scene_path, &log_path),
            EditorOperation::Rename {
                entity_name: "Copy".into(),
                new_name: "EditedCopy".into(),
            },
        )
        .unwrap();
    let log = history.operation_log().to_jsonl().unwrap();
    let entries = OperationLog::from_jsonl(&log).unwrap();
    assert_eq!(entries.apply_count(), 2);

    history
        .undo(&mut context(&mut world, &lighting, &scene_path, &log_path))
        .unwrap();
    assert!(world.query::<&Name>().iter().any(|name| name.0 == "Copy"));
    history
        .redo(&mut context(&mut world, &lighting, &scene_path, &log_path))
        .unwrap();
    assert!(world
        .query::<&Name>()
        .iter()
        .any(|name| name.0 == "EditedCopy"));
}

#[test]
fn duplicate_component_and_name_fail_without_partial_ecs_write() {
    let (scene_path, log_path) = temp_paths("validation");
    let (mut world, entity) = named_world("Source");
    world.spawn((Transform::default(), Name("Other".into())));
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();

    let duplicate_component = history.apply(
        &mut context(&mut world, &lighting, &scene_path, &log_path),
        EditorOperation::AddComponent {
            entity_name: "Source".into(),
            component: ComponentRecord::Transform {
                translation: [9.0, 9.0, 9.0],
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
        },
    );
    assert!(matches!(
        duplicate_component,
        Err(EditorError::DuplicateComponent { .. })
    ));
    assert_eq!(
        world
            .query_one::<&Transform>(entity)
            .get()
            .unwrap()
            .translation,
        Vec3::new(1.0, 2.0, 3.0)
    );

    let duplicate_name = history.apply(
        &mut context(&mut world, &lighting, &scene_path, &log_path),
        EditorOperation::Rename {
            entity_name: "Source".into(),
            new_name: "Other".into(),
        },
    );
    assert!(matches!(
        duplicate_name,
        Err(EditorError::DuplicateName { .. })
    ));
    assert_eq!(world.query_one::<&Name>(entity).get().unwrap().0, "Source");
}

#[test]
fn delete_undo_redo_and_replay_preserve_state_hash() {
    let (scene_path, log_path) = temp_paths("delete-replay");
    let (mut world, _) = named_world("Source");
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();

    history
        .apply(
            &mut context(&mut world, &lighting, &scene_path, &log_path),
            EditorOperation::Delete {
                entity_name: "Source".into(),
            },
        )
        .unwrap();
    assert!(!world.query::<&Name>().iter().any(|name| name.0 == "Source"));
    let deleted_hash = EditorHistory::state_hash(&world).unwrap();
    history
        .undo(&mut context(&mut world, &lighting, &scene_path, &log_path))
        .unwrap();
    let restored_hash = EditorHistory::state_hash(&world).unwrap();
    assert!(world.query::<&Name>().iter().any(|name| name.0 == "Source"));
    history
        .redo(&mut context(&mut world, &lighting, &scene_path, &log_path))
        .unwrap();
    assert!(!world.query::<&Name>().iter().any(|name| name.0 == "Source"));

    let (mut replay_world, _) = named_world("Source");
    let entries = OperationLog::from_jsonl(&fs::read_to_string(&log_path).unwrap()).unwrap();
    let mut replay_history = EditorHistory::default();
    replay_history
        .replay(
            &mut context(&mut replay_world, &lighting, &scene_path, &log_path),
            &entries,
        )
        .unwrap();
    assert_eq!(
        deleted_hash,
        EditorHistory::state_hash(&replay_world).unwrap()
    );
    assert_ne!(restored_hash, deleted_hash);
}
