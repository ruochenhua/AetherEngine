//! G-Buffer Pass
//!
//! Writes position, normal, albedo, and material to four MRT targets.
//! Uses GPU instancing with pre-uploaded instance data.
//!
//! ## Known Pitfalls
//! - **Per-object draw order**: All per-object uniform data must be pre-uploaded
//!   before the render pass begins. `queue.write_buffer` inside the render pass
//!   is unreliable on Metal — use dynamic uniform offsets to switch between
//!   pre-uploaded data per draw batch.

use crate::asset::mesh::{InstanceData, Vertex};
use crate::renderer::extract::RenderBatch;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::renderable::*;
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use glam::Mat4;
use std::sync::Arc;

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
    /// Per-batch albedo texture bind group layout and bind groups.
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_groups: Vec<wgpu::BindGroup>,
    fallback_white: Arc<crate::asset::texture::GpuTexture>,
    /// Per-instance transform + entity_id vertex buffer.
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,

    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    albedo_handle: Option<ResHandle<GAlbedo>>,
    material_handle: Option<ResHandle<GMaterial>>,
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
            let obj = ObjectUniform {
                albedo: batch.material.albedo,
                roughness: batch.material.roughness,
                metallic: batch.material.metallic,
            };
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
            let gpu_tex = match &batch.albedo_texture {
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
                        resource: wgpu::BindingResource::TextureView(&gpu_tex.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&gpu_tex.sampler),
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
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GBuffer"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: resources.get(self.pos_handle.unwrap()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: resources.get(self.normal_handle.unwrap()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: resources.get(self.albedo_handle.unwrap()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: resources.get(self.material_handle.unwrap()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: resources.get(self.depth_handle.unwrap()),
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

impl GBufferPass {
    /// Create a new G-Buffer pass.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _texture_cache: &crate::asset::texture_cache::GpuTextureCache,
    ) -> Self {
        let shader_source = GBUFFER_SHADER;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GBuffer Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let vp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GBuffer VP BGL"),
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

        let obj_size = std::mem::size_of::<ObjectUniform>() as u64;
        let object_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("GBuffer Obj BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(std::num::NonZeroU64::new(obj_size).unwrap()),
                    },
                    count: None,
                }],
            });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("GBuffer Texture BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GBuffer PL"),
            bind_group_layouts: &[
                Some(&vp_bgl),
                Some(&object_bind_group_layout),
                Some(&texture_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GBuffer Pipeline"),
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
            label: Some("GBuffer VP Buf"),
            size: std::mem::size_of::<ViewProjUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_proj_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GBuffer VP BG"),
            layout: &vp_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_proj_buffer.as_entire_binding(),
            }],
        });

        let initial_object_capacity = 256usize;
        let object_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GBuffer Obj Buf"),
            size: (initial_object_capacity as u64) * obj_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let object_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GBuffer Obj BG"),
            layout: &object_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &object_buffer,
                    offset: 0,
                    size: Some(std::num::NonZeroU64::new(obj_size).unwrap()),
                }),
            }],
        });

        let initial_instance_capacity = 256;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GBuffer Instance Buf"),
            size: (initial_instance_capacity * std::mem::size_of::<InstanceData>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let fallback_white = Arc::new(crate::asset::texture::GpuTexture::from_cpu(
            device,
            queue,
            &crate::asset::texture::CpuTexture::from_color(255, 255, 255, 255),
            Some("gbuffer_fallback_white"),
        ));

        Self {
            device: device.clone(),
            pipeline,
            view_proj_buffer,
            view_proj_bind_group,
            object_buffer,
            object_buffer_capacity: initial_object_capacity,
            object_bind_group,
            object_bind_group_layout,
            texture_bind_group_layout,
            texture_bind_groups: Vec::new(),
            fallback_white,
            instance_buffer,
            instance_buffer_capacity: initial_instance_capacity,
            pos_handle: None,
            normal_handle: None,
            albedo_handle: None,
            material_handle: None,
            depth_handle: None,
            batches: Arc::from([]),
            view: Mat4::IDENTITY,
            proj: Mat4::IDENTITY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::headless_device_queue;

    fn init_ctx<'a>(device: &'a wgpu::Device, queue: &'a wgpu::Queue) -> InitContext<'a> {
        let texture_cache = Box::leak(Box::new(crate::asset::texture_cache::GpuTextureCache::new(
            device, queue,
        )));
        InitContext {
            device,
            queue,
            surface_format: wgpu::TextureFormat::Bgra8UnormSrgb,
            depth_format: wgpu::TextureFormat::Depth32Float,
            width: 64,
            height: 64,
            ibl_resources: None,
            texture_cache,
        }
    }

    #[test]
    fn signature_ok() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let ctx = init_ctx(&device, &queue);
        let pass = GBufferPass::init(&ctx);
        let sig = pass.signature();
        assert_eq!(sig.writes.len(), 5);
    }

    #[test]
    fn resolve_ok() {
        let Some((device, queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let ctx = init_ctx(&device, &queue);
        let mut pass = GBufferPass::init(&ctx);
        let mut table = ResourceTable::new();
        for (type_id, name, fmt) in [
            (
                std::any::TypeId::of::<GPosition>(),
                GPosition::NAME,
                wgpu::TextureFormat::Rgba16Float,
            ),
            (
                std::any::TypeId::of::<GNormal>(),
                GNormal::NAME,
                wgpu::TextureFormat::Rgba16Float,
            ),
            (
                std::any::TypeId::of::<GAlbedo>(),
                GAlbedo::NAME,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            (
                std::any::TypeId::of::<GMaterial>(),
                GMaterial::NAME,
                wgpu::TextureFormat::Rg8Unorm,
            ),
            (
                std::any::TypeId::of::<GDepth>(),
                GDepth::NAME,
                wgpu::TextureFormat::Depth32Float,
            ),
        ] {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(name),
                size: wgpu::Extent3d {
                    width: 64,
                    height: 64,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: fmt,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            table.allocate(
                type_id,
                name,
                tex.create_view(&wgpu::TextureViewDescriptor::default()),
            );
        }
        pass.resolve(&device, &table);
        assert!(pass.pos_handle.is_some());
    }
}

/// WGSL source for the G-buffer pass (mesh geometry + materials into G-buffer MRTs).
pub(crate) const GBUFFER_SHADER: &str = r#"
struct VertexInput { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) tangent: vec4<f32>, };
struct InstanceInput {
    @location(4) model_matrix_0: vec4<f32>,
    @location(5) model_matrix_1: vec4<f32>,
    @location(6) model_matrix_2: vec4<f32>,
    @location(7) model_matrix_3: vec4<f32>,
    @location(8) entity_id: u32,
};
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) world_pos: vec3<f32>, @location(1) world_normal: vec3<f32>, @location(2) uv: vec2<f32>, };
struct ViewProjUniform { view: mat4x4<f32>, proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> vp: ViewProjUniform;

struct ObjectData { albedo: vec4<f32>, roughness: f32, metallic: f32, };
@group(1) @binding(0) var<uniform> obj: ObjectData;
@group(2) @binding(0) var albedo_texture: texture_2d<f32>;
@group(2) @binding(1) var albedo_sampler: sampler;

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3);
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = vp.proj * vp.view * world_pos;
    out.world_pos = world_pos.xyz;
    let nm = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = normalize(nm * in.normal);
    out.uv = in.uv;
    return out;
}

struct FragmentOutput { @location(0) position: vec4<f32>, @location(1) normal: vec4<f32>, @location(2) albedo: vec4<f32>, @location(3) material: vec2<f32>, }
@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    out.position = vec4<f32>(in.world_pos, 1.0);
    out.normal = vec4<f32>(in.world_normal * 0.5 + 0.5, 1.0);
    let tex_color = textureSample(albedo_texture, albedo_sampler, in.uv);
    out.albedo = obj.albedo * tex_color;
    out.material = vec2<f32>(obj.roughness, obj.metallic);
    return out;
}
"#;
