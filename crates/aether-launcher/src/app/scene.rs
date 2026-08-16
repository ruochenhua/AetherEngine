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

/// Write camera state to the first `(Transform, Camera)` entity.
pub(crate) fn write_camera_to_world(camera: &FlyCamera, world: &mut World) {
    let target = world
        .query::<(Entity, (&Transform, &Camera))>()
        .iter()
        .next()
        .map(|(entity, _)| entity);
    if let Some(entity) = target {
        let _ = world.insert(
            entity,
            (
                Transform {
                    translation: camera.position,
                    rotation: glam::Quat::from_euler(
                        glam::EulerRot::YXZ,
                        camera.yaw,
                        camera.pitch,
                        0.0,
                    ),
                    scale: glam::Vec3::ONE,
                },
                Camera {
                    fov: camera.fov,
                    near: camera.near,
                    far: camera.far,
                    speed: camera.speed,
                },
            ),
        );
    }
}

/// Auto-open the scene provided via `--scene` on the command line.
pub(crate) fn open_cli_scene(app: &mut App, ctx: &RenderContext) {
    if let Some(scene_path) = &app.cli.scene {
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
                    if let Some((pos, yaw, pitch, fov, speed, near, far)) =
                        read_camera_from_world(world)
                    {
                        app.camera.position = pos;
                        app.camera.yaw = yaw;
                        app.camera.pitch = pitch;
                        app.camera.fov = fov;
                        app.camera.speed = speed;
                        app.camera.base_speed = speed;
                        app.camera.near = near;
                        app.camera.far = far;
                        app.camera.active = false;
                    }
                    // Queue a pipeline rebuild so the first frame after
                    // `resumed()` uses a scheduler that includes TerrainPass.
                    app.pending_terrain_pipeline_rebuild = true;
                }
                Err(e) => {
                    error!("Open scene error: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Process a pending scene load request from the menu.
pub(crate) fn process_pending_load(app: &mut App) {
    if let Some(idx) = app.pending_load.take() {
        let entry = &app.scene_entries[idx];
        if let LauncherState::Running {
            ref mut world,
            ref mut lighting,
        } = app.state
        {
            let ctx = app.ctx.as_ref().unwrap();
            match SceneLoader::open_scene(
                &entry.path,
                &ctx.device,
                &app.mesh_registry,
                &mut app.asset_manager,
                world,
            ) {
                Ok(new_lighting) => {
                    *lighting = new_lighting;
                    if let Some((pos, yaw, pitch, fov, speed, near, far)) =
                        read_camera_from_world(world)
                    {
                        app.camera.position = pos;
                        app.camera.yaw = yaw;
                        app.camera.pitch = pitch;
                        app.camera.fov = fov;
                        app.camera.speed = speed;
                        app.camera.base_speed = speed;
                        app.camera.near = near;
                        app.camera.far = far;
                        app.camera.active = false;
                    }
                    app.show_overlay = false;
                    app.pending_terrain_pipeline_rebuild = true;
                }
                Err(e) => {
                    error!("Open scene error: {:?}", e);
                }
            }
        }
    }
}

/// Process pending scene operations triggered by the editor UI.
mod ops;
pub(crate) use ops::process_post_ui_ops;
