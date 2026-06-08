//! Scene loader.
//!
//! Converts RON scene descriptions into ECS entities and lighting uniforms.

use crate::{
    asset::{mesh::GpuMesh, registry::BuiltinMeshRegistry},
    ecs::components::{Camera, Light, MeshHandle, Name, Transform, Visibility},
    ecs::World,
    renderer::light::{DirectionalLight, LightType, LightingUniforms},
    renderer::renderable::MaterialUniform,
    scene::{MeshRef, SceneDescription},
};
use glam::{Quat, Vec3};
use std::path::Path;
use std::sync::Arc;

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
        world: &mut World,
    ) -> anyhow::Result<LightingUniforms> {
        let desc = Self::from_file(path)?;
        world.clear();
        Self::build_world(&desc, device, registry, world)
    }

    /// Import objects from a `.ron` file into an existing world.
    ///
    /// Only object entities are appended. Camera, lights, and existing
    /// entities are preserved.
    pub fn import_scene(
        path: &Path,
        device: &wgpu::Device,
        registry: &BuiltinMeshRegistry,
        world: &mut World,
    ) -> anyhow::Result<LightingUniforms> {
        let desc = Self::from_file(path)?;
        Self::build_objects(&desc, device, registry, world)?;
        Ok(Self::build_lighting_uniforms(&desc))
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
        world: &mut World,
    ) -> anyhow::Result<LightingUniforms> {
        Self::spawn_camera(world, &desc.camera);
        Self::spawn_light(world, desc.lights.first());
        Self::build_objects(desc, device, registry, world)?;
        Ok(Self::build_lighting_uniforms(desc))
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
        ));
        world.spawn((
            Transform {
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            Light {
                light_type: LightType::Directional,
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
                cast_shadow: true,
            },
        ));

        LightingUniforms {
            camera_pos: [3.0, 3.0, 3.0],
            _pad1: 0.0,
            light: DirectionalLight {
                direction: [0.0, -1.0, 0.0],
                _pad: 0.0,
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
            },
            ambient_intensity: 0.05,
            debug_mode: 0,
            shadow_normal_bias: 0.001,
            shadow_map_size: 2048.0,
            light_view_proj: [[0.0; 4]; 4],
            inv_view_proj: [[0.0; 4]; 4],
            ssao_enabled: 1,
            shadow_enabled: 1,
            ibl_enabled: 1,
            _pad4: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Spawn a camera entity from `CameraConfig`.
    fn spawn_camera(world: &mut World, camera: &crate::scene::CameraConfig) {
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
                ..Camera::default()
            },
        ));
    }

    /// Spawn a light entity from `LightConfig`.
    fn spawn_light(world: &mut World, light_cfg: Option<&crate::scene::LightConfig>) {
        let cfg = match light_cfg {
            Some(c) => c,
            None => return,
        };
        // For directional lights, the direction vector determines the rotation.
        // We compute a quaternion that rotates -Y (default light direction) to cfg.direction.
        let direction = Vec3::from_array(cfg.direction).normalize();
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
                light_type: cfg.light_type,
                color: cfg.color,
                intensity: cfg.intensity,
                cast_shadow: true,
            },
        ));
    }

    /// Spawn object entities from `SceneDescription.objects`.
    fn build_objects(
        desc: &SceneDescription,
        device: &wgpu::Device,
        registry: &BuiltinMeshRegistry,
        world: &mut World,
    ) -> anyhow::Result<()> {
        for obj in &desc.objects {
            let mesh_name = match &obj.mesh {
                MeshRef::Builtin(name) => name.clone(),
                MeshRef::File(path) => {
                    anyhow::bail!("File mesh not yet supported: '{}' (Phase 1 limitation)", path);
                }
            };
            let cpu_mesh = registry
                .get(&mesh_name)
                .ok_or_else(|| anyhow::anyhow!("Unknown built-in mesh: '{}'", mesh_name))?;

            let gpu_mesh = Arc::new(GpuMesh::from_cpu(device, &cpu_mesh));

            let transform = Transform {
                translation: Vec3::from_array(obj.transform.translation),
                rotation: Quat::from_array(obj.transform.rotation),
                scale: Vec3::from_array(obj.transform.scale),
            };

            let material = MaterialUniform {
                albedo: obj.material.albedo,
                roughness: obj.material.roughness,
                metallic: obj.material.metallic,
                _pad: [0.0, 0.0],
            };

            world.spawn((
                transform,
                MeshHandle::new(gpu_mesh, mesh_name),
                material,
                Visibility::default(),
                Name(obj.name.clone()),
            ));
        }
        Ok(())
    }

    /// Construct lighting uniforms from scene lights and ambient.
    fn build_lighting_uniforms(desc: &SceneDescription) -> LightingUniforms {
        let light = desc.lights.first().map_or_else(
            || DirectionalLight {
                direction: [0.0, -1.0, 0.0],
                _pad: 0.0,
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
            },
            |cfg| DirectionalLight {
                direction: cfg.direction,
                _pad: 0.0,
                color: cfg.color,
                intensity: cfg.intensity,
            },
        );

        LightingUniforms {
            camera_pos: desc.camera.position,
            _pad1: 0.0,
            light,
            ambient_intensity: desc.ambient,
            debug_mode: 0,
            shadow_normal_bias: 0.001,
            shadow_map_size: 2048.0,
            light_view_proj: [[0.0; 4]; 4],
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
    use crate::scene::{CameraConfig, LightConfig, MeshRef, ObjectConfig};

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

    fn test_scene_desc() -> SceneDescription {
        SceneDescription {
            name: "Test".into(),
            camera: CameraConfig::default(),
            lights: vec![LightConfig::default()],
            ambient: 0.05,
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

        SceneLoader::build_world(&desc, &device, &registry, &mut world)
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

        SceneLoader::build_world(&desc, &device, &registry, &mut world).unwrap();

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

        SceneLoader::build_world(&desc, &device, &registry, &mut world).unwrap();

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

        let lighting =
            SceneLoader::build_world(&desc, &device, &registry, &mut world).unwrap();
        assert_eq!(lighting.ambient_intensity, 0.05);
        assert_eq!(lighting.light.direction, [0.0, -1.0, 0.0]);
    }

    #[test]
    fn build_world_spawns_camera_entity() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();

        SceneLoader::build_world(&desc, &device, &registry, &mut world).unwrap();

        let mut found = false;
        for (transform, camera) in world.query::<(&Transform, &Camera)>().iter() {
            found = true;
            assert_eq!(transform.translation, Vec3::new(3.0, 3.0, 3.0));
            assert!((camera.fov - 45.0f32.to_radians()).abs() < 0.001);
        }
        assert!(found, "expected camera entity in world");
    }

    #[test]
    fn build_world_spawns_light_entity() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();

        SceneLoader::build_world(&desc, &device, &registry, &mut world).unwrap();

        let mut found = false;
        for (_transform, light) in world.query::<(&Transform, &Light)>().iter() {
            found = true;
            assert_eq!(light.light_type, crate::renderer::light::LightType::Directional);
            assert_eq!(light.color, [1.0, 1.0, 1.0]);
            assert_eq!(light.intensity, 1.0);
        }
        assert!(found, "expected light entity in world");
    }

    #[test]
    fn build_world_attaches_name_to_objects() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();
        let mut world = World::new();

        SceneLoader::build_world(&desc, &device, &registry, &mut world).unwrap();

        let mut found_cube = false;
        let mut found_sphere = false;
        for name in world.query::<&Name>().iter() {
            match name.0.as_str() {
                "cube_left" => found_cube = true,
                "sphere_right" => found_sphere = true,
                _ => {}
            }
        }
        assert!(found_cube, "expected cube_left name in world");
        assert!(found_sphere, "expected sphere_right name in world");
    }

    #[test]
    fn build_world_unknown_mesh_returns_error() {
        let device = headless_device();
        let registry = test_registry();
        let desc = SceneDescription {
            name: "Bad".into(),
            camera: CameraConfig::default(),
            lights: vec![],
            ambient: 0.0,
            objects: vec![ObjectConfig {
                name: "nope".into(),
                mesh: MeshRef::Builtin("dragon".into()),
                transform: Default::default(),
                material: Default::default(),
            }],
        };
        let mut world = World::new();

        let result = SceneLoader::build_world(&desc, &device, &registry, &mut world);
        match result {
            Err(e) => assert!(e.to_string().contains("dragon")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn build_world_file_mesh_returns_error() {
        let device = headless_device();
        let registry = test_registry();
        let desc = SceneDescription {
            name: "File".into(),
            camera: CameraConfig::default(),
            lights: vec![],
            ambient: 0.0,
            objects: vec![ObjectConfig {
                name: "dragon".into(),
                mesh: MeshRef::File("assets/dragon.obj".into()),
                transform: Default::default(),
                material: Default::default(),
            }],
        };
        let mut world = World::new();

        let result = SceneLoader::build_world(&desc, &device, &registry, &mut world);
        match result {
            Err(e) => assert!(e.to_string().contains("Phase 1")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn open_scene_clears_world() {
        let device = headless_device();
        let registry = test_registry();
        let dir = std::env::temp_dir().join("aether_test_open_scene");
        let _ = std::fs::create_dir(&dir);
        let path = dir.join("test_scene.ron");
        std::fs::write(
            &path,
            r#"SceneDescription(
    name: "Open Test",
    camera: (position: (1.0, 2.0, 3.0)),
    lights: [(
        light_type: Directional,
        direction: (0.0, -1.0, 0.0),
        color: (1.0, 1.0, 1.0),
        intensity: 1.0,
    )],
    objects: [
        (mesh: Builtin("cube"), name: "OnlyCube"),
    ],
)"#,
        )
        .unwrap();

        let mut world = World::new();
        // Pre-populate with a stray entity
        world.spawn((Transform::default(),));
        assert_eq!(world.len(), 1);

        SceneLoader::open_scene(&path, &device, &registry, &mut world).unwrap();

        // camera + light + 1 object = 3 entities
        assert_eq!(world.len(), 3);

        // Verify the stray entity was cleared
        let mut count = 0;
        for _ in world.query::<&Name>().iter() {
            count += 1;
        }
        assert_eq!(count, 1);

        // cleanup
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
    name: "Import Test",
    camera: (position: (5.0, 5.0, 5.0)),
    objects: [
        (mesh: Builtin("sphere"), name: "ImportedSphere"),
    ],
)"#,
        )
        .unwrap();

        let mut world = World::new();
        // Pre-populate with a camera and an object
        world.spawn((Transform::default(), Camera::default()));
        world.spawn((Transform::default(), Name("ExistingCube".into())));
        assert_eq!(world.len(), 2);

        SceneLoader::import_scene(&path, &device, &registry, &mut world).unwrap();

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
        assert_eq!(desc.camera.position, [1.0, 2.0, 3.0]);

        // cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
