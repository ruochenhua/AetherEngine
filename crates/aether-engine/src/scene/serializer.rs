//! Scene serializer: ECS World → RON.
//!
//! Traverses the ECS World and produces a `SceneDescription` that can be
//! written to a `.ron` file.
//!
//! ## Known Pitfalls
//! - **Name Component 遗漏**: `serialize_world` 查询 `(Transform, MeshHandle,
//!   MaterialUniform, Visibility, Name)`。未附加 `Name` 的 entity 会被静默跳过。
//!   所有 spawn 路径必须附加 `Name`。
//! - **相机保存前同步**: 保存前必须调用 `write_camera_to_world` 将 FlyCamera
//!   的 position/rotation 写回 ECS Camera Component，否则 RON 使用 stale 数据。

use crate::ecs::components::{
    Atmosphere, Camera, Clouds, GodRay, Light, MeshHandle, Name, Terrain, Transform, Visibility,
    Water,
};
use crate::ecs::World;
use crate::renderer::light::LightingUniforms;
use crate::renderer::renderable::MaterialUniform;
use crate::scene::{
    AtmosphereConfig, CameraConfig, CloudConfig, GodRayConfig, LightConfig, MaterialConfig,
    MeshRef, ObjectConfig, SceneDescription, TerrainConfig, TransformConfig, WaterConfig,
};
use glam::Vec3;

/// Serialize the ECS World into a `SceneDescription`.
///
/// - Extracts camera from the first entity with `(Transform, Camera)`.
/// - Extracts lights from entities with `(Transform, Light)`.
/// - Extracts objects from entities with `(Transform, MeshHandle, MaterialUniform, Visibility, Name)`.
/// - Ignores entities with only editor components.
/// - Uses the provided lighting state for ambient.
pub fn serialize_world(
    world: &World,
    lighting: &LightingUniforms,
    scene_name: &str,
) -> SceneDescription {
    let camera = extract_camera(world);
    let lights = extract_lights(world);
    let terrain = extract_terrain(world);
    let atmosphere = extract_atmosphere(world);
    let water = extract_water(world);
    let clouds = extract_clouds(world);
    let god_ray = extract_god_ray(world);
    let objects = extract_objects(world);

    SceneDescription {
        name: scene_name.to_string(),
        camera,
        lights,
        ambient: lighting.ambient_intensity,
        terrain,
        atmosphere,
        water,
        clouds,
        god_ray,
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

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn extract_camera(world: &World) -> CameraConfig {
    let mut camera = CameraConfig::default();

    if let Some((transform, cam)) = world.query::<(&Transform, &Camera)>().iter().next() {
        let (yaw, pitch, _roll) = transform.rotation.to_euler(glam::EulerRot::YXZ);
        camera = CameraConfig {
            position: transform.translation.to_array(),
            yaw,
            pitch,
            speed: cam.speed,
            fov: cam.fov.to_degrees(),
            near: cam.near,
            far: cam.far,
        };
    }
    camera
}

fn extract_lights(world: &World) -> Vec<LightConfig> {
    let mut lights = Vec::new();

    for (transform, light) in world.query::<(&Transform, &Light)>().iter() {
        // Direction is derived from rotation: default light direction is -Y,
        // so rotated direction = rotation * -Y.
        let direction = (transform.rotation * Vec3::NEG_Y).normalize().to_array();
        lights.push(LightConfig {
            light_type: light.light_type,
            direction,
            position: transform.translation.to_array(),
            color: light.color,
            intensity: light.intensity,
            range: light.range,
            inner_cone_angle: light.inner_cone_angle,
            outer_cone_angle: light.outer_cone_angle,
        });
    }

    lights
}

fn extract_terrain(world: &World) -> Option<TerrainConfig> {
    world
        .query::<&Terrain>()
        .iter()
        .next()
        .map(|terrain| TerrainConfig {
            source: terrain.source.clone(),
            geometry: terrain.geometry.clone(),
            splatmap: terrain.splatmap_path.clone(),
            layers: if terrain.layer_configs.is_empty() {
                vec![
                    crate::scene::TerrainLayerConfig::default(),
                    crate::scene::TerrainLayerConfig::default(),
                    crate::scene::TerrainLayerConfig::default(),
                    crate::scene::TerrainLayerConfig::default(),
                ]
            } else {
                terrain.layer_configs.clone()
            },
        })
}

fn extract_atmosphere(world: &World) -> Option<AtmosphereConfig> {
    world
        .query::<&Atmosphere>()
        .iter()
        .next()
        .map(|atmos| atmos.config.clone())
}

fn extract_water(world: &World) -> Option<WaterConfig> {
    world
        .query::<&Water>()
        .iter()
        .next()
        .map(|water| water.config.clone())
}

fn extract_clouds(world: &World) -> Option<CloudConfig> {
    world
        .query::<&Clouds>()
        .iter()
        .next()
        .map(|clouds| clouds.config.clone())
}

fn extract_god_ray(world: &World) -> Option<GodRayConfig> {
    world
        .query::<&GodRay>()
        .iter()
        .next()
        .map(|gr| gr.config.clone())
}

fn extract_objects(world: &World) -> Vec<ObjectConfig> {
    let mut objects = Vec::new();
    for (transform, mesh_handle, material, visibility, name, stored_config) in world
        .query::<(
            &Transform,
            &MeshHandle,
            &MaterialUniform,
            &Visibility,
            &Name,
            Option<&MaterialConfig>,
        )>()
        .iter()
    {
        let mesh_ref = match &mesh_handle.source {
            crate::ecs::components::MeshSource::Builtin(name) => MeshRef::Builtin(name.clone()),
            crate::ecs::components::MeshSource::File(path) => MeshRef::File(path.clone()),
        };

        let obj = ObjectConfig {
            name: name.0.clone(),
            mesh: mesh_ref,
            transform: TransformConfig {
                translation: transform.translation.to_array(),
                rotation: transform.rotation.to_array(),
                scale: transform.scale.to_array(),
            },
            material: stored_config.cloned().unwrap_or_else(|| MaterialConfig {
                albedo: material.albedo,
                roughness: material.roughness,
                metallic: material.metallic,
                unlit: material.unlit != 0,
                albedo_texture: None,
                ..MaterialConfig::default()
            }),
            visible: visibility.0,
        };
        objects.push(obj);
    }

    objects
}

#[cfg(test)]
mod tests;
