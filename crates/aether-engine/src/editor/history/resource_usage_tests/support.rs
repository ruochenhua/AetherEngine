use super::*;
use crate::asset::mesh::{CpuMesh, GpuMesh};
use std::sync::Arc;

pub fn root() -> &'static Path {
    Path::new(if cfg!(windows) {
        "C:/project"
    } else {
        "/project"
    })
}

pub fn material() -> MaterialConfig {
    MaterialConfig {
        albedo_texture: Some("textures/./same.png".into()),
        normal_texture: Some("textures/./same.png".into()),
        orm_texture: Some("textures/orm.png".into()),
        emissive_texture: Some("textures/emissive.png".into()),
        transparent: Some(TransparentMaterialConfig {
            texture: Some("textures/glass.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub fn object() -> ObjectConfig {
    ObjectConfig {
        name: "Mesh".into(),
        mesh: MeshRef::File("meshes/./cube.obj".into()),
        material: material(),
        material_asset: None,
        visible: true,
        transform: Default::default(),
        physics: None,
    }
}

pub fn mesh() -> MeshHandle {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, _queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    MeshHandle::new(
        Arc::new(GpuMesh::from_cpu(&device, &CpuMesh::cube())),
        MeshSource::File("meshes/./cube.obj".into()),
        "cube",
    )
}

pub fn prepared(material: MaterialConfig, linked: Option<&str>) -> PreparedMaterial {
    PreparedMaterial {
        material,
        uniform: MaterialUniform::default(),
        extracted_material: ExtractedMaterial::from_legacy_uniform(MaterialUniform::default()),
        material_asset: linked.map(|path| MaterialAssetRef(path.into())),
        material_asset_status: None,
        transparent_material: None,
    }
}

pub fn paths(label: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "aether-history-usage-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    (root.join("scene.ron"), root.join("scene.editor.jsonl"))
}

pub fn context<'a>(
    world: &'a mut World,
    lighting: &'a LightingUniforms,
    scene: &'a Path,
    log: &'a Path,
) -> EditorContext<'a> {
    EditorContext::new(world, lighting, "UsageTest", scene, log)
}

pub fn log(operations: Vec<EditorOperation>) -> OperationLog {
    let jsonl = operations
        .into_iter()
        .enumerate()
        .map(|(index, operation)| {
            serde_json::to_string(&OperationLogEntry {
                schema_version: EDITOR_SCHEMA_VERSION,
                sequence: 42 + index as u64,
                operation,
            })
            .unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");
    OperationLog::from_jsonl(&jsonl).unwrap()
}

pub fn candidate(entry: &HistoryResourceUse) -> &str {
    entry.project_relative.as_ref().unwrap().as_str()
}

pub fn assert_slots(report: &[HistoryResourceUse], prefix: &str) {
    let slots = report
        .iter()
        .filter(|entry| entry.source.field.starts_with(prefix))
        .map(|entry| (entry.source.field.strip_prefix(prefix).unwrap(), entry.kind))
        .collect::<Vec<_>>();
    assert_eq!(
        slots,
        vec![
            ("albedo_texture", WorldResourceKind::AlbedoTexture),
            ("emissive_texture", WorldResourceKind::EmissiveTexture),
            ("normal_texture", WorldResourceKind::NormalTexture),
            ("orm_texture", WorldResourceKind::OrmTexture),
            ("transparent.texture", WorldResourceKind::TransparentTexture),
        ]
    );
}
