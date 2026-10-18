// Editor UI operations for scene management.

#[path = "playback.rs"]
mod playback;
#[path = "project_files_ops.rs"]
mod project_files_ops;

use super::transition::{process_scene_transition, request_scene_transition, save_scene};
use super::*;
use crate::app::scene_transition::SceneTransitionTarget;

pub(crate) fn process_post_ui_ops(app: &mut App) {
    playback::process_pending(app);
    project_files_ops::process(app);
    if app.pending_new_scene {
        app.pending_new_scene = false;
        request_scene_transition(app, SceneTransitionTarget::NewScene);
    }
    if app.pending_open_dialog {
        app.pending_open_dialog = false;
        request_scene_transition(app, SceneTransitionTarget::OpenDialog);
    }
    process_project_browser_open(app);
    process_scene_transition(app);
    if app.exit_requested {
        return;
    }

    // Import scene
    if app.pending_import_dialog {
        app.pending_import_dialog = false;
        let mut import_succeeded = false;
        if let Some(reason) = import_scene_block_reason(app) {
            app.editor_operation_message = Some(reason);
        } else if let Some(path) = rfd::FileDialog::new()
            .set_directory("./scenes")
            .add_filter("RON", &["ron"])
            .pick_file()
        {
            if let LauncherState::Running { ref mut world, .. } = app.state {
                let ctx = app.ctx.as_ref().unwrap();
                match SceneLoader::import_scene(
                    &path,
                    &ctx.device,
                    &app.mesh_registry,
                    &mut app.asset_manager,
                    world,
                ) {
                    Ok(()) => {
                        info!("Imported scene from {:?}", path);
                        app.pending_terrain_pipeline_rebuild = true;
                        import_succeeded = true;
                    }
                    Err(e) => {
                        error!("Import scene error: {:?}", e);
                    }
                }
            }
        }
        if import_succeeded {
            super::super::editor::refresh_document_state(app);
        }
    }

    if app.pending_save_dialog {
        app.pending_save_dialog = false;
        save_scene(app, false);
    }
    if app.pending_save_as_dialog {
        app.pending_save_as_dialog = false;
        save_scene(app, true);
    }

    // Handle add object actions
    let mut added_entity: Option<Entity> = None;

    let play_blocks_scene_edits = app.particle_runtime.playback
        != super::super::particle_runtime::physics_runtime::PlaybackState::Stopped;
    if play_blocks_scene_edits {
        app.pending_add_cube = false;
        app.pending_add_sphere = false;
        app.pending_add_terrain = false;
        app.pending_add_water = false;
        app.pending_despawn_entity = None;
        app.editor_operation_message = Some("请先停止模拟，再编辑场景。".into());
    } else if app.editor_write_degraded.is_some() {
        app.pending_add_cube = false;
        app.pending_add_sphere = false;
        app.pending_add_terrain = false;
        app.pending_add_water = false;
        app.pending_despawn_entity = None;
    }

    if app.pending_add_cube {
        app.pending_add_cube = false;
        if let LauncherState::Running { ref mut world, .. } = app.state {
            let ctx = app.ctx.as_ref().unwrap();
            if let Some(cpu_mesh) = app.mesh_registry.get("cube") {
                let gpu_mesh = Arc::new(GpuMesh::from_cpu(&ctx.device, &cpu_mesh));
                let entity = world.spawn((
                    Transform::default(),
                    MeshHandle::new(gpu_mesh, MeshSource::Builtin("cube".into()), "cube"),
                    aether_engine::renderer::renderable::MaterialUniform {
                        albedo: [0.8, 0.3, 0.2, 1.0],
                        roughness: 0.5,
                        metallic: 0.0,
                        unlit: 0,
                        _pad: 0,
                        albedo_texture_id: 0,
                        ..Default::default()
                    },
                    aether_engine::renderer::extracted_material::ExtractedMaterial::from_legacy_uniform(
                        aether_engine::renderer::renderable::MaterialUniform {
                            albedo: [0.8, 0.3, 0.2, 1.0],
                            roughness: 0.5,
                            metallic: 0.0,
                            unlit: 0,
                            _pad: 0,
                            albedo_texture_id: 0,
                            ..Default::default()
                        },
                    ),
                    Visibility::default(),
                    Name("Cube".into()),
                ));
                added_entity = Some(entity);
                info!("Added cube entity {:?}", entity);
            }
        }
    }

    if app.pending_add_sphere {
        app.pending_add_sphere = false;
        if let LauncherState::Running { ref mut world, .. } = app.state {
            let ctx = app.ctx.as_ref().unwrap();
            if let Some(cpu_mesh) = app.mesh_registry.get("sphere") {
                let gpu_mesh = Arc::new(GpuMesh::from_cpu(&ctx.device, &cpu_mesh));
                let entity = world.spawn((
                    Transform::default(),
                    MeshHandle::new(gpu_mesh, MeshSource::Builtin("sphere".into()), "sphere"),
                    aether_engine::renderer::renderable::MaterialUniform {
                        albedo: [0.2, 0.5, 0.8, 1.0],
                        roughness: 0.05,
                        metallic: 0.0,
                        unlit: 0,
                        _pad: 0,
                        albedo_texture_id: 0,
                        ..Default::default()
                    },
                    aether_engine::renderer::extracted_material::ExtractedMaterial::from_legacy_uniform(
                        aether_engine::renderer::renderable::MaterialUniform {
                            albedo: [0.2, 0.5, 0.8, 1.0],
                            roughness: 0.05,
                            metallic: 0.0,
                            unlit: 0,
                            _pad: 0,
                            albedo_texture_id: 0,
                            ..Default::default()
                        },
                    ),
                    Visibility::default(),
                    Name("Sphere".into()),
                ));
                added_entity = Some(entity);
                info!("Added sphere entity {:?}", entity);
            }
        }
    }

    if app.pending_add_terrain {
        app.pending_add_terrain = false;
        if let LauncherState::Running { ref mut world, .. } = app.state {
            // Replace any existing terrain entity.
            let existing: Vec<Entity> = world
                .query::<(Entity, &Terrain)>()
                .iter()
                .map(|(e, _)| e)
                .collect();
            for e in existing {
                let _ = world.despawn(e);
            }

            let terrain = Terrain {
                source: aether_engine::scene::TerrainSource::Procedural {
                    seed: 1,
                    frequency: 0.05,
                    amplitude: 32.0,
                },
                geometry: aether_engine::scene::TerrainGeometry {
                    extent: 256.0,
                    chunk_size: 64,
                    max_lod: 4,
                    albedo_tiling: 64.0,
                },
                material: aether_engine::asset::terrain_material::TerrainMaterial::default(),
                splatmap_path: None,
                layer_configs: vec![
                    aether_engine::scene::TerrainLayerConfig::default(),
                    aether_engine::scene::TerrainLayerConfig::default(),
                    aether_engine::scene::TerrainLayerConfig::default(),
                    aether_engine::scene::TerrainLayerConfig::default(),
                ],
            };
            let entity = world.spawn((Transform::default(), terrain, Name("Terrain".into())));
            added_entity = Some(entity);
            info!("Added terrain entity {:?}", entity);
        }
        app.pending_terrain_pipeline_rebuild = true;
    }

    if app.pending_add_water {
        app.pending_add_water = false;
        if let LauncherState::Running { ref mut world, .. } = app.state {
            // Replace any existing water entity.
            let existing: Vec<Entity> = world
                .query::<(Entity, &Water)>()
                .iter()
                .map(|(e, _)| e)
                .collect();
            for e in existing {
                let _ = world.despawn(e);
            }

            let water = Water {
                config: aether_engine::scene::WaterConfig::default(),
                dudv_texture: None,
                normal_texture: None,
            };
            let entity = world.spawn((Transform::default(), water, Name("Water".into())));
            added_entity = Some(entity);
            info!("Added water entity {:?}", entity);
        }
    }

    // Auto-select newly added entity
    if let Some(entity) = added_entity {
        if let LauncherState::Running { ref mut world, .. } = app.state {
            // Deselect all: collect first, then modify
            let to_deselect: Vec<_> = world
                .query::<(Entity, &Selected)>()
                .iter()
                .map(|(e, _)| e)
                .collect();
            for e in to_deselect {
                let _ = world.remove::<(Selected,)>(e);
            }
            // Select new entity
            let _ = world.insert(entity, (Selected,));
        }
        super::super::editor::refresh_document_state(app);
    }

    // Handle hierarchy panel selection
    if let Some(entity) = app.pending_select_entity.take() {
        if let LauncherState::Running { ref mut world, .. } = app.state {
            // Deselect all
            let to_deselect: Vec<_> = world
                .query::<(Entity, &Selected)>()
                .iter()
                .map(|(e, _)| e)
                .collect();
            for e in to_deselect {
                let _ = world.remove::<(Selected,)>(e);
            }
            // Select chosen entity
            let _ = world.insert(entity, (Selected,));
        }
    }

    // Handle despawn (delete entity)
    if let Some(entity) = app.pending_despawn_entity.take() {
        let mut despawned = false;
        if let LauncherState::Running { ref mut world, .. } = app.state {
            despawned = world.despawn(entity).is_ok();
        }
        app.pending_terrain_pipeline_rebuild = true;
        if despawned {
            super::super::editor::refresh_document_state(app);
        }
    }
}

fn import_scene_block_reason(app: &App) -> Option<String> {
    if let Some(reason) = app.editor_write_degraded.as_ref() {
        return Some(format!(
            "场景导入已停止：{reason}。请重新打开场景或恢复磁盘文件。"
        ));
    }
    if !app.document_state.can_transition() {
        return Some("当前场景状态未知或需要恢复，暂不能导入场景。".into());
    }
    if app.particle_runtime.playback
        != super::super::particle_runtime::physics_runtime::PlaybackState::Stopped
    {
        return Some("请先停止模拟，再导入场景。".into());
    }
    None
}

fn process_project_browser_open(app: &mut App) {
    // Browser requests can be produced while the shared workspace is visible
    // in Menu. Keep them pending until renderer startup has created a world.
    if matches!(app.state, LauncherState::Menu) {
        return;
    }
    if app.ctx.is_none() {
        return;
    }
    let Some(path) = app.pending_project_open.take() else {
        return;
    };
    request_scene_transition(app, SceneTransitionTarget::OpenPath(path));
}

#[cfg(test)]
pub(crate) fn process_project_browser_open_with_device(app: &mut App, device: &wgpu::Device) {
    if matches!(app.state, LauncherState::Menu) {
        return;
    }
    let Some(path) = app.pending_project_open.take() else {
        return;
    };
    if let LauncherState::Running {
        ref mut world,
        ref mut lighting,
    } = app.state
    {
        match SceneLoader::open_scene(
            &path,
            device,
            &app.mesh_registry,
            &mut app.asset_manager,
            world,
        ) {
            Ok(new_lighting) => {
                *lighting = new_lighting;
                super::super::particle_runtime::physics_runtime::scene_switched(
                    &mut app.particle_runtime,
                );
                if let Err(error) = super::super::particle_runtime::load_scene(
                    &mut app.particle_runtime.particles,
                    world,
                    &mut app.cli.time,
                ) {
                    error!("Particle runtime scene initialization failed: {error}");
                }
                update_camera_from_world(&mut app.camera, world);
                app.pending_terrain_pipeline_rebuild = true;
                super::super::editor::activate_scene(app, path.clone());
                info!(path = %path.display(), "Opened scene from project browser");
            }
            Err(error) => report_load_error(&path, &error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cli::parse_args;
    use aether_engine::{
        ecs::{components::Name, World},
        renderer::light::LightingUniforms,
    };

    #[test]
    fn degraded_editor_consumes_import_request_without_mutating_world() {
        let mut world = World::new();
        world.spawn((Name("Baseline".into()),));
        let mut app = App {
            editor_write_degraded: Some("scene restore failed".into()),
            pending_import_dialog: true,
            ..App::new(parse_args(&["aether-launcher".into()]).unwrap())
        };
        app.state = LauncherState::Running {
            world,
            lighting: LightingUniforms::default(),
        };

        process_post_ui_ops(&mut app);

        assert!(!app.pending_import_dialog);
        assert!(app
            .editor_operation_message
            .as_deref()
            .unwrap()
            .contains("场景导入已停止"));
        let LauncherState::Running { world, .. } = &app.state else {
            panic!("scene state should remain running")
        };
        assert!(world
            .query::<&Name>()
            .iter()
            .any(|name| name.0 == "Baseline"));
    }
}
