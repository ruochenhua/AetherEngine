//! G-Buffer Pass
//!
//! Writes position, normal, albedo, material, and emissive data to five MRT targets.
//! Uses GPU instancing with pre-uploaded instance data.
//!
//! ## Known Pitfalls
//! - **Per-object draw order**: All per-object uniform data must be pre-uploaded
//!   before the render pass begins. `queue.write_buffer` inside the render pass
//!   is unreliable on Metal — use dynamic uniform offsets to switch between
//!   pre-uploaded data per draw batch.

use crate::asset::mesh::InstanceData;
use crate::renderer::extract::RenderBatch;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::renderable::*;
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use glam::Mat4;
use std::sync::Arc;

mod shaders;
#[cfg(test)]
mod tests;

/// G-Buffer Pass — renders world-space position, normal, albedo, and material
/// properties into a multi-render-target (MRT) framebuffer for deferred shading.
pub struct GBufferPass {
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    view_proj_buffer: wgpu::Buffer,
    view_proj_bind_group: wgpu::BindGroup,
    /// Per-batch material uniform buffer (dynamic).
    object_buffer: wgpu::Buffer,
    object_buffer_capacity: usize,
    object_bind_group: wgpu::BindGroup,
    object_bind_group_layout: wgpu::BindGroupLayout,
    /// Per-batch material texture bind group layout and bind groups.
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_groups: Vec<wgpu::BindGroup>,
    fallback_white: Arc<crate::asset::texture::GpuTexture>,
    fallback_normal: Arc<crate::asset::texture::GpuTexture>,
    /// Per-instance transform + entity_id vertex buffer.
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,

    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    albedo_handle: Option<ResHandle<GAlbedo>>,
    material_handle: Option<ResHandle<GMaterial>>,
    emissive_handle: Option<ResHandle<GEmissive>>,
    depth_handle: Option<ResHandle<GDepth>>,

    batches: Arc<[RenderBatch]>,
    view: Mat4,
    proj: Mat4,
}

impl Pass for GBufferPass {
    fn name(&self) -> &str {
        "GBuffer"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("GBuffer")
            .write::<GPosition>(wgpu::TextureFormat::Rgba16Float)
            .write::<GNormal>(wgpu::TextureFormat::Rgba16Float)
            .write::<GAlbedo>(wgpu::TextureFormat::Rgba8Unorm)
            .write::<GMaterial>(wgpu::TextureFormat::Rg8Unorm)
            .write::<GEmissive>(wgpu::TextureFormat::Rgba8Uint)
            .write::<GDepth>(wgpu::TextureFormat::Depth32Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device, ctx.queue, ctx.texture_cache)
    }

    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.pos_handle = Some(resources.handle::<GPosition>());
        self.normal_handle = Some(resources.handle::<GNormal>());
        self.albedo_handle = Some(resources.handle::<GAlbedo>());
        self.material_handle = Some(resources.handle::<GMaterial>());
        self.emissive_handle = Some(resources.handle::<GEmissive>());
        self.depth_handle = Some(resources.handle::<GDepth>());
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.batches = frame.batches.clone();
        self.view = frame.camera.view_matrix();
        self.proj = frame.camera.projection_matrix(frame.aspect);

        // Upload view/proj
        let vp = ViewProjUniform {
            view: self.view.to_cols_array_2d(),
            proj: self.proj.to_cols_array_2d(),
        };
        frame
            .queue
            .write_buffer(&self.view_proj_buffer, 0, bytemuck::cast_slice(&[vp]));

        // Upload per-batch material data (one ObjectUniform per batch)
        let obj_size = std::mem::size_of::<ObjectUniform>() as wgpu::BufferAddress;
        let batch_count = self.batches.len();
        if batch_count > self.object_buffer_capacity {
            let new_capacity = batch_count.max(256);
            self.object_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GBuffer Obj Buf"),
                size: (new_capacity as u64) * obj_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.object_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("GBuffer Obj BG"),
                layout: &self.object_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.object_buffer,
                        offset: 0,
                        size: Some(std::num::NonZeroU64::new(obj_size).unwrap()),
                    }),
                }],
            });
            self.object_buffer_capacity = new_capacity;
        }
        let mut obj_data: Vec<u8> = Vec::with_capacity(batch_count * obj_size as usize);
        for batch in self.batches.iter() {
            let obj = ObjectUniform::from_material(&batch.material);
            obj_data.extend_from_slice(bytemuck::cast_slice(&[obj]));
        }
        if !obj_data.is_empty() {
            frame.queue.write_buffer(&self.object_buffer, 0, &obj_data);
        }

        // Upload instance data
        let total_instances: usize = self.batches.iter().map(|b| b.instances.len()).sum();
        if total_instances > self.instance_buffer_capacity {
            let new_capacity = total_instances.max(256);
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GBuffer Instance Buf"),
                size: (new_capacity * std::mem::size_of::<InstanceData>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_buffer_capacity = new_capacity;
        }
        let mut instance_data: Vec<u8> =
            Vec::with_capacity(total_instances * std::mem::size_of::<InstanceData>());
        for batch in self.batches.iter() {
            instance_data.extend_from_slice(bytemuck::cast_slice(&batch.instances));
        }
        if !instance_data.is_empty() {
            frame
                .queue
                .write_buffer(&self.instance_buffer, 0, &instance_data);
        }

        // Build per-batch albedo texture bind groups.
        self.texture_bind_groups.clear();
        for batch in self.batches.iter() {
            let albedo_tex = match &batch.albedo_texture {
                Some(handle) => frame
                    .texture_cache
                    .get_or_upload(handle.clone(), frame.asset_manager),
                None => self.fallback_white.clone(),
            };
            let normal_tex = match &batch.normal_texture {
                Some(handle) => frame
                    .texture_cache
                    .get_or_upload(handle.clone(), frame.asset_manager),
                None => self.fallback_normal.clone(),
            };
            let orm_tex = match &batch.orm_texture {
                Some(handle) => frame
                    .texture_cache
                    .get_or_upload(handle.clone(), frame.asset_manager),
                None => self.fallback_white.clone(),
            };
            let emissive_tex = match &batch.emissive_texture {
                Some(handle) => frame
                    .texture_cache
                    .get_or_upload(handle.clone(), frame.asset_manager),
                None => self.fallback_white.clone(),
            };
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("GBuffer Texture BG"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&albedo_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&albedo_tex.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&normal_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&normal_tex.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&orm_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Sampler(&orm_tex.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&emissive_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::Sampler(&emissive_tex.sampler),
                    },
                ],
            });
            self.texture_bind_groups.push(bg);
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let (
            Some(pos_view),
            Some(normal_view),
            Some(albedo_view),
            Some(material_view),
            Some(emissive_view),
            Some(depth_view),
        ) = (
            resources.get_if_handle(self.pos_handle),
            resources.get_if_handle(self.normal_handle),
            resources.get_if_handle(self.albedo_handle),
            resources.get_if_handle(self.material_handle),
            resources.get_if_handle(self.emissive_handle),
            resources.get_if_handle(self.depth_handle),
        )
        else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GBuffer"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: pos_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: normal_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: albedo_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: material_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: emissive_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
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

        let obj_size = std::mem::size_of::<ObjectUniform>() as wgpu::BufferAddress;
        let mut instance_offset = 0usize;
        for (batch_index, batch) in self.batches.iter().enumerate() {
            let instance_count = batch.instances.len() as u32;
            if instance_count == 0 || batch.mesh.vertex_count == 0 {
                continue;
            }

            let offset = batch_index as u32 * obj_size as u32;
            pass.set_bind_group(1, &self.object_bind_group, &[offset]);
            pass.set_bind_group(2, &self.texture_bind_groups[batch_index], &[]);

            pass.set_vertex_buffer(0, batch.mesh.vertex_buffer.slice(..));
            let instance_byte_start =
                (instance_offset * std::mem::size_of::<InstanceData>()) as wgpu::BufferAddress;
            let instance_byte_end = instance_byte_start
                + (batch.instances.len() * std::mem::size_of::<InstanceData>())
                    as wgpu::BufferAddress;
            pass.set_vertex_buffer(
                1,
                self.instance_buffer
                    .slice(instance_byte_start..instance_byte_end),
            );
            if let Some(ref ib) = batch.mesh.index_buffer {
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                let start = batch.mesh.index_offset;
                let end = start + batch.mesh.index_count;
                pass.draw_indexed(start..end, 0, 0..instance_count);
            } else {
                pass.draw(0..batch.mesh.vertex_count, 0..instance_count);
            }
            instance_offset += batch.instances.len();
        }
    }
}

mod pipeline;
