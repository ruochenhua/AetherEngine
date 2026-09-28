//! Simulation and ECS extraction before GPU frame application.

use super::super::{cli::CliArgs, particle_runtime};
use aether_engine::{
    asset::{texture::CpuTexture, AssetManager},
    ecs::World,
    particles::ParticleFrame,
    renderer::extract::{
        extract_optional_pass_data, extract_render_batches, OptionalPassData, RenderBatch,
    },
};
use std::sync::Arc;
use tracing::error;

pub(super) fn extract(
    particle_runtime_state: &mut aether_engine::particles::ParticleRuntime,
    physics_runtime_state: &mut Option<aether_engine::physics::PhysicsRuntime>,
    cli: &mut CliArgs,
    asset_manager: &mut AssetManager,
    freeze_time: bool,
    world: &mut World,
    dt: f32,
) -> (Vec<RenderBatch>, OptionalPassData) {
    let particle_frame: Option<Arc<ParticleFrame>> = if cli.particles_enabled {
        if particle_runtime::is_static_billboard_fixture(cli.scene.as_deref()) {
            let texture = asset_manager
                .load::<CpuTexture>("assets/models/cyborg/cyborg_diffuse.png")
                .ok();
            Some(super::particle_fixture::billboard_frame(texture))
        } else {
            match particle_runtime::advance(particle_runtime_state, &mut cli.time, dt) {
                Ok(frame) => Some(frame),
                Err(error) => {
                    error!("Particle simulation failed: {error}");
                    None
                }
            }
        }
    } else {
        None
    };

    let particle_published_clock = cli.particles_enabled
        && !particle_runtime::is_static_billboard_fixture(cli.scene.as_deref())
        && cli.time.mode != aether_engine::time::TimeMode::Seek;
    if let Err(error) = particle_runtime::physics_runtime::advance(
        physics_runtime_state,
        world,
        &mut cli.time,
        dt,
        freeze_time,
        particle_published_clock,
    ) {
        error!("Physics simulation failed: {error}");
    }

    let batches = extract_render_batches(world);
    let mut optional = extract_optional_pass_data(world);
    optional.particles = particle_frame;
    (batches, optional)
}
