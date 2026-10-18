//! Scene-level operations for the launcher.

use super::{App, LauncherState, SceneEntry};
use aether_engine::{
    asset::mesh::GpuMesh,
    ecs::components::{
        Camera, MeshHandle, MeshSource, Name, Selected, Terrain, Transform, Visibility, Water,
    },
    ecs::{Entity, World},
    renderer::{camera::FlyCamera, context::RenderContext},
    scene::loader::SceneLoader,
};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};

/// Discover all `.ron` scene files in the `scenes` directory.
pub(crate) fn discover_scenes() -> Vec<SceneEntry> {
    let mut entries = Vec::new();
    let scenes_dir = std::path::Path::new("scenes");
    if !scenes_dir.is_dir() {
        return entries;
    }
    let Ok(dir) = std::fs::read_dir(scenes_dir) else {
        return entries;
    };
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "ron") {
            let name = match SceneLoader::from_file(&path) {
                Ok(desc) => desc.name,
                Err(_) => path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
            };
            entries.push(SceneEntry { name, path });
        }
    }
    entries.sort_by_key(|a| a.name.clone());
    entries
}

/// Read camera state from the first `(Transform, Camera)` entity.
pub(crate) fn read_camera_from_world(
    world: &World,
) -> Option<(glam::Vec3, f32, f32, f32, f32, f32, f32)> {
    world
        .query::<(&Transform, &Camera)>()
        .iter()
        .next()
        .map(|(transform, cam)| {
            let (yaw, pitch, _roll) = transform.rotation.to_euler(glam::EulerRot::YXZ);
            (
                transform.translation,
                yaw,
                pitch,
                cam.fov,
                cam.speed,
                cam.near,
                cam.far,
            )
        })
}

pub(crate) fn update_camera_from_world(camera: &mut FlyCamera, world: &World) {
    if let Some((pos, yaw, pitch, fov, speed, near, far)) = read_camera_from_world(world) {
        camera.position = pos;
        camera.yaw = yaw;
        camera.pitch = pitch;
        camera.fov = fov;
        camera.speed = speed;
        camera.base_speed = speed;
        camera.near = near;
        camera.far = far;
        camera.active = false;
    }
}

/// Auto-open the scene provided via `--scene` on the command line.
pub(crate) fn open_cli_scene(app: &mut App, ctx: &RenderContext) {
    if let Some(scene_path) = app.cli.scene.clone() {
        let path = PathBuf::from(scene_path);
        if let LauncherState::Running {
            ref mut world,
            ref mut lighting,
        } = app.state
        {
            match SceneLoader::open_scene(
                &path,
                &ctx.device,
                &app.mesh_registry,
                &mut app.asset_manager,
                world,
            ) {
                Ok(new_lighting) => {
                    *lighting = new_lighting;
                    if let Err(error) = super::particle_runtime::load_scene(
                        &mut app.particle_runtime.particles,
                        world,
                        &mut app.cli.time,
                    ) {
                        error!("Particle runtime scene initialization failed: {error}");
                    }
                    update_camera_from_world(&mut app.camera, world);
                    // Queue a pipeline rebuild so the first frame after
                    // `resumed()` uses a scheduler that includes TerrainPass.
                    app.pending_terrain_pipeline_rebuild = true;
                    super::editor::activate_scene(app, path.clone());
                }
                Err(e) => {
                    report_load_error(&path, &e);
                    // Keep the default empty scene alive so the launcher can
                    // still be used for manual scene discovery/editing.
                    super::editor::start_unsaved_scene(app);
                    super::editor::checkpoint_untitled_scene(app);
                }
            }
        }
    } else {
        super::editor::checkpoint_untitled_scene(app);
    }
}

/// Process a pending scene load request from the menu.
pub(crate) fn process_pending_load(app: &mut App) {
    if let Some(idx) = app.pending_load.take() {
        let path = app.scene_entries[idx].path.clone();
        transition::request_scene_transition(
            app,
            crate::app::scene_transition::SceneTransitionTarget::OpenPath(path),
        );
    }
}

/// Process pending scene operations triggered by the editor UI.
mod ops;
mod save;
mod transition;
pub(crate) use ops::process_post_ui_ops;
#[cfg(test)]
pub(crate) use ops::process_project_browser_open_with_device;
pub(crate) use transition::request_scene_transition;

pub(crate) fn report_load_error(path: &std::path::Path, error: &anyhow::Error) {
    tracing::error!(path = %path.display(), %error, "Open scene error");
}
