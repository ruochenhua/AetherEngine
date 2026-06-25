//! Atmosphere Pass — physically based sky scattering.
//!
//! A full-screen pass that runs after deferred lighting and replaces the
//! environment-map sky background with Rayleigh + Mie scattering. Geometry
//! pixels (identified by G-Buffer position alpha > 0) are preserved from the
//! input `SceneColor`.

use crate::math::Vec3;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::{GDepth, SceneColor};
use crate::renderer::resource_table::ResourceTable;
use wgpu::util::DeviceExt;

/// GPU uniform data for the atmosphere shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AtmosphereUniform {
    /// Direction toward the sun (world space).
    pub sun_direction: [f32; 3],
    /// Padding to 16-byte alignment.
    pub _pad0: f32,
    /// Camera world-space position.
    pub camera_pos: [f32; 3],
    /// Padding to 16-byte alignment.
    pub _pad1: f32,
    /// Planet radius in world units.
    pub planet_radius: f32,
    /// Atmosphere shell thickness.
    pub atmosphere_height: f32,
    /// Rayleigh density scale height.
    pub rayleigh_scale_height: f32,
    /// Mie density scale height.
    pub mie_scale_height: f32,
    /// Rayleigh scattering coefficients (RGB).
    pub rayleigh_scattering: [f32; 3],
    /// Padding to 16-byte alignment.
    pub _pad2: f32,
    /// Mie scattering coefficients (RGB).
    pub mie_scattering: [f32; 3],
    /// Padding to 16-byte alignment.
    pub _pad3: f32,
    /// Sun intensity multiplier.
    pub sun_intensity: f32,
    /// Mie asymmetry parameter (g).
    pub mie_asymmetry: f32,
    /// Padding to 16-byte alignment.
    pub _pad4: f32,
    /// Padding to 16-byte alignment.
    pub _pad5: f32,
    /// Inverse view-projection matrix for view-ray reconstruction.
    pub inv_view_proj: [[f32; 4]; 4],
}

impl Default for AtmosphereUniform {
    fn default() -> Self {
        Self {
            sun_direction: [0.0, 0.2, -1.0],
            _pad0: 0.0,
            camera_pos: [0.0; 3],
            _pad1: 0.0,
            planet_radius: 6360.0,
            atmosphere_height: 100.0,
            rayleigh_scale_height: 8.0,
            mie_scale_height: 1.2,
            rayleigh_scattering: [0.005802, 0.013558, 0.033100],
            _pad2: 0.0,
            mie_scattering: [0.004000, 0.004000, 0.004000],
            _pad3: 0.0,
            sun_intensity: 20.0,
            mie_asymmetry: 0.758,
            _pad4: 0.0,
            _pad5: 0.0,
            inv_view_proj: [[0.0; 4]; 4],
        }
    }
}

/// Atmosphere render pass.
pub struct AtmospherePass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group: Option<wgpu::BindGroup>,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    scene_color_handle: Option<ResHandle<SceneColor>>,
    depth_handle: Option<ResHandle<GDepth>>,
    has_atmosphere: bool,
}

impl Pass for AtmospherePass {
    fn name(&self) -> &str {
        "Atmosphere"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Atmosphere")
            .read::<GDepth>()
            .write::<SceneColor>(wgpu::TextureFormat::Rgba16Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.scene_color_handle = Some(resources.handle::<SceneColor>());
        self.depth_handle = Some(resources.handle::<GDepth>());

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Atmosphere Texture Bind Group Layout"),
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

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atmosphere Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    resources.get(self.depth_handle.unwrap()),
                ),
            }],
        }));
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.has_atmosphere
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        if let Some(atmos) = frame.optional.atmosphere.clone() {
            self.has_atmosphere = true;
            let sun_dir = Vec3::from_array(frame.lighting.light.direction).normalize();
            // `light.direction` points FROM the light; the sun is in the opposite direction.
            let sun_toward = -sun_dir;

            let proj = frame.camera.projection_matrix(frame.aspect);
            let view = frame.camera.view_matrix();
            let inv_view_proj = (proj * view).inverse();

            let uniforms = AtmosphereUniform {
                sun_direction: sun_toward.into(),
                camera_pos: frame.camera.position.into(),
                planet_radius: atmos.config.planet_radius,
                atmosphere_height: atmos.config.atmosphere_height,
                rayleigh_scale_height: atmos.config.rayleigh_scale_height,
                mie_scale_height: atmos.config.mie_scale_height,
                rayleigh_scattering: atmos.config.rayleigh_scattering,
                mie_scattering: atmos.config.mie_scattering,
                sun_intensity: atmos.config.sun_intensity,
                mie_asymmetry: atmos.config.mie_asymmetry,
                inv_view_proj: inv_view_proj.to_cols_array_2d(),
                ..Default::default()
            };
            frame
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        } else {
            self.has_atmosphere = false;
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        if !self.has_atmosphere {
            return;
        }

        let scene_color_view = resources.get(self.scene_color_handle.unwrap());
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("AtmospherePass: resolve not called");

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Atmosphere Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene_color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
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

impl AtmospherePass {
    /// Create a new atmosphere pass.
    pub fn new(device: &wgpu::Device) -> Self {
        let output_format = wgpu::TextureFormat::Rgba16Float;
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

struct AtmosphereUniform {
    sun_direction: vec3<f32>,
    _pad0: f32,
    camera_pos: vec3<f32>,
    _pad1: f32,
    planet_radius: f32,
    atmosphere_height: f32,
    rayleigh_scale_height: f32,
    mie_scale_height: f32,
    rayleigh_scattering: vec3<f32>,
    _pad2: f32,
    mie_scattering: vec3<f32>,
    _pad3: f32,
    sun_intensity: f32,
    mie_asymmetry: f32,
    _pad4: f32,
    _pad5: f32,
    inv_view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> atmos: AtmosphereUniform;
@group(1) @binding(0) var gbuffer_depth: texture_depth_2d;

const PI: f32 = 3.14159265359;

fn ray_sphere_intersect(r0: vec3<f32>, rd: vec3<f32>, center: vec3<f32>, radius: f32) -> vec2<f32> {
    let oc = r0 - center;
    let b = dot(oc, rd);
    let c = dot(oc, oc) - radius * radius;
    let d = b * b - c;
    if (d < 0.0) {
        return vec2<f32>(1e10, -1e10);
    }
    let sd = sqrt(d);
    return vec2<f32>(-b - sd, -b + sd);
}

fn rayleigh_phase(cos_theta: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + cos_theta * cos_theta);
}

fn mie_phase(cos_theta: f32, g: f32) -> f32 {
    let gg = g * g;
    let num = (1.0 - gg) * (1.0 + cos_theta * cos_theta);
    let denom = (2.0 + gg) * pow(1.0 + gg - 2.0 * g * cos_theta, 1.5);
    return num / denom;
}

fn atmosphere_color(ray_origin: vec3<f32>, ray_dir: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    let planet_center = vec3<f32>(0.0, -atmos.planet_radius, 0.0);
    let atmo_radius = atmos.planet_radius + atmos.atmosphere_height;

    let atmo_hit = ray_sphere_intersect(ray_origin, ray_dir, planet_center, atmo_radius);
    var t0 = max(atmo_hit.x, 0.0);
    var t1 = atmo_hit.y;
    if (t1 <= t0) {
        return vec3<f32>(0.0);
    }

    let planet_hit = ray_sphere_intersect(ray_origin, ray_dir, planet_center, atmos.planet_radius);
    if (planet_hit.x > 0.0) {
        t1 = min(t1, planet_hit.x);
    }

    let ray_length = t1 - t0;
    if (ray_length <= 0.0) {
        return vec3<f32>(0.0);
    }

    let sample_count = 16.0;
    let step_size = ray_length / sample_count;
    var sample_point = ray_origin + ray_dir * (t0 + step_size * 0.5);

    var optical_depth_r: f32 = 0.0;
    var optical_depth_m: f32 = 0.0;
    var total_r: vec3<f32> = vec3<f32>(0.0);
    var total_m: vec3<f32> = vec3<f32>(0.0);

    for (var i: f32 = 0.0; i < sample_count; i = i + 1.0) {
        let h = length(sample_point - planet_center) - atmos.planet_radius;
        let density_r = exp(-h / atmos.rayleigh_scale_height) * step_size;
        let density_m = exp(-h / atmos.mie_scale_height) * step_size;

        optical_depth_r += density_r;
        optical_depth_m += density_m;

        // Sun ray optical depth from sample point to top of atmosphere.
        let sun_atmo = ray_sphere_intersect(sample_point, sun_dir, planet_center, atmo_radius);
        let sun_step = sun_atmo.y;
        let sun_step_size = sun_step / 8.0;
        var sun_sample = sample_point + sun_dir * (sun_step_size * 0.5);
        var sun_od_r: f32 = 0.0;
        var sun_od_m: f32 = 0.0;
        for (var j: f32 = 0.0; j < 8.0; j = j + 1.0) {
            let sun_h = length(sun_sample - planet_center) - atmos.planet_radius;
            sun_od_r += exp(-sun_h / atmos.rayleigh_scale_height) * sun_step_size;
            sun_od_m += exp(-sun_h / atmos.mie_scale_height) * sun_step_size;
            sun_sample += sun_dir * sun_step_size;
        }

        let extinction = exp(-(
            atmos.rayleigh_scattering * (optical_depth_r + sun_od_r) +
            atmos.mie_scattering * (optical_depth_m + sun_od_m)
        ));

        total_r += density_r * extinction;
        total_m += density_m * extinction;

        sample_point += ray_dir * step_size;
    }

    let mu = dot(ray_dir, sun_dir);
    let phase_r = rayleigh_phase(mu);
    let phase_m = mie_phase(mu, atmos.mie_asymmetry);

    return atmos.sun_intensity * (
        atmos.rayleigh_scattering * total_r * phase_r +
        atmos.mie_scattering * total_m * phase_m
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Discard geometry pixels so the existing SceneColor (from LightingPass) is preserved.
    // The GBuffer depth was cleared to 1.0 for sky and written to < 1.0 for geometry.
    let dims = vec2<f32>(textureDimensions(gbuffer_depth, 0));
    let coord = vec2<i32>(in.uv * dims);
    let depth = textureLoad(gbuffer_depth, coord, 0);
    if (depth < 0.9999) {
        discard;
    }

    let clip = vec4<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, 0.0, 1.0);
    let world_ray = atmos.inv_view_proj * clip;
    let world_pos = world_ray.xyz / world_ray.w;
    let ray_dir = normalize(world_pos - atmos.camera_pos);
    let sun_dir = normalize(atmos.sun_direction);

    var color = atmosphere_color(atmos.camera_pos, ray_dir, sun_dir);

    // Bright sun disc.
    let cos_sun = dot(ray_dir, sun_dir);
    if (cos_sun > 0.9998) {
        color += vec3<f32>(atmos.sun_intensity * 5.0);
    }

    return vec4<f32>(color, 1.0);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Atmosphere Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Atmosphere Uniform Bind Group Layout"),
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

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Atmosphere Texture Bind Group Layout"),
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
            label: Some("Atmosphere Pipeline Layout"),
            bind_group_layouts: &[
                Some(&uniform_bind_group_layout),
                Some(&texture_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Atmosphere Pipeline"),
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

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Atmosphere Uniform Buffer"),
            size: std::mem::size_of::<AtmosphereUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atmosphere Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
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
            label: Some("Atmosphere Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group: None,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            scene_color_handle: None,
            depth_handle: None,
            has_atmosphere: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::Atmosphere;
    use crate::ecs::World;
    use crate::renderer::camera::FlyCamera;
    use crate::renderer::extract::extract_optional_pass_data;
    use crate::renderer::frame::FrameConfig;
    use crate::renderer::light::LightingUniforms;
    use crate::renderer::resource::ResourceTag;
    use crate::scene::AtmosphereConfig;

    fn headless_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
    }

    fn init_ctx<'a>(device: &'a wgpu::Device, queue: &'a wgpu::Queue) -> InitContext<'a> {
        InitContext {
            device,
            queue,
            surface_format: wgpu::TextureFormat::Bgra8UnormSrgb,
            depth_format: wgpu::TextureFormat::Depth32Float,
            width: 64,
            height: 64,
            ibl_resources: None,
        }
    }

    #[test]
    fn atmosphere_pass_signature_reads_depth_and_writes_scene_color() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let pass = AtmospherePass::init(&ctx);
        let sig = pass.signature();
        assert_eq!(sig.name, "Atmosphere");
        assert!(sig.reads.iter().any(|s| s.name == GDepth::NAME));
        assert_eq!(sig.writes.len(), 1);
        assert_eq!(sig.writes[0].name, SceneColor::NAME);
    }

    #[test]
    fn atmosphere_pass_skipped_without_component() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let pass = AtmospherePass::init(&ctx);
        let world = World::new();
        let optional = extract_optional_pass_data(&world);
        let camera = FlyCamera::default();
        let lighting = LightingUniforms::default();
        let frame = RenderFrame {
            batches: std::sync::Arc::from([]),
            camera: &camera,
            lighting: &lighting,
            queue: &queue,
            aspect: 1.0,
            delta_time: 0.016,
            config: &FrameConfig::default(),
            optional: &optional,
        };
        assert!(!pass.should_run(&frame));
    }

    #[test]
    fn atmosphere_pass_runs_when_component_present() {
        let (device, queue) = headless_device();
        let ctx = init_ctx(&device, &queue);
        let mut pass = AtmospherePass::init(&ctx);
        let mut world = World::new();
        world.spawn((Atmosphere {
            config: AtmosphereConfig::default(),
        },));
        let optional = extract_optional_pass_data(&world);
        let camera = FlyCamera::default();
        let lighting = LightingUniforms::default();
        let frame = RenderFrame {
            batches: std::sync::Arc::from([]),
            camera: &camera,
            lighting: &lighting,
            queue: &queue,
            aspect: 1.0,
            delta_time: 0.016,
            config: &FrameConfig::default(),
            optional: &optional,
        };
        pass.apply_frame(&frame);
        assert!(pass.should_run(&frame));
    }
}
