//! Planar water reflection pass.
//!
//! Renders a mirror image of opaque meshes into a texture that the water pass
//! samples for reflections. The camera is mirrored across the water plane
//! (Y = level) and the scene is drawn with a simple forward lit shader.
//!
//! The pass is always present in the pipeline but skips execution when the
//! scene has no water or when `reflection_enabled` is false.

use crate::asset::mesh::{InstanceData, Vertex};
use crate::renderer::extract::RenderBatch;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::renderable::ObjectUniform;
use crate::renderer::resource::{WaterReflectionColor, WaterReflectionDepth};
use crate::renderer::resource_table::ResourceTable;
use glam::{Mat4, Vec3};
use std::sync::Arc;

/// Planar reflection pass state.
pub struct WaterReflectionPass {
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    object_buffer: wgpu::Buffer,
    object_buffer_capacity: usize,
    object_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_groups: Vec<wgpu::BindGroup>,
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,

    color_handle: Option<ResHandle<WaterReflectionColor>>,
    depth_handle: Option<ResHandle<WaterReflectionDepth>>,

    batches: Arc<[RenderBatch]>,
    view: Mat4,
    proj: Mat4,
    light_dir: Vec3,
    light_color: Vec3,
    ambient: Vec3,
    has_water: bool,
    reflection_enabled: bool,
    reflection_level: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ReflectionUniform {
    view: [[f32; 4]; 4],
    proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient: [f32; 4],
}

impl Pass for WaterReflectionPass {
    fn name(&self) -> &str {
        "WaterReflection"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("WaterReflection")
            .write::<WaterReflectionColor>(wgpu::TextureFormat::Rgba16Float)
            .write::<WaterReflectionDepth>(wgpu::TextureFormat::Depth32Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device, ctx.queue)
    }

    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.color_handle = Some(resources.handle::<WaterReflectionColor>());
        self.depth_handle = Some(resources.handle::<WaterReflectionDepth>());
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        if let Some(water) = frame.optional.water.clone() {
            self.has_water = true;
            self.reflection_enabled = water.config.reflection_enabled;
            self.reflection_level = water.config.level;
        } else {
            self.has_water = false;
            self.reflection_enabled = false;
            return;
        }

        if !self.reflection_enabled {
            return;
        }

        self.batches = frame.batches.clone();

        // Mirror the camera across the water plane by post-multiplying the view
        // matrix with a Y-reflection matrix. This is more robust than manually
        // adjusting position/pitch.
        let level = self.reflection_level;
        let reflection_matrix = Mat4::from_cols_array_2d(&[
            [1.0, 0.0, 0.0, 0.0],
            [0.0, -1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 2.0 * level, 0.0, 1.0],
        ]);

        self.view = frame.camera.view_matrix() * reflection_matrix;
        self.proj = frame.camera.projection_matrix(frame.aspect);

        let light = &frame.lighting.light;
        self.light_dir = Vec3::new(
            -light.direction[0],
            -light.direction[1],
            -light.direction[2],
        );
        self.light_color = Vec3::new(
            light.color[0] * light.intensity,
            light.color[1] * light.intensity,
            light.color[2] * light.intensity,
        );
        self.ambient = Vec3::splat(frame.lighting.ambient_intensity);

        frame.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[ReflectionUniform {
                view: self.view.to_cols_array_2d(),
                proj: self.proj.to_cols_array_2d(),
                light_dir: self.light_dir.extend(0.0).to_array(),
                light_color: self.light_color.extend(0.0).to_array(),
                ambient: self.ambient.extend(0.0).to_array(),
            }]),
        );

        let obj_size = std::mem::size_of::<ObjectUniform>() as wgpu::BufferAddress;
        let batch_count = self.batches.len();
        if batch_count > self.object_buffer_capacity {
            let new_capacity = batch_count.max(256);
            self.object_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("WaterReflection Obj Buf"),
                size: (new_capacity as u64) * obj_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.object_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("WaterReflection Obj BG"),
                layout: &self.pipeline.get_bind_group_layout(1),
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

        let total_instances: usize = self.batches.iter().map(|b| b.instances.len()).sum();
        if total_instances > self.instance_buffer_capacity {
            let new_capacity = total_instances.max(256);
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("WaterReflection Instance Buf"),
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

        self.texture_bind_groups.clear();
        for batch in self.batches.iter() {
            let gpu_tex = frame
                .texture_cache
                .get_or_upload_optional(batch.albedo_texture.clone(), frame.asset_manager);
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("WaterReflection Texture BG"),
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

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_water && self.reflection_enabled
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("WaterReflection"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: resources.get(self.color_handle.unwrap()),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
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
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);

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

impl WaterReflectionPass {
    /// Create a new planar reflection pass.
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue) -> Self {
        let shader_source = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};
struct InstanceInput {
    @location(4) model_matrix_0: vec4<f32>,
    @location(5) model_matrix_1: vec4<f32>,
    @location(6) model_matrix_2: vec4<f32>,
    @location(7) model_matrix_3: vec4<f32>,
    @location(8) entity_id: u32,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};
struct ReflectionUniform {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    ambient: vec4<f32>,
};
@group(0) @binding(0) var<uniform> ru: ReflectionUniform;

struct ObjectData { albedo: vec4<f32>, roughness: f32, metallic: f32, };
@group(1) @binding(0) var<uniform> obj: ObjectData;
@group(2) @binding(0) var albedo_texture: texture_2d<f32>;
@group(2) @binding(1) var albedo_sampler: sampler;

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3);
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = ru.proj * ru.view * world_pos;
    out.world_pos = world_pos.xyz;
    let nm = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = normalize(nm * in.normal);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(albedo_texture, albedo_sampler, in.uv);
    let albedo = obj.albedo.rgb * tex_color.rgb;

    let n = normalize(in.world_normal);
    let l = normalize(ru.light_dir.xyz);
    let v = normalize(-in.world_pos);
    let h = normalize(l + v);

    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);

    let diffuse = albedo * n_dot_l * ru.light_color.rgb;
    let specular = pow(n_dot_h, 64.0) * ru.light_color.rgb;
    let ambient = albedo * ru.ambient.rgb;

    return vec4<f32>(ambient + diffuse + specular * 0.3, 1.0);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("WaterReflection Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("WaterReflection Uniform BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
                label: Some("WaterReflection Obj BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
                label: Some("WaterReflection Texture BGL"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("WaterReflection Pipeline Layout"),
            bind_group_layouts: &[
                Some(&uniform_bind_group_layout),
                Some(&object_bind_group_layout),
                Some(&texture_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("WaterReflection Pipeline"),
            layout: Some(&pipeline_layout),
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
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // Reflection flips triangle winding, so cull the opposite faces.
                cull_mode: Some(wgpu::Face::Front),
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

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WaterReflection Uniform Buffer"),
            size: std::mem::size_of::<ReflectionUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WaterReflection Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let object_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WaterReflection Object Buffer"),
            size: 256 * obj_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let object_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WaterReflection Object Bind Group"),
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

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WaterReflection Instance Buffer"),
            size: (256 * std::mem::size_of::<InstanceData>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device: device.clone(),
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            object_buffer,
            object_buffer_capacity: 256,
            object_bind_group,
            texture_bind_group_layout,
            texture_bind_groups: Vec::new(),
            instance_buffer,
            instance_buffer_capacity: 256,
            color_handle: None,
            depth_handle: None,
            batches: Arc::from([]),
            view: Mat4::IDENTITY,
            proj: Mat4::IDENTITY,
            light_dir: Vec3::Y,
            light_color: Vec3::ONE,
            ambient: Vec3::splat(0.1),
            has_water: false,
            reflection_enabled: false,
            reflection_level: 0.0,
        }
    }
}
