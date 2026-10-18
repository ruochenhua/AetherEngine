use super::*;
use crate::app::scene_transition::{
    PendingSceneTransition, SceneTransitionChoice, SceneTransitionTarget,
};

pub(crate) fn request_scene_transition(app: &mut App, target: SceneTransitionTarget) {
    if app.pending_scene_transition.is_some() {
        return;
    }
    if app.particle_runtime.playback
        != super::super::particle_runtime::physics_runtime::PlaybackState::Stopped
    {
        app.editor_operation_message = Some("请先停止模拟，再切换场景或关闭编辑器。".into());
        return;
    }
    if matches!(app.state, LauncherState::Running { .. })
        && (app.editor_write_degraded.is_some() || !app.document_state.can_transition())
    {
        app.editor_operation_message =
            Some("当前场景状态未知或需要恢复，暂不能切换或关闭。".into());
        return;
    }
    let dirty = matches!(app.state, LauncherState::Running { .. })
        && app.document_state.status() == super::super::document_state::DocumentStatus::Dirty;
    let transition = PendingSceneTransition::new(target, dirty);
    app.pending_transition_choice = transition.choice;
    app.pending_scene_transition = Some(transition);
    app.transition_cancel_focus_pending = dirty;
}

pub(super) fn process_scene_transition(app: &mut App) {
    let Some(choice) = app.pending_transition_choice.take() else {
        return;
    };
    let Some(mut transition) = app.pending_scene_transition.take() else {
        return;
    };
    if choice == SceneTransitionChoice::Cancel {
        return;
    }
    if choice == SceneTransitionChoice::Save && !save_scene(app, false) {
        return;
    }
    transition.choice = Some(choice);
    match transition.target {
        SceneTransitionTarget::NewScene => create_new_scene(app),
        SceneTransitionTarget::OpenDialog => open_scene_dialog(app),
        SceneTransitionTarget::OpenPath(path) => open_scene_path(app, &path),
        SceneTransitionTarget::CloseApplication => app.exit_requested = true,
    }
}

fn create_new_scene(app: &mut App) {
    super::super::editor::start_unsaved_scene(app);
    if let LauncherState::Running {
        ref mut world,
        ref mut lighting,
    } = app.state
    {
        let ctx = app.ctx.as_ref().unwrap();
        world.clear();
        *lighting = SceneLoader::new_empty(world);
        super::super::particle_runtime::physics_runtime::scene_switched(&mut app.particle_runtime);
        if let Some(cpu_mesh) = app.mesh_registry.get("cube") {
            let gpu_mesh = Arc::new(GpuMesh::from_cpu(&ctx.device, &cpu_mesh));
            let material = aether_engine::renderer::renderable::MaterialUniform {
                albedo: [0.8, 0.3, 0.2, 1.0],
                roughness: 0.5,
                metallic: 0.0,
                unlit: 0,
                _pad: 0,
                albedo_texture_id: 0,
                ..Default::default()
            };
            world.spawn((
                Transform::default(),
                MeshHandle::new(gpu_mesh, MeshSource::Builtin("cube".into()), "cube"),
                material,
                aether_engine::renderer::extracted_material::ExtractedMaterial::from_legacy_uniform(
                    material,
                ),
                Visibility::default(),
                Name("DefaultCube".into()),
                Selected,
            ));
        }
        app.camera = FlyCamera::default();
        if let Err(error) = super::super::particle_runtime::load_scene(
            &mut app.particle_runtime.particles,
            world,
            &mut app.cli.time,
        ) {
            error!("Particle runtime reset for new scene failed: {error}");
        }
    }
    super::super::editor::checkpoint_untitled_scene(app);
    info!("New scene created");
}

fn open_scene_dialog(app: &mut App) {
    if let Some(path) = rfd::FileDialog::new()
        .set_directory("./scenes")
        .add_filter("RON", &["ron"])
        .pick_file()
    {
        open_scene_path(app, &path);
    }
}

fn open_scene_path(app: &mut App, path: &std::path::Path) {
    if matches!(app.state, LauncherState::Menu) {
        app.editor_operation_message = Some("编辑器尚未启动，当前不能打开场景。".into());
        return;
    }
    let Some(device) = app.ctx.as_ref().map(|ctx| ctx.device.clone()) else {
        app.editor_operation_message = Some("渲染设备尚未就绪，当前不能打开场景。".into());
        return;
    };
    if let LauncherState::Running {
        ref mut world,
        ref mut lighting,
    } = app.state
    {
        let candidate_result = (|| {
            let description = SceneLoader::from_file(path)?;
            let mut candidate = World::new();
            let new_lighting = SceneLoader::build_world(
                &description,
                &device,
                &app.mesh_registry,
                &mut app.asset_manager,
                &mut candidate,
            )?;
            let mut staged_particles = aether_engine::particles::ParticleRuntime::default();
            let mut staged_time = app.cli.time.clone();
            super::super::particle_runtime::load_scene(
                &mut staged_particles,
                &candidate,
                &mut staged_time,
            )
            .map_err(anyhow::Error::msg)?;
            Ok::<_, anyhow::Error>((candidate, new_lighting, staged_particles, staged_time))
        })();
        match candidate_result {
            Ok((candidate, new_lighting, staged_particles, staged_time)) => {
                *world = candidate;
                *lighting = new_lighting;
                app.particle_runtime.particles = staged_particles;
                app.cli.time = staged_time;
                super::super::particle_runtime::physics_runtime::scene_switched(
                    &mut app.particle_runtime,
                );
                update_camera_from_world(&mut app.camera, world);
                app.pending_terrain_pipeline_rebuild = true;
                super::super::editor::activate_scene(app, path.to_path_buf());
                app.editor_operation_message = None;
                info!(path = %path.display(), "Opened scene");
            }
            Err(error) => {
                app.editor_operation_message = Some(format!("打开场景失败：{error}"));
                report_load_error(path, &error);
            }
        }
    }
}

pub(super) fn save_scene(app: &mut App, force_save_as: bool) -> bool {
    if let Some(reason) = app.editor_write_degraded.as_ref() {
        app.editor_operation_message = Some(format!(
            "场景写入已停止：{reason}。请重新打开场景或恢复磁盘文件。"
        ));
        return false;
    }
    if !app.document_state.can_transition() {
        app.editor_operation_message = Some("当前场景状态未知或需要恢复，暂不能保存。".into());
        return false;
    }
    if app.particle_runtime.playback
        != super::super::particle_runtime::physics_runtime::PlaybackState::Stopped
    {
        app.editor_operation_message = Some("请先停止模拟，再保存场景。".into());
        return false;
    }
    if !matches!(app.state, LauncherState::Running { .. }) {
        return false;
    }
    let existing_path = match app.document_state.identity() {
        super::super::document_state::DocumentIdentity::Saved(path) if !force_save_as => {
            Some(path.clone())
        }
        _ => None,
    };
    let Some(path) = existing_path.or_else(|| {
        rfd::FileDialog::new()
            .set_directory("./scenes")
            .set_file_name("scene.ron")
            .add_filter("RON scenes", &["ron"])
            .save_file()
    }) else {
        app.editor_operation_message = Some("保存已取消，场景没有切换。".into());
        return false;
    };
    let path = if !path.extension().is_some_and(|ext| ext == "ron") {
        path.with_extension("ron")
    } else {
        path
    };
    let scene_name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    match super::super::editor::serialize_active_scene(app, &scene_name) {
        Ok(ron) => match super::save::write_scene_atomic(&path, &ron) {
            Ok(()) => {
                if !super::super::editor::update_save_path(app, path.clone()) {
                    app.editor_operation_message =
                        Some("文件已写入，但无法确认当前文档状态；场景切换已取消。".into());
                    return false;
                }
                app.editor_operation_message = None;
                info!("Saved scene to {:?}", path);
                true
            }
            Err(error) => {
                app.editor_operation_message = Some(format!("保存失败：{error}"));
                error!("Save scene error: {:?}", error);
                false
            }
        },
        Err(error) => {
            app.editor_operation_message = Some(format!("场景序列化失败：{error}"));
            error!("Serialize scene error: {:?}", error);
            false
        }
    }
}

#[cfg(test)]
mod tests;
