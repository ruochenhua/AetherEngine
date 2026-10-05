//! Simulation and ECS extraction before GPU frame application.

use super::super::{cli::CliArgs, particle_runtime};
use aether_engine::{
    asset::{texture::CpuTexture, AssetManager},
    ecs::World,
    particles::ParticleFrame,
    renderer::extract::{
        extract_optional_pass_data, extract_render_batches_with_instancing, OptionalPassData,
        RenderBatch,
    },
    renderer::{
        camera::FlyCamera,
        visibility::{extract_visibility, LodHistory, VisibilityFrame, VisibilityPolicy},
    },
};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::error;

pub(super) struct RenderExtractContext<'a> {
    pub simulation_paused: bool,
    pub dt: f32,
    pub camera: &'a FlyCamera,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub lod_history: &'a mut LodHistory,
}

pub(super) fn extract(
    particle_runtime_state: &mut aether_engine::particles::ParticleRuntime,
    physics_runtime_state: &mut Option<aether_engine::physics::PhysicsRuntime>,
    cli: &mut CliArgs,
    asset_manager: &mut AssetManager,
    world: &mut World,
    context: RenderExtractContext<'_>,
) -> (Vec<RenderBatch>, OptionalPassData, Option<VisibilityFrame>) {
    let RenderExtractContext {
        simulation_paused,
        dt,
        camera,
        viewport_width,
        viewport_height,
        lod_history,
    } = context;
    let simulation_dt = if simulation_paused { 0.0 } else { dt };
    let particle_frame: Option<Arc<ParticleFrame>> = if cli.particles_enabled {
        if particle_runtime::is_static_billboard_fixture(cli.scene.as_deref()) {
            let texture = asset_manager
                .load::<CpuTexture>("assets/models/cyborg/cyborg_diffuse.png")
                .ok();
            Some(super::particle_fixture::billboard_frame(texture))
        } else {
            match particle_runtime::advance(particle_runtime_state, &mut cli.time, simulation_dt) {
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
        simulation_dt,
        simulation_paused,
        particle_published_clock,
        cli.physics_debug_enabled,
    ) {
        error!("Physics simulation failed: {error}");
    }

    let benchmark_policy = cli.benchmark.as_ref().map(|benchmark| {
        (benchmark.visibility_enabled() || benchmark.lod_enabled()).then_some(VisibilityPolicy {
            frustum_culling: benchmark.visibility_enabled(),
            lod_enabled: benchmark.lod_enabled(),
        })
    });
    let instancing_enabled = cli
        .benchmark
        .as_ref()
        .map_or(true, |benchmark| benchmark.instancing_enabled());
    let policy = benchmark_policy.flatten().or_else(|| {
        cli.benchmark
            .is_none()
            .then_some(VisibilityPolicy::default())
    });
    let (batches, visibility) = if let Some(policy) = policy {
        let frame = extract_visibility(
            world,
            camera,
            viewport_width,
            viewport_height,
            policy,
            lod_history,
        );
        let visible_ids: HashSet<u32> = frame
            .items
            .iter()
            .map(|item| item.entity_bits as u32)
            .collect();
        let mut batches = extract_render_batches_with_instancing(world, None, instancing_enabled);
        for batch in &mut batches {
            batch
                .instances
                .retain(|instance| visible_ids.contains(&instance.entity_id));
        }
        batches.retain(|batch| !batch.instances.is_empty());
        (batches, Some(frame))
    } else {
        lod_history.clear();
        (
            extract_render_batches_with_instancing(world, None, instancing_enabled),
            None,
        )
    };
    let mut optional = extract_optional_pass_data(world);
    optional.particles = particle_frame;
    optional.physics_debug_frame = physics_runtime_state
        .as_ref()
        .and_then(aether_engine::physics::PhysicsRuntime::debug_frame);
    (batches, optional, visibility)
}
