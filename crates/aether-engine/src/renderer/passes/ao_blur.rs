//! AO Blur Pass — Bilateral blur for SSAO
//!
//! Full-screen quad pass. Applies a 3×3 bilateral (depth-aware) gaussian blur
//! to the raw SSAO output, preserving edges while smoothing noise from the
//! 16-sample kernel.
//!
//! Pipeline: SSAOPass (→AOTexture) → AOBlurPass (→AOTextureBlurred) → LightingPass

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

#[path = "ao_blur_shader.rs"]
mod shader;

/// AO blur parameters (matches WGSL std140 layout, 48 bytes total).
/// WGSL std140: depth_sigma@0 + texel_size(vec2)@8 + camera_pos(vec3)@16 +
/// camera_forward(vec3)@32 = 48.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurParams {
    depth_sigma: f32,
    _pad0: f32,
    texel_size: [f32; 2],
    camera_pos: [f32; 3],
    _pad1: f32,
    camera_forward: [f32; 3],
    _pad2: f32,
}

/// AO Blur pass state.
pub struct AOBlurPass {
    pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    ao_handle: Option<ResHandle<AOTexture>>,
    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    texture_bind_group: Option<wgpu::BindGroup>,
    #[allow(dead_code)]
    texture_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    params_bind_group_layout: wgpu::BindGroupLayout,
    screen_size: [f32; 2],
    half_width: u32,
    half_height: u32,
    enabled: bool,
}

impl Pass for AOBlurPass {
    fn name(&self) -> &str {
        "AOBlur"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("AOBlur")
            .read::<AOTexture>()
            .read::<GPosition>()
            .read::<GNormal>()
            .write_sized::<AOTextureBlurred>(
                wgpu::TextureFormat::R8Unorm,
                self.half_width.max(1),
                self.half_height.max(1),
            )
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.ao_handle = Some(resources.handle::<AOTexture>());
        self.pos_handle = Some(resources.handle::<GPosition>());
        self.normal_handle = Some(resources.handle::<GNormal>());

        let ao_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("AOBlur AO Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let gbuffer_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("AOBlur GBuffer Sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let ao_view = resources.get(self.ao_handle.unwrap());
        let pos_view = resources.get(self.pos_handle.unwrap());
        let normal_view = resources.get(self.normal_handle.unwrap());

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("AOBlur Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(ao_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(pos_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&ao_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&gbuffer_sampler),
                },
            ],
        }));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.set_enabled(frame.config.ssao_enabled);
        self.set_screen_size(frame.config.screen_width, frame.config.screen_height);

        let texel_w = if self.half_width > 0 {
            1.0 / self.half_width as f32
        } else {
            0.0
        };
        let texel_h = if self.half_height > 0 {
            1.0 / self.half_height as f32
        } else {
            0.0
        };
        let p = BlurParams {
            depth_sigma: 0.5,
            _pad0: 0.0,
            texel_size: [texel_w, texel_h],
            camera_pos: frame.camera.position.to_array(),
            _pad1: 0.0,
            camera_forward: frame.camera.forward().to_array(),
            _pad2: 0.0,
        };
        frame
            .queue
            .write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[p]));
    }

    fn should_run(&self, _frame: &RenderFrame) -> bool {
        self.enabled
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
            .expect("AOBlur: resolve not called");
        let ao_blurred_view = resources.get(resources.handle::<AOTextureBlurred>());

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("AO Blur Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ao_blurred_view,
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
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }
}

impl AOBlurPass {
    /// Enable or disable the AO blur pass.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Update screen dimensions (called on init and resize).
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_size = [width as f32, height as f32];
        self.half_width = width / 2;
        self.half_height = height / 2;
    }

    /// Create a new AO blur pass with all GPU resources.
    pub fn new(device: &wgpu::Device) -> Self {
        let shader_source = shader::AO_BLUR_SHADER_SRC;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("AOBlur Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("AOBlur Texture BGL"),
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let params_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("AOBlur Params BGL"),
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
            label: Some("AOBlur Pipeline Layout"),
            bind_group_layouts: &[Some(&texture_bgl), Some(&params_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("AOBlur Pipeline"),
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

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("AOBlur Params"),
            size: std::mem::size_of::<BlurParams>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("AOBlur Params BG"),
            layout: &params_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
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
            label: Some("AOBlur Quad Vtx"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            params_buffer,
            params_bind_group: params_bg,
            ao_handle: None,
            pos_handle: None,
            normal_handle: None,
            texture_bind_group: None,
            texture_bind_group_layout: texture_bgl,
            params_bind_group_layout: params_bgl,
            screen_size: [1024.0, 768.0],
            half_width: 512,
            half_height: 384,
            enabled: true,
        }
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
        let sig = AOBlurPass::new(&device).signature();
        assert_eq!(sig.name, "AOBlur");
        assert_eq!(sig.reads.len(), 3);
        assert_eq!(sig.writes.len(), 1);
        assert!(
            sig.writes[0].type_id == TypeId::of::<AOTextureBlurred>()
                && sig.writes[0].name == "ao_blurred"
        );
    }

    #[test]
    fn blur_params_match_wgsl_layout() {
        assert_eq!(std::mem::size_of::<BlurParams>(), 48);
    }

    #[test]
    fn init_creates_resources() {
        let _pass = AOBlurPass::new(&headless_device());
    }
}
