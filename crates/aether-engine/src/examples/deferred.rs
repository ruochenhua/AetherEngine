//! 02_deferred – Deferred Shading with Lighting + Debug Overlay
//!
//! Renders a cube and sphere with Blinn-Phong lighting.
//! UE-style fly camera, debug grid, and origin gizmo.

use crate::{
    asset::mesh::{CpuMesh, GpuMesh},
    examples::{Example, ExampleStateSnapshot},
    input::InputManager,
    renderer::{
        camera::FlyCamera,
        context::{GBuffer, RenderContext},
        passes::{
            debug::{build_gizmo_lines, build_grid_lines, DebugLinePass},
            gbuffer::{GBufferPass, MaterialUniform, Renderable},
            lighting::{DirectionalLight, LightingPass, LightingUniforms},
        },
    },
};
use glam::{Mat4, Vec3};
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Deferred shading example: cube + sphere with Blinn-Phong lighting.
pub struct DeferredExample {
    gbuffer_pass: Option<GBufferPass>,
    lighting_pass: Option<LightingPass>,
    debug_pass: Option<DebugLinePass>,
    gbuffer: Option<GBuffer>,
    renderables: Option<Vec<Renderable>>,

    // Debug geometry
    grid_vertex_buffer: Option<wgpu::Buffer>,
    grid_vertex_count: u32,
    gizmo_vertex_buffer: Option<wgpu::Buffer>,
    gizmo_vertex_count: u32,

    fly_cam: Option<FlyCamera>,
    proj: Option<Mat4>,
    aspect: f32,

    lighting_uniforms: Option<LightingUniforms>,

    last_frame_time: std::time::Instant,
    fps: f32,
    scroll_input: f32, // accumulated scroll for this frame
}

impl DeferredExample {
    pub fn new() -> Self {
        Self {
            gbuffer_pass: None,
            lighting_pass: None,
            debug_pass: None,
            gbuffer: None,
            renderables: None,
            grid_vertex_buffer: None,
            grid_vertex_count: 0,
            gizmo_vertex_buffer: None,
            gizmo_vertex_count: 0,
            fly_cam: None,
            proj: None,
            aspect: 1280.0 / 720.0,
            lighting_uniforms: None,
            last_frame_time: std::time::Instant::now(),
            fps: 0.0,
            scroll_input: 0.0,
        }
    }
}

impl Default for DeferredExample {
    fn default() -> Self {
        Self::new()
    }
}

impl Example for DeferredExample {
    fn name(&self) -> &'static str {
        "02_deferred"
    }

    fn description(&self) -> &'static str {
        "Deferred shading: cube + sphere with Blinn-Phong lighting, fly cam."
    }

    fn init(&mut self, ctx: &RenderContext) -> anyhow::Result<()> {
        let gbuffer_pass = GBufferPass::new(&ctx.device);
        let gbuffer = GBuffer::new(&ctx.device, ctx.config.width, ctx.config.height);
        let lighting_pass = LightingPass::new(&ctx.device, &gbuffer, &ctx.config);
        let debug_pass = DebugLinePass::new(
            &ctx.device,
            ctx.config.format,
            wgpu::TextureFormat::Depth32Float,
        );

        // --- Meshes ---
        let cube_cpu = CpuMesh::cube();
        let sphere_cpu = CpuMesh::sphere(32);
        let cube_gpu = GpuMesh::from_cpu(&ctx.device, &cube_cpu);
        let sphere_gpu = GpuMesh::from_cpu(&ctx.device, &sphere_cpu);

        let renderables = vec![
            Renderable {
                mesh: Arc::new(cube_gpu),
                transform: Mat4::from_translation(Vec3::new(-0.8, 0.0, 0.0)),
                material: MaterialUniform {
                    albedo: [0.8, 0.3, 0.2, 1.0],
                    roughness: 0.5,
                    metallic: 0.0,
                    _pad: [0.0, 0.0],
                },
            },
            Renderable {
                mesh: Arc::new(sphere_gpu),
                transform: Mat4::from_translation(Vec3::new(0.8, 0.0, 0.0)),
                material: MaterialUniform {
                    albedo: [0.2, 0.5, 0.8, 1.0],
                    roughness: 0.05,
                    metallic: 0.0,
                    _pad: [0.0, 0.0],
                },
            },
        ];

        // --- Debug geometry ---
        let (grid_verts, grid_count) = build_grid_lines(5.0, 1.0);
        let grid_vb = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Debug Grid VB"),
            contents: bytemuck::cast_slice(&grid_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let (gizmo_verts, gizmo_count) = build_gizmo_lines(0.15);
        let gizmo_vb = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Debug Gizmo VB"),
            contents: bytemuck::cast_slice(&gizmo_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // --- Camera ---
        let fly_cam = FlyCamera::default();
        self.aspect = ctx.config.width as f32 / ctx.config.height as f32;
        self.proj = Some(fly_cam.projection_matrix(self.aspect));

        // --- Lighting ---
        let lighting_uniforms = LightingUniforms {
            camera_pos: fly_cam.position.into(),
            light: DirectionalLight {
                direction: [0.0, -1.0, 0.0],
                _pad: 0.0,
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
            },
            ambient_intensity: 0.05,
            ..Default::default()
        };
        lighting_pass.update_uniforms(&ctx.queue, &lighting_uniforms);

        self.gbuffer_pass = Some(gbuffer_pass);
        self.lighting_pass = Some(lighting_pass);
        self.debug_pass = Some(debug_pass);
        self.gbuffer = Some(gbuffer);
        self.renderables = Some(renderables);
        self.grid_vertex_buffer = Some(grid_vb);
        self.grid_vertex_count = grid_count;
        self.gizmo_vertex_buffer = Some(gizmo_vb);
        self.gizmo_vertex_count = gizmo_count;
        self.fly_cam = Some(fly_cam);
        self.lighting_uniforms = Some(lighting_uniforms);
        self.last_frame_time = std::time::Instant::now();

        Ok(())
    }

    fn cleanup(&mut self, _ctx: &RenderContext) {
        self.gbuffer_pass = None;
        self.lighting_pass = None;
        self.debug_pass = None;
        self.gbuffer = None;
        self.renderables = None;
        self.grid_vertex_buffer = None;
        self.gizmo_vertex_buffer = None;
        self.fly_cam = None;
        self.proj = None;
        self.lighting_uniforms = None;
    }

    fn update(&mut self, _ctx: &RenderContext, dt: f32, input: &InputManager) {
        let cam = self.fly_cam.as_mut().unwrap();
        let uniforms = self.lighting_uniforms.as_mut().unwrap();

        // Debug visualisation toggles
        if input.key_pressed(winit::keyboard::KeyCode::Digit0) {
            uniforms.debug_mode = 0;
        }
        if input.key_pressed(winit::keyboard::KeyCode::Digit1) {
            uniforms.debug_mode = 1;
        }
        if input.key_pressed(winit::keyboard::KeyCode::Digit2) {
            uniforms.debug_mode = 2;
        }
        if input.key_pressed(winit::keyboard::KeyCode::Digit3) {
            uniforms.debug_mode = 3;
        }
        if input.key_pressed(winit::keyboard::KeyCode::Digit4) {
            uniforms.debug_mode = 4;
        }
        if input.key_pressed(winit::keyboard::KeyCode::Digit5) {
            uniforms.debug_mode = 5;
        }

        // Fly camera update
        let (dx, dy) = input.mouse_delta();
        cam.update(dt, dx, dy, self.scroll_input, input);
        self.scroll_input = 0.0;
    }

    fn prepare(&mut self, ctx: &RenderContext) {
        let cam = self.fly_cam.as_ref().unwrap();
        let uniforms = self.lighting_uniforms.as_mut().unwrap();
        uniforms.camera_pos = cam.position.into();

        self.lighting_pass
            .as_ref()
            .unwrap()
            .update_uniforms(&ctx.queue, uniforms);

        // Update debug pass uniform
        let proj = cam.projection_matrix(self.aspect);
        let view = cam.view_matrix();
        let view_proj = proj * view;
        self.proj = Some(proj);

        self.debug_pass
            .as_ref()
            .unwrap()
            .update_uniform(&ctx.queue, &view_proj);
    }

    fn render(
        &mut self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
    ) -> anyhow::Result<()> {
        let gbuffer = self.gbuffer.as_ref().unwrap();
        let renderables = self.renderables.as_ref().unwrap();
        let proj = self.proj.as_ref().unwrap();
        let cam = self.fly_cam.as_ref().unwrap();
        let view = cam.view_matrix();
        let depth_view = &gbuffer.depth;

        // 1. G-Buffer pass
        self.gbuffer_pass
            .as_ref()
            .unwrap()
            .execute(encoder, gbuffer, ctx, renderables, &view, proj);

        // 2. Lighting pass → swapchain (no depth, fullscreen quad)
        self.lighting_pass
            .as_ref()
            .unwrap()
            .execute(encoder, target_view);

        // 3. Debug lines (grid + gizmo) on top, depth-tested against G-Buffer
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Debug Line Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let debug = self.debug_pass.as_ref().unwrap();
            debug.draw(
                &mut pass,
                self.grid_vertex_buffer.as_ref().unwrap(),
                self.grid_vertex_count,
            );
            debug.draw(
                &mut pass,
                self.gizmo_vertex_buffer.as_ref().unwrap(),
                self.gizmo_vertex_count,
            );
        }

        Ok(())
    }

    fn ui(&mut self, egui_ctx: &egui::Context) {
        // Accumulate scroll for next frame's camera update
        self.scroll_input += egui_ctx.input(|i| i.smooth_scroll_delta.y * 0.01);

        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;
        let frame_time_ms = dt * 1000.0;
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
        }

        egui::Window::new("Aether Debug")
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
            .show(egui_ctx, |ui| {
                let mode_names = [
                    "Full", "Ambient", "Diffuse", "Specular", "Normals", "NdotL",
                ];
                let mode = self
                    .lighting_uniforms
                    .as_ref()
                    .map(|u| u.debug_mode as usize)
                    .unwrap_or(0);

                // Debug mode
                ui.label(egui::RichText::new("Lighting Debug").weak());
                ui.label(format!(
                    "  [0-5]: {}",
                    mode_names.get(mode).unwrap_or(&"?")
                ));

                // Camera controls
                ui.separator();
                ui.label(egui::RichText::new("Fly Camera").weak());
                if let Some(ref cam) = self.fly_cam {
                    let status = if cam.active { "◉ ACTIVE" } else { "○ IDLE" };
                    ui.label(format!("  {}", status));
                    ui.label(format!("  Speed: {:.1} u/s", cam.speed));
                    let p = cam.position;
                    ui.label(format!("  Pos: ({:.1}, {:.1}, {:.1})", p.x, p.y, p.z));
                }
                ui.label("  [RMB] Toggle fly mode");
                ui.label("  [WASD] Move  [QE] Up/Down");
                ui.label("  [Scroll] Adjust speed");

                // Stats
                ui.separator();
                ui.label(format!("FPS: {:.1}", self.fps));
                ui.label(format!("Frame: {:.2} ms", frame_time_ms));
            });
    }

    fn resize(&mut self, ctx: &RenderContext, width: u32, height: u32) {
        self.aspect = width as f32 / height as f32;

        let gbuffer = GBuffer::new(&ctx.device, width, height);
        self.lighting_pass
            .as_mut()
            .unwrap()
            .recreate_bind_group(&ctx.device, &gbuffer);
        self.gbuffer = Some(gbuffer);
    }

    fn snapshot(&self) -> Option<ExampleStateSnapshot> {
        Some(ExampleStateSnapshot {
            fps: self.fps,
            frame_time_ms: if self.fps > 0.0 { 1000.0 / self.fps } else { 0.0 },
            renderable_count: self.renderables.as_ref().map(|r| r.len()),
            ..Default::default()
        })
    }
}
