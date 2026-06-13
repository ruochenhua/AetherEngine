//! SSR Pass — Screen-Space Reflection
//!
//! Full-screen quad pass that traces reflection rays in screen space.
//! Reads G-Buffer and scene color, outputs reflection contribution.
//!
//! Pipeline: LightingPass(→SceneColor) → SSRPass(→ReflectionTexture)
//!           → CompositePass(→swapchain)

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

/// SSR settings (matches WGSL std140 layout).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SSRSettings {
    camera_pos: [f32; 3],
    _pad0: f32,
    view_proj: [[f32; 4]; 4],
    screen_size: [f32; 2],
    _pad1: [f32; 2],
    max_distance: f32,
    linear_steps: f32,
    thickness: f32,
    step_exponent: f32,
    jitter_amount: f32,
    min_roughness: f32,
    max_roughness: f32,
    edge_fade_start: f32,
    edge_fade_end: f32,
    ssr_debug_mode: u32,
    ssr_enabled: u32,
    _pad2: u32,
}

/// SSR pass state.
pub struct SSRPass {
    pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    settings_buffer: wgpu::Buffer,
    settings_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    settings_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    // Handles populated by resolve
    pos_handle: Option<ResHandle<GPosition>>,
    normal_handle: Option<ResHandle<GNormal>>,
    material_handle: Option<ResHandle<GMaterial>>,
    depth_handle: Option<ResHandle<GDepth>>,
    scene_color_handle: Option<ResHandle<SceneColor>>,
    texture_bind_group: Option<wgpu::BindGroup>,
    // Per-frame / mutable
    ssr_debug_mode: u32,
    ssr_enabled: u32,
    screen_size: [f32; 2],
}

impl Pass for SSRPass {
    fn name(&self) -> &str {
        "SSR"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("SSR")
            .read::<GPosition>("gbuffer_position")
            .read::<GNormal>("gbuffer_normal")
            .read::<GMaterial>("gbuffer_material")
            .read::<GDepth>("gbuffer_depth")
            .read::<SceneColor>("scene_color")
            .write::<ReflectionTexture>("reflection", wgpu::TextureFormat::Rgba16Float)
    }

    fn init(device: &wgpu::Device) -> Self {
        Self::new(device)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.pos_handle = Some(resources.handle::<GPosition>("gbuffer_position"));
        self.normal_handle = Some(resources.handle::<GNormal>("gbuffer_normal"));
        self.material_handle = Some(resources.handle::<GMaterial>("gbuffer_material"));
        self.depth_handle = Some(resources.handle::<GDepth>("gbuffer_depth"));
        self.scene_color_handle = Some(resources.handle::<SceneColor>("scene_color"));

        let pos_view = resources.get(self.pos_handle.unwrap());
        let norm_view = resources.get(self.normal_handle.unwrap());
        let material_view = resources.get(self.material_handle.unwrap());
        let depth_view = resources.get(self.depth_handle.unwrap());
        let scene_color_view = resources.get(self.scene_color_handle.unwrap());

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSR Texture Bind Group"),
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
                    resource: wgpu::BindingResource::TextureView(material_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(scene_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        let proj = frame.camera.projection_matrix(frame.aspect);
        let view = frame.camera.view_matrix();
        let view_proj = proj * view;

        let settings = SSRSettings {
            camera_pos: frame.camera.position.into(),
            _pad0: 0.0,
            view_proj: view_proj.to_cols_array_2d(),
            screen_size: self.screen_size,
            _pad1: [0.0; 2],
            max_distance: 20.0,
            linear_steps: 12.0,
            thickness: 0.5,
            step_exponent: 1.0,
            jitter_amount: 1.0,
            min_roughness: 0.08,
            max_roughness: 0.6,
            edge_fade_start: 0.0,
            edge_fade_end: 0.1,
            ssr_debug_mode: self.ssr_debug_mode,
            ssr_enabled: self.ssr_enabled,
            _pad2: 0,
        };
        frame
            .queue
            .write_buffer(&self.settings_buffer, 0, bytemuck::cast_slice(&[settings]));
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
            .expect("SSR: resolve not called");
        let reflection_view = resources.get(resources.handle::<ReflectionTexture>("reflection"));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SSR Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: reflection_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
        pass.set_bind_group(1, &self.settings_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl SSRPass {
    /// Create a new SSR pass.
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

struct SSRSettings {
    camera_pos: vec3<f32>,
    _pad0: f32,
    view_proj: mat4x4<f32>,
    screen_size: vec2<f32>,
    _pad1: vec2<f32>,
    max_distance: f32,
    linear_steps: f32,
    thickness: f32,
    step_exponent: f32,
    jitter_amount: f32,
    min_roughness: f32,
    max_roughness: f32,
    edge_fade_start: f32,
    edge_fade_end: f32,
    ssr_debug_mode: u32,
    ssr_enabled: u32,
    _pad2: u32,
};

@group(0) @binding(0) var gbuffer_position: texture_2d<f32>;
@group(0) @binding(1) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(2) var gbuffer_material: texture_2d<f32>;
@group(0) @binding(3) var gbuffer_depth: texture_2d<f32>;
@group(0) @binding(4) var scene_color: texture_2d<f32>;
@group(0) @binding(5) var tex_sampler: sampler;

@group(1) @binding(0) var<uniform> settings: SSRSettings;

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let h = fract(sin(vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)))) * 43758.5453);
    return h * 2.0 - 1.0;
}

// PCG 2D random number generator
fn pcg2d(v: vec2<u32>) -> vec2<u32> {
    var v_out = v;
    v_out.x = v_out.x * 747796405u + 2891336453u;
    v_out.y = v_out.y * 747796405u + 2891336453u;
    let word = ((v_out.x >> ((v_out.y >> 28u) + 4u)) ^ v_out.y) * 277803737u;
    v_out.x = (word >> 22u) ^ word;
    v_out.y = v_out.x;
    return v_out;
}

// Generate pseudo-random vec2 in [0,1] from screen pixel coord
fn rand2d(uv: vec2<f32>) -> vec2<f32> {
    let px = vec2<u32>(uv * settings.screen_size);
    let h = pcg2d(px);
    return vec2<f32>(h) / 4294967295.0;
}

// Sample the Visible Normal Distribution Function (VNDF) for GGX.
// Based on Heitz 2018 "Sampling the GGX Distribution of Visible Normals".
// V: view vector in tangent space (pointing toward surface)
// alpha: roughness squared
// u1, u2: random numbers in [0,1]
// Returns sampled half-vector H.
fn sample_vndf_ggx(V: vec3<f32>, alpha: f32, u1: f32, u2: f32) -> vec3<f32> {
    // Stretch view
    let Vh = normalize(vec3<f32>(alpha * V.x, alpha * V.y, V.z));

    // Build orthonormal basis around Vh
    let lensq = Vh.x * Vh.x + Vh.y * Vh.y;
    var T1: vec3<f32>;
    if (lensq > 0.0) {
        T1 = vec3<f32>(-Vh.y, Vh.x, 0.0) / sqrt(lensq);
    } else {
        T1 = vec3<f32>(1.0, 0.0, 0.0);
    }
    let T2 = cross(Vh, T1);

    // Sample point in disk
    let r = sqrt(u1);
    let phi = 2.0 * 3.14159265359 * u2;
    var t1 = r * cos(phi);
    var t2 = r * sin(phi);
    let s = 0.5 * (1.0 + Vh.z);
    t2 = mix(sqrt(max(0.0, 1.0 - t1 * t1)) * t2, s * t2, s);

    // Reproject onto hemisphere
    let Nh = t1 * T1 + t2 * T2 + sqrt(max(0.0, 1.0 - t1 * t1 - t2 * t2)) * Vh;

    // Unstretch and normalize
    return normalize(vec3<f32>(alpha * Nh.x, alpha * Nh.y, max(0.0, Nh.z)));
}

// Screen-space ray march following the article approach.
// Returns (hit, hit_uv.x, hit_uv.y, steps_taken_ratio).
fn ray_march(world_pos: vec3<f32>, rd: vec3<f32>, uv: vec2<f32>) -> vec4<f32> {
    // Offset start point to avoid self-intersection.
    // Dynamic offset: larger for distant surfaces (grazing angle self-intersection).
    let dist_to_camera = length(world_pos - settings.camera_pos);
    let start_offset = max(0.5, dist_to_camera * 0.03);
    let start_pos = world_pos + rd * start_offset;
    let end_pos = world_pos + rd * settings.max_distance;

    var start_clip = settings.view_proj * vec4<f32>(start_pos, 1.0);
    var end_clip = settings.view_proj * vec4<f32>(end_pos, 1.0);

    // Clip ray to camera front plane in clip space.
    // If both endpoints are behind the camera, the ray can't produce a valid reflection.
    let epsilon = 0.01;
    if (start_clip.w < epsilon && end_clip.w < epsilon) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    if (start_clip.w < epsilon) {
        let t = (epsilon - start_clip.w) / (end_clip.w - start_clip.w);
        start_clip = mix(start_clip, end_clip, t);
    }
    if (end_clip.w < epsilon) {
        let t = (epsilon - start_clip.w) / (end_clip.w - start_clip.w);
        end_clip = mix(start_clip, end_clip, t);
    }

    let start_ndc = start_clip.xyz / start_clip.w;
    let end_ndc = end_clip.xyz / end_clip.w;

    // Clip NDC line to view frustum [-1,1]×[-1,1]×[0,1]
    let ndc_delta = end_ndc - start_ndc;
    var t_min = 0.0;
    var t_max = 1.0;

    // X bounds [-1, 1]
    if (abs(ndc_delta.x) > 1e-6) {
        let tx1 = (-1.0 - start_ndc.x) / ndc_delta.x;
        let tx2 = (1.0 - start_ndc.x) / ndc_delta.x;
        t_min = max(t_min, min(tx1, tx2));
        t_max = min(t_max, max(tx1, tx2));
    } else if (start_ndc.x < -1.0 || start_ndc.x > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Y bounds [-1, 1]
    if (abs(ndc_delta.y) > 1e-6) {
        let ty1 = (-1.0 - start_ndc.y) / ndc_delta.y;
        let ty2 = (1.0 - start_ndc.y) / ndc_delta.y;
        t_min = max(t_min, min(ty1, ty2));
        t_max = min(t_max, max(ty1, ty2));
    } else if (start_ndc.y < -1.0 || start_ndc.y > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Z bounds [0, 1]
    if (abs(ndc_delta.z) > 1e-6) {
        let tz1 = (0.0 - start_ndc.z) / ndc_delta.z;
        let tz2 = (1.0 - start_ndc.z) / ndc_delta.z;
        t_min = max(t_min, min(tz1, tz2));
        t_max = min(t_max, max(tz1, tz2));
    } else if (start_ndc.z < 0.0 || start_ndc.z > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    if (t_min > t_max) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let clipped_start_ndc = start_ndc + ndc_delta * t_min;
    let clipped_end_ndc = start_ndc + ndc_delta * t_max;

    // Convert NDC to screen space (pixel coordinates).
    // wgpu NDC: (-1,-1) = bottom-left. textureLoad (0,0) = top-left.
    let start_screen = vec3<f32>(
        (clipped_start_ndc.x + 1.0) * 0.5 * settings.screen_size.x,
        (1.0 - clipped_start_ndc.y) * 0.5 * settings.screen_size.y,
        clipped_start_ndc.z
    );
    let end_screen = vec3<f32>(
        (clipped_end_ndc.x + 1.0) * 0.5 * settings.screen_size.x,
        (1.0 - clipped_end_ndc.y) * 0.5 * settings.screen_size.y,
        clipped_end_ndc.z
    );

    let screen_diff = end_screen - start_screen;
    let pixel_dist = max(abs(screen_diff.x), abs(screen_diff.y));
    // One sample per pixel for best quality (can reduce to *0.5 for performance)
    var sample_count = i32(pixel_dist);
    sample_count = min(sample_count, 64);
    sample_count = max(sample_count, 2);

    var last_screen = start_screen;
    var last_t = 0.0;
    var hit = 0.0;
    var hit_uv = vec2<f32>(0.0);
    var steps_taken = 0.0;

    let dims = vec2<i32>(settings.screen_size);
    let jitter_val = settings.jitter_amount * (rand2d(uv).x - 0.5);

    for (var i = 1; i <= sample_count; i++) {
        let raw_t = (f32(i) + jitter_val) / f32(sample_count);
        let current_t = pow(clamp(raw_t, 0.0, 1.0), settings.step_exponent);
        let current_screen = start_screen + screen_diff * current_t;
        steps_taken = f32(i) - 1.0;

        let px = vec2<i32>(current_screen.xy);
        let clamped_px = clamp(px, vec2<i32>(0), dims - vec2<i32>(1));

        // Sample scene world position from GBuffer
        let scene_pos = textureLoad(gbuffer_position, clamped_px, 0).xyz;

        // Skip empty pixels (sky / background)
        if (length(scene_pos) < 0.0001) {
            last_screen = current_screen;
            last_t = current_t;
            continue;
        }

        // Reproject scene position to NDC for depth comparison
        let scene_clip = settings.view_proj * vec4<f32>(scene_pos, 1.0);
        let scene_ndc = scene_clip.xyz / scene_clip.w;

        let ray_ndc_z = mix(clipped_start_ndc.z, clipped_end_ndc.z, current_t);
        let last_ray_z = last_screen.z;
        let scene_z = scene_ndc.z;

        // Intersection: ray crosses the scene surface (last and current on opposite sides).
        let z_tolerance = 0.002;
        if ((last_ray_z > scene_z + z_tolerance && ray_ndc_z <= scene_z + z_tolerance) ||
            (last_ray_z < scene_z - z_tolerance && ray_ndc_z >= scene_z - z_tolerance)) {

            // Binary search refinement in screen-space t
            var t0 = last_t;
            var t1 = current_t;
            var s0 = last_screen;
            var s1 = current_screen;

            for (var b = 0; b < 8; b++) {
                let tm = (t0 + t1) * 0.5;
                let sm = start_screen + screen_diff * tm;
                let spx = vec2<i32>(sm.xy);
                let scl = clamp(spx, vec2<i32>(0), dims - vec2<i32>(1));
                let smp = textureLoad(gbuffer_position, scl, 0).xyz;

                if (length(smp) < 0.0001) {
                    t0 = tm;
                    s0 = sm;
                    continue;
                }

                let smc = settings.view_proj * vec4<f32>(smp, 1.0);
                let smn = smc.xyz / smc.w;
                let smn_z = smn.z;
                let rmz = mix(clipped_start_ndc.z, clipped_end_ndc.z, tm);

                // Check if [t0, tm] intersects (with same tolerance)
                if ((s0.z > smn_z + z_tolerance && rmz <= smn_z + z_tolerance) ||
                    (s0.z < smn_z - z_tolerance && rmz >= smn_z - z_tolerance)) {
                    t1 = tm;
                    s1 = sm;
                } else {
                    t0 = tm;
                    s0 = sm;
                }
            }

            let final_t = (t0 + t1) * 0.5;
            let final_screen = start_screen + screen_diff * final_t;
            hit_uv = final_screen.xy / settings.screen_size;

            // Outlier rejection: verify the hit is consistent with the depth buffer.
            // Large NDC-Z discrepancy means the hit was triggered by oversized tolerance
            // on a distant virtual surface, not a real intersection.
            let hit_px = vec2<i32>(hit_uv * settings.screen_size);
            let hit_clamped_px = clamp(hit_px, vec2<i32>(0), dims - vec2<i32>(1));
            let hit_depth = textureLoad(gbuffer_depth, hit_clamped_px, 0).r;
            let expected_ndc_z = mix(clipped_start_ndc.z, clipped_end_ndc.z, final_t);
            if (abs(hit_depth - expected_ndc_z) < 0.02) {
                hit = 1.0;
                break;
            }
            // Inconsistent: treat as miss and keep marching
            hit = 0.0;
        }

        last_screen = current_screen;
        last_t = current_t;
    }

    return vec4<f32>(hit, hit_uv.x, hit_uv.y, steps_taken / f32(sample_count));
}

// Simple 2D hash for deterministic per-pixel noise (no temporal flicker)
fn hash22(p: vec2<f32>) -> vec2<f32> {
    var p3 = fract(vec3<f32>(p.xyx) * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.xx + p3.yz) * p3.zy);
}

// Screen-edge fade factor
fn edge_fade(uv: vec2<f32>) -> f32 {
    let edge_dist = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    return smoothstep(settings.edge_fade_start, settings.edge_fade_end, edge_dist);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let pos_sample = textureSample(gbuffer_position, tex_sampler, uv);
    let norm_sample = textureSample(gbuffer_normal, tex_sampler, uv);
    let material_sample = textureSample(gbuffer_material, tex_sampler, uv);

    // SSR toggle
    if (settings.ssr_enabled == 0u) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Sky check: GBuffer normal is (0,0,0) after clear
    if (norm_sample.r == 0.0 && norm_sample.g == 0.0 && norm_sample.b == 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let world_pos = pos_sample.xyz;
    let N = normalize(norm_sample.xyz * 2.0 - 1.0);
    let roughness = material_sample.r;
    let metallic = material_sample.g;

    let view_dir = normalize(world_pos - settings.camera_pos);

    // Roughness fade: only process low-roughness surfaces
    let roughness_factor = 1.0 - smoothstep(settings.min_roughness, settings.max_roughness, roughness);

    if (roughness_factor <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Perturb normal based on roughness to break perfect mirror stripes
    let noise = hash22(uv * 43758.5453);
    let N_perturbed = normalize(N + vec3<f32>(noise.x - 0.5, noise.y - 0.5, 0.0) * roughness * 0.5);
    let rd = normalize(reflect(view_dir, N_perturbed));

    // Debug modes from slice 2
    if (settings.ssr_debug_mode == 1u) {
        return vec4<f32>(rd * 0.5 + 0.5, 1.0);
    }
    if (settings.ssr_debug_mode == 2u) {
        return vec4<f32>(normalize(settings.camera_pos - world_pos) * 0.5 + 0.5, 1.0);
    }
    if (settings.ssr_debug_mode == 3u) {
        return vec4<f32>(vec3<f32>(roughness), 1.0);
    }
    if (settings.ssr_debug_mode == 4u) {
        return vec4<f32>(vec3<f32>(metallic), 1.0);
    }

    // Screen-space ray march
    let result = ray_march(world_pos, rd, uv);
    var hit = result.x;
    let hit_uv = result.yz;
    let step_norm = result.w;

    // Reject self-intersections: if the hit point is too close in screen-space,
    // it's likely the same surface (or adjacent pixel) and should be discarded.
    // World-space threshold fails for distant surfaces (adjacent pixels can be >1.5m apart),
    // so we use screen-space distance which is uniform regardless of depth.
    if (hit > 0.5) {
        let screen_dist_px = length(hit_uv - uv) * min(settings.screen_size.x, settings.screen_size.y);
        if (screen_dist_px < 12.0) {
            hit = 0.0;
        }
    }

    // Debug modes from slice 3
    if (settings.ssr_debug_mode == 5u) {
        // Hit UV visualization
        if (hit > 0.5) {
            return vec4<f32>(hit_uv.x, hit_uv.y, 0.0, 1.0);
        } else {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
    }
    if (settings.ssr_debug_mode == 6u) {
        // Hit/Miss binary
        let color = select(vec3<f32>(0.0), vec3<f32>(1.0), hit > 0.5);
        return vec4<f32>(color, 1.0);
    }
    if (settings.ssr_debug_mode == 7u) {
        // Step count heatmap
        let heat = step_norm;
        return vec4<f32>(heat, heat * 0.5, 1.0 - heat, 1.0);
    }
    if (settings.ssr_debug_mode == 8u) {
        // Ray endpoint UV
        let ray_end = world_pos + rd * settings.max_distance;
        let cs_end = settings.view_proj * vec4<f32>(ray_end, 1.0);
        let ndc_end = cs_end.xyz / cs_end.w;
        var uv_end = ndc_end.xy * 0.5 + 0.5;
        uv_end.y = 1.0 - uv_end.y;
        return vec4<f32>(uv_end.x, uv_end.y, 0.0, 1.0);
    }

    // Slice 4: color sampling and compositing
    if (hit > 0.5) {
        let edge_factor = edge_fade(hit_uv);
        let ssr_color = textureSample(scene_color, tex_sampler, hit_uv).rgb;
        let alpha = hit * edge_factor * roughness_factor;
        return vec4<f32>(ssr_color, alpha);
    }

    // No hit: output transparent
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSR Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSR Texture BGL"),
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
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
            ],
        });

        let settings_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSR Settings BGL"),
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
            label: Some("SSR Pipeline Layout"),
            bind_group_layouts: &[Some(&texture_bgl), Some(&settings_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SSR Pipeline"),
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

        let settings_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSR Settings Buffer"),
            size: std::mem::size_of::<SSRSettings>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let settings_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSR Settings BG"),
            layout: &settings_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: settings_buffer.as_entire_binding(),
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
            label: Some("SSR Quad Vtx"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SSR Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            settings_buffer,
            settings_bind_group,
            texture_bind_group_layout: texture_bgl,
            settings_bind_group_layout: settings_bgl,
            sampler,
            pos_handle: None,
            normal_handle: None,
            material_handle: None,
            depth_handle: None,
            scene_color_handle: None,
            texture_bind_group: None,
            ssr_debug_mode: 0,
            ssr_enabled: 1,
            screen_size: [1280.0, 720.0],
        }
    }

    /// Set SSR debug visualization mode.
    pub fn set_debug_mode(&mut self, mode: u32) {
        self.ssr_debug_mode = mode;
    }

    /// Enable or disable SSR.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.ssr_enabled = if enabled { 1 } else { 0 };
    }

    /// Update screen dimensions.
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_size = [width as f32, height as f32];
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
    fn signature_declares_correct_resources() {
        let device = headless_device();
        let sig = SSRPass::new(&device).signature();
        assert_eq!(sig.name, "SSR");
        assert_eq!(sig.reads.len(), 5);
        assert_eq!(sig.writes.len(), 1);
        assert!(
            sig.writes[0].type_id == TypeId::of::<ReflectionTexture>()
                && sig.writes[0].name == "reflection"
        );
    }

    #[test]
    fn init_creates_resources() {
        let _pass = SSRPass::new(&headless_device());
    }
}
