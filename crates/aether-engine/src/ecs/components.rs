//! ECS component definitions.
//!
//! Plain data structs implementing `hecs::Component`.

use crate::asset::mesh::GpuMesh;
use crate::asset::terrain_material::TerrainMaterial;
use crate::renderer::light::LightType;
use crate::scene::{TerrainGeometry, TerrainLayerConfig, TerrainSource};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// World-space transform (TRS decomposition).
///
/// Stored as translation + rotation + scale so that gizmo and inspector
/// can edit individual components without Mat4 decomposition.
#[derive(Clone, Debug, PartialEq)]
pub struct Transform {
    /// Translation vector.
    pub translation: Vec3,
    /// Rotation quaternion.
    pub rotation: Quat,
    /// Scale vector.
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

/// Source of a mesh reference — used to round-trip scenes through RON.
#[derive(Clone, Debug, PartialEq)]
pub enum MeshSource {
    /// Built-in mesh identified by name (e.g. "cube", "sphere").
    Builtin(String),
    /// External mesh file path.
    File(String),
}

/// Handle to a GPU mesh.
///
/// Shared ownership via `Arc` so that multiple entities can reference
/// the same mesh without duplicating GPU memory.
/// `source` stores whether the mesh came from a built-in shape or an external
/// file, which is what the serializer writes back to the scene file. `name`
/// is kept for UI labels and debugging.
#[derive(Clone)]
pub struct MeshHandle {
    /// GPU mesh data.
    pub mesh: Arc<GpuMesh>,
    /// Original mesh source for serialization.
    pub source: MeshSource,
    /// Human-readable mesh name for the UI / debug output.
    pub name: String,
}

impl MeshHandle {
    /// Create a new mesh handle.
    pub fn new(mesh: Arc<GpuMesh>, source: MeshSource, name: impl Into<String>) -> Self {
        Self {
            mesh,
            source,
            name: name.into(),
        }
    }
}

impl std::fmt::Debug for MeshHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeshHandle")
            .field("source", &self.source)
            .field("name", &self.name)
            .finish()
    }
}

/// Whether an entity should be rendered.
#[derive(Clone, Copy, Debug)]
pub struct Visibility(pub bool);

impl Default for Visibility {
    fn default() -> Self {
        Self(true)
    }
}

/// Editor marker: entity is currently selected.
#[derive(Clone, Copy, Debug)]
pub struct Selected;

/// Camera component.
///
/// Stores camera intrinsic parameters. Extrinsic parameters (position,
/// rotation) are stored in the `Transform` component on the same entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Vertical field of view in radians.
    pub fov: f32,
    /// Near clip plane.
    pub near: f32,
    /// Far clip plane.
    pub far: f32,
    /// Movement speed (units per second) for fly mode.
    pub speed: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov: 45.0f32.to_radians(),
            near: 0.1,
            far: 1000.0,
            speed: 4.0,
        }
    }
}

/// Light component for ECS entities.
///
/// Stores light properties for directional, point, or spot lights.
/// Paired with `Transform` on the same entity for position/direction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Light {
    /// Type of light.
    pub light_type: LightType,
    /// Light color [r, g, b].
    pub color: [f32; 3],
    /// Light intensity.
    pub intensity: f32,
    /// Whether this light casts shadows.
    pub cast_shadow: bool,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            light_type: LightType::Directional,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            cast_shadow: true,
        }
    }
}

impl Light {
    /// Create a directional light.
    pub fn directional(color: [f32; 3], intensity: f32) -> Self {
        Self {
            light_type: LightType::Directional,
            color,
            intensity,
            cast_shadow: true,
        }
    }

    /// Create a point light.
    pub fn point(color: [f32; 3], intensity: f32) -> Self {
        Self {
            light_type: LightType::Point,
            color,
            intensity,
            cast_shadow: false,
        }
    }

    /// Create a spot light.
    pub fn spot(color: [f32; 3], intensity: f32) -> Self {
        Self {
            light_type: LightType::Spot,
            color,
            intensity,
            cast_shadow: true,
        }
    }
}

/// Name component for ECS entities.
///
/// Stores a human-readable instance name (e.g. "MyCube") for display
/// in the hierarchy panel and serialization. Separate from `MeshHandle.name`,
/// which stores the mesh reference (e.g. "cube").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Name(pub String);

/// Atmosphere component.
///
/// Attached to a single scene-level entity when the scene description
/// contains an `atmosphere` section. The `AtmospherePass` reads this
/// component to render a physically based sky background.
#[derive(Clone, Debug, PartialEq)]
pub struct Atmosphere {
    /// Atmosphere configuration data.
    pub config: crate::scene::AtmosphereConfig,
}

/// Terrain component.
///
/// Attached to a single scene-level entity when the scene description
/// contains a `terrain` section. The `TerrainPass` reads this component
/// to generate and render chunked LOD geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct Terrain {
    /// Height data source.
    pub source: TerrainSource,
    /// Geometry generation strategy.
    pub geometry: TerrainGeometry,
    /// Runtime material with resolved texture handles.
    pub material: TerrainMaterial,
    /// Original splat map path for serialization round-trip.
    pub splatmap_path: Option<String>,
    /// Original layer configurations for serialization round-trip.
    pub layer_configs: Vec<TerrainLayerConfig>,
}

/// Water component.
///
/// Attached to a single scene-level entity when the scene description
/// contains a `water` section. The `WaterPass` reads this component
/// to render a transparent Gerstner-wave water surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Water {
    /// Water configuration data.
    pub config: crate::scene::WaterConfig,
}

/// Volumetric cloud component.
///
/// Attached to a single scene-level entity when the scene description
/// contains a `clouds` section. The `VolumetricCloudPass` reads this
/// component to render ray-marched cloud layers.
#[derive(Clone, Debug, PartialEq)]
pub struct Clouds {
    /// Cloud configuration data.
    pub config: crate::scene::CloudConfig,
}

/// God ray component.
///
/// Attached to a single scene-level entity when the scene description
/// contains a `god_ray` section. The `GodRayPass` reads this component
/// to render volumetric light shafts.
#[derive(Clone, Debug, PartialEq)]
pub struct GodRay {
    /// God ray configuration data.
    pub config: crate::scene::GodRayConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;

    #[test]
    fn spawn_light_entity_and_query() {
        let mut world = World::new();
        let light = Light {
            light_type: LightType::Directional,
            color: [1.0, 0.8, 0.6],
            intensity: 2.5,
            cast_shadow: true,
        };
        world.spawn((Transform::default(), light.clone()));

        let mut found = false;
        for (transform, l) in world.query::<(&Transform, &Light)>().iter() {
            found = true;
            assert_eq!(l.light_type, LightType::Directional);
            assert_eq!(l.color, [1.0, 0.8, 0.6]);
            assert_eq!(l.intensity, 2.5);
            assert_eq!(transform.translation, Vec3::ZERO);
        }
        assert!(found, "expected light entity in world");
    }

    #[test]
    fn spawn_named_entity_and_query() {
        let mut world = World::new();
        world.spawn((Transform::default(), Name("MyCube".into())));

        let mut found = false;
        for (transform, name) in world.query::<(&Transform, &Name)>().iter() {
            found = true;
            assert_eq!(name.0, "MyCube");
            assert_eq!(transform.translation, Vec3::ZERO);
        }
        assert!(found, "expected named entity in world");
    }

    #[test]
    fn light_is_serializable() {
        let light = Light {
            light_type: LightType::Point,
            color: [0.5, 0.5, 1.0],
            intensity: 1.0,
            cast_shadow: false,
        };
        let ron = ron::ser::to_string(&light).expect("should serialize");
        assert!(ron.contains("Point"));
        let deserialized: Light = ron::de::from_str(&ron).expect("should deserialize");
        assert_eq!(deserialized, light);
    }

    #[test]
    fn light_default_is_directional() {
        let light = Light::default();
        assert_eq!(light.light_type, LightType::Directional);
        assert_eq!(light.color, [1.0, 1.0, 1.0]);
        assert_eq!(light.intensity, 1.0);
        assert!(light.cast_shadow);
    }

    #[test]
    fn name_default_is_empty() {
        assert_eq!(Name::default().0, "");
    }
}
