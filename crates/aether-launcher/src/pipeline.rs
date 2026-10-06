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
        transparent::TransparentPass, volumetric_cloud::VolumetricCloudPass, water::WaterPass,
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
    transparent_enabled: bool,
    particles_enabled: bool,
    local_lights_enabled: bool,
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
        local_lights_enabled,
    };

    let mut ssao = SSAOPass::init(&ctx);
    ssao.set_screen_size(width, height);
    let mut ao_blur = AOBlurPass::init(&ctx);
    ao_blur.set_screen_size(width, height);

    let mut builder = PipelineBuilder::new()
        .add_pass(ShadowPass::init(&ctx))
        .add_pass(GBufferPass::init(&ctx));
    if has_terrain {
        builder = builder.add_pass(TerrainPass::init(&ctx));
    }
    let mut builder = builder
        .add_pass(ssao)
        .add_pass(ao_blur)
        .add_pass(LightingPass::init(&ctx))
        .add_pass(AtmospherePass::init(&ctx))
        .add_pass(VolumetricCloudPass::init(&ctx))
        .add_pass(SSRPass::init(&ctx))
        .add_pass(GodRayPass::init(&ctx))
        .add_pass(WaterReflectionPass::init(&ctx));
    let transparent_features = transparent_pass_features(transparent_enabled, particles_enabled);
    if transparent_features.register_pass {
        let transparent_pass = TransparentPass::init(&ctx)
            .with_transparent_meshes_enabled(transparent_features.draw_meshes);
        builder = builder.add_pass(transparent_pass);
    }
    let composite = if transparent_features.register_pass {
        CompositePass::new_with_transparency(device, surface_format)
    } else {
        CompositePass::init(&ctx)
    };
    let scheduler = builder
        .add_pass(WaterPass::init(&ctx))
        .add_pass(composite)
        .add_pass(BloomPass::init(&ctx))
        .add_pass(ToneMappingPass::init(&ctx))
        .add_pass(FXAAPass::init(&ctx))
        .add_pass(DebugLinePass::init(&ctx))
        .build(device, width, height)?;

    Ok((scheduler, ibl_resources))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TransparentPassFeatures {
    register_pass: bool,
    draw_meshes: bool,
}

fn transparent_pass_features(
    transparent_enabled: bool,
    particles_enabled: bool,
) -> TransparentPassFeatures {
    TransparentPassFeatures {
        register_pass: transparent_enabled || particles_enabled,
        draw_meshes: transparent_enabled,
    }
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
    use aether_engine::renderer::extracted_material::ExtractedMaterial;
    use aether_engine::renderer::renderable::MaterialUniform;
    if let Some(cpu_mesh) = mesh_registry.get("cube") {
        let gpu_mesh = Arc::new(aether_engine::asset::mesh::GpuMesh::from_cpu(
            device, &cpu_mesh,
        ));
        let material = MaterialUniform {
            albedo: [0.8, 0.3, 0.2, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            unlit: 0,
            _pad: 0,
            albedo_texture_id: 0,
            ..MaterialUniform::default()
        };
        world.spawn((
            Transform::default(),
            MeshHandle::new(gpu_mesh, MeshSource::Builtin("cube".into()), "cube"),
            material,
            ExtractedMaterial::from_legacy_uniform(material),
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

#[cfg(test)]
mod tests {
    use super::{transparent_pass_features, TransparentPassFeatures};

    #[test]
    fn transparent_pass_registration_tracks_mesh_and_particle_flags_independently() {
        assert_eq!(
            transparent_pass_features(false, false),
            TransparentPassFeatures {
                register_pass: false,
                draw_meshes: false,
            }
        );
        assert_eq!(
            transparent_pass_features(true, false),
            TransparentPassFeatures {
                register_pass: true,
                draw_meshes: true,
            }
        );
        assert_eq!(
            transparent_pass_features(false, true),
            TransparentPassFeatures {
                register_pass: true,
                draw_meshes: false,
            }
        );
        assert_eq!(
            transparent_pass_features(true, true),
            TransparentPassFeatures {
                register_pass: true,
                draw_meshes: true,
            }
        );
    }
}
