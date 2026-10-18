use super::*;

#[test]
fn history_resource_usage_inverse_add_keeps_snapshot_material_fields() {
    let (scene, target) = paths("inverse-add");
    let mut world = World::new();
    let entity = world.spawn((
        Name("Mesh".into()),
        Transform::default(),
        Visibility(true),
        material(),
    ));
    let lighting = LightingUniforms::default();
    let mut history = EditorHistory::default();
    history
        .apply(
            &mut context(&mut world, &lighting, &scene, &target),
            EditorOperation::AddComponent {
                entity_name: "Mesh".into(),
                component: ComponentRecord::Material {
                    config: MaterialConfig::default(),
                },
            },
        )
        .unwrap();
    history
        .undo(&mut context(&mut world, &lighting, &scene, &target))
        .unwrap();
    let report = history.resource_usage(root());
    assert_eq!(report.len(), 5);
    assert_slots(&report, "inverse.Add.record.Material.config.");
    assert!(report
        .iter()
        .all(|entry| entry.source.owner == HistoryResourceOwner::Redo));
    assert!(world.contains(entity));
}

#[test]
fn history_resource_usage_skips_non_resource_strings_and_builtin_meshes() {
    let mut obj = object();
    obj.mesh = MeshRef::Builtin("not/a/file.obj".into());
    obj.material = MaterialConfig::default();
    obj.name = "also/not/a/file.png".into();
    let log = log(vec![
        EditorOperation::PlaceObject { object: obj },
        EditorOperation::Rename {
            entity_name: "path/old.png".into(),
            new_name: "path/new.png".into(),
        },
        EditorOperation::Copy {
            source_name: "path/source.png".into(),
            new_name: "path/copy.png".into(),
        },
        EditorOperation::Delete {
            entity_name: "path/deleted.png".into(),
        },
        EditorOperation::AddComponent {
            entity_name: "Mesh".into(),
            component: ComponentRecord::Name {
                value: "path/name.png".into(),
            },
        },
        EditorOperation::AddComponent {
            entity_name: "Mesh".into(),
            component: ComponentRecord::Mesh {
                source: MeshRef::Builtin("path/builtin.png".into()),
            },
        },
    ]);
    assert!(log.resource_usage(root()).is_empty());
    assert!(EditorHistory::default().resource_usage(root()).is_empty());
}

#[test]
fn history_resource_usage_absolute_paths_follow_project_containment() {
    let mut obj = object();
    obj.mesh = MeshRef::File(root().join("meshes/model.obj").to_str().unwrap().into());
    obj.material_asset = Some(
        root()
            .with_file_name("outside")
            .join("material.ron")
            .to_str()
            .unwrap()
            .into(),
    );
    obj.material = MaterialConfig {
        normal_texture: Some(root().join("textures/normal.png").to_str().unwrap().into()),
        ..Default::default()
    };
    let report = log(vec![EditorOperation::PlaceObject { object: obj }]).resource_usage(root());
    assert_eq!(report.len(), 3);
    let mesh = report
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::Mesh)
        .unwrap();
    assert_eq!(candidate(mesh), "meshes/model.obj");
    let material = report
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::MaterialAsset)
        .unwrap();
    assert_eq!(
        material.project_relative,
        Err(WorldResourceDiagnostic::OutsideProjectRoot)
    );
    let normal = report
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::NormalTexture)
        .unwrap();
    assert_eq!(candidate(normal), "textures/normal.png");
}
