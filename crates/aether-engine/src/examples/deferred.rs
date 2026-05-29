//! 02_deferred – Deferred Shading with Lighting + Debug Overlay
//!
//! Renders a cube and sphere with Blinn-Phong lighting.

use crate::{
    asset::{
        gpu_material::GpuMaterial,
        mesh::{CpuMesh, GpuMesh},
    },
    examples::{Example, ExampleStateSnapshot},
    input::InputManager,
    renderer::{
        context::{GBuffer, RenderContext},
        passes::{
            gbuffer::{GBufferPass, MaterialUniform, Renderable},
            lighting::{LightingPass, LightsUniform},
            shadow::ShadowPass,
            tonemap::TonemapPass,
        },
    },
};
use glam::{Mat4, Vec3};
use std::sync::Arc;

/// Deferred shading example: cube + sphere with Blinn-Phong lighting.
pub struct DeferredExample {
    // GPU resources – wrapped in Option so `cleanup` can drop them.
    gbuffer_pass: Option<GBufferPass>,
    shadow_pass: Option<ShadowPass>,
    lighting_pass: Option<LightingPass>,
    tonemap_pass: Option<TonemapPass>,
    gbuffer: Option<GBuffer>,
    hdr_texture: Option<wgpu::Texture>,
    renderables: Option<Vec<Renderable>>,

    // Camera & projection
    view: Mat4,
    proj: Option<Mat4>,

    // Lighting
    lights_uniform: Option<LightsUniform>,

    // Timing
    last_frame_time: std::time::Instant,
    fps: f32,
}

impl DeferredExample {
    /// Create a new deferred shading example.
    pub fn new() -> Self {
        Self {
            gbuffer_pass: None,
            shadow_pass: None,
            lighting_pass: None,
            tonemap_pass: None,
            gbuffer: None,
            hdr_texture: None,
            renderables: None,
            view: Mat4::IDENTITY,
            proj: None,
            lights_uniform: None,
            last_frame_time: std::time::Instant::now(),
            fps: 0.0,
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
        "Deferred shading: cube + sphere with Blinn-Phong lighting and tonemap."
    }

    fn init(&mut self, ctx: &RenderContext) -> anyhow::Result<()> {
        let gbuffer_pass = GBufferPass::new(&ctx.device, &ctx.queue);
        let shadow_pass = ShadowPass::new(&ctx.device, 2048);

        let gbuffer = GBuffer::new(&ctx.device, ctx.config.width, ctx.config.height);
        let lighting_pass = LightingPass::new(
            &ctx.device,
            &gbuffer,
            shadow_pass.shadow_map_view(),
            &shadow_pass.shadow_sampler,
        );

        let hdr_texture = create_hdr_texture(&ctx.device, ctx.config.width, ctx.config.height);
        let tonemap_pass = TonemapPass::new(
            &ctx.device,
            &hdr_texture.create_view(&wgpu::TextureViewDescriptor::default()),
            ctx.config.format,
        );

        // Meshes
        let cube_cpu = CpuMesh::cube();
        let sphere_cpu = CpuMesh::sphere(16);
        let cube_gpu = GpuMesh::from_cpu(&ctx.device, &cube_cpu);
        let sphere_gpu = GpuMesh::from_cpu(&ctx.device, &sphere_cpu);

        let default_mat = Arc::new(GpuMaterial::new_default(
            &ctx.device,
            &ctx.queue,
            gbuffer_pass.material_texture_bind_group_layout(),
        ));

        let cube_renderable = Renderable {
            mesh: Arc::new(cube_gpu),
            transform: Mat4::from_translation(Vec3::new(-0.8, 0.0, 0.0)),
            material: MaterialUniform {
                albedo: [0.8, 0.3, 0.2, 1.0],
                roughness: 0.5,
                metallic: 0.0,
                ao: 1.0,
                emissive_intensity: 0.0,
            },
            material_bind_group: Some(default_mat.bind_group.clone()),
        };

        let sphere_renderable = Renderable {
            mesh: Arc::new(sphere_gpu),
            transform: Mat4::from_translation(Vec3::new(0.8, 0.0, 0.0)),
            material: MaterialUniform {
                albedo: [0.2, 0.5, 0.8, 1.0],
                roughness: 0.3,
                metallic: 0.1,
                ao: 1.0,
                emissive_intensity: 0.0,
            },
            material_bind_group: Some(default_mat.bind_group.clone()),
        };

        let renderables = vec![cube_renderable, sphere_renderable];

        // Camera
        let eye = Vec3::new(3.0, 3.0, 3.0);
        let target = Vec3::ZERO;
        let up = Vec3::Y;
        self.view = Mat4::look_at_rh(eye, target, up);
        self.proj = Some(Mat4::perspective_rh(
            45.0f32.to_radians(),
            ctx.config.width as f32 / ctx.config.height as f32,
            0.1,
            100.0,
        ));

        // Lighting
        let lights_uniform = LightsUniform {
            camera_pos: eye.into(),
            ..Default::default()
        };
        lighting_pass.update_lights(&ctx.queue, &lights_uniform);

        self.gbuffer_pass = Some(gbuffer_pass);
        self.shadow_pass = Some(shadow_pass);
        self.lighting_pass = Some(lighting_pass);
        self.tonemap_pass = Some(tonemap_pass);
        self.gbuffer = Some(gbuffer);
        self.hdr_texture = Some(hdr_texture);
        self.renderables = Some(renderables);
        self.lights_uniform = Some(lights_uniform);
        self.last_frame_time = std::time::Instant::now();
        self.fps = 0.0;

        Ok(())
    }

    fn cleanup(&mut self, _ctx: &RenderContext) {
        self.gbuffer_pass = None;
        self.shadow_pass = None;
        self.lighting_pass = None;
        self.tonemap_pass = None;
        self.gbuffer = None;
        self.hdr_texture = None;
        self.renderables = None;
        self.proj = None;
        self.lights_uniform = None;
    }

    fn update(&mut self, _ctx: &RenderContext, _dt: f32, _input: &InputManager) {
        // Static scene – no per-frame logic.
    }

    fn prepare(&mut self, _ctx: &RenderContext) {
        // No GPU uploads needed per frame.
    }

    fn render(
        &mut self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
    ) -> anyhow::Result<()> {
        let gbuffer = self.gbuffer.as_ref().unwrap();
        let hdr_texture = self.hdr_texture.as_ref().unwrap();
        let renderables = self.renderables.as_ref().unwrap();
        let proj = self.proj.as_ref().unwrap();

        let hdr_view = hdr_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // 1. G-Buffer pass
        self.gbuffer_pass
            .as_ref()
            .unwrap()
            .execute(encoder, gbuffer, ctx, renderables, &self.view, proj);

        // 2. Lighting pass → HDR
        self.lighting_pass
            .as_ref()
            .unwrap()
            .execute(encoder, &hdr_view);

        // 3. Tonemap → swapchain
        self.tonemap_pass
            .as_ref()
            .unwrap()
            .execute(encoder, target_view);

        Ok(())
    }

    fn ui(&mut self, egui_ctx: &egui::Context) {
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
                ui.label(format!("FPS: {:.1}", self.fps));
                ui.label(format!("Frame: {:.2} ms", frame_time_ms));
                ui.label(format!(
                    "Resolution: {}x{}",
                    egui_ctx.screen_rect().width() as u32,
                    egui_ctx.screen_rect().height() as u32
                ));
            });
    }

    fn resize(&mut self, ctx: &RenderContext, width: u32, height: u32) {
        let gbuffer = GBuffer::new(&ctx.device, width, height);
        let hdr_texture = create_hdr_texture(&ctx.device, width, height);

        self.lighting_pass
            .as_mut()
            .unwrap()
            .recreate_bind_group(&ctx.device, &gbuffer);
        self.tonemap_pass.as_mut().unwrap().recreate_bind_group(
            &ctx.device,
            &hdr_texture.create_view(&wgpu::TextureViewDescriptor::default()),
        );

        self.gbuffer = Some(gbuffer);
        self.hdr_texture = Some(hdr_texture);
        self.proj = Some(Mat4::perspective_rh(
            45.0f32.to_radians(),
            width as f32 / height as f32,
            0.1,
            100.0,
        ));
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

fn create_hdr_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("HDR Texture"),
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
    })
}
