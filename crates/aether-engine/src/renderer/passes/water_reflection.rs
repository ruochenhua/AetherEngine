//! Planar water reflection pass.
//!
//! Renders a mirror image of opaque meshes into a texture that the water pass
//! samples for reflections. The camera is mirrored across the water plane
//! (Y = level) and the scene is drawn with a simple forward lit shader.
//!
//! The pass is always present in the pipeline but skips execution when the
//! scene has no water or when `reflection_enabled` is false.

use crate::asset::mesh::{InstanceData, Vertex};
use crate::asset::texture::GpuTexture;
use crate::ecs::components::Terrain;
use crate::renderer::extract::RenderBatch;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::renderable::ObjectUniform;
use crate::renderer::resource::{WaterReflectionColor, WaterReflectionDepth};
use crate::renderer::resource_table::ResourceTable;
use crate::terrain::{
    create_terrain_material_bind_group, create_terrain_material_bind_group_layout,
    write_terrain_uniforms, ChunkInstanceData, TerrainGeometry, TerrainUniform,
};
use glam::{Mat4, Vec3};
use std::sync::{Arc, RwLock};

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

    terrain_geometry: Option<Arc<RwLock<TerrainGeometry>>>,
    terrain_pipeline: wgpu::RenderPipeline,
    terrain_buffer: wgpu::Buffer,
    terrain_bind_group: wgpu::BindGroup,
    terrain_bind_group_layout: wgpu::BindGroupLayout,
    terrain_last_splat: Option<Arc<GpuTexture>>,
    terrain_last_layer0: Option<Arc<GpuTexture>>,
    terrain_last_layer1: Option<Arc<GpuTexture>>,
    terrain_last_layer2: Option<Arc<GpuTexture>>,
    terrain_last_layer3: Option<Arc<GpuTexture>>,

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

        // Update terrain material for reflection rendering.
        self.terrain_geometry = frame.terrain_geometry.clone();
        if let Some(terrain) = frame.optional.terrain.as_ref() {
            self.update_terrain_material(terrain, frame.queue, frame.texture_cache, frame.asset_manager);
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

        // Render terrain into the reflection so shorelines/hills appear on the water.
        if let Some(terrain_geometry) = &self.terrain_geometry {
            let terrain = terrain_geometry.read().unwrap();
            let chunks = terrain.chunks();
            if !chunks.is_empty() {
                pass.set_pipeline(&self.terrain_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_bind_group(1, &self.terrain_bind_group, &[]);
                pass.set_vertex_buffer(1, terrain.instance_buffer().slice(..));
                for (chunk_index, chunk) in chunks.iter().enumerate() {
                    let lod_mesh = &terrain.chunk_meshes()[chunk_index][chunk.lod as usize];
                    let instance_start =
                        (chunk_index * std::mem::size_of::<ChunkInstanceData>())
                            as wgpu::BufferAddress;
                    let instance_end =
                        instance_start + std::mem::size_of::<ChunkInstanceData>()
                            as wgpu::BufferAddress;
                    pass.set_vertex_buffer(0, lod_mesh.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, terrain.instance_buffer().slice(instance_start..instance_end));
                    if let Some(ref ib) = lod_mesh.index_buffer {
                        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..lod_mesh.index_count, 0, 0..1);
                    } else {
                        pass.draw(0..lod_mesh.vertex_count, 0..1);
                    }
                }
            }
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

        // Terrain reflection shader: splatted albedo with simple forward lighting.
        let terrain_shader_source = r#"
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
    @location(8) lod: u32,
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

struct TerrainUniform {
    layer_color_0: vec4<f32>,
    layer_color_1: vec4<f32>,
    layer_color_2: vec4<f32>,
    layer_color_3: vec4<f32>,
    layer_roughness: vec4<f32>,
    layer_metallic: vec4<f32>,
    has_splat_map: u32,
    _pad0: u32,
    splat_uv_scale: f32,
    albedo_uv_scale: f32,
    layer_uv_scale: vec4<f32>,
};
@group(1) @binding(0) var<uniform> terrain: TerrainUniform;
@group(1) @binding(1) var splat_map: texture_2d<f32>;
@group(1) @binding(2) var terrain_sampler: sampler;
@group(1) @binding(3) var layer_albedo_0: texture_2d<f32>;
@group(1) @binding(4) var layer_albedo_1: texture_2d<f32>;
@group(1) @binding(5) var layer_albedo_2: texture_2d<f32>;
@group(1) @binding(6) var layer_albedo_3: texture_2d<f32>;

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3);
    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = ru.proj * ru.view * world_pos;
    out.world_pos = world_pos.xyz;
    let nm = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    out.world_normal = normalize(nm * in.normal);
    out.uv = world_pos.xz;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let splat_uv = in.uv * terrain.splat_uv_scale + vec2<f32>(0.5);
    let albedo_uv = in.uv * terrain.albedo_uv_scale;

    var weights: vec4<f32>;
    if (terrain.has_splat_map != 0u) {
        weights = textureSample(splat_map, terrain_sampler, splat_uv);
    } else {
        weights = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }

    let uv0 = albedo_uv * terrain.layer_uv_scale.x;
    let uv1 = albedo_uv * terrain.layer_uv_scale.y;
    let uv2 = albedo_uv * terrain.layer_uv_scale.z;
    let uv3 = albedo_uv * terrain.layer_uv_scale.w;
    let c0 = terrain.layer_color_0 * textureSample(layer_albedo_0, terrain_sampler, uv0);
    let c1 = terrain.layer_color_1 * textureSample(layer_albedo_1, terrain_sampler, uv1);
    let c2 = terrain.layer_color_2 * textureSample(layer_albedo_2, terrain_sampler, uv2);
    let c3 = terrain.layer_color_3 * textureSample(layer_albedo_3, terrain_sampler, uv3);

    let albedo = c0 * weights.x + c1 * weights.y + c2 * weights.z + c3 * weights.w;

    let n = normalize(in.world_normal);
    let l = normalize(ru.light_dir.xyz);
    let v = normalize(-in.world_pos);
    let h = normalize(l + v);

    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);

    let diffuse = albedo.rgb * n_dot_l * ru.light_color.rgb;
    let specular = pow(n_dot_h, 64.0) * ru.light_color.rgb;
    let ambient = albedo.rgb * ru.ambient.rgb;

    return vec4<f32>(ambient + diffuse + specular * 0.3, 1.0);
}
"#;
        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("WaterReflection Terrain Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(terrain_shader_source)),
        });

        let terrain_bind_group_layout = create_terrain_material_bind_group_layout(device);
        let terrain_pipeline_layout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("WaterReflection Terrain Pipeline Layout"),
                bind_group_layouts: &[
                    Some(&uniform_bind_group_layout),
                    Some(&terrain_bind_group_layout),
                ],
                immediate_size: 0,
            },
        );

        let terrain_pipeline = device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("WaterReflection Terrain Pipeline"),
                layout: Some(&terrain_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &terrain_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Vertex::desc(), ChunkInstanceData::desc()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &terrain_shader,
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
            },
        );

        let terrain_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WaterReflection Terrain Material Buf"),
            size: std::mem::size_of::<TerrainUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let fallback_white = Arc::new(GpuTexture::from_cpu(
            device,
            _queue,
            &crate::asset::texture::CpuTexture::from_color(255, 255, 255, 255),
            Some("water_reflection_terrain_fallback_white"),
        ));
        let terrain_bind_group = create_terrain_material_bind_group(
            device,
            &terrain_bind_group_layout,
            &terrain_buffer,
            &fallback_white,
            &fallback_white,
            &fallback_white,
            &fallback_white,
            &fallback_white,
        );

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
            terrain_geometry: None,
            terrain_pipeline,
            terrain_buffer,
            terrain_bind_group,
            terrain_bind_group_layout,
            terrain_last_splat: None,
            terrain_last_layer0: None,
            terrain_last_layer1: None,
            terrain_last_layer2: None,
            terrain_last_layer3: None,
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

    fn update_terrain_material(
        &mut self,
        terrain: &Terrain,
        queue: &wgpu::Queue,
        texture_cache: &crate::asset::texture_cache::GpuTextureCache,
        asset_manager: &crate::asset::AssetManager,
    ) {
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
            &self.terrain_last_splat,
            &self.terrain_last_layer0,
            &self.terrain_last_layer1,
            &self.terrain_last_layer2,
            &self.terrain_last_layer3,
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
            self.terrain_last_splat = Some(splat);
            self.terrain_last_layer0 = Some(layer0);
            self.terrain_last_layer1 = Some(layer1);
            self.terrain_last_layer2 = Some(layer2);
            self.terrain_last_layer3 = Some(layer3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::renderer::extract::extract_optional_pass_data;
    use crate::renderer::frame::{FrameConfig, RenderFrame};
    use crate::renderer::light::LightingUniforms;
    use crate::scene::{TerrainGeometry as TerrainGeometryConfig, TerrainSource, WaterConfig};

    fn headless_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
    }

    #[test]
    fn water_reflection_pass_stores_terrain_geometry() {
        let (device, queue) = headless_device();
        let pass = WaterReflectionPass::new(&device, &queue);
        assert!(pass.terrain_geometry.is_none());

        let mut world = World::new();
        world.spawn((crate::ecs::components::Water {
            config: WaterConfig {
                reflection_enabled: true,
                ..Default::default()
            },
            dudv_texture: None,
            normal_texture: None,
        },));
        let terrain = crate::ecs::components::Terrain {
            source: TerrainSource::Procedural {
                seed: 0,
                frequency: 0.05,
                amplitude: 32.0,
            },
            geometry: TerrainGeometryConfig::default(),
            material: crate::asset::terrain_material::TerrainMaterial::default(),
            splatmap_path: None,
            layer_configs: Vec::new(),
        };
        world.spawn((terrain.clone(),));
        let optional = extract_optional_pass_data(&world);
        let camera = crate::renderer::camera::FlyCamera::default();
        let lighting = LightingUniforms::default();
        let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
        let asset_manager = crate::asset::AssetManager::new();

        let mut terrain_geom = TerrainGeometry::new(&device);
        terrain_geom.update(&device, &queue, &camera, 16.0 / 9.0, &terrain);

        let frame = RenderFrame {
            batches: std::sync::Arc::from([]),
            camera: &camera,
            lighting: &lighting,
            queue: &queue,
            aspect: 16.0 / 9.0,
            delta_time: 0.016,
            config: &FrameConfig::default(),
            optional: &optional,
            terrain_geometry: Some(std::sync::Arc::new(std::sync::RwLock::new(terrain_geom))),
            texture_cache: &texture_cache,
            asset_manager: &asset_manager,
        };

        let mut pass = WaterReflectionPass::new(&device, &queue);
        pass.apply_frame(&frame);
        assert!(pass.should_run(&frame));
        assert!(pass.terrain_geometry.is_some());
    }
}
