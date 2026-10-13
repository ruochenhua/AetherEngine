use super::{EditorContext, EditorError, EditorHistory, EditorOperation, OperationLog};
use crate::asset::mesh::{CpuMesh, GpuMesh};
use crate::ecs::components::{
    MaterialAssetRef, MaterialAssetStatus, MeshHandle, MeshSource, Name, Selected, Transform,
    Visibility,
};
use crate::ecs::{Entity, World};
use crate::renderer::extracted_material::ExtractedMaterial;
use crate::renderer::light::LightingUniforms;
use crate::renderer::renderable::MaterialUniform;
use crate::renderer::transparent::{TransparentBlendMode, TransparentMaterial};
use crate::scene::MaterialConfig;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn paths(label: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "aether-editor-material-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    (root.join("scene.ron"), root.join("scene.editor.jsonl"))
}

fn context<'a>(
    world: &'a mut World,
    lighting: &'a LightingUniforms,
    scene: &'a Path,
    log: &'a Path,
) -> EditorContext<'a> {
    EditorContext::new(world, lighting, "MaterialTest", scene, log)
}

fn headless_device() -> wgpu::Device {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter for a real MeshHandle");
    let (device, _queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device for a real MeshHandle");
    device
}

fn spawn_mesh(world: &mut World, device: &wgpu::Device, name: &str) -> Entity {
    let mesh = Arc::new(GpuMesh::from_cpu(device, &CpuMesh::cube()));
    world.spawn((
        Transform::default(),
        MeshHandle::new(mesh, MeshSource::Builtin("cube".into()), "cube"),
        MaterialConfig::default(),
        MaterialUniform::default(),
        ExtractedMaterial::from_legacy_uniform(MaterialUniform::default()),
        MaterialAssetRef("materials/old.ron".into()),
        MaterialAssetStatus(Some("old warning".into())),
        TransparentMaterial {
            base_color: [0.4, 0.3, 0.2, 0.8],
            texture: None,
            blend: TransparentBlendMode::Alpha,
            alpha_cutoff: Some(0.2),
        },
        Visibility(true),
        Selected,
        Name(name.into()),
    ))
}

fn linked_operation(name: &str, path: &str) -> EditorOperation {
    EditorOperation::SetMaterial {
        entity_name: name.into(),
        assignment: super::MaterialAssignment::Asset { path: path.into() },
    }
}

fn inline_operation(name: &str, material: MaterialConfig) -> EditorOperation {
    EditorOperation::SetMaterial {
        entity_name: name.into(),
        assignment: super::MaterialAssignment::Inline { material },
    }
}

fn prepared(
    material: MaterialConfig,
    asset_path: Option<&str>,
    marker: f32,
) -> super::PreparedMaterial {
    let uniform = MaterialUniform {
        albedo: [marker, 0.2, 0.3, 1.0],
        roughness: marker,
        albedo_texture_id: 51,
        ..MaterialUniform::default()
    };
    super::PreparedMaterial {
        material,
        uniform,
        extracted_material: ExtractedMaterial::from_legacy_uniform(uniform),
        material_asset: asset_path.map(|path| MaterialAssetRef(path.into())),
        material_asset_status: asset_path
            .map(|_| MaterialAssetStatus(Some("reload warning".into()))),
        transparent_material: Some(TransparentMaterial {
            base_color: [marker, 0.1, 0.2, 0.5],
            texture: None,
            blend: TransparentBlendMode::Additive,
            alpha_cutoff: Some(0.4),
        }),
    }
}

fn selection(world: &World) -> Vec<Entity> {
    let mut selected = world
        .query::<(Entity, &Selected)>()
        .iter()
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    selected.sort_by_key(|entity| entity.to_bits().get());
    selected
}

#[test]
fn prepared_material_apply_undo_redo_swaps_full_runtime_state_without_changing_selection() {
    let (scene, log) = paths("undo-redo");
    let device = headless_device();
    let mut world = World::new();
    let entity = spawn_mesh(&mut world, &device, "Mesh");
    let other = world.spawn((Name("Other".into()), Selected));
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    let before_selection = selection(&world);
    let before_material = world
        .query_one::<&MaterialConfig>(entity)
        .get()
        .unwrap()
        .clone();
    let before_uniform = *world.query_one::<&MaterialUniform>(entity).get().unwrap();
    let before_extracted = world
        .query_one::<&ExtractedMaterial>(entity)
        .get()
        .unwrap()
        .clone();
    let before_asset = world
        .query_one::<&MaterialAssetRef>(entity)
        .get()
        .unwrap()
        .clone();
    let before_status = world
        .query_one::<&MaterialAssetStatus>(entity)
        .get()
        .unwrap()
        .clone();
    let before_transparent = world
        .query_one::<&TransparentMaterial>(entity)
        .get()
        .unwrap()
        .clone();

    let target = MaterialConfig {
        albedo: [0.7, 0.5, 0.2, 1.0],
        albedo_texture: Some("textures/wood.png".into()),
        ..MaterialConfig::default()
    };
    let prepared = prepared(target.clone(), Some("materials/wood.ron"), 0.7);
    let operation = linked_operation("Mesh", "materials/wood.ron");
    history
        .apply_prepared_material(
            &mut context(&mut world, &lighting, &scene, &log),
            operation,
            prepared.clone(),
        )
        .unwrap();
    assert_eq!(
        world.query_one::<&MaterialConfig>(entity).get().unwrap(),
        &target
    );
    assert_eq!(
        world.query_one::<&MaterialUniform>(entity).get().unwrap(),
        &prepared.uniform
    );
    assert_eq!(
        world.query_one::<&ExtractedMaterial>(entity).get().unwrap(),
        &prepared.extracted_material
    );
    assert_eq!(
        world.query_one::<&MaterialAssetRef>(entity).get().unwrap(),
        prepared.material_asset.as_ref().unwrap()
    );
    assert_eq!(
        world
            .query_one::<&MaterialAssetStatus>(entity)
            .get()
            .unwrap(),
        prepared.material_asset_status.as_ref().unwrap()
    );
    assert_eq!(
        world
            .query_one::<&TransparentMaterial>(entity)
            .get()
            .unwrap(),
        prepared.transparent_material.as_ref().unwrap()
    );
    let saved_scene = fs::read_to_string(&scene).unwrap();
    assert!(saved_scene.contains("materials/wood.ron"));
    assert_eq!(
        OperationLog::from_jsonl(&fs::read_to_string(&log).unwrap())
            .unwrap()
            .apply_count(),
        1
    );
    assert_eq!(selection(&world), before_selection);

    history
        .undo(&mut context(&mut world, &lighting, &scene, &log))
        .unwrap();
    assert_eq!(
        world.query_one::<&MaterialConfig>(entity).get().unwrap(),
        &before_material
    );
    assert_eq!(
        world.query_one::<&MaterialUniform>(entity).get().unwrap(),
        &before_uniform
    );
    assert_eq!(
        world.query_one::<&ExtractedMaterial>(entity).get().unwrap(),
        &before_extracted
    );
    assert_eq!(
        world.query_one::<&MaterialAssetRef>(entity).get().unwrap(),
        &before_asset
    );
    assert_eq!(
        world
            .query_one::<&MaterialAssetStatus>(entity)
            .get()
            .unwrap(),
        &before_status
    );
    assert_eq!(
        world
            .query_one::<&TransparentMaterial>(entity)
            .get()
            .unwrap(),
        &before_transparent
    );
    assert_eq!(selection(&world), before_selection);

    history
        .redo(&mut context(&mut world, &lighting, &scene, &log))
        .unwrap();
    assert_eq!(
        world.query_one::<&MaterialConfig>(entity).get().unwrap(),
        &target
    );
    assert_eq!(
        world.query_one::<&MaterialUniform>(entity).get().unwrap(),
        &prepared.uniform
    );
    assert_eq!(
        world.query_one::<&ExtractedMaterial>(entity).get().unwrap(),
        &prepared.extracted_material
    );
    assert_eq!(
        world.query_one::<&MaterialAssetRef>(entity).get().unwrap(),
        prepared.material_asset.as_ref().unwrap()
    );
    assert_eq!(
        world
            .query_one::<&MaterialAssetStatus>(entity)
            .get()
            .unwrap(),
        prepared.material_asset_status.as_ref().unwrap()
    );
    assert_eq!(
        world
            .query_one::<&TransparentMaterial>(entity)
            .get()
            .unwrap(),
        prepared.transparent_material.as_ref().unwrap()
    );
    assert_eq!(selection(&world), before_selection);
    assert_eq!(history.operation_log().apply_count(), 1);
    assert!(world.contains(other));
}

#[path = "material_operation_tests/edge_cases.rs"]
mod edge_cases;

#[test]
fn invalid_material_preparation_and_generic_apply_fail_without_side_effects() {
    let (scene, log) = paths("validation");
    let device = headless_device();
    let mut world = World::new();
    let entity = spawn_mesh(&mut world, &device, "Mesh");
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    let original = world
        .query_one::<&MaterialConfig>(entity)
        .get()
        .unwrap()
        .clone();
    let original_uniform = *world.query_one::<&MaterialUniform>(entity).get().unwrap();
    let original_extracted = world
        .query_one::<&ExtractedMaterial>(entity)
        .get()
        .unwrap()
        .clone();
    let original_asset = world
        .query_one::<&MaterialAssetRef>(entity)
        .get()
        .unwrap()
        .clone();
    let original_status = world
        .query_one::<&MaterialAssetStatus>(entity)
        .get()
        .unwrap()
        .clone();
    let original_transparent = world
        .query_one::<&TransparentMaterial>(entity)
        .get()
        .unwrap()
        .clone();
    let operation = linked_operation("Mesh", "materials/wood.ron");
    let mismatch = prepared(MaterialConfig::default(), Some("materials/other.ron"), 0.7);
    assert!(matches!(
        history.apply_prepared_material(
            &mut context(&mut world, &lighting, &scene, &log),
            operation.clone(),
            mismatch
        ),
        Err(EditorError::InvalidMaterialOperation(_))
    ));
    assert!(matches!(
        history.apply(
            &mut context(&mut world, &lighting, &scene, &log),
            operation.clone()
        ),
        Err(EditorError::HostResolutionRequired)
    ));
    let replay_input = OperationLog::from_jsonl(
        &serde_json::to_string(&super::OperationLogEntry {
            schema_version: super::EDITOR_SCHEMA_VERSION,
            sequence: 0,
            operation,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        history.replay(
            &mut context(&mut world, &lighting, &scene, &log),
            &replay_input
        ),
        Err(EditorError::HostResolutionRequired)
    ));
    assert_eq!(
        world.query_one::<&MaterialConfig>(entity).get().unwrap(),
        &original
    );
    assert_eq!(
        world.query_one::<&MaterialUniform>(entity).get().unwrap(),
        &original_uniform
    );
    assert_eq!(
        world.query_one::<&ExtractedMaterial>(entity).get().unwrap(),
        &original_extracted
    );
    assert_eq!(
        world.query_one::<&MaterialAssetRef>(entity).get().unwrap(),
        &original_asset
    );
    assert_eq!(
        world
            .query_one::<&MaterialAssetStatus>(entity)
            .get()
            .unwrap(),
        &original_status
    );
    assert_eq!(
        world
            .query_one::<&TransparentMaterial>(entity)
            .get()
            .unwrap(),
        &original_transparent
    );
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.operation_log().apply_count(), 0);
    assert!(!scene.exists());
    assert!(!log.exists());
}

#[test]
fn missing_ambiguous_and_non_mesh_targets_reject_before_mutation() {
    let (scene, log) = paths("target-validation");
    let device = headless_device();
    let mut world = World::new();
    spawn_mesh(&mut world, &device, "Duplicate");
    spawn_mesh(&mut world, &device, "Duplicate");
    world.spawn((Name("NotMesh".into()),));
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    for name in ["Missing", "Duplicate", "NotMesh"] {
        let operation = inline_operation(name, MaterialConfig::default());
        let prepared = prepared(MaterialConfig::default(), None, 0.8);
        assert!(matches!(
            history.apply_prepared_material(
                &mut context(&mut world, &lighting, &scene, &log),
                operation,
                prepared
            ),
            Err(EditorError::EntityNotFound { .. })
                | Err(EditorError::DuplicateName { .. })
                | Err(EditorError::InvalidMaterialOperation(_))
        ));
    }
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.operation_log().apply_count(), 0);
}

#[test]
fn persistence_failure_restores_runtime_material_and_keeps_history_and_log_unchanged() {
    let (scene, log) = paths("persist-failure");
    fs::write(&scene, "previous scene").unwrap();
    fs::create_dir_all(&log).unwrap();
    let device = headless_device();
    let mut world = World::new();
    let entity = spawn_mesh(&mut world, &device, "Mesh");
    let before = world
        .query_one::<&MaterialConfig>(entity)
        .get()
        .unwrap()
        .clone();
    let before_uniform = *world.query_one::<&MaterialUniform>(entity).get().unwrap();
    let before_asset = world
        .query_one::<&MaterialAssetRef>(entity)
        .get()
        .unwrap()
        .clone();
    let before_status = world
        .query_one::<&MaterialAssetStatus>(entity)
        .get()
        .unwrap()
        .clone();
    let before_transparent = world
        .query_one::<&TransparentMaterial>(entity)
        .get()
        .unwrap()
        .clone();
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    let result = history.apply_prepared_material(
        &mut context(&mut world, &lighting, &scene, &log),
        inline_operation(
            "Mesh",
            MaterialConfig {
                albedo_texture: Some("textures/a.png".into()),
                ..MaterialConfig::default()
            },
        ),
        prepared(
            MaterialConfig {
                albedo_texture: Some("textures/a.png".into()),
                ..MaterialConfig::default()
            },
            None,
            0.9,
        ),
    );

    assert!(matches!(result, Err(EditorError::Io { .. })));
    assert_eq!(
        world.query_one::<&MaterialConfig>(entity).get().unwrap(),
        &before
    );
    assert_eq!(
        world.query_one::<&MaterialUniform>(entity).get().unwrap(),
        &before_uniform
    );
    assert_eq!(
        world.query_one::<&MaterialAssetRef>(entity).get().unwrap(),
        &before_asset
    );
    assert_eq!(
        world
            .query_one::<&MaterialAssetStatus>(entity)
            .get()
            .unwrap(),
        &before_status
    );
    assert_eq!(
        world
            .query_one::<&TransparentMaterial>(entity)
            .get()
            .unwrap(),
        &before_transparent
    );
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.operation_log().apply_count(), 0);
    assert_eq!(fs::read_to_string(&scene).unwrap(), "previous scene");
    assert!(log.is_dir());
}
