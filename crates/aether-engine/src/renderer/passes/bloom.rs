//! Bloom Pass
//!
//! Multi-pass screen-space bloom effect. Extracts bright regions,
//! downsample-blurs through 3 mip levels, upsample-additively blends back,
//! and composites with the original HDR image.
//!
//! Pipeline position: CompositePass → BloomPass → ToneMappingPass
//!
//! Internally executes 7 sub-passes in sequence:
//!   1. Extract     PostProcessInput → BrightTexture
//!   2. Downsample  BrightTexture    → BloomMip0 (1/2)
//!   3. Downsample  BloomMip0        → BloomMip1 (1/4)
//!   4. Downsample  BloomMip1        → BloomMip2 (1/8)
//!   5. Upsample    BloomMip2        → BloomMip1 (add)
//!   6. Upsample    BloomMip1        → BloomMip0 (add)
//!   7. Upsample    BloomMip0        → BloomTexture (add)
//!   8. Composite   PostProcessInput + BloomTexture → BloomResult

use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

/// Bloom parameters (matches WGSL std140 layout).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomUniforms {
    threshold: f32,
    intensity: f32,
    bloom_intensity: f32,
    enabled: u32,
}

/// Bloom Pass implementation.
pub struct BloomPass {
    // Sub-pass pipelines
    extract_pipeline: wgpu::RenderPipeline,
    downsample_pipeline: wgpu::RenderPipeline,
    upsample_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,

    // Shared geometry
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,

    // Uniforms
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    uniform_bind_group_layout: wgpu::BindGroupLayout,

    // Bind group layouts
    extract_bgl: wgpu::BindGroupLayout,
    blur_bgl: wgpu::BindGroupLayout,
    composite_bgl: wgpu::BindGroupLayout,

    // Texture bind groups (recreated in resolve)
    extract_bg: Option<wgpu::BindGroup>,
    downsample0_bg: Option<wgpu::BindGroup>,
    downsample1_bg: Option<wgpu::BindGroup>,
    downsample2_bg: Option<wgpu::BindGroup>,
    upsample0_bg: Option<wgpu::BindGroup>,
    upsample1_bg: Option<wgpu::BindGroup>,
    upsample2_bg: Option<wgpu::BindGroup>,
    composite_bg: Option<wgpu::BindGroup>,

    // Samplers
    sampler_linear_clamp: wgpu::Sampler,
    sampler_linear_wrap: wgpu::Sampler,

    // Resource handles (from ResourceTable)
    input_handle: Option<ResHandle<PostProcessInput>>,
    result_handle: Option<ResHandle<BloomResult>>,

    // Screen dimensions (updated via set_screen_size)
    screen_width: u32,
    screen_height: u32,

    // Intermediate textures and views (created in resolve)
    bright_texture: Option<(wgpu::Texture, wgpu::TextureView)>,
    mip0: Option<(wgpu::Texture, wgpu::TextureView)>,
    mip1: Option<(wgpu::Texture, wgpu::TextureView)>,
    mip2: Option<(wgpu::Texture, wgpu::TextureView)>,
    bloom_texture: Option<(wgpu::Texture, wgpu::TextureView)>,

    // Parameters
    threshold: f32,
    intensity: f32,
    bloom_intensity: f32,
    enabled: bool,
}

impl Pass for BloomPass {
    fn name(&self) -> &str {
        "Bloom"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Bloom")
            .read::<PostProcessInput>("post_process_input")
            .write::<BloomResult>("bloom_result", wgpu::TextureFormat::Rgba16Float)
    }

    fn init(device: &wgpu::Device) -> Self {
        Self::new(device, 1280, 720)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.input_handle = Some(resources.handle::<PostProcessInput>("post_process_input"));
        self.result_handle = Some(resources.handle::<BloomResult>("bloom_result"));

        // Recreate intermediate textures at current screen size
        self.create_intermediate_textures(device);

        let input_view = resources.get(self.input_handle.unwrap());
        let bright_view = &self.bright_texture.as_ref().unwrap().1;
        let mip0_view = &self.mip0.as_ref().unwrap().1;
        let mip1_view = &self.mip1.as_ref().unwrap().1;
        let mip2_view = &self.mip2.as_ref().unwrap().1;
        let bloom_view = &self.bloom_texture.as_ref().unwrap().1;

        // Extract bind group
        self.extract_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Extract BG"),
            layout: &self.extract_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_clamp),
                },
            ],
        }));

        // Downsample bind groups
        self.downsample0_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Downsample 0 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(bright_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_clamp),
                },
            ],
        }));

        self.downsample1_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Downsample 1 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(mip0_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_clamp),
                },
            ],
        }));

        self.downsample2_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Downsample 2 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(mip1_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_clamp),
                },
            ],
        }));

        // Upsample bind groups (read from lower res, write to higher res with additive blend)
        self.upsample0_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Upsample 0 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(mip2_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_wrap),
                },
            ],
        }));

        self.upsample1_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Upsample 1 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(mip1_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_wrap),
                },
            ],
        }));

        self.upsample2_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Upsample 2 BG"),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(mip0_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_wrap),
                },
            ],
        }));

        // Composite bind group
        self.composite_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Composite BG"),
            layout: &self.composite_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(bloom_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_linear_clamp),
                },
            ],
        }));
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let result_view = resources.get(self.result_handle.unwrap());

        // 1. Extract bright regions
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Bloom Extract"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.bright_texture.as_ref().unwrap().1,
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
            pass.set_pipeline(&self.extract_pipeline);
            pass.set_bind_group(0, self.extract_bg.as_ref().unwrap(), &[]);
            pass.set_bind_group(1, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
            pass.draw(0..self.quad_vertex_count, 0..1);
        }

        // 2-4. Downsample chain
        let downsample_targets = [
            (&self.mip0.as_ref().unwrap().1, self.downsample0_bg.as_ref().unwrap(), "Bloom Downsample 0"),
            (&self.mip1.as_ref().unwrap().1, self.downsample1_bg.as_ref().unwrap(), "Bloom Downsample 1"),
            (&self.mip2.as_ref().unwrap().1, self.downsample2_bg.as_ref().unwrap(), "Bloom Downsample 2"),
        ];

        for (view, bg, label) in &downsample_targets {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
            pass.set_pipeline(&self.downsample_pipeline);
            pass.set_bind_group(0, *bg, &[]);
            pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
            pass.draw(0..self.quad_vertex_count, 0..1);
        }

        // 5-6. Upsample chain with additive blending (Mip2→Mip1, Mip1→Mip0)
        let upsample_targets = [
            (&self.mip1.as_ref().unwrap().1, self.upsample0_bg.as_ref().unwrap(), "Bloom Upsample 0"),
            (&self.mip0.as_ref().unwrap().1, self.upsample1_bg.as_ref().unwrap(), "Bloom Upsample 1"),
        ];

        for (view, bg, label) in &upsample_targets {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
            pass.set_pipeline(&self.upsample_pipeline);
            pass.set_bind_group(0, *bg, &[]);
            pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
            pass.draw(0..self.quad_vertex_count, 0..1);
        }

        // 7. Final upsample: Mip0 → BloomTexture (Clear — first write)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Bloom Upsample 2"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.bloom_texture.as_ref().unwrap().1,
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
            pass.set_pipeline(&self.upsample_pipeline);
            pass.set_bind_group(0, self.upsample2_bg.as_ref().unwrap(), &[]);
            pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
            pass.draw(0..self.quad_vertex_count, 0..1);
        }

        // 8. Composite: HDR + Bloom → BloomResult
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Bloom Composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: result_view,
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
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, self.composite_bg.as_ref().unwrap(), &[]);
            pass.set_bind_group(1, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
            pass.draw(0..self.quad_vertex_count, 0..1);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl BloomPass {
    /// Create a new bloom pass.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let extract_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bloom Extract Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(r#"
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

struct BloomUniforms {
    threshold: f32,
    intensity: f32,
    bloom_intensity: f32,
    enabled: u32,
};

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: BloomUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (uniforms.enabled == 0u) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let hdr = textureSample(input_texture, tex_sampler, in.uv).rgb;
    let luminance = dot(hdr, vec3<f32>(0.2126, 0.7152, 0.0722));
    if (luminance > uniforms.threshold) {
        return vec4<f32>((hdr - uniforms.threshold) * uniforms.intensity, 1.0);
    }
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
"#)),
        });

        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bloom Blur Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(r#"
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

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel_size = 1.0 / vec2<f32>(textureDimensions(input_texture));
    let uv = in.uv;

    // 9-tap (3×3) Gaussian approximation
    // Center weight 0.25, cross neighbors 0.125, corners 0.0625
    let center = textureSample(input_texture, tex_sampler, uv).rgb;
    let n = textureSample(input_texture, tex_sampler, uv + vec2( 0.0, -1.0) * texel_size).rgb;
    let s = textureSample(input_texture, tex_sampler, uv + vec2( 0.0,  1.0) * texel_size).rgb;
    let e = textureSample(input_texture, tex_sampler, uv + vec2( 1.0,  0.0) * texel_size).rgb;
    let w = textureSample(input_texture, tex_sampler, uv + vec2(-1.0,  0.0) * texel_size).rgb;
    let ne = textureSample(input_texture, tex_sampler, uv + vec2( 1.0, -1.0) * texel_size).rgb;
    let nw = textureSample(input_texture, tex_sampler, uv + vec2(-1.0, -1.0) * texel_size).rgb;
    let se = textureSample(input_texture, tex_sampler, uv + vec2( 1.0,  1.0) * texel_size).rgb;
    let sw = textureSample(input_texture, tex_sampler, uv + vec2(-1.0,  1.0) * texel_size).rgb;

    return vec4<f32>(center * 0.25 + (n + s + e + w) * 0.125 + (ne + nw + se + sw) * 0.0625, 1.0);
}
"#)),
        });

        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bloom Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(r#"
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

struct BloomUniforms {
    threshold: f32,
    intensity: f32,
    bloom_intensity: f32,
    enabled: u32,
};

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var bloom_texture: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: BloomUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(input_texture, tex_sampler, in.uv).rgb;
    if (uniforms.enabled == 0u) {
        return vec4<f32>(hdr, 1.0);
    }
    let bloom = textureSample(bloom_texture, tex_sampler, in.uv).rgb;
    return vec4<f32>(hdr + bloom * uniforms.bloom_intensity, 1.0);
}
"#)),
        });

        let extract_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bloom Extract BGL"),
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

        let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bloom Blur BGL"),
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

        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bloom Composite BGL"),
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

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bloom Uniform BGL"),
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

        let extract_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Bloom Extract Pipeline Layout"),
            bind_group_layouts: &[Some(&extract_bgl), Some(&uniform_bgl)],
            immediate_size: 0,
        });

        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Bloom Blur Pipeline Layout"),
            bind_group_layouts: &[Some(&blur_bgl)],
            immediate_size: 0,
        });

        let composite_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Bloom Composite Pipeline Layout"),
            bind_group_layouts: &[Some(&composite_bgl), Some(&uniform_bgl)],
            immediate_size: 0,
        });

        let extract_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Bloom Extract Pipeline"),
            layout: Some(&extract_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &extract_shader,
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
                module: &extract_shader,
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

        let downsample_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Bloom Downsample Pipeline"),
            layout: Some(&blur_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blur_shader,
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
                module: &blur_shader,
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

        let upsample_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Bloom Upsample Pipeline"),
            layout: Some(&blur_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blur_shader,
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
                module: &blur_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
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

        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Bloom Composite Pipeline"),
            layout: Some(&composite_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &composite_shader,
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
                module: &composite_shader,
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
            label: Some("Bloom Uniform Buffer"),
            size: std::mem::size_of::<BloomUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bloom Uniform BG"),
            layout: &uniform_bgl,
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
            label: Some("Bloom Quad Vtx"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let sampler_linear_clamp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Bloom Sampler Clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let sampler_linear_wrap = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Bloom Sampler Wrap"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        let mut pass = Self {
            extract_pipeline,
            downsample_pipeline,
            upsample_pipeline,
            composite_pipeline,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            uniform_buffer,
            uniform_bind_group,
            uniform_bind_group_layout: uniform_bgl,
            extract_bgl,
            blur_bgl,
            composite_bgl,
            extract_bg: None,
            downsample0_bg: None,
            downsample1_bg: None,
            downsample2_bg: None,
            upsample0_bg: None,
            upsample1_bg: None,
            upsample2_bg: None,
            composite_bg: None,
            sampler_linear_clamp,
            sampler_linear_wrap,
            input_handle: None,
            result_handle: None,
            screen_width: width,
            screen_height: height,
            bright_texture: None,
            mip0: None,
            mip1: None,
            mip2: None,
            bloom_texture: None,
            threshold: 1.0,
            intensity: 1.0,
            bloom_intensity: 0.5,
            enabled: true,
        };

        pass.create_intermediate_textures(device);
        pass
    }

    /// Set screen dimensions (call before rebuild on resize).
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    /// Enable/disable bloom.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set extraction threshold.
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold;
    }

    /// Set extraction intensity.
    pub fn set_intensity(&mut self, intensity: f32) {
        self.intensity = intensity;
    }

    /// Set final bloom intensity.
    pub fn set_bloom_intensity(&mut self, intensity: f32) {
        self.bloom_intensity = intensity;
    }

    /// Update uniform buffer with current parameters.
    pub fn update_uniforms(&self, queue: &wgpu::Queue) {
        let uniforms = BloomUniforms {
            threshold: self.threshold,
            intensity: self.intensity,
            bloom_intensity: self.bloom_intensity,
            enabled: if self.enabled { 1 } else { 0 },
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    fn create_intermediate_textures(&mut self, device: &wgpu::Device) {
        let w = self.screen_width;
        let h = self.screen_height;

        let create_tex = |label: &str, width: u32, height: u32| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };

        self.bright_texture = Some(create_tex("Bloom Bright", w, h));
        self.mip0 = Some(create_tex("Bloom Mip0", w / 2, h / 2));
        self.mip1 = Some(create_tex("Bloom Mip1", w / 4, h / 4));
        self.mip2 = Some(create_tex("Bloom Mip2", w / 8, h / 8));
        self.bloom_texture = Some(create_tex("Bloom Texture", w, h));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("need adapter");
        let (device, _) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("need device");
        device
    }

    #[test]
    fn signature_declares_resources() {
        let device = headless_device();
        let pass = BloomPass::new(&device, 64, 64);
        let sig = pass.signature();
        assert_eq!(sig.name, "Bloom");
        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.writes.len(), 1);
    }

    #[test]
    fn init_creates_resources() {
        let _pass = BloomPass::new(&headless_device(), 64, 64);
    }

    #[test]
    fn default_params() {
        let device = headless_device();
        let pass = BloomPass::new(&device, 64, 64);
        assert!(pass.enabled);
        assert_eq!(pass.threshold, 1.0);
        assert_eq!(pass.intensity, 1.0);
        assert_eq!(pass.bloom_intensity, 0.5);
    }

    #[test]
    fn params_can_be_changed() {
        let device = headless_device();
        let mut pass = BloomPass::new(&device, 64, 64);
        pass.set_enabled(false);
        pass.set_threshold(2.0);
        pass.set_intensity(3.0);
        pass.set_bloom_intensity(0.8);
        assert!(!pass.enabled);
        assert_eq!(pass.threshold, 2.0);
        assert_eq!(pass.intensity, 3.0);
        assert_eq!(pass.bloom_intensity, 0.8);
    }
}
