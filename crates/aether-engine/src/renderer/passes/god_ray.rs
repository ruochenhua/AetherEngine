//! God Ray Pass — volumetric light shafts.
//!
//! A full-screen pass that runs after SSR and before water/composite. It ray
//! marches from each screen pixel toward the sun's screen-space position,
//! samples the G-Buffer depth to detect occlusion, and writes a separate
//! `GodRayColor` overlay. The composite pass adds this overlay on top of the
//! lit scene.

use crate::renderer::frame::RenderFrame;
use crate::renderer::light::sun_direction_from_lighting;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::{GDepth, GodRayColor};
use crate::renderer::resource_table::ResourceTable;
use wgpu::util::DeviceExt;

/// GPU uniform data for the god ray shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GodRayUniform {
    /// View-projection matrix.
    pub view_proj: glam::Mat4,
    /// Inverse view-projection matrix.
    pub inv_view_proj: glam::Mat4,
    /// Camera world-space position (xyz, w unused).
    pub camera_pos: glam::Vec4,
    /// Direction toward the sun (xyz, w unused).
    pub sun_direction: glam::Vec4,
    /// x=samples, y=density, z=decay, w=weight.
    pub params: glam::Vec4,
    /// Final exposure multiplier.
    pub exposure: f32,
    /// Padding to 16-byte alignment.
    pub _pad: [f32; 3],
}

impl Default for GodRayUniform {
    fn default() -> Self {
        Self {
            view_proj: glam::Mat4::IDENTITY,
            inv_view_proj: glam::Mat4::IDENTITY,
            camera_pos: glam::Vec4::ZERO,
            sun_direction: glam::Vec4::new(0.0, 0.2, -1.0, 0.0),
            params: glam::Vec4::new(64.0, 0.5, 0.95, 0.5),
            exposure: 0.3,
            _pad: [0.0; 3],
        }
    }
}

/// God ray render pass.
pub struct GodRayPass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group: Option<wgpu::BindGroup>,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    depth_handle: Option<ResHandle<GDepth>>,
    god_ray_color_handle: Option<ResHandle<GodRayColor>>,
    has_god_ray: bool,
}

impl Pass for GodRayPass {
    fn name(&self) -> &str {
        "GodRay"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("GodRay")
            .read::<GDepth>()
            .write::<GodRayColor>(wgpu::TextureFormat::Rgba16Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.depth_handle = Some(resources.handle::<GDepth>());
        self.god_ray_color_handle = Some(resources.handle::<GodRayColor>());

        let depth_view = resources.get(self.depth_handle.unwrap());

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("God Ray Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(depth_view),
            }],
        }));
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_god_ray
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.has_god_ray = false;
        if let Some(god_ray) = frame.optional.god_ray.clone() {
            self.has_god_ray = true;
            let proj = frame.camera.projection_matrix(frame.aspect);
            let view = frame.camera.view_matrix();
            let view_proj = proj * view;
            let inv_view_proj = view_proj.inverse();

            let sun_toward = sun_direction_from_lighting(frame.lighting);

            let cfg = &god_ray.config;
            let uniforms = GodRayUniform {
                view_proj,
                inv_view_proj,
                camera_pos: glam::Vec4::from((frame.camera.position, 0.0)),
                sun_direction: glam::Vec4::from((sun_toward, 0.0)),
                params: glam::Vec4::new(cfg.samples as f32, cfg.density, cfg.decay, cfg.weight),
                exposure: cfg.exposure,
                _pad: [0.0; 3],
            };

            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        if !self.has_god_ray {
            return;
        }

        let god_ray_color_view = resources.get(self.god_ray_color_handle.unwrap());
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("GodRayPass: resolve not called");

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("God Ray Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: god_ray_color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_bind_group(1, texture_bg, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }
}

impl GodRayPass {
    /// Create a new god ray pass.
    pub fn new(device: &wgpu::Device) -> Self {
        let output_format = wgpu::TextureFormat::Rgba16Float;
        let shader_source = r#"
struct GodRayUniform {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_direction: vec4<f32>,
    params: vec4<f32>,
    exposure: f32,
};

@group(0) @binding(0) var<uniform> ray: GodRayUniform;
@group(1) @binding(0) var depth_tex: texture_depth_2d;

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(depth_tex, 0));
    let uv = frag_coord.xy / dims;
    let coord = vec2<i32>(frag_coord.xy);

    let depth = textureLoad(depth_tex, coord, 0);
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world_h = ray.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;

    // Compute screen-space sun position.
    let sun_world = ray.camera_pos.xyz + ray.sun_direction.xyz * 1000.0;
    let sun_clip = ray.view_proj * vec4<f32>(sun_world, 1.0);
    let sun_ndc = sun_clip.xyz / sun_clip.w;
    let sun_uv = vec2<f32>(sun_ndc.x * 0.5 + 0.5, 0.5 - sun_ndc.y * 0.5);
    let sun_screen = sun_uv * dims;

    let delta = sun_screen - frag_coord.xy;
    let ray_dir = delta / dims;

    let samples = u32(ray.params.x);
    let density = ray.params.y;
    let decay_rate = ray.params.z;
    let weight = ray.params.w;
    let exposure = ray.exposure;

    var illumination = 0.0;
    var decay = 1.0;
    let step_size = 1.0 / f32(samples);

    for (var i = 0u; i < samples; i = i + 1u) {
        let t = f32(i) * step_size * density;
        let sample_uv = uv + ray_dir * t;
        let sample_coord = clamp(vec2<i32>(sample_uv * dims), vec2<i32>(0), vec2<i32>(dims) - vec2<i32>(1));
        let sample_depth = textureLoad(depth_tex, sample_coord, 0);

        // Treat far-plane (sky) samples as lit; geometry samples are occluders.
        if (sample_depth > 0.9999) {
            illumination += decay * weight;
        }
        decay *= decay_rate;
    }

    let intensity = illumination * exposure;
    let color = vec3<f32>(1.0, 0.95, 0.8) * intensity;
    return vec4<f32>(color, intensity);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("God Ray Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("God Ray Uniform Buffer"),
            size: std::mem::size_of::<GodRayUniform>() as wgpu::BufferAddress,
            usage: {
                let usage = wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST;
                #[cfg(test)]
                let usage = usage | wgpu::BufferUsages::COPY_SRC;
                usage
            },
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("God Ray Uniform Bind Group Layout"),
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

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("God Ray Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("God Ray Texture Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("God Ray Pipeline Layout"),
            bind_group_layouts: &[
                Some(&uniform_bind_group_layout),
                Some(&texture_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("God Ray Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
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
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
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
            multiview_mask: None,
            cache: None,
        });

        let quad_vertices: [[f32; 2]; 6] = [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ];
        let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("God Ray Quad Vtx"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group_layout,
            texture_bind_group: None,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            depth_handle: None,
            god_ray_color_handle: None,
            has_god_ray: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::GodRay;
    use crate::ecs::World;
    use crate::renderer::extract::extract_optional_pass_data;
    use crate::renderer::frame::FrameConfig;
    use crate::renderer::light::{sun_direction_from_lighting, LightingUniforms};
    use glam::Vec3;

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
            .0
    }

    #[test]
    fn god_ray_pass_signature_reads_depth_and_writes_overlay() {
        let device = headless_device();
        let sig = GodRayPass::new(&device).signature();
        assert_eq!(sig.name, "GodRay");
        assert!(sig.reads.iter().any(|s| s.name == "gbuffer_depth"));
        assert!(sig.writes.iter().any(|s| s.name == "god_ray_color"));
    }

    #[test]
    fn god_ray_pass_skipped_without_component() {
        let device = headless_device();
        let pass = GodRayPass::new(&device);
        assert!(!pass.has_god_ray);
    }

    #[test]
    fn god_ray_uniform_default_is_aligned() {
        let _ = GodRayUniform::default();
        assert_eq!(std::mem::size_of::<GodRayUniform>() % 16, 0);
    }

    /// Verifies that the sun direction written to the GodRay uniform buffer
    /// matches the shared `sun_direction_from_lighting` helper.
    #[test]
    fn god_ray_pass_uses_light_direction_for_sun() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("need device");

        let mut pass = GodRayPass::new(&device);
        let mut world = World::new();
        world.spawn((GodRay {
            config: crate::scene::GodRayConfig {
                samples: 16,
                density: 0.6,
                decay: 0.9,
                weight: 0.3,
                exposure: 0.2,
            },
        },));
        let optional = extract_optional_pass_data(&world);

        let camera = crate::renderer::camera::FlyCamera::default();
        let lighting = LightingUniforms::default();
        let assets = crate::asset::AssetManager::new();
        let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue,
        );
        let frame = crate::renderer::frame::RenderFrame {
            batches: std::sync::Arc::from([]),
            camera: &camera,
            lighting: &lighting,
            queue: &queue,
            aspect: 1.0,
            delta_time: 0.016,
            config: &FrameConfig::default(),
            optional: &optional,
            terrain_geometry: None,
            texture_cache: &texture_cache,
            asset_manager: &assets,
        };

        pass.apply_frame(&frame);
        assert!(pass.has_god_ray);

        let uniform_size = std::mem::size_of::<GodRayUniform>() as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GodRay Sun Direction Readback"),
            size: uniform_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GodRay Sun Direction Copy"),
        });
        encoder.copy_buffer_to_buffer(
            &pass.uniform_buffer,
            0,
            &staging,
            0,
            uniform_size,
        );
        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| {
            result.expect("failed to map uniform readback buffer");
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        let data = slice.get_mapped_range();
        let uniforms: &[GodRayUniform] = bytemuck::cast_slice(&data);
        let expected_sun = sun_direction_from_lighting(&lighting);
        let actual_sun = Vec3::new(
            uniforms[0].sun_direction.x,
            uniforms[0].sun_direction.y,
            uniforms[0].sun_direction.z,
        );
        assert!(
            actual_sun.abs_diff_eq(expected_sun, 1e-4),
            "expected sun_direction {expected_sun:?}, got {actual_sun:?}"
        );
    }

    /// Headless render-to-texture test that verifies the god ray pass produces
    /// a visible light-shaft overlay and saves the result as a PNG for visual
    /// inspection.
    #[test]
    fn god_ray_pass_renders_overlay_to_texture() {
        use std::any::TypeId;
        use std::borrow::Cow;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("need device");

        let width = 128u32;
        let height = 128u32;

        // Depth texture: far plane (sky) everywhere so the ray march accumulates light.
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GodRay Test Depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Clear the depth texture to the far plane (sky) so the ray march accumulates light.
        let mut clear_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GodRay Test Clear Depth"),
        });
        clear_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GodRay Test Clear Depth Pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
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
        queue.submit(std::iter::once(clear_encoder.finish()));

        // Draw a small centered occluder so the god ray test image shows visible shafts.
        let occluder_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GodRay Test Occluder Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                r#"
@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.5, 1.0);
}
"#,
            )),
        });
        let occluder_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GodRay Test Occluder Layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let occluder_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GodRay Test Occluder Pipeline"),
            layout: Some(&occluder_layout),
            vertex: wgpu::VertexState {
                module: &occluder_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
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
            fragment: None,
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let occluder_vertices: [[f32; 2]; 6] = [
            [-0.25, -0.25],
            [0.25, -0.25],
            [0.25, 0.25],
            [-0.25, -0.25],
            [0.25, 0.25],
            [-0.25, 0.25],
        ];
        let occluder_vbo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GodRay Test Occluder Vtx"),
            contents: bytemuck::cast_slice(&occluder_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut occ_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GodRay Test Occluder"),
        });
        {
            let mut pass = occ_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("GodRay Test Occluder Pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
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
            pass.set_pipeline(&occluder_pipeline);
            pass.set_vertex_buffer(0, occluder_vbo.slice(..));
            pass.draw(0..6, 0..1);
        }
        queue.submit(std::iter::once(occ_encoder.finish()));

        // God ray output texture.
        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GodRay Test Output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut resources = ResourceTable::new();
        resources.allocate_with_texture(
            TypeId::of::<GDepth>(),
            "gbuffer_depth",
            depth_texture,
            depth_view,
        );
        resources.allocate_with_texture(
            TypeId::of::<GodRayColor>(),
            "god_ray_color",
            output_texture,
            output_view,
        );

        let mut pass = GodRayPass::new(&device);
        pass.resolve(&device, &resources);

        // World with a directional light and a god-ray component.
        let mut world = World::new();
        world.spawn((
            crate::ecs::components::Transform {
                translation: glam::Vec3::new(0.0, 10.0, 0.0),
                rotation: glam::Quat::from_euler(
                    glam::EulerRot::YXZ,
                    0.0,
                    std::f32::consts::FRAC_PI_4,
                    0.0,
                ),
                scale: glam::Vec3::ONE,
            },
            crate::ecs::components::Light {
                light_type: crate::renderer::light::LightType::Directional,
                color: [1.0, 0.95, 0.8],
                intensity: 1.0,
                cast_shadow: false,
            },
        ));
        world.spawn((GodRay {
            config: crate::scene::GodRayConfig {
                samples: 16,
                density: 0.6,
                decay: 0.9,
                weight: 0.3,
                exposure: 0.2,
            },
        },));

        let camera = crate::renderer::camera::FlyCamera {
            position: glam::Vec3::new(0.0, 1.0, 5.0),
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
            fov: 45.0f32.to_radians(),
            near: 0.1,
            far: 100.0,
            speed: 1.0,
            base_speed: 1.0,
            min_speed: 0.1,
            max_speed: 10.0,
            sensitivity: 0.001,
            active: false,
        };
        let lighting = crate::renderer::light::LightingUniforms::default();

        let optional = extract_optional_pass_data(&world);
        let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
        let asset_manager = crate::asset::AssetManager::new();
        let frame = RenderFrame {
            batches: std::sync::Arc::from([]),
            camera: &camera,
            lighting: &lighting,
            queue: &queue,
            aspect: width as f32 / height as f32,
            delta_time: 0.016,
            config: &FrameConfig::default(),
            optional: &optional,
            terrain_geometry: None,
            texture_cache: &texture_cache,
            asset_manager: &asset_manager,
        };
        pass.apply_frame(&frame);
        assert!(pass.has_god_ray);

        let output_view_ref = resources.get(pass.god_ray_color_handle.unwrap());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GodRay Test"),
        });
        pass.execute(&mut encoder, &resources, output_view_ref);
        queue.submit(std::iter::once(encoder.finish()));

        // Read back the output texture.
        let bytes_per_row = (width * 8).div_ceil(256) * 256;
        let buffer_size = bytes_per_row as u64 * height as u64;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GodRay Test Readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GodRay Test Copy"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: resources
                    .texture(pass.god_ray_color_handle.unwrap())
                    .unwrap(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));

        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

        let data = readback.slice(..).get_mapped_range();
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            let row_start = (y * bytes_per_row) as usize;
            for x in 0..width {
                let idx = row_start + (x * 8) as usize;
                let r = half::f16::from_ne_bytes([data[idx], data[idx + 1]]).to_f32();
                let g = half::f16::from_ne_bytes([data[idx + 2], data[idx + 3]]).to_f32();
                let b = half::f16::from_ne_bytes([data[idx + 4], data[idx + 5]]).to_f32();
                let a = half::f16::from_ne_bytes([data[idx + 6], data[idx + 7]]).to_f32();
                rgba.push((r.clamp(0.0, 1.0) * 255.0) as u8);
                rgba.push((g.clamp(0.0, 1.0) * 255.0) as u8);
                rgba.push((b.clamp(0.0, 1.0) * 255.0) as u8);
                rgba.push((a.clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
        drop(data);
        readback.unmap();

        let img = image::RgbaImage::from_raw(width, height, rgba).expect("valid image buffer");
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let out_path = workspace_root.join("tests/output/14_god_rays_unit.png");
        std::fs::create_dir_all(out_path.parent().unwrap()).ok();
        img.save(&out_path).expect("save screenshot");

        // Sanity check: the overlay is not entirely transparent.
        let avg_alpha: f32 =
            img.pixels().map(|p| p[3] as f32).sum::<f32>() / (width * height) as f32;
        assert!(
            avg_alpha > 1.0,
            "god ray overlay should be visible (avg alpha {:.2})",
            avg_alpha
        );
    }
}
