use super::{
    asset_validation::{project_relative_texture_path, validate_obj_asset},
    App, LauncherState,
};
use aether_engine::{
    asset::{
        mesh::{CpuMesh, GpuMesh},
        AssetManager,
    },
    ecs::components::{MeshHandle, MeshSource},
    editor::{EditorContext, EditorError, EditorOperation, OperationLog, PreparedObject},
    renderer::{extracted_material::ExtractedMaterial, renderable::MaterialUniform},
    scene::{material::MaterialResolver, MaterialConfig, MeshRef, ObjectConfig},
};
use std::{path::Path, sync::Arc};

/// Rebuild a scene from its baseline state and an explicit operation log.
/// Ordinary scene opening intentionally does not call this function.
#[allow(dead_code)] // This explicit reconstruction entry point is reserved for baseline replay and tests.
pub fn replay_operation_log_from_baseline(app: &mut App, jsonl: &str) -> Result<(), EditorError> {
    let log = OperationLog::from_jsonl(jsonl).map_err(|error| {
        EditorError::Serialization(format!("invalid editor operation log: {error}"))
    })?;
    let scene_path = app
        .cli
        .scene
        .as_deref()
        .map(std::path::PathBuf::from)
        .ok_or_else(|| EditorError::Serialization("no active baseline scene".into()))?;
    let scene_name = scene_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let log_path = super::operation_log_path(&scene_path);
    let project_root = app.workspace.project_root().to_path_buf();
    let device = app
        .ctx
        .as_ref()
        .ok_or_else(|| EditorError::Serialization("render context is not ready".into()))?
        .device
        .clone();

    let App {
        state,
        editor_history,
        asset_manager,
        ..
    } = app;
    let LauncherState::Running { world, lighting } = state else {
        return Err(EditorError::Serialization(
            "no running baseline scene".into(),
        ));
    };
    let mut context = EditorContext::new(world, lighting, &scene_name, &scene_path, &log_path);
    replay_resolved_log(
        editor_history,
        &mut context,
        &log,
        &project_root,
        &device,
        asset_manager,
    )
}

/// Resolve every asset-bearing operation through the same host seam as live drops.
pub(super) fn replay_resolved_log(
    history: &mut aether_engine::editor::EditorHistory,
    context: &mut EditorContext<'_>,
    log: &OperationLog,
    root: &Path,
    device: &wgpu::Device,
    assets: &mut AssetManager,
) -> Result<(), EditorError> {
    for entry in log.entries() {
        match &entry.operation {
            EditorOperation::PlaceObject { object } => {
                let prepared = resolve_logged_object(object, root, device, assets)?;
                history.apply_prepared(context, entry.operation.clone(), prepared)?;
            }
            EditorOperation::SetMaterial { assignment, .. } => {
                let prepared =
                    super::material_drop::resolve_material_assignment(assignment, root, assets)
                        .map_err(|e| EditorError::InvalidMaterialOperation(e.to_string()))?;
                history.apply_prepared_material(context, entry.operation.clone(), prepared)?;
            }
            operation => history.apply(context, operation.clone())?,
        }
    }
    Ok(())
}

#[allow(dead_code)] // Used through the explicit replay entry point and its focused test.
pub(super) fn resolve_logged_object(
    object: &ObjectConfig,
    project_root: &Path,
    device: &wgpu::Device,
    assets: &mut AssetManager,
) -> Result<PreparedObject, EditorError> {
    if object.material_asset.is_some() || object.physics.is_some() {
        return Err(EditorError::InvalidObjectOperation(
            "replay currently supports OBJ objects without linked materials or physics".into(),
        ));
    }
    let MeshRef::File(relative_path) = &object.mesh else {
        return Err(EditorError::InvalidObjectOperation(
            "replay currently supports project-relative OBJ meshes".into(),
        ));
    };
    let canonical_path = validate_obj_asset(Path::new(relative_path), project_root)
        .map_err(|error| EditorError::InvalidObjectOperation(error.to_string()))?;
    let canonical_root = project_root
        .canonicalize()
        .map_err(|error| EditorError::InvalidObjectOperation(error.to_string()))?;
    let asset_path = canonical_path
        .strip_prefix(&canonical_root)
        .map_err(|error| EditorError::InvalidObjectOperation(error.to_string()))?
        .to_string_lossy()
        .replace('\\', "/");
    if assets.project_root().canonicalize().ok().as_deref() != Some(canonical_root.as_path()) {
        return Err(EditorError::InvalidObjectOperation(
            "asset manager is rooted at a different project directory".into(),
        ));
    }
    let handle = assets
        .load::<CpuMesh>(&asset_path)
        .map_err(|error| EditorError::InvalidObjectOperation(error.to_string()))?;
    let mesh = assets.get(handle).ok_or_else(|| {
        EditorError::InvalidObjectOperation(format!("loaded OBJ is not available: {asset_path}"))
    })?;
    if mesh.submeshes.len() > 1 || mesh.positions.is_empty() || mesh.indices.is_empty() {
        return Err(EditorError::InvalidObjectOperation(
            "replay requires a valid OBJ mesh with at most one material".into(),
        ));
    }
    let material_config = if let Some(part) = mesh.submeshes.first() {
        MaterialConfig {
            albedo: part.material.base_color,
            roughness: part.material.roughness,
            metallic: part.material.metallic,
            albedo_texture: part
                .material
                .albedo_texture
                .as_deref()
                .map(|path| project_relative_texture_path(path, &canonical_root))
                .transpose()
                .map_err(|error| EditorError::InvalidObjectOperation(error.to_string()))?,
            ..MaterialConfig::default()
        }
    } else {
        object.material.clone()
    };
    let material = MaterialResolver::new(&canonical_root)
        .resolve(&material_config, assets)
        .map_err(|error| EditorError::InvalidObjectOperation(error.to_string()))?;
    let gpu_mesh = GpuMesh::from_cpu(device, &mesh);
    let gpu_mesh = if let Some(part) = mesh.submeshes.first() {
        GpuMesh::submesh_view(&gpu_mesh, part.index_offset as u32, part.index_count as u32)
    } else {
        gpu_mesh
    };
    Ok(PreparedObject {
        object: object.clone(),
        mesh: MeshHandle::new(
            Arc::new(gpu_mesh),
            MeshSource::File(asset_path),
            object.name.clone(),
        ),
        material: MaterialUniform::from_resolution(&material),
        extracted_material: ExtractedMaterial::from_resolution(&material),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_engine::{
        asset::AssetManager,
        ecs::{
            components::{MeshHandle, MeshSource, Name, Transform},
            World,
        },
        editor::{EditorHistory, EditorOperation, OperationLogEntry},
        renderer::light::LightingUniforms,
        scene::{MaterialConfig, TransformConfig},
    };
    use glam::{Quat, Vec3};
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn launcher_replay_resolves_logged_obj_and_preserves_object_config() {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("aether_replay_{}_{}", std::process::id(), id));
        fs::create_dir_all(root.join("assets/models")).unwrap();
        fs::write(
            root.join("assets/models/sample.obj"),
            "v -1 2 -1\nv 1 2 -1\nv 0 4 1\nf 1 2 3\n",
        )
        .unwrap();
        let object = ObjectConfig {
            name: "ReplayModel".into(),
            mesh: MeshRef::File("assets/models/sample.obj".into()),
            transform: TransformConfig {
                translation: [-4.0, 3.0, 7.0],
                rotation: Quat::IDENTITY.to_array(),
                scale: Vec3::splat(2.0).to_array(),
            },
            material: MaterialConfig::default(),
            material_asset: None,
            visible: true,
            physics: None,
        };
        let entry = OperationLogEntry {
            schema_version: 2,
            sequence: 0,
            operation: EditorOperation::PlaceObject { object },
        };
        let log =
            OperationLog::from_jsonl(&format!("{}\n", serde_json::to_string(&entry).unwrap()))
                .unwrap();
        let device = headless_device();
        let mut assets = AssetManager::with_project_root(&root);
        let mut world = World::new();
        let lighting = LightingUniforms::default();
        let scene_path = root.join("scenes/replay.ron");
        fs::create_dir_all(scene_path.parent().unwrap()).unwrap();
        let log_path = scene_path.with_extension("editor.jsonl");
        let mut history = EditorHistory::default();
        let mut context =
            EditorContext::new(&mut world, &lighting, "replay", &scene_path, &log_path);

        history
            .replay_with_prepared(&mut context, &log, |object| {
                resolve_logged_object(object, &root, &device, &mut assets)
            })
            .unwrap();

        let entity = context
            .world
            .query::<(aether_engine::ecs::Entity, &Name)>()
            .iter()
            .find_map(|(entity, name)| (name.0 == "ReplayModel").then_some(entity))
            .unwrap();
        let mut query = context.world.query_one::<&Transform>(entity);
        let transform = query.get().unwrap();
        assert_eq!(transform.translation, Vec3::new(-4.0, 3.0, 7.0));
        assert_eq!(transform.scale, Vec3::splat(2.0));
        let mut mesh_query = context.world.query_one::<&MeshHandle>(entity);
        let mesh = mesh_query.get().unwrap();
        assert!(
            matches!(mesh.source, MeshSource::File(ref path) if path == "assets/models/sample.obj")
        );
        assert_eq!(history.operation_log().apply_count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("headless adapter required for launcher replay test");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("headless device required for launcher replay test")
            .0
    }
}
