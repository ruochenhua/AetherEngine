use crate::ecs::World;
use tracing::trace;

/// Physics system (placeholder).
///
/// Phase 1-3: Empty implementation.
/// Phase 4: Integrate rapier3d or custom physics solver.
pub fn physics_system(world: &mut World, dt: f32) {
    trace!("Physics system update (dt: {})", dt);

    // Phase 1-3: No physics simulation
    // The system exists to demonstrate the ECS architecture

    // Phase 4 TODO:
    // 1. Collect all RigidBody + Collider + Transform entities
    // 2. Update rapier3d world (or custom solver)
    // 3. Write back new transforms

    let _ = world; // Silence unused warning for placeholder
    let _ = dt;
}
