//! Scene serializer: ECS World → RON.
//!
//! Traverses the ECS World and produces a `SceneDescription` that can be
//! written to a `.ron` file. Editor-only components (`Selected`) are filtered
//! out automatically.

use crate::ecs::components::{Camera, MeshHandle, Transform, Visibility};
use crate::ecs::World;
use crate::renderer::light::LightingUniforms;
use crate::renderer::renderable::MaterialUniform;
use crate::scene::{
    CameraConfig, MeshRef, ObjectConfig, SceneDescription, TransformConfig, MaterialConfig,
};

/// Serialize the ECS World into a `SceneDescription`.
///
/// - Extracts camera from the first entity with `(Transform, Camera)`.
/// - Extracts objects from entities with `(Transform, MeshHandle, MaterialUniform, Visibility)`.
/// - Ignores entities with only editor components.
/// - Uses the provided lighting state for ambient and lights.
pub fn serialize_world(world: &World, lighting: &LightingUniforms, scene_name: &str) -> SceneDescription {
    let mut camera = CameraConfig::default();

    // Find camera entity
    for (transform, cam) in world.query::<(&Transform, &Camera)>().iter() {
        let (yaw, pitch, _roll) = transform.rotation.to_euler(glam::EulerRot::YXZ);
        camera = CameraConfig {
            position: transform.translation.to_array(),
            yaw,
            pitch,
            speed: 4.0,
            fov: cam.fov.to_degrees(),
        };
        break; // Use first camera only
    }

    // Extract objects
    let mut objects = Vec::new();
    for (transform, mesh_handle, material, _visibility) in world
        .query::<(&Transform, &MeshHandle, &MaterialUniform, &Visibility)>()
        .iter()
    {
        let obj = ObjectConfig {
            name: String::new(),
            mesh: MeshRef::Builtin(mesh_handle.name.clone()),
            transform: TransformConfig {
                translation: transform.translation.to_array(),
                rotation: transform.rotation.to_array(),
                scale: transform.scale.to_array(),
            },
            material: MaterialConfig {
                albedo: material.albedo,
                roughness: material.roughness,
                metallic: material.metallic,
            },
        };
        objects.push(obj);
    }

    SceneDescription {
        name: scene_name.to_string(),
        camera,
        lights: vec![], // TODO: persist lights when they become ECS entities
        ambient: lighting.ambient_intensity,
        objects,
    }
}

/// Serialize a `SceneDescription` to a pretty-printed RON string.
pub fn to_ron_string(desc: &SceneDescription) -> anyhow::Result<String> {
    let config = ron::ser::PrettyConfig::new()
        .depth_limit(4)
        .separate_tuple_members(true)
        .enumerate_arrays(false);
    let s = ron::ser::to_string_pretty(desc, config)?;
    Ok(s)
}
