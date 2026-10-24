use super::*;
use crate::{
    app::{
        cli::parse_args,
        workspace::{
            AssetDragPayload, MaterialDropRequest, MaterialDropTarget, ProjectFileAction,
            ViewportDropRequest,
        },
        App, PendingAssetImport, SceneEntry,
    },
    project_index::ResourceKind,
};
use aether_engine::{
    ecs::World,
    editor::EditorOperation,
    scene::{MaterialConfig, MeshRef, ObjectConfig, TransformConfig},
};
use std::path::PathBuf;

fn fixture() -> App {
    let mut app = App::new(parse_args(&["aether-launcher".into()]).unwrap());
    app.workspace.set_project_root(PathBuf::from("/project"));
    app
}

#[test]
fn app_request_snapshot_covers_each_path_slot_without_consuming_requests() {
    let mut app = fixture();
    app.scene_entries = vec![SceneEntry {
        name: "Start".into(),
        path: PathBuf::from("/project/scenes/start.ron"),
    }];
    app.pending_load = Some(0);
    app.pending_project_open = Some(PathBuf::from("/outside/scenes/next.ron"));
    app.pending_project_file = Some(ProjectFileAction::ImportAsset {
        destination: "assets/models".into(),
    });
    app.pending_asset_import = Some(PendingAssetImport {
        destination: "assets/models".into(),
        sources: vec![PathBuf::from("/external/model.obj")],
        requires_confirmation: true,
    });
    app.pending_asset_drop = Some(ViewportDropRequest {
        relative_path: "assets/models/ship.obj".into(),
        pointer_physical: [0.0; 2],
        viewport_points_min: [0.0; 2],
        viewport_points_max: [1.0; 2],
        pixels_per_point: 1.0,
    });
    let entity = World::new().spawn(());
    app.pending_material_drop = Some(MaterialDropRequest {
        entity,
        target: MaterialDropTarget::Texture(aether_engine::scene::material::TextureUsage::Albedo),
        resource: AssetDragPayload {
            relative_path: "assets/textures/a.png".into(),
            kind: ResourceKind::Image,
        },
    });
    let material = MaterialConfig {
        albedo_texture: Some("assets/shared.png".into()),
        normal_texture: Some("../escape.png".into()),
        orm_texture: Some("assets/orm.png".into()),
        emissive_texture: Some("assets/emissive.png".into()),
        transparent: Some(aether_engine::scene::TransparentMaterialConfig {
            texture: Some("assets/transparent.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    app.pending_editor_operation = Some(EditorOperation::PlaceObject {
        object: ObjectConfig {
            name: "Ship".into(),
            mesh: MeshRef::File("assets/ship.obj".into()),
            material,
            material_asset: Some("assets/shared.png".into()),
            visible: true,
            transform: TransformConfig::default(),
            physics: None,
        },
    });

    let snapshot = capture_app_request_resource_usage(&app);
    assert_eq!(
        snapshot.len(),
        14,
        "every path-bearing App slot and operation field must be reported"
    );
    assert!(snapshot
        .iter()
        .any(|row| row.source.owner == AppRequestOwner::SceneSelection
            && row.authored_path.as_deref() == Some(Path::new("/project/scenes/start.ron"))));
    assert!(snapshot
        .iter()
        .any(|row| row.source.owner == AppRequestOwner::AssetImport
            && row.project_relative == Err(AppRequestDiagnostic::OutsideProjectRoot)));
    assert!(snapshot.iter().any(|row| row.authored_path.as_deref()
        == Some(Path::new("../escape.png"))
        && row.project_relative == Err(AppRequestDiagnostic::OutsideProjectRoot)));
    assert!(snapshot
        .iter()
        .any(|row| row.source.owner == AppRequestOwner::ProjectOpen
            && row.authored_path.as_deref() == Some(Path::new("/outside/scenes/next.ron"))
            && row.project_relative == Err(AppRequestDiagnostic::OutsideProjectRoot)));
    assert_eq!(
        snapshot
            .iter()
            .filter(|row| row.authored_path.as_deref() == Some(Path::new("assets/shared.png")))
            .count(),
        2,
        "equal path spellings in distinct semantic slots remain distinct"
    );
    for slot in [
        "operation.object.material.albedo_texture",
        "operation.object.material.normal_texture",
        "operation.object.material.orm_texture",
        "operation.object.material.emissive_texture",
        "operation.object.material.transparent.texture",
    ] {
        assert!(
            snapshot.iter().any(|row| row.source.slot == slot),
            "missing operation field {slot}"
        );
    }
    assert!(snapshot
        .iter()
        .any(|row| row.source.owner == AppRequestOwner::ViewportAssetDrop
            && row.kind == Some(AppRequestResourceKind::Mesh)));
    assert!(snapshot
        .iter()
        .any(|row| row.source.owner == AppRequestOwner::MaterialDrop
            && row.kind == Some(AppRequestResourceKind::Texture)));
    assert_eq!(
        capture_app_request_resource_usage(&app),
        snapshot,
        "snapshot must be deterministic and repeatable"
    );
    assert!(
        app.pending_load.is_some()
            && app.pending_project_open.is_some()
            && app.pending_asset_import.is_some()
    );
    assert!(matches!(
        app.pending_editor_operation,
        Some(EditorOperation::PlaceObject { .. })
    ));
}

#[test]
fn app_request_snapshot_marks_invalid_scene_index_unknown() {
    let mut app = fixture();
    app.pending_load = Some(4);
    let snapshot = capture_app_request_resource_usage(&app);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].source.owner, AppRequestOwner::SceneSelection);
    assert_eq!(snapshot[0].authored_path, None);
    assert_eq!(
        snapshot[0].project_relative,
        Err(AppRequestDiagnostic::UnknownRequest)
    );
}

#[test]
fn app_request_snapshot_visits_folder_destinations_and_other_operation_variants() {
    let mut app = fixture();
    app.pending_project_file = Some(ProjectFileAction::CreateFolder {
        destination: "assets/new-folder".into(),
        name: "child".into(),
    });
    app.pending_editor_operation = Some(EditorOperation::SetMaterial {
        entity_name: "Mesh".into(),
        assignment: aether_engine::editor::MaterialAssignment::Inline {
            material: MaterialConfig {
                albedo_texture: Some("assets/set-material.png".into()),
                ..MaterialConfig::default()
            },
        },
    });
    let snapshot = capture_app_request_resource_usage(&app);
    assert!(snapshot
        .iter()
        .any(|row| row.source.slot == "create_folder.destination"
            && row.authored_path.as_deref() == Some(Path::new("assets/new-folder"))));
    assert!(snapshot.iter().any(|row| row.source.slot
        == "operation.assignment.Inline.material.albedo_texture"
        && row.kind == Some(AppRequestResourceKind::Texture)));

    app.pending_editor_operation = Some(EditorOperation::AddComponent {
        entity_name: "Mesh".into(),
        component: aether_engine::editor::ComponentRecord::Mesh {
            source: MeshRef::File("assets/component.obj".into()),
        },
    });
    let snapshot = capture_app_request_resource_usage(&app);
    assert!(snapshot.iter().any(
        |row| row.source.slot == "operation.component.Mesh.source.File"
            && row.authored_path.as_deref() == Some(Path::new("assets/component.obj"))
    ));
}

#[test]
fn app_request_snapshot_marks_embedded_nul_paths_invalid() {
    let mut app = fixture();
    app.pending_editor_operation = Some(EditorOperation::SetMaterial {
        entity_name: "Mesh".into(),
        assignment: aether_engine::editor::MaterialAssignment::Asset {
            path: "assets/bad\0name.mat.ron".into(),
        },
    });

    let snapshot = capture_app_request_resource_usage(&app);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(
        snapshot[0].authored_path.as_deref(),
        Some(Path::new("assets/bad\0name.mat.ron"))
    );
    assert_eq!(
        snapshot[0].project_relative,
        Err(AppRequestDiagnostic::InvalidPath)
    );
}

#[test]
fn app_request_snapshot_classifies_uppercase_obj_drop_as_mesh() {
    let mut app = fixture();
    app.pending_asset_drop = Some(crate::app::workspace::ViewportDropRequest {
        relative_path: "assets/models/ship.OBJ".into(),
        pointer_physical: [0.0; 2],
        viewport_points_min: [0.0; 2],
        viewport_points_max: [1.0; 2],
        pixels_per_point: 1.0,
    });

    let snapshot = capture_app_request_resource_usage(&app);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].kind, Some(AppRequestResourceKind::Mesh));
}
