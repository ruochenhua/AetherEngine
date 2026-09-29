//! Serializable physics configuration attached to renderable scene objects.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Physics simulation settings for one scene object.
pub struct PhysicsConfig {
    /// Body mass, velocity, and static/dynamic mode.
    #[serde(default)]
    pub body: PhysicsBodyConfig,
    /// One through four shapes attached to the body.
    pub colliders: Vec<PhysicsColliderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Initial rigid-body properties from a scene file.
pub struct PhysicsBodyConfig {
    /// Positive dynamic-body mass in engine units.
    #[serde(default = "default_mass")]
    pub mass: f32,
    /// Whether the body is immovable.
    #[serde(default)]
    pub is_static: bool,
    /// Initial linear velocity in world units per second.
    #[serde(default)]
    pub velocity: [f32; 3],
    /// Initial angular velocity in radians per second.
    #[serde(default)]
    pub angular_velocity: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Collider shape and contact material settings.
pub struct PhysicsColliderConfig {
    /// Shape used by the physics backend.
    pub shape: PhysicsColliderShapeConfig,
    /// Whether the collider reports overlap without contact response.
    #[serde(default)]
    pub is_trigger: bool,
    /// Surface friction coefficient.
    #[serde(default = "default_friction")]
    pub friction: f32,
    /// Restitution coefficient in the range [0, 1].
    #[serde(default)]
    pub restitution: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Collider shape forms currently supported by scene fixtures.
pub enum PhysicsColliderShapeConfig {
    /// Ball collider described by its radius.
    Sphere {
        /// Sphere radius in local units.
        radius: f32,
    },
    /// Oriented box collider described by its half extents.
    Box {
        /// Positive half-size along each local axis.
        half_extents: [f32; 3],
    },
    /// Capsule collider, retained for lossless scene serialization.
    Capsule {
        /// Capsule radius.
        radius: f32,
        /// Cylindrical section height.
        height: f32,
    },
    /// Mesh collider marker, retained for lossless scene serialization.
    Mesh,
}

fn default_mass() -> f32 {
    1.0
}

fn default_friction() -> f32 {
    0.5
}

impl Default for PhysicsBodyConfig {
    fn default() -> Self {
        Self {
            mass: default_mass(),
            is_static: false,
            velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
        }
    }
}
