//! Tone Mapping Pass
//!
//! Full-screen quad pass that converts HDR linear scene color to LDR
//! display output. Supports ACES Filmic, Reinhard, and Off (linear clip).
//!
//! Reads `PostProcessInput` (Rgba16Float) and writes directly to swapchain.

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

/// Tone mapping algorithm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum ToneMappingMode {
    /// No tone mapping — linear clip to [0,1].
    Off = 0,
    /// Reinhard: color / (color + 1).
    Reinhard = 1,
    /// ACES Filmic approximation.
    #[default]
    ACES = 2,
}

/// Uniforms for the tone mapping shader.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ToneMappingUniforms {
    mode: u32,
    _pad: [u32; 3],
}

/// Tone Mapping Pass implementation.
pub struct ToneMappingPass {
    pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    input_handle: Option<ResHandle<BloomResult>>,
    texture_bind_group: Option<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    mode: ToneMappingMode,
}

impl Pass for ToneMappingPass {
    fn name(&self) -> &str {
        "ToneMapping"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("ToneMapping")
            .read::<BloomResult>()
            .write::<FxaaInput>(wgpu::TextureFormat::Bgra8UnormSrgb)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device, ctx.surface_format)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.input_handle = Some(resources.handle::<BloomResult>());
        let input_view = resources.get(self.input_handle.unwrap());

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ToneMapping Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.set_mode(frame.config.tone_mapping_mode);
        self.update_uniforms(frame.queue);
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("ToneMappingPass: resolve not called");
        let fxaa_view = resources.get(resources.handle::<FxaaInput>());

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Tone Mapping Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: fxaa_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, texture_bg, &[]);
        pass.set_bind_group(1, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }
}

impl ToneMappingPass {
    /// Create a new tone mapping pass.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader_source = TONE_MAPPING_SHADER;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tone Mapping Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ToneMapping Texture BGL"),
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

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ToneMapping Uniform BGL"),
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
            label: Some("ToneMapping Pipeline Layout"),
            bind_group_layouts: &[
                Some(&texture_bind_group_layout),
                Some(&uniform_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ToneMapping Pipeline"),
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
            multiview_mask: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ToneMapping Uniform Buffer"),
            size: std::mem::size_of::<ToneMappingUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ToneMapping Uniform Bind Group"),
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
            label: Some("ToneMapping Quad Vtx"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ToneMapping Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group_layout,
            input_handle: None,
            texture_bind_group: None,
            sampler,
            mode: ToneMappingMode::default(),
        }
    }

    /// Set the tone mapping mode.
    pub fn set_mode(&mut self, mode: ToneMappingMode) {
        self.mode = mode;
    }

    /// Update the uniform buffer with the current mode.
    pub fn update_uniforms(&self, queue: &wgpu::Queue) {
        let uniforms = ToneMappingUniforms {
            mode: self.mode as u32,
            _pad: [0; 3],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::headless_device_queue;

    #[test]
    fn signature_declares_reads_and_write() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let pass = ToneMappingPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        let sig = pass.signature();
        assert_eq!(sig.name, "ToneMapping");
        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.writes.len(), 1);
    }

    #[test]
    fn init_creates_resources() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let _pass = ToneMappingPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
    }

    #[test]
    fn default_mode_is_aces() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let pass = ToneMappingPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        assert_eq!(pass.mode, ToneMappingMode::ACES);
    }

    #[test]
    fn mode_can_be_changed() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let mut pass = ToneMappingPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        pass.set_mode(ToneMappingMode::Reinhard);
        assert_eq!(pass.mode, ToneMappingMode::Reinhard);
        pass.set_mode(ToneMappingMode::Off);
        assert_eq!(pass.mode, ToneMappingMode::Off);
    }
}

/// WGSL source for the tone mapping pass.
pub(crate) const TONE_MAPPING_SHADER: &str = r#"
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

struct ToneMappingUniforms {
    mode: u32,
};

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: ToneMappingUniforms;

// ── ACES Filmic Tone Mapping ─────────────────────────────────────────

fn aces_tone_map(x: vec3<f32>) -> vec3<f32> {
    // ACES Filmic approximation (Krzysztof Narkowicz)
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn reinhard_tone_map(x: vec3<f32>) -> vec3<f32> {
    return x / (x + vec3<f32>(1.0));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(input_texture, tex_sampler, in.uv).rgb;

    var ldr: vec3<f32>;
    if (uniforms.mode == 0u) {
        // Off — linear clip
        ldr = clamp(hdr, vec3<f32>(0.0), vec3<f32>(1.0));
    } else if (uniforms.mode == 1u) {
        // Reinhard
        ldr = reinhard_tone_map(hdr);
    } else {
        // ACES Filmic (default)
        ldr = aces_tone_map(hdr);
    }

    return vec4<f32>(ldr, 1.0);
}
"#;
