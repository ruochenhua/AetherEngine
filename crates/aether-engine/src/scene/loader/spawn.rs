//! Entity spawning helpers for `SceneLoader`.

use crate::{
    asset::{
        terrain_material::{TerrainLayer, TerrainMaterial},
        texture::CpuTexture,
        AssetManager,
    },
    ecs::components::{Atmosphere, Camera, Clouds, GodRay, Light, Name, Terrain, Transform, Water},
    ecs::World,
    scene::{AtmosphereConfig, CloudConfig, GodRayConfig, TerrainConfig, WaterConfig},
};
use glam::{Quat, Vec3};

/// Spawn a camera entity from `CameraConfig`.
pub(super) fn spawn_camera(world: &mut World, camera: &crate::scene::CameraConfig) {
    let yaw = camera.yaw;
    let pitch = camera.pitch;
    world.spawn((
        Transform {
            translation: Vec3::from_array(camera.position),
            rotation: Quat::from_euler(glam::EulerRot::YXZ, yaw, pitch, 0.0),
            scale: Vec3::ONE,
        },
        Camera {
            fov: camera.fov.to_radians(),
            near: camera.near,
            far: camera.far,
            speed: camera.speed,
        },
        Name("Camera".into()),
    ));
}

/// Spawn a light entity from `LightConfig`.
///
/// If `light_cfg` is `None`, a default directional light is spawned so that
/// every loaded scene has a valid light source.
pub(super) fn spawn_light(world: &mut World, light_cfg: Option<&crate::scene::LightConfig>) {
    let (light_type, color, intensity, direction) = match light_cfg {
        Some(c) => (
            c.light_type,
            c.color,
            c.intensity,
            Vec3::from_array(c.direction),
        ),
        None => (
            crate::renderer::light::LightType::Directional,
            [1.0, 1.0, 1.0],
            1.0,
            Vec3::NEG_Y,
        ),
    };
    // For directional lights, the direction vector determines the rotation.
    // We compute a quaternion that rotates -Y (default light direction) to the target direction.
    let direction = direction.normalize();
    let rotation = if direction.abs_diff_eq(Vec3::NEG_Y, 1e-6) {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_arc(Vec3::NEG_Y, direction)
    };
    world.spawn((
        Transform {
            translation: Vec3::ZERO,
            rotation,
            scale: Vec3::ONE,
        },
        Light {
            light_type,
            color,
            intensity,
            cast_shadow: true,
        },
        Name("DirectionalLight".into()),
    ));
}

/// Spawn a single atmosphere entity from `AtmosphereConfig`.
pub(super) fn spawn_atmosphere(world: &mut World, atmos_cfg: Option<&AtmosphereConfig>) {
    let cfg = match atmos_cfg {
        Some(c) => c,
        None => return,
    };

    world.spawn((
        Transform::default(),
        Atmosphere {
            config: cfg.clone(),
        },
        Name("Atmosphere".into()),
    ));
}

/// Spawn a single water entity from `WaterConfig`.
pub(super) fn spawn_water(
    world: &mut World,
    water_cfg: Option<&WaterConfig>,
    assets: &mut AssetManager,
) {
    let cfg = match water_cfg {
        Some(c) => c,
        None => return,
    };

    let dudv_texture = cfg.dudv_map.as_ref().and_then(|path| {
        assets
            .load::<CpuTexture>(path)
            .map_err(|e| tracing::warn!("Failed to load water dudv map '{}': {}", path, e))
            .ok()
    });
    let normal_texture = cfg.normal_map.as_ref().and_then(|path| {
        assets
            .load::<CpuTexture>(path)
            .map_err(|e| tracing::warn!("Failed to load water normal map '{}': {}", path, e))
            .ok()
    });

    world.spawn((
        Transform::default(),
        Water {
            config: cfg.clone(),
            dudv_texture,
            normal_texture,
        },
        Name("Water".into()),
    ));
}

/// Spawn a single cloud entity from `CloudConfig`.
pub(super) fn spawn_clouds(world: &mut World, clouds_cfg: Option<&CloudConfig>) {
    let cfg = match clouds_cfg {
        Some(c) => c,
        None => return,
    };

    world.spawn((
        Transform::default(),
        Clouds {
            config: cfg.clone(),
        },
        Name("Clouds".into()),
    ));
}

/// Spawn a single god-ray entity from `GodRayConfig`.
pub(super) fn spawn_god_ray(world: &mut World, god_ray_cfg: Option<&GodRayConfig>) {
    let cfg = match god_ray_cfg {
        Some(c) => c,
        None => return,
    };

    world.spawn((
        Transform::default(),
        GodRay {
            config: cfg.clone(),
        },
        Name("GodRay".into()),
    ));
}

/// Spawn a single terrain entity from `TerrainConfig`.
pub(super) fn spawn_terrain(
    world: &mut World,
    terrain_cfg: Option<&TerrainConfig>,
    assets: &mut AssetManager,
) {
    let cfg = match terrain_cfg {
        Some(c) => c,
        None => return,
    };

    let material = build_terrain_material(cfg, assets);

    world.spawn((
        Transform::default(),
        Terrain {
            source: cfg.source.clone(),
            geometry: cfg.geometry.clone(),
            material,
            splatmap_path: cfg.splatmap.clone(),
            layer_configs: cfg.layers.clone(),
        },
        Name("Terrain".into()),
    ));
}

/// Build a runtime `TerrainMaterial` from a `TerrainConfig`.
///
/// Loads the splat map and any layer textures through `AssetManager`.
pub(super) fn build_terrain_material(
    cfg: &TerrainConfig,
    assets: &mut AssetManager,
) -> TerrainMaterial {
    let splat_map = cfg
        .splatmap
        .as_ref()
        .and_then(|path| assets.load::<CpuTexture>(path).ok());

    let mut layers = [
        TerrainLayer::default(),
        TerrainLayer::default(),
        TerrainLayer::default(),
        TerrainLayer::default(),
    ];

    for (i, layer_cfg) in cfg.layers.iter().take(4).enumerate() {
        layers[i] = TerrainLayer {
            albedo: layer_cfg.albedo,
            roughness: layer_cfg.roughness,
            metallic: layer_cfg.metallic,
            albedo_texture: layer_cfg
                .albedo_texture
                .as_ref()
                .and_then(|path| assets.load::<CpuTexture>(path).ok()),
            normal_texture: layer_cfg
                .normal_texture
                .as_ref()
                .and_then(|path| assets.load::<CpuTexture>(path).ok()),
            roughness_metallic_texture: layer_cfg
                .roughness_metallic_texture
                .as_ref()
                .and_then(|path| assets.load::<CpuTexture>(path).ok()),
            uv_scale: layer_cfg.uv_scale,
        };
    }

    TerrainMaterial { splat_map, layers }
}
