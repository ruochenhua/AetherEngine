//! Scene loader.
//!
//! Converts RON scene descriptions into ECS entities and lighting uniforms.

use crate::{
    asset::{registry::BuiltinMeshRegistry, AssetManager},
    ecs::components::{Camera, Light, Name, Transform},
    ecs::World,
    renderer::light::LightingUniforms,
    scene::SceneDescription,
};
use glam::{Quat, Vec3};
use std::path::Path;
use tracing::warn;

mod lighting;
mod objects;
mod spawn;

/// Loads RON scene files and spawns entities into an ECS World.
pub struct SceneLoader;

impl SceneLoader {
    /// Load a scene description from a `.ron` file.
    pub fn from_file(path: &Path) -> anyhow::Result<SceneDescription> {
        let content = std::fs::read_to_string(path)?;
        SceneDescription::from_ron(&content)
    }

    /// Open a scene file, replacing all entities in the world.
    ///
    /// Clears the world, then spawns camera, lights, and objects from the RON.
    pub fn open_scene(
        path: &Path,
        device: &wgpu::Device,
        registry: &BuiltinMeshRegistry,
        assets: &mut AssetManager,
        world: &mut World,
    ) -> anyhow::Result<LightingUniforms> {
        let desc = Self::from_file(path)?;
        world.clear();
        Self::build_world(&desc, device, registry, assets, world)
    }

    /// Import objects from a `.ron` file into an existing world.
    ///
    /// Only object entities are appended. Camera, lights, lighting uniforms,
    /// and existing entities are preserved.
    pub fn import_scene(
        path: &Path,
        device: &wgpu::Device,
        registry: &BuiltinMeshRegistry,
        assets: &mut AssetManager,
        world: &mut World,
    ) -> anyhow::Result<()> {
        let desc = Self::from_file(path)?;
        objects::build_objects(&desc, device, registry, assets, world)?;
        Ok(())
    }

    /// Build scene entities into an ECS World.
    ///
    /// - Spawns one `(Transform, Camera)` entity from `desc.camera`.
    /// - Spawns one `(Transform, Light)` entity from `desc.lights[0]`.
    /// - Spawns one entity per object with `(Transform, MeshHandle, MaterialUniform, Visibility, Name)`.
    /// - `Builtin` mesh references are resolved via the registry.
    /// - `File` mesh references return an error.
    /// - Returns the lighting uniforms derived from the scene description.
    pub fn build_world(
        desc: &SceneDescription,
        device: &wgpu::Device,
        registry: &BuiltinMeshRegistry,
        assets: &mut AssetManager,
        world: &mut World,
    ) -> anyhow::Result<LightingUniforms> {
        if desc.lights.len() > 1 {
            warn!(
                "Scene contains {} lights; only the first light is currently supported and will be loaded",
                desc.lights.len()
            );
        }
        spawn::spawn_camera(world, &desc.camera);
        spawn::spawn_light(world, desc.lights.first());
        spawn::spawn_atmosphere(world, desc.atmosphere.as_ref());
        spawn::spawn_water(world, desc.water.as_ref(), assets);
        spawn::spawn_clouds(world, desc.clouds.as_ref());
        spawn::spawn_god_ray(world, desc.god_ray.as_ref());
        spawn::spawn_terrain(world, desc.terrain.as_ref(), assets);
        objects::build_objects(desc, device, registry, assets, world)?;
        Ok(lighting::build_lighting_uniforms(desc))
    }

    /// Create an empty scene with a default camera entity.
    ///
    /// Spawns a single camera entity into the world and returns default
    /// lighting uniforms. The camera is placed at the standard default
    /// position (3, 3, 3) looking toward the origin.
    pub fn new_empty(world: &mut World) -> LightingUniforms {
        let yaw = -std::f32::consts::FRAC_PI_4 - std::f32::consts::FRAC_PI_2;
        let pitch = -std::f32::consts::FRAC_PI_4;
        world.spawn((
            Transform {
                translation: Vec3::new(3.0, 3.0, 3.0),
                rotation: Quat::from_euler(glam::EulerRot::YXZ, yaw, pitch, 0.0),
                scale: Vec3::ONE,
            },
            Camera::default(),
            Name("Camera".into()),
        ));
        world.spawn((
            Transform {
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            Light::default(),
            Name("DirectionalLight".into()),
        ));

        LightingUniforms {
            camera_pos: [3.0, 3.0, 3.0],
            _pad1: 0.0,
            light: crate::renderer::light::DirectionalLight {
                direction: [0.0, -1.0, 0.0],
                _pad: 0.0,
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
            },
            ambient_intensity: 0.05,
            debug_mode: 0,
            shadow_normal_bias: 0.001,
            shadow_map_size: 2048.0,
            cascade_view_projs: [[[0.0; 4]; 4]; 4],
            cascade_splits: [0.0; 4],
            cascade_count: 4,
            _pad_cascade: [0; 3],
            inv_view_proj: [[0.0; 4]; 4],
            camera_forward: [0.0, 0.0, -1.0],
            _pad_cam: 0,
            ssao_enabled: 0,
            shadow_enabled: 1,
            ibl_enabled: 1,
            _pad4: 0,
        }
    }
}

#[cfg(test)]
mod tests;
