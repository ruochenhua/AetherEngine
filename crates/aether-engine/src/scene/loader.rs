use super::{Scene, SceneEntity};
use crate::ecs::World;
use anyhow::{Context, Result};
use std::path::Path;
use tracing::{info, warn};

/// Scene loader.
pub struct SceneLoader;

impl SceneLoader {
    /// Load a scene from a file.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Scene> {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read scene file: {}", path.display()))?;

        match ext {
            "ron" => Self::load_from_ron(&content),
            "yaml" | "yml" => Self::load_from_yaml(&content),
            _ => {
                warn!("Unknown scene format: {}, trying RON", ext);
                Self::load_from_ron(&content)
            }
        }
    }

    /// Load scene from RON string.
    pub fn load_from_ron(content: &str) -> Result<Scene> {
        let scene: Scene = ron::de::from_str(content)
            .context("Failed to parse RON scene")?;
        info!("Loaded scene '{}' from RON ({} entities)", scene.name, scene.entities.len());
        Ok(scene)
    }

    /// Load scene from YAML string.
    pub fn load_from_yaml(content: &str) -> Result<Scene> {
        // TODO: Implement YAML scene loading for KongEngine compatibility
        warn!("YAML scene loading not yet implemented, falling back to RON");
        Self::load_from_ron(content)
    }

    /// Instantiate a scene into the ECS world.
    pub fn instantiate(scene: &Scene, world: &mut World) -> Result<Vec<crate::ecs::Entity>> {
        let mut entities = Vec::new();

        for entity_desc in &scene.entities {
            let entity = Self::spawn_entity(world, entity_desc)?;
            entities.push(entity);
        }

        info!("Instantiated {} entities from scene '{}'", entities.len(), scene.name);
        Ok(entities)
    }

    fn spawn_entity(
        world: &mut World,
        desc: &SceneEntity,
    ) -> Result<crate::ecs::Entity> {
        use crate::ecs::Component;
        use crate::renderer::camera::Camera;
        use crate::renderer::light::Light;
        use crate::renderer::mesh::Mesh;
        use crate::scene::TransformData;
        use glam::{Quat, Vec3};

        let mut components: Vec<Box<dyn hecs::Component>> = Vec::new();

        // Transform
        let transform = desc.transform.clone().unwrap_or_default();
        let transform_component = crate::ecs::Transform {
            translation: Vec3::from_array(transform.translation),
            rotation: Quat::from_array(transform.rotation),
            scale: Vec3::from_array(transform.scale),
        };

        // Note: hecs requires tuple bundles for spawn
        // We'll use a simpler approach - build a tuple dynamically
        let entity = world.spawn((transform_component,));

        // Add mesh if present
        if let Some(_mesh_data) = &desc.mesh {
            // TODO: Load mesh asset and attach Mesh component
        }

        // Add light if present
        if let Some(light_data) = &desc.light {
            let light = Light {
                light_type: match light_data.light_type.as_str() {
                    "directional" => crate::renderer::light::LightType::Directional,
                    "point" => crate::renderer::light::LightType::Point,
                    "spot" => crate::renderer::light::LightType::Spot,
                    _ => crate::renderer::light::LightType::Directional,
                },
                color: Vec3::from_array(light_data.color),
                intensity: light_data.intensity,
                cast_shadow: light_data.cast_shadow,
            };
            world.insert(entity, (light,)).ok();
        }

        // Add camera if present
        if let Some(camera_data) = &desc.camera {
            let camera = Camera {
                fov: camera_data.fov,
                near: camera_data.near,
                far: camera_data.far,
                aspect: 16.0 / 9.0,
            };
            world.insert(entity, (camera,)).ok();
        }

        Ok(entity)
    }
}

// Transform component for ECS
use crate::ecs::Component;

/// Transform component.
#[derive(Debug, Clone, Component)]
pub struct Transform {
    /// Translation.
    pub translation: Vec3,
    /// Rotation quaternion.
    pub rotation: Quat,
    /// Scale.
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    /// Compute the model matrix.
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_translation(self.translation)
            * Mat4::from_quat(self.rotation)
            * Mat4::from_scale(self.scale)
    }
}
