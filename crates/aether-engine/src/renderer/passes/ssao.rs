//! SSAO Pass — Screen-Space Ambient Occlusion
//!
//! Full-screen quad pass. Follows the LearnOpenGL SSAO approach:
//! - View-space Z depth comparison
//! - Hemisphere samples in tangent space, transformed via TBN
//! - 16-sample kernel (unrolled, no dynamic array indexing)
//! - Hash-based per-pixel rotation
//! - Inline bilateral blur removed (needs separate pass)
//!
//! Pipeline: GBufferPass → SSAOPass (→AOTexture) → LightingPass

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

/// SSAO parameters (matches WGSL std140 layout).
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

/// SSAO pass state.
pub struct SSAOPass {
    pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    params_buffer: wgpu::Buffer,
    view_proj_buffer: wgpu::Buffer,
    view_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    view_proj_bind_group: wgpu::BindGroup,
    view_bind_group: wgpu::BindGroup,
    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    texture_bind_group: Option<wgpu::BindGroup>,
    #[allow(dead_code)]
    texture_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    params_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    view_proj_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    view_bind_group_layout: wgpu::BindGroupLayout,
    // Per-frame
    view_proj: glam::Mat4,
    view: glam::Mat4,
    screen_size: [f32; 2],
    // SSAO parameters (mutable via UI)
    radius: f32,
    bias: f32,
    intensity: f32,
}

impl Pass for SSAOPass {
    fn name(&self) -> &str { "SSAO" }

    fn signature(&self) -> PassSignature {
        PassSignature::new("SSAO")
            .read::<GPosition>("gbuffer_position")
            .read::<GNormal>("gbuffer_normal")
            .write::<AOTexture>("ao", wgpu::TextureFormat::R8Unorm)
    }

    fn init(device: &wgpu::Device) -> Self { Self::new(device) }

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

        self.texture_bind_group = Some(device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("SSAO Texture Bind Group"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(pos_view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(norm_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
                ],
            },
        ));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        let proj = frame.camera.projection_matrix(frame.aspect);
        let view = frame.camera.view_matrix();
        self.view = view;
        self.view_proj = proj * view;

        let p = SSAOParams {
            radius: self.radius,
            bias: self.bias,
            intensity: self.intensity,
            _pad0: 0.0,
            screen_size: self.screen_size,
            _pad1: [0.0; 2],
        };
        frame.queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[p]));
        frame.queue.write_buffer(&self.view_proj_buffer, 0, bytemuck::cast_slice(self.view_proj.as_ref()));
        frame.queue.write_buffer(&self.view_buffer, 0, bytemuck::cast_slice(self.view.as_ref()));
    }

    fn execute(&self, encoder: &mut wgpu::CommandEncoder, resources: &ResourceTable, _sv: &wgpu::TextureView) {
        let texture_bg = self.texture_bind_group.as_ref().expect("SSAO: resolve not called");
        let ao_view = resources.get(resources.handle::<AOTexture>("ao"));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SSAO Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ao_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, texture_bg, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        pass.set_bind_group(2, &self.view_proj_bind_group, &[]);
        pass.set_bind_group(3, &self.view_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

impl SSAOPass {
    /// Update screen dimensions (called on init and resize).
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_size = [width as f32, height as f32];
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

@group(1) @binding(0) var<uniform> params: SSAOParams;
@group(2) @binding(0) var<uniform> view_proj: mat4x4<f32>;
@group(3) @binding(0) var<uniform> view_mat: mat4x4<f32>;

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

    // View-space Z (for depth comparison, per LearnOpenGL)
    let view_pos4 = view_mat * vec4<f32>(world_pos, 1.0);
    let frag_view_z = view_pos4.z;
    let view_N4 = view_mat * vec4<f32>(world_N, 0.0);
    let view_N = normalize(view_N4.xyz);

    // Random rotation (hash-based)
    let rot = hash2(uv * 1024.0);
    let rvec = vec3<f32>(rot.x, rot.y, 0.0);

    // TBN in WORLD space for sample direction
    let tangent = normalize(rvec - world_N * dot(rvec, world_N));
    let bitangent = cross(world_N, tangent);

    var occlusion: f32 = 0.0;

    // ── 16-sample hemisphere kernel (unrolled) ───

    // Sample 0
    {
        let ks = vec3<f32>( 0.5381,  0.1856,  0.4319);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 1
    {
        let ks = vec3<f32>( 0.1379,  0.1967,  0.8544);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 2
    {
        let ks = vec3<f32>(-0.3476,  0.4789,  0.5346);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 3
    {
        let ks = vec3<f32>(-0.4791,  0.3456,  0.4765);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 4
    {
        let ks = vec3<f32>( 0.3883, -0.2607,  0.5915);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 5
    {
        let ks = vec3<f32>(-0.4340, -0.4412,  0.5311);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 6
    {
        let ks = vec3<f32>( 0.2380, -0.3947,  0.6678);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 7
    {
        let ks = vec3<f32>(-0.2238, -0.5672,  0.4478);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 8
    {
        let ks = vec3<f32>( 0.3065,  0.0922,  0.1805);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 9
    {
        let ks = vec3<f32>( 0.1244,  0.6114,  0.2616);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 10
    {
        let ks = vec3<f32>( 0.4086,  0.4489,  0.2630);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 11
    {
        let ks = vec3<f32>(-0.4856,  0.3076,  0.1789);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 12
    {
        let ks = vec3<f32>(-0.3801, -0.3943,  0.1389);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 13
    {
        let ks = vec3<f32>(-0.1088, -0.6461,  0.0838);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 14
    {
        let ks = vec3<f32>( 0.2919, -0.4453,  0.3042);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }
    // Sample 15
    {
        let ks = vec3<f32>( 0.3513, -0.1539,  0.0345);
        let sd = tangent * ks.x + bitangent * ks.y + world_N * ks.z;
        let sw = world_pos.xyz + sd * params.radius;
        let sc = view_proj * vec4<f32>(sw, 1.0);
        let suv = vec2<f32>(sc.x / sc.w * 0.5 + 0.5, -sc.y / sc.w * 0.5 + 0.5);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            let occ_world = textureSample(gbuffer_position, gbuffer_sampler, suv);
            let occ_view_z = (view_mat * vec4<f32>(occ_world.xyz, 1.0)).z;
            let sv_view_z = (view_mat * vec4<f32>(sw, 1.0)).z;
            if (occ_view_z >= sv_view_z + params.bias) {
                let r = smoothstep(0.0, 1.0, params.radius / max(abs(frag_view_z - occ_view_z), 0.001));
                occlusion += r;
            }
        }
    }

    // Final AO value (per LearnOpenGL: invert, power)
    occlusion = 1.0 - occlusion / 16.0;
    occlusion = pow(occlusion, params.intensity);

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
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });

        let params_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO Params BGL"),
            entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }],
        });

        let view_proj_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO ViewProj BGL"),
            entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }],
        });

        let view_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO View BGL"),
            entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSAO Pipeline Layout"),
            bind_group_layouts: &[Some(&texture_bgl), Some(&params_bgl), Some(&view_proj_bgl), Some(&view_bgl)],
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
                    attributes: &[wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 }],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::R8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, strip_index_format: None, front_face: wgpu::FrontFace::Ccw, cull_mode: None, polygon_mode: wgpu::PolygonMode::Fill, unclipped_depth: false, conservative: false },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSAO Params"), size: std::mem::size_of::<SSAOParams>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let view_proj_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSAO ViewProj"), size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let view_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSAO View"), size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });

        let params_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO Params BG"), layout: &params_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: params_buffer.as_entire_binding() }],
        });
        let view_proj_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO ViewProj BG"), layout: &view_proj_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: view_proj_buffer.as_entire_binding() }],
        });
        let view_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO View BG"), layout: &view_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: view_buffer.as_entire_binding() }],
        });

        let quad_vertices: [[f32; 2]; 6] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SSAO Quad Vtx"), contents: bytemuck::cast_slice(&quad_vertices), usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline, quad_vertex_buffer, quad_vertex_count: 6,
            params_buffer, view_proj_buffer, view_buffer,
            params_bind_group: params_bg, view_proj_bind_group: view_proj_bg, view_bind_group: view_bg,
            pos_handle: None, normal_handle: None, texture_bind_group: None,
            texture_bind_group_layout: texture_bgl,
            params_bind_group_layout: params_bgl,
            view_proj_bind_group_layout: view_proj_bgl,
            view_bind_group_layout: view_bgl,
            view_proj: glam::Mat4::IDENTITY, view: glam::Mat4::IDENTITY,
            screen_size: [1024.0, 768.0],
            radius: 0.15,
            bias: 0.025,
            intensity: 1.5,
        }
    }

    /// Set SSAO sample radius (world-space units).
    pub fn set_radius(&mut self, radius: f32) {
        self.radius = radius.max(0.001);
    }

    /// Set SSAO depth comparison bias.
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
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("need adapter");
        let (device, _) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("need device");
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
