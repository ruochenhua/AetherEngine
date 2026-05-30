//! Lighting Pass
//!
//! Full-screen quad pass that reads G-Buffer textures and computes
//! Blinn-Phong lighting. Outputs directly to the swapchain.

use crate::renderer::context::GBuffer;

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
        }
    }
}

/// Lighting Pass implementation.
pub struct LightingPass {
    pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    /// Texture bind group (gbuffer textures + sampler).
    texture_bind_group: wgpu::BindGroup,
    /// Uniform bind group (lighting params).
    uniform_bind_group: wgpu::BindGroup,
}

impl LightingPass {
    /// Create a new lighting pass.
    pub fn new(
        device: &wgpu::Device,
        gbuffer: &GBuffer,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        let shader_source = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(position, 0.0, 1.0);
    // Flip Y: wgpu NDC Y=1 is top, but texture UV=(0,0) is also top,
    // so position.y * 0.5 + 0.5 maps bottom→top. We negate y to
    // correctly align the G-Buffer sample with the rendered geometry.
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
};

@group(0) @binding(0) var gbuffer_position: texture_2d<f32>;
@group(0) @binding(1) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuffer_albedo: texture_2d<f32>;
@group(0) @binding(3) var gbuffer_material: texture_2d<f32>;
@group(0) @binding(4) var gbuffer_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: LightingUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;

    // Sample G-Buffer
    let position_sample = textureSample(gbuffer_position, gbuffer_sampler, uv);
    let normal_sample = textureSample(gbuffer_normal, gbuffer_sampler, uv);
    let albedo_sample = textureSample(gbuffer_albedo, gbuffer_sampler, uv);
    let material_sample = textureSample(gbuffer_material, gbuffer_sampler, uv);

    let world_pos = position_sample.xyz;
    // Skip background pixels: G-Buffer normal alpha is 1.0 only where geometry was written.
    // Clear color has alpha 0.0.
    if (normal_sample.a < 0.5) {
        return vec4<f32>(0.05, 0.05, 0.05, 1.0);
    }

    // Decode normal from [0,1] back to [-1,1]
    let N = normalize(normal_sample.xyz * 2.0 - 1.0);
    let albedo = albedo_sample.rgb;
    let roughness = material_sample.r;
    let metallic = material_sample.g;

    // Light calculations
    let L = normalize(-uniforms.light.direction);
    let V = normalize(uniforms.camera_pos - world_pos);
    let H = normalize(L + V);

    // Ambient
    let ambient = albedo * uniforms.ambient_intensity;

    // Diffuse
    let NdotL = max(dot(N, L), 0.0);
    let diffuse = albedo * NdotL * uniforms.light.color * uniforms.light.intensity;

    // Specular (Blinn-Phong)
    let NdotH = max(dot(N, H), 0.0);
    let shininess = mix(8.0, 128.0, 1.0 - roughness);
    let specular_intensity = pow(NdotH, shininess);
    let specular_color = mix(vec3<f32>(0.04), albedo, metallic);
    let specular = specular_color * specular_intensity * uniforms.light.intensity;

    let final_color = ambient + diffuse + specular;

    // Debug visualization: select output based on debug_mode
    var output_color: vec3<f32>;
    if (uniforms.debug_mode == 1u) {
        // Ambient only
        output_color = ambient;
    } else if (uniforms.debug_mode == 2u) {
        // Diffuse only
        output_color = diffuse;
    } else if (uniforms.debug_mode == 3u) {
        // Specular only
        output_color = specular;
    } else if (uniforms.debug_mode == 4u) {
        // Normals as color
        output_color = N * 0.5 + 0.5;
    } else if (uniforms.debug_mode == 5u) {
        // NdotL as grayscale
        output_color = vec3<f32>(NdotL);
    } else {
        // Full lighting (mode 0)
        output_color = final_color;
    }

    // Simple tone mapping (Reinhard). Gamma encoding is handled by the sRGB framebuffer.
    let mapped = output_color / (output_color + vec3<f32>(1.0));

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
                    // G-Buffer position
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
                    // G-Buffer normal
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
                    // G-Buffer albedo
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
                    // G-Buffer material
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
                    // Sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
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
            bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
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
                    format: config.format,
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("GBuffer Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lighting Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&gbuffer.position),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gbuffer.normal),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&gbuffer.albedo),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&gbuffer.material),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
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
            texture_bind_group_layout,
            uniform_bind_group_layout,
            uniform_buffer,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            texture_bind_group,
            uniform_bind_group,
        }
    }

    /// Update lighting uniforms.
    pub fn update_uniforms(
        &self, queue: &wgpu::Queue, uniforms: &LightingUniforms,
    ) {
        queue.write_buffer(
            &self.uniform_buffer, 0, bytemuck::cast_slice(&[*uniforms]),
        );
    }

    /// Recreate texture bind group after G-Buffer resize.
    pub fn recreate_bind_group(
        &mut self, device: &wgpu::Device, gbuffer: &GBuffer,
    ) {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("GBuffer Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        self.texture_bind_group = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("Lighting Texture Bind Group"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &gbuffer.position,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &gbuffer.normal,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            &gbuffer.albedo,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(
                            &gbuffer.material,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            },
        );
    }

    /// Execute the lighting pass (creates its own render pass).
    pub fn execute(
        &self, encoder: &mut wgpu::CommandEncoder, output_view: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("Lighting Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            },
        );
        self.execute_in_pass(&mut pass);
    }

    /// Execute the lighting pass into an existing render pass.
    pub fn execute_in_pass<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.texture_bind_group, &[]);
        pass.set_bind_group(1, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }
}