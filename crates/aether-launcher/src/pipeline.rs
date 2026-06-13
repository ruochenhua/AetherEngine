//! Helper functions for pipeline and screenshot operations.

use aether_engine::renderer::{
    ibl::IblResources,
    passes::{
        ao_blur::AOBlurPass, atmosphere::AtmospherePass, bloom::BloomPass,
        composite::CompositePass, debug::DebugLinePass, fxaa::FXAAPass, gbuffer::GBufferPass,
        god_ray::GodRayPass, lighting::LightingPass, shadow::ShadowPass, ssao::SSAOPass,
        ssr::SSRPass, terrain::TerrainPass, tone_mapping::ToneMappingPass,
        volumetric_cloud::VolumetricCloudPass, water::WaterPass,
    },
    pipeline_builder::PipelineBuilder,
    scheduler::Scheduler,
};
use std::sync::Arc;

/// Build the full render pipeline (IBL + all passes + scheduler).
///
/// `has_terrain` controls whether `TerrainPass` is registered; per ADR-0010 it
/// should only be present when the scene actually contains terrain.
pub fn build_pipeline(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    has_terrain: bool,
) -> (Scheduler, IblResources) {
    let ibl_resources = IblResources::generate(
        device,
        Some(queue),
        &aether_engine::renderer::ibl::IblConfig {
            environment_path: Some("assets/hdr/newport_loft.hdr".into()),
            ..Default::default()
        },
    );

    let mut ssao = SSAOPass::new(device);
    ssao.set_screen_size(width, height);
    let mut ao_blur = AOBlurPass::new(device);
    ao_blur.set_screen_size(width, height);

    let mut builder = PipelineBuilder::new()
        .add_pass(ShadowPass::new(device))
        .add_pass(GBufferPass::new(device));
    if has_terrain {
        builder = builder.add_pass(TerrainPass::new(device));
    }
    let mut scheduler = builder
        .add_pass(ssao)
        .add_pass(ao_blur)
        .add_pass(LightingPass::new_with_ibl(
            device,
            surface_format,
            &ibl_resources,
        ))
        .add_pass(AtmospherePass::new(device))
        .add_pass(VolumetricCloudPass::new(device, queue))
        .add_pass(SSRPass::new(device))
        .add_pass(GodRayPass::new(device))
        .add_pass(WaterPass::new(device))
        .add_pass(CompositePass::new(device, surface_format))
        .add_pass(BloomPass::new(device, width, height))
        .add_pass(ToneMappingPass::new(device, surface_format))
        .add_pass(FXAAPass::new(device, surface_format))
        .add_pass(DebugLinePass::new(device, surface_format, depth_format))
        .build(device, width, height);

    scheduler.set_ssr_screen_size(width, height);
    (scheduler, ibl_resources)
}

/// Spawn a default cube into the world for pickable content.
pub fn spawn_default_cube(
    device: &wgpu::Device,
    mesh_registry: &aether_engine::asset::registry::BuiltinMeshRegistry,
    world: &mut aether_engine::ecs::World,
) {
    use aether_engine::ecs::components::{MeshHandle, Name, Selected, Transform, Visibility};
    use aether_engine::renderer::renderable::MaterialUniform;
    if let Some(cpu_mesh) = mesh_registry.get("cube") {
        let gpu_mesh = Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(
            device, &cpu_mesh,
        ));
        world.spawn((
            Transform::default(),
            MeshHandle::new(gpu_mesh, "cube"),
            MaterialUniform {
                albedo: [0.8, 0.3, 0.2, 1.0],
                roughness: 0.5,
                metallic: 0.0,
                _pad: [0.0, 0.0],
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
