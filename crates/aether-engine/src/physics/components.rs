use glam::Vec3;

/// Rigid body component.
///
/// Marker + data for physics simulation.
/// Currently stores velocity and mass; actual simulation is deferred to Phase 4.
#[derive(Debug, Clone)]
pub struct RigidBody {
    /// Linear velocity.
    pub velocity: Vec3,
    /// Angular velocity.
    pub angular_velocity: Vec3,
    /// Mass (0.0 = static).
    pub mass: f32,
    /// Is this body static (immovable)?
    pub is_static: bool,
}


impl Default for RigidBody {
    fn default() -> Self {
        Self {
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            mass: 1.0,
            is_static: false,
        }
    }
}

/// Collider shape.
#[derive(Debug, Clone)]
pub enum ColliderShape {
    /// Sphere with radius.
    Sphere(f32),
    /// Axis-aligned bounding box (half-extents).
    Box(Vec3),
    /// Capsule (radius, height).
    Capsule(f32, f32),
    /// Triangle mesh (mesh handle).
    Mesh, // TODO: Add mesh handle
}

/// Collider component.
#[derive(Debug, Clone)]
pub struct Collider {
    /// Collider shape.
    pub shape: ColliderShape,
    /// Is this a trigger (no collision response)?
    pub is_trigger: bool,
    /// Friction coefficient.
    pub friction: f32,
    /// Restitution (bounciness).
    pub restitution: f32,
}


impl Default for Collider {
    fn default() -> Self {
        Self {
            shape: ColliderShape::Sphere(1.0),
            is_trigger: false,
            friction: 0.5,
            restitution: 0.0,
        }
    }
}
