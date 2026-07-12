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
