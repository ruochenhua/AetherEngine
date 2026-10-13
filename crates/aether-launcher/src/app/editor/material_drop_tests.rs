use super::material_drop::{prepare_material_drop, resolve_material_assignment};
use crate::{
    app::workspace::{AssetDragPayload, MaterialDropRequest, MaterialDropTarget},
    project_index::ResourceKind,
};
use aether_engine::{
    asset::{
        mesh::{CpuMesh, GpuMesh},
        AssetManager,
    },
    ecs::{
        components::{
            MaterialAssetRef, MeshHandle, MeshSource, Name, Selected, Transform, Visibility,
        },
        Entity, World,
    },
    editor::{EditorContext, EditorHistory, MaterialAssignment, OperationLog},
    renderer::{extracted_material::ExtractedMaterial, renderable::MaterialUniform},
    scene::{material::TextureUsage, MaterialConfig, SceneDescription, TransparentMaterialConfig},
};
use std::{fs, path::PathBuf, sync::Arc};

pub(super) struct Fixture {
    pub(super) root: PathBuf,
    pub(super) world: World,
    pub(super) entity: Entity,
    pub(super) assets: AssetManager,
    pub(super) device: wgpu::Device,
}
impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("aether-material-drop-{}-{id}", std::process::id()));
        fs::create_dir_all(root.join("assets/materials")).unwrap();
        fs::create_dir_all(root.join("scenes")).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([128, 64, 32, 255]))
            .save(root.join("assets/materials/color.png"))
            .unwrap();
        let config = MaterialConfig {
            roughness: 0.23,
            metallic: 0.71,
            ..Default::default()
        };
        fs::write(
            root.join("assets/materials/sample.ron"),
            ron::ser::to_string(&MaterialConfig {
                albedo_texture: Some("color.png".into()),
                ..config.clone()
            })
            .unwrap(),
        )
        .unwrap();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .unwrap();
        let (device, _) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let mut world = World::new();
        let entity = world.spawn((
            Name("Mesh".into()),
            Transform::default(),
            Selected,
            Visibility::default(),
            config,
            MaterialUniform::default(),
            ExtractedMaterial::from_legacy_uniform(MaterialUniform::default()),
            MeshHandle::new(
                Arc::new(GpuMesh::from_cpu(&device, &CpuMesh::cube())),
                MeshSource::Builtin("cube".into()),
                "cube",
            ),
        ));
        Self {
            assets: AssetManager::with_project_root(&root),
            root,
            world,
            entity,
            device,
        }
    }
    pub(super) fn request(&self, target: MaterialDropTarget) -> MaterialDropRequest {
        MaterialDropRequest {
            entity: self.entity,
            target,
            resource: AssetDragPayload {
                relative_path: if target == MaterialDropTarget::Asset {
                    "assets/materials/sample.ron"
                } else {
                    "assets/materials/color.png"
                }
                .into(),
                kind: if target == MaterialDropTarget::Asset {
                    ResourceKind::Material
                } else {
                    ResourceKind::Image
                },
            },
        }
    }
    pub(super) fn prepare(
        &mut self,
        request: &MaterialDropRequest,
    ) -> anyhow::Result<(
        aether_engine::editor::EditorOperation,
        aether_engine::editor::PreparedMaterial,
    )> {
        prepare_material_drop(request, &self.root, &mut self.assets, &self.world)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn material_drop_resolves_relative_dependencies_and_preserves_selection_history_and_ron() {
    let mut f = Fixture::new();
    let request = f.request(MaterialDropTarget::Asset);
    let (operation, prepared) = f.prepare(&request).unwrap();
    assert_eq!(
        prepared.material.albedo_texture.as_deref(),
        Some("color.png")
    );
    assert!(prepared.extracted_material.albedo_texture().is_some());
    let expected = prepared.uniform;
    let scene = f.root.join("scenes/drop.ron");
    let log = scene.with_extension("editor.jsonl");
    let lighting = Default::default();
    let mut history = EditorHistory::default();
    let mut ctx = EditorContext::new(&mut f.world, &lighting, "drop", &scene, &log);
    history
        .apply_prepared_material(&mut ctx, operation, prepared)
        .unwrap();
    assert!(ctx.world.query_one::<&Selected>(f.entity).get().is_ok());
    assert_eq!(history.operation_log().apply_count(), 1);
    let ron: SceneDescription = ron::from_str(&fs::read_to_string(&scene).unwrap()).unwrap();
    assert_eq!(
        ron.objects[0].material_asset.as_deref(),
        Some("assets/materials/sample.ron")
    );
    assert_eq!(ron.objects[0].material, MaterialConfig::default());
    history.undo(&mut ctx).unwrap();
    assert!(ctx
        .world
        .query_one::<&MaterialAssetRef>(f.entity)
        .get()
        .is_err());
    history.redo(&mut ctx).unwrap();
    assert_eq!(
        *ctx.world
            .query_one::<&MaterialUniform>(f.entity)
            .get()
            .unwrap(),
        expected
    );
    assert!(ctx.world.query_one::<&Selected>(f.entity).get().is_ok());
    assert_eq!(
        OperationLog::from_jsonl(&fs::read_to_string(log).unwrap())
            .unwrap()
            .apply_count(),
        1
    );
}

#[test]
fn image_drop_changes_only_selected_slot_and_reconciles_transparent_albedo() {
    let mut f = Fixture::new();
    let initial = MaterialConfig {
        transparent: Some(TransparentMaterialConfig {
            texture: Some("assets/materials/color.png".into()),
            ..Default::default()
        }),
        ..f.world
            .query_one::<&MaterialConfig>(f.entity)
            .get()
            .unwrap()
            .clone()
    };
    f.world.insert(f.entity, (initial.clone(),)).unwrap();
    for slot in [
        TextureUsage::Albedo,
        TextureUsage::Normal,
        TextureUsage::Orm,
        TextureUsage::Emissive,
    ] {
        let request = f.request(MaterialDropTarget::Texture(slot));
        let (_, prepared) = f.prepare(&request).unwrap();
        let mut expected = initial.clone();
        match slot {
            TextureUsage::Albedo => {
                expected.albedo_texture = Some(request.resource.relative_path.clone());
                expected.transparent.as_mut().unwrap().texture =
                    Some(request.resource.relative_path.clone());
            }
            TextureUsage::Normal => {
                expected.normal_texture = Some(request.resource.relative_path.clone())
            }
            TextureUsage::Orm => {
                expected.orm_texture = Some(request.resource.relative_path.clone())
            }
            TextureUsage::Emissive => {
                expected.emissive_texture = Some(request.resource.relative_path.clone())
            }
        }
        assert_eq!(prepared.material, expected);
        assert_eq!(
            prepared.transparent_material.as_ref().unwrap().texture,
            prepared.extracted_material.albedo_texture()
        );
    }
}

#[test]
fn material_drop_rejects_unsafe_or_invalid_resources_before_any_mutation() {
    let mut f = Fixture::new();
    fs::write(f.root.join("assets/materials/corrupt.png"), "broken").unwrap();
    fs::write(f.root.join("assets/materials/corrupt.ron"), "broken").unwrap();
    fs::create_dir_all(f.root.join("assets/materials/directory.png")).unwrap();
    let initial = f
        .world
        .query_one::<&MaterialConfig>(f.entity)
        .get()
        .unwrap()
        .clone();
    for (path, kind, target) in [
        (
            "assets/materials/color.png",
            ResourceKind::Material,
            MaterialDropTarget::Asset,
        ),
        (
            "assets/materials/sample.ron",
            ResourceKind::Image,
            MaterialDropTarget::Texture(TextureUsage::Albedo),
        ),
        (
            "assets/materials/color.png",
            ResourceKind::Obj,
            MaterialDropTarget::Texture(TextureUsage::Albedo),
        ),
        (
            "assets/materials/corrupt.png",
            ResourceKind::Image,
            MaterialDropTarget::Texture(TextureUsage::Albedo),
        ),
        (
            "assets/materials/corrupt.ron",
            ResourceKind::Material,
            MaterialDropTarget::Asset,
        ),
        (
            "assets/materials/missing.png",
            ResourceKind::Image,
            MaterialDropTarget::Texture(TextureUsage::Normal),
        ),
        (
            "assets/../assets/materials/color.png",
            ResourceKind::Image,
            MaterialDropTarget::Texture(TextureUsage::Orm),
        ),
        (
            "assets/materials/directory.png",
            ResourceKind::Image,
            MaterialDropTarget::Texture(TextureUsage::Emissive),
        ),
    ] {
        let request = MaterialDropRequest {
            entity: f.entity,
            target,
            resource: AssetDragPayload {
                relative_path: path.into(),
                kind,
            },
        };
        assert!(f.prepare(&request).is_err(), "{path} must fail");
        assert_eq!(
            *f.world
                .query_one::<&MaterialConfig>(f.entity)
                .get()
                .unwrap(),
            initial
        );
        assert!(f.world.query_one::<&Selected>(f.entity).get().is_ok());
        assert!(!f.root.join("scenes/drop.ron").exists());
        assert!(!f.root.join("scenes/drop.editor.jsonl").exists());
    }
    let request = f.request(MaterialDropTarget::Texture(TextureUsage::Albedo));
    f.world
        .insert(
            f.entity,
            (MaterialAssetRef("assets/materials/sample.ron".into()),),
        )
        .unwrap();
    assert!(f.prepare(&request).is_err());
    let non_mesh = f.world.spawn((Name("Light".into()),));
    let mut request = f.request(MaterialDropTarget::Asset);
    request.entity = non_mesh;
    assert!(f.prepare(&request).is_err());
}

#[test]
fn linked_material_missing_dependency_is_rejected_instead_of_fallback_success() {
    let mut f = Fixture::new();
    fs::remove_file(f.root.join("assets/materials/color.png")).unwrap();
    let request = f.request(MaterialDropTarget::Asset);
    assert!(f.prepare(&request).is_err());
}

#[cfg(unix)]
#[test]
fn image_and_material_dependency_symlinks_are_rejected() {
    let mut f = Fixture::new();
    std::os::unix::fs::symlink(
        f.root.join("assets/materials/color.png"),
        f.root.join("assets/materials/link.png"),
    )
    .unwrap();
    let mut request = f.request(MaterialDropTarget::Texture(TextureUsage::Normal));
    request.resource.relative_path = "assets/materials/link.png".into();
    assert!(f.prepare(&request).is_err());
    let config = MaterialConfig {
        albedo_texture: Some("link.png".into()),
        ..Default::default()
    };
    fs::write(
        f.root.join("assets/materials/sample.ron"),
        ron::ser::to_string(&config).unwrap(),
    )
    .unwrap();
    let request = f.request(MaterialDropTarget::Asset);
    assert!(f.prepare(&request).is_err());
}

#[test]
fn inline_and_asset_log_resolution_use_same_live_resolver() {
    let mut f = Fixture::new();
    for target in [
        MaterialDropTarget::Asset,
        MaterialDropTarget::Texture(TextureUsage::Albedo),
    ] {
        let request = f.request(target);
        let (operation, live) = f.prepare(&request).unwrap();
        let aether_engine::editor::EditorOperation::SetMaterial { assignment, .. } = operation
        else {
            panic!("material operation")
        };
        let replay = resolve_material_assignment(&assignment, &f.root, &mut f.assets).unwrap();
        assert_eq!(replay.material, live.material);
        assert_eq!(replay.uniform, live.uniform);
        assert_eq!(replay.transparent_material, live.transparent_material);
        if matches!(assignment, MaterialAssignment::Inline { .. }) {
            assert!(replay.material_asset.is_none());
        }
    }
}

#[test]
fn material_release_revalidates_document_kind_after_project_scan() {
    let mut f = Fixture::new();
    let request = f.request(MaterialDropTarget::Asset);
    for source in [r#"(name: "a scene", objects: [])"#, "(unexpected: 1)"] {
        fs::write(f.root.join("assets/materials/sample.ron"), source).unwrap();
        assert!(
            f.prepare(&request).is_err(),
            "a non-material RON document must not decode as a default material"
        );
    }
}

#[test]
fn oversized_material_and_image_resources_are_rejected_before_assignment() {
    let mut f = Fixture::new();
    let mut document = fs::read_to_string(f.root.join("assets/materials/sample.ron")).unwrap();
    document.push_str(&" ".repeat(256 * 1024 + 1));
    fs::write(f.root.join("assets/materials/sample.ron"), document).unwrap();
    let request = f.request(MaterialDropTarget::Asset);
    assert!(f
        .prepare(&request)
        .err()
        .expect("oversized resource must fail")
        .to_string()
        .contains("size limit"));
    let request = f.request(MaterialDropTarget::Texture(TextureUsage::Normal));
    let file = fs::OpenOptions::new()
        .write(true)
        .open(f.root.join("assets/materials/color.png"))
        .unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    assert!(f
        .prepare(&request)
        .err()
        .expect("oversized resource must fail")
        .to_string()
        .contains("size limit"));
    image::RgbaImage::new(8193, 1)
        .save(f.root.join("assets/materials/color.png"))
        .unwrap();
    assert!(f
        .prepare(&request)
        .err()
        .expect("oversized resource must fail")
        .to_string()
        .contains("dimensions"));
}
