//! 03_gltf_scene – Scene with Orbit Camera + Ground Plane
//!
//! Renders multiple objects with an orbit camera using deferred shading.
//! (Scene loading from GLTF/OBJ is not yet implemented – uses procedural geometry.)

use crate::{
    asset::mesh::{CpuMesh, GpuMesh},
    examples::{Example, ExampleStateSnapshot},
    input::InputManager,
    renderer::{
        camera::OrbitCamera,
        context::{GBuffer, RenderContext},
        passes::{
            gbuffer::{GBufferPass, MaterialUniform, Renderable},
            lighting::{LightingPass, LightingUniforms},
        },
    },
};
use glam::{Mat4, Vec3};
use std::sync::Arc;

/// Scene example with orbit camera and multiple objects.
pub struct GltfSceneExample {
    gbuffer_pass: Option<GBufferPass>,
    lighting_pass: Option<LightingPass>,
    gbuffer: Option<GBuffer>,
    renderables: Option<Vec<Renderable>>,

    orbit_cam: Option<OrbitCamera>,
    proj: Option<Mat4>,

    lighting_uniforms: Option<LightingUniforms>,

    last_frame_time: std::time::Instant,
    fps: f32,
}

impl GltfSceneExample {
    pub fn new() -> Self {
        Self {
            gbuffer_pass: None,
            lighting_pass: None,
            gbuffer: None,
            renderables: None,
            orbit_cam: None,
            proj: None,
            lighting_uniforms: None,
            last_frame_time: std::time::Instant::now(),
            fps: 0.0,
        }
    }

    pub fn with_default_model() -> Self {
        Self::new()
    }
}

impl Default for GltfSceneExample {
    fn default() -> Self {
        Self::new()
    }
}

impl Example for GltfSceneExample {
    fn name(&self) -> &'static str {
        "03_gltf_scene"
    }

    fn description(&self) -> &'static str {
        "Multi-object scene with orbit camera + deferred shading."
    }

    fn init(&mut self, ctx: &RenderContext) -> anyhow::Result<()> {
        let gbuffer_pass = GBufferPass::new(&ctx.device);
        let gbuffer = GBuffer::new(&ctx.device, ctx.config.width, ctx.config.height);
        let lighting_pass = LightingPass::new(
            &ctx.device,
            &gbuffer,
            &ctx.config,
        );

        // Meshes
        let cube_cpu = CpuMesh::cube();
        let sphere_cpu = CpuMesh::sphere(16);
        let quad_cpu = CpuMesh::quad();

        let cube_gpu = Arc::new(GpuMesh::from_cpu(&ctx.device, &cube_cpu));
        let sphere_gpu = Arc::new(GpuMesh::from_cpu(&ctx.device, &sphere_cpu));
        let quad_gpu = Arc::new(GpuMesh::from_cpu(&ctx.device, &quad_cpu));

        let mut renderables = Vec::new();

        // Ground plane (scaled quad)
        renderables.push(Renderable {
            mesh: quad_gpu.clone(),
            transform: Mat4::from_scale_rotation_translation(
                Vec3::new(5.0, 1.0, 5.0),
                glam::Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                Vec3::new(0.0, -1.0, 0.0),
            ),
            material: MaterialUniform {
                albedo: [0.3, 0.4, 0.3, 1.0],
                roughness: 0.8,
                metallic: 0.0,
                _pad: [0.0, 0.0],
            },
        });

        // Cube left
        renderables.push(Renderable {
            mesh: cube_gpu.clone(),
            transform: Mat4::from_translation(Vec3::new(-1.2, 0.0, 0.0)),
            material: MaterialUniform {
                albedo: [0.8, 0.3, 0.2, 1.0],
                roughness: 0.5,
                metallic: 0.0,
                _pad: [0.0, 0.0],
            },
        });

        // Sphere right
        renderables.push(Renderable {
            mesh: sphere_gpu.clone(),
            transform: Mat4::from_translation(Vec3::new(1.2, 0.0, 0.0)),
            material: MaterialUniform {
                albedo: [0.2, 0.5, 0.8, 1.0],
                roughness: 0.3,
                metallic: 0.3,
                _pad: [0.0, 0.0],
            },
        });

        // Tall cube in back
        renderables.push(Renderable {
            mesh: cube_gpu.clone(),
            transform: Mat4::from_scale_rotation_translation(
                Vec3::new(0.5, 1.5, 0.5),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.5, -1.5),
            ),
            material: MaterialUniform {
                albedo: [0.6, 0.6, 0.3, 1.0],
                roughness: 0.4,
                metallic: 0.0,
                _pad: [0.0, 0.0],
            },
        });

        // Camera
        let orbit_cam = OrbitCamera {
            distance: 5.0,
            azimuth: 0.75,
            polar: 0.8,
            target: Vec3::new(0.0, 0.0, 0.0),
            sensitivity: 0.005,
            zoom_sensitivity: 0.5,
        };

        self.proj = Some(Mat4::perspective_rh(
            45.0f32.to_radians(),
            ctx.config.width as f32 / ctx.config.height as f32,
            0.1,
            100.0,
        ));

        let lighting_uniforms = LightingUniforms {
            camera_pos: orbit_cam.position().into(),
            ..Default::default()
        };
        lighting_pass.update_uniforms(&ctx.queue, &lighting_uniforms);

        self.gbuffer_pass = Some(gbuffer_pass);
        self.lighting_pass = Some(lighting_pass);
        self.gbuffer = Some(gbuffer);
        self.renderables = Some(renderables);
        self.orbit_cam = Some(orbit_cam);
        self.lighting_uniforms = Some(lighting_uniforms);
        self.last_frame_time = std::time::Instant::now();

        Ok(())
    }

    fn cleanup(&mut self, _ctx: &RenderContext) {
        self.gbuffer_pass = None;
        self.lighting_pass = None;
        self.gbuffer = None;
        self.renderables = None;
        self.orbit_cam = None;
        self.proj = None;
        self.lighting_uniforms = None;
    }

    fn update(&mut self, _ctx: &RenderContext, _dt: f32, input: &InputManager) {
        let (dx, dy) = input.mouse_delta();
        let cam = self.orbit_cam.as_mut().unwrap();

        if input.key_held(winit::keyboard::KeyCode::KeyA) {
            cam.update(5.0, 0.0, 0.0); // rotate left
        }
        if input.key_held(winit::keyboard::KeyCode::KeyD) {
            cam.update(-5.0, 0.0, 0.0); // rotate right
        }
        if input.key_held(winit::keyboard::KeyCode::KeyW) {
            cam.update(0.0, -5.0, 0.0); // rotate up
        }
        if input.key_held(winit::keyboard::KeyCode::KeyS) {
            cam.update(0.0, 5.0, 0.0); // rotate down
        }
        if input.mouse_pressed(winit::event::MouseButton::Left) && (dx != 0.0 || dy != 0.0) {
            cam.update(dx * 50.0, -dy * 50.0, 0.0);
        }
    }

    fn prepare(&mut self, ctx: &RenderContext) {
        let cam = self.orbit_cam.as_ref().unwrap();
        let uniforms = self.lighting_uniforms.as_mut().unwrap();
        uniforms.camera_pos = cam.position().into();

        self.lighting_pass
            .as_ref()
            .unwrap()
            .update_uniforms(&ctx.queue, uniforms);
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
        let cam = self.orbit_cam.as_ref().unwrap();
        let view = Mat4::look_at_rh(cam.position(), cam.target, cam.up());

        // 1. G-Buffer pass
        self.gbuffer_pass
            .as_ref()
            .unwrap()
            .execute(encoder, gbuffer, ctx, renderables, &view, proj);

        // 2. Lighting pass → swapchain
        self.lighting_pass
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

        egui::Window::new("Scene Debug")
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
            .show(egui_ctx, |ui| {
                ui.label(format!("FPS: {:.1}", self.fps));
                ui.label(format!("Frame: {:.2} ms", frame_time_ms));
                if let Some(ref cam) = self.orbit_cam {
                    let pos = cam.position();
                    ui.label(format!(
                        "Camera: ({:.1}, {:.1}, {:.1})",
                        pos.x, pos.y, pos.z
                    ));
                }
                ui.label("WASD / Mouse-drag: orbit camera");
            });
    }

    fn resize(&mut self, ctx: &RenderContext, width: u32, height: u32) {
        let gbuffer = GBuffer::new(&ctx.device, width, height);
        self.lighting_pass
            .as_mut()
            .unwrap()
            .recreate_bind_group(&ctx.device, &gbuffer);
        self.gbuffer = Some(gbuffer);
        self.proj = Some(Mat4::perspective_rh(
            45.0f32.to_radians(),
            width as f32 / height as f32,
            0.1,
            100.0,
        ));
    }

    fn snapshot(&self) -> Option<ExampleStateSnapshot> {
        let cam_pos = self
            .orbit_cam
            .as_ref()
            .map(|c| {
                let p = c.position();
                [p.x, p.y, p.z]
            });

        Some(ExampleStateSnapshot {
            fps: self.fps,
            frame_time_ms: if self.fps > 0.0 { 1000.0 / self.fps } else { 0.0 },
            renderable_count: self.renderables.as_ref().map(|r| r.len()),
            camera_position: cam_pos,
            ..Default::default()
        })
    }
}
