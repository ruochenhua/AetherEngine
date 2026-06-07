//! ECS component definitions.
//!
//! Plain data structs implementing `hecs::Component`.

use crate::asset::mesh::GpuMesh;
use crate::renderer::light::LightType;
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

/// Handle to a GPU mesh.
///
/// Shared ownership via `Arc` so that multiple entities can reference
/// the same mesh without duplicating GPU memory.
/// The `name` field stores the original mesh reference (e.g. "cube") for serialization.
#[derive(Clone)]
pub struct MeshHandle {
    /// GPU mesh data.
    pub mesh: Arc<GpuMesh>,
    /// Original mesh name (e.g. "cube") for serialization.
    pub name: String,
}

impl MeshHandle {
    /// Create a new mesh handle.
    pub fn new(mesh: Arc<GpuMesh>, name: impl Into<String>) -> Self {
        Self {
            mesh,
            name: name.into(),
        }
    }
}

impl std::fmt::Debug for MeshHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MeshHandle").field(&self.name).finish()
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
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Vertical field of view in radians.
    pub fov: f32,
    /// Near clip plane.
    pub near: f32,
    /// Far clip plane.
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov: 45.0f32.to_radians(),
            near: 0.1,
            far: 1000.0,
        }
    }
}

/// Light component for ECS entities.
///
/// Stores light properties for directional, point, or spot lights.
/// Paired with `Transform` on the same entity for position/direction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Light {
    /// Type of light.
    pub light_type: LightType,
    /// Light color [r, g, b].
    pub color: [f32; 3],
    /// Light intensity.
    pub intensity: f32,
}

/// Name component for ECS entities.
///
/// Stores a human-readable instance name (e.g. "MyCube") for display
/// in the hierarchy panel and serialization. Separate from `MeshHandle.name`,
/// which stores the mesh reference (e.g. "cube").
#[derive(Clone, Debug)]
pub struct Name(pub String);

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
        };
        let ron = ron::ser::to_string(&light).expect("should serialize");
        assert!(ron.contains("Point"));
        let deserialized: Light = ron::de::from_str(&ron).expect("should deserialize");
        assert_eq!(deserialized.light_type, LightType::Point);
        assert_eq!(deserialized.color, [0.5, 0.5, 1.0]);
    }
}
