//! Composite Pass
//!
//! Final full-screen quad pass that composites the lit scene color with
//! screen-space reflections, water, volumetric clouds and god rays, then
//! outputs to the post-process input texture.
//!
//! Pipeline: ... → SSRPass → GodRayPass → WaterPass → CompositePass → ...

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

/// Composite pass uniforms (std140 layout).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CompositeUniforms {
    camera_pos: [f32; 3],
    _pad0: f32,
}

/// Composite pass state.
pub struct CompositePass {
    pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    scene_color_handle: Option<ResHandle<SceneColor>>,
    reflection_handle: Option<ResHandle<ReflectionTexture>>,
    water_color_handle: Option<ResHandle<WaterColor>>,
    cloud_color_handle: Option<ResHandle<CloudColor>>,
    god_ray_color_handle: Option<ResHandle<GodRayColor>>,
    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    albedo_handle: Option<ResHandle<GAlbedo>>,
    material_handle: Option<ResHandle<GMaterial>>,
    texture_bind_group: Option<wgpu::BindGroup>,
    #[allow(dead_code)]
    texture_bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl Pass for CompositePass {
    fn name(&self) -> &str {
        "Composite"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("Composite")
            .read::<SceneColor>("scene_color")
            .read::<ReflectionTexture>("reflection")
            .read::<WaterColor>("water_color")
            .read::<CloudColor>("cloud_color")
            .read::<GodRayColor>("god_ray_color")
            .read::<GPosition>("gbuffer_position")
            .read::<GNormal>("gbuffer_normal")
            .read::<GAlbedo>("gbuffer_albedo")
            .read::<GMaterial>("gbuffer_material")
            .write::<PostProcessInput>("post_process_input", wgpu::TextureFormat::Rgba16Float)
    }

    fn init(device: &wgpu::Device) -> Self {
        Self::new(device, wgpu::TextureFormat::Bgra8UnormSrgb)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.scene_color_handle = Some(resources.handle::<SceneColor>("scene_color"));
        self.reflection_handle = Some(resources.handle::<ReflectionTexture>("reflection"));
        self.water_color_handle = Some(resources.handle::<WaterColor>("water_color"));
        self.cloud_color_handle = Some(resources.handle::<CloudColor>("cloud_color"));
        self.god_ray_color_handle = Some(resources.handle::<GodRayColor>("god_ray_color"));
        self.pos_handle = Some(resources.handle::<GPosition>("gbuffer_position"));
        self.normal_handle = Some(resources.handle::<GNormal>("gbuffer_normal"));
        self.albedo_handle = Some(resources.handle::<GAlbedo>("gbuffer_albedo"));
        self.material_handle = Some(resources.handle::<GMaterial>("gbuffer_material"));

        let scene_color_view = resources.get(self.scene_color_handle.unwrap());
        let reflection_view = resources.get(self.reflection_handle.unwrap());
        let water_color_view = resources.get(self.water_color_handle.unwrap());
        let cloud_color_view = resources.get(self.cloud_color_handle.unwrap());
        let god_ray_color_view = resources.get(self.god_ray_color_handle.unwrap());
        let pos_view = resources.get(self.pos_handle.unwrap());
        let normal_view = resources.get(self.normal_handle.unwrap());
        let albedo_view = resources.get(self.albedo_handle.unwrap());
        let material_view = resources.get(self.material_handle.unwrap());

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(reflection_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(water_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(cloud_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(god_ray_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(pos_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(material_view),
                },
            ],
        }));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        let uniforms = CompositeUniforms {
            camera_pos: frame.camera.position.into(),
            _pad0: 0.0,
        };
        frame
            .queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
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
            .expect("Composite: resolve not called");
        let post_process_view =
            resources.get(resources.handle::<PostProcessInput>("post_process_input"));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: post_process_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            multiview_mask: None,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, texture_bg, &[]);
        pass.set_bind_group(1, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl CompositePass {
    /// Create a new composite pass.
    pub fn new(device: &wgpu::Device, _surface_format: wgpu::TextureFormat) -> Self {
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

@group(0) @binding(0) var scene_color: texture_2d<f32>;
@group(0) @binding(1) var reflection_texture: texture_2d<f32>;
@group(0) @binding(2) var water_color: texture_2d<f32>;
@group(0) @binding(3) var cloud_color: texture_2d<f32>;
@group(0) @binding(4) var god_ray_color: texture_2d<f32>;
@group(0) @binding(5) var tex_sampler: sampler;
@group(0) @binding(6) var gbuffer_position: texture_2d<f32>;
@group(0) @binding(7) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(8) var gbuffer_albedo: texture_2d<f32>;
@group(0) @binding(9) var gbuffer_material: texture_2d<f32>;

struct CompositeUniforms {
    camera_pos: vec3<f32>,
    _pad0: f32,
};

@group(1) @binding(0) var<uniform> uniforms: CompositeUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let scene = textureSample(scene_color, tex_sampler, uv);
    let refl  = textureSample(reflection_texture, tex_sampler, uv);
    let water = textureSample(water_color, tex_sampler, uv);
    let cloud = textureSample(cloud_color, tex_sampler, uv);
    let god_ray = textureSample(god_ray_color, tex_sampler, uv);

    // Decode G-Buffer to compute Fresnel reflectance.
    let world_pos = textureSample(gbuffer_position, tex_sampler, uv).xyz;
    let normal_sample = textureSample(gbuffer_normal, tex_sampler, uv);
    let albedo = textureSample(gbuffer_albedo, tex_sampler, uv).rgb;
    let material_sample = textureSample(gbuffer_material, tex_sampler, uv);

    var lit: vec3<f32>;
    if (normal_sample.r == 0.0 && normal_sample.g == 0.0 && normal_sample.b == 0.0) {
        // Sky / background: bypass SSR blend.
        lit = scene.rgb;
    } else {
        let N = normalize(normal_sample.xyz * 2.0 - 1.0);
        let V = normalize(uniforms.camera_pos - world_pos);
        let NdotV = max(dot(N, V), 0.0);
        let roughness = material_sample.r;
        let metallic = material_sample.g;
        let F0 = mix(vec3<f32>(0.04), albedo, metallic);
        let fresnel = F0 + (vec3<f32>(1.0) - F0) * pow(1.0 - NdotV, 5.0);
        // Mask low-roughness surfaces: SSR pass already encodes roughness in refl.a,
        // but re-apply here so non-reflective pixels are unchanged.
        let reflectance = fresnel * refl.a;
        lit = mix(scene.rgb, refl.rgb, reflectance);
    }

    let with_clouds = mix(lit, cloud.rgb, cloud.a);
    let with_god_rays = with_clouds + god_ray.rgb;
    let final_color = mix(with_god_rays, water.rgb, water.a);
    return vec4<f32>(final_color, 1.0);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Composite Texture BGL"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Composite Uniform BGL"),
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
            label: Some("Composite Pipeline Layout"),
            bind_group_layouts: &[Some(&texture_bgl), Some(&uniform_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Composite Pipeline"),
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

        let quad_vertices: [[f32; 2]; 6] = [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ];
        let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Composite Quad Vtx"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Composite Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Composite Uniform Buffer"),
            size: std::mem::size_of::<CompositeUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite Uniform BG"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            scene_color_handle: None,
            reflection_handle: None,
            water_color_handle: None,
            cloud_color_handle: None,
            god_ray_color_handle: None,
            pos_handle: None,
            normal_handle: None,
            albedo_handle: None,
            material_handle: None,
            texture_bind_group: None,
            texture_bind_group_layout: texture_bgl,
            uniform_buffer,
            uniform_bind_group,
            uniform_bind_group_layout: uniform_bgl,
            sampler,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn signature_declares_reads_and_write() {
        let device = headless_device();
        let sig = CompositePass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb).signature();
        assert_eq!(sig.name, "Composite");
        assert_eq!(sig.reads.len(), 9);
        assert_eq!(sig.writes.len(), 1);
    }

    #[test]
    fn init_creates_resources() {
        let _pass = CompositePass::new(&headless_device(), wgpu::TextureFormat::Bgra8UnormSrgb);
    }
}
