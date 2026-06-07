//! ECS component definitions.
//!
//! Plain data structs implementing `hecs::Component`.

use crate::asset::mesh::GpuMesh;
use glam::{Quat, Vec3};
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
