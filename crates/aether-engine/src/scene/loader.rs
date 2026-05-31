//! Scene loader.
//!
//! Converts RON scene descriptions into GPU resources (`Renderable` lists,
//! `LightingUniforms`) that the Launcher feeds into the deferred pipeline.

use crate::{
    asset::{mesh::GpuMesh, registry::BuiltinMeshRegistry},
    renderer::passes::{
        gbuffer::{MaterialUniform, Renderable},
        lighting::{DirectionalLight, LightingUniforms},
    },
    scene::{MeshRef, SceneDescription},
};
use glam::{Mat4, Quat, Vec3};
use std::path::Path;
use std::sync::Arc;

/// GPU resources built from a scene description.
pub struct SceneResources {
    /// All renderable objects in the scene.
    pub renderables: Vec<Renderable>,
    /// Lighting uniforms ready for upload.
    pub lighting_uniforms: LightingUniforms,
}

/// Loads RON scene files and builds GPU resources.
pub struct SceneLoader;

impl SceneLoader {
    /// Load a scene description from a `.ron` file.
    pub fn from_file(path: &Path) -> anyhow::Result<SceneDescription> {
        let content = std::fs::read_to_string(path)?;
        SceneDescription::from_ron(&content)
    }

    /// Build GPU resources from a scene description.
    ///
    /// - `Builtin` mesh references are resolved via the registry and uploaded
    ///   to the GPU.
    /// - `File` mesh references return an error in Phase 1.
    pub fn build_resources(
        desc: &SceneDescription,
        device: &wgpu::Device,
        registry: &BuiltinMeshRegistry,
    ) -> anyhow::Result<SceneResources> {
        let mut renderables = Vec::with_capacity(desc.objects.len());

        for obj in &desc.objects {
            let cpu_mesh = match &obj.mesh {
                MeshRef::Builtin(name) => registry
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("Unknown built-in mesh: '{}'", name))?,
                MeshRef::File(path) => {
                    anyhow::bail!("File mesh not yet supported: '{}' (Phase 1 limitation)", path);
                }
            };

            let gpu_mesh = Arc::new(GpuMesh::from_cpu(device, &cpu_mesh));

            // Build model matrix from transform config
            let transform = Mat4::from_scale_rotation_translation(
                Vec3::from_array(obj.transform.scale),
                Quat::from_array(obj.transform.rotation),
                Vec3::from_array(obj.transform.translation),
            );

            let material = MaterialUniform {
                albedo: obj.material.albedo,
                roughness: obj.material.roughness,
                metallic: obj.material.metallic,
                _pad: [0.0, 0.0],
            };

            renderables.push(Renderable {
                mesh: gpu_mesh,
                transform,
                material,
            });
        }

        // Build lighting uniforms
        let lighting_uniforms = Self::build_lighting_uniforms(desc);

        Ok(SceneResources {
            renderables,
            lighting_uniforms,
        })
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
            _pad2: [0.0; 2],
            light_view_proj: [[0.0; 4]; 4],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{CameraConfig, LightConfig, MeshRef, ObjectConfig};

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        let (device, _queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
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
    fn build_resources_creates_correct_number_of_renderables() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();

        let resources = SceneLoader::build_resources(&desc, &device, &registry)
            .expect("should build resources");

        assert_eq!(resources.renderables.len(), 2);
    }

    #[test]
    fn build_resources_sets_correct_material() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();

        let resources = SceneLoader::build_resources(&desc, &device, &registry).unwrap();
        let cube = &resources.renderables[0];
        assert_eq!(cube.material.albedo, [0.8, 0.3, 0.2, 1.0]);
        assert_eq!(cube.material.roughness, 0.5);
    }

    #[test]
    fn build_resources_sets_correct_transform() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();

        let resources = SceneLoader::build_resources(&desc, &device, &registry).unwrap();
        let sphere = &resources.renderables[1];
        let expected = Vec3::new(0.8, 0.0, 0.0);
        let actual = sphere.transform.w_axis.truncate();
        assert!((actual - expected).length() < 0.001);
    }

    #[test]
    fn build_resources_sets_lighting_uniforms() {
        let device = headless_device();
        let registry = test_registry();
        let desc = test_scene_desc();

        let resources = SceneLoader::build_resources(&desc, &device, &registry).unwrap();
        assert_eq!(resources.lighting_uniforms.ambient_intensity, 0.05);
        assert_eq!(
            resources.lighting_uniforms.light.direction,
            [0.0, -1.0, 0.0]
        );
    }

    #[test]
    fn build_resources_unknown_mesh_returns_error() {
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

        let result = SceneLoader::build_resources(&desc, &device, &registry);
        match result {
            Err(e) => assert!(e.to_string().contains("dragon")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn build_resources_file_mesh_returns_error() {
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

        let result = SceneLoader::build_resources(&desc, &device, &registry);
        match result {
            Err(e) => assert!(e.to_string().contains("Phase 1")),
            Ok(_) => panic!("expected error"),
        }
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
