use super::{Scene, SceneEntity, TransformData};
use anyhow::Result;
use std::path::Path;
use tracing::info;

/// Scene serializer.
pub struct SceneSerializer;

impl SceneSerializer {
    /// Serialize a scene to a RON file.
    pub fn to_ron(scene: &Scene, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let content = ron::ser::to_string_pretty(
            scene,
            ron::ser::PrettyConfig::default(),
        )?;
        std::fs::write(path, content)?;
        info!("Scene '{}' serialized to {}", scene.name, path.display());
        Ok(())
    }

    /// Create a minimal test scene.
    pub fn create_test_scene() -> Scene {
        Scene {
            name: "Test Scene".to_string(),
            entities: vec![
                SceneEntity {
                    name: "Main Camera".to_string(),
                    transform: Some(TransformData {
                        translation: [0.0, 2.0, 5.0],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                        scale: [1.0, 1.0, 1.0],
                    }),
                    mesh: None,
                    light: None,
                    camera: Some(super::CameraData {
                        fov: 45.0f32.to_radians(),
                        near: 0.1,
                        far: 1000.0,
                    }),
                },
                SceneEntity {
                    name: "Directional Light".to_string(),
                    transform: Some(TransformData {
                        translation: [10.0, 10.0, 10.0],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                        scale: [1.0, 1.0, 1.0],
                    }),
                    mesh: None,
                    light: Some(super::LightData {
                        light_type: "directional".to_string(),
                        color: [1.0, 1.0, 1.0],
                        intensity: 1.0,
                        cast_shadow: true,
                    }),
                    camera: None,
                },
            ],
        }
    }
}
