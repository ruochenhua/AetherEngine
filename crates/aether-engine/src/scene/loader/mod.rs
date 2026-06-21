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
        _assets: &mut AssetManager,
        world: &mut World,
    ) -> anyhow::Result<()> {
        let desc = Self::from_file(path)?;
        objects::build_objects(&desc, device, registry, world)?;
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
        spawn::spawn_water(world, desc.water.as_ref());
        spawn::spawn_clouds(world, desc.clouds.as_ref());
        spawn::spawn_god_ray(world, desc.god_ray.as_ref());
        spawn::spawn_terrain(world, desc.terrain.as_ref(), assets);
        objects::build_objects(desc, device, registry, world)?;
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
            ssao_enabled: 1,
            shadow_enabled: 1,
            ibl_enabled: 1,
            _pad4: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::Transform;
    use crate::ecs::World;
    use crate::renderer::renderable::MaterialUniform;
    use crate::scene::{
        AtmosphereConfig, CameraConfig, LightConfig, MeshRef, ObjectConfig, TerrainConfig,
        TerrainGeometry, TerrainLayerConfig, TerrainSource, WaterConfig,
    };

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        let (device, _queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("need device");
        device
    }

    fn test_registry() -> BuiltinMeshRegistry {
        BuiltinMeshRegistry::new()
    }

    fn test_assets() -> AssetManager {
        AssetManager::new()
    }

    fn test_scene_desc() -> SceneDescription {
        SceneDescription {
            name: "Test".into(),
            camera: CameraConfig::default(),
            lights: vec![LightConfig::default()],
            ambient: 0.05,
            terrain: None,
            atmosphere: None,
            water: None,
            clouds: None,
            god_ray: None,
            objects: vec![
                ObjectConfig {
                    name: "cube_left".into(),
                    mesh: MeshRef::Builtin("cube".into()),
                    transform: crate::scene::TransformConfig {
                        translation: [-0.8, 0.0, 0.0],
                        ..Default::default()
                    },
                    material: crate::scene::MaterialConfig {
                        albedo: [0.8, 0.3, 0.2, 1.0],
                        roughness: 0.5,
                        metallic: 0.0,
                    },
                },
                ObjectConfig {
                    name: "sphere_right".into(),
                    mesh: MeshRef::Builtin("sphere".into()),
                    transform: crate::scene::TransformConfig {
                        translation: [0.8, 0.0, 0.0],
                        ..Default::default()
                    },
                    material: crate::scene::MaterialConfig {
                        albedo: [0.2, 0.5, 0.8, 1.0],
                        roughness: 0.05,
                        metallic: 0.0,
                    },
                },
            ],
        }
    }

    #[test]
    fn build_world_creates_correct_number_of_entities() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();

        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world)
            .expect("should build world");

        // camera + light + 2 objects = 4 entities
        assert_eq!(world.len(), 4);
    }

    #[test]
    fn build_world_sets_correct_material() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();

        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

        let mut found = false;
        for material in world.query::<&MaterialUniform>().iter() {
            if material.albedo == [0.8, 0.3, 0.2, 1.0] {
                found = true;
                assert_eq!(material.roughness, 0.5);
                break;
            }
        }
        assert!(found, "expected cube material in world");
    }

    #[test]
    fn build_world_sets_correct_transform() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();

        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

        let mut found = false;
        for transform in world.query::<&Transform>().iter() {
            if (transform.translation - Vec3::new(0.8, 0.0, 0.0)).length() < 0.001 {
                found = true;
                break;
            }
        }
        assert!(found, "expected sphere transform in world");
    }

    #[test]
    fn build_world_sets_lighting_uniforms() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();

        let mut assets = test_assets();
        let lighting =
            SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();
        assert_eq!(lighting.ambient_intensity, 0.05);
    }

    #[test]
    fn build_world_spawns_camera_entity() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();
        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

        let camera_count = world.query::<&Camera>().iter().count();
        assert_eq!(camera_count, 1);
    }

    #[test]
    fn build_world_spawns_light_entity() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();
        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

        let light_count = world.query::<&Light>().iter().count();
        assert_eq!(light_count, 1);
    }

    #[test]
    fn build_world_attaches_name_to_objects() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();
        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

        let names: Vec<String> = world.query::<&Name>().iter().map(|n| n.0.clone()).collect();
        assert!(names.contains(&"cube_left".to_string()));
        assert!(names.contains(&"sphere_right".to_string()));
    }

    #[test]
    fn build_world_attaches_name_to_scene_level_entities() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();
        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

        let names: Vec<String> = world.query::<&Name>().iter().map(|n| n.0.clone()).collect();
        assert!(names.contains(&"Camera".to_string()));
        assert!(names.contains(&"DirectionalLight".to_string()));
    }

    #[test]
    fn build_world_unknown_mesh_returns_error() {
        let device = headless_device();
        let registry = test_registry();
        let mut desc = test_scene_desc();
        desc.objects[0].mesh = MeshRef::Builtin("nonexistent".into());
        let mut world = World::new();
        let mut assets = test_assets();

        let result = SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world);
        assert!(result.is_err());
    }

    #[test]
    fn build_world_file_mesh_returns_error() {
        let device = headless_device();
        let registry = test_registry();
        let mut desc = test_scene_desc();
        desc.objects[0].mesh = MeshRef::File("foo.obj".into());
        let mut world = World::new();
        let mut assets = test_assets();

        let result = SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world);
        assert!(result.is_err());
    }

    #[test]
    fn open_scene_clears_world() {
        let device = headless_device();
        let registry = test_registry();
        let mut world = World::new();
        world.spawn((Transform::default(), Name("extra".into())));
        let mut assets = test_assets();

        let dir = std::env::temp_dir().join("aether_test_open_scene");
        let _ = std::fs::create_dir(&dir);
        let path = dir.join("open_scene.ron");
        std::fs::write(
            &path,
            r#"SceneDescription(
    name: "Open",
    camera: (position: (0.0, 0.0, 0.0)),
    lights: [],
    objects: [],
)"#,
        )
        .unwrap();

        SceneLoader::open_scene(&path, &device, &registry, &mut assets, &mut world).unwrap();
        // Only camera + default light should remain
        assert_eq!(world.len(), 2);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn import_scene_appends_objects() {
        let device = headless_device();
        let registry = test_registry();

        let dir = std::env::temp_dir().join("aether_test_import_scene");
        let _ = std::fs::create_dir(&dir);
        let path = dir.join("import_scene.ron");
        std::fs::write(
            &path,
            r#"SceneDescription(
    name: "Import",
    camera: (position: (1.0, 2.0, 3.0)),
    lights: [(light_type: Directional, direction: (0.0, -1.0, 0.0), color: (1.0, 1.0, 1.0), intensity: 1.0)],
    objects: [
        (name: "ImportedSphere", mesh: Builtin("sphere"), transform: (translation: (0.0, 0.0, 0.0)), material: (albedo: (1.0, 1.0, 1.0, 1.0))),
    ],
)"#,
        )
        .unwrap();

        let mut world = World::new();
        // Pre-populate with a camera and an object
        world.spawn((Transform::default(), Camera::default()));
        world.spawn((Transform::default(), Name("ExistingCube".into())));
        assert_eq!(world.len(), 2);

        let mut assets = test_assets();
        SceneLoader::import_scene(&path, &device, &registry, &mut assets, &mut world).unwrap();

        // 2 existing + 1 imported object = 3 entities (camera and light NOT imported)
        assert_eq!(world.len(), 3);

        let mut names: Vec<String> = world.query::<&Name>().iter().map(|n| n.0.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["ExistingCube", "ImportedSphere"]);

        // Verify existing camera is preserved
        let camera_count = world.query::<&Camera>().iter().count();
        assert_eq!(camera_count, 1);

        // cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn from_file_loads_valid_ron() {
        let dir = std::env::temp_dir().join("aether_test_scene");
        let _ = std::fs::create_dir(&dir);
        let path = dir.join("test_scene.ron");
        std::fs::write(
            &path,
            r#"SceneDescription(
    name: "From File",
    camera: (position: (1.0, 2.0, 3.0)),
)"#,
        )
        .unwrap();

        let desc = SceneLoader::from_file(&path).expect("should load");
        assert_eq!(desc.name, "From File");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn build_world_no_terrain_entity_when_missing() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();
        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

        let terrain_count = world
            .query::<&crate::ecs::components::Terrain>()
            .iter()
            .count();
        assert_eq!(terrain_count, 0);
    }

    #[test]
    fn build_world_spawns_terrain_entity_when_configured() {
        let device = headless_device();
        let registry = test_registry();
        let mut desc = test_scene_desc();
        desc.terrain = Some(TerrainConfig {
            source: TerrainSource::Procedural {
                seed: 0,
                frequency: 0.1,
                amplitude: 1.0,
            },
            geometry: TerrainGeometry {
                extent: 128.0,
                chunk_size: 32,
                max_lod: 2,
            },
            splatmap: None,
            layers: vec![TerrainLayerConfig::default()],
        });
        let mut world = World::new();
        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

        let terrain_count = world
            .query::<&crate::ecs::components::Terrain>()
            .iter()
            .count();
        assert_eq!(terrain_count, 1);
    }

    #[test]
    fn build_world_spawns_atmosphere_entity_when_configured() {
        let device = headless_device();
        let registry = test_registry();
        let mut desc = test_scene_desc();
        desc.atmosphere = Some(AtmosphereConfig::default());
        let mut world = World::new();
        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

        let count = world
            .query::<&crate::ecs::components::Atmosphere>()
            .iter()
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn build_world_spawns_water_entity_when_configured() {
        let device = headless_device();
        let registry = test_registry();
        let mut desc = test_scene_desc();
        desc.water = Some(WaterConfig::default());
        let mut world = World::new();
        let mut assets = test_assets();
        SceneLoader::build_world(&desc, &device, &registry, &mut assets, &mut world).unwrap();

        let count = world
            .query::<&crate::ecs::components::Water>()
            .iter()
            .count();
        assert_eq!(count, 1);
    }
}
