//! Extraction of optional scene and runtime data consumed by renderer passes.

use crate::ecs::components::{Atmosphere, Clouds, GodRay, Terrain, Water};
use crate::ecs::World;
use crate::renderer::lighting::LightingFrame;
use std::sync::Arc;

/// Optional scene components consumed by conditional render passes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptionalPassData {
    /// Unified extracted lighting frame shared by lighting and volumetric consumers.
    pub lighting: LightingFrame,
    /// Terrain component for `TerrainPass`.
    pub terrain: Option<Terrain>,
    /// Water component for `WaterPass`.
    pub water: Option<Water>,
    /// Atmosphere component for `AtmospherePass`.
    pub atmosphere: Option<Atmosphere>,
    /// Cloud component for `VolumetricCloudPass`.
    pub clouds: Option<Clouds>,
    /// God ray component for `GodRayPass`.
    pub god_ray: Option<GodRay>,
    /// Immutable particle snapshot consumed by the transparent billboard renderer.
    pub particles: Option<Arc<crate::particles::ParticleFrame>>,
    /// Optional collider wireframe snapshot, absent when physics debug is off.
    pub physics_debug_frame: Option<crate::physics::PhysicsDebugFrame>,
}

/// Extract optional pass data from the ECS World.
pub fn extract_optional_pass_data(world: &World) -> OptionalPassData {
    OptionalPassData {
        lighting: crate::renderer::lighting::extract_lighting_frame(world, 0.0),
        terrain: world.query::<&Terrain>().iter().next().cloned(),
        water: world.query::<&Water>().iter().next().cloned(),
        atmosphere: world.query::<&Atmosphere>().iter().next().cloned(),
        clouds: world.query::<&Clouds>().iter().next().cloned(),
        god_ray: world.query::<&GodRay>().iter().next().cloned(),
        particles: None,
        physics_debug_frame: None,
    }
}
