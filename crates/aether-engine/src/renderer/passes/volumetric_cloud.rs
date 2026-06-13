//! Volumetric Cloud Pass — ray-marched cloud overlay.
//!
//! A full-screen pass that runs after the atmosphere pass and before water/
//! composite. It ray-marches through a horizontal cloud slab, samples a 3D
//! procedural noise texture for density, and writes the result as a separate
//! `CloudColor` overlay. The composite pass blends this overlay over the lit
//! scene.

use crate::ecs::components::Clouds;
use crate::ecs::World;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::{CloudColor, GDepth};
use crate::renderer::resource_table::ResourceTable;
use wgpu::util::DeviceExt;

/// Size of the generated 3D noise texture (NxNxN).
const NOISE_SIZE: u32 = 64;

/// GPU uniform data for the volumetric cloud shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CloudUniform {
    /// Inverse view-projection matrix for view-ray reconstruction.
    pub inv_view_proj: glam::Mat4,
    /// Camera world-space position (xyz, w unused).
    pub camera_pos: glam::Vec4,
    /// Direction toward the sun (xyz, w unused).
    pub sun_direction: glam::Vec4,
    /// Cloud slab bounds and density: x=min_y, y=max_y, z=coverage, w=density.
    pub cloud_bounds: glam::Vec4,
    /// Wind direction (xyz) and current time (w).
    pub wind_time: glam::Vec4,
}

impl Default for CloudUniform {
    fn default() -> Self {
        Self {
            inv_view_proj: glam::Mat4::IDENTITY,
            camera_pos: glam::Vec4::ZERO,
            sun_direction: glam::Vec4::new(0.0, 0.2, -1.0, 0.0),
            cloud_bounds: glam::Vec4::new(80.0, 120.0, 0.5, 1.0),
            wind_time: glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
        }
    }
}

/// Volumetric cloud render pass.
pub struct VolumetricCloudPass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group: Option<wgpu::BindGroup>,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    noise_texture: wgpu::Texture,
    noise_view: wgpu::TextureView,
    noise_sampler: wgpu::Sampler,
    noise_data: Vec<u8>,
    noise_uploaded: bool,
    depth_handle: Option<ResHandle<GDepth>>,
    cloud_color_handle: Option<ResHandle<CloudColor>>,
    has_clouds: bool,
    time: f32,
}

impl Pass for VolumetricCloudPass {
    fn name(&self) -> &str {
        "VolumetricCloud"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("VolumetricCloud")
            .read::<GDepth>("gbuffer_depth")
            .write::<CloudColor>("cloud_color", wgpu::TextureFormat::Rgba16Float)
    }

    fn init(device: &wgpu::Device) -> Self {
        Self::new_without_upload(device)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.depth_handle = Some(resources.handle::<GDepth>("gbuffer_depth"));
        self.cloud_color_handle = Some(resources.handle::<CloudColor>("cloud_color"));

        let depth_view = resources.get(self.depth_handle.unwrap());
        let _cloud_color_view = resources.get(self.cloud_color_handle.unwrap());

        // Create the texture bind group: depth + noise + sampler.
        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Volumetric Cloud Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.noise_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.noise_sampler),
                },
            ],
        }));
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_clouds
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        if let Some(clouds) = Self::read_clouds(frame.world) {
            self.has_clouds = true;
            self.time += frame.delta_time * clouds.config.wind_speed;

            let proj = frame.camera.projection_matrix(frame.aspect);
            let view = frame.camera.view_matrix();
            let inv_view_proj = (proj * view).inverse();

            let light_dir = glam::Vec3::from_array(frame.lighting.light.direction).normalize();
            let sun_toward = -light_dir;

            let cfg = &clouds.config;
            let uniforms = CloudUniform {
                inv_view_proj,
                camera_pos: glam::Vec4::from((frame.camera.position, 0.0)),
                sun_direction: glam::Vec4::from((sun_toward, 0.0)),
                cloud_bounds: glam::Vec4::new(
                    cfg.bottom_altitude,
                    cfg.top_altitude,
                    cfg.coverage,
                    cfg.density,
                ),
                wind_time: glam::Vec4::new(
                    cfg.wind_direction[0],
                    cfg.wind_direction[1],
                    0.0,
                    self.time,
                ),
            };

            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

            // Upload the noise texture the first time we have a valid queue.
            if !self.noise_uploaded {
                frame.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.noise_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &self.noise_data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(NOISE_SIZE),
                        rows_per_image: Some(NOISE_SIZE),
                    },
                    wgpu::Extent3d {
                        width: NOISE_SIZE,
                        height: NOISE_SIZE,
                        depth_or_array_layers: NOISE_SIZE,
                    },
                );
                self.noise_uploaded = true;
            }
        } else {
            self.has_clouds = false;
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        if !self.has_clouds {
            return;
        }

        let cloud_color_view = resources.get(self.cloud_color_handle.unwrap());
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("VolumetricCloudPass: resolve not called");

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Volumetric Cloud Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cloud_color_view,
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

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl VolumetricCloudPass {
    /// Create a new cloud pass, including a CPU-generated 3D noise texture.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut pass = Self::new_without_upload(device);
        // Immediately upload noise so the first frame is ready.
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &pass.noise_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pass.noise_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(NOISE_SIZE),
                rows_per_image: Some(NOISE_SIZE),
            },
            wgpu::Extent3d {
                width: NOISE_SIZE,
                height: NOISE_SIZE,
                depth_or_array_layers: NOISE_SIZE,
            },
        );
        pass.noise_uploaded = true;
        pass
    }

    fn new_without_upload(device: &wgpu::Device) -> Self {
        let output_format = wgpu::TextureFormat::Rgba16Float;
        let shader_source = r#"
struct CloudUniform {
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_direction: vec4<f32>,
    cloud_bounds: vec4<f32>,
    wind_time: vec4<f32>,
};

@group(0) @binding(0) var<uniform> clouds: CloudUniform;
@group(1) @binding(0) var depth_tex: texture_depth_2d;
@group(1) @binding(1) var noise_tex: texture_3d<f32>;
@group(1) @binding(2) var noise_sampler: sampler;

const PI: f32 = 3.14159265359;

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
    let world_h = clouds.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;

    let ray_dir = normalize(world_pos - clouds.camera_pos.xyz);
    let bounds = clouds.cloud_bounds;
    let min_y = bounds.x;
    let max_y = bounds.y;
    let coverage = bounds.z;
    let density_scale = bounds.w;

    if (abs(ray_dir.y) < 0.0001) {
        return vec4<f32>(0.0);
    }

    let t_min = (min_y - clouds.camera_pos.y) / ray_dir.y;
    let t_max = (max_y - clouds.camera_pos.y) / ray_dir.y;
    if (t_max < 0.0) {
        return vec4<f32>(0.0);
    }

    var t_enter = max(t_min, 0.0);
    var t_exit = t_max;
    if (t_enter > t_exit) {
        return vec4<f32>(0.0);
    }

    // Stop marching at the reconstructed geometry depth.
    let geo_dist = length(world_pos - clouds.camera_pos.xyz);
    t_exit = min(t_exit, geo_dist);
    if (t_enter >= t_exit) {
        return vec4<f32>(0.0);
    }

    let steps = 32.0;
    let dt = (t_exit - t_enter) / steps;
    let sun_dir = normalize(clouds.sun_direction.xyz);
    let wind = clouds.wind_time.xyz * clouds.wind_time.w;

    var transmittance = 1.0;
    var light_energy = 0.0;

    for (var i = 0.0; i < steps; i += 1.0) {
        let t = t_enter + (i + 0.5) * dt;
        let pos = clouds.camera_pos.xyz + ray_dir * t;
        let noise_uvw = pos * 0.005 + wind * 0.01;
        let n = textureSample(noise_tex, noise_sampler, noise_uvw).r;
        let density = max(n - (1.0 - coverage), 0.0) * density_scale;

        if (density > 0.0) {
            let extinction = density * 0.15;
            let sample_trans = exp(-extinction * dt);
            transmittance *= sample_trans;

            let light = max(dot(ray_dir, sun_dir), 0.0) * 0.5 + 0.5;
            light_energy += density * dt * transmittance * light;
        }
    }

    let alpha = 1.0 - transmittance;
    let cloud_color = vec3<f32>(1.0, 0.98, 0.95);
    return vec4<f32>(light_energy * cloud_color, alpha);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Volumetric Cloud Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cloud Uniform Buffer"),
            size: std::mem::size_of::<CloudUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Cloud Uniform Bind Group Layout"),
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
            label: Some("Cloud Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Cloud Texture Bind Group Layout"),
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
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D3,
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
            label: Some("Volumetric Cloud Pipeline Layout"),
            bind_group_layouts: &[
                Some(&uniform_bind_group_layout),
                Some(&texture_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Volumetric Cloud Pipeline"),
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
            label: Some("Cloud Quad Vtx"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let (noise_texture, noise_view) = {
            let _data = vec![0u8; (NOISE_SIZE * NOISE_SIZE * NOISE_SIZE) as usize];
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Cloud Noise Texture"),
                size: wgpu::Extent3d {
                    width: NOISE_SIZE,
                    height: NOISE_SIZE,
                    depth_or_array_layers: NOISE_SIZE,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D3,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };

        let noise_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Cloud Noise Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group_layout,
            texture_bind_group: None,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            noise_texture,
            noise_view,
            noise_sampler,
            noise_data: generate_cloud_noise_data(NOISE_SIZE),
            noise_uploaded: false,
            depth_handle: None,
            cloud_color_handle: None,
            has_clouds: false,
            time: 0.0,
        }
    }

    /// Read the first `Clouds` component from the world.
    fn read_clouds(world: &World) -> Option<Clouds> {
        world.query::<&Clouds>().iter().next().cloned()
    }
}

/// Generate cloud noise data without uploading to the GPU.
fn generate_cloud_noise_data(size: u32) -> Vec<u8> {
    let mut data = vec![0u8; (size * size * size) as usize];
    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let p = glam::Vec3::new(x as f32, y as f32, z as f32) / size as f32;
                let value = fbm_value_noise(p, 4, 2.0, 0.5);
                let idx = ((z * size * size) + (y * size) + x) as usize;
                data[idx] = (value.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }
    data
}

fn fbm_value_noise(p: glam::Vec3, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;
    for _ in 0..octaves {
        total += amplitude * trilinear_value_noise(p * frequency);
        max_value += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }
    total / max_value
}

fn trilinear_value_noise(p: glam::Vec3) -> f32 {
    let i = p.floor().as_ivec3();
    let f = p.fract();
    let u = f * f * (3.0 - 2.0 * f);
    let mut value = 0.0;
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let corner = i + glam::IVec3::new(x, y, z);
                let hash = lattice_hash(corner);
                let wx = if x == 0 { 1.0 - u.x } else { u.x };
                let wy = if y == 0 { 1.0 - u.y } else { u.y };
                let wz = if z == 0 { 1.0 - u.z } else { u.z };
                value += hash * wx * wy * wz;
            }
        }
    }
    value
}

fn lattice_hash(p: glam::IVec3) -> f32 {
    let mut n =
        p.x.wrapping_mul(374761393) ^ p.y.wrapping_mul(668265263) ^ p.z.wrapping_mul(2086444801);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n = n ^ (n >> 16);
    (n as f32 / u32::MAX as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless_device_queue() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
    }

    #[test]
    fn cloud_pass_signature_reads_depth_and_writes_cloud_color() {
        let (device, queue) = headless_device_queue();
        let sig = VolumetricCloudPass::new(&device, &queue).signature();
        assert_eq!(sig.name, "VolumetricCloud");
        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.writes.len(), 1);
    }

    #[test]
    fn cloud_noise_texture_has_expected_dimensions() {
        let (device, queue) = headless_device_queue();
        let pass = VolumetricCloudPass::new(&device, &queue);
        assert_eq!(pass.noise_texture.width(), NOISE_SIZE);
        assert_eq!(pass.noise_texture.height(), NOISE_SIZE);
        assert_eq!(pass.noise_texture.depth_or_array_layers(), NOISE_SIZE);
        assert_eq!(pass.noise_texture.format(), wgpu::TextureFormat::R8Unorm);
    }

    #[test]
    fn cloud_pass_runs_when_cloud_component_present() {
        let (device, queue) = headless_device_queue();
        let pass = VolumetricCloudPass::new(&device, &queue);
        assert!(pass
            .signature()
            .writes
            .iter()
            .any(|s| s.name == "cloud_color"));
        assert!(pass
            .signature()
            .reads
            .iter()
            .any(|s| s.name == "gbuffer_depth"));
    }

    #[test]
    fn cloud_pass_skipped_without_component() {
        let (device, queue) = headless_device_queue();
        let pass = VolumetricCloudPass::new(&device, &queue);
        // Without apply_frame being called, has_clouds remains false.
        assert!(!pass.has_clouds);
    }
}
