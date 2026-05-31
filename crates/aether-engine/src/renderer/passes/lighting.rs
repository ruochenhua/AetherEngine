//! Lighting Pass
//!
//! Full-screen quad pass that reads G-Buffer textures and computes
//! Blinn-Phong lighting. Outputs directly to the swapchain.
//!
//! Implements the `Pass` trait for type-safe scheduling.

use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use wgpu::util::DeviceExt;

/// Directional light data.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DirectionalLight {
    /// Light direction (pointing FROM the light).
    pub direction: [f32; 3],
    /// Padding.
    pub _pad: f32,
    /// Light color.
    pub color: [f32; 3],
    /// Light intensity.
    pub intensity: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: [-1.0, -1.0, -1.0],
            _pad: 0.0,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
        }
    }
}

/// Lighting uniform data.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightingUniforms {
    /// Camera world position.
    pub camera_pos: [f32; 3],
    /// Padding.
    pub _pad1: f32,
    /// Directional light.
    pub light: DirectionalLight,
    /// Ambient light intensity.
    pub ambient_intensity: f32,
    /// Debug visualization mode:
    /// 0 = full lighting, 1 = ambient only, 2 = diffuse only,
    /// 3 = specular only, 4 = normals, 5 = NdotL.
    pub debug_mode: u32,
    #[allow(dead_code)]
    pub(crate) _pad2: [f32; 2],
    /// Light-space view-projection for shadow sampling.
    pub light_view_proj: [[f32; 4]; 4],
}

impl Default for LightingUniforms {
    fn default() -> Self {
        Self {
            camera_pos: [3.0, 3.0, 3.0],
            _pad1: 0.0,
            light: DirectionalLight::default(),
            ambient_intensity: 0.1,
            debug_mode: 0,
            _pad2: [0.0; 2],
            light_view_proj: [[0.0; 4]; 4],
        }
    }
}

/// Lighting Pass implementation.
pub struct LightingPass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    /// G-Buffer texture handles (populated by resolve).
    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    albedo_handle: Option<ResHandle<GAlbedo>>,
    material_handle: Option<ResHandle<GMaterial>>,
    /// Shadow depth handle (populated by resolve).
    shadow_depth_handle: Option<ResHandle<ShadowDepth>>,
    /// Texture bind group (recreated during resolve).
    texture_bind_group: Option<wgpu::BindGroup>,
    /// Shadow bind group (recreated during resolve).
    shadow_bind_group: Option<wgpu::BindGroup>,
    /// Uniform bind group.
    uniform_bind_group: wgpu::BindGroup,
    /// Bind group layouts (needed for recreate).
    texture_bind_group_layout: wgpu::BindGroupLayout,
    shadow_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    /// Surface format for render pipeline creation.
    surface_format: wgpu::TextureFormat,
}

impl Pass for LightingPass {
    fn name(&self) -> &str {
        "Lighting"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Lighting")
            .read::<GPosition>("gbuffer_position")
            .read::<GNormal>("gbuffer_normal")
            .read::<GAlbedo>("gbuffer_albedo")
            .read::<GMaterial>("gbuffer_material")
            .read::<ShadowDepth>("shadow_depth")
    }

    fn init(device: &wgpu::Device) -> Self {
        // Default format — will be set properly via new().
        Self::new_inner(device, wgpu::TextureFormat::Bgra8UnormSrgb)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.pos_handle = Some(resources.handle::<GPosition>("gbuffer_position"));
        self.normal_handle = Some(resources.handle::<GNormal>("gbuffer_normal"));
        self.albedo_handle = Some(resources.handle::<GAlbedo>("gbuffer_albedo"));
        self.material_handle = Some(resources.handle::<GMaterial>("gbuffer_material"));
        self.shadow_depth_handle = Some(resources.handle::<ShadowDepth>("shadow_depth"));

        // Create samplers
        let gbuffer_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("GBuffer Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::Less),
            ..Default::default()
        });

        let shadow_debug_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow Debug Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let pos_view = resources.get(self.pos_handle.unwrap());
        let norm_view = resources.get(self.normal_handle.unwrap());
        let albedo_view = resources.get(self.albedo_handle.unwrap());
        let material_view = resources.get(self.material_handle.unwrap());
        let shadow_view = resources.get(self.shadow_depth_handle.unwrap());

        self.texture_bind_group = Some(device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("Lighting Texture Bind Group"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(pos_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(norm_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(albedo_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(material_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&gbuffer_sampler),
                    },
                ],
            },
        ));

        self.shadow_bind_group = Some(device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("Lighting Shadow Bind Group"),
                layout: &self.shadow_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(shadow_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&shadow_debug_sampler),
                    },
                ],
            },
        ));
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        _resources: &ResourceTable,
        _queue: &wgpu::Queue,
        surface_view: &wgpu::TextureView,
    ) {
        let texture_bg = self.texture_bind_group.as_ref()
            .expect("LightingPass: resolve not called");

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Lighting Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, texture_bg, &[]);
        pass.set_bind_group(1, &self.uniform_bind_group, &[]);
        pass.set_bind_group(2, self.shadow_bind_group.as_ref().expect("Shadow BG not set"), &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }
}

impl LightingPass {
    /// Create a new lighting pass with the given surface format.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        Self::new_inner(device, surface_format)
    }

    fn new_inner(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader_source = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(position, 0.0, 1.0);
    out.uv = vec2<f32>(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5);
    return out;
}

struct DirectionalLight {
    direction: vec3<f32>,
    _pad: f32,
    color: vec3<f32>,
    intensity: f32,
};

struct LightingUniforms {
    camera_pos: vec3<f32>,
    _pad1: f32,
    light: DirectionalLight,
    ambient_intensity: f32,
    debug_mode: u32,
    _pad2: f32,
    _pad3: f32,
    light_view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var gbuffer_position: texture_2d<f32>;
@group(0) @binding(1) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuffer_albedo: texture_2d<f32>;
@group(0) @binding(3) var gbuffer_material: texture_2d<f32>;
@group(0) @binding(4) var gbuffer_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: LightingUniforms;

@group(2) @binding(0) var shadow_depth: texture_depth_2d;
@group(2) @binding(1) var shadow_sampler: sampler_comparison;
@group(2) @binding(2) var shadow_debug_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let position_sample = textureSample(gbuffer_position, gbuffer_sampler, uv);
    let normal_sample = textureSample(gbuffer_normal, gbuffer_sampler, uv);
    let albedo_sample = textureSample(gbuffer_albedo, gbuffer_sampler, uv);
    let material_sample = textureSample(gbuffer_material, gbuffer_sampler, uv);

    let world_pos = position_sample.xyz;
    if (normal_sample.a < 0.5) {
        return vec4<f32>(0.05, 0.05, 0.05, 1.0);
    }

    let N = normalize(normal_sample.xyz * 2.0 - 1.0);
    let albedo = albedo_sample.rgb;
    let roughness = material_sample.r;
    let metallic = material_sample.g;

    let L = normalize(-uniforms.light.direction);
    let V = normalize(uniforms.camera_pos - world_pos);
    let H = normalize(L + V);

    let ambient = albedo * uniforms.ambient_intensity;
    let NdotL = max(dot(N, L), 0.0);
    let diffuse = albedo * NdotL * uniforms.light.color * uniforms.light.intensity;

    let NdotH = max(dot(N, H), 0.0);
    let shininess = mix(8.0, 128.0, 1.0 - roughness);
    let specular_intensity = pow(NdotH, shininess);
    let specular_color = mix(vec3<f32>(0.04), albedo, metallic);
    let specular = specular_color * specular_intensity * uniforms.light.intensity;

    let lit_color = ambient + diffuse + specular;

    // Shadow: transform world_pos to light space, sample with PCF
    let light_clip = uniforms.light_view_proj * vec4<f32>(world_pos, 1.0);
    var visibility: f32 = 1.0;
    if (light_clip.w > 0.0) {
        let light_ndc = light_clip.xyz / light_clip.w;
        let uv = vec2<f32>(light_ndc.x * 0.5 + 0.5, 0.5 - light_ndc.y * 0.5);
        // Slope-scale bias: more bias at grazing angles, less when facing the light
        let bias = max(0.0005, 0.005 * (1.0 - NdotL));
        let ref_depth = light_ndc.z - bias;
        // PCF 3x3
        let texel_size = 1.0 / 1024.0;
        visibility = 0.0;
        for (var x: i32 = -1; x <= 1; x = x + 1) {
            for (var y: i32 = -1; y <= 1; y = y + 1) {
                let offset = vec2<f32>(f32(x) * texel_size, f32(y) * texel_size);
                visibility = visibility + textureSampleCompare(
                    shadow_depth, shadow_sampler, uv + offset, ref_depth
                );
            }
        }
        visibility = visibility / 9.0;
    }
    let shadow_factor = mix(0.3, 1.0, visibility);
    let final_color = lit_color * shadow_factor;

    var output_color: vec3<f32>;
    if (uniforms.debug_mode == 1u) {
        output_color = ambient;
    } else if (uniforms.debug_mode == 2u) {
        output_color = diffuse;
    } else if (uniforms.debug_mode == 3u) {
        output_color = specular;
    } else if (uniforms.debug_mode == 4u) {
        output_color = N * 0.5 + 0.5;
    } else if (uniforms.debug_mode == 5u) {
        output_color = vec3<f32>(NdotL);
    } else if (uniforms.debug_mode == 6u) {
        // Shadow map as seen from light: sample depth at screen UV
        let d = textureSample(shadow_depth, shadow_debug_sampler, in.uv);
        output_color = vec3<f32>(d);
    } else {
        output_color = final_color;
    }

    // Tone mapping — skip for debug mode 6 to see raw depth values
    let mapped = select(
        output_color / (output_color + vec3<f32>(1.0)),
        output_color,
        uniforms.debug_mode == 6u
    );
    return vec4<f32>(mapped, 1.0);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Lighting Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Lighting Texture Bind Group Layout"),
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
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
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
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let shadow_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Shadow Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Lighting Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Lighting Pipeline Layout"),
            bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout, &shadow_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Lighting Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    }],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Lighting Uniform Buffer"),
            size: std::mem::size_of::<LightingUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lighting Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Full-screen quad vertices (2 triangles)
        let quad_vertices: [[f32; 2]; 6] = [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ];

        let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Lighting Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            uniform_buffer,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            pos_handle: None,
            normal_handle: None,
            albedo_handle: None,
            material_handle: None,
            shadow_depth_handle: None,
            texture_bind_group: None,
            shadow_bind_group: None,
            uniform_bind_group,
            texture_bind_group_layout,
            shadow_bind_group_layout,
            uniform_bind_group_layout,
            surface_format,
        }
    }

    /// Update lighting uniforms.
    pub fn update_uniforms(&self, queue: &wgpu::Queue, uniforms: &LightingUniforms) {
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[*uniforms]),
        );
    }
}
