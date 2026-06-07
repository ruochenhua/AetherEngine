//! Scene management module.
//!
//! Data types for describing 3D scenes declaratively. Scenes are serialized
//! as RON files and loaded by the Launcher.

pub mod loader;
pub mod serializer;

use crate::renderer::light::LightType;
use serde::{Deserialize, Serialize};

/// Mesh reference — either a built-in shape or an external file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MeshRef {
    /// Built-in mesh identified by name ("cube", "sphere", "quad").
    Builtin(String),
    /// External mesh file path.
    File(String),
}

// ---------------------------------------------------------------------------
// Scene description — the root of a RON scene file
// ---------------------------------------------------------------------------

/// Top-level scene description, deserialized from a `.ron` file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneDescription {
    /// Human-readable scene name.
    pub name: String,
    /// Camera initial configuration.
    pub camera: CameraConfig,
    /// Lights in the scene.
    #[serde(default)]
    pub lights: Vec<LightConfig>,
    /// Ambient light intensity (0.0 – 1.0).
    #[serde(default)]
    pub ambient: f32,
    /// Objects in the scene.
    #[serde(default)]
    pub objects: Vec<ObjectConfig>,
}

/// FlyCamera initial parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CameraConfig {
    /// World-space starting position [x, y, z].
    pub position: [f32; 3],
    /// Initial yaw angle in radians.
    #[serde(default)]
    pub yaw: f32,
    /// Initial pitch angle in radians.
    #[serde(default)]
    pub pitch: f32,
    /// Movement speed (units per second).
    #[serde(default = "default_camera_speed")]
    pub speed: f32,
    /// Vertical field of view in degrees.
    #[serde(default = "default_fov")]
    pub fov: f32,
}

fn default_camera_speed() -> f32 {
    4.0
}
fn default_fov() -> f32 {
    45.0
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            position: [3.0, 3.0, 3.0],
            yaw: -2.356,
            pitch: -0.785,
            speed: default_camera_speed(),
            fov: default_fov(),
        }
    }
}

/// Light configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LightConfig {
    /// Type of light.
    pub light_type: LightType,
    /// Light direction [x, y, z] (for Directional lights).
    #[serde(default)]
    pub direction: [f32; 3],
    /// Light color [r, g, b].
    #[serde(default = "default_light_color")]
    pub color: [f32; 3],
    /// Light intensity.
    #[serde(default = "default_intensity")]
    pub intensity: f32,
}

fn default_light_color() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}
fn default_intensity() -> f32 {
    1.0
}

impl Default for LightConfig {
    fn default() -> Self {
        Self {
            light_type: LightType::Directional,
            direction: [0.0, -1.0, 0.0],
            color: default_light_color(),
            intensity: default_intensity(),
        }
    }
}

/// Object (renderable entity) configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectConfig {
    /// Human-readable name for debugging.
    #[serde(default)]
    pub name: String,
    /// Mesh reference.
    pub mesh: MeshRef,
    /// Transform.
    #[serde(default)]
    pub transform: TransformConfig,
    /// PBR material parameters.
    #[serde(default)]
    pub material: MaterialConfig,
}

/// Transform data for an object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransformConfig {
    /// Translation [x, y, z].
    #[serde(default)]
    pub translation: [f32; 3],
    /// Rotation quaternion [x, y, z, w].
    #[serde(default = "default_rotation")]
    pub rotation: [f32; 4],
    /// Scale [x, y, z].
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
}

fn default_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}
fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

impl Default for TransformConfig {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: default_rotation(),
            scale: default_scale(),
        }
    }
}

/// PBR material parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaterialConfig {
    /// Albedo color [r, g, b, a].
    #[serde(default = "default_albedo")]
    pub albedo: [f32; 4],
    /// Surface roughness (0 = mirror, 1 = matte).
    #[serde(default)]
    pub roughness: f32,
    /// Metalness (0 = dielectric, 1 = metal).
    #[serde(default)]
    pub metallic: f32,
}

fn default_albedo() -> [f32; 4] {
    [0.8, 0.8, 0.8, 1.0]
}

impl Default for MaterialConfig {
    fn default() -> Self {
        Self {
            albedo: default_albedo(),
            roughness: 0.5,
            metallic: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// RON parsing
// ---------------------------------------------------------------------------

impl SceneDescription {
    /// Parse a scene from a RON string.
    pub fn from_ron(content: &str) -> anyhow::Result<Self> {
        let desc: SceneDescription = ron::de::from_str(content)?;
        Ok(desc)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_scene() {
        let ron = r#"
            SceneDescription(
                name: "Empty",
                camera: (position: (0.0, 0.0, 0.0)),
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        assert_eq!(scene.name, "Empty");
        assert!(scene.objects.is_empty());
        assert!(scene.lights.is_empty());
        assert_eq!(scene.ambient, 0.0);
    }

    #[test]
    fn parse_single_object_with_builtin_mesh() {
        let ron = r#"
            SceneDescription(
                name: "One Cube",
                camera: (position: (3.0, 3.0, 3.0)),
                objects: [
                    (
                        name: "MyCube",
                        mesh: Builtin("cube"),
                        transform: (
                            translation: (-0.8, 0.0, 0.0),
                        ),
                        material: (
                            albedo: (0.8, 0.3, 0.2, 1.0),
                            roughness: 0.5,
                            metallic: 0.0,
                        ),
                    ),
                ],
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        assert_eq!(scene.objects.len(), 1);
        let obj = &scene.objects[0];
        assert_eq!(obj.name, "MyCube");
        assert_eq!(obj.mesh, MeshRef::Builtin("cube".into()));
        assert_eq!(obj.transform.translation, [-0.8, 0.0, 0.0]);
        assert_eq!(obj.material.albedo, [0.8, 0.3, 0.2, 1.0]);
        assert_eq!(obj.material.roughness, 0.5);
    }

    #[test]
    fn parse_multiple_objects_with_light() {
        let ron = r#"
            SceneDescription(
                name: "Two Objects",
                camera: (position: (3.0, 3.0, 3.0)),
                ambient: 0.05,
                lights: [
                    (
                        light_type: Directional,
                        direction: (0.0, -1.0, 0.0),
                        color: (1.0, 1.0, 1.0),
                        intensity: 1.0,
                    ),
                ],
                objects: [
                    (
                        mesh: Builtin("cube"),
                        transform: (translation: (-0.8, 0.0, 0.0)),
                    ),
                    (
                        mesh: Builtin("sphere"),
                        transform: (translation: (0.8, 0.0, 0.0)),
                        material: (roughness: 0.05),
                    ),
                ],
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        assert_eq!(scene.objects.len(), 2);
        assert_eq!(scene.lights.len(), 1);
        assert_eq!(scene.ambient, 0.05);
        assert_eq!(scene.lights[0].light_type, LightType::Directional);
        assert_eq!(scene.lights[0].direction, [0.0, -1.0, 0.0]);
        // Second object should have default albedo
        assert_eq!(scene.objects[1].material.albedo, [0.8, 0.8, 0.8, 1.0]);
    }

    #[test]
    fn parse_with_file_mesh_reference() {
        let ron = r#"
            SceneDescription(
                name: "File Mesh",
                camera: (position: (0.0, 0.0, 0.0)),
                objects: [
                    (
                        name: "Dragon",
                        mesh: File("assets/models/dragon.obj"),
                    ),
                ],
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        assert_eq!(scene.objects.len(), 1);
        assert_eq!(scene.objects[0].mesh, MeshRef::File("assets/models/dragon.obj".into()));
    }

    #[test]
    fn parse_camera_full_config() {
        let ron = r#"
            SceneDescription(
                name: "Camera Test",
                camera: (
                    position: (5.0, 10.0, 5.0),
                    yaw: -1.5,
                    pitch: -0.5,
                    speed: 8.0,
                    fov: 60.0,
                ),
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        assert_eq!(scene.camera.position, [5.0, 10.0, 5.0]);
        assert_eq!(scene.camera.yaw, -1.5);
        assert_eq!(scene.camera.pitch, -0.5);
        assert_eq!(scene.camera.speed, 8.0);
        assert_eq!(scene.camera.fov, 60.0);
    }

    #[test]
    fn invalid_ron_returns_error() {
        let ron = "not valid ron {";
        let result = SceneDescription::from_ron(ron);
        assert!(result.is_err());
    }

    #[test]
    fn missing_camera_field_returns_error() {
        let ron = r#"
            SceneDescription(
                name: "No Camera",
            )
        "#;
        let result = SceneDescription::from_ron(ron);
        assert!(result.is_err());
    }

    #[test]
    fn all_defaults_populated() {
        let ron = r#"
            SceneDescription(
                name: "Defaults",
                camera: (position: (0.0, 0.0, 0.0)),
                objects: [
                    (mesh: Builtin("quad"),),
                ],
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        let obj = &scene.objects[0];
        assert_eq!(obj.name, ""); // default
        assert_eq!(obj.transform.translation, [0.0; 3]);
        assert_eq!(obj.transform.rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(obj.transform.scale, [1.0; 3]);
        assert_eq!(obj.material.albedo, [0.8, 0.8, 0.8, 1.0]);
        assert_eq!(obj.material.roughness, 0.5);
        assert_eq!(obj.material.metallic, 0.0);
    }

    #[test]
    fn parse_shadow_demo_has_7_objects() {
        let content = include_str!("../../../../scenes/03_shadow_demo.ron");
        let desc = SceneDescription::from_ron(content).expect("should parse");
        assert_eq!(desc.objects.len(), 7, "Expected 7 objects");
    }
}
