//! G-Buffer Pass
//!
//! Uses dynamic uniform offsets for per-object data. All transforms + materials
//! are uploaded to a single buffer before the render pass, avoiding
//! in-pass write synchronization issues.

use crate::asset::mesh::{GpuMesh, Vertex};
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use glam::Mat4;
use std::sync::Arc;

/// Per-object uniform (model + material), 256-byte aligned for dynamic offset.
#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct ObjectUniform {
    pub model: [[f32; 4]; 4],
    pub albedo: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub _pad: [u8; 168], // fill to exactly 256 bytes
}

// Safety: ObjectUniform is #[repr(C, align(256))] with no invalid bit patterns
unsafe impl bytemuck::Pod for ObjectUniform {}
unsafe impl bytemuck::Zeroable for ObjectUniform {}

/// View-projection uniform (shared).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewProjUniform {
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform { pub albedo: [f32; 4], pub roughness: f32, pub metallic: f32, pub _pad: [f32; 2], }

impl Default for MaterialUniform {
    fn default() -> Self { Self { albedo: [0.8, 0.3, 0.2, 1.0], roughness: 0.5, metallic: 0.0, _pad: [0.0, 0.0] } }
}

#[derive(Clone)]
pub struct Renderable {
    pub mesh: Arc<GpuMesh>,
    pub transform: Mat4,
    pub material: MaterialUniform,
}

pub struct GBufferPass {
    pipeline: wgpu::RenderPipeline,
    view_proj_buffer: wgpu::Buffer,
    view_proj_bind_group: wgpu::BindGroup,
    /// Per-object uniform buffer (dynamic).
    object_buffer: wgpu::Buffer,
    object_bind_group: wgpu::BindGroup,

    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    albedo_handle: Option<ResHandle<GAlbedo>>,
    material_handle: Option<ResHandle<GMaterial>>,
    depth_handle: Option<ResHandle<GDepth>>,

    renderables: Vec<Renderable>,
    view: Mat4,
    proj: Mat4,
}

impl Pass for GBufferPass {
    fn name(&self) -> &str { "GBuffer" }

    fn signature(&self) -> PassSignature {
        PassSignature::new("GBuffer")
            .write::<GPosition>("gbuffer_position", wgpu::TextureFormat::Rgba16Float)
            .write::<GNormal>("gbuffer_normal", wgpu::TextureFormat::Rgba16Float)
            .write::<GAlbedo>("gbuffer_albedo", wgpu::TextureFormat::Rgba8Unorm)
            .write::<GMaterial>("gbuffer_material", wgpu::TextureFormat::Rg8Unorm)
            .write::<GDepth>("gbuffer_depth", wgpu::TextureFormat::Depth32Float)
    }

    fn init(device: &wgpu::Device) -> Self { Self::new(device) }

    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.pos_handle = Some(resources.handle::<GPosition>("gbuffer_position"));
        self.normal_handle = Some(resources.handle::<GNormal>("gbuffer_normal"));
        self.albedo_handle = Some(resources.handle::<GAlbedo>("gbuffer_albedo"));
        self.material_handle = Some(resources.handle::<GMaterial>("gbuffer_material"));
        self.depth_handle = Some(resources.handle::<GDepth>("gbuffer_depth"));
    }

    fn execute(&self, encoder: &mut wgpu::CommandEncoder, resources: &ResourceTable, queue: &wgpu::Queue, _surface_view: &wgpu::TextureView) {
        // Upload view/proj
        let vp = ViewProjUniform { view: self.view.to_cols_array_2d(), proj: self.proj.to_cols_array_2d() };
        queue.write_buffer(&self.view_proj_buffer, 0, bytemuck::cast_slice(&[vp]));

        // Upload all per-object data at once (before render pass)
        let obj_size = std::mem::size_of::<ObjectUniform>() as wgpu::BufferAddress;
        let mut obj_data: Vec<u8> = Vec::with_capacity(self.renderables.len() * obj_size as usize);
        for r in &self.renderables {
            let obj = ObjectUniform {
                model: r.transform.to_cols_array_2d(),
                albedo: r.material.albedo,
                roughness: r.material.roughness,
                metallic: r.material.metallic,
                _pad: [0u8; 168],
            };
            obj_data.extend_from_slice(bytemuck::cast_slice(&[obj]));
        }
        // Recreate object buffer if needed
        // For now, the buffer is pre-allocated in new() with MAX_OBJECTS * obj_size
        if !obj_data.is_empty() {
            queue.write_buffer(&self.object_buffer, 0, &obj_data);
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GBuffer"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment { view: resources.get(self.pos_handle.unwrap()), resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } }),
                Some(wgpu::RenderPassColorAttachment { view: resources.get(self.normal_handle.unwrap()), resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } }),
                Some(wgpu::RenderPassColorAttachment { view: resources.get(self.albedo_handle.unwrap()), resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } }),
                Some(wgpu::RenderPassColorAttachment { view: resources.get(self.material_handle.unwrap()), resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: resources.get(self.depth_handle.unwrap()),
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None, occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.view_proj_bind_group, &[]);

        for (i, renderable) in self.renderables.iter().enumerate() {
            let offset = i as wgpu::DynamicOffset * obj_size as wgpu::DynamicOffset;
            pass.set_bind_group(1, &self.object_bind_group, &[offset as u32]);

            pass.set_vertex_buffer(0, renderable.mesh.vertex_buffer.slice(..));
            if let Some(ref ib) = renderable.mesh.index_buffer {
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..renderable.mesh.index_count, 0, 0..1);
            } else {
                pass.draw(0..renderable.mesh.vertex_count, 0..1);
            }
        }
    }
}

const MAX_OBJECTS: usize = 256;

impl GBufferPass {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader_source = r#"
struct VertexInput { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) tangent: vec4<f32>, };
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) world_pos: vec3<f32>, @location(1) world_normal: vec3<f32>, @location(2) uv: vec2<f32>, };
struct ViewProjUniform { view: mat4x4<f32>, proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> vp: ViewProjUniform;

struct ObjectData { model: mat4x4<f32>, albedo: vec4<f32>, roughness: f32, metallic: f32, };
@group(1) @binding(0) var<uniform> obj: ObjectData;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = obj.model * vec4<f32>(in.position, 1.0);
    out.clip_position = vp.proj * vp.view * world_pos;
    out.world_pos = world_pos.xyz;
    let nm = mat3x3<f32>(obj.model[0].xyz, obj.model[1].xyz, obj.model[2].xyz);
    out.world_normal = normalize(nm * in.normal);
    out.uv = in.uv;
    return out;
}

struct FragmentOutput { @location(0) position: vec4<f32>, @location(1) normal: vec4<f32>, @location(2) albedo: vec4<f32>, @location(3) material: vec2<f32>, }
@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    out.position = vec4<f32>(in.world_pos, 1.0);
    out.normal = vec4<f32>(in.world_normal * 0.5 + 0.5, 1.0);
    out.albedo = obj.albedo;
    out.material = vec2<f32>(obj.roughness, obj.metallic);
    return out;
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GBuffer Shader"), source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let vp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GBuffer VP BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });

        let obj_size = std::mem::size_of::<ObjectUniform>() as u64;
        let obj_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GBuffer Obj BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: Some(std::num::NonZeroU64::new(obj_size).unwrap()) },
                count: None,
            }],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GBuffer PL"), bind_group_layouts: &[&vp_bgl, &obj_bgl], push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GBuffer Pipeline"), layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[Vertex::desc()] },
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: "fs_main",
                targets: &[
                    Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba16Float, blend: None, write_mask: wgpu::ColorWrites::ALL }),
                    Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba16Float, blend: None, write_mask: wgpu::ColorWrites::ALL }),
                    Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL }),
                    Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rg8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL }),
                ],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, strip_index_format: None, front_face: wgpu::FrontFace::Ccw, cull_mode: Some(wgpu::Face::Back), polygon_mode: wgpu::PolygonMode::Fill, unclipped_depth: false, conservative: false },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState::default() }),
            multisample: wgpu::MultisampleState::default(), multiview: None,
        });

        let view_proj_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GBuffer VP Buf"), size: std::mem::size_of::<ViewProjUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let view_proj_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GBuffer VP BG"), layout: &vp_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: view_proj_buffer.as_entire_binding() }],
        });

        let obj_buf_size = (MAX_OBJECTS as u64) * obj_size;
        let object_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GBuffer Obj Buf"), size: obj_buf_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let object_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GBuffer Obj BG"), layout: &obj_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &object_buffer, offset: 0, size: Some(std::num::NonZeroU64::new(obj_size).unwrap()) }) }],
        });

        Self {
            pipeline, view_proj_buffer, view_proj_bind_group, object_buffer, object_bind_group,
            pos_handle: None, normal_handle: None, albedo_handle: None, material_handle: None, depth_handle: None,
            renderables: Vec::new(), view: Mat4::IDENTITY, proj: Mat4::IDENTITY,
        }
    }

    pub fn set_frame_data(&mut self, renderables: &[Renderable], view: Mat4, proj: Mat4) {
        self.renderables = renderables.to_vec();
        self.view = view;
        self.proj = proj;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::pass::SlotKind;

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("need adapter");
        let (device, _queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).expect("need device");
        device
    }

    #[test] fn signature_ok() {
        let pass = GBufferPass::init(&headless_device());
        let sig = pass.signature();
        assert_eq!(sig.writes.len(), 5);
    }

    #[test] fn resolve_ok() {
        let device = headless_device();
        let mut pass = GBufferPass::init(&device);
        let mut table = ResourceTable::new();
        for (type_id, name, fmt) in [
            (std::any::TypeId::of::<GPosition>(), "gbuffer_position", wgpu::TextureFormat::Rgba16Float),
            (std::any::TypeId::of::<GNormal>(), "gbuffer_normal", wgpu::TextureFormat::Rgba16Float),
            (std::any::TypeId::of::<GAlbedo>(), "gbuffer_albedo", wgpu::TextureFormat::Rgba8Unorm),
            (std::any::TypeId::of::<GMaterial>(), "gbuffer_material", wgpu::TextureFormat::Rg8Unorm),
            (std::any::TypeId::of::<GDepth>(), "gbuffer_depth", wgpu::TextureFormat::Depth32Float),
        ] {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(name), size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
                mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: fmt,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
            });
            table.allocate(type_id, name, tex.create_view(&wgpu::TextureViewDescriptor::default()));
        }
        pass.resolve(&device, &table);
        assert!(pass.pos_handle.is_some());
    }
}
