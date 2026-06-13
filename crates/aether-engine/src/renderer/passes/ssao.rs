//! SSAO Pass — Screen-Space Ambient Occlusion
//!
//! Full-screen quad pass. Follows the LearnOpenGL SSAO approach:
//! - All math in VIEW space (world pos → view pos via view_mat)
//! - Hemisphere samples in tangent space → view-space offsets via view-space TBN
//! - View-space Z depth comparison with bias
//! - Range falloff in view-space units (params.radius is view-space radius)
//! - 16-sample kernel (const array + for loop)
//! - Hash-based per-pixel rotation
//! - No blur pass (planned as separate AOBlurPass)
//!
//! Pipeline: GBufferPass → SSAOPass (→AOTexture) → LightingPass

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

/// SSAO parameters (matches WGSL std140 layout).
/// `radius` and `bias` are in world-space units. Because the view transform is
/// rigid, these map 1:1 to view-space lengths used during hemisphere sampling.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SSAOParams {
    radius: f32,
    bias: f32,
    intensity: f32,
    _pad0: f32,
    screen_size: [f32; 2],
    _pad1: [f32; 2],
}

/// Per-frame uniform data (SSAO params + projection + view), matches WGSL std140.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SSAOFrameUniforms {
    params: SSAOParams,  // offset 0,   32 bytes
    proj: [[f32; 4]; 4], // offset 32,  64 bytes (mat4x4 = 4 × vec4)
    view: [[f32; 4]; 4], // offset 96,  64 bytes
}
// Total: 160 bytes, aligned to 16

/// SSAO pass state.
pub struct SSAOPass {
    pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    frame_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    texture_bind_group: Option<wgpu::BindGroup>,
    #[allow(dead_code)]
    texture_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    frame_bind_group_layout: wgpu::BindGroupLayout,
    // Per-frame
    proj: glam::Mat4,
    view: glam::Mat4,
    screen_size: [f32; 2],
    half_width: u32,
    half_height: u32,
    // SSAO parameters (mutable via UI)
    radius: f32,
    bias: f32,
    intensity: f32,
}

impl Pass for SSAOPass {
    fn name(&self) -> &str {
        "SSAO"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("SSAO")
            .read::<GPosition>("gbuffer_position")
            .read::<GNormal>("gbuffer_normal")
            .write_sized::<AOTexture>(
                "ao",
                wgpu::TextureFormat::R8Unorm,
                self.half_width.max(1),
                self.half_height.max(1),
            )
    }

    fn init(device: &wgpu::Device) -> Self {
        Self::new(device)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.pos_handle = Some(resources.handle::<GPosition>("gbuffer_position"));
        self.normal_handle = Some(resources.handle::<GNormal>("gbuffer_normal"));

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SSAO GBuffer Sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let pos_view = resources.get(self.pos_handle.unwrap());
        let norm_view = resources.get(self.normal_handle.unwrap());

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO Texture Bind Group"),
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
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        }));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        let proj = frame.camera.projection_matrix(frame.aspect);
        let view = frame.camera.view_matrix();
        self.view = view;
        self.proj = proj;

        let uniforms = SSAOFrameUniforms {
            params: SSAOParams {
                radius: self.radius,
                bias: self.bias,
                intensity: self.intensity,
                _pad0: 0.0,
                screen_size: self.screen_size,
                _pad1: [0.0; 2],
            },
            proj: proj.to_cols_array_2d(),
            view: view.to_cols_array_2d(),
        };
        frame
            .queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _sv: &wgpu::TextureView,
    ) {
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("SSAO: resolve not called");
        let ao_view = resources.get(resources.handle::<AOTexture>("ao"));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SSAO Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ao_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, texture_bg, &[]);
        pass.set_bind_group(1, &self.frame_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl SSAOPass {
    /// Update screen dimensions (called on init and resize).
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_size = [width as f32, height as f32];
        self.half_width = width / 2;
        self.half_height = height / 2;
    }

    /// Create a new SSAO pass with all GPU resources.
    pub fn new(device: &wgpu::Device) -> Self {
        let shader_source = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = vec2<f32>(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
    return out;
}

struct SSAOParams {
    radius: f32,
    bias: f32,
    intensity: f32,
    _pad0: f32,
    screen_size: vec2<f32>,
    _pad1: vec2<f32>,
};

@group(0) @binding(0) var gbuffer_position: texture_2d<f32>;
@group(0) @binding(1) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuffer_sampler: sampler;

struct FrameUniforms {
    params: SSAOParams,
    proj_mat: mat4x4<f32>,
    view_mat: mat4x4<f32>,
};

@group(1) @binding(0) var<uniform> frame: FrameUniforms;

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let h = fract(sin(vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)))) * 43758.5453);
    return h * 2.0 - 1.0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let pos_sample = textureSample(gbuffer_position, gbuffer_sampler, uv);
    let norm_sample = textureSample(gbuffer_normal, gbuffer_sampler, uv);

    // Sky check: GBuffer normal is (0,0,0) after clear
    if (norm_sample.r == 0.0 && norm_sample.g == 0.0 && norm_sample.b == 0.0) {
        return vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }

    let world_pos = pos_sample.xyz;
    let world_N = normalize(norm_sample.xyz * 2.0 - 1.0);

    // Transform to VIEW space (per LearnOpenGL: all SSAO math in view space)
    let view_pos4 = frame.view_mat * vec4<f32>(world_pos, 1.0);
    let view_pos = view_pos4.xyz;
    let frag_view_z = view_pos.z;
    let view_N4 = frame.view_mat * vec4<f32>(world_N, 0.0);
    let view_N = normalize(view_N4.xyz);

    // Random rotation (hash-based)
    let rot = hash2(uv * 1024.0);
    let rvec = vec3<f32>(rot.x, rot.y, 0.0);

    // TBN in VIEW space
    let tangent = normalize(rvec - view_N * dot(rvec, view_N));
    let bitangent = cross(view_N, tangent);

    var occlusion: f32 = 0.0;

    // ── 16-sample hemisphere kernel ───
    const KERNEL: array<vec3<f32>, 16> = array<vec3<f32>, 16>(
        vec3<f32>( 0.5381,  0.1856,  0.4319),
        vec3<f32>( 0.1379,  0.1967,  0.8544),
        vec3<f32>(-0.3476,  0.4789,  0.5346),
        vec3<f32>(-0.4791,  0.3456,  0.4765),
        vec3<f32>( 0.3883, -0.2607,  0.5915),
        vec3<f32>(-0.4340, -0.4412,  0.5311),
        vec3<f32>( 0.2380, -0.3947,  0.6678),
        vec3<f32>(-0.2238, -0.5672,  0.4478),
        vec3<f32>( 0.3065,  0.0922,  0.1805),
        vec3<f32>( 0.1244,  0.6114,  0.2616),
        vec3<f32>( 0.4086,  0.4489,  0.2630),
        vec3<f32>(-0.4856,  0.3076,  0.1789),
        vec3<f32>(-0.3801, -0.3943,  0.1389),
        vec3<f32>(-0.1088, -0.6461,  0.0838),
        vec3<f32>( 0.2919, -0.4453,  0.3042),
        vec3<f32>( 0.3513, -0.1539,  0.0345),
    );

    let view_radius = frame.params.radius;
    let view_bias = frame.params.bias;

    for (var i: u32 = 0u; i < 16u; i = i + 1u) {
        let ks = KERNEL[i];
        let sd = tangent * ks.x + bitangent * ks.y + view_N * ks.z;
        let sv = view_pos + sd * view_radius;
        let sc = frame.proj_mat * vec4<f32>(sv, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (frame.view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            if (occ_view_z >= sv.z + view_bias) {
                let z_delta = abs(frag_view_z - occ_view_z);
                // Range falloff: attenuate occlusion when the occluder is farther
                // than `radius` away in view-space Z. This prevents distant surfaces
                // from casting dark halos.
                let range_attenuation = 1.0 - smoothstep(0.0, view_radius, z_delta);
                occlusion += range_attenuation;
            }
        }
    }

    // Final AO value (per LearnOpenGL: invert, power)
    occlusion = 1.0 - occlusion / 16.0;
    occlusion = pow(occlusion, frame.params.intensity);

    // TODO: Add a separate AO blur pass. Inline bilateral blur was removed
    // because a fragment shader cannot sample its own output texture.
    // The previous code did `blurred += occlusion * w` which always evaluates
    // to `occlusion` (center value only, no actual neighbor sampling).

    return vec4<f32>(occlusion, 0.0, 0.0, 1.0);
}
"#;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSAO Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO Texture BGL"),
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

        let frame_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO Frame BGL"),
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
            label: Some("SSAO Pipeline Layout"),
            bind_group_layouts: &[Some(&texture_bgl), Some(&frame_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SSAO Pipeline"),
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
                    format: wgpu::TextureFormat::R8Unorm,
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

        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSAO Frame"),
            size: std::mem::size_of::<SSAOFrameUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO Frame BG"),
            layout: &frame_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
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
            label: Some("SSAO Quad Vtx"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            frame_buffer,
            frame_bind_group: frame_bg,
            pos_handle: None,
            normal_handle: None,
            texture_bind_group: None,
            texture_bind_group_layout: texture_bgl,
            frame_bind_group_layout: frame_bgl,
            proj: glam::Mat4::IDENTITY,
            view: glam::Mat4::IDENTITY,
            screen_size: [1024.0, 768.0],
            half_width: 512,
            half_height: 384,
            radius: 0.5,
            bias: 0.025,
            intensity: 1.5,
        }
    }

    /// Set SSAO sample radius (world-space units). The shader scales it by the
    /// fragment's view depth so the screen-space coverage stays consistent.
    pub fn set_radius(&mut self, radius: f32) {
        self.radius = radius.max(0.001);
    }

    /// Set SSAO depth comparison bias (world-space units). Scaled by view depth
    /// in the shader together with `radius`.
    pub fn set_bias(&mut self, bias: f32) {
        self.bias = bias.max(0.0);
    }

    /// Set SSAO intensity (power exponent).
    pub fn set_intensity(&mut self, intensity: f32) {
        self.intensity = intensity.max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        let (device, _) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("need device");
        device
    }

    #[test]
    fn signature_declares_reads_and_writes() {
        let device = headless_device();
        let sig = SSAOPass::new(&device).signature();
        assert_eq!(sig.name, "SSAO");
        assert_eq!(sig.reads.len(), 2);
        assert_eq!(sig.writes.len(), 1);
        assert!(sig.writes[0].type_id == TypeId::of::<AOTexture>() && sig.writes[0].name == "ao");
    }

    #[test]
    fn init_creates_resources() {
        let _pass = SSAOPass::new(&headless_device());
    }
}
