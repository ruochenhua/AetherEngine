use aether_engine::{
    asset::mesh::{CpuMesh, GpuMesh},
    ecs::{
        components::{MeshHandle, MeshSource, Name},
        World,
    },
    editor::{EditorOperation, PreparedObject},
    renderer::{extracted_material::ExtractedMaterial, renderable::MaterialUniform},
    scene::{material::MaterialResolver, MaterialConfig, MeshRef, ObjectConfig, TransformConfig},
};
use glam::{Quat, Vec3};
use std::{
    fs,
    io::BufRead,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

/// Load and stage a supported OBJ drop without changing the live world.
pub(super) fn prepare_obj_drop(
    relative_path: &str,
    project_root: &Path,
    device: &wgpu::Device,
    assets: &mut aether_engine::asset::AssetManager,
    ground_point: Vec3,
    world: &World,
) -> anyhow::Result<(EditorOperation, PreparedObject)> {
    let relative = Path::new(relative_path);
    let canonical_path = validate_obj_asset(relative, project_root)?;
    let canonical_root = project_root.canonicalize()?;
    let asset_path = canonical_path
        .strip_prefix(&canonical_root)?
        .to_string_lossy()
        .replace('\\', "/");
    let manager_root = assets.project_root().canonicalize()?;
    if manager_root != canonical_root {
        anyhow::bail!("asset manager is rooted at a different project directory");
    }

    let handle = assets.load::<CpuMesh>(&asset_path)?;
    let cpu_mesh = assets
        .get(handle)
        .ok_or_else(|| anyhow::anyhow!("loaded OBJ is not available: {asset_path}"))?;
    if !cpu_mesh.submeshes.is_empty() {
        anyhow::bail!(
            "OBJ contains multiple material submeshes; placement currently supports one mesh"
        );
    }
    if cpu_mesh.positions.is_empty()
        || cpu_mesh.indices.is_empty()
        || cpu_mesh
            .positions
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
    {
        anyhow::bail!("OBJ has no valid indexed mesh geometry");
    }

    let aabb = cpu_mesh.compute_aabb();
    let mut name = canonical_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if name.is_empty() {
        name = "Object".into();
    }
    let base_name = name.clone();
    let mut suffix = 2;
    while world.query::<&Name>().iter().any(|item| item.0 == name) {
        name = format!("{base_name}_{suffix}");
        suffix += 1;
    }

    let material = MaterialConfig::default();
    let resolution = MaterialResolver::new(&canonical_root).resolve(&material, assets)?;
    let object = ObjectConfig {
        name,
        mesh: MeshRef::File(asset_path.clone()),
        transform: TransformConfig {
            translation: [ground_point.x, -aabb.min.y, ground_point.z],
            rotation: Quat::IDENTITY.to_array(),
            scale: Vec3::ONE.to_array(),
        },
        material,
        material_asset: None,
        visible: true,
        physics: None,
    };
    let prepared = PreparedObject {
        object: object.clone(),
        mesh: MeshHandle::new(
            Arc::new(GpuMesh::from_cpu(device, &cpu_mesh)),
            MeshSource::File(asset_path),
            object.name.clone(),
        ),
        material: MaterialUniform::from_resolution(&resolution),
        extracted_material: ExtractedMaterial::from_resolution(&resolution),
    };
    let operation = EditorOperation::PlaceObject { object };
    Ok((operation, prepared))
}

pub(super) fn validate_obj_asset(
    relative_path: &Path,
    project_root: &Path,
) -> anyhow::Result<PathBuf> {
    if relative_path.is_absolute()
        || relative_path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || relative_path.components().next() != Some(Component::Normal("assets".as_ref()))
        || !relative_path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("obj"))
    {
        anyhow::bail!("only project-relative OBJ files under assets/ can be placed");
    }

    let root_metadata = fs::symlink_metadata(project_root)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        anyhow::bail!("project root is missing or is a symlink");
    }
    let canonical_root = project_root.canonicalize()?;
    let assets_root = canonical_root.join("assets");
    let assets_metadata = fs::symlink_metadata(&assets_root)?;
    if assets_metadata.file_type().is_symlink() || !assets_metadata.is_dir() {
        anyhow::bail!("project assets directory is missing or is a symlink");
    }
    let mut cursor = canonical_root.clone();
    for component in relative_path.components() {
        let Component::Normal(name) = component else {
            anyhow::bail!("invalid project resource path");
        };
        cursor.push(name);
        let metadata = fs::symlink_metadata(&cursor)?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!("symbolic links are not supported for dropped resources");
        }
    }

    let canonical_path = cursor.canonicalize()?;
    let canonical_assets = assets_root.canonicalize()?;
    if !canonical_path.starts_with(&canonical_assets) || !canonical_path.is_file() {
        anyhow::bail!("resource is outside the project assets directory");
    }
    let reader = std::io::BufReader::new(fs::File::open(&canonical_path)?);
    for line in reader.lines() {
        if line?.split_whitespace().next() == Some("mtllib") {
            anyhow::bail!("OBJ material libraries and external dependencies are not supported yet");
        }
    }
    Ok(canonical_path)
}

pub(super) fn apply_pending_drop(app: &mut super::App) {
    let Some(request) = app.pending_asset_drop.take() else {
        return;
    };
    if let Some(reason) = app.editor_write_degraded.as_ref() {
        app.editor_operation_message = Some(format!("编辑器写入已停止：{reason}"));
        return;
    }
    let Some(scene_path) = app.cli.scene.as_deref().map(PathBuf::from) else {
        app.editor_operation_message = Some("请先保存场景，再从 Project 拖入模型。".into());
        return;
    };
    let Some(render_context) = app.ctx.as_ref() else {
        app.editor_operation_message = Some("渲染上下文尚未就绪，无法放置模型。".into());
        return;
    };
    let scale = request.pixels_per_point;
    let Some(drop_rect) = aether_engine::renderer::picking::ScreenRect::from_points(
        glam::Vec2::from_array(request.viewport_points_min),
        glam::Vec2::from_array(request.viewport_points_max),
        scale,
    ) else {
        app.editor_operation_message = Some("视口区域无效，模型未放置。".into());
        return;
    };
    let pointer = glam::Vec2::from_array(request.pointer_physical);
    let width = render_context.config.width as f32;
    let height = render_context.config.height as f32;
    let aspect = width / height;
    let view = app.camera.view_matrix();
    let projection = app.camera.projection_matrix(aspect);
    let Some(ray) = aether_engine::renderer::picking::screen_ray_for_pointer(
        pointer,
        drop_rect,
        glam::Vec2::new(width, height),
        view,
        projection,
        app.camera.position,
    ) else {
        app.editor_operation_message = Some("拖放位置不在有效视口内，模型未放置。".into());
        return;
    };
    let Some(ground_point) =
        aether_engine::renderer::picking::intersect_horizontal_plane(&ray, 0.0)
    else {
        app.editor_operation_message = Some("该视角无法向前与 y=0 地面相交，模型未放置。".into());
        return;
    };

    let project_root = app.workspace.project_root().to_path_buf();
    let staged = match &app.state {
        super::LauncherState::Running { world, .. } => prepare_obj_drop(
            &request.relative_path,
            &project_root,
            &render_context.device,
            &mut app.asset_manager,
            ground_point,
            world,
        ),
        super::LauncherState::Menu => {
            app.editor_operation_message = Some("当前没有可编辑的场景。".into());
            return;
        }
    };
    let (operation, prepared) = match staged {
        Ok(value) => value,
        Err(error) => {
            app.editor_operation_message = Some(format!("无法放置资源：{error}"));
            tracing::warn!(%error, "project asset drop rejected");
            return;
        }
    };

    let scene_name = scene_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let log_path = super::operation_log_path(&scene_path);
    let result = {
        let super::App {
            state,
            editor_history,
            ..
        } = app;
        match state {
            super::LauncherState::Running { world, lighting } => editor_history.apply_prepared(
                &mut aether_engine::editor::EditorContext::new(
                    world,
                    lighting,
                    &scene_name,
                    &scene_path,
                    &log_path,
                ),
                operation,
                prepared,
            ),
            super::LauncherState::Menu => Err(aether_engine::editor::EditorError::Serialization(
                "no running scene".into(),
            )),
        }
    };
    match result {
        Ok(()) => {
            app.undo_action_order.push(super::UndoActionKind::Operation);
            app.redo_action_order.clear();
            app.editor_operation_message = Some(format!(
                "已放置并保存模型；编辑日志：{}",
                log_path.display()
            ));
        }
        Err(error) => {
            super::mark_degraded(app, &error);
            app.editor_operation_message = Some(format!("模型放置失败：{error}"));
            tracing::error!(%error, "project asset placement failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_engine::{
        asset::AssetManager,
        ecs::components::{Selected, Transform},
        editor::{EditorContext, EditorHistory, OperationLog},
        renderer::light::LightingUniforms,
        scene::loader::SceneLoader,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("aether_obj_drop_{}_{}", std::process::id(), id));
            fs::create_dir_all(root.join("assets/models")).unwrap();
            Self(root)
        }

        fn obj(&self, content: &str) -> PathBuf {
            let path = self.0.join("assets/models/sample.obj");
            fs::write(&path, content).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn valid_project_obj_is_resolved_within_assets_root() {
        let fixture = Fixture::new();
        let obj = fixture.obj("v 0 0 0\n");
        let relative = validate_obj_asset(Path::new("assets/models/sample.obj"), &fixture.0)
            .expect("valid project OBJ should resolve");
        assert_eq!(relative, obj.canonicalize().unwrap());
    }

    #[test]
    fn obj_dependencies_and_paths_outside_assets_are_rejected() {
        let fixture = Fixture::new();
        fixture.obj("mtllib sample.mtl\nv 0 0 0\n");
        assert!(validate_obj_asset(Path::new("assets/models/sample.obj"), &fixture.0).is_err());
        assert!(validate_obj_asset(Path::new("../outside.obj"), &fixture.0).is_err());
        assert!(validate_obj_asset(Path::new("scenes/sample.obj"), &fixture.0).is_err());
    }

    #[test]
    fn placement_persists_roundtrips_and_redo_reselects_the_object() {
        let fixture = Fixture::new();
        fixture.obj("v -1 2 -1\nv 1 2 -1\nv 0 4 1\nf 1 2 3\n");
        let device = headless_device();
        let mut assets = AssetManager::with_project_root(&fixture.0);
        let mut world = World::new();
        let existing = world.spawn((Transform::default(), Name("Existing".into()), Selected));
        let (operation, prepared) = prepare_obj_drop(
            "assets/models/sample.obj",
            &fixture.0,
            &device,
            &mut assets,
            Vec3::new(-3.0, 0.0, -4.0),
            &world,
        )
        .unwrap();
        let dropped_name = prepared.object.name.clone();
        let scene_path = fixture.0.join("scenes/placed.ron");
        fs::create_dir_all(scene_path.parent().unwrap()).unwrap();
        let log_path = scene_path.with_extension("editor.jsonl");
        let lighting = LightingUniforms::default();
        let mut history = EditorHistory::default();
        let mut context =
            EditorContext::new(&mut world, &lighting, "placed", &scene_path, &log_path);

        history
            .apply_prepared(&mut context, operation, prepared)
            .unwrap();
        let dropped = context
            .world
            .query::<(aether_engine::ecs::Entity, &Name)>()
            .iter()
            .find_map(|(entity, name)| (name.0 == dropped_name).then_some(entity))
            .unwrap();
        let translation = context
            .world
            .query_one::<&Transform>(dropped)
            .get()
            .unwrap()
            .translation;
        assert_eq!(translation, Vec3::new(-3.0, -2.0, -4.0));
        assert!(context.world.query_one::<&Selected>(dropped).get().is_ok());
        assert_eq!(
            OperationLog::from_jsonl(&fs::read_to_string(&log_path).unwrap())
                .unwrap()
                .apply_count(),
            1
        );

        let _ = context.world.remove::<(Selected,)>(dropped);
        context.world.insert(existing, (Selected,)).unwrap();
        history.undo(&mut context).unwrap();
        assert!(context.world.query_one::<&Name>(dropped).get().is_err());
        assert!(context.world.query_one::<&Selected>(existing).get().is_ok());
        history.redo(&mut context).unwrap();
        let redone = context
            .world
            .query::<(aether_engine::ecs::Entity, &Name)>()
            .iter()
            .find_map(|(entity, name)| (name.0 == dropped_name).then_some(entity))
            .unwrap();
        assert!(context.world.query_one::<&Selected>(redone).get().is_ok());

        let mut reopened = World::new();
        let mut reopened_assets = AssetManager::with_project_root(&fixture.0);
        SceneLoader::open_scene(
            &scene_path,
            &device,
            &aether_engine::asset::registry::BuiltinMeshRegistry::new(),
            &mut reopened_assets,
            &mut reopened,
        )
        .unwrap();
        assert!(reopened
            .query::<&Name>()
            .iter()
            .any(|name| name.0 == dropped_name));
    }

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("headless adapter required for OBJ placement test");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("headless device required for OBJ placement test")
            .0
    }
}
