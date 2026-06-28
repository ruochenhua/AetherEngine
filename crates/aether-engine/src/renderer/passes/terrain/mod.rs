//! Terrain Pass — renders chunked LOD terrain into the GBuffer.
//!
//! The pass is conditionally registered only when the loaded scene contains a
//! `Terrain` configuration. It writes the same GBuffer resources as
//! `GBufferPass` so that deferred lighting and post-processing apply unchanged.

use crate::asset::mesh::{GpuMesh, Vertex};
use crate::ecs::components::Terrain;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::renderable::ViewProjUniform;
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use crate::terrain::Chunk;
use std::sync::Arc;

mod shaders;
mod update;

/// Maximum number of terrain chunks that can be drawn in one frame.
const MAX_TERRAIN_CHUNKS: usize = 1024;

/// Terrain render pass.
pub struct TerrainPass {
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    view_proj_buffer: wgpu::Buffer,
    view_proj_bind_group: wgpu::BindGroup,
    terrain_buffer: wgpu::Buffer,
    terrain_bind_group: wgpu::BindGroup,
    terrain_bind_group_layout: wgpu::BindGroupLayout,

    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    albedo_handle: Option<ResHandle<GAlbedo>>,
    material_handle: Option<ResHandle<GMaterial>>,
    depth_handle: Option<ResHandle<GDepth>>,

    chunks: Vec<Chunk>,
    chunk_meshes: Vec<Vec<Arc<GpuMesh>>>,
    visible_chunk_indices: Vec<usize>,
    chunk_instance_data: Vec<ChunkInstanceData>,
    instance_buffer: wgpu::Buffer,

    last_terrain: Option<Terrain>,
    has_terrain: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct TerrainUniform {
    layer_color_0: [f32; 4],
    layer_color_1: [f32; 4],
    layer_color_2: [f32; 4],
    layer_color_3: [f32; 4],
    layer_roughness: [f32; 4],
    layer_metallic: [f32; 4],
    has_splat_map: u32,
    _pad0: u32,
    splat_uv_scale: f32,
    albedo_uv_scale: f32,
    layer_uv_scale: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ChunkInstanceData {
    model_matrix: [[f32; 4]; 4],
    lod: u32,
    _pad: [u32; 3],
}

impl Pass for TerrainPass {
    fn name(&self) -> &str {
        "Terrain"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Terrain")
            .write::<GPosition>(wgpu::TextureFormat::Rgba16Float)
            .write::<GNormal>(wgpu::TextureFormat::Rgba16Float)
            .write::<GAlbedo>(wgpu::TextureFormat::Rgba8Unorm)
            .write::<GMaterial>(wgpu::TextureFormat::Rg8Unorm)
            .write::<GDepth>(wgpu::TextureFormat::Depth32Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device, ctx.queue)
    }

    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.pos_handle = Some(resources.handle::<GPosition>());
        self.normal_handle = Some(resources.handle::<GNormal>());
        self.albedo_handle = Some(resources.handle::<GAlbedo>());
        self.material_handle = Some(resources.handle::<GMaterial>());
        self.depth_handle = Some(resources.handle::<GDepth>());
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_terrain
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        tracing::debug!(
            target: "aether_engine::renderer::passes::terrain",
            "TerrainPass::apply_frame called, optional.terrain.is_some()={}",
            frame.optional.terrain.is_some()
        );
        if let Some(terrain) = frame.optional.terrain.clone() {
            self.has_terrain = true;
            self.update_terrain(
                terrain,
                &frame.camera.view_matrix(),
                &frame.camera.projection_matrix(frame.aspect),
                frame.queue,
                frame.texture_cache,
                frame.asset_manager,
            );
        } else {
            self.has_terrain = false;
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        tracing::debug!(
            target: "aether_engine::renderer::passes::terrain",
            "TerrainPass::execute called, has_terrain={}, visible_chunks={}",
            self.has_terrain,
            self.visible_chunk_indices.len()
        );
        if !self.has_terrain || self.visible_chunk_indices.is_empty() {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Terrain"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: resources.get(self.pos_handle.unwrap()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: resources.get(self.normal_handle.unwrap()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: resources.get(self.albedo_handle.unwrap()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: resources.get(self.material_handle.unwrap()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: resources.get(self.depth_handle.unwrap()),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.view_proj_bind_group, &[]);
        pass.set_bind_group(1, &self.terrain_bind_group, &[]);
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));

        for (i, chunk_index) in self.visible_chunk_indices.iter().enumerate() {
            let chunk = &self.chunks[*chunk_index];
            let lod_mesh = &self.chunk_meshes[*chunk_index][chunk.lod as usize];
            let instance_start =
                (i * std::mem::size_of::<ChunkInstanceData>()) as wgpu::BufferAddress;
            let instance_end =
                instance_start + std::mem::size_of::<ChunkInstanceData>() as wgpu::BufferAddress;
            pass.set_vertex_buffer(0, lod_mesh.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, self.instance_buffer.slice(instance_start..instance_end));
            if let Some(ref ib) = lod_mesh.index_buffer {
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..lod_mesh.index_count, 0, 0..1);
            } else {
                pass.draw(0..lod_mesh.vertex_count, 0..1);
            }
        }
    }
}

impl TerrainPass {
    /// Create a new terrain pass.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shaders::TERRAIN)),
        });

        let vp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Terrain VP BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let terrain_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terrain PL"),
            bind_group_layouts: &[Some(&vp_bgl), Some(&terrain_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Terrain Pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Vertex::desc(), ChunkInstanceData::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rg8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let view_proj_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain VP Buf"),
            size: std::mem::size_of::<ViewProjUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_proj_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Terrain VP BG"),
            layout: &vp_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_proj_buffer.as_entire_binding(),
            }],
        });

        let terrain_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain Material Buf"),
            size: std::mem::size_of::<TerrainUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Fallback 1x1 white texture until the first real terrain material is resolved.
        let fallback = Arc::new(crate::asset::texture::GpuTexture::from_cpu(
            device,
            queue,
            &crate::asset::texture::CpuTexture::from_color(255, 255, 255, 255),
            Some("terrain_fallback_white"),
        ));
        let terrain_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Terrain Material BG"),
            layout: &terrain_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: terrain_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&fallback.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&fallback.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&fallback.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&fallback.view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&fallback.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&fallback.view),
                },
            ],
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain Instance Buf"),
            size: (MAX_TERRAIN_CHUNKS * std::mem::size_of::<ChunkInstanceData>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device: device.clone(),
            pipeline,
            view_proj_buffer,
            view_proj_bind_group,
            terrain_buffer,
            terrain_bind_group,
            terrain_bind_group_layout: terrain_bgl,
            pos_handle: None,
            normal_handle: None,
            albedo_handle: None,
            material_handle: None,
            depth_handle: None,
            chunks: Vec::new(),
            chunk_meshes: Vec::new(),
            visible_chunk_indices: Vec::new(),
            chunk_instance_data: Vec::new(),
            instance_buffer,
            last_terrain: None,
            has_terrain: false,
        }
    }
}

impl ChunkInstanceData {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ChunkInstanceData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests;
