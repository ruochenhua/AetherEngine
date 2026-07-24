//! Shared terrain material helpers.
//!
//! Used by `TerrainPass` and `WaterReflectionPass` to build the same splatting
//! uniform buffer, bind group layout, and bind group.

use crate::asset::terrain_material::TerrainMaterial;
use crate::asset::texture::GpuTexture;
use std::sync::Arc;

/// GPU uniform buffer layout for terrain splatting material parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TerrainUniform {
    /// Albedo color for layer 0.
    pub layer_color_0: [f32; 4],
    /// Albedo color for layer 1.
    pub layer_color_1: [f32; 4],
    /// Albedo color for layer 2.
    pub layer_color_2: [f32; 4],
    /// Albedo color for layer 3.
    pub layer_color_3: [f32; 4],
    /// Per-layer roughness values.
    pub layer_roughness: [f32; 4],
    /// Per-layer metallic values.
    pub layer_metallic: [f32; 4],
    /// Non-zero when a splat map is bound.
    pub has_splat_map: u32,
    /// Padding to align the following floats.
    pub _pad0: u32,
    /// UV scale for the splat map.
    pub splat_uv_scale: f32,
    /// Global UV scale for albedo textures.
    pub albedo_uv_scale: f32,
    /// Per-layer UV scales.
    pub layer_uv_scale: [f32; 4],
}

/// Build the bind group layout shared by terrain material-aware passes.
pub fn create_terrain_material_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Terrain Material BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        })
}

/// Create a bind group for the terrain material using the provided textures.
#[allow(clippy::too_many_arguments)]
pub fn create_terrain_material_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    splat: &Arc<GpuTexture>,
    layer0: &Arc<GpuTexture>,
    layer1: &Arc<GpuTexture>,
    layer2: &Arc<GpuTexture>,
    layer3: &Arc<GpuTexture>,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Terrain Material BG"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&splat.view,
                ),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&splat.sampler,
                ),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&layer0.view,
                ),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&layer1.view,
                ),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&layer2.view,
                ),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(&layer3.view,
                ),
            },
        ],
    })
}

/// Upload terrain material parameters into `buffer`.
pub fn write_terrain_uniforms(
    buffer: &wgpu::Buffer,
    material: &TerrainMaterial,
    has_splat_map: bool,
    extent: f32,
    albedo_tiling: f32,
    queue: &wgpu::Queue,
) {
    let uniform = terrain_uniform_from_material(material, has_splat_map, extent, albedo_tiling);
    queue.write_buffer(buffer, 0, bytemuck::cast_slice(&[uniform]));
}

fn terrain_uniform_from_material(
    material: &TerrainMaterial,
    has_splat_map: bool,
    extent: f32,
    albedo_tiling: f32,
) -> TerrainUniform {
    let mut colors = [[0.5, 0.5, 0.5, 1.0]; 4];
    let mut roughness = [0.8; 4];
    let mut metallic = [0.0; 4];
    for (i, layer) in material.layers.iter().enumerate() {
        colors[i] = layer.albedo;
        roughness[i] = layer.roughness;
        metallic[i] = layer.metallic;
    }

    // Map world-space XZ positions to splat/albedo UVs.
    let full_extent = (extent * 2.0).max(0.001);
    let splat_uv_scale = 1.0 / full_extent;
    let albedo_uv_scale = albedo_tiling.max(0.001) / full_extent;

    let mut layer_uv_scale = [1.0f32; 4];
    for (i, layer) in material.layers.iter().enumerate() {
        layer_uv_scale[i] = layer.uv_scale;
    }

    TerrainUniform {
        layer_color_0: colors[0],
        layer_color_1: colors[1],
        layer_color_2: colors[2],
        layer_color_3: colors[3],
        layer_roughness: roughness,
        layer_metallic: metallic,
        has_splat_map: if has_splat_map { 1 } else { 0 },
        _pad0: 0,
        splat_uv_scale,
        albedo_uv_scale,
        layer_uv_scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::headless_device_queue;
    use std::mem::{offset_of, size_of};

    #[test]
    fn terrain_uniform_matches_gpu_layout() {
        // The WGSL `TerrainUniform` struct is mirrored verbatim in both
        // `passes/terrain/shaders.rs` and `passes/water_reflection.rs` as a
        // 128-byte block; layout drift here silently corrupts both passes.
        assert_eq!(
            size_of::<TerrainUniform>(),
            128,
            "TerrainUniform size drifted from the 128-byte WGSL layout"
        );
        // Note: the Rust struct itself is only 4-byte aligned ([f32; 4]
        // fields); what matters for the GPU is the byte layout below, which
        // `queue.write_buffer` copies verbatim into the uniform buffer.
        // Field offsets must match the WGSL member order and alignment.
        assert_eq!(offset_of!(TerrainUniform, layer_color_0), 0);
        assert_eq!(offset_of!(TerrainUniform, layer_color_1), 16);
        assert_eq!(offset_of!(TerrainUniform, layer_color_2), 32);
        assert_eq!(offset_of!(TerrainUniform, layer_color_3), 48);
        assert_eq!(offset_of!(TerrainUniform, layer_roughness), 64);
        assert_eq!(offset_of!(TerrainUniform, layer_metallic), 80);
        assert_eq!(offset_of!(TerrainUniform, has_splat_map), 96);
        assert_eq!(offset_of!(TerrainUniform, splat_uv_scale), 104);
        assert_eq!(offset_of!(TerrainUniform, albedo_uv_scale), 108);
        assert_eq!(offset_of!(TerrainUniform, layer_uv_scale), 112);
    }

    #[test]
    fn write_terrain_uniforms_uploads_expected_fields() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };

        let uniform_size = size_of::<TerrainUniform>() as u64;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain Uniform Test Buf"),
            size: uniform_size,
            usage: wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let mut material = TerrainMaterial::default();
        material.layers[0].albedo = [1.0, 0.0, 0.0, 1.0];
        material.layers[1].albedo = [0.0, 1.0, 0.0, 1.0];
        material.layers[0].roughness = 0.25;
        material.layers[1].roughness = 0.75;
        material.layers[2].metallic = 0.5;
        material.layers[0].uv_scale = 2.0;
        material.layers[3].uv_scale = 8.0;

        // full_extent = 128 * 2 = 256, so splat scale = 1/256, albedo scale = 64/256.
        write_terrain_uniforms(&buffer, &material, true, 128.0, 64.0, &queue);

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain Uniform Readback"),
            size: uniform_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Terrain Uniform Copy"),
        });
        encoder.copy_buffer_to_buffer(&buffer, 0, &staging, 0, uniform_size);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| {
            result.expect("failed to map terrain uniform readback buffer");
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        let data = slice.get_mapped_range();
        let uniforms: &[TerrainUniform] = bytemuck::cast_slice(&data);
        let u = &uniforms[0];

        assert_eq!(
            u.layer_color_0,
            [1.0, 0.0, 0.0, 1.0],
            "layer 0 albedo must come from the material"
        );
        assert_eq!(
            u.layer_color_1,
            [0.0, 1.0, 0.0, 1.0],
            "layer 1 albedo must come from the material"
        );
        assert_eq!(
            u.layer_color_2,
            [0.5, 0.5, 0.5, 1.0],
            "unmodified layers keep the default grey albedo"
        );
        assert_eq!(
            u.layer_roughness,
            [0.25, 0.75, 0.8, 0.8],
            "per-layer roughness must match the material (default 0.8)"
        );
        assert_eq!(
            u.layer_metallic,
            [0.0, 0.0, 0.5, 0.0],
            "per-layer metallic must match the material (default 0.0)"
        );
        assert_eq!(u.has_splat_map, 1, "has_splat_map=true must upload as 1");
        // 1/256 and 64/256 are exactly representable in f32.
        assert_eq!(
            u.splat_uv_scale,
            1.0 / 256.0,
            "splat_uv_scale must be 1 / (extent * 2)"
        );
        assert_eq!(
            u.albedo_uv_scale,
            64.0 / 256.0,
            "albedo_uv_scale must be albedo_tiling / (extent * 2)"
        );
        assert_eq!(
            u.layer_uv_scale,
            [2.0, 1.0, 1.0, 8.0],
            "per-layer uv_scale must match the material (default 1.0)"
        );
    }
}
