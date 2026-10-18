use super::resource_usage::*;
use super::*;
use crate::ecs::components::{MaterialAssetRef, MeshSource, Selected};
use crate::editor::{
    MaterialAssignment, OperationLogEntry, PreparedMaterial, PreparedObject,
    WorldResourceDiagnostic, WorldResourceKind, EDITOR_SCHEMA_VERSION,
};
use crate::renderer::extracted_material::ExtractedMaterial;
use crate::renderer::light::LightingUniforms;
use crate::scene::{MaterialConfig, MeshRef, ObjectConfig, TransparentMaterialConfig};
use std::fs;
use std::path::{Path, PathBuf};

#[path = "resource_usage_tests/support.rs"]
mod support;
use support::*;

#[test]
fn history_resource_usage_log_visits_place_material_and_component_operations() {
    let mut linked = object();
    linked.material_asset = Some("materials/./linked.ron".into());
    let log = log(vec![
        EditorOperation::PlaceObject { object: linked },
        EditorOperation::SetMaterial {
            entity_name: "Mesh".into(),
            assignment: MaterialAssignment::Inline {
                material: material(),
            },
        },
        EditorOperation::SetMaterial {
            entity_name: "Mesh".into(),
            assignment: MaterialAssignment::Asset {
                path: "materials/linked.ron".into(),
            },
        },
        EditorOperation::AddComponent {
            entity_name: "Mesh".into(),
            component: ComponentRecord::Material { config: material() },
        },
        EditorOperation::AddComponent {
            entity_name: "Mesh".into(),
            component: ComponentRecord::Mesh {
                source: MeshRef::File("meshes/cube.obj".into()),
            },
        },
    ]);
    let report = log.resource_usage(root());
    assert_eq!(report.len(), 19);
    let place = &report[..7];
    assert_slots(place, "operation.object.material.");
    let mesh = place
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::Mesh)
        .unwrap();
    assert_eq!(mesh.authored_path, "meshes/./cube.obj");
    assert_eq!(candidate(mesh), "meshes/cube.obj");
    let albedo = place
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::AlbedoTexture)
        .unwrap();
    // ObjectConfig's shadowed inline fields remain project-root relative;
    // they are not the linked document's resolved runtime MaterialConfig.
    assert_eq!(candidate(albedo), "textures/same.png");
    assert_slots(&report[7..12], "operation.assignment.Inline.material.");
    assert_eq!(report[12].source.field, "operation.assignment.Asset.path");
    assert_slots(&report[13..18], "operation.component.Material.config.");
    assert_eq!(
        report[18].source.field,
        "operation.component.Mesh.source.File"
    );
    assert!(report
        .iter()
        .all(|entry| entry.source.owner == HistoryResourceOwner::CommittedLog));
    assert!(report
        .iter()
        .all(|entry| entry.source.entity_name.as_deref() == Some("Mesh")));
    assert!(report[..7].iter().all(|entry| entry.source.index == 0
        && entry.source.sequence == Some(42)
        && entry.source.operation == "place_object"));
}

#[test]
fn history_resource_usage_place_undo_redo_keeps_operation_and_inverse_owners() {
    let (scene, target) = paths("place");
    let mut world = World::new();
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    let object = object();
    history
        .apply_prepared(
            &mut context(&mut world, &lighting, &scene, &target),
            EditorOperation::PlaceObject {
                object: object.clone(),
            },
            PreparedObject {
                object,
                mesh: mesh(),
                material: MaterialUniform::default(),
                extracted_material: ExtractedMaterial::from_legacy_uniform(
                    MaterialUniform::default(),
                ),
            },
        )
        .unwrap();
    let applied = history.resource_usage(root());
    assert_eq!(applied.len(), 12);
    assert!(applied[..6]
        .iter()
        .all(|entry| entry.source.owner == HistoryResourceOwner::Undo));
    assert!(applied[6..]
        .iter()
        .all(|entry| entry.source.owner == HistoryResourceOwner::CommittedLog));
    assert_eq!(
        applied[..6]
            .iter()
            .map(|e| &e.authored_path)
            .collect::<Vec<_>>(),
        applied[6..]
            .iter()
            .map(|e| &e.authored_path)
            .collect::<Vec<_>>()
    );
    assert_ne!(applied[0].source, applied[6].source);
    assert_eq!(history.operation_log().resource_usage(root()), applied[6..]);

    history
        .undo(&mut context(&mut world, &lighting, &scene, &target))
        .unwrap();
    let undone = history.resource_usage(root());
    assert_eq!(undone.len(), 12);
    assert!(undone
        .iter()
        .all(|entry| entry.source.owner == HistoryResourceOwner::Redo
            && entry.source.index == 0
            && entry.source.sequence.is_none()));
    assert_slots(&undone, "inverse.Restore.material_config.");
    assert_eq!(
        undone
            .iter()
            .filter(|entry| entry.kind == WorldResourceKind::Mesh)
            .count(),
        2
    );
    assert!(history.operation_log().resource_usage(root()).is_empty());
    assert!(world.query::<&Name>().iter().next().is_none());
    history
        .redo(&mut context(&mut world, &lighting, &scene, &target))
        .unwrap();
    assert_eq!(history.resource_usage(root()), applied);
}

#[test]
fn history_resource_usage_material_inverse_uses_linked_base_across_undo_redo() {
    let (scene, target) = paths("material");
    let mut world = World::new();
    world.spawn((
        Name("Mesh".into()),
        Transform::default(),
        Visibility(true),
        mesh(),
        MaterialUniform::default(),
        material(),
        MaterialAssetRef("materials/old.ron".into()),
    ));
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    history
        .apply_prepared_material(
            &mut context(&mut world, &lighting, &scene, &target),
            EditorOperation::SetMaterial {
                entity_name: "Mesh".into(),
                assignment: MaterialAssignment::Inline {
                    material: material(),
                },
            },
            prepared(material(), None),
        )
        .unwrap();
    let applied = history.resource_usage(root());
    assert_eq!(applied.len(), 16);
    assert_slots(&applied, "inverse.Material.material.");
    let inverse = applied
        .iter()
        .find(|entry| entry.source.field == "inverse.Material.material.albedo_texture")
        .unwrap();
    assert_eq!(candidate(inverse), "materials/textures/same.png");
    assert!(applied.iter().any(
        |entry| entry.source.field == "inverse.Material.material_asset.0"
            && candidate(entry) == "materials/old.ron"
    ));
    history
        .undo(&mut context(&mut world, &lighting, &scene, &target))
        .unwrap();
    let undone = history.resource_usage(root());
    assert_eq!(undone.len(), 10);
    assert!(undone
        .iter()
        .all(|entry| entry.source.owner == HistoryResourceOwner::Redo));
    let inverse = undone
        .iter()
        .find(|entry| entry.source.field == "inverse.Material.material.albedo_texture")
        .unwrap();
    assert_eq!(candidate(inverse), "textures/same.png");
    history
        .redo(&mut context(&mut world, &lighting, &scene, &target))
        .unwrap();
    assert_eq!(history.resource_usage(root()), applied);
}

#[test]
fn history_resource_usage_delete_retains_runtime_restore_and_diagnoses_invalid_base() {
    let (scene, target) = paths("delete");
    let mut world = World::new();
    world.spawn((
        Name("Mesh".into()),
        Transform::default(),
        Visibility(true),
        mesh(),
        MaterialUniform::default(),
        material(),
        MaterialAssetRef("../../escape.ron".into()),
    ));
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    history
        .apply(
            &mut context(&mut world, &lighting, &scene, &target),
            EditorOperation::Delete {
                entity_name: "Mesh".into(),
            },
        )
        .unwrap();
    let report = history.resource_usage(root());
    assert_eq!(report.len(), 7);
    assert_slots(&report, "inverse.Restore.material_config.");
    assert!(report
        .iter()
        .filter(|entry| entry.kind != WorldResourceKind::Mesh
            && entry.kind != WorldResourceKind::MaterialAsset)
        .all(|entry| entry.project_relative == Err(WorldResourceDiagnostic::InvalidMaterialBase)));
    let linked = report
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::MaterialAsset)
        .unwrap();
    assert_eq!(
        linked.project_relative,
        Err(WorldResourceDiagnostic::OutsideProjectRoot)
    );
    assert!(report
        .iter()
        .all(|entry| entry.source.entity_name.as_deref() == Some("Mesh")
            && entry.source.operation == "delete"));
    history
        .undo(&mut context(&mut world, &lighting, &scene, &target))
        .unwrap();
    assert!(history.resource_usage(root()).is_empty());
    history
        .redo(&mut context(&mut world, &lighting, &scene, &target))
        .unwrap();
    assert_eq!(history.resource_usage(root()), report);
}

#[test]
fn history_resource_usage_invalid_paths_are_retained_with_loader_diagnostics() {
    let mut obj = object();
    obj.mesh = MeshRef::File("../mesh.obj".into());
    obj.material_asset = Some("".into());
    obj.material = MaterialConfig {
        albedo_texture: Some("".into()),
        normal_texture: Some("../../outside.png".into()),
        orm_texture: Some("bad\0path".into()),
        emissive_texture: Some("textures/../ok.png".into()),
        transparent: Some(TransparentMaterialConfig {
            texture: Some("textures\\glass.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let report = log(vec![EditorOperation::PlaceObject { object: obj }]).resource_usage(root());
    assert_eq!(report.len(), 7);
    let result = |kind| {
        report
            .iter()
            .find(|entry| entry.kind == kind)
            .unwrap()
            .project_relative
            .clone()
    };
    assert_eq!(
        result(WorldResourceKind::Mesh),
        Err(WorldResourceDiagnostic::OutsideProjectRoot)
    );
    assert_eq!(
        result(WorldResourceKind::MaterialAsset),
        Err(WorldResourceDiagnostic::InvalidPath)
    );
    assert_eq!(
        result(WorldResourceKind::AlbedoTexture),
        Err(WorldResourceDiagnostic::InvalidPath)
    );
    assert_eq!(
        result(WorldResourceKind::NormalTexture),
        Err(WorldResourceDiagnostic::OutsideProjectRoot)
    );
    assert_eq!(
        result(WorldResourceKind::OrmTexture),
        Err(WorldResourceDiagnostic::InvalidPath)
    );
    assert_eq!(
        result(WorldResourceKind::EmissiveTexture).unwrap().as_str(),
        "ok.png"
    );
    if cfg!(unix) {
        assert_eq!(
            result(WorldResourceKind::TransparentTexture),
            Err(WorldResourceDiagnostic::AmbiguousTextureSeparators)
        );
    }
}

#[test]
fn history_resource_usage_deterministic_order_preserves_duplicate_log_records() {
    let operation = EditorOperation::SetMaterial {
        entity_name: "same".into(),
        assignment: MaterialAssignment::Inline {
            material: material(),
        },
    };
    let log = log(vec![operation.clone(), operation]);
    let report = log.resource_usage(root());
    assert_eq!(report.len(), 10);
    assert_eq!(report, log.resource_usage(root()));
    assert_slots(&report[..5], "operation.assignment.Inline.material.");
    assert_slots(&report[5..], "operation.assignment.Inline.material.");
    assert!(report[..5]
        .iter()
        .all(|entry| entry.source.index == 0 && entry.source.sequence == Some(42)));
    assert!(report[5..]
        .iter()
        .all(|entry| entry.source.index == 1 && entry.source.sequence == Some(43)));
    assert_eq!(candidate(&report[0]), candidate(&report[2]));
    assert_ne!(report[0].source, report[2].source);
    assert_ne!(report[0].source, report[5].source);
}

#[test]
fn history_resource_usage_is_observational_for_history_world_and_persisted_targets() {
    let (scene, target) = paths("observe");
    let mut world = World::new();
    world.spawn((
        Name("Before".into()),
        Transform::default(),
        Visibility(true),
        Selected,
        material(),
    ));
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    history
        .apply(
            &mut context(&mut world, &lighting, &scene, &target),
            EditorOperation::Delete {
                entity_name: "Before".into(),
            },
        )
        .unwrap();
    let state = EditorHistory::state_sha256(&world).unwrap();
    let saved_log = history.operation_log().clone();
    let scene_meta = fs::metadata(&scene).unwrap();
    let log_meta = fs::metadata(&target).unwrap();
    for _ in 0..3 {
        let report = history.resource_usage(&scene.with_file_name("absent-project-root"));
        assert_eq!(report.len(), 5);
    }
    assert_eq!((history.undo_len(), history.redo_len()), (1, 0));
    assert_eq!(history.operation_log(), &saved_log);
    assert_eq!(EditorHistory::state_sha256(&world).unwrap(), state);
    assert_eq!(
        fs::metadata(&scene).unwrap().modified().unwrap(),
        scene_meta.modified().unwrap()
    );
    assert_eq!(
        fs::metadata(&target).unwrap().modified().unwrap(),
        log_meta.modified().unwrap()
    );
    assert_eq!(fs::metadata(&target).unwrap().len(), log_meta.len());
    history
        .undo(&mut context(&mut world, &lighting, &scene, &target))
        .unwrap();
    let restored = world
        .query::<&MaterialConfig>()
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(restored, vec![material()]);
    assert_eq!(world.query::<&Selected>().iter().count(), 1);
}

#[path = "resource_usage_tests/edge_cases.rs"]
mod edge_cases;
