//! Cascaded Shadow Map (CSM) pass.
//!
//! Renders scene depth from the directional light's perspective into a
//! layered depth texture. Each layer corresponds to one cascade covering
//! a subset of the camera frustum.
//!
//! ## Known Pitfalls
//! - **Depth-only rendering**: The fragment filter must not write color.
//! - **Per-object draw order**: Do NOT use `queue.write_buffer` inside the
//!   render pass to update per-object uniforms; Metal may serve stale data.
//!   Pre-upload all instance data to a dynamic uniform buffer before the
//!   render pass, then switch via `set_bind_group(offset)` + draw per batch.

use crate::asset::mesh::{InstanceData, Vertex};
use crate::asset::shader::ShaderLibrary;
use crate::renderer::extract::{opaque_batches, RenderBatch};
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use crate::terrain::{ChunkInstanceData, TerrainGeometry};
use glam::Mat4;
use std::sync::{Arc, RwLock};
mod cascades;
#[cfg(test)]
mod math_tests;
mod shaders;
mod skinned;
pub use cascades::compute_cascades;
#[cfg(test)]
use cascades::split_distances;
#[cfg(test)]
mod tests;

/// Number of cascades.
pub const CASCADE_COUNT: usize = 4;
/// Fixed resolution for each cascade depth map.
pub const SHADOW_MAP_SIZE: u32 = 4096;
/// Per-cascade light-space matrix.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CascadeUniform {
    /// Combined light view-projection matrix for this cascade.
    pub light_view_proj: [[f32; 4]; 4],
}
/// CPU-side cascade data.
#[derive(Clone, Copy, Debug)]
pub struct Cascade {
    /// Light view-projection matrix.
    pub view_proj: Mat4,
    /// Far split distance in view space.
    pub split_depth: f32,
}
/// Cascaded shadow map pass.
pub struct ShadowPass {
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    skinned_pipeline: wgpu::RenderPipeline,
    cascade_uniform_buffer: wgpu::Buffer,
    cascade_bind_group: wgpu::BindGroup,
    /// Per-instance transform vertex buffer.
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    shadow_depth_handle: Option<ResHandle<ShadowDepth>>,
    cascade_views: Vec<wgpu::TextureView>,
    batches: Arc<[RenderBatch]>,
    cascades: [Cascade; CASCADE_COUNT],
    terrain_geometry: Option<Arc<RwLock<TerrainGeometry>>>,
}
impl Pass for ShadowPass {
    fn name(&self) -> &str {
        "Shadow"
    }
    fn signature(&self) -> PassSignature {
        PassSignature::new("Shadow").write_array::<ShadowDepth>(
            wgpu::TextureFormat::Depth32Float,
            SHADOW_MAP_SIZE,
            SHADOW_MAP_SIZE,
            CASCADE_COUNT as u32,
        )
    }
    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device)
    }
    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.shadow_depth_handle = Some(resources.handle::<ShadowDepth>());
        let texture = resources
            .texture(self.shadow_depth_handle.unwrap())
            .expect("ShadowDepth must own its texture");
        self.cascade_views.clear();
        for i in 0..CASCADE_COUNT {
            self.cascade_views
                .push(texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some(&format!("Shadow Cascade {}", i)),
                    format: Some(wgpu::TextureFormat::Depth32Float),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: Some(1),
                    base_array_layer: i as u32,
                    array_layer_count: Some(1),
                    ..Default::default()
                }));
        }
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.batches = opaque_batches(&frame.batches);
        self.terrain_geometry = frame.terrain_geometry.clone();
        let light_dir = glam::Vec3::from_array(frame.lighting.light.direction).normalize();

        self.cascades = compute_cascades(frame, &light_dir);

        let aligned = Self::aligned_cascade_uniform_size();
        for (i, cascade) in self.cascades.iter().enumerate() {
            let uniform = CascadeUniform {
                light_view_proj: cascade.view_proj.to_cols_array_2d(),
            };
            frame.queue.write_buffer(
                &self.cascade_uniform_buffer,
                (i * aligned) as u64,
                bytemuck::cast_slice(&[uniform]),
            );
        }

        // Upload instance data
        let total_instances: usize = self.batches.iter().map(|b| b.instances.len()).sum();
        if total_instances > self.instance_buffer_capacity {
            let new_capacity = total_instances.max(256);
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Shadow Instance Buf"),
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
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        _resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let aligned_size = Self::aligned_cascade_uniform_size() as u32;
        for (i, view) in self.cascade_views.iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(&format!("Shadow Cascade {}", i)),
                color_attachments: &[],
                multiview_mask: None,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.cascade_bind_group, &[(i as u32 * aligned_size)]);

            let mut instance_offset = 0usize;
            for batch in self.batches.iter() {
                let instance_count = batch.instances.len() as u32;
                if instance_count == 0 {
                    continue;
                }
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
                if let Some(skinned) = &batch.skinned {
                    pass.set_pipeline(&self.skinned_pipeline);
                    pass.set_bind_group(1, skinned.bind_group.as_ref(), &[]);
                    pass.set_vertex_buffer(0, skinned.mesh.vertex_buffer.slice(..));
                    if let Some(index_buffer) = &skinned.mesh.index_buffer {
                        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        crate::counted_draw!(
                            pass,
                            draw_indexed,
                            0..skinned.mesh.draw_count,
                            0,
                            0..instance_count
                        );
                    } else {
                        crate::counted_draw!(
                            pass,
                            draw,
                            0..skinned.mesh.draw_count,
                            0..instance_count
                        );
                    }
                } else {
                    pass.set_pipeline(&self.pipeline);
                    if batch.mesh.vertex_count != 0 {
                        pass.set_vertex_buffer(0, batch.mesh.vertex_buffer.slice(..));
                        if let Some(ref ib) = batch.mesh.index_buffer {
                            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                            let start = batch.mesh.index_offset;
                            let end = start + batch.mesh.index_count;
                            crate::counted_draw!(
                                pass,
                                draw_indexed,
                                start..end,
                                0,
                                0..instance_count
                            );
                        } else {
                            crate::counted_draw!(
                                pass,
                                draw,
                                0..batch.mesh.vertex_count,
                                0..instance_count
                            );
                        }
                    }
                }
                instance_offset += batch.instances.len();
            }

            // Render terrain chunks into the shadow map so terrain casts shadows.
            if let Some(terrain_geometry) = &self.terrain_geometry {
                pass.set_pipeline(&self.pipeline);
                let Ok(terrain) = terrain_geometry.read() else {
                    continue;
                };
                let chunks = terrain.chunks();
                let chunk_meshes = terrain.chunk_meshes();
                let instance_buffer = terrain.instance_buffer();
                pass.set_vertex_buffer(1, instance_buffer.slice(..));
                for (chunk_index, chunk) in chunks.iter().enumerate() {
                    let lod_mesh = &chunk_meshes[chunk_index][chunk.lod as usize];
                    let instance_start = (chunk_index * std::mem::size_of::<ChunkInstanceData>())
                        as wgpu::BufferAddress;
                    let instance_end = instance_start
                        + std::mem::size_of::<ChunkInstanceData>() as wgpu::BufferAddress;
                    pass.set_vertex_buffer(0, lod_mesh.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, instance_buffer.slice(instance_start..instance_end));
                    if let Some(ref ib) = lod_mesh.index_buffer {
                        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                        crate::counted_draw!(pass, draw_indexed, 0..lod_mesh.index_count, 0, 0..1);
                    } else {
                        crate::counted_draw!(pass, draw, 0..lod_mesh.vertex_count, 0..1);
                    }
                }
            }
        }
    }
}

impl ShadowPass {
    /// Create a new cascaded shadow pass.
    pub fn new(device: &wgpu::Device) -> Self {
        let src = shaders::SHADOW_SHADER_SRC;
        let shader = ShaderLibrary::create_shader_module(device, "Shadow Shader", src)
            .expect("Shadow shader must be valid WGSL");

        let vp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("S VP BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow PL"),
            bind_group_layouts: &[Some(&vp_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow Pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Vertex::desc(), InstanceData::instance_desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[],
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
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let skinned_pipeline = skinned::create_pipeline(device, &shader, &vp_bgl);

        let aligned_size = Self::aligned_cascade_uniform_size();
        let vp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("S VP"),
            size: (CASCADE_COUNT as u64) * aligned_size as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vp_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("S VP BG"),
            layout: &vp_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &vp_buf,
                    offset: 0,
                    size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<CascadeUniform>() as u64)
                            .unwrap(),
                    ),
                }),
            }],
        });

        let initial_instance_capacity = 256;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow Instance Buf"),
            size: (initial_instance_capacity * std::mem::size_of::<InstanceData>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device: device.clone(),
            pipeline,
            skinned_pipeline,
            cascade_uniform_buffer: vp_buf,
            cascade_bind_group: vp_bg,
            instance_buffer,
            instance_buffer_capacity: initial_instance_capacity,
            shadow_depth_handle: None,
            cascade_views: Vec::new(),
            batches: Arc::from([]),
            cascades: [Cascade {
                view_proj: Mat4::IDENTITY,
                split_depth: 0.0,
            }; CASCADE_COUNT],
            terrain_geometry: None,
        }
    }

    /// Return the CPU-side cascade data computed for the current frame.
    pub fn cascades(&self) -> &[Cascade; CASCADE_COUNT] {
        &self.cascades
    }

    fn aligned_cascade_uniform_size() -> usize {
        let size = std::mem::size_of::<CascadeUniform>();
        let alignment = 256; // wgpu uniform dynamic offset alignment
        size.div_ceil(alignment) * alignment
    }
}
