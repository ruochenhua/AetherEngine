//! Composite Pass
//!
//! Final full-screen quad pass that composites the lit scene color with
//! screen-space reflections, water, volumetric clouds and god rays, then
//! outputs to the post-process input texture.
//!
//! Pipeline: ... → SSRPass → GodRayPass → WaterPass → CompositePass → ...

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use wgpu::util::DeviceExt;

mod shaders;
#[cfg(test)]
mod tests;

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
    transparent_color_handle: Option<ResHandle<TransparentColor>>,
    transparent_enabled: bool,
    texture_bind_group: Option<wgpu::BindGroup>,
    #[allow(dead_code)]
    texture_bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

/// Apply the frozen CPU reference equation for the composite overlay order.
///
/// The shader uses the same sequence: transparent, cloud, god ray, then water.
#[cfg(test)]
pub(crate) fn compose_overlays(
    lit1: [f32; 3],
    transparent: [f32; 4],
    cloud: [f32; 4],
    god_ray: [f32; 4],
    water: [f32; 4],
) -> [f32; 3] {
    let transparent_alpha = transparent[3].clamp(0.0, 1.0);
    let lit2 = [
        lit1[0] * (1.0 - transparent_alpha) + transparent[0],
        lit1[1] * (1.0 - transparent_alpha) + transparent[1],
        lit1[2] * (1.0 - transparent_alpha) + transparent[2],
    ];
    let cloud_alpha = cloud[3].clamp(0.0, 1.0);
    let lit3 = [
        lit2[0] * (1.0 - cloud_alpha) + cloud[0],
        lit2[1] * (1.0 - cloud_alpha) + cloud[1],
        lit2[2] * (1.0 - cloud_alpha) + cloud[2],
    ];
    let lit4 = [
        lit3[0] + god_ray[0],
        lit3[1] + god_ray[1],
        lit3[2] + god_ray[2],
    ];
    if water[3] > 0.0001 {
        let water_alpha = water[3].clamp(0.0, 1.0);
        [
            lit4[0] * (1.0 - water_alpha) + water[0] * water_alpha,
            lit4[1] * (1.0 - water_alpha) + water[1] * water_alpha,
            lit4[2] * (1.0 - water_alpha) + water[2] * water_alpha,
        ]
    } else {
        lit4
    }
}

impl Pass for CompositePass {
    fn name(&self) -> &str {
        "Composite"
    }

    fn signature(&self) -> PassSignature {
        let mut signature = PassSignature::new("Composite")
            .read::<SceneColor>()
            .read::<ReflectionTexture>()
            .read::<WaterColor>()
            .read::<CloudColor>()
            .read::<GodRayColor>()
            .read::<GPosition>()
            .read::<GNormal>()
            .read::<GAlbedo>()
            .read::<GMaterial>();
        if self.transparent_enabled {
            signature = signature.read::<TransparentColor>();
        }
        signature.write::<PostProcessInput>(wgpu::TextureFormat::Rgba16Float)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device, ctx.surface_format)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.scene_color_handle = Some(resources.handle::<SceneColor>());
        self.reflection_handle = Some(resources.handle::<ReflectionTexture>());
        self.water_color_handle = Some(resources.handle::<WaterColor>());
        self.cloud_color_handle = Some(resources.handle::<CloudColor>());
        self.god_ray_color_handle = Some(resources.handle::<GodRayColor>());
        self.pos_handle = Some(resources.handle::<GPosition>());
        self.normal_handle = Some(resources.handle::<GNormal>());
        self.albedo_handle = Some(resources.handle::<GAlbedo>());
        self.material_handle = Some(resources.handle::<GMaterial>());
        if self.transparent_enabled {
            self.transparent_color_handle = Some(resources.handle::<TransparentColor>());
        }

        let scene_color_view = resources.get(self.scene_color_handle.unwrap());
        let reflection_view = resources.get(self.reflection_handle.unwrap());
        let water_color_view = resources.get(self.water_color_handle.unwrap());
        let cloud_color_view = resources.get(self.cloud_color_handle.unwrap());
        let god_ray_color_view = resources.get(self.god_ray_color_handle.unwrap());
        let pos_view = resources.get(self.pos_handle.unwrap());
        let normal_view = resources.get(self.normal_handle.unwrap());
        let albedo_view = resources.get(self.albedo_handle.unwrap());
        let material_view = resources.get(self.material_handle.unwrap());

        let mut entries = vec![
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
        ];
        if let Some(handle) = self.transparent_color_handle {
            entries.push(wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::TextureView(resources.get(handle)),
            });
        }
        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &entries,
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
        let Some(texture_bg) = self.texture_bind_group.as_ref() else {
            return;
        };
        let post_process_view = resources.get(resources.handle::<PostProcessInput>());

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
}

impl CompositePass {
    fn texture_bind_group_layout_entries(
        transparent_enabled: bool,
    ) -> Vec<wgpu::BindGroupLayoutEntry> {
        let mut entries = (0..10)
            .map(|binding| Self::texture_layout_entry(binding, binding == 5))
            .collect::<Vec<_>>();
        if transparent_enabled {
            entries.push(Self::texture_layout_entry(10, false));
        }
        entries
    }

    fn texture_layout_entry(binding: u32, sampler: bool) -> wgpu::BindGroupLayoutEntry {
        let ty = if sampler {
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)
        } else {
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            }
        };
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        }
    }

    /// Create a new composite pass.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        Self::new_with_variant(device, surface_format, false)
    }

    /// Create a composite pass with the transparent overlay ABI enabled.
    pub fn new_with_transparency(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_variant(device, surface_format, true)
    }

    fn new_with_variant(
        device: &wgpu::Device,
        _surface_format: wgpu::TextureFormat,
        transparent_enabled: bool,
    ) -> Self {
        let shader_source = shaders::shader_source(transparent_enabled);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source),
        });

        let texture_entries = Self::texture_bind_group_layout_entries(transparent_enabled);
        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Composite Texture BGL"),
            entries: &texture_entries,
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
            transparent_color_handle: None,
            transparent_enabled,
            texture_bind_group: None,
            texture_bind_group_layout: texture_bgl,
            uniform_buffer,
            uniform_bind_group,
            uniform_bind_group_layout: uniform_bgl,
            sampler,
        }
    }
}
