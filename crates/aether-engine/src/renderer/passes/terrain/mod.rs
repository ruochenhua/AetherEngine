//! Terrain Pass — renders chunked LOD terrain into the GBuffer.
//!
//! The pass is conditionally registered only when the loaded scene contains a
//! `Terrain` configuration. It writes the same GBuffer resources as
//! `GBufferPass` so that deferred lighting and post-processing apply unchanged.
//!
//! The actual chunk geometry is owned by the shared [`TerrainGeometry`] cache and
//! injected through [`RenderFrame::terrain_geometry`]; this pass only maintains
//! the terrain-specific material bindings.

use crate::asset::texture::GpuTexture;
use crate::asset::texture_cache::GpuTextureCache;
use crate::asset::AssetManager;
use crate::ecs::components::Terrain;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::renderable::ViewProjUniform;
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use crate::terrain::{
    create_terrain_material_bind_group, create_terrain_material_bind_group_layout,
    write_terrain_uniforms, ChunkInstanceData, TerrainGeometry, TerrainUniform,
};
use std::sync::{Arc, RwLock};

mod shaders;

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

    terrain_geometry: Option<Arc<RwLock<TerrainGeometry>>>,
    has_terrain: bool,

    /// Cached terrain material bind group state.
    last_splat: Option<Arc<GpuTexture>>,
    last_layer0: Option<Arc<GpuTexture>>,
    last_layer1: Option<Arc<GpuTexture>>,
    last_layer2: Option<Arc<GpuTexture>>,
    last_layer3: Option<Arc<GpuTexture>>,
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
            self.terrain_geometry = frame.terrain_geometry.clone();
            self.update_material(
                &terrain,
                &frame.camera.view_matrix(),
                &frame.camera.projection_matrix(frame.aspect),
                frame.queue,
                frame.texture_cache,
                frame.asset_manager,
            );
        } else {
            self.has_terrain = false;
            self.terrain_geometry = None;
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let terrain_geometry_guard = match &self.terrain_geometry {
            Some(g) => g.read().unwrap(),
            None => return,
        };
        let terrain_geometry = &*terrain_geometry_guard;
        let visible = terrain_geometry.visible_chunk_indices();
        tracing::debug!(
            target: "aether_engine::renderer::passes::terrain",
            "TerrainPass::execute called, has_terrain={}, visible_chunks={}",
            self.has_terrain,
            visible.len()
        );
        if !self.has_terrain || visible.is_empty() {
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
        pass.set_vertex_buffer(1, terrain_geometry.instance_buffer().slice(..));

        for chunk_index in visible.iter() {
            let chunk = &terrain_geometry.chunks()[*chunk_index];
            let lod_mesh = &terrain_geometry.chunk_meshes()[*chunk_index][chunk.lod as usize];
            let instance_start =
                (*chunk_index * std::mem::size_of::<ChunkInstanceData>()) as wgpu::BufferAddress;
            let instance_end =
                instance_start + std::mem::size_of::<ChunkInstanceData>() as wgpu::BufferAddress;
            pass.set_vertex_buffer(0, lod_mesh.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, terrain_geometry.instance_buffer().slice(instance_start..instance_end));
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
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                crate::renderer::passes::terrain::shaders::TERRAIN,
            )),
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

        let terrain_bgl = create_terrain_material_bind_group_layout(device);

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
                buffers: &[
                    crate::asset::mesh::Vertex::desc(),
                    ChunkInstanceData::desc(),
                ],
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
        let terrain_bind_group = create_terrain_material_bind_group(
            device,
            &terrain_bgl,
            &terrain_buffer,
            &fallback,
            &fallback,
            &fallback,
            &fallback,
            &fallback,
        );

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
            terrain_geometry: None,
            has_terrain: false,
            last_splat: None,
            last_layer0: None,
            last_layer1: None,
            last_layer2: None,
            last_layer3: None,
        }
    }

    fn update_material(
        &mut self,
        terrain: &Terrain,
        view: &glam::Mat4,
        proj: &glam::Mat4,
        queue: &wgpu::Queue,
        texture_cache: &GpuTextureCache,
        asset_manager: &AssetManager,
    ) {
        use crate::renderer::renderable::ViewProjUniform;

        let vp = ViewProjUniform {
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
        };
        queue.write_buffer(&self.view_proj_buffer, 0, bytemuck::cast_slice(&[vp]));

        write_terrain_uniforms(
            &self.terrain_buffer,
            &terrain.material,
            terrain.splatmap_path.is_some(),
            terrain.geometry.extent,
            terrain.geometry.albedo_tiling,
            queue,
        );

        let splat = texture_cache.get_or_upload_optional(
            terrain.material.splat_map.clone(),
            asset_manager,
        );
        let layer0 = texture_cache.get_or_upload_optional(
            terrain.material.layers[0].albedo_texture.clone(),
            asset_manager,
        );
        let layer1 = texture_cache.get_or_upload_optional(
            terrain.material.layers[1].albedo_texture.clone(),
            asset_manager,
        );
        let layer2 = texture_cache.get_or_upload_optional(
            terrain.material.layers[2].albedo_texture.clone(),
            asset_manager,
        );
        let layer3 = texture_cache.get_or_upload_optional(
            terrain.material.layers[3].albedo_texture.clone(),
            asset_manager,
        );

        let needs_rebuild = match (
            &self.last_splat,
            &self.last_layer0,
            &self.last_layer1,
            &self.last_layer2,
            &self.last_layer3,
        ) {
            (Some(last_splat), Some(last_l0), Some(last_l1), Some(last_l2), Some(last_l3)) => {
                !Arc::ptr_eq(last_splat, &splat)
                    || !Arc::ptr_eq(last_l0, &layer0)
                    || !Arc::ptr_eq(last_l1, &layer1)
                    || !Arc::ptr_eq(last_l2, &layer2)
                    || !Arc::ptr_eq(last_l3, &layer3)
            }
            _ => true,
        };

        if needs_rebuild {
            self.terrain_bind_group = create_terrain_material_bind_group(
                &self.device,
                &self.terrain_bind_group_layout,
                &self.terrain_buffer,
                &splat,
                &layer0,
                &layer1,
                &layer2,
                &layer3,
            );
            self.last_splat = Some(splat);
            self.last_layer0 = Some(layer0);
            self.last_layer1 = Some(layer1);
            self.last_layer2 = Some(layer2);
            self.last_layer3 = Some(layer3);
        }
    }
}

#[cfg(test)]
mod tests;
