//! Helper functions for pipeline and screenshot operations.

use aether_engine::asset::texture_cache::GpuTextureCache;
use aether_engine::renderer::{
    ibl::IblResources,
    pass::{InitContext, Pass},
    passes::{
        ao_blur::AOBlurPass, atmosphere::AtmospherePass, bloom::BloomPass,
        composite::CompositePass, debug::DebugLinePass, fxaa::FXAAPass, gbuffer::GBufferPass,
        god_ray::GodRayPass, lighting::LightingPass, shadow::ShadowPass, ssao::SSAOPass,
        ssr::SSRPass, terrain::TerrainPass, tone_mapping::ToneMappingPass,
        volumetric_cloud::VolumetricCloudPass, water::WaterPass,
        water_reflection::WaterReflectionPass,
    },
    pipeline_builder::{PipelineBuildError, PipelineBuilder},
    scheduler::Scheduler,
};
use std::sync::Arc;

/// Build the full render pipeline (IBL + all passes + scheduler).
///
/// `has_terrain` controls whether `TerrainPass` is registered; per ADR-0010 it
/// should only be present when the scene actually contains terrain.
#[allow(clippy::too_many_arguments)]
pub fn build_pipeline(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_cache: &GpuTextureCache,
    surface_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    has_terrain: bool,
) -> Result<(Scheduler, IblResources), PipelineBuildError> {
    let ibl_resources = IblResources::generate(
        device,
        Some(queue),
        &aether_engine::renderer::ibl::IblConfig {
            environment_path: Some("assets/hdr/newport_loft.hdr".into()),
            ..Default::default()
        },
    );

    let ctx = InitContext {
        device,
        queue,
        surface_format,
        depth_format,
        width,
        height,
        ibl_resources: Some(&ibl_resources),
        texture_cache,
    };

    let mut ssao = SSAOPass::init(&ctx);
    ssao.set_screen_size(width, height);
    let mut ao_blur = AOBlurPass::init(&ctx);
    ao_blur.set_screen_size(width, height);
    // SSR traces at half resolution derived from its screen size; without this
    // it would allocate its trace target at the hardcoded 640x360 default.
    let mut ssr = SSRPass::init(&ctx);
    ssr.set_screen_size(width, height);

    let mut builder = PipelineBuilder::new()
        .add_pass(ShadowPass::init(&ctx))
        .add_pass(GBufferPass::init(&ctx));
    if has_terrain {
        builder = builder.add_pass(TerrainPass::init(&ctx));
    }
    let scheduler = builder
        .add_pass(ssao)
        .add_pass(ao_blur)
        .add_pass(LightingPass::init(&ctx))
        .add_pass(AtmospherePass::init(&ctx))
        .add_pass(VolumetricCloudPass::init(&ctx))
        .add_pass(ssr)
        .add_pass(GodRayPass::init(&ctx))
        .add_pass(WaterReflectionPass::init(&ctx))
        .add_pass(WaterPass::init(&ctx))
        .add_pass(CompositePass::init(&ctx))
        .add_pass(BloomPass::init(&ctx))
        .add_pass(ToneMappingPass::init(&ctx))
        .add_pass(FXAAPass::init(&ctx))
        .add_pass(DebugLinePass::init(&ctx))
        .build(device, width, height)?;

    Ok((scheduler, ibl_resources))
}

/// Spawn a default cube into the world for pickable content.
pub fn spawn_default_cube(
    device: &wgpu::Device,
    mesh_registry: &aether_engine::asset::registry::BuiltinMeshRegistry,
    world: &mut aether_engine::ecs::World,
) {
    use aether_engine::ecs::components::{
        MeshHandle, MeshSource, Name, Selected, Transform, Visibility,
    };
    use aether_engine::renderer::renderable::MaterialUniform;
    if let Some(cpu_mesh) = mesh_registry.get("cube") {
        let gpu_mesh = Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(
            device, &cpu_mesh,
        ));
        world.spawn((
            Transform::default(),
            MeshHandle::new(gpu_mesh, MeshSource::Builtin("cube".into()), "cube"),
            MaterialUniform {
                albedo: [0.8, 0.3, 0.2, 1.0],
                roughness: 0.5,
                metallic: 0.0,
                _pad: [0.0, 0.0],
                albedo_texture_id: 0,
            },
            Visibility::default(),
            Name("DefaultCube".into()),
            Selected,
        ));
    }
}

/// Compute required buffer size for a screenshot.
pub fn screenshot_buffer_size(width: u32, height: u32) -> (u64, u32) {
    let bytes_per_row = (width * 4).div_ceil(256) * 256;
    (bytes_per_row as u64 * height as u64, bytes_per_row)
}
