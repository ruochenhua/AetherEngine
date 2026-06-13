//! Water Pass — transparent forward water surface with Gerstner waves.
//!
//! Renders a large subdivided plane displaced by Gerstner waves on the GPU.
//! The pass runs after SSR and before composite. It samples the lit scene
//! color for refraction and the SSR reflection texture for reflections, then
//! blends the result into a separate `WaterColor` overlay that the composite
//! pass mixes over the opaque scene.

use crate::asset::mesh::{CpuMesh, GpuMesh, Vertex};
use crate::ecs::components::Water;
use crate::ecs::World;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::{GDepth, ReflectionTexture, SceneColor, WaterColor};
use crate::renderer::resource_table::ResourceTable;
use std::sync::Arc;

/// GPU uniform data for the water shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WaterUniform {
    /// View-projection matrix.
    pub view_proj: glam::Mat4,
    /// Camera world-space position (xyz, w unused).
    pub camera_pos: glam::Vec4,
    /// Shallow water color (rgb, a unused).
    pub water_color: glam::Vec4,
    /// Deep water color (rgb, a unused).
    pub deep_color: glam::Vec4,
    /// Wave direction on the XZ plane.
    pub wave_direction: glam::Vec2,
    /// Wave amplitude.
    pub wave_amplitude: f32,
    /// Wave wavelength.
    pub wave_wavelength: f32,
    /// Wave speed.
    pub wave_speed: f32,
    /// Wave steepness.
    pub wave_steepness: f32,
    /// Current animation time in seconds.
    pub time: f32,
    /// Water level (world-space Y).
    pub level: f32,
    /// Fresnel power.
    pub fresnel_power: f32,
    /// Refraction UV distortion scale.
    pub refraction_scale: f32,
    /// Reflection intensity multiplier.
    pub reflectivity: f32,
    /// Explicit padding to reach a 192-byte struct size.
    pub _pad0: f32,
    /// Explicit padding.
    pub _pad1: f32,
    /// Explicit padding.
    pub _pad2: f32,
    /// Explicit padding.
    pub _pad3: f32,
    /// Explicit padding.
    pub _pad4: f32,
    /// Explicit padding.
    pub _pad5: f32,
    /// Explicit padding.
    pub _pad6: f32,
    /// Explicit padding.
    pub _pad7: f32,
    /// Explicit padding.
    pub _pad8: f32,
}

impl Default for WaterUniform {
    fn default() -> Self {
        Self {
            view_proj: glam::Mat4::IDENTITY,
            camera_pos: glam::Vec4::new(0.0, 0.0, 0.0, 0.0),
            water_color: glam::Vec4::new(0.0, 0.35, 0.45, 1.0),
            deep_color: glam::Vec4::new(0.0, 0.15, 0.25, 1.0),
            wave_direction: glam::Vec2::new(1.0, 0.5),
            wave_amplitude: 0.3,
            wave_wavelength: 8.0,
            wave_speed: 2.0,
            wave_steepness: 0.6,
            time: 0.0,
            level: 0.0,
            fresnel_power: 3.0,
            refraction_scale: 0.02,
            reflectivity: 0.6,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
            _pad4: 0.0,
            _pad5: 0.0,
            _pad6: 0.0,
            _pad7: 0.0,
            _pad8: 0.0,
        }
    }
}

/// Water render pass.
pub struct WaterPass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group: Option<wgpu::BindGroup>,
    mesh: Arc<GpuMesh>,
    scene_color_handle: Option<ResHandle<SceneColor>>,
    reflection_handle: Option<ResHandle<ReflectionTexture>>,
    depth_handle: Option<ResHandle<GDepth>>,
    water_color_handle: Option<ResHandle<WaterColor>>,
    has_water: bool,
    time: f32,
}

impl Pass for WaterPass {
    fn name(&self) -> &str {
        "Water"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Water")
            .read::<SceneColor>("scene_color")
            .read::<ReflectionTexture>("reflection")
            .read::<GDepth>("gbuffer_depth")
            .write::<WaterColor>("water_color", wgpu::TextureFormat::Rgba16Float)
    }

    fn init(device: &wgpu::Device) -> Self {
        Self::new(device)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.scene_color_handle = Some(resources.handle::<SceneColor>("scene_color"));
        self.reflection_handle = Some(resources.handle::<ReflectionTexture>("reflection"));
        self.depth_handle = Some(resources.handle::<GDepth>("gbuffer_depth"));
        self.water_color_handle = Some(resources.handle::<WaterColor>("water_color"));

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Water Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Water Texture Bind Group Layout"),
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
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Water Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        resources.get(self.scene_color_handle.unwrap()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        resources.get(self.reflection_handle.unwrap()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        }));
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_water
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        if let Some(water) = Self::read_water(frame.world) {
            self.has_water = true;
            self.time += frame.delta_time;

            let proj = frame.camera.projection_matrix(frame.aspect);
            let view = frame.camera.view_matrix();
            let view_proj = proj * view;

            let cfg = water.config;
            let uniforms = WaterUniform {
                view_proj,
                camera_pos: frame.camera.position.extend(0.0),
                water_color: glam::Vec4::from_array([
                    cfg.water_color[0],
                    cfg.water_color[1],
                    cfg.water_color[2],
                    1.0,
                ]),
                deep_color: glam::Vec4::from_array([
                    cfg.deep_color[0],
                    cfg.deep_color[1],
                    cfg.deep_color[2],
                    1.0,
                ]),
                wave_direction: glam::Vec2::from_array(cfg.wave_direction),
                wave_amplitude: cfg.wave_amplitude,
                wave_wavelength: cfg.wave_wavelength,
                wave_speed: cfg.wave_speed,
                wave_steepness: cfg.wave_steepness,
                time: self.time,
                level: cfg.level,
                fresnel_power: cfg.fresnel_power,
                refraction_scale: cfg.refraction_scale,
                reflectivity: cfg.reflectivity,
                _pad0: 0.0,
                _pad1: 0.0,
                _pad2: 0.0,
                _pad3: 0.0,
                _pad4: 0.0,
                _pad5: 0.0,
                _pad6: 0.0,
                _pad7: 0.0,
                _pad8: 0.0,
            };
            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        } else {
            self.has_water = false;
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        if !self.has_water {
            return;
        }

        let water_color_view = resources.get(self.water_color_handle.unwrap());
        let depth_view = resources.get(self.depth_handle.unwrap());
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("WaterPass: resolve not called");

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Water Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: water_color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
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
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_bind_group(1, texture_bg, &[]);
        pass.set_vertex_buffer(0, self.mesh.vertex_buffer.slice(..));
        if let Some(ref index_buffer) = self.mesh.index_buffer {
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.mesh.index_count, 0, 0..1);
        } else {
            pass.draw(0..self.mesh.vertex_count, 0..1);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl WaterPass {
    /// Create a new water pass.
    pub fn new(device: &wgpu::Device) -> Self {
        let output_format = wgpu::TextureFormat::Rgba16Float;
        let shader_source = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct WaterUniform {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    water_color: vec4<f32>,
    deep_color: vec4<f32>,
    wave_direction: vec2<f32>,
    wave_amplitude: f32,
    wave_wavelength: f32,
    wave_speed: f32,
    wave_steepness: f32,
    time: f32,
    level: f32,
    fresnel_power: f32,
    refraction_scale: f32,
    reflectivity: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
    _pad4: f32,
    _pad5: f32,
    _pad6: f32,
    _pad7: f32,
    _pad8: f32,
};

@group(0) @binding(0) var<uniform> water: WaterUniform;
@group(1) @binding(0) var scene_color: texture_2d<f32>;
@group(1) @binding(1) var reflection_texture: texture_2d<f32>;
@group(1) @binding(2) var tex_sampler: sampler;

const PI: f32 = 3.14159265359;

fn gerstner_displacement(position: vec2<f32>, time: f32) -> vec3<f32> {
    var dir = normalize(water.wave_direction);
    if (length(dir) < 0.0001) {
        dir = vec2<f32>(1.0, 0.0);
    }
    let w = 2.0 * PI / max(water.wave_wavelength, 0.001);
    let phi = water.wave_speed * w * time;
    let q = water.wave_steepness;
    let theta = w * dot(dir, position) + phi;
    let c = cos(theta);
    let s = sin(theta);
    let amp = water.wave_amplitude;
    return vec3<f32>(
        q * amp * dir.x * c,
        amp * s,
        q * amp * dir.y * c
    );
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let base_pos = in.position;
    let displaced = base_pos + gerstner_displacement(base_pos.xz, water.time);
    let world_pos = vec3<f32>(displaced.x, displaced.y + water.level, displaced.z);
    out.clip_position = water.view_proj * vec4<f32>(world_pos, 1.0);
    out.world_pos = world_pos;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(scene_color, 0));
    let screen_uv = in.clip_position.xy / dims;

    // Reconstruct surface normal from position derivatives.
    let ddx = dpdx(in.world_pos);
    let ddy = dpdy(in.world_pos);
    let normal = normalize(cross(ddx, ddy));

    let view_dir = normalize(water.camera_pos.xyz - in.world_pos);
    let n_dot_v = max(dot(normal, view_dir), 0.0);
    let fresnel = pow(1.0 - n_dot_v, water.fresnel_power);

    // Refraction: sample the lit scene behind the water surface, distorted by the normal.
    let refract_uv = clamp(
        screen_uv + normal.xz * water.refraction_scale,
        vec2<f32>(0.0),
        vec2<f32>(1.0)
    );
    let refract_color = textureSample(scene_color, tex_sampler, refract_uv).rgb;
    let water_tint = mix(water.water_color, water.deep_color, 0.5).rgb;
    let refracted = refract_color * water_tint;

    // Reflection: sample SSR reflection texture, also distorted slightly.
    let reflect_uv = clamp(
        screen_uv + normal.xz * water.refraction_scale * 0.5,
        vec2<f32>(0.0),
        vec2<f32>(1.0)
    );
    let reflection = textureSample(reflection_texture, tex_sampler, reflect_uv).rgb;

    let final_color = mix(refracted, reflection, fresnel * water.reflectivity);
    let alpha = mix(0.6, 0.95, fresnel);

    return vec4<f32>(final_color, alpha);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Water Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Water Uniform Bind Group Layout"),
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

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Water Texture Bind Group Layout"),
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
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Water Pipeline Layout"),
            bind_group_layouts: &[
                Some(&uniform_bind_group_layout),
                Some(&texture_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Water Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Water Uniform Buffer"),
            size: std::mem::size_of::<WaterUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Water Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let mesh = Arc::new(GpuMesh::from_cpu(device, &create_water_plane(128, 256.0)));

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group: None,
            mesh,
            scene_color_handle: None,
            reflection_handle: None,
            depth_handle: None,
            water_color_handle: None,
            has_water: false,
            time: 0.0,
        }
    }

    fn read_water(world: &World) -> Option<Water> {
        world.query::<&Water>().iter().next().cloned()
    }
}

/// Create a subdivided XZ-plane CPU mesh centered at the origin.
fn create_water_plane(subdivisions: u32, extent: f32) -> CpuMesh {
    let segments = subdivisions.max(1);
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    for z in 0..=segments {
        for x in 0..=segments {
            let u = x as f32 / segments as f32;
            let v = z as f32 / segments as f32;
            let px = (u - 0.5) * extent;
            let pz = (v - 0.5) * extent;
            positions.push([px, 0.0, pz]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([u * 4.0, v * 4.0]);
        }
    }

    for z in 0..segments {
        for x in 0..segments {
            let i0 = z * (segments + 1) + x;
            let i1 = i0 + 1;
            let i2 = (z + 1) * (segments + 1) + x;
            let i3 = i2 + 1;
            indices.push(i0);
            indices.push(i2);
            indices.push(i1);
            indices.push(i1);
            indices.push(i2);
            indices.push(i3);
        }
    }

    CpuMesh {
        positions,
        normals,
        uvs,
        tangents: Vec::new(),
        indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::renderer::camera::FlyCamera;
    use crate::renderer::light::LightingUniforms;
    use crate::scene::WaterConfig;

    fn headless_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
    }

    #[test]
    fn water_pass_signature_reads_lit_scene_depth_and_reflection() {
        let (device, _queue) = headless_device();
        let pass = WaterPass::init(&device);
        let sig = pass.signature();
        assert_eq!(sig.name, "Water");
        assert!(sig.reads.iter().any(|s| s.name == "scene_color"));
        assert!(sig.reads.iter().any(|s| s.name == "gbuffer_depth"));
        assert!(sig.reads.iter().any(|s| s.name == "reflection"));
        assert_eq!(sig.writes.len(), 1);
        assert_eq!(sig.writes[0].name, "water_color");
    }

    #[test]
    fn water_pass_skipped_without_component() {
        let (device, queue) = headless_device();
        let pass = WaterPass::init(&device);
        let world = World::new();
        let camera = FlyCamera::default();
        let lighting = LightingUniforms::default();
        let frame = RenderFrame {
            batches: std::sync::Arc::from([]),
            camera: &camera,
            lighting: &lighting,
            queue: &queue,
            aspect: 1.0,
            delta_time: 0.016,
            world: &world,
        };
        assert!(!pass.should_run(&frame));
    }

    #[test]
    fn water_pass_runs_when_component_present() {
        let (device, queue) = headless_device();
        let mut pass = WaterPass::init(&device);
        let mut world = World::new();
        world.spawn((Water {
            config: WaterConfig::default(),
        },));
        let camera = FlyCamera::default();
        let lighting = LightingUniforms::default();
        let frame = RenderFrame {
            batches: std::sync::Arc::from([]),
            camera: &camera,
            lighting: &lighting,
            queue: &queue,
            aspect: 1.0,
            delta_time: 0.016,
            world: &world,
        };
        pass.apply_frame(&frame);
        assert!(pass.should_run(&frame));
    }
}
