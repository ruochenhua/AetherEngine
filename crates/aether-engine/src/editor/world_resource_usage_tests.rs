use super::*;
use crate::asset::prefab::{ComponentPatch, PrefabInstanceConfig, PrefabOverrides};
use crate::asset::terrain_material::TerrainMaterial;
use crate::asset::{
    mesh::{CpuMesh, GpuMesh},
    AssetManager,
};
use crate::ecs::components::{
    MaterialAssetRef, MeshHandle, MeshSource, Name, PrefabInstanceRoot, Selected, Terrain, Water,
};
use crate::scene::{
    MaterialConfig, MeshRef, TerrainGeometry, TerrainLayerConfig, TerrainSource,
    TransparentMaterialConfig, WaterConfig,
};
use std::sync::Arc;

fn root() -> &'static Path {
    Path::new(if cfg!(windows) {
        "C:/project"
    } else {
        "/project"
    })
}

fn material() -> MaterialConfig {
    MaterialConfig {
        albedo_texture: Some("textures/same.png".into()),
        normal_texture: Some("textures/same.png".into()),
        orm_texture: Some("textures/orm.png".into()),
        emissive_texture: Some("textures/emissive.png".into()),
        transparent: Some(TransparentMaterialConfig {
            texture: Some("textures/transparent.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn fields(report: &[WorldResourceUse]) -> Vec<(&str, WorldResourceKind)> {
    report
        .iter()
        .map(|entry| (entry.source.field.as_str(), entry.kind))
        .collect()
}

fn candidate(entry: &WorldResourceUse) -> &str {
    entry.project_relative.as_ref().unwrap().as_str()
}

#[test]
fn world_resource_usage_visits_all_material_slots_and_keeps_duplicate_fields() {
    let mut world = World::new();
    let entity = world.spawn((Name("same name".into()), material()));
    let report = capture_world_resource_usage(&world, root());
    assert_eq!(
        fields(&report),
        vec![
            ("albedo_texture", WorldResourceKind::AlbedoTexture),
            ("emissive_texture", WorldResourceKind::EmissiveTexture),
            ("normal_texture", WorldResourceKind::NormalTexture),
            ("orm_texture", WorldResourceKind::OrmTexture),
            ("transparent.texture", WorldResourceKind::TransparentTexture),
        ]
    );
    assert!(report.iter().all(|entry| entry.source.entity == entity
        && entry.source.entity_name.as_deref() == Some("same name")
        && entry.source.component == "MaterialConfig"));
    assert_eq!(candidate(&report[0]), candidate(&report[2]));
}

#[test]
fn world_resource_usage_visits_prefab_source_and_typed_overrides() {
    let instance = PrefabInstanceConfig {
        instance_id: 23,
        prefab_asset: "prefabs/./tree.ron".into(),
        overrides: PrefabOverrides {
            patches: vec![
                ComponentPatch::NameValue {
                    instance_id: 9,
                    value: "ignore/path.png".into(),
                },
                ComponentPatch::MeshSource {
                    instance_id: 9,
                    source: MeshRef::File("meshes/tree.obj".into()),
                },
                ComponentPatch::MaterialConfig {
                    instance_id: 9,
                    config: material(),
                },
                ComponentPatch::MeshSource {
                    instance_id: 10,
                    source: MeshRef::Builtin("cube".into()),
                },
                ComponentPatch::MeshSource {
                    instance_id: 11,
                    source: MeshRef::File("meshes/tree.obj".into()),
                },
            ],
            ..Default::default()
        },
    };
    let mut world = World::new();
    world.spawn((PrefabInstanceRoot(instance),));
    let report = capture_world_resource_usage(&world, root());
    assert_eq!(report.len(), 8);
    assert!(report
        .iter()
        .all(|entry| entry.source.component == "PrefabInstanceRoot"
            && entry.source.entity_name.is_none()));
    let source = report
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::PrefabAsset)
        .unwrap();
    assert_eq!(source.source.field, "prefab_asset");
    assert_eq!(source.authored_path, "prefabs/./tree.ron");
    assert_eq!(candidate(source), "prefabs/tree.ron");
    assert!(report
        .iter()
        .any(|entry| entry.source.field == "overrides.patches[1].node[9].source"));
    assert!(report
        .iter()
        .any(|entry| entry.source.field == "overrides.patches[4].node[11].source"));
    for slot in [
        "albedo_texture",
        "normal_texture",
        "orm_texture",
        "emissive_texture",
        "transparent.texture",
    ] {
        let entry = report
            .iter()
            .find(|entry| {
                entry.source.field == format!("overrides.patches[2].node[9].config.{slot}")
            })
            .unwrap();
        assert_eq!(candidate(entry), entry.authored_path);
    }
}

#[test]
fn world_resource_usage_visits_every_terrain_layer_and_water_map() {
    let layer = TerrainLayerConfig {
        albedo_texture: Some("textures/shared.png".into()),
        normal_texture: Some("textures/shared.png".into()),
        roughness_metallic_texture: Some("textures/rm.png".into()),
        ..Default::default()
    };
    let mut world = World::new();
    world.spawn((Terrain {
        source: TerrainSource::Heightmap("terrain/height.png".into()),
        geometry: TerrainGeometry::default(),
        material: TerrainMaterial::default(),
        splatmap_path: Some("terrain/splat.png".into()),
        // Even fields beyond the renderer's four runtime layers are retained paths.
        layer_configs: vec![layer; 5],
    },));
    world.spawn((Water {
        config: WaterConfig {
            dudv_map: Some("water/shared.png".into()),
            normal_map: Some("water/shared.png".into()),
            ..Default::default()
        },
        dudv_texture: None,
        normal_texture: None,
    },));
    let report = capture_world_resource_usage(&world, root());
    assert_eq!(report.len(), 19);
    for (field, kind) in [
        ("source.Heightmap", WorldResourceKind::TerrainHeightmap),
        ("splatmap_path", WorldResourceKind::TerrainSplatmap),
        ("config.dudv_map", WorldResourceKind::WaterDudv),
        ("config.normal_map", WorldResourceKind::WaterNormal),
    ] {
        assert!(fields(&report).contains(&(field, kind)), "missing {field}");
    }
    for index in 0..5 {
        for (slot, kind) in [
            ("albedo_texture", WorldResourceKind::TerrainAlbedo),
            ("normal_texture", WorldResourceKind::TerrainNormal),
            (
                "roughness_metallic_texture",
                WorldResourceKind::TerrainRoughnessMetallic,
            ),
        ] {
            assert!(fields(&report)
                .contains(&(format!("layer_configs[{index}].{slot}").as_str(), kind)));
        }
    }
}

#[test]
fn world_resource_usage_uses_material_document_parent_for_live_linked_config() {
    let mut world = World::new();
    world.spawn((
        MaterialAssetRef("materials/nested/../paint.ron".into()),
        MaterialConfig {
            albedo_texture: Some("../textures/paint.png".into()),
            normal_texture: Some("./normal.png".into()),
            ..Default::default()
        },
    ));
    let report = capture_world_resource_usage(&world, root());
    assert_eq!(report.len(), 3);
    let albedo = report
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::AlbedoTexture)
        .unwrap();
    assert_eq!(albedo.authored_path, "../textures/paint.png");
    assert_eq!(candidate(albedo), "textures/paint.png");
    let normal = report
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::NormalTexture)
        .unwrap();
    assert_eq!(candidate(normal), "materials/normal.png");
    let source = report
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::MaterialAsset)
        .unwrap();
    assert_eq!(source.source.component, "MaterialAssetRef");
    assert_eq!(source.source.field, "0");
    assert_eq!(candidate(source), "materials/paint.ron");
}

#[test]
fn world_resource_usage_retains_invalid_and_escaping_fields_with_diagnostics() {
    let mut world = World::new();
    for path in ["", ".", "../outside.ron", "/outside.ron", "C:\\outside.ron"] {
        world.spawn((MaterialAssetRef(path.into()),));
    }
    world.spawn((MaterialConfig {
        albedo_texture: Some("../../outside.png".into()),
        normal_texture: Some(String::new()),
        ..Default::default()
    },));
    world.spawn((MaterialAssetRef("../outside.ron".into()), material()));
    let report = capture_world_resource_usage(&world, root());
    assert_eq!(report.len(), 13);
    assert!(report.iter().all(|entry| entry.project_relative.is_err()));
    assert!(report
        .iter()
        .any(|entry| entry.project_relative == Err(WorldResourceDiagnostic::InvalidPath)));
    assert!(report
        .iter()
        .any(|entry| entry.project_relative == Err(WorldResourceDiagnostic::OutsideProjectRoot)));
    assert_eq!(
        report
            .iter()
            .filter(
                |entry| entry.project_relative == Err(WorldResourceDiagnostic::InvalidMaterialBase)
            )
            .count(),
        5
    );
}

#[test]
fn world_resource_usage_matches_store_normalization_and_in_project_absolute_paths() {
    let mut world = World::new();
    for path in [
        "materials\\nested\\..\\paint.ron".to_owned(),
        "./materials/../materials/paint.ron".to_owned(),
        root()
            .join("materials/paint.ron")
            .to_str()
            .unwrap()
            .to_owned(),
    ] {
        world.spawn((MaterialAssetRef(path),));
    }
    let report = capture_world_resource_usage(&world, root());
    assert_eq!(report.len(), 3);
    assert!(report
        .iter()
        .all(|entry| candidate(entry) == "materials/paint.ron"));
    assert_eq!(report[0].authored_path, "materials\\nested\\..\\paint.ron");
}

#[test]
fn world_resource_usage_diagnoses_nul_fields_instead_of_making_file_candidates() {
    let mut world = World::new();
    world.spawn((MaterialAssetRef("materials/\0paint.ron".into()),));
    world.spawn((MaterialConfig {
        albedo_texture: Some("textures/\0paint.png".into()),
        ..Default::default()
    },));
    let report = capture_world_resource_usage(&world, root());
    assert_eq!(report.len(), 2);
    assert!(report
        .iter()
        .all(|entry| entry.project_relative == Err(WorldResourceDiagnostic::InvalidPath)));
    assert!(report
        .iter()
        .all(|entry| entry.authored_path.contains('\0')));
}

#[test]
fn world_resource_usage_is_deterministic_and_observational_with_real_mesh_and_selection() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, _) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mesh = Arc::new(GpuMesh::from_cpu(&device, &CpuMesh::cube()));
    let mut world = World::new();
    let entity = world.spawn((
        MeshHandle::new(
            mesh.clone(),
            MeshSource::File("meshes/./cube.obj".into()),
            "cube",
        ),
        Name("same".into()),
        Selected,
        material(),
    ));
    let second = world.spawn((
        MeshHandle::new(mesh.clone(), MeshSource::Builtin("cube".into()), "cube"),
        Name("same".into()),
    ));
    world.spawn((
        MaterialAssetRef("materials/second.ron".into()),
        Name("same".into()),
    ));
    let assets = AssetManager::with_project_root(root());
    let history = super::super::EditorHistory::default();
    let assets_before = assets.resource_usage_snapshot();
    let log_before = history.operation_log().clone();
    let report = capture_world_resource_usage(&world, root());
    assert_eq!(report.len(), 7);
    for _ in 0..3 {
        assert_eq!(capture_world_resource_usage(&world, root()), report);
    }
    assert!(report.windows(2).all(|pair| {
        let key = |entry: &WorldResourceUse| {
            (
                entry.source.entity.to_bits().get(),
                entry.source.component,
                entry.source.field.clone(),
            )
        };
        key(&pair[0]) <= key(&pair[1])
    }));
    let file = report
        .iter()
        .find(|entry| entry.kind == WorldResourceKind::Mesh)
        .unwrap();
    assert_eq!(file.source.entity, entity);
    assert_eq!(file.source.component, "MeshHandle");
    assert_eq!(file.source.field, "source.File");
    assert_eq!(candidate(file), "meshes/cube.obj");
    assert!(!report.iter().any(|entry| entry.source.entity == second));
    assert_eq!(world.len(), 3);
    assert_eq!(
        world
            .query::<(Entity, &Selected)>()
            .iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert_eq!(*world.get::<&MaterialConfig>(entity).unwrap(), material());
    let retained = world.get::<&MeshHandle>(entity).unwrap();
    assert_eq!(
        retained.source,
        MeshSource::File("meshes/./cube.obj".into())
    );
    assert!(Arc::ptr_eq(&retained.mesh, &mesh));
    assert_eq!(Arc::strong_count(&mesh), 3);
    assert_eq!(assets.resource_usage_snapshot(), assets_before);
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 0);
    assert_eq!(history.operation_log().clone(), log_before);
}

#[test]
fn world_resource_usage_empty_world_has_no_fields_and_does_not_certify_other_owners() {
    assert!(capture_world_resource_usage(&World::new(), root()).is_empty());
}

#[cfg(unix)]
#[test]
fn world_resource_usage_diagnoses_texture_host_identity_separator_disagreement() {
    let mut world = World::new();
    world.spawn((MaterialConfig {
        albedo_texture: Some("textures\\..\\color.png".into()),
        ..Default::default()
    },));
    world.spawn((
        MaterialAssetRef("materials/paint.ron".into()),
        MaterialConfig {
            normal_texture: Some("..\\normal.png".into()),
            ..Default::default()
        },
    ));
    let report = capture_world_resource_usage(&world, root());
    assert_eq!(report.len(), 3);
    assert_eq!(
        report
            .iter()
            .filter(|entry| entry.project_relative
                == Err(WorldResourceDiagnostic::AmbiguousTextureSeparators))
            .count(),
        2
    );
}
